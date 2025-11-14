use super::{App, HELP_POPUP_WIDTH, ScrollableBlockType};
use crate::{
    app_block::AppBlock,
    content_line_maker::{WrappingMode, calculate_content_width, content_into_lines},
    status_bar::StatusBar,
    theme,
};
use anyhow::Result;
use ratatui::{
    prelude::*,
    widgets::{Paragraph, StatefulWidget, Widget},
};

/// helper function to highlight filter matches in text
/// splits text into spans, applying bold & underlined style to matching parts
fn create_highlighted_line(text: &str, filter_query: &str, base_style: Style) -> Line<'static> {
    if filter_query.is_empty() {
        return Line::styled(text.to_string(), base_style);
    }

    let text_lower = text.to_lowercase();
    let query_lower = filter_query.to_lowercase();
    let mut spans = Vec::new();
    let mut last_pos = 0;

    // find all occurrences of the filter query
    while let Some(match_pos) = text_lower[last_pos..].find(&query_lower) {
        let absolute_pos = last_pos + match_pos;

        // add non-matching part before the match
        if last_pos < absolute_pos {
            spans.push(Span::styled(
                text[last_pos..absolute_pos].to_string(),
                base_style,
            ));
        }

        // add matching part with bold & underlined
        let match_end = absolute_pos + filter_query.len();
        spans.push(Span::styled(
            text[absolute_pos..match_end].to_string(),
            base_style.add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
        ));

        last_pos = match_end;
    }

    // add remaining text after last match
    if last_pos < text.len() {
        spans.push(Span::styled(text[last_pos..].to_string(), base_style));
    }

    Line::from(spans)
}

impl App {
    pub(super) fn render_footer(&self, area: Rect, buf: &mut Buffer) -> Result<()> {
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

    pub(super) fn render_help_popup(&self, area: Rect, buf: &mut Buffer) -> Result<()> {
        use ratatui::widgets::{Block, Borders, Clear};

        let help_text = vec![
            Line::from("Navigation:".bold()),
            Line::from("  j/k/↑/↓  - Move to prev/next log"),
            Line::from("  d        - Jump to bottom (latest log)"),
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
    pub(super) fn render_scrollable_block(
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

    pub(super) fn render_logs(&mut self, area: Rect, buf: &mut Buffer) -> Result<()> {
        self.last_logs_area = Some(area);

        // clamp scroll position with fresh area (handles resize)
        let _ = self.clamp_logs_scroll_position();

        let [content_area, scrollbar_area] = Layout::horizontal([
            Constraint::Fill(1),   // Main content takes most space
            Constraint::Length(1), // Scrollbar is 1 character wide
        ])
        .margin(0)
        .areas(area);

        let is_log_focused = self.is_log_block_focused().unwrap_or(false);

        // clone filter_query early to avoid borrow checker issues
        let filter_query = self.get_filter_query().to_string();
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
        let scroll_position = logs_block.get_scroll_position();

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
            // get the index into raw_logs from displaying_logs
            let raw_idx = self.displaying_logs.get(i).unwrap();
            let log_item = &self.raw_logs[raw_idx];

            let detail_text = self.parser.format_preview(log_item, self.detail_level);
            let level = log_item.get_metadata("level").unwrap_or("").to_uppercase();
            let level_style = match level.as_str() {
                "ERROR" => theme::ERROR_STYLE,
                "WARNING" | "WARN" => theme::WARN_STYLE,
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

            let truncated_text = truncated_line.to_string();

            // apply highlighting if filter is active
            let final_line = if !filter_query.is_empty() {
                let highlighted_line =
                    create_highlighted_line(&truncated_text, &filter_query, final_style);

                // add padding for selected items
                if is_selected {
                    let padded_text = format!("{:<width$}", truncated_text, width = content_width);
                    // re-apply highlighting to padded text
                    create_highlighted_line(&padded_text, &filter_query, final_style)
                } else {
                    highlighted_line
                }
            } else {
                let padded_text = if is_selected {
                    format!("{:<width$}", truncated_text, width = content_width)
                } else {
                    truncated_text
                };
                Line::styled(padded_text, final_style)
            };

            content_lines.push(final_line);
        }

        // Update horizontal scrollbar state
        logs_block.update_horizontal_scrollbar_state(max_content_width, content_width);

        let logs_block = &mut self.logs_block;
        logs_block.set_lines_count(total_lines);

        // this remapping is because the scrolling behavior of the LOGS block cannot exceed the last row
        // that is, the last row is can only be scrolled to the bottom, not any further. unlike other blocks
        let scrollbar_content_length = total_lines.saturating_sub(visible_height);
        logs_block.update_scrollbar_state(scrollbar_content_length, Some(scroll_position));

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

    pub(super) fn render_details(&mut self, area: Rect, buf: &mut Buffer) -> Result<()> {
        // handle prev_selected_log_id state management and scroll reset
        let (indices, state) = (&self.displaying_logs.indices, &self.displaying_logs.state);

        if let Some(i) = state.selected() {
            let raw_idx = indices[i];
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
                let raw_idx = indices[i];
                let item = &self.raw_logs[raw_idx];

                Some((
                    item.time.clone(),
                    item.metadata.clone(),
                    item.content.clone(),
                ))
            } else {
                None
            }
        };

        let text_wrapping_enabled = self.text_wrapping_enabled;

        // generate content using the cloned data
        let (content, max_content_width) = if let Some((time, metadata, item_content)) =
            &selected_item
        {
            // start with time field
            let mut content_lines = vec![Line::from(vec!["Time: ".bold(), time.clone().into()])];

            // define preferred display order for common metadata fields
            let preferred_order = ["level", "origin", "tag"];

            // add metadata fields in preferred order if they exist
            for key in &preferred_order {
                if let Some(value) = metadata.get(*key) {
                    let label = format!(
                        "{}: ",
                        key.chars().next().unwrap().to_uppercase().to_string() + &key[1..]
                    );
                    content_lines.push(Line::from(vec![label.bold(), value.clone().into()]));
                }
            }

            // add any other metadata fields not in the preferred order
            for (key, value) in metadata.iter() {
                if !preferred_order.contains(&key.as_str()) {
                    let label = format!(
                        "{}: ",
                        key.chars().next().unwrap().to_uppercase().to_string() + &key[1..]
                    );
                    content_lines.push(Line::from(vec![label.bold(), value.clone().into()]));
                }
            }

            // add content field
            content_lines.push(Line::from("Content:".bold()));

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
                // calculate header widths dynamically based on actual fields
                let mut header_widths = vec!["Time: ".len()];
                for key in &preferred_order {
                    if metadata.contains_key(*key) {
                        let label_len = key.len() + 2; // +2 for ": "
                        header_widths.push(label_len);
                    }
                }
                for key in metadata.keys() {
                    if !preferred_order.contains(&key.as_str()) {
                        let label_len = key.len() + 2; // +2 for ": "
                        header_widths.push(label_len);
                    }
                }
                header_widths.push("Content:".len());

                let header_width = header_widths.iter().max().copied().unwrap_or(0);
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

    pub(super) fn render_debug_logs(&mut self, area: Rect, buf: &mut Buffer) -> Result<()> {
        // generate content for the debug logs block
        let debug_logs_lines = if let Ok(logs) = self.debug_logs.lock() {
            if logs.is_empty() {
                vec![Line::from("No debug logs...".italic())]
            } else {
                logs.iter()
                    .rev() // show most recent first
                    .map(|log_entry| {
                        let log_upper = log_entry.to_uppercase();
                        let style = if log_upper.contains("ERROR") {
                            theme::ERROR_STYLE
                        } else if log_upper.contains("WARNING") || log_upper.contains("WARN") {
                            theme::WARN_STYLE
                        } else if log_upper.contains("DEBUG") {
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
