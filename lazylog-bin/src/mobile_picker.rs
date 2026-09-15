use crossterm::event::{
    self, Event, KeyCode, KeyEventKind, MouseButton, MouseEvent, MouseEventKind,
};
use lazylog_android::connected_devices as connected_android_devices;
use lazylog_framework::{
    Panel, SELECTED_STYLE, get_mode_color,
    status_bar::{StatusBar, StatusGravity, StatusStyle},
};
use lazylog_ios::{
    IosAppAvailability, app_availabilities, connected_devices as connected_ios_devices,
};
use ratatui::{
    Terminal,
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Clear, List, ListItem, ListState, Padding, Paragraph},
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
const PROVIDERS: [ProviderKind; 4] = [
    ProviderKind::Ios,
    ProviderKind::Android,
    ProviderKind::DyehPreview,
    ProviderKind::DyehEditor,
];
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
            _ if value == "像塑" || value == "像塑内测版" => Ok(Self::EffectCam),
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
            Self::EffectCam => "像塑内测版",
            Self::Douyin => "抖音开发版",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct IosSelection {
    pub(crate) device: String,
    pub(crate) app: IosApp,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ProviderKind {
    Ios,
    Android,
    DyehPreview,
    DyehEditor,
}

impl ProviderKind {
    pub(crate) fn display_name(self) -> &'static str {
        match self {
            Self::Ios => "iOS",
            Self::Android => "Android",
            Self::DyehPreview => "DYEH 预览",
            Self::DyehEditor => "DYEH 编辑器",
        }
    }

    pub(crate) fn mode_name(self) -> &'static str {
        match self {
            Self::Ios => "ios",
            Self::Android => "android",
            Self::DyehPreview => "dyeh preview",
            Self::DyehEditor => "dyeh editor",
        }
    }

    fn platform(self) -> Option<PickerPlatform> {
        match self {
            Self::Ios => Some(PickerPlatform::Ios),
            Self::Android => Some(PickerPlatform::Android),
            Self::DyehPreview | Self::DyehEditor => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum PickerSelection {
    Ios(IosSelection),
    Android { device: String },
    DyehPreview,
    DyehEditor,
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
    Provider,
    Device,
    App,
}

impl TabKind {
    fn title(self) -> &'static str {
        match self {
            Self::Provider => "Provider",
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
    Complete(PickerSelection),
    Cancel,
}

pub(crate) struct PickerState {
    selected_provider: Option<ProviderKind>,
    highlighted_provider: usize,
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
    message: Option<String>,
}

impl PickerState {
    pub(crate) fn new(
        requested_provider: Option<ProviderKind>,
        requested_app: Option<IosApp>,
    ) -> Self {
        let tabs = requested_provider
            .map(Self::tabs_for)
            .unwrap_or_else(|| vec![TabKind::Provider]);
        let active_tab =
            usize::from(requested_provider.is_some_and(|provider| provider.platform().is_some()));
        let highlighted_app = requested_app
            .and_then(|requested| APPS.iter().position(|app| *app == requested))
            .unwrap_or(0);
        Self {
            selected_provider: requested_provider,
            highlighted_provider: requested_provider
                .and_then(|requested| PROVIDERS.iter().position(|provider| *provider == requested))
                .unwrap_or(0),
            tabs,
            active_tab,
            devices: Vec::new(),
            highlighted_device: None,
            selected_device: None,
            highlighted_app,
            selected_app: None,
            requested_app,
            app_availability: None,
            discovery_error: None,
            has_discovered: false,
            message: None,
        }
    }

    fn tabs_for(provider: ProviderKind) -> Vec<TabKind> {
        match provider {
            ProviderKind::Ios => vec![TabKind::Provider, TabKind::Device, TabKind::App],
            ProviderKind::Android => vec![TabKind::Provider, TabKind::Device],
            ProviderKind::DyehPreview | ProviderKind::DyehEditor => vec![TabKind::Provider],
        }
    }

    pub(crate) fn set_message(&mut self, message: impl Into<String>) {
        self.message = Some(message.into());
    }

    fn selected_platform(&self) -> Option<PickerPlatform> {
        self.selected_provider.and_then(ProviderKind::platform)
    }

    fn selected_ios_device(&self) -> Option<String> {
        matches!(self.selected_provider, Some(ProviderKind::Ios))
            .then(|| self.selected_device.clone())
            .flatten()
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
                    self.active_tab = self.tab_index(TabKind::Device).unwrap_or(0);
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

    fn invalidate_after_provider(&mut self) {
        self.devices.clear();
        self.highlighted_device = None;
        self.selected_device = None;
        self.requested_app = None;
        self.invalidate_after_device();
        self.discovery_error = None;
        self.has_discovered = false;
    }

    fn tab_index(&self, tab: TabKind) -> Option<usize> {
        self.tabs.iter().position(|candidate| *candidate == tab)
    }

    fn commit_provider(&mut self, provider: ProviderKind) -> PickerAction {
        if self.selected_provider != Some(provider) {
            self.selected_provider = Some(provider);
            self.invalidate_after_provider();
            self.tabs = Self::tabs_for(provider);
        }

        match provider {
            ProviderKind::Ios | ProviderKind::Android => {
                self.active_tab = self
                    .tab_index(TabKind::Device)
                    .expect("mobile provider must include a device tab");
                PickerAction::Continue
            }
            ProviderKind::DyehPreview => PickerAction::Complete(PickerSelection::DyehPreview),
            ProviderKind::DyehEditor => PickerAction::Complete(PickerSelection::DyehEditor),
        }
    }

    fn commit_highlighted_provider(&mut self) -> PickerAction {
        self.commit_provider(PROVIDERS[self.highlighted_provider])
    }

    fn commit_device(&mut self, id: String) -> PickerAction {
        let device_changed = self.selected_device.as_deref() != Some(id.as_str());
        if device_changed {
            self.selected_device = Some(id.clone());
            self.invalidate_after_device();
        }

        match self.selected_provider {
            Some(ProviderKind::Android) => {
                PickerAction::Complete(PickerSelection::Android { device: id })
            }
            Some(ProviderKind::Ios) => {
                if let Some(app) = self.requested_app.take() {
                    self.selected_app = Some(app);
                    PickerAction::Complete(PickerSelection::Ios(IosSelection { device: id, app }))
                } else {
                    self.active_tab = self
                        .tab_index(TabKind::App)
                        .expect("iOS picker must include an APP tab");
                    PickerAction::Continue
                }
            }
            _ => PickerAction::Continue,
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
        if !APPS.contains(&app) {
            return PickerAction::Continue;
        }
        self.selected_app = Some(app);
        PickerAction::Complete(PickerSelection::Ios(IosSelection { device, app }))
    }

    fn move_tab(&mut self, delta: isize) {
        self.active_tab = move_index(self.active_tab, self.tabs.len(), delta);
    }

    fn move_highlight(&mut self, delta: isize) {
        match self.active_tab() {
            TabKind::Provider => {
                self.highlighted_provider =
                    move_index(self.highlighted_provider, PROVIDERS.len(), delta);
            }
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
        if matches!(
            &event,
            Event::Key(key) if key.kind == KeyEventKind::Press
        ) || matches!(
            &event,
            Event::Mouse(MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                ..
            })
        ) {
            self.message = None;
        }

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
                    TabKind::Provider => self.commit_highlighted_provider(),
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
                    TabKind::Provider => provider_at_position(visible.list.area, column, row)
                        .map(|provider| self.commit_provider(provider))
                        .unwrap_or(PickerAction::Continue),
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

fn provider_at_position(list_area: Rect, column: u16, row: u16) -> Option<ProviderKind> {
    if !contains(list_area, column, row) {
        return None;
    }
    let index = row.checked_sub(list_area.y)? as usize;
    PROVIDERS.get(index).copied()
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

fn device_options(platform: PickerPlatform) -> io::Result<Vec<DeviceOption>> {
    match platform {
        PickerPlatform::Ios => ios_device_options(),
        PickerPlatform::Android => android_device_options(),
    }
}

fn centered_panel(area: Rect) -> Rect {
    let inset_width = if area.width > 4 {
        area.width - 4
    } else {
        area.width
    };
    let width = inset_width.min(88);
    let height = area.height.min(14);
    Rect::new(
        area.x + area.width.saturating_sub(width) / 2,
        area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    )
}

fn picker_layout(area: Rect) -> (Rect, Rect) {
    let [body, footer] = Layout::vertical([Constraint::Fill(1), Constraint::Length(1)]).areas(area);
    (centered_panel(body), footer)
}

fn tab_width(tab: TabKind) -> u16 {
    (tab.title().width() + 2).min(u16::MAX as usize) as u16
}

fn tab_areas(panel_area: Rect, tabs: &[TabKind]) -> Vec<Rect> {
    let mut x = panel_area.x.saturating_add(1);
    let right = panel_area.x.saturating_add(panel_area.width);
    tabs.iter()
        .map(|tab| {
            let width = tab_width(*tab).min(right.saturating_sub(x));
            let area = Rect::new(x, panel_area.y, width, u16::from(width > 0));
            x = x.saturating_add(width);
            area
        })
        .collect()
}

fn mode_color(state: &PickerState) -> Color {
    let provider = if state.active_tab() == TabKind::Provider {
        PROVIDERS.get(state.highlighted_provider).copied()
    } else {
        state.selected_provider
    };
    get_mode_color(&provider.map(|provider| provider.mode_name().to_string()))
}

fn tabs_title(state: &PickerState, color: Color) -> Line<'static> {
    let spans = state.tabs.iter().enumerate().map(|(index, tab)| {
        let style = if index == state.active_tab {
            Style::default()
                .fg(Color::Black)
                .bg(color)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        Span::styled(format!(" {} ", tab.title()), style)
    });
    Line::from_iter(spans)
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

fn app_row(app: IosApp, availability: Option<DisplayAvailability>) -> String {
    let identity = app.display_name();
    let Some(availability) = availability else {
        return identity.to_string();
    };
    let identity_column_width = APPS
        .iter()
        .map(|app| app.display_name().width())
        .max()
        .unwrap_or_default();
    let padding = identity_column_width.saturating_sub(identity.width()) + 2;
    format!("{identity}{}{}", " ".repeat(padding), availability.label())
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
            format!(
                "{CONTENT_INDENT}确认后将重新启动 {}；安装状态正在后台检测",
                app.display_name()
            ),
            Style::default().fg(Color::Yellow),
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
            format!(
                "{CONTENT_INDENT}提示：{} 未安装；确认后仍会尝试启动",
                app.display_name()
            ),
            Style::default().fg(Color::Red),
        ),
        DisplayAvailability::DetectionFailed => (
            format!(
                "{CONTENT_INDENT}安装状态检测失败；确认后仍会尝试启动 {}",
                app.display_name()
            ),
            Style::default().fg(Color::Yellow),
        ),
    }
}

fn render_list(
    frame: &mut ratatui::Frame<'_>,
    items: impl IntoIterator<Item = ListItem<'static>>,
    selected: Option<usize>,
    offset: usize,
    area: Rect,
) {
    let list = List::new(items).highlight_style(SELECTED_STYLE);
    let mut list_state = ListState::default()
        .with_selected(selected)
        .with_offset(offset);
    frame.render_stateful_widget(list, area, &mut list_state);
}

fn draw_picker(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    state: &PickerState,
) -> io::Result<VisibleAreas> {
    let mut visible = VisibleAreas::default();
    terminal.draw(|frame| {
        let (panel_area, footer_area) = picker_layout(frame.area());
        let color = mode_color(state);
        let panel = Panel::new(color)
            .title(tabs_title(state, color))
            .padding(Padding::horizontal(1))
            .focused(true)
            .build();
        let panel_inner = panel.inner(panel_area);
        let [list_area, notice_area] =
            Layout::vertical([Constraint::Fill(1), Constraint::Length(1)]).areas(panel_inner);
        frame.render_widget(Clear, panel_area);
        frame.render_widget(panel, panel_area);

        visible.tabs = tab_areas(panel_area, &state.tabs);
        let offset = visible_offset(state, list_area);
        visible.list = VisibleList {
            area: list_area,
            offset,
        };

        match state.active_tab() {
            TabKind::Provider => {
                let items = PROVIDERS.iter().map(|provider| {
                    ListItem::new(format!("{CONTENT_INDENT}{}", provider.display_name()))
                });
                render_list(frame, items, Some(state.highlighted_provider), 0, list_area);
            }
            TabKind::Device => {
                if state.devices.is_empty() {
                    let message = match (&state.discovery_error, state.has_discovered) {
                        (Some(error), _) => format!("设备发现失败：{error}\n正在重试…"),
                        (None, true) => format!(
                            "未发现已连接的 {} 设备\n连接后会自动出现",
                            state
                                .selected_platform()
                                .map(PickerPlatform::display_name)
                                .unwrap_or("移动")
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
                        ListItem::new(format!(
                            "{CONTENT_INDENT}{}  ({})",
                            device.name, device.detail
                        ))
                    });
                    render_list(frame, items, state.highlighted_device, offset, list_area);
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
                } else {
                    let items = APPS.iter().enumerate().map(|(index, app)| {
                        ListItem::new(format!(
                            "{CONTENT_INDENT}{}",
                            app_row(*app, state.app_availability.map(|values| values[index]),)
                        ))
                    });
                    render_list(frame, items, Some(state.highlighted_app), 0, list_area);
                }
            }
        }

        let notice = match state.active_tab() {
            TabKind::Provider | TabKind::Device => Line::raw(""),
            TabKind::App => {
                let (text, style) = selected_app_notice(state);
                Line::styled(text, style)
            }
        };
        frame.render_widget(Paragraph::new(notice), notice_area);

        let provider_name = state
            .selected_provider
            .map(ProviderKind::display_name)
            .unwrap_or("选择 Provider");
        let mut status_bar = StatusBar::new()
            .add_status(
                StatusGravity::Left,
                provider_name.to_string(),
                StatusStyle::new().fg(color),
            )
            .add_status_plain(
                StatusGravity::Right,
                &format!("lazylog v{}", env!("CARGO_PKG_VERSION")),
            );
        if let Some(message) = &state.message {
            status_bar = status_bar.add_status(
                StatusGravity::Mid,
                format!(" {message} "),
                StatusStyle::new().fg(Color::LightRed),
            );
        } else {
            status_bar = status_bar.add_status_plain(
                StatusGravity::Mid,
                "←/→ Tab · ↑/↓ 选择 · Enter 确认 · Esc 返回 · q 退出",
            );
        }
        status_bar.render(footer_area, frame.buffer_mut());
    })?;
    Ok(visible)
}

type DiscoveryWorker = (
    PickerPlatform,
    mpsc::Receiver<io::Result<Vec<DeviceOption>>>,
    WorkerGuard,
);

fn spawn_discovery_for(platform: Option<PickerPlatform>) -> Option<DiscoveryWorker> {
    platform.map(|platform| {
        let (updates, guard) = spawn_discovery(move || device_options(platform));
        (platform, updates, guard)
    })
}

pub(crate) fn pick(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    state: &mut PickerState,
) -> io::Result<Option<PickerSelection>> {
    terminal.clear()?;
    let mut discovery_worker = spawn_discovery_for(state.selected_platform());
    let mut availability_worker = state.selected_ios_device().map(|device| {
        let (updates, guard) = spawn_availability_worker(device.clone());
        (device, updates, guard)
    });
    let mut visible = draw_picker(terminal, state)?;

    loop {
        let mut changed = false;
        if let Some((platform, updates, _)) = &discovery_worker
            && Some(*platform) == state.selected_platform()
        {
            while let Ok(update) = updates.try_recv() {
                if state.update_devices(update) {
                    availability_worker = None;
                }
                changed = true;
            }
        }
        if let Some((_, updates, _)) = &availability_worker {
            while let Ok((device, availability)) = updates.try_recv() {
                state.update_availability(&device, availability);
                changed = true;
            }
        }
        if changed {
            visible = draw_picker(terminal, state)?;
        }

        if !event::poll(EVENT_POLL_INTERVAL)? {
            continue;
        }

        let previous_platform = state.selected_platform();
        let previous_ios_device = state.selected_ios_device();
        match state.handle_event(event::read()?, &visible) {
            PickerAction::Continue => {
                if state.selected_platform() != previous_platform {
                    discovery_worker = spawn_discovery_for(state.selected_platform());
                }
                if state.selected_ios_device() != previous_ios_device {
                    availability_worker = state.selected_ios_device().map(|device| {
                        let (updates, guard) = spawn_availability_worker(device.clone());
                        (device, updates, guard)
                    });
                }
                visible = draw_picker(terminal, state)?;
            }
            PickerAction::Cancel => return Ok(None),
            PickerAction::Complete(selection) => return Ok(Some(selection)),
        }
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

    fn ios_state(device: Option<&str>) -> PickerState {
        let mut state = PickerState::new(Some(ProviderKind::Ios), None);
        state.selected_device = device.map(str::to_string);
        state
    }

    #[test]
    fn root_flow_starts_on_provider_with_first_option_highlighted() {
        let state = PickerState::new(None, None);

        assert_eq!(state.tabs, vec![TabKind::Provider]);
        assert_eq!(state.active_tab(), TabKind::Provider);
        assert_eq!(state.highlighted_provider, 0);
    }

    #[test]
    fn ios_flow_exposes_progressive_tabs() {
        let state = PickerState::new(Some(ProviderKind::Ios), None);

        assert_eq!(
            state.tabs,
            vec![TabKind::Provider, TabKind::Device, TabKind::App]
        );
        assert_eq!(state.active_tab(), TabKind::Device);
    }

    #[test]
    fn android_flow_exposes_provider_and_device_tabs() {
        let state = PickerState::new(Some(ProviderKind::Android), None);

        assert_eq!(state.tabs, vec![TabKind::Provider, TabKind::Device]);
        assert_eq!(state.active_tab(), TabKind::Device);
    }

    #[test]
    fn dyeh_flows_only_need_the_provider_tab() {
        for provider in [ProviderKind::DyehPreview, ProviderKind::DyehEditor] {
            let state = PickerState::new(Some(provider), None);
            assert_eq!(state.tabs, vec![TabKind::Provider]);
        }
    }

    #[test]
    fn all_current_providers_are_listed() {
        assert_eq!(
            PROVIDERS,
            [
                ProviderKind::Ios,
                ProviderKind::Android,
                ProviderKind::DyehPreview,
                ProviderKind::DyehEditor,
            ]
        );
    }

    #[test]
    fn choosing_ios_advances_to_device() {
        let mut state = PickerState::new(None, None);

        assert_eq!(
            state.commit_provider(ProviderKind::Ios),
            PickerAction::Continue
        );
        assert_eq!(state.active_tab(), TabKind::Device);
        assert_eq!(
            state.tabs,
            vec![TabKind::Provider, TabKind::Device, TabKind::App]
        );
    }

    #[test]
    fn choosing_dyeh_completes_without_a_device() {
        let mut state = PickerState::new(None, None);

        assert_eq!(
            state.commit_provider(ProviderKind::DyehPreview),
            PickerAction::Complete(PickerSelection::DyehPreview)
        );
    }

    #[test]
    fn selecting_a_different_provider_invalidates_later_steps() {
        let mut state = ios_state(Some("one"));
        state.selected_app = Some(IosApp::Douyin);
        state.app_availability = Some([DisplayAvailability::Installed; APPS.len()]);

        state.commit_provider(ProviderKind::Android);

        assert_eq!(state.selected_device, None);
        assert_eq!(state.selected_app, None);
        assert_eq!(state.app_availability, None);
        assert_eq!(state.tabs, vec![TabKind::Provider, TabKind::Device]);
    }

    #[test]
    fn horizontal_navigation_switches_tabs() {
        let mut state = PickerState::new(Some(ProviderKind::Ios), None);
        let visible = VisibleAreas::default();

        state.handle_event(key(KeyCode::Right), &visible);
        assert_eq!(state.active_tab(), TabKind::App);

        state.handle_event(key(KeyCode::Left), &visible);
        assert_eq!(state.active_tab(), TabKind::Device);
        state.handle_event(key(KeyCode::Left), &visible);
        assert_eq!(state.active_tab(), TabKind::Provider);
    }

    #[test]
    fn escape_goes_back_one_tab_but_never_quits() {
        let mut state = PickerState::new(Some(ProviderKind::Ios), None);
        let visible = VisibleAreas::default();
        state.active_tab = 2;

        assert_eq!(
            state.handle_event(key(KeyCode::Esc), &visible),
            PickerAction::Continue
        );
        assert_eq!(state.active_tab(), TabKind::Device);
        state.handle_event(key(KeyCode::Esc), &visible);
        assert_eq!(state.active_tab(), TabKind::Provider);
        assert_eq!(
            state.handle_event(key(KeyCode::Esc), &visible),
            PickerAction::Continue
        );
        assert_eq!(state.active_tab(), TabKind::Provider);
    }

    #[test]
    fn q_is_the_picker_quit_key() {
        let mut state = PickerState::new(None, None);

        assert_eq!(
            state.handle_event(key(KeyCode::Char('q')), &VisibleAreas::default()),
            PickerAction::Cancel
        );
    }

    #[test]
    fn mouse_click_switches_tabs() {
        let mut state = PickerState::new(Some(ProviderKind::Ios), None);
        let visible = VisibleAreas {
            tabs: vec![
                Rect::new(0, 0, 10, 1),
                Rect::new(10, 0, 10, 1),
                Rect::new(20, 0, 10, 1),
            ],
            list: VisibleList::default(),
        };
        let click = MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 22,
            row: 0,
            modifiers: KeyModifiers::NONE,
        };

        state.handle_event(Event::Mouse(click), &visible);

        assert_eq!(state.active_tab(), TabKind::App);
    }

    #[test]
    fn selecting_a_different_device_invalidates_app_state() {
        let mut state = ios_state(Some("one"));
        state.selected_app = Some(IosApp::Douyin);
        state.app_availability = Some([DisplayAvailability::Installed; APPS.len()]);

        state.commit_device("two".to_string());

        assert_eq!(state.selected_device.as_deref(), Some("two"));
        assert_eq!(state.selected_app, None);
        assert_eq!(state.app_availability, None);
    }

    #[test]
    fn selecting_the_same_device_preserves_later_state() {
        let mut state = ios_state(Some("one"));
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
        let mut state = ios_state(Some("one"));
        state.active_tab = 2;

        assert!(state.update_devices(Ok(vec![device("two")])));

        assert_eq!(state.selected_device, None);
        assert_eq!(state.active_tab(), TabKind::Device);
    }

    #[test]
    fn app_cannot_be_confirmed_without_a_device() {
        let mut state = ios_state(None);

        assert_eq!(state.commit_app(IosApp::EffectCam), PickerAction::Continue);
    }

    #[test]
    fn preset_app_can_be_confirmed_before_installation_hint_arrives() {
        let mut state = ios_state(Some("one"));

        assert_eq!(
            state.commit_app(IosApp::EffectCam),
            PickerAction::Complete(PickerSelection::Ios(IosSelection {
                device: "one".to_string(),
                app: IosApp::EffectCam,
            }))
        );
    }

    #[test]
    fn requested_app_skips_app_tab_once_device_is_selected() {
        let mut state = PickerState::new(Some(ProviderKind::Ios), Some(IosApp::Douyin));

        assert_eq!(
            state.commit_device("one".to_string()),
            PickerAction::Complete(PickerSelection::Ios(IosSelection {
                device: "one".to_string(),
                app: IosApp::Douyin,
            }))
        );
        assert_eq!(state.requested_app, None);
    }

    #[test]
    fn installation_hint_never_disables_a_preset_app() {
        let mut state = ios_state(Some("one"));

        state.app_availability = Some([
            DisplayAvailability::NotInstalled,
            DisplayAvailability::Installed,
        ]);
        assert!(matches!(
            state.commit_app(IosApp::EffectCam),
            PickerAction::Complete(PickerSelection::Ios(_))
        ));

        state.app_availability = Some([
            DisplayAvailability::DetectionFailed,
            DisplayAvailability::Installed,
        ]);
        assert!(matches!(
            state.commit_app(IosApp::EffectCam),
            PickerAction::Complete(PickerSelection::Ios(_))
        ));
    }

    #[test]
    fn discovery_selects_the_first_device_by_default() {
        let mut state = PickerState::new(Some(ProviderKind::Ios), None);

        state.update_devices(Ok(vec![device("one"), device("two")]));

        assert_eq!(state.highlighted_device, Some(0));
    }

    #[test]
    fn tabs_are_compact_and_embedded_on_the_panel_border() {
        assert_eq!(tab_width(TabKind::Provider), 10);
        assert_eq!(tab_width(TabKind::Device), 6);
        assert_eq!(tab_width(TabKind::App), 5);

        let areas = tab_areas(
            Rect::new(10, 4, 40, 12),
            &[TabKind::Provider, TabKind::Device, TabKind::App],
        );
        assert_eq!(
            areas,
            vec![
                Rect::new(11, 4, 10, 1),
                Rect::new(21, 4, 6, 1),
                Rect::new(27, 4, 5, 1),
            ]
        );
    }

    #[test]
    fn picker_panel_is_centered_and_bounded() {
        assert_eq!(
            centered_panel(Rect::new(0, 0, 120, 30)),
            Rect::new(16, 8, 88, 14)
        );
        assert_eq!(
            centered_panel(Rect::new(3, 2, 40, 10)),
            Rect::new(5, 2, 36, 10)
        );
    }

    #[test]
    fn app_statuses_start_in_the_same_terminal_column() {
        let effectcam = app_row(IosApp::EffectCam, Some(DisplayAvailability::Installed));
        let douyin = app_row(IosApp::Douyin, Some(DisplayAvailability::Installed));
        let effectcam_status = effectcam.find('✓').unwrap();
        let douyin_status = douyin.find('✓').unwrap();

        assert_eq!(
            effectcam[..effectcam_status].width(),
            douyin[..douyin_status].width()
        );
    }

    #[test]
    fn preset_app_rows_exist_before_availability_hint_arrives() {
        assert_eq!(app_row(IosApp::EffectCam, None), "像塑内测版");
        assert_eq!(app_row(IosApp::Douyin, None), "抖音开发版");
    }
}
