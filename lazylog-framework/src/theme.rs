use ratatui::{prelude::*, style::Color};

// basic 16 colors supported by macOS default terminal
#[allow(dead_code)]
const NAMED_COLORS: [Color; 16] = [
    Color::Black,
    Color::Red,
    Color::Green,
    Color::Yellow,
    Color::Blue,
    Color::Magenta,
    Color::Cyan,
    Color::Gray,
    Color::DarkGray,
    Color::LightRed,
    Color::LightGreen,
    Color::LightYellow,
    Color::LightBlue,
    Color::LightMagenta,
    Color::LightCyan,
    Color::White,
];

pub const MODE_COLORS: &[(Option<&str>, Color)] = &[
    (Some("ios"), Color::LightBlue),
    (Some("ios effect"), Color::LightBlue),
    (Some("android"), Color::Rgb(255, 165, 0)), // orange
    (Some("android effect"), Color::Rgb(255, 165, 0)),
    (Some("dyeh"), Color::LightGreen),
    (None, Color::Gray), // default
];

pub fn get_mode_color(mode_name: &Option<String>) -> Color {
    if let Some(name) = mode_name {
        for (mode, color) in MODE_COLORS {
            if let Some(m) = mode {
                if name.to_lowercase().contains(m) {
                    return *color;
                }
            }
        }
    }
    Color::Gray
}

pub const TEXT_FG_COLOR: Color = Color::Gray;

#[allow(dead_code)]
pub const LOG_HEADER_STYLE: Style = Style::new().fg(Color::White).bg(Color::DarkGray);

pub const SELECTED_STYLE: Style = Style::new().bg(Color::DarkGray);

pub const INFO_STYLE: Style = Style::new().fg(Color::White);

pub const WARN_STYLE: Style = Style::new().fg(Color::LightYellow);

pub const ERROR_STYLE: Style = Style::new().fg(Color::LightRed);

pub const DEBUG_STYLE: Style = Style::new().fg(Color::LightGreen);

pub const DISPLAY_EVENT_STYLE: Style = Style::new()
    .fg(Color::Black)
    .bg(Color::Yellow)
    .add_modifier(Modifier::BOLD);

pub const FILTER_FOCUS_STYLE: Style = Style::new().bg(Color::DarkGray);
