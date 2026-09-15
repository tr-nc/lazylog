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
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc,
};
use std::thread;
use std::time::Duration;

const DISCOVERY_INTERVAL: Duration = Duration::from_secs(1);
const EVENT_POLL_INTERVAL: Duration = Duration::from_millis(50);

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DeviceOption {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) detail: String,
}

#[derive(Debug, PartialEq, Eq)]
enum PickerAction {
    Continue,
    Select(String),
    Cancel,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct VisibleList {
    area: Rect,
    offset: usize,
}

struct PickerState {
    devices: Vec<DeviceOption>,
    selected: Option<usize>,
    discovery_error: Option<String>,
    has_discovered: bool,
}

impl PickerState {
    fn new() -> Self {
        Self {
            devices: Vec::new(),
            selected: None,
            discovery_error: None,
            has_discovered: false,
        }
    }

    fn update_devices(&mut self, result: io::Result<Vec<DeviceOption>>) {
        self.has_discovered = true;
        match result {
            Ok(devices) => {
                let selected_id = self
                    .selected
                    .and_then(|index| self.devices.get(index))
                    .map(|device| device.id.as_str());
                self.selected = selected_id
                    .and_then(|id| devices.iter().position(|device| device.id == id))
                    .or_else(|| (!devices.is_empty()).then_some(0));
                self.devices = devices;
                self.discovery_error = None;
            }
            Err(error) => self.discovery_error = Some(error.to_string()),
        }
    }

    fn handle_event(&mut self, event: Event, visible: VisibleList) -> PickerAction {
        match event {
            Event::Key(key) if key.kind == KeyEventKind::Press => match key.code {
                KeyCode::Up | KeyCode::Char('k') => {
                    self.move_selection(-1);
                    PickerAction::Continue
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    self.move_selection(1);
                    PickerAction::Continue
                }
                KeyCode::Enter => self
                    .selected
                    .and_then(|index| self.devices.get(index))
                    .map(|device| PickerAction::Select(device.id.clone()))
                    .unwrap_or(PickerAction::Continue),
                KeyCode::Esc | KeyCode::Char('q') => PickerAction::Cancel,
                _ => PickerAction::Continue,
            },
            Event::Mouse(MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column,
                row,
                ..
            }) => self
                .device_at_position(visible, column, row)
                .map(|device| PickerAction::Select(device.id.clone()))
                .unwrap_or(PickerAction::Continue),
            Event::Mouse(MouseEvent {
                kind: MouseEventKind::ScrollUp,
                ..
            }) => {
                self.move_selection(-1);
                PickerAction::Continue
            }
            Event::Mouse(MouseEvent {
                kind: MouseEventKind::ScrollDown,
                ..
            }) => {
                self.move_selection(1);
                PickerAction::Continue
            }
            _ => PickerAction::Continue,
        }
    }

    fn move_selection(&mut self, delta: isize) {
        if self.devices.is_empty() {
            self.selected = None;
            return;
        }

        let current = self.selected.unwrap_or(0);
        self.selected = Some(if delta < 0 {
            current.saturating_sub(delta.unsigned_abs())
        } else {
            current
                .saturating_add(delta as usize)
                .min(self.devices.len() - 1)
        });
    }

    fn device_at_position(
        &self,
        visible: VisibleList,
        column: u16,
        row: u16,
    ) -> Option<&DeviceOption> {
        if column < visible.area.x || column >= visible.area.x.saturating_add(visible.area.width) {
            return None;
        }
        let visible_index = row.checked_sub(visible.area.y)? as usize;
        if visible_index >= visible.area.height as usize {
            return None;
        }
        self.devices.get(visible.offset + visible_index)
    }
}

struct DiscoveryGuard {
    should_stop: Arc<AtomicBool>,
}

impl Drop for DiscoveryGuard {
    fn drop(&mut self) {
        self.should_stop.store(true, Ordering::Relaxed);
    }
}

fn spawn_discovery<F>(
    mut discover: F,
) -> (
    mpsc::Receiver<io::Result<Vec<DeviceOption>>>,
    DiscoveryGuard,
)
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

            let mut elapsed = Duration::ZERO;
            while elapsed < DISCOVERY_INTERVAL && !worker_stop.load(Ordering::Relaxed) {
                let sleep = Duration::from_millis(50).min(DISCOVERY_INTERVAL - elapsed);
                thread::sleep(sleep);
                elapsed += sleep;
            }
        }
    });

    (receiver, DiscoveryGuard { should_stop })
}

fn picker_layout(area: Rect) -> [Rect; 3] {
    Layout::vertical([
        Constraint::Length(2),
        Constraint::Fill(1),
        Constraint::Length(2),
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
        .selected
        .map(|selected| selected.saturating_sub(height - 1))
        .unwrap_or(0)
}

fn draw_picker(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    platform: &str,
    state: &PickerState,
) -> io::Result<VisibleList> {
    let mut visible = VisibleList::default();
    terminal.draw(|frame| {
        let [header_area, list_area, footer_area] = picker_layout(frame.area());
        let offset = visible_offset(state, list_area);
        visible = VisibleList {
            area: list_area,
            offset,
        };

        let header = Paragraph::new(Line::styled(
            format!("选择 {platform} 设备"),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ))
        .alignment(Alignment::Center);
        frame.render_widget(header, header_area);

        if state.devices.is_empty() {
            let message = match (&state.discovery_error, state.has_discovered) {
                (Some(error), _) => format!("设备发现失败：{error}\n正在重试…"),
                (None, true) => format!("未发现已连接的 {platform} 设备\n连接后会自动出现"),
                (None, false) => "正在发现设备…".to_string(),
            };
            frame.render_widget(
                Paragraph::new(message)
                    .alignment(Alignment::Center)
                    .style(Style::default().fg(Color::DarkGray)),
                list_area,
            );
        } else {
            let items = state
                .devices
                .iter()
                .map(|device| ListItem::new(format!("{}  ({})", device.name, device.detail)));
            let list = List::new(items).highlight_symbol("▶ ").highlight_style(
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            );
            let mut list_state = ListState::default()
                .with_selected(state.selected)
                .with_offset(offset);
            frame.render_stateful_widget(list, list_area, &mut list_state);
        }

        let footer = Paragraph::new("↑/↓ 或 j/k 选择 · Enter 确认 · 鼠标点击选择 · Esc/q 退出")
            .alignment(Alignment::Center)
            .style(Style::default().fg(Color::DarkGray));
        frame.render_widget(footer, footer_area);
    })?;
    Ok(visible)
}

pub(crate) fn pick<F>(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    platform: &str,
    discover: F,
) -> io::Result<Option<String>>
where
    F: FnMut() -> io::Result<Vec<DeviceOption>> + Send + 'static,
{
    terminal.clear()?;
    let (updates, _guard) = spawn_discovery(discover);
    let mut state = PickerState::new();
    let mut visible = draw_picker(terminal, platform, &state)?;

    loop {
        let mut changed = false;
        while let Ok(update) = updates.try_recv() {
            state.update_devices(update);
            changed = true;
        }
        if changed {
            visible = draw_picker(terminal, platform, &state)?;
        }

        if !event::poll(EVENT_POLL_INTERVAL)? {
            continue;
        }

        match state.handle_event(event::read()?, visible) {
            PickerAction::Continue => visible = draw_picker(terminal, platform, &state)?,
            PickerAction::Select(device) => return Ok(Some(device)),
            PickerAction::Cancel => return Ok(None),
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

    #[test]
    fn discovery_selects_the_first_device_by_default() {
        let mut state = PickerState::new();
        state.update_devices(Ok(vec![device("one"), device("two")]));

        assert_eq!(state.selected, Some(0));
        assert_eq!(
            state.handle_event(
                Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
                VisibleList::default(),
            ),
            PickerAction::Select("one".to_string())
        );
    }

    #[test]
    fn refresh_preserves_the_selected_device_by_identity() {
        let mut state = PickerState::new();
        state.update_devices(Ok(vec![device("one"), device("two")]));
        state.selected = Some(1);
        state.update_devices(Ok(vec![device("two"), device("three")]));

        assert_eq!(state.selected, Some(0));
        assert_eq!(state.devices[state.selected.unwrap()].id, "two");
    }

    #[test]
    fn empty_refresh_clears_the_selection() {
        let mut state = PickerState::new();
        state.update_devices(Ok(vec![device("one")]));
        state.update_devices(Ok(Vec::new()));

        assert_eq!(state.selected, None);
    }
}
