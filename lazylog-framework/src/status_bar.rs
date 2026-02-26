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

    pub fn create(
        text: String,
        duration: Duration,
        style: Option<Style>,
        default_style: Style,
    ) -> Self {
        let event_style = style.unwrap_or(default_style);
        Self::new(text, duration, event_style)
    }

    pub fn check_and_clear(event: Option<Self>) -> Option<Self> {
        match event {
            Some(e) if e.is_expired() => None,
            other => other,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum StatusGravity {
    Left,
    Mid,
    Right,
}

#[derive(Clone, Default)]
pub struct StatusStyle {
    pub fg: Option<Color>,
    pub bg: Option<Color>,
}

impl StatusStyle {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn fg(mut self, color: Color) -> Self {
        self.fg = Some(color);
        self
    }

    pub fn bg(mut self, color: Color) -> Self {
        self.bg = Some(color);
        self
    }

    pub fn to_style(&self) -> Style {
        let mut style = Style::default();
        if let Some(fg) = self.fg {
            style = style.fg(fg);
        }
        if let Some(bg) = self.bg {
            style = style.bg(bg);
        }
        style
    }
}

struct StatusSegment {
    text: String,
    style: StatusStyle,
}

pub struct StatusBar {
    left_segments: Vec<StatusSegment>,
    mid_segments: Vec<StatusSegment>,
    right_segments: Vec<StatusSegment>,
    bg_color: Option<Color>,
    style: Option<Style>,
}

impl StatusBar {
    pub fn new() -> Self {
        Self {
            left_segments: Vec::new(),
            mid_segments: Vec::new(),
            right_segments: Vec::new(),
            bg_color: None,
            style: None,
        }
    }

    pub fn add_status(mut self, gravity: StatusGravity, text: String, style: StatusStyle) -> Self {
        let segment = StatusSegment { text, style };
        match gravity {
            StatusGravity::Left => self.left_segments.push(segment),
            StatusGravity::Mid => self.mid_segments.push(segment),
            StatusGravity::Right => self.right_segments.push(segment),
        }
        self
    }

    pub fn add_status_plain(self, gravity: StatusGravity, text: &str) -> Self {
        self.add_status(gravity, text.to_string(), StatusStyle::new())
    }

    #[allow(dead_code)]
    pub fn set_bg(mut self, color: Color) -> Self {
        self.bg_color = Some(color);
        self
    }

    pub fn set_style(mut self, style: Style) -> Self {
        self.style = Some(style);
        self
    }

    fn build_gravity_spans<'a>(
        segments: &'a [StatusSegment],
        sep: &'a str,
    ) -> (Vec<Span<'a>>, usize) {
        let mut spans = Vec::new();
        let mut total_len = 0;
        for (i, seg) in segments.iter().enumerate() {
            if i > 0 {
                spans.push(Span::raw(sep));
                total_len += sep.chars().count();
            }
            let style = seg.style.to_style();
            spans.push(Span::styled(seg.text.as_str(), style));
            total_len += seg.text.chars().count();
        }
        (spans, total_len)
    }

    pub fn render(self, area: Rect, buf: &mut Buffer) {
        let total_width = area.width as usize;
        let sep = " | ";

        let (left_spans, left_len) = Self::build_gravity_spans(&self.left_segments, sep);
        let (mid_spans, mid_len) = Self::build_gravity_spans(&self.mid_segments, sep);
        let (right_spans, right_len) = Self::build_gravity_spans(&self.right_segments, sep);

        let mid_center_start = total_width.saturating_sub(mid_len) / 2;
        let left_to_mid_padding = mid_center_start.saturating_sub(left_len);

        let right_start = total_width.saturating_sub(right_len);
        let mid_end = mid_center_start + mid_len;
        let mid_to_right_padding = right_start.saturating_sub(mid_end);

        let mut spans = vec![];
        spans.extend(left_spans);
        spans.push(Span::raw(" ".repeat(left_to_mid_padding)));

        spans.extend(mid_spans);
        spans.push(Span::raw(" ".repeat(mid_to_right_padding)));

        spans.extend(right_spans);

        let line = Line::from(spans);

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
