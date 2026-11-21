use crate::ui::M3Palette;
use iced::Color;
use image::imageops::FilterType;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum PaletteStyle {
    #[default]
    Classic,
    TonalSpot,
    Expressive,
    Vibrant,
    Neutral,
}

impl PaletteStyle {
    pub fn as_str(self) -> &'static str {
        match self {
            PaletteStyle::Classic => "classic",
            PaletteStyle::TonalSpot => "tonal-spot",
            PaletteStyle::Expressive => "expressive",
            PaletteStyle::Vibrant => "vibrant",
            PaletteStyle::Neutral => "neutral",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            PaletteStyle::Classic => "Classic",
            PaletteStyle::TonalSpot => "Tonal Spot",
            PaletteStyle::Expressive => "Expressive",
            PaletteStyle::Vibrant => "Vibrant",
            PaletteStyle::Neutral => "Neutral",
        }
    }

    pub fn from_str_lossy(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "tonal-spot" | "tonal" => PaletteStyle::TonalSpot,
            "expressive" => PaletteStyle::Expressive,
            "vibrant" => PaletteStyle::Vibrant,
            "neutral" => PaletteStyle::Neutral,
            _ => PaletteStyle::Classic,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CoverHueAnalysis {
    pub primary_hue: f32,
    pub secondary_hue: f32,
    pub saturation: f32,
    pub mean_luminance: f32,
    pub intensity: u8,
    pub is_monochrome: bool,
    pub suggested_dark: bool,
}

impl Default for CoverHueAnalysis {
    fn default() -> Self {
        Self {
            primary_hue: 256.0,
            secondary_hue: 296.0,
            saturation: 0.35,
            mean_luminance: 0.25,
            intensity: 45,
            is_monochrome: false,
            suggested_dark: true,
        }
    }
}

pub fn relative_luminance(c: Color) -> f32 {
    0.2126 * c.r + 0.7152 * c.g + 0.0722 * c.b
}

pub fn rgb_to_hsl(r: f32, g: f32, b: f32) -> (f32, f32, f32) {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) * 0.5;
    let d = max - min;
    if d < 1e-5 {
        return (0.0, 0.0, l);
    }
    let s = if l > 0.5 {
        d / (2.0 - max - min).max(1e-5)
    } else {
        d / (max + min).max(1e-5)
    };
    let mut h = if (max - r).abs() < 1e-5 {
        (g - b) / d + if g < b { 6.0 } else { 0.0 }
    } else if (max - g).abs() < 1e-5 {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    };
    h *= 60.0;
    if h < 0.0 {
        h += 360.0;
    }
    (h % 360.0, s.clamp(0.0, 1.0), l.clamp(0.0, 1.0))
}

pub fn hsl_to_color(h_deg: f32, s: f32, l: f32) -> Color {
    let h = ((h_deg % 360.0) + 360.0) % 360.0 / 360.0;
    let s = s.clamp(0.0, 1.0);
    let l = l.clamp(0.0, 1.0);
    if s < 1e-5 {
        return Color::from_rgb(l, l, l);
    }
    let q = if l < 0.5 {
        l * (1.0 + s)
    } else {
        l + s - l * s
    };
    let p = 2.0 * l - q;
    let hue2rgb = |t_in: f32| -> f32 {
        let mut t = t_in;
        if t < 0.0 {
            t += 1.0;
        }
        if t > 1.0 {
            t -= 1.0;
        }
        if t < 1.0 / 6.0 {
            p + (q - p) * 6.0 * t
        } else if t < 0.5 {
            q
        } else if t < 2.0 / 3.0 {
            p + (q - p) * (2.0 / 3.0 - t) * 6.0
        } else {
            p
        }
    };
    Color::from_rgb(
        hue2rgb(h + 1.0 / 3.0),
        hue2rgb(h),
        hue2rgb(h - 1.0 / 3.0),
    )
}

fn angular_diff(a: f32, b: f32) -> f32 {
    let d = (a - b).abs() % 360.0;
    if d > 180.0 { 360.0 - d } else { d }
}

pub fn analyze_cover_bytes(bytes: &[u8]) -> CoverHueAnalysis {
    let Ok(img) = image::load_from_memory(bytes) else {
        return CoverHueAnalysis::default();
    };
    let thumb = img.resize_exact(48, 48, FilterType::Triangle).to_rgba8();

    let mut lum_sum = 0.0f32;
    let mut lum_count = 0usize;
    let mut buckets = [0.0f32; 24];
    let mut sat_sum = 0.0f32;
    let mut valid_count = 0usize;

    for px in thumb.pixels() {
        let a = px[3];
        if a < 32 {
            continue;
        }
        let r = px[0] as f32 / 255.0;
        let g = px[1] as f32 / 255.0;
        let b = px[2] as f32 / 255.0;
        lum_sum += 0.2126 * r + 0.7152 * g + 0.0722 * b;
        lum_count += 1;

        let (h, s, l) = rgb_to_hsl(r, g, b);
        if s >= 0.15 && (0.10..=0.90).contains(&l) {
            let bucket = ((h / 15.0).floor() as usize).min(23);
            buckets[bucket] += s;
            sat_sum += s;
            valid_count += 1;
        }
    }

    let mean_luminance = if lum_count > 0 {
        lum_sum / lum_count as f32
    } else {
        0.25
    };
    let suggested_dark = mean_luminance <= 0.58;

    if valid_count == 0 {
        return CoverHueAnalysis {
            primary_hue: 256.0,
            secondary_hue: 296.0,
            saturation: 0.0,
            mean_luminance,
            intensity: 0,
            is_monochrome: true,
            suggested_dark,
        };
    }

    let mut primary_bucket = 0usize;
    let mut primary_weight = -1.0f32;
    for (idx, &w) in buckets.iter().enumerate() {
        if w > primary_weight {
            primary_weight = w;
            primary_bucket = idx;
        }
    }

    let primary_hue = primary_bucket as f32 * 15.0 + 7.5;
    let mut secondary_bucket = None;
    let mut secondary_weight = 0.0f32;
    for (idx, &w) in buckets.iter().enumerate() {
        let h = idx as f32 * 15.0 + 7.5;
        if angular_diff(h, primary_hue) >= 60.0 && w > secondary_weight {
            secondary_weight = w;
            secondary_bucket = Some(idx);
        }
    }

    let secondary_hue = secondary_bucket
        .map(|b| b as f32 * 15.0 + 7.5)
        .unwrap_or((primary_hue + 40.0) % 360.0);

    let avg_sat = sat_sum / valid_count as f32;
    let intensity = if valid_count < 8 {
        0
    } else {
        (avg_sat * 90.0).round().clamp(20.0, 85.0) as u8
    };
    let is_monochrome = intensity < 8;

    CoverHueAnalysis {
        primary_hue,
        secondary_hue,
        saturation: avg_sat,
        mean_luminance,
        intensity,
        is_monochrome,
        suggested_dark,
    }
}

pub fn clamp_light_contrast(c: Color) -> Color {
    let lum = relative_luminance(c);
    if lum <= 0.55 {
        return c;
    }
    let scale = 0.55 / lum.max(1e-5);
    Color::from_rgb(
        (c.r * scale).clamp(0.0, 1.0),
        (c.g * scale).clamp(0.0, 1.0),
        (c.b * scale).clamp(0.0, 1.0),
    )
}

pub fn generate_m3_palette(
    analysis: &CoverHueAnalysis,
    style: PaletteStyle,
    mode: &str,
) -> M3Palette {
    let is_dark = if mode.eq_ignore_ascii_case("auto") {
        analysis.suggested_dark
    } else {
        !mode.eq_ignore_ascii_case("light")
    };

    if style == PaletteStyle::Classic {
        return if is_dark {
            M3Palette::dark()
        } else {
            M3Palette::light()
        };
    }

    let (hue, sec_hue, base_sat) = if analysis.is_monochrome || analysis.intensity < 8 {
        (256.0, 296.0, 0.08)
    } else {
        let sat = match style {
            PaletteStyle::Vibrant => (analysis.saturation * 1.15).clamp(0.55, 0.92),
            PaletteStyle::Expressive => (analysis.saturation * 1.05).clamp(0.45, 0.85),
            PaletteStyle::TonalSpot => analysis.saturation.clamp(0.30, 0.68),
            PaletteStyle::Neutral => analysis.saturation.clamp(0.06, 0.22),
            PaletteStyle::Classic => 0.45,
        };
        let sec = if style == PaletteStyle::Expressive {
            (analysis.primary_hue + 120.0) % 360.0
        } else {
            analysis.secondary_hue
        };
        (analysis.primary_hue, sec, sat)
    };

    let surf_sat = (base_sat * 0.22).clamp(0.02, 0.16);
    if is_dark {
        M3Palette {
            is_dark: true,
            page_bg: hsl_to_color(hue, surf_sat, 0.045),
            panel_bg: hsl_to_color(hue, surf_sat, 0.07),
            background: hsl_to_color(hue, surf_sat, 0.075),
            surface: hsl_to_color(hue, surf_sat, 0.075),
            surface_container_lowest: hsl_to_color(hue, surf_sat, 0.055),
            surface_container_low: hsl_to_color(hue, surf_sat, 0.105),
            surface_container: hsl_to_color(hue, surf_sat, 0.125),
            surface_container_high: hsl_to_color(hue, surf_sat, 0.165),
            surface_container_highest: hsl_to_color(hue, surf_sat, 0.21),
            surface_bright: hsl_to_color(hue, surf_sat, 0.23),
            primary: hsl_to_color(hue, (base_sat * 1.1).clamp(0.12, 0.95), 0.78),
            on_primary: hsl_to_color(hue, base_sat, 0.18),
            primary_container: hsl_to_color(hue, (base_sat * 0.8).clamp(0.10, 0.75), 0.28),
            on_primary_container: hsl_to_color(hue, (base_sat * 0.9).clamp(0.12, 0.90), 0.90),
            secondary_container: hsl_to_color(sec_hue, (base_sat * 0.55).clamp(0.06, 0.55), 0.26),
            on_secondary_container: hsl_to_color(sec_hue, (base_sat * 0.7).clamp(0.08, 0.80), 0.90),
            error: M3Palette::dark().error,
            on_error: M3Palette::dark().on_error,
            on_surface: hsl_to_color(hue, surf_sat * 0.5, 0.91),
            on_surface_variant: hsl_to_color(hue, surf_sat * 0.7, 0.78),
            outline: hsl_to_color(hue, surf_sat * 0.6, 0.58),
            outline_variant: hsl_to_color(hue, surf_sat * 0.7, 0.29),
        }
    } else {
        let primary = clamp_light_contrast(hsl_to_color(
            hue,
            (base_sat * 1.05).clamp(0.12, 0.88),
            0.44,
        ));
        M3Palette {
            is_dark: false,
            page_bg: hsl_to_color(hue, surf_sat, 0.98),
            panel_bg: hsl_to_color(hue, surf_sat, 0.95),
            background: hsl_to_color(hue, surf_sat, 0.98),
            surface: hsl_to_color(hue, surf_sat, 0.98),
            surface_container_lowest: Color::WHITE,
            surface_container_low: hsl_to_color(hue, surf_sat, 0.955),
            surface_container: hsl_to_color(hue, surf_sat, 0.935),
            surface_container_high: hsl_to_color(hue, surf_sat, 0.91),
            surface_container_highest: hsl_to_color(hue, surf_sat, 0.885),
            surface_bright: hsl_to_color(hue, surf_sat, 0.975),
            primary,
            on_primary: Color::from_rgb(0.99, 0.98, 1.0),
            primary_container: hsl_to_color(hue, (base_sat * 0.85).clamp(0.10, 0.80), 0.89),
            on_primary_container: hsl_to_color(hue, base_sat, 0.16),
            secondary_container: hsl_to_color(sec_hue, (base_sat * 0.6).clamp(0.08, 0.65), 0.90),
            on_secondary_container: hsl_to_color(sec_hue, base_sat * 0.7, 0.16),
            error: M3Palette::light().error,
            on_error: M3Palette::light().on_error,
            on_surface: hsl_to_color(hue, surf_sat * 0.6, 0.12),
            on_surface_variant: hsl_to_color(hue, surf_sat * 0.7, 0.30),
            outline: hsl_to_color(hue, surf_sat * 0.5, 0.47),
            outline_variant: hsl_to_color(hue, surf_sat * 0.6, 0.78),
        }
    }
}
