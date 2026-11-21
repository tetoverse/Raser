pub mod palette;

pub use palette::{
    analyze_cover_bytes, generate_m3_palette, hsl_to_color, relative_luminance, rgb_to_hsl,
    CoverHueAnalysis, PaletteStyle,
};
