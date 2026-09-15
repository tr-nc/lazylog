use crossterm::event::{
    self, Event, KeyCode, KeyEventKind, MouseButton, MouseEvent, MouseEventKind,
};
use lazylog_android::connected_devices as connected_android_devices;
use lazylog_ios::{
    IosAppAvailability, app_availabilities, connected_devices as connected_ios_devices,
};
use ratatui::{
    Terminal,
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::Line,
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
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
const REFRESH_INTERVAL: Duration = Duration::from_secs(1);
const EVENT_POLL_INTERVAL: Duration = Duration::from_millis(50);
const CONTENT_INDENT: &str = "  ";

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

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct IosSelection {
    pub(crate) device: String,
    pub(crate) app: IosApp,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct DeviceOption {
    id: String,
    name: String,
    detail: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PickerPlatform {
    Ios,
    Android,
}

impl PickerPlatform {
    fn display_name(self) -> &'static str {
        match self {
            Self::Ios => "iOS",
            Self::Android => "Android",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TabKind {
    Device,
    App,
}

impl TabKind {
    fn title(self) -> &'static str {
        match self {
            Self::Device => "设备",
            Self::App => "APP",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DisplayAvailability {
    Installed,
    NotInstalled,
    DetectionFailed,
}

impl DisplayAvailability {
    fn label(self) -> &'static str {
        match self {
            Self::Installed => "✓ 已安装",
            Self::NotInstalled => "– 未安装",
            Self::DetectionFailed => "? 检测失败",
        }
    }
}

impl From<IosAppAvailability> for DisplayAvailability {
    fn from(value: IosAppAvailability) -> Self {
        match value {
            IosAppAvailability::Installed => Self::Installed,
            IosAppAvailability::NotInstalled => Self::NotInstalled,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct VisibleList {
    area: Rect,
    offset: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct VisibleAreas {
    tabs: Vec<Rect>,
    list: VisibleList,
}

#[derive(Debug, PartialEq, Eq)]
enum PickerAction {
    Continue,
    CompleteDevice(String),
    CompleteIos(IosSelection),
    Cancel,
}

struct PickerState {
    platform: PickerPlatform,
    tabs: Vec<TabKind>,
    active_tab: usize,
    devices: Vec<DeviceOption>,
    highlighted_device: Option<usize>,
    selected_device: Option<String>,
    highlighted_app: usize,
    selected_app: Option<IosApp>,
    requested_app: Option<IosApp>,
    app_availability: Option<[DisplayAvailability; APPS.len()]>,
    discovery_error: Option<String>,
    has_discovered: bool,
}

impl PickerState {
    fn ios(retained_device: Option<String>, requested_app: Option<IosApp>) -> Self {
        let active_tab = usize::from(retained_device.is_some());
        let highlighted_app = requested_app
            .and_then(|requested| APPS.iter().position(|app| *app == requested))
            .unwrap_or(0);
        Self {
            platform: PickerPlatform::Ios,
            tabs: vec![TabKind::Device, TabKind::App],
            active_tab,
            devices: Vec::new(),
            highlighted_device: None,
            selected_device: retained_device,
            highlighted_app,
            selected_app: requested_app,
            requested_app,
            app_availability: None,
            discovery_error: None,
            has_discovered: false,
        }
    }

    fn android() -> Self {
        Self {
            platform: PickerPlatform::Android,
            tabs: vec![TabKind::Device],
            active_tab: 0,
            devices: Vec::new(),
            highlighted_device: None,
            selected_device: None,
            highlighted_app: 0,
            selected_app: None,
            requested_app: None,
            app_availability: None,
            discovery_error: None,
            has_discovered: false,
        }
    }

    fn active_tab(&self) -> TabKind {
        self.tabs[self.active_tab]
    }

    fn update_devices(&mut self, result: io::Result<Vec<DeviceOption>>) -> bool {
        self.has_discovered = true;
        match result {
            Ok(devices) => {
                let highlighted_id = self
                    .highlighted_device
                    .and_then(|index| self.devices.get(index))
                    .map(|device| device.id.as_str());
                self.highlighted_device = highlighted_id
                    .and_then(|id| devices.iter().position(|device| device.id == id))
                    .or_else(|| (!devices.is_empty()).then_some(0));

                let selected_was_lost = self
                    .selected_device
                    .as_ref()
                    .is_some_and(|id| !devices.iter().any(|device| &device.id == id));
                self.devices = devices;
                self.discovery_error = None;
                if selected_was_lost {
                    self.selected_device = None;
                    self.invalidate_after_device();
                    self.active_tab = 0;
                }
                selected_was_lost
            }
            Err(error) => {
                self.discovery_error = Some(error.to_string());
                false
            }
        }
    }

    fn invalidate_after_device(&mut self) {
        self.selected_app = None;
        self.app_availability = None;
    }

    fn commit_device(&mut self, id: String) -> PickerAction {
        let device_changed = self.selected_device.as_deref() != Some(id.as_str());
        if device_changed {
            self.selected_device = Some(id.clone());
            self.invalidate_after_device();
        }

        match self.platform {
            PickerPlatform::Android => PickerAction::CompleteDevice(id),
            PickerPlatform::Ios => {
                if let Some(app) = self.requested_app {
                    self.selected_app = Some(app);
                    PickerAction::CompleteIos(IosSelection { device: id, app })
                } else {
                    self.active_tab = self
                        .tabs
                        .iter()
                        .position(|tab| *tab == TabKind::App)
                        .expect("iOS picker must include an APP tab");
                    PickerAction::Continue
                }
            }
        }
    }

    fn commit_highlighted_device(&mut self) -> PickerAction {
        self.highlighted_device
            .and_then(|index| self.devices.get(index))
            .map(|device| device.id.clone())
            .map(|id| self.commit_device(id))
            .unwrap_or(PickerAction::Continue)
    }

    fn update_availability(
        &mut self,
        device: &str,
        availability: [DisplayAvailability; APPS.len()],
    ) {
        if self.selected_device.as_deref() == Some(device) {
            self.app_availability = Some(availability);
        }
    }

    fn commit_app(&mut self, app: IosApp) -> PickerAction {
        let Some(device) = self.selected_device.clone() else {
            return PickerAction::Continue;
        };
        let Some(index) = APPS.iter().position(|candidate| *candidate == app) else {
            return PickerAction::Continue;
        };
        if self
            .app_availability
            .is_none_or(|availability| availability[index] != DisplayAvailability::Installed)
        {
            return PickerAction::Continue;
        }
        self.selected_app = Some(app);
        PickerAction::CompleteIos(IosSelection { device, app })
    }

    fn move_tab(&mut self, delta: isize) {
        self.active_tab = move_index(self.active_tab, self.tabs.len(), delta);
    }

    fn move_highlight(&mut self, delta: isize) {
        match self.active_tab() {
            TabKind::Device => {
                if self.devices.is_empty() {
                    self.highlighted_device = None;
                } else {
                    self.highlighted_device = Some(move_index(
                        self.highlighted_device.unwrap_or(0),
                        self.devices.len(),
                        delta,
                    ));
                }
            }
            TabKind::App => {
                self.highlighted_app = move_index(self.highlighted_app, APPS.len(), delta);
            }
        }
    }

    fn handle_event(&mut self, event: Event, visible: &VisibleAreas) -> PickerAction {
        match event {
            Event::Key(key) if key.kind == KeyEventKind::Press => match key.code {
                KeyCode::Left | KeyCode::Char('h') => {
                    self.move_tab(-1);
                    PickerAction::Continue
                }
                KeyCode::Right | KeyCode::Char('l') => {
                    self.move_tab(1);
                    PickerAction::Continue
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    self.move_highlight(-1);
                    PickerAction::Continue
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    self.move_highlight(1);
                    PickerAction::Continue
                }
                KeyCode::Enter => match self.active_tab() {
                    TabKind::Device => self.commit_highlighted_device(),
                    TabKind::App => self.commit_app(APPS[self.highlighted_app]),
                },
                KeyCode::Esc => {
                    if self.active_tab > 0 {
                        self.active_tab -= 1;
                    }
                    PickerAction::Continue
                }
                KeyCode::Char('q') => PickerAction::Cancel,
                _ => PickerAction::Continue,
            },
            Event::Mouse(MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column,
                row,
                ..
            }) => {
                if let Some(tab) = tab_at_position(&visible.tabs, column, row) {
                    self.active_tab = tab;
                    return PickerAction::Continue;
                }
                match self.active_tab() {
                    TabKind::Device => self
                        .device_at_position(visible.list, column, row)
                        .map(|device| device.id.clone())
                        .map(|id| self.commit_device(id))
                        .unwrap_or(PickerAction::Continue),
                    TabKind::App => app_at_position(visible.list.area, column, row)
                        .map(|app| self.commit_app(app))
                        .unwrap_or(PickerAction::Continue),
                }
            }
            Event::Mouse(MouseEvent {
                kind: MouseEventKind::ScrollUp,
                ..
            }) => {
                self.move_highlight(-1);
                PickerAction::Continue
            }
            Event::Mouse(MouseEvent {
                kind: MouseEventKind::ScrollDown,
                ..
            }) => {
                self.move_highlight(1);
                PickerAction::Continue
            }
            _ => PickerAction::Continue,
        }
    }

    fn device_at_position(
        &self,
        visible: VisibleList,
        column: u16,
        row: u16,
    ) -> Option<&DeviceOption> {
        if !contains(visible.area, column, row) {
            return None;
        }
        let visible_index = row.checked_sub(visible.area.y)? as usize;
        self.devices.get(visible.offset + visible_index)
    }
}

fn move_index(current: usize, count: usize, delta: isize) -> usize {
    if count == 0 {
        return 0;
    }
    if delta < 0 {
        current.saturating_sub(delta.unsigned_abs())
    } else {
        current.saturating_add(delta as usize).min(count - 1)
    }
}

fn contains(area: Rect, column: u16, row: u16) -> bool {
    column >= area.x
        && column < area.x.saturating_add(area.width)
        && row >= area.y
        && row < area.y.saturating_add(area.height)
}

fn tab_at_position(tab_areas: &[Rect], column: u16, row: u16) -> Option<usize> {
    tab_areas
        .iter()
        .position(|area| contains(*area, column, row))
}

fn app_at_position(list_area: Rect, column: u16, row: u16) -> Option<IosApp> {
    if !contains(list_area, column, row) {
        return None;
    }
    let index = row.checked_sub(list_area.y)? as usize;
    APPS.get(index).copied()
}

struct WorkerGuard {
    should_stop: Arc<AtomicBool>,
}

impl Drop for WorkerGuard {
    fn drop(&mut self) {
        self.should_stop.store(true, Ordering::Relaxed);
    }
}

fn wait_for_next_refresh(should_stop: &AtomicBool) {
    let mut elapsed = Duration::ZERO;
    while elapsed < REFRESH_INTERVAL && !should_stop.load(Ordering::Relaxed) {
        let sleep = Duration::from_millis(50).min(REFRESH_INTERVAL - elapsed);
        thread::sleep(sleep);
        elapsed += sleep;
    }
}

fn spawn_discovery<F>(
    mut discover: F,
) -> (mpsc::Receiver<io::Result<Vec<DeviceOption>>>, WorkerGuard)
where
    F: FnMut() -> io::Result<Vec<DeviceOption>> + Send + 'static,
{
    let (sender, receiver) = mpsc::channel();
    let should_stop = Arc::new(AtomicBool::new(false));
    let worker_stop = should_stop.clone();

    thread::spawn(move || {
        while !worker_stop.load(Ordering::Relaxed) {
            if sender.send(discover()).is_err() {
                return;
            }
            wait_for_next_refresh(&worker_stop);
        }
    });

    (receiver, WorkerGuard { should_stop })
}

type AvailabilityUpdate = (String, [DisplayAvailability; APPS.len()]);

fn spawn_availability_worker(device: String) -> (mpsc::Receiver<AvailabilityUpdate>, WorkerGuard) {
    let (sender, receiver) = mpsc::channel();
    let should_stop = Arc::new(AtomicBool::new(false));
    let worker_stop = should_stop.clone();

    thread::spawn(move || {
        while !worker_stop.load(Ordering::Relaxed) {
            let bundle_ids = APPS.map(IosApp::bundle_id);
            let availability = match app_availabilities(&device, &bundle_ids) {
                Ok(values) if values.len() == APPS.len() => [values[0].into(), values[1].into()],
                Ok(_) | Err(_) => [DisplayAvailability::DetectionFailed; APPS.len()],
            };
            if sender.send((device.clone(), availability)).is_err() {
                return;
            }
            wait_for_next_refresh(&worker_stop);
        }
    });

    (receiver, WorkerGuard { should_stop })
}

fn ios_device_options() -> io::Result<Vec<DeviceOption>> {
    connected_ios_devices()
        .map(|devices| {
            devices
                .into_iter()
                .map(|device| DeviceOption {
                    id: device.identifier.clone(),
                    name: device.name,
                    detail: format!(
                        "{} · {} · {}",
                        device.model, device.transport, device.identifier
                    ),
                })
                .collect()
        })
        .map_err(io::Error::other)
}

fn android_device_options() -> io::Result<Vec<DeviceOption>> {
    connected_android_devices()
        .map(|devices| {
            devices
                .into_iter()
                .map(|device| {
                    let detail = match device.product {
                        Some(product) if product != device.name => {
                            format!("{} · {}", product, device.serial)
                        }
                        _ => device.serial.clone(),
                    };
                    DeviceOption {
                        id: device.serial,
                        name: device.name,
                        detail,
                    }
                })
                .collect()
        })
        .map_err(io::Error::other)
}

fn picker_layout(area: Rect) -> [Rect; 4] {
    Layout::vertical([
        Constraint::Length(3),
        Constraint::Length(1),
        Constraint::Fill(1),
        Constraint::Length(3),
    ])
    .margin(2)
    .areas(area)
}

fn visible_offset(state: &PickerState, list_area: Rect) -> usize {
    let height = list_area.height as usize;
    if height == 0 {
        return 0;
    }
    state
        .highlighted_device
        .map(|selected| selected.saturating_sub(height - 1))
        .unwrap_or(0)
}

fn app_row(app: IosApp, state: DisplayAvailability) -> String {
    let identity = format!("{}  ({})", app.display_name(), app.subtitle());
    let identity_column_width = APPS
        .iter()
        .map(|app| format!("{}  ({})", app.display_name(), app.subtitle()).width())
        .max()
        .unwrap_or_default();
    let padding = identity_column_width.saturating_sub(identity.width()) + 2;
    format!("{identity}{}· {}", " ".repeat(padding), state.label())
}

fn selected_app_notice(state: &PickerState) -> (String, Style) {
    if state.selected_device.is_none() {
        return (
            format!("{CONTENT_INDENT}请先选择设备"),
            Style::default().fg(Color::DarkGray),
        );
    }

    let app = APPS[state.highlighted_app];
    let Some(availability) = state.app_availability else {
        return (
            format!("{CONTENT_INDENT}正在检测已安装的 APP…"),
            Style::default().fg(Color::DarkGray),
        );
    };

    match availability[state.highlighted_app] {
        DisplayAvailability::Installed => (
            format!(
                "{CONTENT_INDENT}确认后将重新启动 {}；现有进程（如有）会被终止",
                app.display_name()
            ),
            Style::default().fg(Color::Yellow),
        ),
        DisplayAvailability::NotInstalled => (
            format!("{CONTENT_INDENT}{} 未安装在当前设备上", app.display_name()),
            Style::default().fg(Color::Red),
        ),
        DisplayAvailability::DetectionFailed => (
            format!("{CONTENT_INDENT}无法检测 {} 是否已安装", app.display_name()),
            Style::default().fg(Color::Red),
        ),
    }
}

fn draw_picker(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    state: &PickerState,
) -> io::Result<VisibleAreas> {
    let mut visible = VisibleAreas::default();
    terminal.draw(|frame| {
        let [tabs_area, _, list_area, footer_area] = picker_layout(frame.area());
        let tab_constraints = state
            .tabs
            .iter()
            .map(|_| Constraint::Ratio(1, state.tabs.len() as u32));
        visible.tabs = Layout::horizontal(tab_constraints)
            .split(tabs_area)
            .to_vec();
        let offset = visible_offset(state, list_area);
        visible.list = VisibleList {
            area: list_area,
            offset,
        };

        for (index, (tab, area)) in state.tabs.iter().zip(&visible.tabs).enumerate() {
            let active = index == state.active_tab;
            let style = if active {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::DarkGray)
            };
            let border_style = if active {
                Style::default().fg(Color::Cyan)
            } else {
                Style::default().fg(Color::DarkGray)
            };
            frame.render_widget(
                Paragraph::new(tab.title())
                    .alignment(Alignment::Center)
                    .style(style)
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .border_style(border_style),
                    ),
                *area,
            );
        }

        match state.active_tab() {
            TabKind::Device => {
                if state.devices.is_empty() {
                    let message = match (&state.discovery_error, state.has_discovered) {
                        (Some(error), _) => format!("设备发现失败：{error}\n正在重试…"),
                        (None, true) => format!(
                            "未发现已连接的 {} 设备\n连接后会自动出现",
                            state.platform.display_name()
                        ),
                        (None, false) => "正在发现设备…".to_string(),
                    };
                    frame.render_widget(
                        Paragraph::new(message)
                            .alignment(Alignment::Center)
                            .style(Style::default().fg(Color::DarkGray)),
                        list_area,
                    );
                } else {
                    let items = state.devices.iter().map(|device| {
                        ListItem::new(format!("{}  ({})", device.name, device.detail))
                    });
                    let list = List::new(items).highlight_symbol("▶ ").highlight_style(
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::BOLD),
                    );
                    let mut list_state = ListState::default()
                        .with_selected(state.highlighted_device)
                        .with_offset(offset);
                    frame.render_stateful_widget(list, list_area, &mut list_state);
                }
            }
            TabKind::App => {
                if state.selected_device.is_none() {
                    frame.render_widget(
                        Paragraph::new("请先选择设备")
                            .alignment(Alignment::Center)
                            .style(Style::default().fg(Color::DarkGray)),
                        list_area,
                    );
                } else if let Some(availability) = state.app_availability {
                    let items = APPS
                        .iter()
                        .enumerate()
                        .map(|(index, app)| ListItem::new(app_row(*app, availability[index])));
                    let list = List::new(items).highlight_symbol("▶ ").highlight_style(
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::BOLD),
                    );
                    let mut list_state =
                        ListState::default().with_selected(Some(state.highlighted_app));
                    frame.render_stateful_widget(list, list_area, &mut list_state);
                } else {
                    frame.render_widget(
                        Paragraph::new("正在检测已安装的 APP…")
                            .alignment(Alignment::Center)
                            .style(Style::default().fg(Color::DarkGray)),
                        list_area,
                    );
                }
            }
        }

        let notice = match state.active_tab() {
            TabKind::Device => Line::raw(""),
            TabKind::App => {
                let (text, style) = selected_app_notice(state);
                Line::styled(text, style)
            }
        };
        let controls = if state.tabs.len() > 1 {
            "←/→ 切换 Tab · ↑/↓ 选择 · Enter 确认 · Esc 返回 · q 退出"
        } else {
            "↑/↓ 选择 · Enter 确认 · q 退出"
        };
        frame.render_widget(
            Paragraph::new(vec![
                notice,
                Line::styled(
                    format!("{CONTENT_INDENT}{controls}"),
                    Style::default().fg(Color::DarkGray),
                ),
            ]),
            footer_area,
        );
    })?;
    Ok(visible)
}

fn run_picker<F>(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    mut state: PickerState,
    discover: F,
) -> io::Result<Option<PickerAction>>
where
    F: FnMut() -> io::Result<Vec<DeviceOption>> + Send + 'static,
{
    terminal.clear()?;
    let (device_updates, _discovery_guard) = spawn_discovery(discover);
    let mut availability_worker = state.selected_device.clone().map(|device| {
        let (updates, guard) = spawn_availability_worker(device.clone());
        (device, updates, guard)
    });
    let mut visible = draw_picker(terminal, &state)?;

    loop {
        let mut changed = false;
        while let Ok(update) = device_updates.try_recv() {
            if state.update_devices(update) {
                availability_worker = None;
            }
            changed = true;
        }
        if let Some((_, updates, _)) = &availability_worker {
            while let Ok((device, availability)) = updates.try_recv() {
                state.update_availability(&device, availability);
                changed = true;
            }
        }
        if changed {
            visible = draw_picker(terminal, &state)?;
        }

        if !event::poll(EVENT_POLL_INTERVAL)? {
            continue;
        }

        let previous_device = state.selected_device.clone();
        match state.handle_event(event::read()?, &visible) {
            PickerAction::Continue => {
                if state.selected_device != previous_device {
                    availability_worker = state.selected_device.clone().map(|device| {
                        let (updates, guard) = spawn_availability_worker(device.clone());
                        (device, updates, guard)
                    });
                }
                visible = draw_picker(terminal, &state)?;
            }
            PickerAction::Cancel => return Ok(None),
            completed => return Ok(Some(completed)),
        }
    }
}

pub(crate) fn pick_ios(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    retained_device: Option<String>,
    requested_app: Option<IosApp>,
) -> io::Result<Option<IosSelection>> {
    match run_picker(
        terminal,
        PickerState::ios(retained_device, requested_app),
        ios_device_options,
    )? {
        Some(PickerAction::CompleteIos(selection)) => Ok(Some(selection)),
        None => Ok(None),
        Some(_) => unreachable!("iOS picker returned a non-iOS selection"),
    }
}

pub(crate) fn pick_android(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
) -> io::Result<Option<String>> {
    match run_picker(terminal, PickerState::android(), android_device_options)? {
        Some(PickerAction::CompleteDevice(device)) => Ok(Some(device)),
        None => Ok(None),
        Some(_) => unreachable!("Android picker returned a non-Android selection"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyEvent, KeyModifiers};

    fn device(id: &str) -> DeviceOption {
        DeviceOption {
            id: id.to_string(),
            name: id.to_string(),
            detail: "detail".to_string(),
        }
    }

    fn key(code: KeyCode) -> Event {
        Event::Key(KeyEvent::new(code, KeyModifiers::NONE))
    }

    #[test]
    fn ios_flow_exposes_device_then_app_tabs() {
        let state = PickerState::ios(None, None);

        assert_eq!(state.tabs, vec![TabKind::Device, TabKind::App]);
        assert_eq!(state.active_tab(), TabKind::Device);
    }

    #[test]
    fn android_flow_exposes_only_device_tab() {
        let mut state = PickerState::android();

        state.move_tab(1);

        assert_eq!(state.tabs, vec![TabKind::Device]);
        assert_eq!(state.active_tab(), TabKind::Device);
    }

    #[test]
    fn horizontal_navigation_switches_ios_tabs() {
        let mut state = PickerState::ios(None, None);
        let visible = VisibleAreas::default();

        state.handle_event(key(KeyCode::Right), &visible);
        assert_eq!(state.active_tab(), TabKind::App);

        state.handle_event(key(KeyCode::Left), &visible);
        assert_eq!(state.active_tab(), TabKind::Device);
    }

    #[test]
    fn escape_goes_back_one_tab_but_never_quits() {
        let mut state = PickerState::ios(None, None);
        let visible = VisibleAreas::default();
        state.active_tab = 1;

        assert_eq!(
            state.handle_event(key(KeyCode::Esc), &visible),
            PickerAction::Continue
        );
        assert_eq!(state.active_tab(), TabKind::Device);

        assert_eq!(
            state.handle_event(key(KeyCode::Esc), &visible),
            PickerAction::Continue
        );
        assert_eq!(state.active_tab(), TabKind::Device);
    }

    #[test]
    fn q_is_the_picker_quit_key() {
        let mut state = PickerState::ios(None, None);

        assert_eq!(
            state.handle_event(key(KeyCode::Char('q')), &VisibleAreas::default()),
            PickerAction::Cancel
        );
    }

    #[test]
    fn mouse_click_switches_tabs() {
        let mut state = PickerState::ios(None, None);
        let visible = VisibleAreas {
            tabs: vec![Rect::new(0, 0, 10, 3), Rect::new(10, 0, 10, 3)],
            list: VisibleList::default(),
        };
        let click = MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 12,
            row: 1,
            modifiers: KeyModifiers::NONE,
        };

        state.handle_event(Event::Mouse(click), &visible);

        assert_eq!(state.active_tab(), TabKind::App);
    }

    #[test]
    fn selecting_a_different_device_invalidates_later_steps() {
        let mut state = PickerState::ios(Some("one".to_string()), None);
        state.selected_app = Some(IosApp::Douyin);
        state.app_availability = Some([DisplayAvailability::Installed; APPS.len()]);

        state.commit_device("two".to_string());

        assert_eq!(state.selected_device.as_deref(), Some("two"));
        assert_eq!(state.selected_app, None);
        assert_eq!(state.app_availability, None);
    }

    #[test]
    fn selecting_the_same_device_preserves_later_state() {
        let mut state = PickerState::ios(Some("one".to_string()), None);
        state.selected_app = Some(IosApp::Douyin);
        state.app_availability = Some([DisplayAvailability::Installed; APPS.len()]);

        state.commit_device("one".to_string());

        assert_eq!(state.selected_app, Some(IosApp::Douyin));
        assert_eq!(
            state.app_availability,
            Some([DisplayAvailability::Installed; APPS.len()])
        );
    }

    #[test]
    fn disconnected_selected_device_returns_to_device_tab() {
        let mut state = PickerState::ios(Some("one".to_string()), None);

        assert!(state.update_devices(Ok(vec![device("two")])));

        assert_eq!(state.selected_device, None);
        assert_eq!(state.active_tab(), TabKind::Device);
    }

    #[test]
    fn app_cannot_be_confirmed_without_device_or_installation() {
        let mut state = PickerState::ios(None, None);

        assert_eq!(state.commit_app(IosApp::EffectCam), PickerAction::Continue);

        state.selected_device = Some("one".to_string());
        state.app_availability = Some([
            DisplayAvailability::NotInstalled,
            DisplayAvailability::Installed,
        ]);
        assert_eq!(state.commit_app(IosApp::EffectCam), PickerAction::Continue);

        state.app_availability = Some([
            DisplayAvailability::DetectionFailed,
            DisplayAvailability::Installed,
        ]);
        assert_eq!(state.commit_app(IosApp::EffectCam), PickerAction::Continue);
    }

    #[test]
    fn installed_app_can_be_confirmed() {
        let mut state = PickerState::ios(Some("one".to_string()), None);
        state.app_availability = Some([DisplayAvailability::Installed; APPS.len()]);

        assert_eq!(
            state.commit_app(IosApp::EffectCam),
            PickerAction::CompleteIos(IosSelection {
                device: "one".to_string(),
                app: IosApp::EffectCam,
            })
        );
    }

    #[test]
    fn discovery_selects_the_first_device_by_default() {
        let mut state = PickerState::ios(None, None);

        state.update_devices(Ok(vec![device("one"), device("two")]));

        assert_eq!(state.highlighted_device, Some(0));
    }

    #[test]
    fn app_statuses_start_in_the_same_terminal_column() {
        let effectcam = app_row(IosApp::EffectCam, DisplayAvailability::Installed);
        let douyin = app_row(IosApp::Douyin, DisplayAvailability::Installed);
        let effectcam_separator = effectcam.find('·').unwrap();
        let douyin_separator = douyin.find('·').unwrap();

        assert_eq!(
            effectcam[..effectcam_separator].width(),
            douyin[..douyin_separator].width()
        );
    }
}
