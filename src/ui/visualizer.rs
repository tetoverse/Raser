use crate::app::Message;
use crate::ui::{with_alpha, M3Palette};
use iced::mouse;
use iced::widget::canvas::{self, Canvas, Geometry, Path};
use iced::{Element, Length, Point, Rectangle, Renderer, Size, Theme};
use std::f32::consts::TAU;

pub const FFT_BINS: usize = 64;
pub const POLAR_POINTS: usize = 360;
pub const HOLD_MS: f32 = 2000.0;
pub const MORPH_MS: f32 = 1250.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum M3CoverShape {
    Cookie7Sided,
    Cookie9Sided,
    Cookie12Sided,
    Clover4Leaf,
}

impl M3CoverShape {
    pub const ALL: [M3CoverShape; 4] = [
        M3CoverShape::Cookie7Sided,
        M3CoverShape::Cookie9Sided,
        M3CoverShape::Cookie12Sided,
        M3CoverShape::Clover4Leaf,
    ];

    pub fn next(self) -> Self {
        match self {
            M3CoverShape::Cookie7Sided => M3CoverShape::Cookie9Sided,
            M3CoverShape::Cookie9Sided => M3CoverShape::Cookie12Sided,
            M3CoverShape::Cookie12Sided => M3CoverShape::Clover4Leaf,
            M3CoverShape::Clover4Leaf => M3CoverShape::Cookie7Sided,
        }
    }

    pub fn polar_radii(self) -> Vec<f32> {
        let mut out = Vec::with_capacity(POLAR_POINTS);
        for i in 0..POLAR_POINTS {
            let theta = (i as f32 / POLAR_POINTS as f32) * TAU;
            let r = match self {
                M3CoverShape::Cookie7Sided => 0.86 + 0.14 * (7.0 * theta).cos(),
                M3CoverShape::Cookie9Sided => 0.88 + 0.12 * (9.0 * theta).cos(),
                M3CoverShape::Cookie12Sided => 0.90 + 0.10 * (12.0 * theta).cos(),
                M3CoverShape::Clover4Leaf => {
                    0.82 + 0.18 * (4.0 * theta).cos().abs().powf(0.85)
                }
            };
            out.push(r.clamp(0.45, 1.0));
        }
        out
    }
}

pub fn cubic_ease_in_out(t: f32) -> f32 {
    let x = t.clamp(0.0, 1.0);
    if x < 0.5 {
        4.0 * x * x * x
    } else {
        1.0 - (-2.0 * x + 2.0).powi(3) / 2.0
    }
}

pub fn interpolate_cover_radii(from: &[f32], to: &[f32], progress: f32) -> Vec<f32> {
    let e = cubic_ease_in_out(progress);
    from.iter()
        .zip(to.iter())
        .map(|(&a, &b)| a + (b - a) * e)
        .collect()
}

pub fn smooth_fft_bins(prev: &mut [f32; FFT_BINS], raw: &[u8], playing: bool) {
    for (i, slot) in prev.iter_mut().enumerate() {
        let target = if playing {
            raw.get(i).copied().unwrap_or(0) as f32 / 255.0
        } else {
            0.0
        };
        if target > *slot {
            *slot = *slot * 0.35 + target * 0.65;
        } else {
            *slot = (*slot * 0.82).max(target);
            if !playing && *slot < 0.01 {
                *slot = 0.0;
            }
        }
    }
}

pub struct VisualizerProgram<'a> {
    pub bins: &'a [f32; FFT_BINS],
    pub palette: M3Palette,
    pub playing: bool,
    pub bar_count: usize,
}

impl<'a> canvas::Program<Message> for VisualizerProgram<'a> {
    type State = ();

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut frame = canvas::Frame::new(renderer, bounds.size());
        let count = self.bar_count.clamp(16, FFT_BINS);
        let pad_x = 8.0f32;
        let usable_w = (bounds.width - pad_x * 2.0).max(16.0);
        let step = usable_w / count as f32;
        let bar_w = (step * 0.62).clamp(2.5, 14.0);
        let max_h = (bounds.height - 12.0).max(8.0);
        let base_y = bounds.height - 6.0;

        for i in 0..count {
            let src_idx = (i * FFT_BINS) / count;
            let amp = if self.playing {
                self.bins[src_idx].clamp(0.03, 1.0)
            } else {
                0.04
            };
            let h = (amp * max_h).max(3.0);
            let x = pad_x + i as f32 * step + (step - bar_w) * 0.5;
            let y = base_y - h;
            let color = if i % 3 == 0 {
                with_alpha(self.palette.primary, 0.92)
            } else {
                with_alpha(self.palette.on_primary_container, 0.72)
            };
            let rect = Path::rounded_rectangle(
                Point::new(x, y),
                Size::new(bar_w, h),
                (bar_w / 2.0).into(),
            );
            frame.fill(&rect, color);
        }

        vec![frame.into_geometry()]
    }
}

pub fn view_visualizer_canvas<'a>(
    bins: &'a [f32; FFT_BINS],
    palette: M3Palette,
    playing: bool,
    bar_count: usize,
    height: f32,
) -> Element<'a, Message> {
    Canvas::new(VisualizerProgram {
        bins,
        palette,
        playing,
        bar_count,
    })
    .width(Length::Fill)
    .height(Length::Fixed(height))
    .into()
}

pub struct M3CoverProgram {
    pub palette: M3Palette,
    pub shape_index: usize,
    pub morph_progress: f32,
}

impl canvas::Program<Message> for M3CoverProgram {
    type State = ();

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut frame = canvas::Frame::new(renderer, bounds.size());
        let from_shape = M3CoverShape::ALL[self.shape_index % M3CoverShape::ALL.len()];
        let to_shape = from_shape.next();
        let radii = interpolate_cover_radii(
            &from_shape.polar_radii(),
            &to_shape.polar_radii(),
            self.morph_progress,
        );

        let cx = bounds.width * 0.5;
        let cy = bounds.height * 0.5;
        let max_r = (bounds.width.min(bounds.height) * 0.48).max(8.0);

        let path = Path::new(|b| {
            for (i, &rad_scale) in radii.iter().enumerate() {
                let theta = (i as f32 / POLAR_POINTS as f32) * TAU;
                let r = max_r * rad_scale;
                let pt = Point::new(cx + r * theta.cos(), cy + r * theta.sin());
                if i == 0 {
                    b.move_to(pt);
                } else {
                    b.line_to(pt);
                }
            }
            b.close();
        });

        frame.fill(&path, with_alpha(self.palette.primary_container, 0.85));
        frame.stroke(
            &path,
            canvas::Stroke::default()
                .with_color(with_alpha(self.palette.primary, 0.9))
                .with_width(2.0),
        );

        vec![frame.into_geometry()]
    }
}

pub fn view_m3_cover_backdrop(
    palette: M3Palette,
    shape_index: usize,
    morph_progress: f32,
    size: f32,
) -> Element<'static, Message> {
    Canvas::new(M3CoverProgram {
        palette,
        shape_index,
        morph_progress,
    })
    .width(Length::Fixed(size))
    .height(Length::Fixed(size))
    .into()
}
