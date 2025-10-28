use super::{App, SCROLL_PAD};
use anyhow::Result;
use ratatui::prelude::*;

impl App {
    pub(super) fn ensure_selection_visible(&mut self) -> Result<()> {
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

    pub(super) fn update_autoscroll_state(&mut self) {
        self.autoscroll = self.logs_block.get_scroll_position() == 0;
    }

    /// Update the UI after manually changing selection
    /// This ensures the selection is visible, disables autoscroll, and updates scrollbar
    pub(super) fn after_selection_change(&mut self) -> Result<()> {
        self.ensure_selection_visible()?;
        self.autoscroll = false;
        self.update_logs_scrollbar_state();
        Ok(())
    }

    /// Find the index of a log item by its UUID
    pub(super) fn find_log_by_uuid(&self, uuid: &uuid::Uuid) -> Option<usize> {
        self.displaying_logs
            .indices
            .iter()
            .position(|&raw_idx| &self.raw_logs[raw_idx].id == uuid)
    }

    /// Update the selection based on the currently tracked UUID
    pub(super) fn update_selection_by_uuid(&mut self) {
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
    pub(super) fn update_selected_uuid(&mut self) {
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
