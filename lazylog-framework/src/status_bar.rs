use ratatui::{
    prelude::*,
    widgets::{Paragraph, Widget},
};
use std::time::{Duration, Instant};
use unicode_width::UnicodeWidthStr;

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

    pub fn from_colors(fg: Option<Color>, bg: Option<Color>) -> Self {
        Self { fg, bg }
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
}

impl Default for StatusBar {
    fn default() -> Self {
        Self::new()
    }
}

impl StatusBar {
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
                total_len += UnicodeWidthStr::width(sep);
            }
            let style = seg.style.to_style();
            spans.push(Span::styled(seg.text.as_str(), style));
            total_len += UnicodeWidthStr::width(seg.text.as_str());
        }
        (spans, total_len)
    }

    pub fn render(self, area: Rect, buf: &mut Buffer) {
        let total_width = area.width as usize;
        let sep = " | ";

        let (left_spans, left_len) = Self::build_gravity_spans(&self.left_segments, sep);
        let (mid_spans, mid_len) = Self::build_gravity_spans(&self.mid_segments, sep);
        let (right_spans, right_len) = Self::build_gravity_spans(&self.right_segments, sep);

        let mut base_style = Style::default();
        if let Some(bg) = self.bg_color {
            base_style = base_style.bg(bg);
        }
        if let Some(custom_style) = self.style {
            base_style = base_style.patch(custom_style);
        }

        buf.set_style(area, base_style);

        // The center owns its screen-relative position. Side statuses may be
        // clipped, but must never push a notification off-center or off-screen.
        let mid_width = mid_len.min(total_width);
        let mid_start = (total_width - mid_width) / 2;
        let mid_end = mid_start + mid_width;
        let right_start = if mid_width > 0 {
            total_width
                .saturating_sub(right_len)
                .max((mid_end + usize::from(mid_end < total_width)).min(total_width))
        } else {
            total_width.saturating_sub(right_len)
        };
        let left_limit = if mid_width > 0 {
            mid_start
        } else {
            right_start
        };
        let left_width =
            left_len.min(left_limit.saturating_sub(usize::from(mid_width > 0 || right_len > 0)));

        for (spans, start, width) in [
            (left_spans, 0, left_width),
            (mid_spans, mid_start, mid_width),
            (right_spans, right_start, total_width - right_start),
        ] {
            if width > 0 {
                Paragraph::new(Line::from(spans)).style(base_style).render(
                    Rect::new(area.x + start as u16, area.y, width as u16, area.height),
                    buf,
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn center_stays_visible_at_narrow_widths_and_nonzero_origins() {
        for width in [0, 1, 12, 40, 120] {
            let area = Rect::new(3, 2, width, 1);
            let mut buf = Buffer::empty(area);
            let message = "Exported: ~/Downloads/lazylog-abcdef.log";
            StatusBar::new()
                .add_status_plain(StatusGravity::Left, "a very long provider status")
                .add_status_plain(StatusGravity::Mid, message)
                .add_status_plain(StatusGravity::Right, "20 / 20")
                .render(area, &mut buf);
            let visible_width = message.width().min(width as usize);
            let start = (width as usize - visible_width) / 2;
            let rendered: String = (start..start + visible_width)
                .map(|x| buf[(area.x + x as u16, area.y)].symbol())
                .collect();
            assert_eq!(rendered, &message[..visible_width]);
        }
    }

    #[test]
    fn side_statuses_keep_their_edges_without_a_center_message() {
        let area = Rect::new(0, 0, 20, 1);
        let mut buf = Buffer::empty(area);
        StatusBar::new()
            .add_status_plain(StatusGravity::Left, "left")
            .add_status_plain(StatusGravity::Right, "right")
            .render(area, &mut buf);
        let rendered: String = (0..20).map(|x| buf[(x, 0)].symbol()).collect();
        assert_eq!(rendered, "left           right");
    }

    #[test]
    fn long_center_message_is_centered_without_being_pushed_by_side_statuses() {
        let area = Rect::new(0, 0, 100, 1);
        let mut buf = Buffer::empty(area);
        let message = "Exported 20 logs: /Users/test/Downloads/lazylog-0123456789abcdef.log";
        StatusBar::new()
            .add_status_plain(
                StatusGravity::Left,
                "ios devicectl app-console (抖音开发版) | connected | wrap on",
            )
            .add_status_plain(StatusGravity::Mid, message)
            .add_status_plain(StatusGravity::Right, "20 / 20")
            .render(area, &mut buf);
        let start = (area.width as usize - message.width()) / 2;
        let rendered: String = (start..start + message.width())
            .map(|x| buf[(x as u16, 0)].symbol())
            .collect();
        assert_eq!(rendered, message);
    }
}
