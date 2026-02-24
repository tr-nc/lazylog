use super::{App, HORIZONTAL_SCROLL_STEP};
use anyhow::Result;
use ratatui::prelude::*;

impl App {
    /// Clamps the logs block scroll position to prevent scrolling out of bounds
    /// When viewport height changes, preserves the bottom-most visible item position
    pub(super) fn clamp_logs_scroll_position(&mut self) -> Result<()> {
        let lines_count = self.logs_block.get_lines_count();
        let current_position = self.logs_block.get_scroll_position();

        // calculate viewport height to determine max scroll position
        let viewport_height = if let Some(area) = self.last_logs_area {
            let is_focused = self.is_log_block_focused().unwrap_or(false);
            let [main_content_area, _] =
                Layout::horizontal([Constraint::Fill(1), Constraint::Length(1)])
                    .margin(0)
                    .areas(area);

            let [content_area, _] = Layout::vertical([Constraint::Fill(1), Constraint::Length(1)])
                .margin(0)
                .areas(main_content_area);

            let inner_area = self.logs_block.get_content_rect(content_area, is_focused);
            inner_area.height as usize
        } else {
            1 // fallback if area not yet rendered
        };

        // detect viewport height change and preserve bottom item position
        let adjusted_position = if let Some(prev_height) = self.last_logs_viewport_height {
            if prev_height != viewport_height {
                // viewport height changed - calculate which item was at the bottom
                let prev_bottom_item = current_position
                    .saturating_add(prev_height)
                    .saturating_sub(1)
                    .min(lines_count.saturating_sub(1));

                // adjust scroll position to keep that item at the bottom
                let new_position = prev_bottom_item
                    .saturating_add(1)
                    .saturating_sub(viewport_height);

                log::debug!(
                    "Viewport height changed: {} -> {}, adjusting scroll: {} -> {} (preserving bottom item: {})",
                    prev_height,
                    viewport_height,
                    current_position,
                    new_position,
                    prev_bottom_item
                );

                new_position
            } else {
                current_position
            }
        } else {
            current_position
        };

        // update stored viewport height
        self.last_logs_viewport_height = Some(viewport_height);

        // max scroll position: stop when last item is fully displayed
        let max_scroll = lines_count.saturating_sub(viewport_height);

        // clamp adjusted position to valid range
        let clamped_position = adjusted_position.min(max_scroll);

        if clamped_position != current_position {
            self.logs_block.set_scroll_position(clamped_position);
        }

        self.logs_block
            .update_scrollbar_state(lines_count, Some(clamped_position));

        Ok(())
    }

    pub(super) fn handle_log_item_scrolling(
        &mut self,
        move_next: bool,
        circular: bool,
    ) -> Result<()> {
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

    pub(super) fn handle_logs_view_scrolling(&mut self, move_down: bool) -> Result<()> {
        let lines_count = self.logs_block.get_lines_count();
        let current_position = self.logs_block.get_scroll_position();

        // calculate viewport height to determine max scroll position
        let viewport_height = if let Some(area) = self.last_logs_area {
            let is_focused = self.is_log_block_focused().unwrap_or(false);
            let [main_content_area, _] =
                Layout::horizontal([Constraint::Fill(1), Constraint::Length(1)])
                    .margin(0)
                    .areas(area);

            let [content_area, _] = Layout::vertical([Constraint::Fill(1), Constraint::Length(1)])
                .margin(0)
                .areas(main_content_area);

            let inner_area = self.logs_block.get_content_rect(content_area, is_focused);
            inner_area.height as usize
        } else {
            1 // fallback if area not yet rendered
        };

        // max scroll position: stop when last item is fully displayed
        let max_scroll = lines_count.saturating_sub(viewport_height);

        let new_position = if move_down {
            if current_position >= max_scroll {
                current_position // stop when last item is displayed
            } else {
                current_position.saturating_add(1)
            }
        } else {
            current_position.saturating_sub(1)
        };

        self.logs_block.set_scroll_position(new_position);

        // use the clamping method to ensure bounds are respected
        self.clamp_logs_scroll_position()?;

        Ok(())
    }

    pub(super) fn handle_details_block_scrolling(&mut self, move_next: bool) -> Result<()> {
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
                .min(last_index) // don't exceed bottom
        } else {
            current_position.saturating_sub(1)
        };

        self.details_block.set_scroll_position(new_position);
        self.details_block
            .update_scrollbar_state(lines_count, Some(new_position));

        Ok(())
    }

    pub(super) fn handle_debug_logs_scrolling(&mut self, move_next: bool) -> Result<()> {
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

    pub(super) fn handle_horizontal_scrolling(
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

    pub(super) fn handle_vertical_scrollbar_drag(
        &mut self,
        block_id: uuid::Uuid,
        mouse_row: u16,
    ) -> Result<()> {
        if block_id == self.logs_block.id() {
            let Some(area) = self.last_logs_area else {
                return Ok(());
            };

            let [content_area, scrollbar_area] =
                Layout::horizontal([Constraint::Fill(1), Constraint::Length(1)])
                    .margin(0)
                    .areas(area);

            let track_height = scrollbar_area.height.saturating_sub(1);
            let relative_row = mouse_row.saturating_sub(scrollbar_area.y).min(track_height);
            let track_height = track_height as usize;

            let lines_count = self.logs_block.get_lines_count();
            if lines_count == 0 {
                self.logs_block.set_scroll_position(0);
                self.logs_block.update_scrollbar_state(0, Some(0));
                return Ok(());
            }

            let [main_content_area, _horizontal_scrollbar_area] =
                Layout::vertical([Constraint::Fill(1), Constraint::Length(1)])
                    .margin(0)
                    .areas(content_area);

            let is_focused = self.get_display_focused_block() == self.logs_block.id();
            let inner_area = self
                .logs_block
                .get_content_rect(main_content_area, is_focused);
            let viewport_height = inner_area.height as usize;
            let max_scroll = lines_count.saturating_sub(viewport_height);

            let new_position = if track_height == 0 || max_scroll == 0 {
                0
            } else {
                (relative_row as usize * max_scroll) / track_height
            };

            self.logs_block.set_scroll_position(new_position);
            self.logs_block
                .update_scrollbar_state(max_scroll, Some(new_position));
            return Ok(());
        }

        if block_id == self.details_block.id() {
            let Some(area) = self.last_details_area else {
                return Ok(());
            };

            let [_content_area, scrollbar_area] =
                Layout::horizontal([Constraint::Fill(1), Constraint::Length(1)])
                    .margin(0)
                    .areas(area);

            let track_height = scrollbar_area.height.saturating_sub(1);
            let relative_row = mouse_row.saturating_sub(scrollbar_area.y).min(track_height);
            let track_height = track_height as usize;

            let lines_count = self.details_block.get_lines_count();
            let max_scroll = lines_count.saturating_sub(1);
            let new_position = if track_height == 0 || max_scroll == 0 {
                0
            } else {
                (relative_row as usize * max_scroll) / track_height
            };

            self.details_block.set_scroll_position(new_position);
            self.details_block
                .update_scrollbar_state(lines_count, Some(new_position));
        }

        Ok(())
    }
}
