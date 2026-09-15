use crossterm::event::{
    self, Event, KeyCode, KeyEventKind, MouseButton, MouseEvent, MouseEventKind,
};
use lazylog_ios::{IosAppState, app_states, connected_devices};
use ratatui::{
    Terminal,
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::Line,
    widgets::{List, ListItem, ListState, Paragraph},
};
use std::io;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc,
};
use std::thread;
use std::time::Duration;
use unicode_width::UnicodeWidthStr;

const APPS: [IosApp; 2] = [IosApp::EffectCam, IosApp::Douyin];
const STATUS_REFRESH_INTERVAL: Duration = Duration::from_secs(1);
const EVENT_POLL_INTERVAL: Duration = Duration::from_millis(50);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum IosApp {
    EffectCam,
    Douyin,
}

impl IosApp {
    pub(crate) const EFFECTCAM_BUNDLE_ID: &'static str = "com.ss.ios.ugc.EffectCamInhouse";
    pub(crate) const DOUYIN_BUNDLE_ID: &'static str = "com.ss.iphone.ugc.AwemeInhouse";

    pub(crate) fn parse(value: &str) -> Result<Self, io::Error> {
        match value.to_ascii_lowercase().as_str() {
            "effectcam" | "xiangsu" => Ok(Self::EffectCam),
            "douyin" | "aweme" => Ok(Self::Douyin),
            _ if value == "像塑" => Ok(Self::EffectCam),
            _ if value == "抖音" => Ok(Self::Douyin),
            _ => Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("Unknown iOS app '{value}'; expected effectcam or douyin"),
            )),
        }
    }

    pub(crate) fn bundle_id(self) -> &'static str {
        match self {
            Self::EffectCam => Self::EFFECTCAM_BUNDLE_ID,
            Self::Douyin => Self::DOUYIN_BUNDLE_ID,
        }
    }

    pub(crate) fn display_name(self) -> &'static str {
        match self {
            Self::EffectCam => "像塑",
            Self::Douyin => "抖音开发版",
        }
    }

    fn subtitle(self) -> &'static str {
        match self {
            Self::EffectCam => "EffectCam",
            Self::Douyin => "Douyin",
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
enum PickerAction {
    Continue,
    Select(IosApp),
    Cancel,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DisplayAppState {
    Checking,
    Running,
    NotRunning,
    NotInstalled,
    Unknown,
}

impl DisplayAppState {
    fn label(self) -> &'static str {
        match self {
            Self::Checking => "… 检测中",
            Self::Running => "● 进程仍存在",
            Self::NotRunning => "○ 无现有进程",
            Self::NotInstalled => "– 未安装",
            Self::Unknown => "? 状态未知",
        }
    }
}

impl From<IosAppState> for DisplayAppState {
    fn from(value: IosAppState) -> Self {
        match value {
            IosAppState::ProcessPresent => Self::Running,
            IosAppState::NoProcess => Self::NotRunning,
            IosAppState::NotInstalled => Self::NotInstalled,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PickerOutcome {
    Selected(IosApp),
    DeviceDisconnected,
    Cancelled,
}

enum StatusUpdate {
    App(IosApp, DisplayAppState),
    DeviceDisconnected,
}

struct PickerState {
    selected: Option<usize>,
    app_states: [DisplayAppState; APPS.len()],
}

impl PickerState {
    fn new() -> Self {
        Self {
            selected: Some(0),
            app_states: [DisplayAppState::Checking; APPS.len()],
        }
    }

    fn update_app_state(&mut self, app: IosApp, state: DisplayAppState) {
        if let Some(index) = APPS.iter().position(|candidate| *candidate == app) {
            self.app_states[index] = state;
        }
    }

    fn app_is_selectable(&self, app: IosApp) -> bool {
        let state = APPS
            .iter()
            .position(|candidate| *candidate == app)
            .map(|index| self.app_states[index]);
        !matches!(
            state,
            Some(DisplayAppState::Checking | DisplayAppState::NotInstalled)
        )
    }

    fn select(&self, app: IosApp) -> PickerAction {
        if self.app_is_selectable(app) {
            PickerAction::Select(app)
        } else {
            PickerAction::Continue
        }
    }

    fn handle_event(&mut self, event: Event, list_area: Rect) -> PickerAction {
        match event {
            Event::Key(key) if key.kind == KeyEventKind::Press => match key.code {
                KeyCode::Up | KeyCode::Char('k') => {
                    self.selected = Some(match self.selected {
                        Some(selected) => selected.saturating_sub(1),
                        None => APPS.len() - 1,
                    });
                    PickerAction::Continue
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    self.selected = Some(match self.selected {
                        Some(selected) => (selected + 1).min(APPS.len() - 1),
                        None => 0,
                    });
                    PickerAction::Continue
                }
                KeyCode::Enter => self
                    .selected
                    .map(|selected| self.select(APPS[selected]))
                    .unwrap_or(PickerAction::Continue),
                KeyCode::Esc | KeyCode::Char('q') => PickerAction::Cancel,
                _ => PickerAction::Continue,
            },
            Event::Mouse(MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column,
                row,
                ..
            }) => app_at_position(list_area, column, row)
                .map(|app| self.select(app))
                .unwrap_or(PickerAction::Continue),
            Event::Mouse(MouseEvent {
                kind: MouseEventKind::ScrollUp,
                ..
            }) => {
                self.selected = Some(match self.selected {
                    Some(selected) => selected.saturating_sub(1),
                    None => APPS.len() - 1,
                });
                PickerAction::Continue
            }
            Event::Mouse(MouseEvent {
                kind: MouseEventKind::ScrollDown,
                ..
            }) => {
                self.selected = Some(match self.selected {
                    Some(selected) => (selected + 1).min(APPS.len() - 1),
                    None => 0,
                });
                PickerAction::Continue
            }
            _ => PickerAction::Continue,
        }
    }
}

struct StatusWorkerGuard {
    should_stop: Arc<AtomicBool>,
}

impl Drop for StatusWorkerGuard {
    fn drop(&mut self) {
        self.should_stop.store(true, Ordering::Relaxed);
    }
}

fn spawn_status_worker(device: String) -> (mpsc::Receiver<StatusUpdate>, StatusWorkerGuard) {
    let (sender, receiver) = mpsc::channel();
    let should_stop = Arc::new(AtomicBool::new(false));
    let worker_stop = should_stop.clone();

    thread::spawn(move || {
        while !worker_stop.load(Ordering::Relaxed) {
            if connected_devices().is_ok_and(|devices| {
                !devices
                    .iter()
                    .any(|candidate| candidate.identifier == device)
            }) {
                let _ = sender.send(StatusUpdate::DeviceDisconnected);
                return;
            }

            let bundle_ids = APPS.map(IosApp::bundle_id);
            match app_states(&device, &bundle_ids) {
                Ok(states) => {
                    for (app, state) in APPS.into_iter().zip(states) {
                        if sender
                            .send(StatusUpdate::App(app, DisplayAppState::from(state)))
                            .is_err()
                        {
                            return;
                        }
                    }
                }
                Err(_) => {
                    for app in APPS {
                        if sender
                            .send(StatusUpdate::App(app, DisplayAppState::Unknown))
                            .is_err()
                        {
                            return;
                        }
                    }
                }
            }

            let mut elapsed = Duration::ZERO;
            while elapsed < STATUS_REFRESH_INTERVAL && !worker_stop.load(Ordering::Relaxed) {
                let sleep = Duration::from_millis(50).min(STATUS_REFRESH_INTERVAL - elapsed);
                thread::sleep(sleep);
                elapsed += sleep;
            }
        }
    });

    (receiver, StatusWorkerGuard { should_stop })
}

fn picker_layout(area: Rect) -> [Rect; 3] {
    Layout::vertical([
        Constraint::Length(2),
        Constraint::Length(APPS.len() as u16),
        Constraint::Fill(1),
    ])
    .margin(2)
    .areas(area)
}

fn app_at_position(list_area: Rect, column: u16, row: u16) -> Option<IosApp> {
    if column < list_area.x || column >= list_area.x.saturating_add(list_area.width) {
        return None;
    }

    let index = row.checked_sub(list_area.y)? as usize;
    APPS.get(index).copied()
}

fn app_row(app: IosApp, state: DisplayAppState) -> String {
    let identity = format!("{}  ({})", app.display_name(), app.subtitle());
    let identity_column_width = APPS
        .iter()
        .map(|app| format!("{}  ({})", app.display_name(), app.subtitle()).width())
        .max()
        .unwrap_or_default();
    let padding = identity_column_width.saturating_sub(identity.width()) + 2;
    format!("{identity}{}· {}", " ".repeat(padding), state.label())
}

const CONTENT_INDENT: &str = "  ";

fn selected_notice(app: IosApp, state: DisplayAppState) -> (String, Style) {
    match state {
        DisplayAppState::Running => (
            format!(
                "{CONTENT_INDENT}⚠ 检测到 {} 的现有进程（可能在后台或挂起）；确认后会终止并重新启动",
                app.display_name()
            ),
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
        DisplayAppState::NotInstalled => (
            format!("{CONTENT_INDENT}{} 未安装在当前设备上", app.display_name()),
            Style::default().fg(Color::Red),
        ),
        DisplayAppState::Checking => (
            format!(
                "{CONTENT_INDENT}正在检测 {} 的进程状态…",
                app.display_name()
            ),
            Style::default().fg(Color::DarkGray),
        ),
        DisplayAppState::Unknown => (
            format!(
                "{CONTENT_INDENT}无法确认 {} 是否存在进程；确认后仍会尝试终止并重新启动",
                app.display_name()
            ),
            Style::default().fg(Color::Yellow),
        ),
        DisplayAppState::NotRunning => (
            format!(
                "{CONTENT_INDENT}{} 无现有进程；确认后将启动并接入日志",
                app.display_name()
            ),
            Style::default().fg(Color::DarkGray),
        ),
    }
}

fn draw_picker(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    state: &PickerState,
) -> io::Result<Rect> {
    let mut list_area = Rect::default();
    terminal.draw(|frame| {
        let [header_area, current_list_area, footer_area] = picker_layout(frame.area());
        list_area = current_list_area;

        let header = Paragraph::new(Line::styled(
            "选择要加载的 iOS App",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ))
        .alignment(Alignment::Center);
        frame.render_widget(header, header_area);

        let items = APPS
            .iter()
            .enumerate()
            .map(|(index, app)| ListItem::new(app_row(*app, state.app_states[index])));
        let list = List::new(items).highlight_symbol("▶ ").highlight_style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        );
        let mut list_state = ListState::default();
        list_state.select(state.selected);
        frame.render_stateful_widget(list, list_area, &mut list_state);

        let selected = state.selected.unwrap_or(0);
        let selected_app = APPS[selected];
        let selected_state = state.app_states[selected];
        let (notice_text, notice_style) = selected_notice(selected_app, selected_state);
        let notice = Line::styled(notice_text, notice_style);
        let footer = Paragraph::new(vec![
            notice,
            Line::styled(
                format!("{CONTENT_INDENT}↑/↓ 或 j/k 选择 · Enter 确认 · 鼠标点击选择 · Esc/q 退出"),
                Style::default().fg(Color::DarkGray),
            ),
        ])
        .alignment(Alignment::Left);
        frame.render_widget(footer, footer_area);
    })?;
    Ok(list_area)
}

pub(crate) fn pick(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    device: &str,
) -> io::Result<PickerOutcome> {
    terminal.clear()?;
    let (status_updates, _worker_guard) = spawn_status_worker(device.to_string());
    let mut state = PickerState::new();
    let mut list_area = draw_picker(terminal, &state)?;

    loop {
        let mut changed = false;
        while let Ok(update) = status_updates.try_recv() {
            match update {
                StatusUpdate::App(app, app_state) => {
                    state.update_app_state(app, app_state);
                    changed = true;
                }
                StatusUpdate::DeviceDisconnected => {
                    return Ok(PickerOutcome::DeviceDisconnected);
                }
            }
        }
        if changed {
            list_area = draw_picker(terminal, &state)?;
        }

        if !event::poll(EVENT_POLL_INTERVAL)? {
            continue;
        }

        match state.handle_event(event::read()?, list_area) {
            PickerAction::Continue => list_area = draw_picker(terminal, &state)?,
            PickerAction::Select(app) => return Ok(PickerOutcome::Selected(app)),
            PickerAction::Cancel => return Ok(PickerOutcome::Cancelled),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyEvent, KeyModifiers};

    #[test]
    fn keyboard_navigation_requires_confirmation() {
        let mut state = PickerState::new();
        let area = Rect::new(10, 5, 30, 2);
        state.update_app_state(IosApp::EffectCam, DisplayAppState::NotRunning);
        state.update_app_state(IosApp::Douyin, DisplayAppState::NotRunning);

        assert_eq!(
            state.handle_event(
                Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
                area,
            ),
            PickerAction::Select(IosApp::EffectCam)
        );
        assert_eq!(state.selected, Some(0));

        assert_eq!(
            state.handle_event(
                Event::Key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE)),
                area,
            ),
            PickerAction::Continue
        );
        assert_eq!(state.selected, Some(1));
        assert_eq!(
            state.handle_event(
                Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
                area,
            ),
            PickerAction::Select(IosApp::Douyin)
        );
    }

    #[test]
    fn mouse_click_selects_the_clicked_app() {
        let mut state = PickerState::new();
        let area = Rect::new(10, 5, 30, 2);
        state.update_app_state(IosApp::Douyin, DisplayAppState::NotRunning);
        let click = MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 12,
            row: 6,
            modifiers: KeyModifiers::NONE,
        };

        assert_eq!(
            state.handle_event(Event::Mouse(click), area),
            PickerAction::Select(IosApp::Douyin)
        );
    }

    #[test]
    fn runtime_status_is_recorded_for_the_matching_app() {
        let mut state = PickerState::new();

        state.update_app_state(IosApp::Douyin, DisplayAppState::Running);

        assert_eq!(state.app_states[0], DisplayAppState::Checking);
        assert_eq!(state.app_states[1], DisplayAppState::Running);
    }

    #[test]
    fn app_statuses_start_in_the_same_terminal_column() {
        let effectcam = app_row(IosApp::EffectCam, DisplayAppState::Running);
        let douyin = app_row(IosApp::Douyin, DisplayAppState::Running);
        let effectcam_separator = effectcam.find('·').unwrap();
        let douyin_separator = douyin.find('·').unwrap();

        assert_eq!(
            effectcam[..effectcam_separator].width(),
            douyin[..douyin_separator].width()
        );
    }

    #[test]
    fn contextual_notice_uses_the_same_indent_as_list_content() {
        let (notice, _) = selected_notice(IosApp::EffectCam, DisplayAppState::Running);

        assert!(notice.starts_with(CONTENT_INDENT));
        assert_eq!(CONTENT_INDENT.width(), 2);
    }

    #[test]
    fn checking_and_uninstalled_apps_cannot_be_confirmed() {
        let mut state = PickerState::new();
        let area = Rect::new(10, 5, 30, 2);
        let enter = Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

        assert_eq!(
            state.handle_event(enter.clone(), area),
            PickerAction::Continue
        );

        state.update_app_state(IosApp::EffectCam, DisplayAppState::NotInstalled);
        assert_eq!(state.handle_event(enter, area), PickerAction::Continue);
    }
}
