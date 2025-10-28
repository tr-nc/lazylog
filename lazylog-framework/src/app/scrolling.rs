use super::{App, HORIZONTAL_SCROLL_STEP};
use anyhow::Result;
use ratatui::prelude::*;

impl App {
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
}
