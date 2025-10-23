use ratatui::{
    prelude::*,
    widgets::{Paragraph, Widget},
};
use std::time::{Duration, Instant};

pub struct DisplayEvent {
    pub text: String,
    pub duration: Duration,
    pub start_time: Instant,
    pub style: Style,
}

impl DisplayEvent {
    pub fn new(text: String, duration: Duration, style: Style) -> Self {
        Self {
            text,
            duration,
            start_time: Instant::now(),
            style,
        }
    }

    pub fn is_expired(&self) -> bool {
        self.start_time.elapsed() >= self.duration
    }
}

pub struct StatusBar {
    left: String,
    mid: String,
    right: String,
    bg_color: Option<Color>,
    left_fg: Option<Color>,
    mid_fg: Option<Color>,
    right_fg: Option<Color>,
    style: Option<Style>,
}

impl StatusBar {
    pub fn new() -> Self {
        Self {
            left: String::new(),
            mid: String::new(),
            right: String::new(),
            bg_color: None,
            left_fg: None,
            mid_fg: None,
            right_fg: None,
            style: None,
        }
    }

    pub fn set_left(mut self, text: String) -> Self {
        self.left = text;
        self
    }

    pub fn set_mid(mut self, text: String) -> Self {
        self.mid = text;
        self
    }

    pub fn set_right(mut self, text: String) -> Self {
        self.right = text;
        self
    }

    pub fn set_bg(mut self, color: Color) -> Self {
        self.bg_color = Some(color);
        self
    }

    pub fn set_left_fg(mut self, color: Color) -> Self {
        self.left_fg = Some(color);
        self
    }

    pub fn set_mid_fg(mut self, color: Color) -> Self {
        self.mid_fg = Some(color);
        self
    }

    pub fn set_right_fg(mut self, color: Color) -> Self {
        self.right_fg = Some(color);
        self
    }

    pub fn set_style(mut self, style: Style) -> Self {
        self.style = Some(style);
        self
    }

    pub fn render(self, area: Rect, buf: &mut Buffer) {
        let total_width = area.width as usize;
        let left_len = self.left.chars().count();
        let mid_len = self.mid.chars().count();
        let right_len = self.right.chars().count();

        // calculate where middle text should start to be truly centered
        let mid_center_start = total_width.saturating_sub(mid_len) / 2;

        // calculate padding from left content to centered middle
        let left_to_mid_padding = mid_center_start.saturating_sub(left_len);

        // calculate where middle text ends
        let mid_end = mid_center_start.saturating_add(mid_len);

        // calculate where right content should start (from the right edge)
        let right_start = total_width.saturating_sub(right_len);

        // calculate padding from middle to right
        let mid_to_right_padding = right_start.saturating_sub(mid_end);

        let line = Line::from(vec![
            self.left.into(),
            " ".repeat(left_to_mid_padding).into(),
            self.mid.into(),
            " ".repeat(mid_to_right_padding).into(),
            self.right.dim().into(),
        ]);

        let paragraph = if let Some(style) = self.style {
            Paragraph::new(line).style(style)
        } else {
            Paragraph::new(line)
        };

        paragraph.render(area, buf);
    }
}
