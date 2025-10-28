use ratatui::{
    prelude::*,
    style::palette::{
        self,
        tailwind::{Palette, ZINC},
    },
};

pub const TEXT_FG_COLOR: Color = select_color_with_default_palette(PaletteIdx::C200);

#[allow(dead_code)]
pub const LOG_HEADER_STYLE: Style = Style::new()
    .fg(select_color_with_default_palette(PaletteIdx::C100))
    .bg(select_color_with_default_palette(PaletteIdx::C400));
pub const SELECTED_STYLE: Style =
    Style::new().bg(select_color_with_default_palette(PaletteIdx::C700));
pub const INFO_STYLE: Style = Style::new().fg(select_color_from_palette(
    PaletteIdx::C100,
    palette::tailwind::ZINC,
));
pub const WARN_STYLE: Style = Style::new().fg(select_color_from_palette(
    PaletteIdx::C400,
    palette::tailwind::ORANGE,
));
pub const ERROR_STYLE: Style = Style::new().fg(select_color_from_palette(
    PaletteIdx::C400,
    palette::tailwind::RED,
));
pub const DEBUG_STYLE: Style = Style::new().fg(select_color_from_palette(
    PaletteIdx::C400,
    palette::tailwind::GREEN,
));
pub const DISPLAY_EVENT_STYLE: Style = Style::new()
    .fg(select_color_with_default_palette(PaletteIdx::C100))
    .bg(select_color_from_palette(
        PaletteIdx::C400,
        palette::tailwind::ORANGE,
    ))
    .add_modifier(Modifier::BOLD);

pub enum PaletteIdx {
    #[allow(dead_code)]
    C50,
    #[allow(dead_code)]
    C100,
    #[allow(dead_code)]
    C200,
    #[allow(dead_code)]
    C300,
    #[allow(dead_code)]
    C400,
    #[allow(dead_code)]
    C500,
    #[allow(dead_code)]
    C600,
    #[allow(dead_code)]
    C700,
    #[allow(dead_code)]
    C800,
    #[allow(dead_code)]
    C900,
    #[allow(dead_code)]
    C950,
}

pub const fn select_color_from_palette(idx: PaletteIdx, palette: Palette) -> Color {
    match idx {
        PaletteIdx::C50 => palette.c50,
        PaletteIdx::C100 => palette.c100,
        PaletteIdx::C200 => palette.c200,
        PaletteIdx::C300 => palette.c300,
        PaletteIdx::C400 => palette.c400,
        PaletteIdx::C500 => palette.c500,
        PaletteIdx::C600 => palette.c600,
        PaletteIdx::C700 => palette.c700,
        PaletteIdx::C800 => palette.c800,
        PaletteIdx::C900 => palette.c900,
        PaletteIdx::C950 => palette.c950,
    }
}

pub const fn select_color_with_default_palette(idx: PaletteIdx) -> Color {
    select_color_from_palette(idx, ZINC)
}
