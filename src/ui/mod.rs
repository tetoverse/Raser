pub mod karaoke;
pub mod playbar;
pub mod sidebar;
pub mod titlebar;
pub mod views;
pub mod visualizer;

use iced::widget::{button, container, slider, svg, text_input};
use iced::{Background, Border, Color, Shadow, Theme, Vector};

pub const CORNER_XS: f32 = 4.0;
pub const CORNER_S: f32 = 8.0;
pub const CORNER_M: f32 = 12.0;
pub const CORNER_L: f32 = 16.0;
pub const CORNER_XL: f32 = 28.0;
pub const CORNER_FULL: f32 = 9999.0;

pub const TITLEBAR_HEIGHT: f32 = 40.0;
pub const TOP_NAV_HEIGHT: f32 = 64.0;
pub const SIDEBAR_CONNECTED_WIDTH: f32 = 84.0;
pub const SIDEBAR_WIDTH: f32 = 80.0;
pub const SIDEBAR_CLASSIC_WIDTH: f32 = 80.0;
pub const PLAYBAR_HEIGHT: f32 = 96.0;
pub const PLAYBAR_CLASSIC_HEIGHT: f32 = 98.0;
pub const RIGHT_PANEL_WIDTH: f32 = 340.0;

#[derive(Debug, Clone, Copy)]
pub struct M3Palette {
    pub is_dark: bool,
    pub page_bg: Color,
    pub panel_bg: Color,
    pub background: Color,
    pub surface: Color,
    pub surface_container_lowest: Color,
    pub surface_container_low: Color,
    pub surface_container: Color,
    pub surface_container_high: Color,
    pub surface_container_highest: Color,
    pub surface_bright: Color,
    pub primary: Color,
    pub on_primary: Color,
    pub primary_container: Color,
    pub on_primary_container: Color,
    pub secondary_container: Color,
    pub on_secondary_container: Color,
    pub error: Color,
    pub on_error: Color,
    pub on_surface: Color,
    pub on_surface_variant: Color,
    pub outline: Color,
    pub outline_variant: Color,
}

const fn hex(rgb: u32) -> Color {
    Color::from_rgb(
        ((rgb >> 16) & 0xFF) as f32 / 255.0,
        ((rgb >> 8) & 0xFF) as f32 / 255.0,
        (rgb & 0xFF) as f32 / 255.0,
    )
}

impl M3Palette {
    /// Exact default dark tokens from `ui/styles.css:31-65, 10793-10798`
    pub const fn dark() -> Self {
        Self {
            is_dark: true,
            page_bg: hex(0x0A0A0C),
            panel_bg: hex(0x111114),
            background: hex(0x121214),
            surface: hex(0x121214),
            surface_container_lowest: hex(0x0D0E0F),
            surface_container_low: hex(0x1A1B1E),
            surface_container: hex(0x1E1F22),
            surface_container_high: hex(0x28292C),
            surface_container_highest: hex(0x333438),
            surface_bright: hex(0x38383A),
            primary: hex(0xD0BCFF),
            on_primary: hex(0x381E72),
            primary_container: hex(0x4F378B),
            on_primary_container: hex(0xEADDFF),
            secondary_container: hex(0x4A4458),
            on_secondary_container: hex(0xE8DEF8),
            error: hex(0xF2B8B5),
            on_error: hex(0x601410),
            on_surface: hex(0xE6E6E8),
            on_surface_variant: hex(0xCAC4D0),
            outline: hex(0x938F99),
            outline_variant: hex(0x49454F),
        }
    }

    /// Exact default light tokens from `ui/styles.css:8975-9007, 10799-10806` & `ui/app.js:1400-1428`
    pub const fn light() -> Self {
        Self {
            is_dark: false,
            page_bg: hex(0xFAFAFC),
            panel_bg: hex(0xF1EFF4),
            background: hex(0xFAFAFC),
            surface: hex(0xFCF8FA),
            surface_container_lowest: hex(0xFFFFFF),
            surface_container_low: hex(0xF5F3F7),
            surface_container: hex(0xEFEDF2),
            surface_container_high: hex(0xE9E7EC),
            surface_container_highest: hex(0xE3E1E6),
            surface_bright: hex(0xFAF8FB),
            primary: hex(0x6750A4),
            on_primary: hex(0xFEFBFF),
            primary_container: hex(0xEADDFF),
            on_primary_container: hex(0x21005D),
            secondary_container: hex(0xE8DEF8),
            on_secondary_container: hex(0x1D192B),
            error: hex(0xB3261E),
            on_error: hex(0xFFFFFF),
            on_surface: hex(0x1C1B1F),
            on_surface_variant: hex(0x49454F),
            outline: hex(0x79747E),
            outline_variant: hex(0xCAC4D0),
        }
    }

    pub fn from_mode(mode: &str) -> Self {
        if mode.eq_ignore_ascii_case("light") {
            Self::light()
        } else {
            Self::dark()
        }
    }

    pub fn to_iced_theme(self) -> Theme {
        Theme::custom(
            if self.is_dark { "Raser M3 Dark" } else { "Raser M3 Light" }.to_string(),
            iced::theme::Palette {
                background: self.background,
                text: self.on_surface,
                primary: self.primary,
                success: hex(0x6DD58C),
                warning: hex(0xF5B83D),
                danger: self.error,
            },
        )
    }
}

pub fn with_alpha(c: Color, alpha: f32) -> Color {
    Color { a: alpha, ..c }
}

pub fn surface_container_style(bg: Color, radius: f32) -> container::Style {
    container::Style {
        background: Some(Background::Color(bg)),
        border: Border {
            color: Color::TRANSPARENT,
            width: 0.0,
            radius: radius.into(),
        },
        ..Default::default()
    }
}

pub fn surface_card_bordered(bg: Color, border_color: Color, radius: f32) -> container::Style {
    container::Style {
        background: Some(Background::Color(bg)),
        border: Border {
            color: border_color,
            width: 1.0,
            radius: radius.into(),
        },
        ..Default::default()
    }
}

pub fn modal_card_style(p: M3Palette) -> container::Style {
    container::Style {
        background: Some(Background::Color(p.surface_container_high)),
        border: Border {
            color: with_alpha(p.outline_variant, 0.45),
            width: 1.0,
            radius: CORNER_XL.into(),
        },
        shadow: Shadow {
            color: Color::from_rgba(0.0, 0.0, 0.0, 0.45),
            offset: Vector::new(0.0, 12.0),
            blur_radius: 32.0,
        },
        ..Default::default()
    }
}

pub fn pill_primary_btn(p: M3Palette, status: button::Status) -> button::Style {
    let bg = match status {
        button::Status::Hovered => Color {
            a: 0.9,
            ..p.primary
        },
        button::Status::Pressed => Color {
            a: 0.82,
            ..p.primary
        },
        button::Status::Disabled => with_alpha(p.surface_container_highest, 0.5),
        button::Status::Active => p.primary,
    };
    button::Style {
        background: Some(Background::Color(bg)),
        text_color: p.on_primary,
        border: Border {
            radius: CORNER_FULL.into(),
            ..Default::default()
        },
        ..Default::default()
    }
}

pub fn outline_pill_btn(p: M3Palette, active: bool, status: button::Status) -> button::Style {
    let (bg, fg) = if active {
        (p.secondary_container, p.on_secondary_container)
    } else {
        match status {
            button::Status::Hovered => (p.surface_container_highest, p.on_surface),
            button::Status::Pressed => (with_alpha(p.on_surface, 0.14), p.on_surface),
            _ => (p.surface_container_high, p.on_surface_variant),
        }
    };
    button::Style {
        background: Some(Background::Color(bg)),
        text_color: fg,
        border: Border {
            radius: CORNER_FULL.into(),
            ..Default::default()
        },
        ..Default::default()
    }
}

pub fn icon_pill_btn(p: M3Palette, active: bool, status: button::Status) -> button::Style {
    let (bg, fg) = if active {
        (p.primary, p.on_primary)
    } else {
        match status {
            button::Status::Hovered => (p.surface_container_highest, p.on_surface),
            button::Status::Pressed => (with_alpha(p.on_surface, 0.14), p.on_surface),
            _ => (p.surface_container_high, p.on_surface_variant),
        }
    };
    button::Style {
        background: Some(Background::Color(bg)),
        text_color: fg,
        border: Border {
            radius: CORNER_FULL.into(),
            ..Default::default()
        },
        ..Default::default()
    }
}

pub fn transparent_icon_btn(p: M3Palette, active: bool, status: button::Status) -> button::Style {
    let (bg, fg) = if active {
        (p.primary_container, p.on_primary_container)
    } else {
        match status {
            button::Status::Hovered => (p.surface_container_high, p.on_surface),
            button::Status::Pressed => (with_alpha(p.on_surface, 0.12), p.on_surface),
            _ => (Color::TRANSPARENT, p.on_surface_variant),
        }
    };
    button::Style {
        background: Some(Background::Color(bg)),
        text_color: fg,
        border: Border {
            radius: CORNER_FULL.into(),
            ..Default::default()
        },
        ..Default::default()
    }
}

pub fn titlebar_close_btn(p: M3Palette, status: button::Status) -> button::Style {
    let (bg, fg) = match status {
        button::Status::Hovered | button::Status::Pressed => (p.error, p.on_error),
        _ => (Color::TRANSPARENT, p.on_surface_variant),
    };
    button::Style {
        background: Some(Background::Color(bg)),
        text_color: fg,
        border: Border {
            radius: CORNER_FULL.into(),
            ..Default::default()
        },
        ..Default::default()
    }
}

pub fn nav_pill_btn(p: M3Palette, active: bool, status: button::Status) -> button::Style {
    let (bg, fg) = if active {
        (p.primary, p.on_primary)
    } else {
        match status {
            button::Status::Hovered => (with_alpha(p.on_surface, 0.08), p.on_surface),
            button::Status::Pressed => (with_alpha(p.on_surface, 0.12), p.on_surface),
            _ => (Color::TRANSPARENT, p.on_surface_variant),
        }
    };
    button::Style {
        background: Some(Background::Color(bg)),
        text_color: fg,
        border: Border {
            radius: CORNER_FULL.into(),
            ..Default::default()
        },
        ..Default::default()
    }
}

pub fn track_row_btn(p: M3Palette, is_playing: bool, status: button::Status) -> button::Style {
    let bg = if is_playing {
        p.surface_container_high
    } else {
        match status {
            button::Status::Hovered => p.surface_container,
            button::Status::Pressed => p.surface_container_high,
            _ => Color::TRANSPARENT,
        }
    };
    button::Style {
        background: Some(Background::Color(bg)),
        text_color: if is_playing { p.primary } else { p.on_surface },
        border: Border {
            radius: 20.0.into(),
            ..Default::default()
        },
        ..Default::default()
    }
}

pub fn search_input_style(p: M3Palette, status: text_input::Status) -> text_input::Style {
    let bg = match status {
        text_input::Status::Focused { .. } => p.surface_container_highest,
        text_input::Status::Hovered => p.surface_container_high,
        _ => p.surface_container,
    };
    text_input::Style {
        background: Background::Color(bg),
        border: Border {
            color: Color::TRANSPARENT,
            width: 0.0,
            radius: CORNER_FULL.into(),
        },
        icon: p.on_surface_variant,
        placeholder: p.outline,
        value: p.on_surface,
        selection: with_alpha(p.primary, 0.35),
    }
}

pub fn modal_input_style(p: M3Palette, status: text_input::Status) -> text_input::Style {
    let border_col = match status {
        text_input::Status::Focused { .. } => p.primary,
        _ => p.outline_variant,
    };
    text_input::Style {
        background: Background::Color(p.surface_container),
        border: Border {
            color: border_col,
            width: 1.0,
            radius: CORNER_L.into(),
        },
        icon: p.on_surface_variant,
        placeholder: p.outline,
        value: p.on_surface,
        selection: with_alpha(p.primary, 0.35),
    }
}

pub fn m3_slider_style(p: M3Palette, _status: slider::Status) -> slider::Style {
    slider::Style {
        rail: slider::Rail {
            backgrounds: (
                Background::Color(p.primary),
                Background::Color(with_alpha(p.on_surface, 0.25)),
            ),
            width: 4.0,
            border: Border {
                radius: CORNER_FULL.into(),
                ..Default::default()
            },
        },
        handle: slider::Handle {
            shape: slider::HandleShape::Circle { radius: 6.0 },
            background: Background::Color(p.primary),
            border_width: 0.0,
            border_color: Color::TRANSPARENT,
        },
    }
}

// --- Crisp SVG Icons matching `ui/index.html` ---

pub fn raser_logo_svg<'a, M>(color: Color, size: f32) -> iced::Element<'a, M> {
    let handle = svg::Handle::from_memory(include_bytes!("../../assets/raser.svg").as_slice());
    svg(handle)
        .width(size)
        .height(size)
        .style(move |_, _| svg::Style { color: Some(color) })
        .into()
}

pub fn icon_svg<'a, M>(svg_str: &'static str, color: Color, size: f32) -> iced::Element<'a, M> {
    let handle = svg::Handle::from_memory(svg_str.as_bytes());
    svg(handle)
        .width(size)
        .height(size)
        .style(move |_, _| svg::Style { color: Some(color) })
        .into()
}

pub const ICON_HEART: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M19 14c1.49-1.46 3-3.21 3-5.5A5.5 5.5 0 0 0 16.5 3c-1.76 0-3 .5-4.5 2-1.5-1.5-2.74-2-4.5-2A5.5 5.5 0 0 0 2 8.5c0 2.3 1.5 4.05 3 5.5l7 7Z"/></svg>"##;
pub const ICON_HEART_FILLED: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="currentColor" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M19 14c1.49-1.46 3-3.21 3-5.5A5.5 5.5 0 0 0 16.5 3c-1.76 0-3 .5-4.5 2-1.5-1.5-2.74-2-4.5-2A5.5 5.5 0 0 0 2 8.5c0 2.3 1.5 4.05 3 5.5l7 7Z"/></svg>"##;
pub const ICON_ALBUMS: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><circle cx="12" cy="12" r="10"/><circle cx="12" cy="12" r="3"/></svg>"##;
pub const ICON_FOLDER: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"/></svg>"##;
pub const ICON_HISTORY: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="10"/><polyline points="12 6 12 12 16 14"/></svg>"##;
pub const ICON_QUEUE: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="4" y1="7" x2="20" y2="7"/><line x1="4" y1="12" x2="20" y2="12"/><line x1="4" y1="17" x2="14" y2="17"/></svg>"##;
pub const ICON_PLUS: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round"><line x1="12" y1="5" x2="12" y2="19"/><line x1="5" y1="12" x2="19" y2="12"/></svg>"##;
pub const ICON_SETTINGS: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83-2.83l.06-.06A1.65 1.65 0 0 0 4.68 15a1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 2.83-2.83l.06.06A1.65 1.65 0 0 0 9 4.68a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 2.83l-.06.06A1.65 1.65 0 0 0 19.4 9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1Z"/></svg>"##;
pub const ICON_PLAY: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="currentColor"><polygon points="6 4 20 12 6 20 6 4"/></svg>"##;
pub const ICON_PAUSE: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="currentColor"><rect x="6" y="4" width="4" height="16" rx="1"/><rect x="14" y="4" width="4" height="16" rx="1"/></svg>"##;
pub const ICON_PREV: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="currentColor"><polygon points="19 20 9 12 19 4 19 20"/><line x1="5" y1="19" x2="5" y2="5" stroke="currentColor" stroke-width="2.5"/></svg>"##;
pub const ICON_NEXT: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="currentColor"><polygon points="5 4 15 12 5 20 5 4"/><line x1="19" y1="5" x2="19" y2="19" stroke="currentColor" stroke-width="2.5"/></svg>"##;
pub const ICON_SHUFFLE: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="16 3 21 3 21 8"/><line x1="4" y1="20" x2="21" y2="3"/><polyline points="21 16 21 21 16 21"/><line x1="15" y1="15" x2="21" y2="21"/><line x1="4" y1="4" x2="9" y2="9"/></svg>"##;
pub const ICON_REPEAT: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="17 1 21 5 17 9"/><path d="M3 11V9a4 4 0 0 1 4-4h14"/><polyline points="7 23 3 19 7 15"/><path d="M21 13v2a4 4 0 0 1-4 4H3"/></svg>"##;
pub const ICON_VOLUME: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polygon points="11 5 6 9 2 9 2 15 6 15 11 19 11 5"/><path d="M15.54 8.46a5 5 0 0 1 0 7.07"/><path d="M19.07 4.93a10 10 0 0 1 0 14.14"/></svg>"##;
pub const ICON_VOLUME_MUTED: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polygon points="11 5 6 9 2 9 2 15 6 15 11 19 11 5"/><line x1="23" y1="9" x2="17" y2="15"/><line x1="17" y1="9" x2="23" y2="15"/></svg>"##;
pub const ICON_SEARCH: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="11" cy="11" r="7"/><line x1="21" y1="21" x2="16.5" y2="16.5"/></svg>"##;
pub const ICON_MUSIC: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M9 18V5l12-2v13"/><circle cx="6" cy="18" r="3"/><circle cx="18" cy="16" r="3"/></svg>"##;
pub const ICON_EDIT: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M12 20h9"/><path d="M16.5 3.5a2.121 2.121 0 0 1 3 3L7 19l-4 1 1-4L16.5 3.5z"/></svg>"##;
pub const ICON_SCISSORS: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="6" cy="6" r="3"/><circle cx="6" cy="18" r="3"/><line x1="20" y1="4" x2="8.12" y2="15.88"/><line x1="14.47" y1="14.48" x2="20" y2="20"/><line x1="8.12" y1="8.12" x2="12" y2="12"/></svg>"##;
pub const ICON_TRASH: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="3 6 5 6 21 6"/><path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2"/></svg>"##;
pub const ICON_MINIMIZE: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><line x1="5" y1="12" x2="19" y2="12"/></svg>"##;
pub const ICON_MAXIMIZE: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><rect x="4" y="4" width="16" height="16" rx="2"/></svg>"##;
pub const ICON_FULLSCREEN: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M8 3H5a2 2 0 0 0-2 2v3m18 0V5a2 2 0 0 0-2-2h-3m0 18h3a2 2 0 0 0 2-2v-3M3 16v3a2 2 0 0 0 2 2h3"/></svg>"##;
pub const ICON_CLOSE: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/></svg>"##;
pub const ICON_MINI_PLAYER: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="2" y="4" width="20" height="16" rx="2"/><rect x="12" y="11" width="7" height="6" rx="1"/></svg>"##;
pub const ICON_BACK: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="19" y1="12" x2="5" y2="12"/><polyline points="12 19 5 12 12 5"/></svg>"##;
pub const ICON_PIN: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="12" y1="17" x2="12" y2="22"/><path d="M5 17h14v-1.76a2 2 0 0 0-1.11-1.79l-1.78-.9A2 2 0 0 1 15 10.76V6h1a2 2 0 0 0 0-4H8a2 2 0 0 0 0 4h1v4.76a2 2 0 0 1-1.11 1.79l-1.78.9A2 2 0 0 0 5 15.24Z"/></svg>"##;
