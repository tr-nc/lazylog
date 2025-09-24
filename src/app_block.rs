use crossterm::event::{MouseEvent, MouseEventKind};
use ratatui::{
    layout::Rect,
    prelude::Stylize,
    style::{Style, palette},
    symbols::scrollbar,
    widgets::{
        Block, BorderType, Borders, Padding, Scrollbar, ScrollbarOrientation, ScrollbarState,
    },
};
use uuid::Uuid;

pub struct AppBlock {
    #[allow(dead_code)]
    id: Uuid,
    title: Option<String>,
    lines_count: usize,
    scroll_position: usize,
    scrollbar_state: ScrollbarState,
    padding: Option<Padding>,
}

impl AppBlock {
    pub fn new() -> Self {
        Self {
            id: Uuid::new_v4(),
            title: None,
            lines_count: 0,
            scroll_position: 0,
            scrollbar_state: ScrollbarState::default(),
            padding: None,
        }
    }

    pub fn set_title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    pub fn set_padding(mut self, padding: Padding) -> Self {
        self.padding = Some(padding);
        self
    }

    pub fn update_title(&mut self, title: impl Into<String>) {
        self.title = Some(title.into());
    }

    #[allow(dead_code)]
    pub fn id(&self) -> Uuid {
        self.id
    }

    pub fn build(&self, focused: bool) -> Block<'_> {
        let mut block = Block::default()
            .borders(Borders::TOP | Borders::LEFT)
            .border_type(BorderType::Rounded);

        if focused {
            block =
                block.border_style(Style::new().fg(ratatui::style::palette::tailwind::ZINC.c100));
        } else {
            block =
                block.border_style(Style::new().fg(ratatui::style::palette::tailwind::ZINC.c600));
        }

        if let Some(title) = &self.title {
            let title_style = if focused {
                Style::new().bold().underlined()
            } else {
                Style::new()
            };
            block = block.title(
                ratatui::prelude::Line::from(title.as_str())
                    .style(title_style)
                    .centered(),
            );
        }

        if let Some(padding) = self.padding {
            block = block.padding(padding);
        }

        block
    }

    pub fn update_scrollbar_state(&mut self, total_items: usize, selected_index: Option<usize>) {
        if total_items > 0 {
            let position = selected_index.unwrap_or(0);
            self.scrollbar_state = self
                .scrollbar_state
                .content_length(total_items)
                .position(position);
        } else {
            // When no items are present, set content_length to 1 to show a 100% height thumb
            self.scrollbar_state = self.scrollbar_state.content_length(1).position(0);
        }
    }

    pub fn set_lines_count(&mut self, lines_count: usize) {
        self.lines_count = lines_count;
    }

    pub fn get_lines_count(&self) -> usize {
        self.lines_count
    }

    pub fn set_scroll_position(&mut self, scroll_position: usize) {
        self.scroll_position = scroll_position;
    }

    pub fn get_scroll_position(&self) -> usize {
        self.scroll_position
    }

    pub fn get_scrollbar_state(&mut self) -> &mut ScrollbarState {
        &mut self.scrollbar_state
    }

    /// Creates a uniform scrollbar widget with consistent styling
    pub fn create_scrollbar(focused: bool) -> Scrollbar<'static> {
        let color = if focused {
            palette::tailwind::ZINC.c100
        } else {
            palette::tailwind::ZINC.c600
        };

        Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .symbols(scrollbar::VERTICAL)
            .style(Style::default().fg(color))
            .begin_symbol(Some("╮"))
            .end_symbol(None)
            .track_symbol(Some("│"))
    }

    /// Returns the content rectangle accounting for block borders
    pub fn get_content_rect(&self, area: Rect, focused: bool) -> Rect {
        self.build(focused).inner(area)
    }

    pub fn handle_mouse_event(
        &self,
        _event: &MouseEvent,
        area: Rect,
        mouse_event: Option<&MouseEvent>,
    ) -> bool {
        if let Some(mouse_event) = mouse_event {
            let inner_area = self.build(false).inner(area);
            let is_hovering = inner_area.contains(ratatui::layout::Position::new(
                mouse_event.column,
                mouse_event.row,
            ));

            // Handle hover focus - return true if mouse is hovering over this block
            if is_hovering && mouse_event.kind == MouseEventKind::Moved {
                return true;
            }
        }
        false
    }
}

impl Default for AppBlock {
    fn default() -> Self {
        Self::new()
    }
}
