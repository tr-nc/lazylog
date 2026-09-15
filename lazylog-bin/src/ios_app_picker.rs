use crossterm::event::{
    self, Event, KeyCode, KeyEventKind, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::{
    Terminal,
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::Line,
    widgets::{List, ListItem, ListState, Paragraph},
};
use std::io;
use std::time::Duration;

const APPS: [IosApp; 2] = [IosApp::EffectCam, IosApp::Douyin];

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

struct PickerState {
    selected: Option<usize>,
}

impl PickerState {
    fn new() -> Self {
        Self { selected: Some(0) }
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
                    .map(|selected| PickerAction::Select(APPS[selected]))
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
                .map(PickerAction::Select)
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

fn picker_layout(area: Rect) -> [Rect; 3] {
    Layout::vertical([
        Constraint::Length(3),
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

fn draw_picker(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    state: &PickerState,
) -> io::Result<Rect> {
    let mut list_area = Rect::default();
    terminal.draw(|frame| {
        let [header_area, current_list_area, footer_area] = picker_layout(frame.area());
        list_area = current_list_area;

        let header = Paragraph::new(vec![
            Line::styled(
                "选择要加载的 iOS App",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Line::from("Lazylog 不会在你确认前启动任何 App"),
        ])
        .alignment(Alignment::Center);
        frame.render_widget(header, header_area);

        let items =
            APPS.map(|app| ListItem::new(format!("{}  ({})", app.display_name(), app.subtitle())));
        let list = List::new(items).highlight_symbol("▶ ").highlight_style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        );
        let mut list_state = ListState::default();
        list_state.select(state.selected);
        frame.render_stateful_widget(list, list_area, &mut list_state);

        let footer = Paragraph::new("↑/↓ 或 j/k 选择 · Enter 确认 · 鼠标点击选择 · Esc/q 取消")
            .alignment(Alignment::Center)
            .style(Style::default().fg(Color::DarkGray));
        frame.render_widget(footer, footer_area);
    })?;
    Ok(list_area)
}

pub(crate) fn pick(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
) -> io::Result<Option<IosApp>> {
    terminal.clear()?;
    let mut state = PickerState::new();
    let mut list_area = draw_picker(terminal, &state)?;

    loop {
        if !event::poll(Duration::from_millis(16))? {
            continue;
        }

        match state.handle_event(event::read()?, list_area) {
            PickerAction::Continue => list_area = draw_picker(terminal, &state)?,
            PickerAction::Select(app) => return Ok(Some(app)),
            PickerAction::Cancel => return Ok(None),
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
}
