use crate::{
    app_block::AppBlock,
    content_line_maker::wrap_content_to_lines,
    file_finder,
    log_list::LogList,
    log_parser::{LogItem, process_delta},
    metadata, theme,
    ui_logger::UiLogger,
};
use anyhow::{Result, anyhow};
use arboard::Clipboard;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, MouseEvent, MouseEventKind};
use memmap2::MmapOptions;
use ratatui::{
    Terminal,
    backend::CrosstermBackend,
    prelude::*,
    widgets::{Padding, Paragraph, StatefulWidget, Widget},
};
use std::{
    //collections::HashMap, // Removed - using direct fields instead
    fs::File,
    io,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};

pub fn start(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<()> {
    color_eyre::install().or(Err(anyhow!("Error installing color_eyre")))?;

    // cd ~/Library/Application\ Support/DouyinAR/Logs/previewLog && open .
    let log_dir_path = match dirs::home_dir() {
        Some(path) => path.join("Library/Application Support/DouyinAR/Logs/previewLog"),
        None => {
            return Err(anyhow!("Error getting home directory"));
        }
    };

    App::new(log_dir_path).run(terminal)
}

struct App {
    is_exiting: bool,
    raw_logs: Vec<LogItem>,
    displaying_logs: LogList,
    log_dir_path: PathBuf,
    log_file_path: PathBuf,
    last_len: u64,
    prev_meta: Option<metadata::MetaSnap>,
    autoscroll: bool,
    filter_mode: bool,                    // Whether we're in filter input mode
    filter_input: String,                 // Current filter input text
    detail_level: u8,                     // Detail level for log display (0-4, default 1)
    debug_logs: Arc<Mutex<Vec<String>>>,  // Debug log messages for UI display
    focused_block_id: Option<uuid::Uuid>, // Currently focused block ID
    logs_block: AppBlock,
    details_block: AppBlock,
    debug_block: AppBlock,
    prev_selected_log_id: Option<uuid::Uuid>, // Track previous selected log item ID for details reset
    selected_log_uuid: Option<uuid::Uuid>,    // Track currently selected log item UUID
    last_logs_area: Option<Rect>, // Store the last rendered logs area for selection visibility

    event: Option<MouseEvent>,
}

impl App {
    fn setup_logger() -> Arc<Mutex<Vec<String>>> {
        let debug_logs = Arc::new(Mutex::new(Vec::new()));
        let logger = Box::new(UiLogger::new(debug_logs.clone()));

        match log::set_logger(Box::leak(logger)) {
            Ok(_) => {
                log::set_max_level(log::LevelFilter::Debug);
            }
            Err(_) => {}
        }

        debug_logs
    }

    fn new(log_dir_path: PathBuf) -> Self {
        let debug_logs = Self::setup_logger();

        // Try to find the initial log file, but don't fail if none exists
        let log_file_path = match file_finder::find_latest_live_log(&log_dir_path) {
            Ok(path) => {
                log::debug!("Found initial log file: {}", path.display());
                path
            }
            Err(e) => {
                log::debug!("No log files found initially: {}", e);
                // Create a non-existent dummy path that will be replaced when a real log appears
                log_dir_path.join("__no_log_file_yet__.log")
            }
        };

        Self {
            is_exiting: false,
            raw_logs: Vec::new(),
            displaying_logs: LogList::new(Vec::new()),
            log_dir_path,
            log_file_path,
            last_len: 0,
            prev_meta: None,
            autoscroll: true,
            filter_mode: false,
            filter_input: String::new(),
            detail_level: 1,
            debug_logs,
            focused_block_id: None,
            logs_block: AppBlock::new().set_title(format!("[1]─Logs")),
            details_block: AppBlock::new()
                .set_title("[2]─Details")
                .set_padding(Padding::horizontal(1)),
            debug_block: AppBlock::new()
                .set_title("[3]─Debug Logs")
                .set_padding(Padding::horizontal(1)),
            prev_selected_log_id: None,
            selected_log_uuid: None,
            last_logs_area: None,

            event: None,
        }
    }

    fn run(mut self, terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<()> {
        self.set_focused_block(self.logs_block.id());

        let poll_interval = Duration::from_millis(100);

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| -> Result<()> {
            while !self.is_exiting {
                self.poll_event(poll_interval)?;
                self.update_logs()?;
                terminal.draw(|frame| frame.render_widget(&mut self, frame.area()))?;
            }
            Ok(())
        }));
        match result {
            Ok(r) => r,
            Err(_) => {
                eprintln!("Application panicked, terminal restored");
                std::process::exit(1);
            }
        }
    }

    fn poll_event(&mut self, poll_interval: Duration) -> Result<()> {
        // Check for newer log files first
        if let Ok(Some(newer_file)) = self.check_for_newer_log_file() {
            self.switch_to_log_file(newer_file)?;
        }

        if event::poll(poll_interval)? {
            let event = event::read()?;
            match event {
                Event::Key(key) => self.handle_key(key)?,
                Event::Mouse(mouse) => {
                    match mouse.kind {
                        MouseEventKind::ScrollDown => {
                            if self.is_log_block_focused()? {
                                self.handle_logs_view_scrolling(true)?;
                            }
                            if self.is_details_block_focused()? {
                                self.handle_details_block_scrolling(true)?;
                            }
                            if self.is_debug_block_focused()? {
                                self.handle_debug_logs_scrolling(true)?;
                            }
                        }
                        MouseEventKind::ScrollUp => {
                            if self.is_log_block_focused()? {
                                self.handle_logs_view_scrolling(false)?;
                            }
                            if self.is_details_block_focused()? {
                                self.handle_details_block_scrolling(false)?;
                            }
                            if self.is_debug_block_focused()? {
                                self.handle_debug_logs_scrolling(false)?;
                            }
                        }
                        MouseEventKind::Moved => {
                            // Mouse moved - the render methods will handle hover focus
                            // Just store the event so blocks can check if mouse is hovering
                        }
                        _ => {}
                    }
                    self.event = Some(mouse);
                }
                Event::Resize(width, height) => {
                    // Terminal was resized, ratatui will handle the layout automatically
                    log::debug!("Terminal resized to {}x{}", width, height);
                }
                _ => {}
            }
        }

        Ok(())
    }

    fn to_underlying_index(total: usize, visual_index: usize) -> usize {
        total.saturating_sub(1).saturating_sub(visual_index)
    }

    fn to_visual_index(total: usize, underlying_index: usize) -> usize {
        total.saturating_sub(1).saturating_sub(underlying_index)
    }

    fn check_for_newer_log_file(&self) -> Result<Option<PathBuf>> {
        match file_finder::find_latest_live_log(&self.log_dir_path) {
            Ok(latest_file_path) => {
                // Check if we currently have no valid log file (first time finding one)
                if !self.log_file_path.exists() {
                    log::debug!("Found first log file: {}", latest_file_path.display());
                    Ok(Some(latest_file_path))
                } else if latest_file_path != self.log_file_path {
                    log::debug!(
                        "Found newer log file: {} (current: {})",
                        latest_file_path.display(),
                        self.log_file_path.display()
                    );
                    Ok(Some(latest_file_path))
                } else {
                    Ok(None)
                }
            }
            Err(e) => {
                log::debug!("No log files found yet: {}", e);
                Ok(None)
            }
        }
    }

    fn switch_to_log_file(&mut self, new_file_path: PathBuf) -> Result<()> {
        log::debug!(
            "Switching from {} to {}",
            self.log_file_path.display(),
            new_file_path.display()
        );

        // Store current UI state
        let current_filter = self.filter_input.clone();
        let current_autoscroll = self.autoscroll;
        let current_detail_level = self.detail_level;

        // Switch to new file
        self.log_file_path = new_file_path;
        self.last_len = 0;
        self.prev_meta = None;

        // Reset logs but preserve UI state
        self.raw_logs.clear();
        self.displaying_logs = LogList::new(Vec::new());

        // Restore UI state
        self.filter_input = current_filter;
        self.autoscroll = current_autoscroll;
        self.detail_level = current_detail_level;

        // Reset blocks state
        self.logs_block.set_scroll_position(0);
        self.logs_block.set_lines_count(0);
        self.details_block.set_scroll_position(0);

        // Clear selection tracking
        self.selected_log_uuid = None;
        self.prev_selected_log_id = None;

        Ok(())
    }

    fn update_logs(&mut self) -> Result<()> {
        // Skip update if we don't have a valid log file yet
        if !self.log_file_path.exists() {
            return Ok(());
        }

        let current_meta = match metadata::stat_path(&self.log_file_path) {
            Ok(m) => m,
            Err(_) => {
                // File might have been deleted or rotated, just skip this update
                return Ok(());
            }
        };

        if metadata::has_changed(&self.prev_meta, &current_meta) {
            if current_meta.len < self.last_len {
                // File truncated/rotated: reset read offset but keep current UI state
                self.last_len = 0;
            }

            if current_meta.len > self.last_len {
                if let Ok(new_items) =
                    map_and_process_delta(&self.log_file_path, self.last_len, current_meta.len)
                {
                    let old_items_count = self.displaying_logs.items.len();
                    let previous_uuid = self.selected_log_uuid;
                    let previous_scroll_pos = Some(self.logs_block.get_scroll_position());

                    log::debug!(
                        "Found {} new log items in file://{}",
                        new_items.len(),
                        self.log_file_path.display().to_string().replace(" ", "%20")
                    );
                    self.raw_logs.extend(new_items);

                    // Rebuild displayed logs (respect filter)
                    if self.filter_input.is_empty() {
                        self.displaying_logs = LogList::new(self.raw_logs.clone());
                    } else {
                        // Re-apply filter without losing selection
                        self.rebuild_filtered_list();
                    }

                    // Restore selection via UUID (no index math)
                    if previous_uuid.is_some() {
                        self.update_selection_by_uuid();
                    } else if self.autoscroll {
                        // No selection -> optionally keep newest selected when autoscroll is ON
                        self.displaying_logs.select_first();
                        self.update_selected_uuid();
                    }

                    // Adjust scroll to keep visible content stable if autoscroll is OFF
                    {
                        let new_items_count = self.displaying_logs.items.len();
                        let items_added = new_items_count.saturating_sub(old_items_count);

                        if self.autoscroll {
                            self.logs_block.set_scroll_position(0);
                        } else if let Some(prev) = previous_scroll_pos {
                            // Because newest is at visual index 0, adding items pushes
                            // existing content down; keep the same lines visible by shifting
                            // the top by items_added.
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
                }
                self.last_len = current_meta.len;
            }

            self.prev_meta = Some(current_meta);
        }
        return Ok(());

        fn map_and_process_delta(
            file_path: &Path,
            prev_len: u64,
            cur_len: u64,
        ) -> Result<Vec<LogItem>> {
            let file = File::open(file_path)?;
            let mmap = unsafe { MmapOptions::new().len(cur_len as usize).map(&file)? };

            let start = (prev_len as usize).min(mmap.len());
            let end = (cur_len as usize).min(mmap.len());
            let delta_bytes = &mmap[start..end];

            if delta_bytes.is_empty() {
                return Ok(Vec::new());
            }

            let delta_str = String::from_utf8_lossy(delta_bytes);
            let log_items = process_delta(&delta_str);

            Ok(log_items)
        }
    }

    fn apply_filter(&mut self) {
        let previous_uuid = self.selected_log_uuid;
        let prev_scroll_pos = Some(self.logs_block.get_scroll_position());

        self.rebuild_filtered_list();

        // Restore selection via UUID if possible
        if previous_uuid.is_some() {
            self.update_selection_by_uuid();
        } else if self.autoscroll {
            self.displaying_logs.select_first();
            self.update_selected_uuid();
        }

        // Clamp scroll position (don't attempt to be clever across filtering)
        {
            let new_total = self.displaying_logs.items.len();
            let mut pos = prev_scroll_pos.unwrap_or(0);
            if new_total == 0 {
                pos = 0;
            } else {
                pos = pos.min(new_total.saturating_sub(1));
            }
            self.logs_block.set_scroll_position(pos);
            self.logs_block.set_lines_count(new_total);
            self.logs_block.update_scrollbar_state(new_total, Some(pos));
        }
    }

    // Helper used by update_logs/apply_filter to rebuild displayed logs
    fn rebuild_filtered_list(&mut self) {
        if self.filter_input.is_empty() {
            self.displaying_logs = LogList::new(self.raw_logs.clone());
        } else {
            let filtered_items: Vec<LogItem> = self
                .raw_logs
                .iter()
                .filter(|item| item.contains(&self.filter_input))
                .cloned()
                .collect();
            self.displaying_logs = LogList::new(filtered_items);
        }
    }

    fn exit_filter_mode(&mut self) {
        self.filter_mode = false;
        self.filter_input.clear();
        // Reset to show all logs
        self.displaying_logs = LogList::new(self.raw_logs.clone());
        self.displaying_logs.select_first();
    }

    fn update_logs_scrollbar_state(&mut self) {
        let total = self.displaying_logs.items.len();

        {
            // Clamp position to valid range
            let max_top = total.saturating_sub(1);
            let pos = self.logs_block.get_scroll_position().min(max_top);
            self.logs_block.set_scroll_position(pos);

            self.logs_block.set_lines_count(total);
            self.logs_block.update_scrollbar_state(total, Some(pos));
        }
    }

    fn render_footer(&self, area: Rect, buf: &mut Buffer) -> Result<()> {
        let help_text = if self.filter_mode {
            format!(
                "Filter: {} (Press Enter to apply, Esc to cancel)",
                self.filter_input
            )
        } else {
            "jk↑↓: nav | gG: top/bottom | /: filter | []: detail | y: yank | JK: scroll focused | c: clear | f: fold | q: quit"
                .to_string()
        };
        Paragraph::new(help_text).centered().render(area, buf);
        Ok(())
    }

    fn render_logs(&mut self, area: Rect, buf: &mut Buffer) -> Result<()> {
        // Store the area for selection visibility calculations
        self.last_logs_area = Some(area);

        // Create a horizontal layout: main content area + scrollbar area
        let [content_area, scrollbar_area] = Layout::horizontal([
            Constraint::Fill(1),   // Main content takes most space
            Constraint::Length(1), // Scrollbar is 1 character wide
        ])
        .margin(0)
        .areas(area);

        let is_log_focused = self.is_log_block_focused().unwrap_or(false);

        // Get and update the LOGS block (title, mouse focus)
        let title = if self.log_file_path.exists() {
            let mut display_content = format!(
                "[1]─Logs | {} / {}",
                self.displaying_logs.items.len(),
                self.raw_logs.len()
            );
            if self.autoscroll {
                display_content += " | Autoscrolling";
            }
            display_content
        } else {
            format!("[1]─Logs | Waiting for log files...")
        };
        self.logs_block.update_title(title);
        let logs_block_id = self.logs_block.id();

        let (should_focus, clicked_row) = if let Some(event) = self.event {
            let was_clicked =
                self.logs_block
                    .handle_mouse_event(&event, content_area, self.event.as_ref());
            let is_left_click = event.kind
                == crossterm::event::MouseEventKind::Up(crossterm::event::MouseButton::Left);

            let inner_area = self.logs_block.build(false).inner(content_area);
            let is_within_bounds =
                inner_area.contains(ratatui::layout::Position::new(event.column, event.row));
            let click_row = if is_left_click && is_within_bounds {
                Some(event.row)
            } else {
                None
            };
            (was_clicked, click_row)
        } else {
            (false, None)
        };

        if should_focus {
            self.set_focused_block(logs_block_id);
        }

        // Use the displaying_logs which contains either filtered or all logs
        let items_to_render = &self.displaying_logs.items;
        let selected_index = self.displaying_logs.state.selected();
        let total_lines = items_to_render.len();

        // Compute inner content rect and visible height
        let inner_area = self
            .logs_block
            .get_content_rect(content_area, is_log_focused);
        let visible_height = inner_area.height as usize;
        let content_width = inner_area.width as usize;

        // Clamp scroll position
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

        // Handle click selection (convert row to absolute index in reversed order)
        let mut selection_changed = false;
        if let Some(click_row) = clicked_row {
            let relative_row = click_row.saturating_sub(inner_area.y);
            let exact_item_number = scroll_position.saturating_add(relative_row as usize);
            if exact_item_number < total_lines {
                self.displaying_logs.state.select(Some(exact_item_number));
                selection_changed = true;
            }
            // Click beyond the end of available lines is ignored
        }

        // Build only the visible slice of lines
        let end = (scroll_position + visible_height).min(total_lines);
        let start = scroll_position.min(end);

        let mut content_lines = Vec::with_capacity(end.saturating_sub(start));
        for i in start..end {
            // Map the visual index (0 = newest/top) to underlying item index
            let item_idx = total_lines.saturating_sub(1).saturating_sub(i);
            let log_item = &items_to_render[item_idx];

            let detail_text = log_item.get_preview_text(self.detail_level);
            let level_style = match log_item.level.as_str() {
                "ERROR" => theme::ERROR_STYLE,
                "WARNING" => theme::WARN_STYLE,
                "INFO" => theme::INFO_STYLE,
                "DEBUG" => theme::DEBUG_STYLE,
                _ => Style::default().fg(theme::TEXT_FG_COLOR),
            };

            // Selection highlighting uses the same (reversed) indices (selected_index compares to i)
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

            // Pad selected lines to full width for a clean highlight bar
            let padded_text = if is_selected {
                format!("{:<width$}", display_text, width = content_width)
            } else {
                display_text
            };

            content_lines.push(Line::styled(padded_text, final_style));
        }

        // Update scrollbar and line counts using TOTAL lines (not just the visible window)
        let logs_block = &mut self.logs_block;
        logs_block.set_lines_count(total_lines);
        logs_block.update_scrollbar_state(total_lines, Some(scroll_position));

        // Build the block after mutable ops
        let block = self.logs_block.build(is_log_focused);

        // Render only the visible slice; no additional vertical scroll needed here
        Paragraph::new(content_lines)
            .block(block)
            .fg(theme::TEXT_FG_COLOR)
            .scroll((0, 0))
            .render(content_area, buf);

        // Render the scrollbar using AppBlock's state
        let scrollbar = AppBlock::create_scrollbar(is_log_focused);
        let logs_block = &mut self.logs_block;
        StatefulWidget::render(
            scrollbar,
            scrollbar_area,
            buf,
            logs_block.get_scrollbar_state(),
        );

        // Update autoscroll state based on current view position (uniform detection)
        self.update_autoscroll_state();

        // Update UUID tracking if selection changed
        if selection_changed {
            self.update_selected_uuid();
        }

        Ok(())
    }

    fn render_details(&mut self, area: Rect, buf: &mut Buffer) -> Result<()> {
        // Get the DETAILS block ID and check if focused
        let details_block_id = self.details_block.id();
        let is_focused = self.focused_block_id == Some(details_block_id);

        // Handle click and set focus
        let should_focus = if let Some(event) = self.event {
            self.details_block
                .handle_mouse_event(&event, area, self.event.as_ref())
        } else {
            false
        };

        if should_focus {
            self.set_focused_block(details_block_id);
        }

        // Create a horizontal layout: main content area + scrollbar area
        let [content_area, scrollbar_area] = Layout::horizontal([
            Constraint::Fill(1),   // Main content takes most space
            Constraint::Length(1), // Scrollbar is 1 character wide
        ])
        .margin(0)
        .areas(area);

        // Use the displaying_logs which contains either filtered or all logs
        let (items, state) = (&self.displaying_logs.items, &self.displaying_logs.state);

        let content = if let Some(i) = state.selected() {
            // Access items in reverse order to match the LOGS panel display order
            let reversed_index = items.len().saturating_sub(1).saturating_sub(i);
            let item = &items[reversed_index];

            // Check if the selected log item has changed and reset scroll position if needed
            if self.prev_selected_log_id != Some(item.id) {
                self.prev_selected_log_id = Some(item.id);
                self.details_block.set_scroll_position(0);
            }

            let mut content_lines = vec![
                Line::from(vec!["Time:   ".bold(), item.time.clone().into()]),
                Line::from(vec!["Level:  ".bold(), item.level.clone().into()]),
                Line::from(vec!["Origin: ".bold(), item.origin.clone().into()]),
                Line::from(vec!["Tag:    ".bold(), item.tag.clone().into()]),
                Line::from("Content:".bold()),
            ];
            // Get the actual content rect accounting for borders
            let content_rect = self
                .details_block
                .get_content_rect(content_area, is_focused);
            content_lines.extend(wrap_content_to_lines(&item.content, content_rect.width));
            content_lines
        } else {
            // No log item selected - clear the previous selection tracking
            if self.prev_selected_log_id.is_some() {
                self.prev_selected_log_id = None;
                self.details_block.set_scroll_position(0);
                log::debug!("No log item selected - resetting details scroll position");
            }
            vec![Line::from("Select a log item to see details...".italic())]
        };

        // The content vector already contains properly wrapped lines
        let lines_count = content.len();

        // Update the details block with lines count and scrollbar state
        self.details_block.set_lines_count(lines_count);
        let scroll_position = self.details_block.get_scroll_position();
        self.details_block
            .update_scrollbar_state(lines_count, Some(scroll_position));

        // Build the block after mutable operations
        let block = self.details_block.build(is_focused);

        Paragraph::new(content)
            .block(block)
            .fg(theme::TEXT_FG_COLOR)
            .scroll((scroll_position as u16, 0))
            .render(content_area, buf);

        let scrollbar = AppBlock::create_scrollbar(is_focused);

        // Use AppBlock's scrollbar state
        StatefulWidget::render(
            scrollbar,
            scrollbar_area,
            buf,
            self.details_block.get_scrollbar_state(),
        );
        Ok(())
    }

    fn render_debug_logs(&mut self, area: Rect, buf: &mut Buffer) -> Result<()> {
        // Get the DEBUG block ID and check if focused
        let debug_block_id = self.debug_block.id();
        let is_focused = self.focused_block_id == Some(debug_block_id);

        // Handle click and set focus
        let should_focus = if let Some(event) = self.event {
            self.debug_block
                .handle_mouse_event(&event, area, self.event.as_ref())
        } else {
            false
        };

        if should_focus {
            self.set_focused_block(debug_block_id);
        }

        // Create a horizontal layout: main content area + scrollbar area
        let [content_area, scrollbar_area] = Layout::horizontal([
            Constraint::Fill(1),   // Main content takes most space
            Constraint::Length(1), // Scrollbar is 1 character wide
        ])
        .margin(0)
        .areas(area);

        // Build the block after getting focus info
        let _block = self.debug_block.build(is_focused);

        let debug_logs_lines = if let Ok(logs) = self.debug_logs.lock() {
            if logs.is_empty() {
                vec![Line::from("No debug logs...".italic())]
            } else {
                logs.iter()
                    .rev() // Show most recent first
                    .map(|log_entry| {
                        let style = if log_entry.contains("ERROR") {
                            theme::ERROR_STYLE
                        } else if log_entry.contains("WARNING") {
                            theme::WARN_STYLE
                        } else if log_entry.contains("DEBUG") {
                            theme::DEBUG_STYLE
                        } else {
                            Style::default().fg(theme::TEXT_FG_COLOR)
                        };
                        Line::styled(log_entry.clone(), style)
                    })
                    .collect()
            }
        } else {
            vec![Line::from("Failed to read debug logs...".italic())]
        };

        // The debug_logs_lines vector already contains properly wrapped lines
        let lines_count = debug_logs_lines.len();

        // Update the debug block with lines count and scrollbar state
        self.debug_block.set_lines_count(lines_count);
        if !is_focused {
            self.debug_block.set_scroll_position(0);
        }
        let scroll_position = self.debug_block.get_scroll_position();
        self.debug_block
            .update_scrollbar_state(lines_count, Some(scroll_position));

        // Build the block after mutable operations
        let _block = self.debug_block.build(is_focused);

        Paragraph::new(debug_logs_lines)
            .block(_block)
            .fg(theme::TEXT_FG_COLOR)
            .scroll((scroll_position as u16, 0))
            .render(content_area, buf);

        let scrollbar = AppBlock::create_scrollbar(is_focused);

        // Use AppBlock's scrollbar state
        StatefulWidget::render(
            scrollbar,
            scrollbar_area,
            buf,
            self.debug_block.get_scrollbar_state(),
        );
        Ok(())
    }

    fn is_log_block_focused(&self) -> Result<bool> {
        if let Some(focused_id) = self.focused_block_id {
            Ok(focused_id == self.logs_block.id())
        } else {
            Ok(false)
        }
    }

    fn is_debug_block_focused(&self) -> Result<bool> {
        if let Some(focused_id) = self.focused_block_id {
            Ok(focused_id == self.debug_block.id())
        } else {
            Ok(false)
        }
    }

    fn is_details_block_focused(&self) -> Result<bool> {
        if let Some(focused_id) = self.focused_block_id {
            Ok(focused_id == self.details_block.id())
        } else {
            Ok(false)
        }
    }

    fn ensure_selection_visible(&mut self) -> Result<()> {
        let selected_index = self.displaying_logs.state.selected();

        if let (Some(selected_idx), Some(visible_area)) = (selected_index, self.last_logs_area) {
            {
                let current_scroll_pos = self.logs_block.get_scroll_position();

                // Calculate visible range within the content area
                let content_rect = self.logs_block.get_content_rect(visible_area, false);
                let visible_height = content_rect.height as usize;

                if visible_height == 0 {
                    return Ok(());
                }

                // Use padding = 1 when there is room; otherwise fall back to 0
                let pad = if visible_height > 2 { 1 } else { 0 };

                let view_start = current_scroll_pos;
                let view_end = current_scroll_pos + visible_height.saturating_sub(1);

                // Keep selected inside [view_start + pad, view_end - pad] when possible
                let mut new_scroll_pos = if selected_idx < view_start.saturating_add(pad) {
                    // Scroll up so selected appears at second line (if pad == 1)
                    selected_idx.saturating_sub(pad)
                } else if selected_idx > view_end.saturating_sub(pad) {
                    // Scroll down so selected is not the last line (keeps a 1-line bottom margin when possible)
                    selected_idx
                        .saturating_add(pad)
                        .saturating_add(1)
                        .saturating_sub(visible_height)
                } else {
                    current_scroll_pos
                };

                // Clamp to valid range
                let total_items = self.displaying_logs.items.len();
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
        // Enable autoscroll when the view is at the topmost position (scroll position 0)
        // Disable autoscroll when the view is not at the top
        self.autoscroll = self.logs_block.get_scroll_position() == 0;
    }

    fn handle_log_item_scrolling(&mut self, move_next: bool, circular: bool) -> Result<()> {
        // Handle selection changes using the original LogList logic
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

        // Update the tracked UUID for the new selection
        self.update_selected_uuid();

        // Ensure the newly selected item is visible
        self.ensure_selection_visible()?;
        self.update_logs_scrollbar_state();
        Ok(())
    }

    fn handle_logs_view_scrolling(&mut self, move_down: bool) -> Result<()> {
        // Handle pure view scrolling without changing selection
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

    fn make_yank_content(&self, item: &LogItem) -> String {
        format!(
            "# Formatted Log\n\n## Time:\n\n{}\n\n## Level:\n\n{}\n\n## Origin:\n\n{}\n\n## Tag:\n\n{}\n\n## Content:\n\n{}\n\n# Raw Log\n\n{}",
            item.time, item.level, item.origin, item.tag, item.content, item.raw_content
        )
    }

    fn yank_current_log(&self) -> Result<()> {
        // Use the displaying_logs which contains either filtered or all logs
        let (items, state) = (&self.displaying_logs.items, &self.displaying_logs.state);

        let Some(i) = state.selected() else {
            log::debug!("No log item selected for yanking");
            return Ok(());
        };

        // Access items in reverse order to match the LOGS panel display order
        let reversed_index = items.len().saturating_sub(1).saturating_sub(i);
        let item = &items[reversed_index];

        let mut clipboard = Clipboard::new()?;
        let yank_content = self.make_yank_content(item);
        clipboard.set_text(&yank_content)?;

        log::debug!(
            "Yanked log content to clipboard: {} chars",
            yank_content.len()
        );

        Ok(())
    }

    fn fold_logs(&mut self) {
        log::debug!("Fold functionality not yet implemented");
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

        // Handle filter mode input
        if self.filter_mode {
            match key.code {
                KeyCode::Esc => {
                    self.exit_filter_mode();
                    return Ok(());
                }
                KeyCode::Enter => {
                    self.apply_filter();
                    self.filter_mode = false;
                    return Ok(());
                }
                KeyCode::Char(c) => {
                    self.filter_input.push(c);
                    return Ok(());
                }
                KeyCode::Backspace => {
                    self.filter_input.pop();
                    return Ok(());
                }
                _ => {}
            }
            return Ok(());
        }

        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => {
                log::debug!("Exit key pressed");
                self.is_exiting = true;
                return Ok(());
            }
            KeyCode::Char('c') => {
                if key.modifiers.contains(event::KeyModifiers::CONTROL) {
                    self.is_exiting = true;
                } else {
                    self.clear_logs();
                }
                return Ok(());
            }
            KeyCode::Char('f') => {
                self.fold_logs();
                return Ok(());
            }
            KeyCode::Char('j') | KeyCode::Down => {
                self.handle_log_item_scrolling(true, true)?;
                return Ok(());
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.handle_log_item_scrolling(false, true)?;
                return Ok(());
            }
            KeyCode::Char('g') => {
                self.displaying_logs.select_first();
                self.update_selected_uuid();
                self.ensure_selection_visible()?;
                self.update_logs_scrollbar_state();
                return Ok(());
            }
            KeyCode::Char('G') => {
                self.displaying_logs.select_last();
                self.update_selected_uuid();
                self.ensure_selection_visible()?;
                self.update_logs_scrollbar_state();
                return Ok(());
            }
            KeyCode::Char('/') => {
                self.filter_mode = true;
                self.filter_input.clear();
                return Ok(());
            }
            KeyCode::Char('[') => {
                // Decrease detail level (show less info) - non-circular
                if self.detail_level > 0 {
                    self.detail_level -= 1;
                }
                return Ok(());
            }
            KeyCode::Char(']') => {
                // Increase detail level (show more info) - non-circular
                if self.detail_level < 4 {
                    self.detail_level += 1;
                }
                return Ok(());
            }
            KeyCode::Char('y') => {
                // Yank (copy) the current log item content to clipboard
                if let Err(e) = self.yank_current_log() {
                    log::debug!("Failed to yank log content: {}", e);
                }
                return Ok(());
            }
            _ => {
                return Ok(());
            }
        }
    }

    fn set_focused_block(&mut self, block_id: uuid::Uuid) {
        self.focused_block_id = Some(block_id);
    }

    fn clear_event(&mut self) {
        self.event = None;
    }

    /// Find the index of a log item by its UUID
    fn find_log_by_uuid(&self, uuid: &uuid::Uuid) -> Option<usize> {
        self.displaying_logs
            .items
            .iter()
            .position(|item| &item.id == uuid)
    }

    /// Update the selection based on the currently tracked UUID
    fn update_selection_by_uuid(&mut self) {
        let Some(uuid) = self.selected_log_uuid else {
            return;
        };

        let Some(underlying_index) = self.find_log_by_uuid(&uuid) else {
            // UUID not found in current list, clear selection
            self.displaying_logs.state.select(None);
            self.selected_log_uuid = None;
            return;
        };

        let total = self.displaying_logs.items.len();
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

        let total = self.displaying_logs.items.len();
        if total == 0 {
            self.selected_log_uuid = None;
            return;
        }

        let underlying_index = App::to_underlying_index(total, visual_index);
        let Some(item) = self.displaying_logs.items.get(underlying_index) else {
            self.selected_log_uuid = None;
            return;
        };

        self.selected_log_uuid = Some(item.id);
    }
}

impl Widget for &mut App {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let [main, debug_area, footer_area] = Layout::vertical([
            Constraint::Fill(1),
            Constraint::Length(6),
            Constraint::Length(1),
        ])
        .areas(area);

        let [logs_area, details_area] =
            Layout::vertical([Constraint::Percentage(60), Constraint::Percentage(40)]).areas(main);

        self.render_logs(logs_area, buf).unwrap();
        self.render_details(details_area, buf).unwrap();
        self.render_debug_logs(debug_area, buf).unwrap();
        self.render_footer(footer_area, buf).unwrap();

        self.clear_event();
    }
}
