use app_lib::theme::{
    analyze_cover_bytes, generate_m3_palette, relative_luminance, CoverHueAnalysis, PaletteStyle,
};
use app_lib::ui::visualizer::{
    interpolate_cover_radii, smooth_fft_bins, M3CoverShape, FFT_BINS, POLAR_POINTS,
};
use image::{DynamicImage, ImageFormat, Rgba, RgbaImage};
use std::io::Cursor;

fn encode_solid_png(r: u8, g: u8, b: u8) -> Vec<u8> {
    let img = RgbaImage::from_pixel(16, 16, Rgba([r, g, b, 255]));
    let mut out = Vec::new();
    DynamicImage::ImageRgba8(img)
        .write_to(&mut Cursor::new(&mut out), ImageFormat::Png)
        .unwrap();
    out
}

#[test]
fn test_cover_hue_extraction_chromatic_and_monochrome() {
    let red_png = encode_solid_png(220, 40, 40);
    let analysis = analyze_cover_bytes(&red_png);
    assert!(!analysis.is_monochrome);
    assert!(analysis.primary_hue < 30.0 || analysis.primary_hue > 330.0);
    assert!(analysis.intensity >= 20);
    assert!(analysis.suggested_dark);

    let bright_cyan_png = encode_solid_png(180, 245, 250);
    let light_analysis = analyze_cover_bytes(&bright_cyan_png);
    assert!(light_analysis.mean_luminance > 0.58);
    assert!(!light_analysis.suggested_dark);

    let gray_png = encode_solid_png(120, 120, 120);
    let mono = analyze_cover_bytes(&gray_png);
    assert!(mono.is_monochrome);
    assert_eq!(mono.intensity, 0);
}

#[test]
fn test_cover_secondary_hue_separation() {
    let mut img = RgbaImage::new(16, 16);
    for (x, _y, px) in img.enumerate_pixels_mut() {
        *px = if x < 10 {
            Rgba([20, 90, 230, 255]) // bright blue
        } else {
            Rgba([230, 160, 20, 255]) // warm orange (>120° away)
        };
    }
    let mut bytes = Vec::new();
    DynamicImage::ImageRgba8(img)
        .write_to(&mut Cursor::new(&mut bytes), ImageFormat::Png)
        .unwrap();
    let analysis = analyze_cover_bytes(&bytes);
    let diff = (analysis.primary_hue - analysis.secondary_hue).abs();
    let ang = if diff > 180.0 { 360.0 - diff } else { diff };
    assert!(ang >= 60.0, "expected >= 60 deg separation, got {}", ang);
}

#[test]
fn test_generate_m3_palette_all_styles_and_light_contrast_clamp() {
    let analysis = CoverHueAnalysis {
        primary_hue: 55.0, // bright yellow can have high luminance in light mode
        secondary_hue: 185.0,
        saturation: 0.9,
        mean_luminance: 0.85,
        intensity: 80,
        is_monochrome: false,
        suggested_dark: false,
    };

    for style in [
        PaletteStyle::Classic,
        PaletteStyle::TonalSpot,
        PaletteStyle::Expressive,
        PaletteStyle::Vibrant,
        PaletteStyle::Neutral,
    ] {
        let dark = generate_m3_palette(&analysis, style, "dark");
        assert!(dark.is_dark);

        let light = generate_m3_palette(&analysis, style, "light");
        assert!(!light.is_dark);
        let lum = relative_luminance(light.primary);
        assert!(
            lum <= 0.56,
            "style {:?} light primary luminance {} exceeded 0.55 clamp",
            style,
            lum
        );
    }

    let auto_light = generate_m3_palette(&analysis, PaletteStyle::Vibrant, "auto");
    assert!(!auto_light.is_dark);
}

#[test]
fn test_fft_smoothing_and_m3_cover_morphing() {
    let mut bins = [0.0f32; FFT_BINS];
    let raw = [255u8; FFT_BINS];
    smooth_fft_bins(&mut bins, &raw, true);
    assert!(bins[0] > 0.5);

    smooth_fft_bins(&mut bins, &[], false);
    assert!(bins[0] < 0.6);

    for shape in M3CoverShape::ALL {
        let radii = shape.polar_radii();
        assert_eq!(radii.len(), POLAR_POINTS);
        assert!(radii.iter().all(|&r| (0.45..=1.0).contains(&r)));
    }

    let a = M3CoverShape::Cookie7Sided.polar_radii();
    let b = M3CoverShape::Clover4Leaf.polar_radii();
    let mid = interpolate_cover_radii(&a, &b, 0.5);
    assert_eq!(mid.len(), POLAR_POINTS);
    for ((&va, &vb), &vm) in a.iter().zip(b.iter()).zip(mid.iter()) {
        assert!((vm - (va + vb) * 0.5).abs() < 1e-4);
    }
}
