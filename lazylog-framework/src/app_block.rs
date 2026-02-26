use ratatui::{
    layout::Rect,
    prelude::Stylize,
    style::{Color, Style},
    symbols::scrollbar,
    widgets::{
        Block, BorderType, Borders, Padding, Scrollbar, ScrollbarOrientation, ScrollbarState,
    },
};
use uuid::Uuid;

fn brighten_color(color: Color) -> Color {
    match color {
        Color::Rgb(r, g, b) => {
            let r = r.saturating_add(10).min(255);
            let g = g.saturating_add(10).min(255);
            let b = b.saturating_add(10).min(255);
            Color::Rgb(r, g, b)
        }
        Color::Gray => Color::Gray,
        c => c,
    }
}

pub fn get_border_color(focused: bool, mode_color: Color) -> Color {
    if focused {
        brighten_color(mode_color)
    } else {
        mode_color
    }
}

pub struct AppBlock {
    #[allow(dead_code)]
    id: Uuid,
    title: Option<String>,
    lines_count: usize,
    scroll_position: usize,
    scrollbar_state: ScrollbarState,
    padding: Option<Padding>,
    horizontal_scroll_position: usize,
    horizontal_scrollbar_state: ScrollbarState,
    content_width: usize,
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
            horizontal_scroll_position: 0,
            horizontal_scrollbar_state: ScrollbarState::default(),
            content_width: 0,
        }
    }

    pub fn set_title(mut self, title: impl Into<String>) -> Self {
        self.update_title(title);
        self
    }

    pub fn set_padding(mut self, padding: Padding) -> Self {
        self.padding = Some(padding);
        self
    }

    pub fn update_title(&mut self, title: impl Into<String>) {
        self.title = Some(format!("─{}", title.into()));
    }

    #[allow(dead_code)]
    pub fn id(&self) -> Uuid {
        self.id
    }

    pub fn build(&self, focused: bool, mode_color: Color) -> Block<'_> {
        let mut block = Block::default()
            .borders(Borders::TOP | Borders::LEFT)
            .border_type(BorderType::Rounded);

        let border_color = if focused {
            // brighten mode color for focused state (blend with white)
            brighten_color(mode_color)
        } else {
            mode_color
        };
        block = block.border_style(Style::new().fg(border_color));

        if let Some(title) = &self.title {
            let title_style = if focused {
                Style::new().bold()
            } else {
                Style::new()
            };
            block = block.title(
                ratatui::prelude::Line::from(title.as_str())
                    .style(title_style)
                    .left_aligned(),
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
            // when no items are present, set content_length to 1 to show a 100% height thumb
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

    pub fn set_horizontal_scroll_position(&mut self, position: usize) {
        self.horizontal_scroll_position = position;
    }

    pub fn get_horizontal_scroll_position(&self) -> usize {
        self.horizontal_scroll_position
    }

    pub fn get_content_width(&self) -> usize {
        self.content_width
    }

    pub fn update_horizontal_scrollbar_state(
        &mut self,
        content_width: usize,
        viewport_width: usize,
    ) {
        self.content_width = content_width;
        if content_width > viewport_width {
            let position = self
                .horizontal_scroll_position
                .min(content_width.saturating_sub(viewport_width));
            self.horizontal_scroll_position = position;
            self.horizontal_scrollbar_state = self
                .horizontal_scrollbar_state
                .content_length(content_width.saturating_sub(viewport_width))
                .position(position);
        } else {
            self.horizontal_scroll_position = 0;
            self.horizontal_scrollbar_state = self
                .horizontal_scrollbar_state
                .content_length(1)
                .position(0);
        }
    }

    pub fn get_horizontal_scrollbar_state(&mut self) -> &mut ScrollbarState {
        &mut self.horizontal_scrollbar_state
    }

    /// Creates a uniform scrollbar widget with consistent styling
    pub fn create_scrollbar(focused: bool, mode_color: Color) -> Scrollbar<'static> {
        let color = get_border_color(focused, mode_color);

        Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .symbols(scrollbar::VERTICAL)
            .style(Style::default().fg(color))
            .begin_symbol(Some("╮"))
            .end_symbol(Some("╯"))
            .track_symbol(Some("│"))
            .thumb_symbol("█")
    }

    /// Creates a horizontal scrollbar widget with consistent styling
    pub fn create_horizontal_scrollbar(focused: bool, mode_color: Color) -> Scrollbar<'static> {
        let color = get_border_color(focused, mode_color);

        Scrollbar::new(ScrollbarOrientation::HorizontalBottom)
            .symbols(scrollbar::HORIZONTAL)
            .style(Style::default().fg(color))
            .begin_symbol(Some("╰"))
            .end_symbol(Some("─"))
            .track_symbol(Some("─"))
            .thumb_symbol("🬋")
    }

    /// Creates a horizontal scrollbar that only shows track (no thumb)
    pub fn create_horizontal_track_only(focused: bool, mode_color: Color) -> Scrollbar<'static> {
        let color = get_border_color(focused, mode_color);

        Scrollbar::new(ScrollbarOrientation::HorizontalBottom)
            .symbols(scrollbar::HORIZONTAL)
            .style(Style::default().fg(color))
            .begin_symbol(Some("╰"))
            .end_symbol(Some("─"))
            .track_symbol(Some("─"))
            .thumb_symbol("─") // Same as track, so thumb is invisible
    }

    /// Returns the content rectangle accounting for block borders
    pub fn get_content_rect(&self, area: Rect, focused: bool, mode_color: Color) -> Rect {
        self.build(focused, mode_color).inner(area)
    }
}

impl Default for AppBlock {
    fn default() -> Self {
        Self::new()
    }
}
