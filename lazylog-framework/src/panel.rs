use ratatui::{
    prelude::{Line, Stylize},
    style::{Color, Style},
    widgets::{Block, BorderType, Borders, Padding},
};

fn brighten_color(color: Color) -> Color {
    match color {
        Color::Rgb(r, g, b) => Color::Rgb(
            r.saturating_add(10),
            g.saturating_add(10),
            b.saturating_add(10),
        ),
        Color::Gray => Color::Gray,
        color => color,
    }
}

pub fn border_color(focused: bool, mode_color: Color) -> Color {
    if focused {
        brighten_color(mode_color)
    } else {
        mode_color
    }
}

/// Shared visual shell for Lazylog panels.
pub struct Panel<'a> {
    title: Option<Line<'a>>,
    borders: Borders,
    padding: Option<Padding>,
    focused: bool,
    mode_color: Color,
}

impl<'a> Panel<'a> {
    pub fn new(mode_color: Color) -> Self {
        Self {
            title: None,
            borders: Borders::ALL,
            padding: None,
            focused: false,
            mode_color,
        }
    }

    pub fn title(mut self, title: Line<'a>) -> Self {
        self.title = Some(title);
        self
    }

    pub fn borders(mut self, borders: Borders) -> Self {
        self.borders = borders;
        self
    }

    pub fn padding(mut self, padding: Padding) -> Self {
        self.padding = Some(padding);
        self
    }

    pub fn focused(mut self, focused: bool) -> Self {
        self.focused = focused;
        self
    }

    pub fn build(self) -> Block<'a> {
        let mut block = Block::default()
            .borders(self.borders)
            .border_type(BorderType::Rounded)
            .border_style(Style::new().fg(border_color(self.focused, self.mode_color)));

        if let Some(title) = self.title {
            block = block.title(if self.focused {
                title.style(Style::new().bold()).left_aligned()
            } else {
                title.left_aligned()
            });
        }
        if let Some(padding) = self.padding {
            block = block.padding(padding);
        }
        block
    }
}
