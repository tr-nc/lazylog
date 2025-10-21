use crate::{
    app_block::AppBlock,
    content_line_maker::{WrappingMode, calculate_content_width, content_into_lines},
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
    fs::File,
    io,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};

#[derive(Clone)]
pub struct AppDesc {
    pub poll_interval: Duration,
    pub show_debug_logs: bool,
}

impl Default for AppDesc {
    fn default() -> Self {
        Self {
            poll_interval: Duration::from_millis(100),
            show_debug_logs: false,
        }
    }
}

pub fn start(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<()> {
    start_with_desc(terminal, AppDesc::default())
}

pub fn start_with_desc(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    desc: AppDesc,
) -> Result<()> {
    color_eyre::install().or(Err(anyhow!("Error installing color_eyre")))?;

    let log_dir_path = match dirs::home_dir() {
        Some(path) => path.join("Library/Application Support/DouyinAR/Logs"),
        None => {
            return Err(anyhow!("Error getting home directory"));
        }
    };

    let app = App::new(log_dir_path, desc.clone());
    app.run(terminal, &desc)
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
    filter_input: String, // Current filter input text (includes leading '/')
    filter_focused: bool, // Whether the filter input is focused
    detail_level: u8,     // Detail level for log display (0-4, default 1)
    debug_logs: Arc<Mutex<Vec<String>>>, // Debug log messages for UI display
    hard_focused_block_id: Option<uuid::Uuid>, // Hard focus: set by clicking, persists until another click
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

    mouse_event: Option<MouseEvent>,
}

impl App {
    fn setup_logger() -> Arc<Mutex<Vec<String>>> {
        let debug_logs = Arc::new(Mutex::new(Vec::new()));
        let logger = Box::new(UiLogger::new(debug_logs.clone()));

        if log::set_logger(Box::leak(logger)).is_ok() {
            log::set_max_level(log::LevelFilter::Debug);
        }

        debug_logs
    }

    fn new(log_dir_path: PathBuf, desc: AppDesc) -> Self {
        let debug_logs = Self::setup_logger();

        let preview_log_dirs = file_finder::find_preview_log_dirs(&log_dir_path);
        let log_file_path = match file_finder::find_latest_live_log(preview_log_dirs) {
            Ok(path) => {
                log::debug!(
                    "Found initial log file: {}",
                    Self::file_path_to_clickable_string(&path)
                );
                path
            }
            Err(e) => {
                log::debug!("No log files found initially: {}", e);
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
            filter_input: String::new(),
            filter_focused: false,
            detail_level: 1,
            debug_logs,
            hard_focused_block_id: None,
            soft_focused_block_id: None,
            logs_block: AppBlock::new().set_title("[1]─Logs".to_string()),
            details_block: AppBlock::new()
                .set_title("[2]─Details")
                .set_padding(Padding::horizontal(1)),
            debug_block: AppBlock::new()
                .set_title("[3]─Debug Logs")
                .set_padding(Padding::horizontal(1)),
            prev_selected_log_id: None,
            selected_log_uuid: None,
            last_logs_area: None,
            last_details_area: None,
            last_debug_area: None,
            text_wrapping_enabled: false, // Default to no wrapping
            show_debug_logs: desc.show_debug_logs,
            show_help_popup: false, // Default to no help popup

            mouse_event: None,
        }
    }

    fn run(
        mut self,
        terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
        desc: &AppDesc,
    ) -> Result<()> {
        self.set_hard_focused_block(self.logs_block.id());

        let poll_interval = desc.poll_interval;

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
        if let Ok(Some(newer_file)) = self.check_for_newer_log_file() {
            self.switch_to_log_file(newer_file)?;
        }

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

    fn to_underlying_index(total: usize, visual_index: usize) -> usize {
        total.saturating_sub(1).saturating_sub(visual_index)
    }

    fn to_visual_index(total: usize, underlying_index: usize) -> usize {
        total.saturating_sub(1).saturating_sub(underlying_index)
    }

    fn check_for_newer_log_file(&self) -> Result<Option<PathBuf>> {
        let preview_log_dirs = file_finder::find_preview_log_dirs(&self.log_dir_path);
        match file_finder::find_latest_live_log(preview_log_dirs) {
            Ok(latest_file_path) => {
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

        let current_filter = self.filter_input.clone();
        let current_autoscroll = self.autoscroll;
        let current_detail_level = self.detail_level;

        self.log_file_path = new_file_path;
        self.last_len = 0;
        self.prev_meta = None;

        self.raw_logs.clear();
        self.displaying_logs = LogList::new(Vec::new());

        self.filter_input = current_filter;
        self.autoscroll = current_autoscroll;
        self.detail_level = current_detail_level;

        self.logs_block.set_scroll_position(0);
        self.logs_block.set_lines_count(0);
        self.details_block.set_scroll_position(0);

        self.selected_log_uuid = None;
        self.prev_selected_log_id = None;

        Ok(())
    }

    fn file_path_to_clickable_string(file_path: &Path) -> String {
        let clickable_string = file_path.display().to_string().replace(" ", "%20");
        format!("file://{}", clickable_string)
    }

    fn update_logs(&mut self) -> Result<()> {
        if !self.log_file_path.exists() {
            return Ok(());
        }

        let current_meta = match metadata::stat_path(&self.log_file_path) {
            Ok(m) => m,
            Err(_) => {
                return Ok(());
            }
        };

        if metadata::has_changed(&self.prev_meta, &current_meta) {
            if current_meta.len < self.last_len {
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
                        "Found {} new log items in {}",
                        new_items.len(),
                        Self::file_path_to_clickable_string(&self.log_file_path)
                    );
                    self.raw_logs.extend(new_items);

                    let filter_query = self.get_filter_query();
                    if filter_query.is_empty() {
                        self.displaying_logs = LogList::new(self.raw_logs.clone());
                    } else {
                        self.rebuild_filtered_list();
                    }

                    if previous_uuid.is_some() {
                        self.update_selection_by_uuid();
                    } else if self.autoscroll {
                        self.displaying_logs.select_first();
                        self.update_selected_uuid();
                    }

                    {
                        let new_items_count = self.displaying_logs.items.len();
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

        self.rebuild_filtered_list();

        if previous_uuid.is_some() {
            self.update_selection_by_uuid();
        } else if self.autoscroll {
            self.displaying_logs.select_first();
            self.update_selected_uuid();
        }

        {
            let new_total = self.displaying_logs.items.len();
            let mut pos = prev_scroll_pos;
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

    fn rebuild_filtered_list(&mut self) {
        let filter_query = self.get_filter_query();
        if filter_query.is_empty() {
            self.displaying_logs = LogList::new(self.raw_logs.clone());
        } else {
            let filtered_items: Vec<LogItem> = self
                .raw_logs
                .iter()
                .filter(|item| item.contains(filter_query))
                .cloned()
                .collect();
            self.displaying_logs = LogList::new(filtered_items);
        }
    }

    fn update_logs_scrollbar_state(&mut self) {
        let total = self.displaying_logs.items.len();

        {
            let max_top = total.saturating_sub(1);
            let pos = self.logs_block.get_scroll_position().min(max_top);
            self.logs_block.set_scroll_position(pos);

            self.logs_block.set_lines_count(total);
            self.logs_block.update_scrollbar_state(total, Some(pos));
        }
    }

    fn render_footer(&self, area: Rect, buf: &mut Buffer) -> Result<()> {
        let help_text = if !self.filter_input.is_empty() {
            self.filter_input.clone()
        } else {
            "Press ? for help | q: quit".to_string()
        };

        let paragraph = if self.filter_focused {
            // slightly lighter background when user can type
            Paragraph::new(help_text)
                .centered()
                .bg(theme::select_color_with_default_palette(
                    theme::PaletteIdx::C400,
                ))
        } else {
            Paragraph::new(help_text).centered()
        };

        paragraph.render(area, buf);
        Ok(())
    }

    fn render_help_popup(&self, area: Rect, buf: &mut Buffer) -> Result<()> {
        use ratatui::widgets::{Block, Borders, Clear};

        // center the popup
        let popup_area = Layout::vertical([
            Constraint::Fill(1),
            Constraint::Length(20),
            Constraint::Fill(1),
        ])
        .split(area)[1];

        let popup_area = Layout::horizontal([
            Constraint::Fill(1),
            Constraint::Length(60),
            Constraint::Fill(1),
        ])
        .split(popup_area)[1];

        // clear the area first
        Clear.render(popup_area, buf);

        let help_text = vec![
            Line::from(""),
            Line::from("Navigation:".bold()),
            Line::from("  j/k/↑/↓  - Move to prev/next log"),
            Line::from("  g/G      - Jump to top/bottom"),
            Line::from("  h/l/←/→  - Horizontal scroll"),
            Line::from(""),
            Line::from("Actions:".bold()),
            Line::from("  /        - Enter filter mode"),
            Line::from("  y        - Copy current log to clipboard"),
            Line::from("  c        - Clear all logs"),
            Line::from("  d        - Toggle debug logs panel"),
            Line::from("  w        - Toggle text wrapping"),
            Line::from("  [/]      - Decrease/increase detail level"),
            Line::from(""),
            Line::from("Focus:".bold()),
            Line::from("  1/2/3    - Focus on Logs/Details/Debug panel"),
            Line::from("  Shift+scroll - Horizontal scroll with mouse"),
            Line::from(""),
        ];

        let block = Block::default()
            .title("Help - Press ? / q / Esc to close")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme::TEXT_FG_COLOR));

        Paragraph::new(help_text)
            .block(block)
            .fg(theme::TEXT_FG_COLOR)
            .render(popup_area, buf);

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

        let title = if self.log_file_path.exists() {
            let filter_query = self.get_filter_query();
            let mut display_content = if filter_query.is_empty() {
                format!("[1]─Logs | {}", self.raw_logs.len())
            } else {
                format!(
                    "[1]─Logs | {} / {}",
                    self.displaying_logs.items.len(),
                    self.raw_logs.len()
                )
            };
            if self.autoscroll {
                display_content += " | Autoscrolling";
            }
            display_content
        } else {
            "[1]─Logs | Waiting for log files...".to_string()
        };
        self.logs_block.update_title(title);
        let logs_block_id = self.logs_block.id();

        let selected_index = self.displaying_logs.state.selected();
        let total_lines = self.displaying_logs.items.len();

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
            let log_item = &self.displaying_logs.items[item_idx];

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

        self.update_autoscroll_state();

        if selection_changed {
            self.update_selected_uuid();
        }

        Ok(())
    }

    fn render_details(&mut self, area: Rect, buf: &mut Buffer) -> Result<()> {
        self.last_details_area = Some(area);
        let details_block_id = self.details_block.id();
        let is_focused = self.get_display_focused_block() == Some(details_block_id);

        let should_hard_focus = if let Some(event) = self.mouse_event {
            let is_left_click = event.kind
                == crossterm::event::MouseEventKind::Up(crossterm::event::MouseButton::Left);
            let inner_area = self.details_block.build(false).inner(area);
            let is_within_bounds =
                inner_area.contains(ratatui::layout::Position::new(event.column, event.row));

            if event.kind == crossterm::event::MouseEventKind::Moved && is_within_bounds {
                self.set_soft_focused_block(details_block_id);
            }

            is_left_click && is_within_bounds
        } else {
            false
        };

        if should_hard_focus {
            self.set_hard_focused_block(details_block_id);
        }

        let [vertical_content_area, scrollbar_area] = Layout::horizontal([
            Constraint::Fill(1),   // Main content takes most space
            Constraint::Length(1), // Scrollbar is 1 character wide
        ])
        .margin(0)
        .areas(area);

        let (items, state) = (&self.displaying_logs.items, &self.displaying_logs.state);

        let (content, max_content_width) = if let Some(i) = state.selected() {
            let reversed_index = items.len().saturating_sub(1).saturating_sub(i);
            let item = &items[reversed_index];

            if self.prev_selected_log_id != Some(item.id) {
                self.prev_selected_log_id = Some(item.id);
                self.details_block.set_scroll_position(0);
                self.details_block.set_horizontal_scroll_position(0);
            }

            let mut content_lines = vec![
                Line::from(vec!["Time:   ".bold(), item.time.clone().into()]),
                Line::from(vec!["Level:  ".bold(), item.level.clone().into()]),
                Line::from(vec!["Origin: ".bold(), item.origin.clone().into()]),
                Line::from(vec!["Tag:    ".bold(), item.tag.clone().into()]),
                Line::from("Content:".bold()),
            ];
            let temp_content_rect = self
                .details_block
                .get_content_rect(vertical_content_area, is_focused);

            let wrapping_mode = if self.text_wrapping_enabled {
                WrappingMode::Wrapped
            } else {
                WrappingMode::Unwrapped
            };
            content_lines.extend(content_into_lines(
                &item.content,
                temp_content_rect.width,
                wrapping_mode,
            ));

            // Calculate max content width for horizontal scrolling
            let max_content_width = if self.text_wrapping_enabled {
                temp_content_rect.width as usize
            } else {
                let header_width = ["Time:   ", "Level:  ", "Origin: ", "Tag:    ", "Content:"]
                    .iter()
                    .map(|h| h.len())
                    .max()
                    .unwrap_or(0);
                let item_content_width = calculate_content_width(&item.content);
                header_width.max(item_content_width)
            };

            (content_lines, max_content_width)
        } else {
            if self.prev_selected_log_id.is_some() {
                self.prev_selected_log_id = None;
                self.details_block.set_scroll_position(0);
                self.details_block.set_horizontal_scroll_position(0);
                log::debug!("No log item selected - resetting details scroll position");
            }
            (
                vec![Line::from("Select a log item to see details...".italic())],
                0,
            )
        };

        // Determine if horizontal scrollbar is needed
        let temp_content_rect = self
            .details_block
            .get_content_rect(vertical_content_area, is_focused);
        let needs_horizontal_scrollbar = max_content_width > temp_content_rect.width as usize;

        // Always allocate space for horizontal scrollbar (consistent layout)
        let [content_area, horizontal_scrollbar_area] = Layout::vertical([
            Constraint::Fill(1),   // Main content
            Constraint::Length(1), // Horizontal scrollbar height
        ])
        .margin(0)
        .areas(vertical_content_area);

        let content_rect = self
            .details_block
            .get_content_rect(content_area, is_focused);
        self.details_block
            .update_horizontal_scrollbar_state(max_content_width, content_rect.width as usize);

        let lines_count = content.len();

        self.details_block.set_lines_count(lines_count);
        let scroll_position = self.details_block.get_scroll_position();
        self.details_block
            .update_scrollbar_state(lines_count, Some(scroll_position));

        let block = self.details_block.build(is_focused);
        let h_scroll = if needs_horizontal_scrollbar {
            self.details_block.get_horizontal_scroll_position() as u16
        } else {
            0
        };

        Paragraph::new(content)
            .block(block)
            .fg(theme::TEXT_FG_COLOR)
            .scroll((scroll_position as u16, h_scroll))
            .render(content_area, buf);

        let scrollbar = AppBlock::create_scrollbar(is_focused);

        StatefulWidget::render(
            scrollbar,
            scrollbar_area,
            buf,
            self.details_block.get_scrollbar_state(),
        );

        // Always render horizontal scrollbar area (track-only when not needed)
        let horizontal_scrollbar = if needs_horizontal_scrollbar {
            AppBlock::create_horizontal_scrollbar(is_focused)
        } else {
            AppBlock::create_horizontal_track_only(is_focused)
        };
        StatefulWidget::render(
            horizontal_scrollbar,
            horizontal_scrollbar_area,
            buf,
            self.details_block.get_horizontal_scrollbar_state(),
        );

        Ok(())
    }

    fn render_debug_logs(&mut self, area: Rect, buf: &mut Buffer) -> Result<()> {
        self.last_debug_area = Some(area);
        let debug_block_id = self.debug_block.id();
        let is_focused = self.get_display_focused_block() == Some(debug_block_id);

        let should_hard_focus = if let Some(event) = self.mouse_event {
            let is_left_click = event.kind
                == crossterm::event::MouseEventKind::Up(crossterm::event::MouseButton::Left);
            let inner_area = self.debug_block.build(false).inner(area);
            let is_within_bounds =
                inner_area.contains(ratatui::layout::Position::new(event.column, event.row));

            if event.kind == crossterm::event::MouseEventKind::Moved && is_within_bounds {
                self.set_soft_focused_block(debug_block_id);
            }

            is_left_click && is_within_bounds
        } else {
            false
        };

        if should_hard_focus {
            self.set_hard_focused_block(debug_block_id);
        }

        let [vertical_content_area, scrollbar_area] = Layout::horizontal([
            Constraint::Fill(1),   // Main content takes most space
            Constraint::Length(1), // Scrollbar is 1 character wide
        ])
        .margin(0)
        .areas(area);

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
                            theme::INFO_STYLE
                        };
                        Line::styled(log_entry.clone(), style)
                    })
                    .collect()
            }
        } else {
            vec![Line::from("Failed to read debug logs...".italic())]
        };

        // Calculate max content width for horizontal scrolling
        let max_content_width = if let Ok(logs) = self.debug_logs.lock() {
            logs.iter()
                .map(|log_entry| log_entry.chars().count())
                .max()
                .unwrap_or(0)
        } else {
            0
        };

        // Determine if horizontal scrollbar is needed
        let temp_content_rect = self
            .debug_block
            .get_content_rect(vertical_content_area, is_focused);
        let needs_horizontal_scrollbar = max_content_width > temp_content_rect.width as usize;

        // Always allocate space for horizontal scrollbar (consistent layout)
        let [content_area, horizontal_scrollbar_area] = Layout::vertical([
            Constraint::Fill(1),   // Main content
            Constraint::Length(1), // Horizontal scrollbar height
        ])
        .margin(0)
        .areas(vertical_content_area);

        let content_rect = self.debug_block.get_content_rect(content_area, is_focused);
        self.debug_block
            .update_horizontal_scrollbar_state(max_content_width, content_rect.width as usize);

        // The debug_logs_lines vector already contains properly wrapped lines
        let lines_count = debug_logs_lines.len();

        self.debug_block.set_lines_count(lines_count);
        let scroll_position = self.debug_block.get_scroll_position();
        self.debug_block
            .update_scrollbar_state(lines_count, Some(scroll_position));

        let _block = self.debug_block.build(is_focused);
        let h_scroll = if needs_horizontal_scrollbar {
            self.debug_block.get_horizontal_scroll_position() as u16
        } else {
            0
        };

        Paragraph::new(debug_logs_lines)
            .block(_block)
            .fg(theme::TEXT_FG_COLOR)
            .scroll((scroll_position as u16, h_scroll))
            .render(content_area, buf);

        let scrollbar = AppBlock::create_scrollbar(is_focused);

        StatefulWidget::render(
            scrollbar,
            scrollbar_area,
            buf,
            self.debug_block.get_scrollbar_state(),
        );

        // Always render horizontal scrollbar area (track-only when not needed)
        let horizontal_scrollbar = if needs_horizontal_scrollbar {
            AppBlock::create_horizontal_scrollbar(is_focused)
        } else {
            AppBlock::create_horizontal_track_only(is_focused)
        };
        StatefulWidget::render(
            horizontal_scrollbar,
            horizontal_scrollbar_area,
            buf,
            self.debug_block.get_horizontal_scrollbar_state(),
        );

        Ok(())
    }

    fn is_log_block_focused(&self) -> Result<bool> {
        Ok(self.get_display_focused_block() == Some(self.logs_block.id()))
    }

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

                let pad = if visible_height > 2 { 1 } else { 0 };

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
        self.autoscroll = self.logs_block.get_scroll_position() == 0;
    }

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

        self.ensure_selection_visible()?;
        self.update_logs_scrollbar_state();
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
            current_position.saturating_add(5).min(max_scroll)
        } else {
            current_position.saturating_sub(5)
        };

        block.set_horizontal_scroll_position(new_position);
        block.update_horizontal_scrollbar_state(content_width, viewport_width);

        Ok(())
    }

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

    fn make_yank_content(&self, item: &LogItem) -> String {
        format!(
            "# Formatted Log\n\n## Time:\n\n{}\n\n## Level:\n\n{}\n\n## Origin:\n\n{}\n\n## Tag:\n\n{}\n\n## Content:\n\n{}\n\n# Raw Log\n\n{}",
            item.time, item.level, item.origin, item.tag, item.content, item.raw_content
        )
    }

    fn yank_current_log(&self) -> Result<()> {
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

        log::debug!("Copied {} chars to clipboard", yank_content.len());

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
                KeyCode::Char('?') | KeyCode::Char('q') | KeyCode::Esc => {
                    self.show_help_popup = false;
                    return Ok(());
                }
                _ => return Ok(()),
            }
        }

        // handle filter input mode when focused
        if !self.filter_input.is_empty() && self.filter_focused {
            match key.code {
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
            KeyCode::Char('q') | KeyCode::Esc => {
                // if filter is active but not focused, clear it
                if !self.filter_input.is_empty() && !self.filter_focused {
                    self.filter_input.clear();
                    self.apply_filter();
                } else {
                    log::debug!("Exit key pressed");
                    self.is_exiting = true;
                }
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
                self.handle_log_item_scrolling(true, true)?;
                Ok(())
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.handle_log_item_scrolling(false, true)?;
                Ok(())
            }
            KeyCode::Char('g') => {
                self.displaying_logs.select_first();
                self.update_selected_uuid();
                self.ensure_selection_visible()?;
                self.update_logs_scrollbar_state();
                Ok(())
            }
            KeyCode::Char('G') => {
                self.displaying_logs.select_last();
                self.update_selected_uuid();
                self.ensure_selection_visible()?;
                self.update_logs_scrollbar_state();
                Ok(())
            }
            KeyCode::Char('/') => {
                self.filter_input = "/".to_string();
                self.filter_focused = true;
                Ok(())
            }
            KeyCode::Char('[') => {
                // decrease detail level (show less info) - non-circular
                if self.detail_level > 0 {
                    self.detail_level -= 1;
                }
                Ok(())
            }
            KeyCode::Char(']') => {
                // increase detail level (show more info) - non-circular
                if self.detail_level < 4 {
                    self.detail_level += 1;
                }
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
                Ok(())
            }
            KeyCode::Char('d') => {
                self.show_debug_logs = !self.show_debug_logs;
                log::debug!("Debug logs visibility toggled: {}", self.show_debug_logs);
                Ok(())
            }
            KeyCode::Char('h') | KeyCode::Left => {
                if let Some(focused_block) = self.get_display_focused_block() {
                    self.handle_horizontal_scrolling(focused_block, false)?;
                }
                Ok(())
            }
            KeyCode::Char('l') | KeyCode::Right => {
                if let Some(focused_block) = self.get_display_focused_block() {
                    self.handle_horizontal_scrolling(focused_block, true)?;
                }
                Ok(())
            }
            KeyCode::Char('?') => {
                self.show_help_popup = !self.show_help_popup;
                Ok(())
            }
            _ => Ok(()),
        }
    }

    fn set_hard_focused_block(&mut self, block_id: uuid::Uuid) {
        self.hard_focused_block_id = Some(block_id);
    }

    fn set_soft_focused_block(&mut self, block_id: uuid::Uuid) {
        if self.soft_focused_block_id != Some(block_id) {
            self.soft_focused_block_id = Some(block_id);
        }
    }

    fn get_display_focused_block(&self) -> Option<uuid::Uuid> {
        self.hard_focused_block_id.or(self.soft_focused_block_id)
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

    fn clear_event(&mut self) {
        self.mouse_event = None;
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
        if self.show_debug_logs {
            let [main, debug_area, footer_area] = Layout::vertical([
                Constraint::Fill(1),
                Constraint::Length(6),
                Constraint::Length(1),
            ])
            .areas(area);

            let [logs_area, details_area] =
                Layout::vertical([Constraint::Percentage(60), Constraint::Percentage(40)])
                    .areas(main);

            self.render_logs(logs_area, buf).unwrap();
            self.render_details(details_area, buf).unwrap();
            self.render_debug_logs(debug_area, buf).unwrap();
            self.render_footer(footer_area, buf).unwrap();
        } else {
            let [main, footer_area] =
                Layout::vertical([Constraint::Fill(1), Constraint::Length(1)]).areas(area);

            let [logs_area, details_area] =
                Layout::vertical([Constraint::Percentage(60), Constraint::Percentage(40)])
                    .areas(main);

            self.render_logs(logs_area, buf).unwrap();
            self.render_details(details_area, buf).unwrap();
            self.render_footer(footer_area, buf).unwrap();
        }

        // render help popup on top if visible
        if self.show_help_popup {
            self.render_help_popup(area, buf).unwrap();
        }

        self.clear_event();
    }
}
