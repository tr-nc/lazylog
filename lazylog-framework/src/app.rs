use crate::{
    app_block::AppBlock,
    content_line_maker::{WrappingMode, calculate_content_width, content_into_lines},
    filter::FilterEngine,
    log_list::LogList,
    log_parser::{LogDetailLevel, LogItem},
    provider::{LogProvider, spawn_provider_thread},
    status_bar::{DisplayEvent, StatusBar},
    theme,
    ui_logger::UiLogger,
};
use anyhow::{Result, anyhow};
use arboard::Clipboard;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, MouseEvent, MouseEventKind};
use ratatui::{
    Terminal,
    backend::CrosstermBackend,
    prelude::*,
    widgets::{Padding, Paragraph, StatefulWidget, Widget},
};
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
    time::Duration,
};

// constants
const DEFAULT_POLL_INTERVAL_MS: u64 = 100;
const DEFAULT_RING_BUFFER_SIZE: usize = 16384;
const HELP_POPUP_WIDTH: u16 = 60;
const SCROLL_PAD: usize = 1;
const HORIZONTAL_SCROLL_STEP: usize = 5;
const DISPLAY_EVENT_DURATION_MS: u64 = 800;

#[derive(Clone)]
pub struct AppDesc {
    pub poll_interval: Duration,
    pub show_debug_logs: bool,
    pub ring_buffer_size: usize,
}

impl Default for AppDesc {
    fn default() -> Self {
        Self {
            poll_interval: Duration::from_millis(DEFAULT_POLL_INTERVAL_MS),
            show_debug_logs: false,
            ring_buffer_size: DEFAULT_RING_BUFFER_SIZE,
        }
    }
}

/// Start the application with default configuration
pub fn start_with_provider<P>(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    provider: P,
) -> Result<()>
where
    P: LogProvider + 'static,
{
    start_with_desc(terminal, provider, AppDesc::default())
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
    text_wrapping_enabled: bool,  // Whether text wrapping is enabled (default false)
    show_debug_logs: bool,        // Whether to show the debug logs block
    show_help_popup: bool,        // Whether to show the help popup
    display_event: Option<DisplayEvent>, // Temporary event to display in footer
    prev_hard_focused_block_id: uuid::Uuid, // Track previous hard focus to detect changes

    mouse_event: Option<MouseEvent>,
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
            spawn_provider_thread(provider, producer, poll_interval);

        // create blocks first so we can reference their IDs
        let logs_block = AppBlock::new().set_title("[1]─Logs".to_string());
        let details_block = AppBlock::new()
            .set_title("[2]─Details")
            .set_padding(Padding::horizontal(1));
        let debug_block = AppBlock::new()
            .set_title("[3]─Debug Logs")
            .set_padding(Padding::horizontal(1));

        let logs_block_id = logs_block.id();

        Self {
            is_exiting: false,
            raw_logs: Vec::new(),
            displaying_logs: LogList::new(Vec::new()),
            log_consumer: consumer,
            provider_thread: Some(provider_thread),
            provider_stop_signal,
            autoscroll: true,
            filter_input: String::new(),
            filter_focused: false,
            filter_engine: FilterEngine::new(),
            detail_level: LogDetailLevel::default(),
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
            text_wrapping_enabled: true,
            show_debug_logs: desc.show_debug_logs,
            show_help_popup: false,
            display_event: None,
            prev_hard_focused_block_id: logs_block_id,

            mouse_event: None,
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

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| -> Result<()> {
            while !self.is_exiting {
                self.poll_event(poll_interval)?;
                self.update_logs()?;
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
                    self.handle_mouse_event(&mouse)?;
                    self.mouse_event = Some(mouse);
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
    fn to_underlying_index(total: usize, visual_index: usize) -> usize {
        total.saturating_sub(1).saturating_sub(visual_index)
    }

    fn to_visual_index(total: usize, underlying_index: usize) -> usize {
        total.saturating_sub(1).saturating_sub(underlying_index)
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

        let old_items_count = self.displaying_logs.len();
        let previous_uuid = self.selected_log_uuid;
        let previous_scroll_pos = Some(self.logs_block.get_scroll_position());

        log::debug!("Received {} new log items from provider", new_logs.len());
        self.raw_logs.extend(new_logs);

        // rebuild filtered list using FilterEngine
        self.rebuild_filtered_list();

        if previous_uuid.is_some() {
            self.update_selection_by_uuid();
        } else if self.autoscroll {
            self.displaying_logs.select_first();
            self.update_selected_uuid();
        }

        {
            let new_items_count = self.displaying_logs.len();
            let items_added = new_items_count.saturating_sub(old_items_count);

            if self.autoscroll {
                self.logs_block.set_scroll_position(0);
            } else if let Some(prev) = previous_scroll_pos {
                // newest is at visual index 0, adding items pushes existing content down;
                // keep the same lines visible by shifting the top by items_added
                let new_scroll_pos = prev.saturating_add(items_added);
                let max_top = new_items_count.saturating_sub(1);
                self.logs_block
                    .set_scroll_position(new_scroll_pos.min(max_top));
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
            // select the first available item
            if self.selected_log_uuid.is_none() && !self.displaying_logs.is_empty() {
                self.displaying_logs.select_first();
                self.update_selected_uuid();
            }
        } else if self.autoscroll {
            self.displaying_logs.select_first();
            self.update_selected_uuid();
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
// Rendering
// ============================================================================

#[derive(Copy, Clone)]
enum ScrollableBlockType {
    Details,
    Debug,
}

impl App {
    fn render_footer(&self, area: Rect, buf: &mut Buffer) -> Result<()> {
        // determine middle text (help, filter, or display event)
        let (mid_text, custom_style) = if let Some(event) = &self.display_event {
            (event.text.clone(), Some(event.style))
        } else if !self.filter_input.is_empty() {
            (self.filter_input.clone(), None)
        } else {
            ("?: help | q: quit".to_string(), None)
        };

        // build left side status (wrap mode)
        let left_text = if self.display_event.is_none() && self.filter_input.is_empty() {
            if self.text_wrapping_enabled {
                "wrap on".to_string()
            } else {
                "wrap off".to_string()
            }
        } else {
            String::new()
        };

        // build right side status (version)
        let right_text = if self.display_event.is_none() && self.filter_input.is_empty() {
            format!("v{}", env!("CARGO_PKG_VERSION"))
        } else {
            String::new()
        };

        // create and render status bar
        let mut status_bar = StatusBar::new()
            .set_left(left_text)
            .set_mid(mid_text)
            .set_right(right_text)
            .set_left_fg(theme::select_color_with_default_palette(
                theme::PaletteIdx::C600,
            ))
            .set_right_fg(theme::select_color_with_default_palette(
                theme::PaletteIdx::C600,
            ));

        if let Some(style) = custom_style {
            status_bar = status_bar.set_style(style);
        } else if self.filter_focused {
            status_bar = status_bar.set_style(Style::default().bg(
                theme::select_color_with_default_palette(theme::PaletteIdx::C500),
            ));
        }

        status_bar.render(area, buf);
        Ok(())
    }

    fn render_help_popup(&self, area: Rect, buf: &mut Buffer) -> Result<()> {
        use ratatui::widgets::{Block, Borders, Clear};

        let help_text = vec![
            Line::from("Navigation:".bold()),
            Line::from("  j/k/↑/↓  - Move to prev/next log"),
            Line::from("  g        - Jump to top"),
            Line::from("  space    - Make selected log visible in view"),
            Line::from(""),
            Line::from("Actions:".bold()),
            Line::from("  /        - Enter filter mode"),
            Line::from("  y        - Copy current log to clipboard"),
            Line::from("  c        - Clear all logs"),
            Line::from("  w        - Toggle text wrapping"),
            Line::from("  [        - Decrease detail level"),
            Line::from("  ]        - Increase detail level"),
            Line::from("  Esc      - Go back / clear filter"),
            Line::from("  q        - Quit program"),
            Line::from(""),
            Line::from("Focus:".bold()),
            Line::from("  <num_key>    - Toggle focus on panel"),
            Line::from("  Shift+scroll - Horizontal scroll with mouse"),
        ];

        // calculate popup height: content lines + 2 for borders
        let popup_height = help_text.len() as u16 + 2;

        // center the popup
        let popup_area = Layout::vertical([
            Constraint::Fill(1),
            Constraint::Length(popup_height),
            Constraint::Fill(1),
        ])
        .split(area)[1];

        let popup_area = Layout::horizontal([
            Constraint::Fill(1),
            Constraint::Length(HELP_POPUP_WIDTH),
            Constraint::Fill(1),
        ])
        .split(popup_area)[1];

        // clear the area first
        Clear.render(popup_area, buf);

        let block = Block::default()
            .title("Help")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme::TEXT_FG_COLOR));

        Paragraph::new(help_text)
            .block(block)
            .fg(theme::TEXT_FG_COLOR)
            .render(popup_area, buf);

        Ok(())
    }

    /// Common rendering logic for scrollable blocks (details and debug logs)
    fn render_scrollable_block(
        &mut self,
        area: Rect,
        buf: &mut Buffer,
        block_type: ScrollableBlockType,
        content: Vec<Line>,
        max_content_width: usize,
    ) -> Result<()> {
        // store last area based on block type
        match block_type {
            ScrollableBlockType::Details => self.last_details_area = Some(area),
            ScrollableBlockType::Debug => self.last_debug_area = Some(area),
        }

        // get block ID and check if focused
        let block_id = match block_type {
            ScrollableBlockType::Details => self.details_block.id(),
            ScrollableBlockType::Debug => self.debug_block.id(),
        };
        let is_focused = self.get_display_focused_block() == block_id;

        // handle mouse events for hard/soft focus
        let should_hard_focus = if let Some(event) = self.mouse_event {
            let is_left_click = event.kind
                == crossterm::event::MouseEventKind::Up(crossterm::event::MouseButton::Left);

            // get block for checking bounds
            let block_ref = match block_type {
                ScrollableBlockType::Details => &self.details_block,
                ScrollableBlockType::Debug => &self.debug_block,
            };
            let inner_area = block_ref.build(false).inner(area);
            let is_within_bounds =
                inner_area.contains(ratatui::layout::Position::new(event.column, event.row));

            if event.kind == crossterm::event::MouseEventKind::Moved && is_within_bounds {
                self.set_soft_focused_block(block_id);
            }

            is_left_click && is_within_bounds
        } else {
            false
        };

        if should_hard_focus {
            self.set_hard_focused_block(block_id);
        }

        // create horizontal layout for content and vertical scrollbar
        let [vertical_content_area, scrollbar_area] = Layout::horizontal([
            Constraint::Fill(1),   // main content takes most space
            Constraint::Length(1), // scrollbar is 1 character wide
        ])
        .margin(0)
        .areas(area);

        let lines_count = content.len();

        // determine if horizontal scrollbar is needed
        let block_ref = match block_type {
            ScrollableBlockType::Details => &self.details_block,
            ScrollableBlockType::Debug => &self.debug_block,
        };
        let temp_content_rect = block_ref.get_content_rect(vertical_content_area, is_focused);
        let needs_horizontal_scrollbar = max_content_width > temp_content_rect.width as usize;

        // create vertical layout for content and horizontal scrollbar
        let [content_area, horizontal_scrollbar_area] = Layout::vertical([
            Constraint::Fill(1),   // main content
            Constraint::Length(1), // horizontal scrollbar height
        ])
        .margin(0)
        .areas(vertical_content_area);

        // get mutable reference to update state
        let block = match block_type {
            ScrollableBlockType::Details => &mut self.details_block,
            ScrollableBlockType::Debug => &mut self.debug_block,
        };

        let content_rect = block.get_content_rect(content_area, is_focused);
        block.update_horizontal_scrollbar_state(max_content_width, content_rect.width as usize);
        block.set_lines_count(lines_count);
        let scroll_position = block.get_scroll_position();
        block.update_scrollbar_state(lines_count, Some(scroll_position));

        let h_scroll = if needs_horizontal_scrollbar {
            block.get_horizontal_scroll_position() as u16
        } else {
            0
        };

        let block_widget = block.build(is_focused);

        // render paragraph with scrolling
        Paragraph::new(content)
            .block(block_widget)
            .fg(theme::TEXT_FG_COLOR)
            .scroll((scroll_position as u16, h_scroll))
            .render(content_area, buf);

        // render vertical scrollbar
        let scrollbar = AppBlock::create_scrollbar(is_focused);
        let block_ref = match block_type {
            ScrollableBlockType::Details => &mut self.details_block,
            ScrollableBlockType::Debug => &mut self.debug_block,
        };
        StatefulWidget::render(
            scrollbar,
            scrollbar_area,
            buf,
            block_ref.get_scrollbar_state(),
        );

        // render horizontal scrollbar (track-only when not needed)
        let horizontal_scrollbar = if needs_horizontal_scrollbar {
            AppBlock::create_horizontal_scrollbar(is_focused)
        } else {
            AppBlock::create_horizontal_track_only(is_focused)
        };
        StatefulWidget::render(
            horizontal_scrollbar,
            horizontal_scrollbar_area,
            buf,
            block_ref.get_horizontal_scrollbar_state(),
        );

        Ok(())
    }

    fn render_logs(&mut self, area: Rect, buf: &mut Buffer) -> Result<()> {
        self.last_logs_area = Some(area);

        let [content_area, scrollbar_area] = Layout::horizontal([
            Constraint::Fill(1),   // Main content takes most space
            Constraint::Length(1), // Scrollbar is 1 character wide
        ])
        .margin(0)
        .areas(area);

        let is_log_focused = self.is_log_block_focused().unwrap_or(false);

        let filter_query = self.get_filter_query();
        let mut title = if filter_query.is_empty() {
            format!("[1]─Logs - {}", self.raw_logs.len())
        } else {
            format!(
                "[1]─Logs - {} of {}",
                self.displaying_logs.len(),
                self.raw_logs.len()
            )
        };

        self.update_autoscroll_state();

        if self.autoscroll {
            title += " - Autoscrolling";
        }
        self.logs_block.update_title(title);
        let logs_block_id = self.logs_block.id();

        let selected_index = self.displaying_logs.state.selected();
        let total_lines = self.displaying_logs.len();

        // Calculate content first to determine if horizontal scrollbar is needed
        let temp_inner_area = self
            .logs_block
            .get_content_rect(content_area, is_log_focused);
        let viewport_width = temp_inner_area.width as usize;

        // Since we're using truncated mode, content will never exceed the viewport width
        let max_content_width = viewport_width;

        // Always allocate space for horizontal scrollbar (consistent layout)
        let [main_content_area, horizontal_scrollbar_area] = Layout::vertical([
            Constraint::Fill(1),   // Main content
            Constraint::Length(1), // Horizontal scrollbar height
        ])
        .margin(0)
        .areas(content_area);

        let (should_hard_focus, clicked_row) = if let Some(event) = self.mouse_event {
            let is_left_click = event.kind
                == crossterm::event::MouseEventKind::Up(crossterm::event::MouseButton::Left);
            let inner_area = self.logs_block.build(false).inner(main_content_area);
            let is_within_bounds =
                inner_area.contains(ratatui::layout::Position::new(event.column, event.row));

            let should_hard_focus = is_left_click && is_within_bounds;
            let click_row = if should_hard_focus {
                Some(event.row)
            } else {
                None
            };

            if event.kind == crossterm::event::MouseEventKind::Moved && is_within_bounds {
                self.set_soft_focused_block(logs_block_id);
            }

            (should_hard_focus, click_row)
        } else {
            (false, None)
        };

        if should_hard_focus {
            self.set_hard_focused_block(logs_block_id);
        }

        let inner_area = self
            .logs_block
            .get_content_rect(main_content_area, is_log_focused);
        let visible_height = inner_area.height as usize;
        let content_width = inner_area.width as usize;

        let logs_block = &mut self.logs_block;
        let mut scroll_position = logs_block.get_scroll_position();
        let max_top = total_lines.saturating_sub(1);
        if total_lines == 0 {
            scroll_position = 0;
            logs_block.set_scroll_position(0);
        } else if scroll_position > max_top {
            scroll_position = max_top;
            logs_block.set_scroll_position(scroll_position);
        }

        let mut selection_changed = false;
        if let Some(click_row) = clicked_row {
            let relative_row = click_row.saturating_sub(inner_area.y);
            let exact_item_number = scroll_position.saturating_add(relative_row as usize);
            if exact_item_number < total_lines {
                self.displaying_logs.state.select(Some(exact_item_number));
                selection_changed = true;
            }
        }

        let end = (scroll_position + visible_height).min(total_lines);
        let start = scroll_position.min(end);

        let mut content_lines = Vec::with_capacity(end.saturating_sub(start));

        for i in start..end {
            let item_idx = total_lines.saturating_sub(1).saturating_sub(i);
            // get the index into raw_logs from displaying_logs
            let raw_idx = self.displaying_logs.get(item_idx).unwrap();
            let log_item = &self.raw_logs[raw_idx];

            let detail_text = log_item.get_preview_text(self.detail_level);
            let level_style = match log_item.level.as_str() {
                "ERROR" => theme::ERROR_STYLE,
                "WARNING" => theme::WARN_STYLE,
                "SYSTEM" => theme::INFO_STYLE,
                _ => Style::default().fg(theme::TEXT_FG_COLOR),
            };

            let is_selected = selected_index == Some(i);
            let display_text = if is_selected {
                format!(" → {}", detail_text)
            } else {
                format!("   {}", detail_text)
            };

            let final_style = if is_selected {
                level_style.patch(theme::SELECTED_STYLE)
            } else {
                level_style
            };

            // Use content_into_lines with Truncated mode to prevent overflow
            let truncated_lines =
                content_into_lines(&display_text, content_width as u16, WrappingMode::Truncated);

            // Since truncated mode returns exactly one line, we can safely get the first
            let truncated_line = truncated_lines
                .into_iter()
                .next()
                .unwrap_or_else(|| Line::from(""));

            let padded_text = if is_selected {
                format!(
                    "{:<width$}",
                    truncated_line.to_string(),
                    width = content_width
                )
            } else {
                truncated_line.to_string()
            };

            content_lines.push(Line::styled(padded_text, final_style));
        }

        // Update horizontal scrollbar state
        logs_block.update_horizontal_scrollbar_state(max_content_width, content_width);

        let logs_block = &mut self.logs_block;
        logs_block.set_lines_count(total_lines);
        logs_block.update_scrollbar_state(total_lines, Some(scroll_position));

        let block = self.logs_block.build(is_log_focused);

        // Determine if horizontal scrollbar is needed
        let needs_horizontal_scrollbar = max_content_width > viewport_width;
        let h_scroll = if needs_horizontal_scrollbar {
            self.logs_block.get_horizontal_scroll_position() as u16
        } else {
            0
        };

        Paragraph::new(content_lines)
            .block(block)
            .fg(theme::TEXT_FG_COLOR)
            .scroll((0, h_scroll))
            .render(main_content_area, buf);

        let scrollbar = AppBlock::create_scrollbar(is_log_focused);
        let logs_block = &mut self.logs_block;
        StatefulWidget::render(
            scrollbar,
            scrollbar_area,
            buf,
            logs_block.get_scrollbar_state(),
        );

        // Always render horizontal scrollbar area (track-only when not needed)
        let horizontal_scrollbar = if needs_horizontal_scrollbar {
            AppBlock::create_horizontal_scrollbar(is_log_focused)
        } else {
            AppBlock::create_horizontal_track_only(is_log_focused)
        };
        StatefulWidget::render(
            horizontal_scrollbar,
            horizontal_scrollbar_area,
            buf,
            logs_block.get_horizontal_scrollbar_state(),
        );

        if selection_changed {
            self.update_selected_uuid();
            // Note: ensure_selection_visible and update_logs_scrollbar_state not needed here
            // because the clicked item is already visible and scrollbar state was updated above
            self.autoscroll = false;
        }

        Ok(())
    }

    fn render_details(&mut self, area: Rect, buf: &mut Buffer) -> Result<()> {
        // handle prev_selected_log_id state management and scroll reset
        let (indices, state) = (&self.displaying_logs.indices, &self.displaying_logs.state);

        if let Some(i) = state.selected() {
            let reversed_index = indices.len().saturating_sub(1).saturating_sub(i);
            let raw_idx = indices[reversed_index];
            let item = &self.raw_logs[raw_idx];

            if self.prev_selected_log_id != Some(item.id) {
                self.prev_selected_log_id = Some(item.id);
                self.details_block.set_scroll_position(0);
                self.details_block.set_horizontal_scroll_position(0);
            }
        } else if self.prev_selected_log_id.is_some() {
            self.prev_selected_log_id = None;
            self.details_block.set_scroll_position(0);
            self.details_block.set_horizontal_scroll_position(0);
            log::debug!("No log item selected - resetting details scroll position");
        }

        // clone the data we need to avoid borrow checker issues
        let selected_item = {
            let (indices, state) = (&self.displaying_logs.indices, &self.displaying_logs.state);

            if let Some(i) = state.selected() {
                let reversed_index = indices.len().saturating_sub(1).saturating_sub(i);
                let raw_idx = indices[reversed_index];
                let item = &self.raw_logs[raw_idx];

                Some((
                    item.time.clone(),
                    item.level.clone(),
                    item.origin.clone(),
                    item.tag.clone(),
                    item.content.clone(),
                ))
            } else {
                None
            }
        };

        let text_wrapping_enabled = self.text_wrapping_enabled;

        // generate content using the cloned data
        let (content, max_content_width) = if let Some((time, level, origin, tag, item_content)) =
            &selected_item
        {
            let mut content_lines = vec![
                Line::from(vec!["Time:   ".bold(), time.clone().into()]),
                Line::from(vec!["Level:  ".bold(), level.clone().into()]),
                Line::from(vec!["Origin: ".bold(), origin.clone().into()]),
                Line::from(vec!["Tag:    ".bold(), tag.clone().into()]),
                Line::from("Content:".bold()),
            ];

            // calculate temp_content_rect to determine wrapping width
            let [vertical_content_area, _] =
                Layout::horizontal([Constraint::Fill(1), Constraint::Length(1)])
                    .margin(0)
                    .areas(area);

            let [content_area, _] = Layout::vertical([Constraint::Fill(1), Constraint::Length(1)])
                .margin(0)
                .areas(vertical_content_area);

            let is_focused = self.get_display_focused_block() == self.details_block.id();
            let temp_content_rect = self
                .details_block
                .get_content_rect(content_area, is_focused);

            let wrapping_mode = if text_wrapping_enabled {
                WrappingMode::Wrapped
            } else {
                WrappingMode::Unwrapped
            };
            content_lines.extend(content_into_lines(
                item_content,
                temp_content_rect.width,
                wrapping_mode,
            ));

            // calculate max content width for horizontal scrolling
            let max_content_width = if text_wrapping_enabled {
                temp_content_rect.width as usize
            } else {
                let header_width = ["Time:   ", "Level:  ", "Origin: ", "Tag:    ", "Content:"]
                    .iter()
                    .map(|h| h.len())
                    .max()
                    .unwrap_or(0);
                let item_content_width = calculate_content_width(item_content);
                header_width.max(item_content_width)
            };

            (content_lines, max_content_width)
        } else {
            (
                vec![Line::from("Select a log item to see details...".italic())],
                0,
            )
        };

        // use helper to render
        self.render_scrollable_block(
            area,
            buf,
            ScrollableBlockType::Details,
            content,
            max_content_width,
        )
    }

    fn render_debug_logs(&mut self, area: Rect, buf: &mut Buffer) -> Result<()> {
        // generate content for the debug logs block
        let debug_logs_lines = if let Ok(logs) = self.debug_logs.lock() {
            if logs.is_empty() {
                vec![Line::from("No debug logs...".italic())]
            } else {
                logs.iter()
                    .rev() // show most recent first
                    .map(|log_entry| {
                        let style = if log_entry.contains("ERROR") {
                            theme::ERROR_STYLE
                        } else if log_entry.contains("WARNING") {
                            theme::WARN_STYLE
                        } else if log_entry.contains("DEBUG") {
                            theme::DEBUG_STYLE
                        } else {
                            theme::INFO_STYLE
                        };
                        Line::styled(log_entry.clone(), style)
                    })
                    .collect()
            }
        } else {
            vec![Line::from("Failed to read debug logs...".italic())]
        };

        // calculate max content width for horizontal scrolling
        let max_content_width = if let Ok(logs) = self.debug_logs.lock() {
            logs.iter()
                .map(|log_entry| log_entry.chars().count())
                .max()
                .unwrap_or(0)
        } else {
            0
        };

        // use helper to render
        self.render_scrollable_block(
            area,
            buf,
            ScrollableBlockType::Debug,
            debug_logs_lines,
            max_content_width,
        )
    }
}

// ============================================================================
// Selection and state management
// ============================================================================
impl App {
    fn ensure_selection_visible(&mut self) -> Result<()> {
        let selected_index = self.displaying_logs.state.selected();

        if let (Some(selected_idx), Some(visible_area)) = (selected_index, self.last_logs_area) {
            {
                let current_scroll_pos = self.logs_block.get_scroll_position();

                // Calculate the main content area (excluding scrollbars)
                let [content_area, _scrollbar_area] =
                    Layout::horizontal([Constraint::Fill(1), Constraint::Length(1)])
                        .margin(0)
                        .areas(visible_area);

                let [main_content_area, _horizontal_scrollbar_area] =
                    Layout::vertical([Constraint::Fill(1), Constraint::Length(1)])
                        .margin(0)
                        .areas(content_area);

                let content_rect = self.logs_block.get_content_rect(main_content_area, false);
                let visible_height = content_rect.height as usize;

                if visible_height == 0 {
                    return Ok(());
                }

                let pad = if visible_height > 2 { SCROLL_PAD } else { 0 };

                let view_start = current_scroll_pos;
                let view_end = current_scroll_pos + visible_height.saturating_sub(1);

                let mut new_scroll_pos = if selected_idx < view_start.saturating_add(pad) {
                    selected_idx.saturating_sub(pad)
                } else if selected_idx > view_end.saturating_sub(pad) {
                    selected_idx
                        .saturating_add(pad)
                        .saturating_add(1)
                        .saturating_sub(visible_height)
                } else {
                    current_scroll_pos
                };

                let total_items = self.displaying_logs.len();
                let max_top = total_items.saturating_sub(1);
                new_scroll_pos = new_scroll_pos.min(max_top);

                if new_scroll_pos != current_scroll_pos {
                    self.logs_block.set_scroll_position(new_scroll_pos);
                    self.logs_block
                        .update_scrollbar_state(total_items, Some(new_scroll_pos));
                }
            }
        }
        Ok(())
    }

    fn update_autoscroll_state(&mut self) {
        self.autoscroll = self.logs_block.get_scroll_position() == 0;
    }

    /// Update the UI after manually changing selection
    /// This ensures the selection is visible, disables autoscroll, and updates scrollbar
    fn after_selection_change(&mut self) -> Result<()> {
        self.ensure_selection_visible()?;
        self.autoscroll = false;
        self.update_logs_scrollbar_state();
        Ok(())
    }

    /// Find the index of a log item by its UUID
    fn find_log_by_uuid(&self, uuid: &uuid::Uuid) -> Option<usize> {
        self.displaying_logs
            .indices
            .iter()
            .position(|&raw_idx| &self.raw_logs[raw_idx].id == uuid)
    }

    /// Update the selection based on the currently tracked UUID
    fn update_selection_by_uuid(&mut self) {
        let Some(uuid) = self.selected_log_uuid else {
            return;
        };

        let Some(underlying_index) = self.find_log_by_uuid(&uuid) else {
            self.displaying_logs.state.select(None);
            self.selected_log_uuid = None;
            return;
        };

        let total = self.displaying_logs.len();
        if total > 0 {
            let visual_index = App::to_visual_index(total, underlying_index);
            self.displaying_logs.state.select(Some(visual_index));
        } else {
            self.displaying_logs.state.select(None);
        }
    }

    /// Update the tracked UUID when selection changes
    fn update_selected_uuid(&mut self) {
        let Some(visual_index) = self.displaying_logs.state.selected() else {
            self.selected_log_uuid = None;
            return;
        };

        let total = self.displaying_logs.len();
        if total == 0 {
            self.selected_log_uuid = None;
            return;
        }

        let underlying_index = App::to_underlying_index(total, visual_index);
        let Some(&raw_idx) = self.displaying_logs.indices.get(underlying_index) else {
            self.selected_log_uuid = None;
            return;
        };

        let item = &self.raw_logs[raw_idx];
        self.selected_log_uuid = Some(item.id);
    }
}

// ============================================================================
// Scrolling operations
// ============================================================================
impl App {
    fn handle_log_item_scrolling(&mut self, move_next: bool, circular: bool) -> Result<()> {
        match (move_next, circular) {
            (true, true) => {
                self.displaying_logs.select_next_circular();
            }
            (true, false) => {
                self.displaying_logs.select_next();
            }
            (false, true) => {
                self.displaying_logs.select_previous_circular();
            }
            (false, false) => {
                self.displaying_logs.select_previous();
            }
        }

        self.update_selected_uuid();
        self.after_selection_change()?;
        Ok(())
    }

    fn handle_logs_view_scrolling(&mut self, move_down: bool) -> Result<()> {
        {
            let lines_count = self.logs_block.get_lines_count();
            let current_position = self.logs_block.get_scroll_position();

            let new_position = if move_down {
                if current_position >= lines_count.saturating_sub(1) {
                    current_position // Stay at bottom
                } else {
                    current_position.saturating_add(1)
                }
            } else {
                current_position.saturating_sub(1)
            };

            self.logs_block.set_scroll_position(new_position);
            self.logs_block
                .update_scrollbar_state(lines_count, Some(new_position));
        }

        Ok(())
    }

    fn handle_details_block_scrolling(&mut self, move_next: bool) -> Result<()> {
        let lines_count = self.details_block.get_lines_count();
        if lines_count == 0 {
            self.details_block.set_scroll_position(0);
            self.details_block.update_scrollbar_state(0, Some(0));
            return Ok(());
        }

        let current_position = self.details_block.get_scroll_position();
        let last_index = lines_count.saturating_sub(1);

        let new_position = if move_next {
            current_position
                .min(last_index) // clamp
                .saturating_add(1)
                .min(last_index) // don’t exceed bottom
        } else {
            current_position.saturating_sub(1)
        };

        self.details_block.set_scroll_position(new_position);
        self.details_block
            .update_scrollbar_state(lines_count, Some(new_position));

        Ok(())
    }

    fn handle_debug_logs_scrolling(&mut self, move_next: bool) -> Result<()> {
        let lines_count = self.debug_block.get_lines_count();
        if lines_count == 0 {
            self.debug_block.set_scroll_position(0);
            self.debug_block.update_scrollbar_state(0, Some(0));
            return Ok(());
        }

        let current_position = self.debug_block.get_scroll_position();
        let last_index = lines_count.saturating_sub(1);

        let new_position = if move_next {
            current_position
                .min(last_index)
                .saturating_add(1)
                .min(last_index)
        } else {
            current_position.saturating_sub(1)
        };

        self.debug_block.set_scroll_position(new_position);
        self.debug_block
            .update_scrollbar_state(lines_count, Some(new_position));

        Ok(())
    }

    fn handle_horizontal_scrolling(
        &mut self,
        block_id: uuid::Uuid,
        move_right: bool,
    ) -> Result<()> {
        let (block, area) = if block_id == self.logs_block.id() {
            (&mut self.logs_block, self.last_logs_area)
        } else if block_id == self.details_block.id() {
            (&mut self.details_block, self.last_details_area)
        } else if block_id == self.debug_block.id() {
            (&mut self.debug_block, self.last_debug_area)
        } else {
            return Ok(());
        };

        let Some(area) = area else {
            return Ok(());
        };

        let current_position = block.get_horizontal_scroll_position();
        let content_width = block.get_content_width();

        // Calculate actual viewport width based on the rendered area
        let [main_content_area, _scrollbar_area] = Layout::horizontal([
            Constraint::Fill(1),   // Main content takes most space
            Constraint::Length(1), // Scrollbar is 1 character wide
        ])
        .margin(0)
        .areas(area);

        let [content_area, _horizontal_scrollbar_area] = Layout::vertical([
            Constraint::Fill(1),   // Main content
            Constraint::Length(1), // Horizontal scrollbar height
        ])
        .margin(0)
        .areas(main_content_area);
        let content_rect = block.get_content_rect(content_area, true);
        let viewport_width = content_rect.width as usize;

        if content_width <= viewport_width {
            return Ok(());
        }

        let max_scroll = content_width.saturating_sub(viewport_width);
        let new_position = if move_right {
            current_position
                .saturating_add(HORIZONTAL_SCROLL_STEP)
                .min(max_scroll)
        } else {
            current_position.saturating_sub(HORIZONTAL_SCROLL_STEP)
        };

        block.set_horizontal_scroll_position(new_position);
        block.update_horizontal_scrollbar_state(content_width, viewport_width);

        Ok(())
    }
}

// ============================================================================
// Event handling
// ============================================================================
impl App {
    fn handle_mouse_event(&mut self, mouse: &MouseEvent) -> Result<()> {
        match mouse.kind {
            MouseEventKind::ScrollDown => {
                if let Some(block_under_mouse) = self.get_block_under_mouse(mouse) {
                    // Check if Shift is held for horizontal scrolling
                    if mouse.modifiers.contains(event::KeyModifiers::SHIFT) {
                        self.handle_horizontal_scrolling(block_under_mouse, true)?;
                    } else {
                        // Normal vertical scrolling
                        if block_under_mouse == self.logs_block.id() {
                            self.handle_logs_view_scrolling(true)?;
                        } else if block_under_mouse == self.details_block.id() {
                            self.handle_details_block_scrolling(true)?;
                        } else if block_under_mouse == self.debug_block.id() {
                            self.handle_debug_logs_scrolling(true)?;
                        }
                    }
                }
            }
            MouseEventKind::ScrollUp => {
                if let Some(block_under_mouse) = self.get_block_under_mouse(mouse) {
                    // Check if Shift is held for horizontal scrolling
                    if mouse.modifiers.contains(event::KeyModifiers::SHIFT) {
                        self.handle_horizontal_scrolling(block_under_mouse, false)?;
                    } else {
                        // Normal vertical scrolling
                        if block_under_mouse == self.logs_block.id() {
                            self.handle_logs_view_scrolling(false)?;
                        } else if block_under_mouse == self.details_block.id() {
                            self.handle_details_block_scrolling(false)?;
                        } else if block_under_mouse == self.debug_block.id() {
                            self.handle_debug_logs_scrolling(false)?;
                        }
                    }
                }
            }
            MouseEventKind::ScrollLeft => {
                log::debug!("ScrollLeft (touchpad)");
                if let Some(block_under_mouse) = self.get_block_under_mouse(mouse) {
                    self.handle_horizontal_scrolling(block_under_mouse, false)?;
                }
            }
            MouseEventKind::ScrollRight => {
                log::debug!("ScrollRight (touchpad)");
                if let Some(block_under_mouse) = self.get_block_under_mouse(mouse) {
                    self.handle_horizontal_scrolling(block_under_mouse, true)?;
                }
            }
            MouseEventKind::Moved => {}
            _ => {}
        }
        Ok(())
    }

    fn yank_current_log(&mut self) -> Result<()> {
        let (indices, state) = (&self.displaying_logs.indices, &self.displaying_logs.state);

        let Some(i) = state.selected() else {
            log::debug!("No log item selected for yanking");
            return Ok(());
        };

        // Access items in reverse order to match the LOGS panel display order
        let reversed_index = indices.len().saturating_sub(1).saturating_sub(i);
        let raw_idx = indices[reversed_index];
        let item = &self.raw_logs[raw_idx];

        let mut clipboard = Clipboard::new()?;
        let yank_content = item.make_yank_content();
        clipboard.set_text(&yank_content)?;

        log::debug!("Copied {} chars to clipboard", yank_content.len());

        self.set_display_event(
            "Selected log copied to clipboard".to_string(),
            Duration::from_millis(DISPLAY_EVENT_DURATION_MS),
            None, // use default style
        );

        Ok(())
    }

    fn clear_logs(&mut self) {
        self.raw_logs.clear();
        self.displaying_logs = LogList::new(Vec::new());
        self.filter_input.clear();
    }

    fn handle_key(&mut self, key: KeyEvent) -> Result<()> {
        if key.kind != KeyEventKind::Press {
            return Ok(());
        }

        // help popup mode has higher priority
        if self.show_help_popup {
            match key.code {
                KeyCode::Char('?') | KeyCode::Esc => {
                    self.show_help_popup = false;
                    return Ok(());
                }
                KeyCode::Char('q') => {
                    // let 'q' fall through to quit the program
                }
                _ => return Ok(()), // ignore other keys when help popup is open
            }
        }

        // handle filter input mode when focused
        if !self.filter_input.is_empty() && self.filter_focused {
            match key.code {
                KeyCode::Esc => {
                    // unfocus and clear filter
                    self.filter_focused = false;
                    self.filter_input.clear();
                    self.apply_filter();
                    return Ok(());
                }
                KeyCode::Char(c) => {
                    self.filter_input.push(c);
                    self.apply_filter();
                    return Ok(());
                }
                KeyCode::Backspace => {
                    self.filter_input.pop();
                    // if user deleted the '/', clear the filter and unfocus
                    if self.filter_input.is_empty() {
                        self.filter_focused = false;
                        self.apply_filter();
                    } else {
                        self.apply_filter();
                    }
                    return Ok(());
                }
                KeyCode::Enter => {
                    // unfocus the filter input, keep the filter active
                    self.filter_focused = false;
                    // if only a '/' is left, clear the filter
                    if self.filter_input.len() == 1 {
                        self.filter_input.clear();
                        self.apply_filter();
                    }
                    return Ok(());
                }
                _ => {}
            }
        }

        match key.code {
            KeyCode::Char('q') => {
                // always quit, regardless of filter state or other modes
                log::debug!("Quit key pressed");
                self.is_exiting = true;
                Ok(())
            }
            KeyCode::Esc => {
                // Esc only goes back (never quits)
                // if filter is active but not focused, clear it
                if !self.filter_input.is_empty() && !self.filter_focused {
                    self.filter_input.clear();
                    self.apply_filter();
                }
                // Esc never quits the program
                Ok(())
            }
            KeyCode::Char('c') => {
                if key.modifiers.contains(event::KeyModifiers::CONTROL) {
                    self.is_exiting = true;
                } else {
                    self.clear_logs();
                }
                Ok(())
            }
            KeyCode::Char('j') | KeyCode::Down => {
                let focused_block = self.get_display_focused_block();
                if focused_block == self.details_block.id() {
                    self.handle_details_block_scrolling(true)?;
                } else if focused_block == self.debug_block.id() {
                    self.handle_debug_logs_scrolling(true)?;
                } else {
                    self.handle_log_item_scrolling(true, true)?;
                }
                Ok(())
            }
            KeyCode::Char('k') | KeyCode::Up => {
                let focused_block = self.get_display_focused_block();
                if focused_block == self.details_block.id() {
                    self.handle_details_block_scrolling(false)?;
                } else if focused_block == self.debug_block.id() {
                    self.handle_debug_logs_scrolling(false)?;
                } else {
                    self.handle_log_item_scrolling(false, true)?;
                }
                Ok(())
            }
            KeyCode::Char('/') => {
                self.filter_input = "/".to_string();
                self.filter_focused = true;
                self.apply_filter();
                Ok(())
            }
            KeyCode::Char('[') => {
                // decrease detail level (show less info) - non-circular
                self.detail_level = self.detail_level.decrement();
                // reset filter cache since preview text changes
                self.filter_engine.reset();
                self.rebuild_filtered_list();
                Ok(())
            }
            KeyCode::Char(']') => {
                // increase detail level (show more info) - non-circular
                self.detail_level = self.detail_level.increment();
                // reset filter cache since preview text changes
                self.filter_engine.reset();
                self.rebuild_filtered_list();
                Ok(())
            }
            KeyCode::Char('y') => {
                if let Err(e) = self.yank_current_log() {
                    log::debug!("Failed to yank log content: {}", e);
                }
                Ok(())
            }
            KeyCode::Char('1') => {
                self.set_hard_focused_block(self.logs_block.id());
                Ok(())
            }
            KeyCode::Char('2') => {
                self.set_hard_focused_block(self.details_block.id());
                Ok(())
            }
            KeyCode::Char('3') => {
                if self.show_debug_logs {
                    self.set_hard_focused_block(self.debug_block.id());
                }
                Ok(())
            }
            KeyCode::Char('w') => {
                self.text_wrapping_enabled = !self.text_wrapping_enabled;
                log::debug!("Text wrapping toggled: {}", self.text_wrapping_enabled);

                let message = if self.text_wrapping_enabled {
                    "Text wrapping enabled"
                } else {
                    "Text wrapping disabled"
                };
                self.set_display_event(
                    message.to_string(),
                    Duration::from_millis(DISPLAY_EVENT_DURATION_MS),
                    None,
                );

                Ok(())
            }
            KeyCode::Char('d') => {
                self.show_debug_logs = !self.show_debug_logs;
                log::debug!("Debug logs visibility toggled: {}", self.show_debug_logs);
                Ok(())
            }
            KeyCode::Char('h') | KeyCode::Left => {
                let focused_block = self.get_display_focused_block();
                self.handle_horizontal_scrolling(focused_block, false)?;
                Ok(())
            }
            KeyCode::Char('l') | KeyCode::Right => {
                let focused_block = self.get_display_focused_block();
                self.handle_horizontal_scrolling(focused_block, true)?;
                Ok(())
            }
            KeyCode::Char('?') => {
                self.show_help_popup = !self.show_help_popup;
                Ok(())
            }
            KeyCode::Char(' ') => {
                self.after_selection_change()?;
                Ok(())
            }
            KeyCode::Char('g') => {
                self.logs_block.set_scroll_position(0);
                // force autoscroll to be true so that we don't wait for the next render to update the scrollbar state
                // waiting for the next render may cause new logs arrive beforehand, thus the view is not at the top
                self.update_autoscroll_state();
                self.update_logs_scrollbar_state();
                Ok(())
            }
            _ => Ok(()),
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
    }
}

// ============================================================================
// Widget implementation
// ============================================================================
impl Widget for &mut App {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // detect if hard focus changed since last render
        let focus_changed = self.hard_focused_block_id != self.prev_hard_focused_block_id;

        // determine dynamic layout based on hard focus
        let (logs_percentage, details_percentage) =
            if self.hard_focused_block_id == self.logs_block.id() {
                (60, 40)
            } else if self.hard_focused_block_id == self.details_block.id() {
                (40, 60)
            } else {
                (60, 40) // default for debug block or any other case
            };

        if self.show_debug_logs {
            let [main, debug_area, footer_area] = Layout::vertical([
                Constraint::Fill(1),
                Constraint::Length(6),
                Constraint::Length(1),
            ])
            .areas(area);

            let [logs_area, details_area] = Layout::vertical([
                Constraint::Percentage(logs_percentage),
                Constraint::Percentage(details_percentage),
            ])
            .areas(main);

            self.render_logs(logs_area, buf).unwrap();
            self.render_details(details_area, buf).unwrap();
            self.render_debug_logs(debug_area, buf).unwrap();
            self.render_footer(footer_area, buf).unwrap();
        } else {
            let [main, footer_area] =
                Layout::vertical([Constraint::Fill(1), Constraint::Length(1)]).areas(area);

            let [logs_area, details_area] = Layout::vertical([
                Constraint::Percentage(logs_percentage),
                Constraint::Percentage(details_percentage),
            ])
            .areas(main);

            self.render_logs(logs_area, buf).unwrap();
            self.render_details(details_area, buf).unwrap();
            self.render_footer(footer_area, buf).unwrap();
        }

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
