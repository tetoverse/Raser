use crate::app::{format_mm_ss, ActiveView, Message, RaserApp};
use crate::i18n::t;
use crate::storage::RepeatMode;
use crate::ui::visualizer;
use crate::ui::{
    icon_pill_btn, icon_svg, m3_slider_style, pill_primary_btn, surface_container_style,
    transparent_icon_btn, with_alpha, M3Palette, CORNER_M, CORNER_XL, ICON_EDIT, ICON_HEART,
    ICON_HEART_FILLED, ICON_MUSIC, ICON_NEXT, ICON_PAUSE, ICON_PLAY, ICON_PREV, ICON_QUEUE,
    ICON_REPEAT, ICON_SCISSORS, ICON_SHUFFLE, ICON_VOLUME, ICON_VOLUME_MUTED,
    PLAYBAR_CLASSIC_HEIGHT, PLAYBAR_HEIGHT,
};
use iced::alignment::Vertical;
use iced::mouse;
use iced::widget::canvas::{self, Canvas, Geometry, Path, Stroke};
use iced::widget::{button, column, container, image as iced_image, row, slider, text, tooltip};
use iced::{Alignment, Background, Border, ContentFit, Element, Length, Padding, Point, Rectangle, Renderer, Size, Theme};

#[derive(Debug, Default)]
pub struct ScrubberState {
    dragging: bool,
    hovered: bool,
    hover_x: f32,
}

pub struct ScrubberProgram {
    pub progress: f32,
    pub duration: f64,
    pub palette: M3Palette,
    pub enabled: bool,
    pub playing: bool,
    pub wave_phase: f32,
}

impl ScrubberProgram {
    fn ratio_at_local(&self, bounds: Rectangle, local_x: f32) -> f32 {
        let pad = 8.0;
        let usable = (bounds.width - pad * 2.0).max(1.0);
        ((local_x - pad) / usable).clamp(0.0, 1.0)
    }

    fn ratio_at(&self, bounds: Rectangle, pos: Point) -> f32 {
        self.ratio_at_local(bounds, pos.x - bounds.x)
    }
}

impl canvas::Program<Message> for ScrubberProgram {
    type State = ScrubberState;

    fn update(
        &self,
        state: &mut Self::State,
        event: &canvas::Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<canvas::Action<Message>> {
        if !self.enabled || self.duration <= 0.0 {
            state.dragging = false;
            state.hovered = false;
            return None;
        }

        match event {
            canvas::Event::Mouse(mouse_event) => match mouse_event {
                mouse::Event::CursorMoved { .. } => {
                    if let Some(pos) = cursor.position_in(bounds) {
                        state.hovered = true;
                        state.hover_x = pos.x;
                        let ratio = self.ratio_at_local(bounds, pos.x);
                        let target = ratio as f64 * self.duration;
                        if state.dragging {
                            return Some(canvas::Action::publish(Message::ScrubberDrag(target)).and_capture());
                        }
                        return Some(canvas::Action::request_redraw());
                    } else if state.dragging {
                        if let Some(pos) = cursor.position() {
                            let ratio = self.ratio_at(bounds, pos);
                            let target = ratio as f64 * self.duration;
                            state.hover_x = (pos.x - bounds.x).clamp(0.0, bounds.width);
                            return Some(canvas::Action::publish(Message::ScrubberDrag(target)).and_capture());
                        }
                    } else if state.hovered {
                        state.hovered = false;
                        return Some(canvas::Action::request_redraw());
                    }
                    None
                }
                mouse::Event::ButtonPressed(mouse::Button::Left) => {
                    if let Some(pos) = cursor.position_in(bounds) {
                        state.dragging = true;
                        state.hovered = true;
                        state.hover_x = pos.x;
                        let ratio = self.ratio_at_local(bounds, pos.x);
                        let target = ratio as f64 * self.duration;
                        return Some(canvas::Action::publish(Message::ScrubberDrag(target)).and_capture());
                    }
                    None
                }
                mouse::Event::ButtonReleased(mouse::Button::Left) => {
                    if state.dragging {
                        state.dragging = false;
                        let pos = cursor.position().unwrap_or(Point::new(bounds.x + state.hover_x, bounds.y));
                        let ratio = self.ratio_at(bounds, pos);
                        let target = ratio as f64 * self.duration;
                        return Some(canvas::Action::publish(Message::ScrubberCommit(target)).and_capture());
                    }
                    None
                }
                mouse::Event::CursorLeft => {
                    if !state.dragging && state.hovered {
                        state.hovered = false;
                        return Some(canvas::Action::request_redraw());
                    }
                    None
                }
                _ => None,
            },
            _ => None,
        }
    }

    fn draw(
        &self,
        state: &Self::State,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut frame = canvas::Frame::new(renderer, bounds.size());
        let cy = bounds.height / 2.0 + 4.0;
        let pad = 8.0;
        let track_w = (bounds.width - pad * 2.0).max(1.0);
        let clamped_progress = self.progress.clamp(0.0, 1.0);
        let fill_x = pad + track_w * clamped_progress;

        let thumb_w = if state.hovered || state.dragging { 7.0 } else { 5.0 };
        let thumb_h = if state.hovered || state.dragging { 22.0 } else { 18.0 };
        let gap = 4.5;

        // Inactive track (right of thumb)
        let inactive_start = (fill_x + thumb_w / 2.0 + gap).min(pad + track_w);
        if inactive_start < pad + track_w {
            let bg_line = Path::line(
                Point::new(inactive_start, cy),
                Point::new(pad + track_w, cy),
            );
            frame.stroke(
                &bg_line,
                Stroke::default()
                    .with_color(with_alpha(self.palette.on_surface, 0.30))
                    .with_width(5.0)
                    .with_line_cap(canvas::LineCap::Round),
            );
        }

        // Active squiggly wave (when playing) or straight track (left of thumb)
        let active_end = (fill_x - thumb_w / 2.0 - gap).max(pad);
        if active_end > pad + 1.0 {
            let active_path = if self.playing {
                Path::new(|builder| {
                    let step = 2.5f32;
                    let amp = 3.2f32;
                    let wavelength = 28.0f32;
                    let mut x = pad;
                    let y0 = cy + ((x / wavelength) * std::f32::consts::TAU + self.wave_phase).sin() * amp;
                    builder.move_to(Point::new(x, y0));
                    while x < active_end {
                        x = (x + step).min(active_end);
                        let y = cy + ((x / wavelength) * std::f32::consts::TAU + self.wave_phase).sin() * amp;
                        builder.line_to(Point::new(x, y));
                    }
                })
            } else {
                Path::line(Point::new(pad, cy), Point::new(active_end, cy))
            };
            frame.stroke(
                &active_path,
                Stroke::default()
                    .with_color(self.palette.primary)
                    .with_width(4.5)
                    .with_line_cap(canvas::LineCap::Round),
            );
        }

        // Vertical pill thumb (M3 slider thumb)
        if self.enabled && self.duration > 0.0 {
            let thumb_rect = Path::rounded_rectangle(
                Point::new(fill_x - thumb_w / 2.0, cy - thumb_h / 2.0),
                Size::new(thumb_w, thumb_h),
                (thumb_w / 2.0).into(),
            );
            frame.fill(&thumb_rect, self.palette.primary);
        }

        // Hover timestamp pill tooltip
        if (state.hovered || state.dragging) && self.enabled && self.duration > 0.0 {
            let hover_ratio = self.ratio_at_local(bounds, state.hover_x);
            let hover_secs = hover_ratio as f64 * self.duration;
            let label = format_mm_ss(hover_secs);
            let pill_w = 44.0;
            let pill_h = 18.0;
            let thumb_center_x = pad + track_w * hover_ratio;
            let pill_x =
                (thumb_center_x - pill_w / 2.0).clamp(0.0, (bounds.width - pill_w).max(0.0));
            let pill_y = 0.0;
            let pill_rect = Path::rounded_rectangle(
                Point::new(pill_x, pill_y),
                Size::new(pill_w, pill_h),
                9.0.into(),
            );
            frame.fill(&pill_rect, self.palette.primary);
            frame.fill_text(canvas::Text {
                content: label,
                position: Point::new(pill_x + pill_w / 2.0, pill_y + pill_h / 2.0),
                color: self.palette.on_primary,
                size: 10.5.into(),
                align_x: iced::alignment::Horizontal::Center.into(),
                align_y: Vertical::Center,
                ..Default::default()
            });
        }

        vec![frame.into_geometry()]
    }

    fn mouse_interaction(
        &self,
        state: &Self::State,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        if state.dragging {
            mouse::Interaction::Grabbing
        } else if self.enabled && self.duration > 0.0 && cursor.is_over(bounds) {
            mouse::Interaction::Pointer
        } else {
            mouse::Interaction::default()
        }
    }
}

pub fn view_playbar<'a>(app: &'a RaserApp, p: M3Palette) -> Element<'a, Message> {
    let current = app.current_track();
    let display_title = current
        .as_ref()
        .map(|tr| app.display_title(tr))
        .unwrap_or_else(|| t(app.lang, "no_track_playing").to_string());
    let display_artist = current
        .as_ref()
        .map(|tr| app.display_artist(tr))
        .unwrap_or_else(|| t(app.lang, "select_track").to_string());

    let is_fav = current
        .as_ref()
        .map(|tr| app.is_favorite(tr))
        .unwrap_or(false);

    // Artwork box (72x72 rounded 12px per ui/styles.css:3043)
    let artwork_inner: Element<'a, Message> = if let Some(handle) = app.current_cover_handle() {
        iced_image::Image::new(handle)
            .width(72)
            .height(72)
            .content_fit(ContentFit::Cover)
            .into()
    } else {
        icon_svg(ICON_MUSIC, p.outline, 22.0)
    };
    let artwork = container(artwork_inner)
        .width(72)
        .height(72)
        .clip(true)
        .align_x(Alignment::Center)
        .align_y(Alignment::Center)
        .style(move |_| surface_container_style(p.surface_container_highest, CORNER_M));

    let meta_col = column![
        text(display_title).size(14).color(p.on_surface),
        text(display_artist).size(12).color(p.on_surface_variant),
    ]
    .spacing(3)
    .width(Length::Fill);

    let mut left_row = row![artwork, meta_col]
        .spacing(14)
        .align_y(Vertical::Center);

    if let Some(tr) = current.as_ref() {
        let fav_btn = tooltip(
            button(
                container(icon_svg(
                    if is_fav { ICON_HEART_FILLED } else { ICON_HEART },
                    if is_fav { p.primary } else { p.on_surface_variant },
                    16.0,
                ))
                .width(Length::Fill)
                .height(Length::Fill)
                .align_x(Alignment::Center)
                .align_y(Alignment::Center),
            )
            .width(34)
            .height(34)
            .padding(0)
            .style(move |_, status| transparent_icon_btn(p, false, status))
            .on_press(Message::ToggleFavorite(tr.clone())),
            text(t(app.lang, "favorites")).size(11).color(p.on_surface),
            tooltip::Position::Top,
        )
        .style(move |_| surface_container_style(p.surface_container_high, 8.0));

        let rename_btn = tooltip(
            button(
                container(icon_svg(ICON_EDIT, p.on_surface_variant, 15.0))
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .align_x(Alignment::Center)
                    .align_y(Alignment::Center),
            )
            .width(32)
            .height(32)
            .padding(0)
            .style(move |_, status| transparent_icon_btn(p, false, status))
            .on_press(Message::OpenRenameTrackModal(tr.clone())),
            text(t(app.lang, "rename_track")).size(11).color(p.on_surface),
            tooltip::Position::Top,
        )
        .style(move |_| surface_container_style(p.surface_container_high, 8.0));

        let trim_btn = tooltip(
            button(
                container(icon_svg(ICON_SCISSORS, p.on_surface_variant, 15.0))
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .align_x(Alignment::Center)
                    .align_y(Alignment::Center),
            )
            .width(32)
            .height(32)
            .padding(0)
            .style(move |_, status| transparent_icon_btn(p, false, status))
            .on_press(Message::OpenTrimTrackModal(tr.clone())),
            text(t(app.lang, "trim_best_part"))
                .size(11)
                .color(p.on_surface),
            tooltip::Position::Top,
        )
        .style(move |_| surface_container_style(p.surface_container_high, 8.0));

        left_row = left_row.push(fav_btn).push(rename_btn).push(trim_btn);
    }

    let left_panel = container(left_row)
        .width(Length::FillPortion(1))
        .padding([0, 10])
        .align_y(Vertical::Center);

    // Center Transport Controls
    let shuffle_active = app.snapshot.settings.shuffle;
    let repeat_mode = app.snapshot.settings.repeat_mode;
    let repeat_active = repeat_mode != RepeatMode::Off;

    let shuffle_btn = tooltip(
        button(
            container(icon_svg(
                ICON_SHUFFLE,
                if shuffle_active {
                    p.on_primary
                } else {
                    p.on_surface_variant
                },
                16.0,
            ))
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(Alignment::Center)
            .align_y(Alignment::Center),
        )
        .width(36)
        .height(36)
        .padding(0)
        .style(move |_, status| icon_pill_btn(p, shuffle_active, status))
        .on_press(Message::ToggleShuffle),
        text(t(app.lang, "shuffle")).size(11).color(p.on_surface),
        tooltip::Position::Top,
    )
    .style(move |_| surface_container_style(p.surface_container_high, 8.0));

    let prev_btn = tooltip(
        button(
            container(icon_svg(ICON_PREV, p.on_surface, 18.0))
                .width(Length::Fill)
                .height(Length::Fill)
                .align_x(Alignment::Center)
                .align_y(Alignment::Center),
        )
        .width(40)
        .height(40)
        .padding(0)
        .style(move |_, status| icon_pill_btn(p, false, status))
        .on_press(Message::PrevTrack),
        text(t(app.lang, "previous")).size(11).color(p.on_surface),
        tooltip::Position::Top,
    )
    .style(move |_| surface_container_style(p.surface_container_high, 8.0));

    let is_playing = app.audio_state.playing;
    let is_loading = app.audio_state.loading;
    let play_btn_w = if is_playing { 70.0 } else { 48.0 };
    let play_btn_radius = if is_playing { 18.0 } else { 24.0 };
    let play_content: Element<'a, Message> = if is_loading {
        text("...").size(14).color(p.on_primary).into()
    } else if is_playing {
        icon_svg(ICON_PAUSE, p.on_primary, 20.0)
    } else {
        icon_svg(ICON_PLAY, p.on_primary, 20.0)
    };

    let play_pause_btn = button(
        container(play_content)
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(Alignment::Center)
            .align_y(Alignment::Center),
    )
    .width(Length::Fixed(play_btn_w))
    .height(Length::Fixed(48.0))
    .padding(0)
    .style(move |_, status| {
        let mut s = pill_primary_btn(p, status);
        s.border = Border {
            radius: play_btn_radius.into(),
            ..Default::default()
        };
        s
    })
    .on_press(Message::TogglePlayPause);

    let next_btn = tooltip(
        button(
            container(icon_svg(ICON_NEXT, p.on_surface, 18.0))
                .width(Length::Fill)
                .height(Length::Fill)
                .align_x(Alignment::Center)
                .align_y(Alignment::Center),
        )
        .width(40)
        .height(40)
        .padding(0)
        .style(move |_, status| icon_pill_btn(p, false, status))
        .on_press(Message::NextTrack),
        text(t(app.lang, "next")).size(11).color(p.on_surface),
        tooltip::Position::Top,
    )
    .style(move |_| surface_container_style(p.surface_container_high, 8.0));

    let repeat_label = match repeat_mode {
        RepeatMode::Off => t(app.lang, "repeat_off"),
        RepeatMode::All => t(app.lang, "repeat_all"),
        RepeatMode::One => t(app.lang, "repeat_one"),
    };
    let repeat_inner: Element<'a, Message> = if repeat_mode == RepeatMode::One {
        row![
            icon_svg(ICON_REPEAT, p.on_primary, 15.0),
            text("1").size(10).color(p.on_primary),
        ]
        .spacing(1)
        .align_y(Vertical::Center)
        .into()
    } else {
        icon_svg(
            ICON_REPEAT,
            if repeat_active {
                p.on_primary
            } else {
                p.on_surface_variant
            },
            16.0,
        )
    };

    let repeat_btn = tooltip(
        button(
            container(repeat_inner)
                .width(Length::Fill)
                .height(Length::Fill)
                .align_x(Alignment::Center)
                .align_y(Alignment::Center),
        )
        .width(36)
        .height(36)
        .padding(0)
        .style(move |_, status| icon_pill_btn(p, repeat_active, status))
        .on_press(Message::CycleRepeat),
        text(repeat_label).size(11).color(p.on_surface),
        tooltip::Position::Top,
    )
    .style(move |_| surface_container_style(p.surface_container_high, 8.0));

    let transport = row![shuffle_btn, prev_btn, play_pause_btn, next_btn, repeat_btn]
        .spacing(8)
        .align_y(Vertical::Center);

    // Scrubber row
    let duration = app.effective_duration();
    let display_pos = app.displayed_position();
    let progress = if duration > 0.0 {
        (display_pos / duration).clamp(0.0, 1.0) as f32
    } else {
        0.0
    };

    let scrubber = Canvas::new(ScrubberProgram {
        progress,
        duration,
        palette: p,
        enabled: current.is_some() && duration > 0.0,
        playing: is_playing,
        wave_phase: app.scrubber_wave_phase,
    })
    .width(Length::Fill)
    .height(Length::Fixed(30.0));

    let show_timestamps = !app.snapshot.settings.advanced.autohide_timestamps
        || app.seek_coordinator.scrubber_drag_target.is_some()
        || app.audio_state.playing;
    let ts_color = if show_timestamps {
        p.outline
    } else {
        with_alpha(p.outline, 0.55)
    };

    let scrubber_row = row![
        text(format_mm_ss(display_pos))
            .size(11)
            .color(ts_color)
            .width(Length::Fixed(40.0)),
        scrubber,
        text(format_mm_ss(duration))
            .size(11)
            .color(ts_color)
            .width(Length::Fixed(40.0)),
    ]
    .spacing(10)
    .align_y(Vertical::Center)
    .width(Length::Fill);

    let center_panel = container(
        column![transport, scrubber_row]
            .spacing(2)
            .align_x(Alignment::Center)
            .width(Length::Fill),
    )
    .width(Length::FillPortion(2))
    .max_width(560.0)
    .align_x(Alignment::Center)
    .align_y(Vertical::Center);

    // Right Controls (Queue panel toggle, Speed cycle, Mute, Volume slider)
    let queue_open = app.snapshot.settings.queue_panel_open;
    let queue_btn = tooltip(
        button(
            container(icon_svg(
                ICON_QUEUE,
                if queue_open {
                    p.on_primary
                } else {
                    p.on_surface_variant
                },
                16.0,
            ))
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(Alignment::Center)
            .align_y(Alignment::Center),
        )
        .width(36)
        .height(36)
        .padding(0)
        .style(move |_, status| icon_pill_btn(p, queue_open, status))
        .on_press(Message::ToggleQueuePanel),
        text(t(app.lang, "queue")).size(11).color(p.on_surface),
        tooltip::Position::Top,
    )
    .style(move |_| surface_container_style(p.surface_container_high, 8.0));

    let speed_btn = tooltip(
        button(
            container(
                text(format!("{:.2}x", app.snapshot.settings.speed))
                    .size(11)
                    .color(p.on_surface),
            )
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(Alignment::Center)
            .align_y(Alignment::Center),
        )
        .width(46)
        .height(32)
        .padding(0)
        .style(move |_, status| icon_pill_btn(p, (app.snapshot.settings.speed - 1.0).abs() > 0.01, status))
        .on_press(Message::CycleSpeed),
        text(t(app.lang, "speed")).size(11).color(p.on_surface),
        tooltip::Position::Top,
    )
    .style(move |_| surface_container_style(p.surface_container_high, 8.0));

    let is_muted = app.snapshot.settings.muted || app.snapshot.settings.volume <= 0.001;
    let mute_btn = tooltip(
        button(
            container(icon_svg(
                if is_muted {
                    ICON_VOLUME_MUTED
                } else {
                    ICON_VOLUME
                },
                if is_muted { p.error } else { p.on_surface_variant },
                17.0,
            ))
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(Alignment::Center)
            .align_y(Alignment::Center),
        )
        .width(36)
        .height(36)
        .padding(0)
        .style(move |_, status| transparent_icon_btn(p, false, status))
        .on_press(Message::ToggleMute),
        text(t(app.lang, if is_muted { "unmute" } else { "mute" }))
            .size(11)
            .color(p.on_surface),
        tooltip::Position::Top,
    )
    .style(move |_| surface_container_style(p.surface_container_high, 8.0));

    let vol_val = if app.snapshot.settings.muted {
        0.0
    } else {
        app.snapshot.settings.volume.clamp(0.0, 1.0)
    };
    let vol_slider = slider(0.0..=1.0, vol_val, Message::SetVolume)
        .step(0.01_f32)
        .width(Length::Fixed(80.0))
        .style(move |_, status| m3_slider_style(p, status));

    let vol_pct = text(format!("{}%", (vol_val * 100.0).round() as u32))
        .size(11)
        .color(p.outline)
        .width(Length::Fixed(34.0));

    let vis_active = matches!(app.active_view, ActiveView::Visualizer);
    let vis_btn = tooltip(
        button(
            container(visualizer::view_visualizer_canvas(
                &app.fft_bins,
                p,
                is_playing,
                16,
                26.0,
            ))
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(Alignment::Center)
            .align_y(Alignment::Center),
        )
        .width(72)
        .height(34)
        .padding(4)
        .style(move |_, status| icon_pill_btn(p, vis_active, status))
        .on_press(Message::SwitchView(if vis_active {
            ActiveView::Discover
        } else {
            ActiveView::Visualizer
        })),
        text(t(app.lang, "visualizer")).size(11).color(p.on_surface),
        tooltip::Position::Top,
    )
    .style(move |_| surface_container_style(p.surface_container_high, 8.0));

    let karaoke_active = matches!(app.active_view, ActiveView::Karaoke);
    let karaoke_btn = tooltip(
        button(text("TV").size(11).color(if karaoke_active {
            p.on_primary
        } else {
            p.on_surface_variant
        }))
        .width(34)
        .height(34)
        .padding(0)
        .style(move |_, status| icon_pill_btn(p, karaoke_active, status))
        .on_press(Message::ToggleKaraokeMode),
        text(t(app.lang, "karaoke_mode")).size(11).color(p.on_surface),
        tooltip::Position::Top,
    )
    .style(move |_| surface_container_style(p.surface_container_high, 8.0));

    let right_panel = container(
        row![vis_btn, karaoke_btn, queue_btn, speed_btn, mute_btn, vol_slider, vol_pct]
            .spacing(8)
            .align_y(Vertical::Center),
    )
    .width(Length::FillPortion(1))
    .align_x(Alignment::End)
    .align_y(Vertical::Center)
    .padding(Padding {
        top: 0.0,
        right: 24.0,
        bottom: 0.0,
        left: 0.0,
    });

    let bar_content = if app.is_mini_player {
        row![center_panel]
            .spacing(8)
            .align_y(Vertical::Center)
            .width(Length::Fill)
    } else {
        row![left_panel, center_panel, right_panel]
            .spacing(16)
            .align_y(Vertical::Center)
            .width(Length::Fill)
    };

    let classic = app.snapshot.settings.advanced.classic_sidebar && !app.is_mini_player;
    let bar_height = if classic {
        PLAYBAR_CLASSIC_HEIGHT
    } else {
        PLAYBAR_HEIGHT
    };
    let bar_radius = if classic || app.is_mini_player {
        CORNER_XL
    } else {
        0.0
    };
    let bar_bg = if classic { p.panel_bg } else { p.panel_bg };

    container(bar_content)
        .width(Length::Fill)
        .height(Length::Fixed(bar_height))
        .padding(Padding {
            top: 6.0,
            right: 18.0,
            bottom: 10.0,
            left: 18.0,
        })
        .style(move |_| surface_container_style(bar_bg, bar_radius))
        .into()
}

pub fn view_mini_player<'a>(app: &'a RaserApp, p: M3Palette) -> Element<'a, Message> {
    let current = app.current_track();
    let display_title = current
        .as_ref()
        .map(|tr| app.display_title(tr))
        .unwrap_or_else(|| t(app.lang, "no_track_playing").to_string());
    let display_artist = current
        .as_ref()
        .map(|tr| app.display_artist(tr))
        .unwrap_or_else(|| t(app.lang, "select_track").to_string());

    let cover_inner: Element<'a, Message> = if let Some(handle) = app.current_cover_handle() {
        iced_image::Image::new(handle)
            .width(180)
            .height(180)
            .content_fit(ContentFit::Cover)
            .into()
    } else {
        icon_svg(ICON_MUSIC, p.primary, 52.0)
    };
    let cover = container(cover_inner)
        .width(180)
        .height(180)
        .clip(true)
        .align_x(Alignment::Center)
        .align_y(Alignment::Center)
        .style(move |_| surface_container_style(p.surface_container_highest, CORNER_XL));

    let meta = column![
        text(display_title).size(18).color(p.on_surface),
        text(display_artist).size(13).color(p.on_surface_variant),
    ]
    .spacing(4)
    .align_x(Alignment::Center);

    let playbar = view_playbar(app, p);
    let mut main_area = column![cover, meta, button(text(t(app.lang, "lyrics")).size(12))
        .height(30)
        .padding([0, 14])
        .style(move |_, status| crate::ui::outline_pill_btn(p, app.mini_lyrics_open, status))
        .on_press(Message::ToggleMiniLyrics)]
    .spacing(12)
    .align_x(Alignment::Center);

    if app.mini_lyrics_open {
        main_area = main_area.push(
            container(crate::ui::karaoke::view_lyrics_pane(app, p, true))
                .width(Length::Fill)
                .height(Length::Fixed(150.0))
                .padding([8, 12])
                .style(move |_| surface_container_style(p.surface_container, CORNER_M)),
        );
    }

    container(
        column![
            container(main_area)
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(Alignment::Center)
            .align_y(Alignment::Center)
            .style(move |_| surface_container_style(p.surface_container_low, CORNER_XL)),
            playbar,
        ]
        .spacing(12)
        .padding(Padding {
            top: 0.0,
            right: 12.0,
            bottom: 12.0,
            left: 12.0,
        }),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .style(move |_| container::Style {
        background: Some(Background::Color(p.background)),
        ..Default::default()
    })
    .into()
}
