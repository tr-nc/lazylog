use super::{App, DISPLAY_EVENT_DURATION_MS};
use crate::provider::{decrement_detail_level, increment_detail_level};
use anyhow::Result;
use arboard::Clipboard;
use crossterm::event::{self, KeyCode, KeyEvent, KeyEventKind, MouseEvent, MouseEventKind};
use ratatui::prelude::*;
use std::time::Duration;

impl App {
    pub(super) fn handle_mouse_event(&mut self, mouse: &MouseEvent) -> Result<()> {
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

    pub(super) fn yank_current_log(&mut self) -> Result<()> {
        let (indices, state) = (&self.displaying_logs.indices, &self.displaying_logs.state);

        let Some(i) = state.selected() else {
            log::debug!("No log item selected for yanking");
            return Ok(());
        };

        // access items in natural order
        let raw_idx = indices[i];
        let item = &self.raw_logs[raw_idx];

        let mut clipboard = Clipboard::new()?;
        let yank_content = self.parser.make_yank_content(item);
        clipboard.set_text(&yank_content)?;

        log::debug!("Copied {} chars to clipboard", yank_content.len());

        self.set_display_event(
            "Selected log copied to clipboard".to_string(),
            Duration::from_millis(DISPLAY_EVENT_DURATION_MS),
            None, // use default style
        );

        Ok(())
    }

    pub(super) fn yank_all_displayed_logs(&mut self) -> Result<()> {
        let indices = &self.displaying_logs.indices;

        if indices.is_empty() {
            log::debug!("No log items to yank");
            return Ok(());
        }

        // collect all displayed log items with blank line separator
        let mut yank_contents = Vec::new();
        for &raw_idx in indices.iter() {
            let item = &self.raw_logs[raw_idx];
            yank_contents.push(self.parser.make_yank_content(item));
        }

        let combined_content = yank_contents.join("\n\n");

        let mut clipboard = Clipboard::new()?;
        clipboard.set_text(&combined_content)?;

        log::debug!(
            "Copied {} log items ({} chars) to clipboard",
            yank_contents.len(),
            combined_content.len()
        );

        self.set_display_event(
            format!("{} logs copied to clipboard", yank_contents.len()),
            Duration::from_millis(DISPLAY_EVENT_DURATION_MS),
            None, // use default style
        );

        Ok(())
    }

    pub(super) fn clear_logs(&mut self) {
        self.raw_logs.clear();
        self.filter_engine.reset();
        self.apply_filter();
    }

    pub(super) fn handle_key(&mut self, key: KeyEvent) -> Result<()> {
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
                self.detail_level = decrement_detail_level(self.detail_level);
                // reset filter cache since preview text changes
                self.filter_engine.reset();
                self.rebuild_filtered_list();
                self.update_selection_by_uuid();
                // notify user of detail level change
                self.set_display_event(
                    format!("Detail level: {}", self.detail_level),
                    Duration::from_millis(DISPLAY_EVENT_DURATION_MS),
                    None,
                );
                Ok(())
            }
            KeyCode::Char(']') => {
                // increase detail level (show more info) - non-circular
                let max = self.parser.max_detail_level();
                self.detail_level = increment_detail_level(self.detail_level, max);
                // reset filter cache since preview text changes
                self.filter_engine.reset();
                self.rebuild_filtered_list();
                self.update_selection_by_uuid();
                // notify user of detail level change
                self.set_display_event(
                    format!("Detail level: {}", self.detail_level),
                    Duration::from_millis(DISPLAY_EVENT_DURATION_MS),
                    None,
                );
                Ok(())
            }
            KeyCode::Char('y') => {
                if let Err(e) = self.yank_current_log() {
                    log::debug!("Failed to yank log content: {}", e);
                }
                Ok(())
            }
            KeyCode::Char('a') => {
                if let Err(e) = self.yank_all_displayed_logs() {
                    log::debug!("Failed to yank all displayed logs: {}", e);
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
            KeyCode::Char('b') => {
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
            KeyCode::Char('d') => {
                // select the last log item (go to bottom - newest)
                self.displaying_logs.select_last();
                self.update_selected_uuid();

                // scroll to bottom (stop when last item is fully displayed)
                let total_items = self.displaying_logs.len();

                // calculate viewport height to determine max scroll position
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

                let max_scroll = total_items.saturating_sub(viewport_height);
                self.logs_block.set_scroll_position(max_scroll);
                // force autoscroll to be true so that we don't wait for the next render to update the scrollbar state
                // waiting for the next render may cause new logs arrive beforehand, thus the view is not at the bottom
                self.update_autoscroll_state();
                self.update_logs_scrollbar_state();
                Ok(())
            }
            _ => Ok(()),
        }
    }
}
