use crate::{
    app_block::AppBlock,
    filter::FilterEngine,
    log_list::LogList,
    log_parser::{LogDetailLevel, LogItem},
    provider::{LogParser, LogProvider, spawn_provider_thread},
    status_bar::DisplayEvent,
    theme,
    ui_logger::UiLogger,
};
use anyhow::{Result, anyhow};
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, MouseEvent},
    execute,
};
use ratatui::{Terminal, backend::CrosstermBackend, prelude::*, widgets::Widget};
use ringbuf::{
    HeapRb,
    traits::{Consumer, Split},
};
use std::{
    io,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

mod events;
mod render;
mod scrolling;
mod selection;

// constants
const DEFAULT_POLL_INTERVAL_MS: u64 = 100;
const DEFAULT_EVENT_POLL_INTERVAL_MS: u64 = 16;
const DEFAULT_RING_BUFFER_SIZE: usize = 16384;
const HELP_POPUP_WIDTH: u16 = 60;
const SCROLL_PAD: usize = 1;
const HORIZONTAL_SCROLL_STEP: usize = 5;
const DISPLAY_EVENT_DURATION_MS: u64 = 800;

#[derive(Clone)]
pub struct AppDesc {
    pub poll_interval: Duration,
    pub event_poll_interval: Duration,
    pub show_debug_logs: bool,
    pub ring_buffer_size: usize,
    pub initial_filter: Option<String>,
    pub parser: Arc<dyn LogParser>,
}

impl AppDesc {
    pub fn new(parser: Arc<dyn LogParser>) -> Self {
        Self {
            poll_interval: Duration::from_millis(DEFAULT_POLL_INTERVAL_MS),
            event_poll_interval: Duration::from_millis(DEFAULT_EVENT_POLL_INTERVAL_MS),
            show_debug_logs: false,
            ring_buffer_size: DEFAULT_RING_BUFFER_SIZE,
            initial_filter: None,
            parser,
        }
    }
}

/// Start the application with default configuration
pub fn start_with_provider<P>(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    provider: P,
    parser: Arc<dyn LogParser>,
) -> Result<()>
where
    P: LogProvider + 'static,
{
    start_with_desc(terminal, provider, AppDesc::new(parser))
}

/// Start the application with custom configuration
pub fn start_with_desc<P>(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    provider: P,
    desc: AppDesc,
) -> Result<()>
where
    P: LogProvider + 'static,
{
    color_eyre::install().or(Err(anyhow!("Error installing color_eyre")))?;

    let app = App::new(provider, desc.clone());
    app.run(terminal, &desc)
}

struct App {
    is_exiting: bool,
    raw_logs: Vec<LogItem>,
    displaying_logs: LogList,
    log_consumer: ringbuf::HeapCons<LogItem>, // receives logs from provider thread
    provider_thread: Option<thread::JoinHandle<()>>,
    provider_stop_signal: Arc<AtomicBool>,
    autoscroll: bool,
    filter_input: String, // Current filter input text (includes leading '/')
    filter_focused: bool, // Whether the filter input is focused
    filter_engine: FilterEngine, // Filtering engine with incremental + parallel support
    detail_level: LogDetailLevel, // Detail level for log display
    parser: Arc<dyn LogParser>, // Parser for log items (handles both parsing and formatting)
    debug_logs: Arc<Mutex<Vec<String>>>, // Debug log messages for UI display
    hard_focused_block_id: uuid::Uuid, // Hard focus: set by clicking, persists until another click (defaults to logs_block)
    soft_focused_block_id: Option<uuid::Uuid>, // Soft focus: set by hovering, changes with mouse movement
    logs_block: AppBlock,
    details_block: AppBlock,
    debug_block: AppBlock,
    prev_selected_log_id: Option<uuid::Uuid>, // Track previous selected log item ID for details reset
    selected_log_uuid: Option<uuid::Uuid>,    // Track currently selected log item UUID
    last_logs_area: Option<Rect>, // Store the last rendered logs area for selection visibility
    last_details_area: Option<Rect>, // Store the last rendered details area
    last_debug_area: Option<Rect>, // Store the last rendered debug area
    last_logs_viewport_height: Option<usize>, // Track viewport height to preserve bottom item on resize
    text_wrapping_enabled: bool,              // Whether text wrapping is enabled (default false)
    mouse_capture_enabled: bool, // Whether mouse events are captured (disable to allow text selection)
    show_debug_logs: bool,       // Whether to show the debug logs block
    show_help_popup: bool,       // Whether to show the help popup
    display_event: Option<DisplayEvent>, // Temporary event to display in footer
    prev_hard_focused_block_id: uuid::Uuid, // Track previous hard focus to detect changes

    mouse_event: Option<MouseEvent>,
    dragging_scrollbar_block: Option<uuid::Uuid>,
    suppress_mouse_up: bool,
    last_click_time: Option<Instant>,
    last_click_pos: Option<(u16, u16)>,
}

#[derive(Copy, Clone)]
pub(super) enum ScrollableBlockType {
    Details,
    Debug,
}

// ============================================================================
// Initialization
// ============================================================================
impl App {
    fn setup_logger() -> Arc<Mutex<Vec<String>>> {
        let debug_logs = Arc::new(Mutex::new(Vec::new()));
        let logger = Box::new(UiLogger::new(debug_logs.clone()));

        if log::set_logger(Box::leak(logger)).is_ok() {
            log::set_max_level(log::LevelFilter::Debug);
        }

        debug_logs
    }

    fn new<P>(provider: P, desc: AppDesc) -> Self
    where
        P: LogProvider + 'static,
    {
        let debug_logs = Self::setup_logger();

        // create ring buffer
        let ring_buffer = HeapRb::<LogItem>::new(desc.ring_buffer_size);
        let (producer, consumer) = ring_buffer.split();

        // spawn provider thread
        let poll_interval = desc.poll_interval;
        let (provider_thread, provider_stop_signal) =
            spawn_provider_thread(provider, desc.parser.clone(), producer, poll_interval);

        // create blocks first so we can reference their IDs
        let logs_block = AppBlock::new().set_title("[1]─Logs".to_string());
        let details_block = AppBlock::new()
            .set_title("[2]─Details")
            .set_padding(ratatui::widgets::Padding::horizontal(1));
        let debug_block = AppBlock::new()
            .set_title("[3]─Debug Logs")
            .set_padding(ratatui::widgets::Padding::horizontal(1));

        let logs_block_id = logs_block.id();

        // setup filter engine with parser
        let mut filter_engine = FilterEngine::new();
        filter_engine.set_formatter(desc.parser.clone());

        let initial_filter_input = desc
            .initial_filter
            .as_ref()
            .map(|value| value.trim_start_matches('/'))
            .filter(|value| !value.is_empty())
            .map(|value| format!("/{}", value))
            .unwrap_or_default();

        Self {
            is_exiting: false,
            raw_logs: Vec::new(),
            displaying_logs: LogList::new(Vec::new()),
            log_consumer: consumer,
            provider_thread: Some(provider_thread),
            provider_stop_signal,
            autoscroll: true,
            filter_input: initial_filter_input,
            filter_focused: false,
            filter_engine,
            detail_level: 1, // default detail level (was Basic)
            parser: desc.parser,
            debug_logs,
            hard_focused_block_id: logs_block_id,
            soft_focused_block_id: None,
            logs_block,
            details_block,
            debug_block,
            prev_selected_log_id: None,
            selected_log_uuid: None,
            last_logs_area: None,
            last_details_area: None,
            last_debug_area: None,
            last_logs_viewport_height: None,
            text_wrapping_enabled: true,
            mouse_capture_enabled: true,
            show_debug_logs: desc.show_debug_logs,
            show_help_popup: false,
            display_event: None,
            prev_hard_focused_block_id: logs_block_id,

            mouse_event: None,
            dragging_scrollbar_block: None,
            suppress_mouse_up: false,
            last_click_time: None,
            last_click_pos: None,
        }
    }
}

// ============================================================================
// Lifecycle
// ============================================================================
impl App {
    fn run(
        mut self,
        terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
        desc: &AppDesc,
    ) -> Result<()> {
        let poll_interval = desc.poll_interval;
        let event_poll_interval = desc.event_poll_interval;
        let mut last_update_logs = Instant::now();

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| -> Result<()> {
            while !self.is_exiting {
                self.poll_event(event_poll_interval)?;

                if last_update_logs.elapsed() >= poll_interval {
                    self.update_logs()?;
                    last_update_logs = Instant::now();
                }

                self.check_and_clear_expired_event();
                terminal.draw(|frame| frame.render_widget(&mut self, frame.area()))?;
            }
            Ok(())
        }));

        // cleanup provider thread before returning
        self.cleanup();

        match result {
            Ok(r) => r,
            Err(_) => {
                eprintln!("Application panicked, terminal restored");
                std::process::exit(1);
            }
        }
    }

    fn cleanup(&mut self) {
        // signal provider thread to stop
        self.provider_stop_signal.store(true, Ordering::Relaxed);

        // join the provider thread
        if let Some(handle) = self.provider_thread.take() {
            log::debug!("Waiting for provider thread to finish...");
            if let Err(e) = handle.join() {
                log::error!("Provider thread panicked: {:?}", e);
            }
        }
    }

    fn poll_event(&mut self, poll_interval: Duration) -> Result<()> {
        if event::poll(poll_interval)? {
            let event = event::read()?;
            match event {
                Event::Key(key) => self.handle_key(key)?,
                Event::Mouse(mouse) => {
                    if self.mouse_capture_enabled {
                        self.handle_mouse_event(&mouse)?;
                        self.mouse_event = Some(mouse);
                    }
                }
                Event::Resize(width, height) => {
                    log::debug!("Terminal resized to {}x{}", width, height);
                }
                _ => {}
            }
        }

        Ok(())
    }
}

// ============================================================================
// Utility methods
// ============================================================================
impl App {
    fn to_underlying_index(_total: usize, visual_index: usize) -> usize {
        visual_index
    }

    fn to_visual_index(_total: usize, underlying_index: usize) -> usize {
        underlying_index
    }

    fn is_log_block_focused(&self) -> Result<bool> {
        Ok(self.get_display_focused_block() == self.logs_block.id())
    }
}

// ============================================================================
// Log and filter management
// ============================================================================
impl App {
    fn update_logs(&mut self) -> Result<()> {
        // consume all available logs from ring buffer
        let mut new_logs = Vec::new();
        while let Some(log) = self.log_consumer.try_pop() {
            new_logs.push(log);
        }

        if new_logs.is_empty() {
            return Ok(());
        }

        let previous_uuid = self.selected_log_uuid;
        let previous_scroll_pos = Some(self.logs_block.get_scroll_position());

        log::debug!("Received {} new log items from provider", new_logs.len());
        let old_raw_count = self.raw_logs.len();
        self.raw_logs.extend(new_logs);

        // use incremental filtering for efficiency (only filters new logs)
        let filter_query = self.get_filter_query().to_string();
        let filtered_indices = self.filter_engine.filter_new_logs(
            &self.raw_logs,
            old_raw_count,
            &filter_query,
            self.detail_level,
        );
        self.displaying_logs = LogList::new(filtered_indices);

        if previous_uuid.is_some() {
            self.update_selection_by_uuid();
        }

        {
            let new_items_count = self.displaying_logs.len();

            if self.autoscroll {
                // scroll to bottom (stop when last item is fully displayed)
                let viewport_height = if let Some(area) = self.last_logs_area {
                    let is_focused = self.is_log_block_focused().unwrap_or(false);
                    let [main_content_area, _] =
                        Layout::horizontal([Constraint::Fill(1), Constraint::Length(1)])
                            .margin(0)
                            .areas(area);

                    let [content_area, _] =
                        Layout::vertical([Constraint::Fill(1), Constraint::Length(1)])
                            .margin(0)
                            .areas(main_content_area);

                    let inner_area = self.logs_block.get_content_rect(content_area, is_focused);
                    inner_area.height as usize
                } else {
                    1 // fallback if area not yet rendered
                };

                let max_scroll = new_items_count.saturating_sub(viewport_height);
                self.logs_block.set_scroll_position(max_scroll);
            } else if previous_scroll_pos.is_some() {
                // oldest is at visual index 0, newest at end;
                // adding items doesn't change visual position of existing items,
                // so scroll position stays the same
                // (scroll position is already set correctly, no adjustment needed)
            }

            self.logs_block.set_lines_count(new_items_count);
            self.logs_block.update_scrollbar_state(
                new_items_count,
                Some(self.logs_block.get_scroll_position()),
            );
        }

        Ok(())
    }

    fn get_filter_query(&self) -> &str {
        // filter_input includes the leading '/', so skip it
        if self.filter_input.starts_with('/') && self.filter_input.len() > 1 {
            &self.filter_input[1..]
        } else {
            ""
        }
    }

    fn apply_filter(&mut self) {
        let previous_uuid = self.selected_log_uuid;
        let prev_scroll_pos = self.logs_block.get_scroll_position();

        // calculate the relative position of the selected item in the viewport
        let prev_relative_offset = if let Some(selected_idx) = self.displaying_logs.state.selected()
        {
            selected_idx.saturating_sub(prev_scroll_pos)
        } else {
            0
        };

        self.rebuild_filtered_list();

        if previous_uuid.is_some() {
            self.update_selection_by_uuid();

            // if the previously selected item is no longer in the filtered list,
            // clear selection
            if self.selected_log_uuid.is_none() {
                self.displaying_logs.state.select(None);
            }
        }

        {
            let new_total = self.displaying_logs.len();
            let mut pos = prev_scroll_pos;
            if new_total == 0 {
                pos = 0;
            } else {
                // try to preserve the relative screen position of the selected item
                if let Some(selected_idx) = self.displaying_logs.state.selected() {
                    // calculate desired scroll position to maintain relative offset
                    let desired_scroll = selected_idx.saturating_sub(prev_relative_offset);
                    // clamp to valid range
                    pos = desired_scroll.min(new_total.saturating_sub(1));
                } else {
                    // fallback to previous scroll position
                    pos = pos.min(new_total.saturating_sub(1));
                }
            }
            self.logs_block.set_scroll_position(pos);
            self.logs_block.set_lines_count(new_total);
            self.logs_block.update_scrollbar_state(new_total, Some(pos));
        }

        // ensure the selected item is scrolled into view after filter changes
        let _ = self.ensure_selection_visible();
    }

    fn rebuild_filtered_list(&mut self) {
        let filter_query = self.get_filter_query().to_string();

        // use FilterEngine for filtering (incremental + parallel)
        let filtered_indices =
            self.filter_engine
                .filter(&self.raw_logs, &filter_query, self.detail_level);

        self.displaying_logs = LogList::new(filtered_indices);
    }

    fn update_logs_scrollbar_state(&mut self) {
        let total = self.displaying_logs.len();

        {
            let max_top = total.saturating_sub(1);
            let pos = self.logs_block.get_scroll_position().min(max_top);
            self.logs_block.set_scroll_position(pos);

            self.logs_block.set_lines_count(total);
            self.logs_block.update_scrollbar_state(total, Some(pos));
        }
    }
}

// ============================================================================
// Focus management
// ============================================================================
impl App {
    fn set_hard_focused_block(&mut self, block_id: uuid::Uuid) {
        self.hard_focused_block_id = block_id;
    }

    fn set_soft_focused_block(&mut self, block_id: uuid::Uuid) {
        if self.soft_focused_block_id != Some(block_id) {
            self.soft_focused_block_id = Some(block_id);
        }
    }

    fn set_mouse_capture(&mut self, enable: bool) -> Result<()> {
        if self.mouse_capture_enabled == enable {
            return Ok(());
        }

        let mut stdout = io::stdout();
        if enable {
            execute!(stdout, EnableMouseCapture)?;
        } else {
            execute!(stdout, DisableMouseCapture)?;
            self.mouse_event = None;
            self.dragging_scrollbar_block = None;
            self.suppress_mouse_up = false;
        }

        self.mouse_capture_enabled = enable;
        Ok(())
    }

    fn get_display_focused_block(&self) -> uuid::Uuid {
        self.hard_focused_block_id
    }

    fn is_mouse_in_area(&self, mouse: &MouseEvent, area: Rect) -> bool {
        mouse.column >= area.x
            && mouse.column < area.x + area.width
            && mouse.row >= area.y
            && mouse.row < area.y + area.height
    }

    fn get_block_under_mouse(&self, mouse: &MouseEvent) -> Option<uuid::Uuid> {
        if let Some(area) = self.last_logs_area
            && self.is_mouse_in_area(mouse, area)
        {
            return Some(self.logs_block.id());
        }

        if let Some(area) = self.last_details_area
            && self.is_mouse_in_area(mouse, area)
        {
            return Some(self.details_block.id());
        }

        if let Some(area) = self.last_debug_area
            && self.is_mouse_in_area(mouse, area)
        {
            return Some(self.debug_block.id());
        }

        None
    }

    fn get_vertical_scrollbar_area(&self, block_id: uuid::Uuid) -> Option<Rect> {
        let area = if block_id == self.logs_block.id() {
            self.last_logs_area
        } else if block_id == self.details_block.id() {
            self.last_details_area
        } else {
            None
        }?;

        let [_content_area, scrollbar_area] =
            Layout::horizontal([Constraint::Fill(1), Constraint::Length(1)])
                .margin(0)
                .areas(area);

        Some(scrollbar_area)
    }

    fn get_scrollbar_block_under_mouse(&self, mouse: &MouseEvent) -> Option<uuid::Uuid> {
        if let Some(area) = self.get_vertical_scrollbar_area(self.logs_block.id())
            && self.is_mouse_in_area(mouse, area)
        {
            return Some(self.logs_block.id());
        }

        if let Some(area) = self.get_vertical_scrollbar_area(self.details_block.id())
            && self.is_mouse_in_area(mouse, area)
        {
            return Some(self.details_block.id());
        }

        None
    }
}

// ============================================================================
// Display events
// ============================================================================
impl App {
    /// Set a display event to show in the footer for a given duration
    pub fn set_display_event(&mut self, text: String, duration: Duration, style: Option<Style>) {
        self.display_event = Some(DisplayEvent::create(
            text,
            duration,
            style,
            theme::DISPLAY_EVENT_STYLE,
        ));
    }

    /// Check if the current display event has expired and clear it if so
    fn check_and_clear_expired_event(&mut self) {
        self.display_event = DisplayEvent::check_and_clear(self.display_event.take());
    }

    fn clear_event(&mut self) {
        self.mouse_event = None;
        self.suppress_mouse_up = false;
    }
}

// ============================================================================
// Widget implementation
// ============================================================================
impl Widget for &mut App {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // detect if hard focus changed since last render
        let focus_changed = self.hard_focused_block_id != self.prev_hard_focused_block_id;

        let (main_area, debug_area, footer_area) = if self.show_debug_logs {
            let [main, debug_area, footer_area] = Layout::vertical([
                Constraint::Fill(1),
                Constraint::Length(6),
                Constraint::Length(1),
            ])
            .areas(area);
            (main, Some(debug_area), footer_area)
        } else {
            let [main, footer_area] =
                Layout::vertical([Constraint::Fill(1), Constraint::Length(1)]).areas(area);
            (main, None, footer_area)
        };

        // If the smaller details panel is at least 8 lines tall, keep the logs panel larger.
        let [_, smaller_details_area] =
            Layout::vertical([Constraint::Percentage(60), Constraint::Percentage(40)])
                .areas(main_area);
        let ratio_switch_enabled = smaller_details_area.height < 8;

        let (logs_percentage, details_percentage) = if !ratio_switch_enabled {
            (60, 40)
        } else if self.hard_focused_block_id == self.details_block.id() {
            (40, 60)
        } else {
            (60, 40) // default for logs block or any other case
        };

        let [logs_area, details_area] = Layout::vertical([
            Constraint::Percentage(logs_percentage),
            Constraint::Percentage(details_percentage),
        ])
        .areas(main_area);

        self.render_logs(logs_area, buf).unwrap();
        self.render_details(details_area, buf).unwrap();
        if let Some(debug_area) = debug_area {
            self.render_debug_logs(debug_area, buf).unwrap();
        }
        self.render_footer(footer_area, buf).unwrap();

        // render help popup on top if visible
        if self.show_help_popup {
            self.render_help_popup(area, buf).unwrap();
        }

        // adjust viewport if hard focus changed (panels resized)
        if focus_changed {
            log::debug!("Hard focus changed, adjusting viewport");
            let _ = self.ensure_selection_visible();
            self.prev_hard_focused_block_id = self.hard_focused_block_id;
        }

        self.clear_event();
    }
}
