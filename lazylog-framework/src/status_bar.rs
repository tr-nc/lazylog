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

    /// Create a display event with the given text, duration, and optional style
    pub fn create(
        text: String,
        duration: Duration,
        style: Option<Style>,
        default_style: Style,
    ) -> Self {
        let event_style = style.unwrap_or(default_style);
        Self::new(text, duration, event_style)
    }

    /// Check if a display event has expired and return None if so
    pub fn check_and_clear(event: Option<Self>) -> Option<Self> {
        match event {
            Some(e) if e.is_expired() => None,
            other => other,
        }
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

    #[allow(dead_code)]
    pub fn set_bg(mut self, color: Color) -> Self {
        self.bg_color = Some(color);
        self
    }

    #[allow(dead_code)]
    pub fn set_left_fg(mut self, color: Color) -> Self {
        self.left_fg = Some(color);
        self
    }

    #[allow(dead_code)]
    pub fn set_mid_fg(mut self, color: Color) -> Self {
        self.mid_fg = Some(color);
        self
    }

    #[allow(dead_code)]
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

        // build spans with individual foreground colors
        let mut spans = vec![];

        // left span
        let left_span = if let Some(fg) = self.left_fg {
            Span::styled(self.left, Style::default().fg(fg))
        } else {
            Span::raw(self.left)
        };
        spans.push(left_span);
        spans.push(Span::raw(" ".repeat(left_to_mid_padding)));

        // mid span
        let mid_span = if let Some(fg) = self.mid_fg {
            Span::styled(self.mid, Style::default().fg(fg))
        } else {
            Span::raw(self.mid)
        };
        spans.push(mid_span);
        spans.push(Span::raw(" ".repeat(mid_to_right_padding)));

        // right span (dimmed by default if no color specified)
        let right_span = if let Some(fg) = self.right_fg {
            Span::styled(self.right, Style::default().fg(fg))
        } else {
            Span::raw(self.right)
        };
        spans.push(right_span);

        let line = Line::from(spans);

        // apply uniform background color and custom style
        let mut base_style = Style::default();
        if let Some(bg) = self.bg_color {
            base_style = base_style.bg(bg);
        }
        if let Some(custom_style) = self.style {
            base_style = base_style.patch(custom_style);
        }

        let paragraph = Paragraph::new(line).style(base_style);
        paragraph.render(area, buf);
    }
}
