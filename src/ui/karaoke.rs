use crate::app::{format_mm_ss, Message, RaserApp};
use crate::i18n::t;
use crate::lyrics::{active_lyric_index, active_word_index, ParsedLyrics};
use crate::ui::playbar::ScrubberProgram;
use crate::ui::visualizer;
use crate::ui::{
    icon_pill_btn, icon_svg, outline_pill_btn, pill_primary_btn, surface_container_style,
    transparent_icon_btn, with_alpha, M3Palette, CORNER_L, ICON_CLOSE, ICON_EDIT,
    ICON_MUSIC, ICON_NEXT, ICON_PAUSE, ICON_PLAY, ICON_PREV,
};
use iced::alignment::Vertical;
use iced::widget::canvas::Canvas;
use iced::widget::{
    button, column, container, image as iced_image, row, scrollable, text, text_editor,
};
use iced::{Alignment, Background, Border, Color, ContentFit, Element, Length, Padding};

pub fn view_lyrics_pane<'a>(
    app: &'a RaserApp,
    p: M3Palette,
    compact: bool,
) -> Element<'a, Message> {
    let mut header = row![
        text(t(app.lang, "lyrics")).size(14).color(p.on_surface),
        container(text("").size(1)).width(Length::Fill),
        button(
            row![
                icon_svg(ICON_EDIT, p.on_surface_variant, 13.0),
                text(t(app.lang, "sync_lyrics")).size(11),
            ]
            .spacing(4)
            .align_y(Vertical::Center),
        )
        .height(28)
        .padding([0, 10])
        .style(move |_, status| outline_pill_btn(p, false, status))
        .on_press(Message::OpenSyncLyricsModal),
    ]
    .spacing(6)
    .align_y(Vertical::Center);

    if !compact {
        header = header.push(
            button(text(t(app.lang, "karaoke_mode")).size(11))
                .height(28)
                .padding([0, 10])
                .style(move |_, status| pill_primary_btn(p, status))
                .on_press(Message::ToggleKaraokeMode),
        );
    }

    if app.lyrics_loading {
        return column![
            header,
            container(text(t(app.lang, "loading_lyrics")).size(13).color(p.outline))
                .width(Length::Fill)
                .padding(24)
                .align_x(Alignment::Center),
        ]
        .spacing(10)
        .into();
    }

    let Some(ref parsed) = app.current_lyrics else {
        return column![
            header,
            container(text(t(app.lang, "no_lyrics")).size(13).color(p.outline))
                .width(Length::Fill)
                .padding(24)
                .align_x(Alignment::Center),
        ]
        .spacing(10)
        .into();
    };

    let body = view_lyrics_lines(app, parsed, p, false);
    column![header, scrollable(body).height(Length::Fill)]
        .spacing(10)
        .into()
}

fn view_lyrics_lines<'a>(
    app: &'a RaserApp,
    parsed: &'a ParsedLyrics,
    p: M3Palette,
    large: bool,
) -> Element<'a, Message> {
    let pos = app.displayed_position() + (app.lyrics_user_offset_ms as f64 / 1000.0);
    let active_idx = active_lyric_index(&parsed.lines, pos);
    let align_x = if parsed.is_rtl {
        Alignment::End
    } else if large {
        Alignment::Center
    } else {
        Alignment::Start
    };

    if parsed.lines.is_empty() {
        return container(
            text(&parsed.plain_text)
                .size(if large { 18 } else { 13 })
                .color(p.on_surface_variant),
        )
        .width(Length::Fill)
        .padding([12, 8])
        .into();
    }

    let mut col = column![].spacing(if large { 16 } else { 8 }).align_x(align_x);
    for (idx, line) in parsed.lines.iter().enumerate() {
        let is_active = active_idx == Some(idx);
        let is_past = active_idx.map(|a| idx < a).unwrap_or(false);

        let line_color = if is_active {
            p.primary
        } else if is_past {
            with_alpha(p.on_surface_variant, 0.55)
        } else {
            p.on_surface_variant
        };
        let font_size = if large {
            if is_active { 28 } else { 20 }
        } else if is_active {
            16
        } else {
            13
        };

        let elem: Element<'a, Message> = if is_active && !line.words.is_empty() {
            let active_word = active_word_index(line, pos);
            let mut w_row = row![].spacing(if large { 8 } else { 5 });
            for (w_idx, w) in line.words.iter().enumerate() {
                let w_active = active_word == Some(w_idx);
                let wc = if w_active {
                    p.on_primary_container
                } else if active_word.map(|aw| w_idx < aw).unwrap_or(false) {
                    p.primary
                } else {
                    with_alpha(p.on_surface_variant, 0.72)
                };
                w_row = w_row.push(text(&w.text).size(font_size).color(wc));
            }
            w_row.wrap().into()
        } else {
            text(&line.text).size(font_size).color(line_color).into()
        };

        let seek_target = (line.time_secs - (app.lyrics_user_offset_ms as f64 / 1000.0)).max(0.0);
        col = col.push(
            button(container(elem).width(Length::Fill).align_x(align_x))
                .width(Length::Fill)
                .padding(if large { [8, 24] } else { [4, 8] })
                .style(move |_, status| {
                    let bg = if is_active && large {
                        with_alpha(p.primary_container, 0.35)
                    } else if matches!(status, button::Status::Hovered) {
                        with_alpha(p.on_surface, 0.06)
                    } else {
                        Color::TRANSPARENT
                    };
                    button::Style {
                        background: Some(Background::Color(bg)),
                        text_color: line_color,
                        border: Border {
                            radius: CORNER_L.into(),
                            ..Default::default()
                        },
                        ..Default::default()
                    }
                })
                .on_press(Message::ScrubberCommit(seek_target)),
        );
    }

    col.width(Length::Fill).into()
}

pub fn view_tv_karaoke<'a>(app: &'a RaserApp, p: M3Palette) -> Element<'a, Message> {
    let cur = app.current_track();
    let title = cur
        .as_ref()
        .map(|tr| app.display_title(tr))
        .unwrap_or_else(|| t(app.lang, "no_track_playing").to_string());
    let artist = cur
        .as_ref()
        .map(|tr| app.display_artist(tr))
        .unwrap_or_else(|| t(app.lang, "select_track").to_string());

    let cover_thumb: Element<'a, Message> = if let Some(handle) = app.current_cover_handle() {
        container(
            iced_image::Image::new(handle)
                .width(56)
                .height(56)
                .content_fit(ContentFit::Cover),
        )
        .width(56)
        .height(56)
        .clip(true)
        .style(move |_| surface_container_style(p.surface_container_high, 12.0))
        .into()
    } else {
        container(icon_svg(ICON_MUSIC, p.primary, 24.0))
            .width(56)
            .height(56)
            .align_x(Alignment::Center)
            .align_y(Alignment::Center)
            .style(move |_| surface_container_style(p.surface_container_high, 12.0))
            .into()
    };

    let top_bar = row![
        cover_thumb,
        column![
            text(title).size(18).color(p.on_surface),
            text(artist).size(13).color(p.on_surface_variant),
        ]
        .spacing(2),
        container(text("").size(1)).width(Length::Fill),
        button(text(format!(
            "{}: {}ms",
            t(app.lang, "offset"),
            app.lyrics_user_offset_ms
        ))
        .size(12))
            .height(34)
            .padding([0, 12])
            .style(move |_, status| outline_pill_btn(p, app.karaoke_settings_open, status))
            .on_press(Message::ToggleKaraokeSettings),
        button(
            container(icon_svg(ICON_CLOSE, p.on_surface, 18.0))
                .width(Length::Fill)
                .height(Length::Fill)
                .align_x(Alignment::Center)
                .align_y(Alignment::Center),
        )
        .width(38)
        .height(38)
        .padding(0)
        .style(move |_, status| transparent_icon_btn(p, false, status))
        .on_press(Message::CloseKaraoke),
    ]
    .spacing(14)
    .align_y(Vertical::Center);

    let lyrics_area: Element<'a, Message> = if app.lyrics_loading {
        container(text(t(app.lang, "loading_lyrics")).size(20).color(p.outline))
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(Alignment::Center)
            .align_y(Alignment::Center)
            .into()
    } else if let Some(ref parsed) = app.current_lyrics {
        scrollable(
            container(view_lyrics_lines(app, parsed, p, true))
                .width(Length::Fill)
                .padding([24, 64]),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
    } else {
        container(
            column![
                text(t(app.lang, "no_lyrics")).size(20).color(p.outline),
                button(text(t(app.lang, "sync_lyrics")).size(13))
                    .height(38)
                    .padding([0, 18])
                    .style(move |_, status| pill_primary_btn(p, status))
                    .on_press(Message::OpenSyncLyricsModal),
            ]
            .spacing(14)
            .align_x(Alignment::Center),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(Alignment::Center)
        .align_y(Alignment::Center)
        .into()
    };

    let duration = app.effective_duration();
    let pos = app.displayed_position();
    let progress = if duration > 0.0 {
        (pos / duration).clamp(0.0, 1.0) as f32
    } else {
        0.0
    };

    let scrubber = Canvas::new(ScrubberProgram {
        progress,
        duration,
        palette: p,
        enabled: cur.is_some() && duration > 0.0,
        playing: app.audio_state.playing,
        wave_phase: app.scrubber_wave_phase,
    })
    .width(Length::Fill)
    .height(Length::Fixed(30.0));

    let transport = row![
        button(icon_svg(ICON_PREV, p.on_surface, 18.0))
            .width(40)
            .height(40)
            .padding(10)
            .style(move |_, status| icon_pill_btn(p, false, status))
            .on_press(Message::PrevTrack),
        button(icon_svg(
            if app.audio_state.playing {
                ICON_PAUSE
            } else {
                ICON_PLAY
            },
            p.on_primary,
            20.0,
        ))
        .width(56)
        .height(46)
        .padding(12)
        .style(move |_, status| pill_primary_btn(p, status))
        .on_press(Message::TogglePlayPause),
        button(icon_svg(ICON_NEXT, p.on_surface, 18.0))
            .width(40)
            .height(40)
            .padding(10)
            .style(move |_, status| icon_pill_btn(p, false, status))
            .on_press(Message::NextTrack),
    ]
    .spacing(12)
    .align_y(Vertical::Center);

    let mut bottom_col = column![].spacing(10).align_x(Alignment::Center);
    if app.karaoke_settings_open {
        bottom_col = bottom_col.push(
            container(
                row![
                    text(t(app.lang, "offset")).size(13).color(p.on_surface),
                    button(text("-250ms").size(12))
                        .height(30)
                        .padding([0, 10])
                        .style(move |_, status| outline_pill_btn(p, false, status))
                        .on_press(Message::AdjustLyricsOffset(-250)),
                    button(text("+250ms").size(12))
                        .height(30)
                        .padding([0, 10])
                        .style(move |_, status| outline_pill_btn(p, false, status))
                        .on_press(Message::AdjustLyricsOffset(250)),
                    button(text(t(app.lang, "visualizer")).size(12))
                        .height(30)
                        .padding([0, 12])
                        .style(move |_, status| {
                            outline_pill_btn(p, app.karaoke_show_visualizer, status)
                        })
                        .on_press(Message::ToggleKaraokeVisualizer),
                ]
                .spacing(10)
                .align_y(Vertical::Center),
            )
            .padding([10, 16])
            .style(move |_| surface_container_style(p.surface_container_high, CORNER_L)),
        );
    }
    if app.karaoke_show_visualizer {
        bottom_col = bottom_col.push(visualizer::view_visualizer_canvas(
            &app.fft_bins,
            p,
            app.audio_state.playing,
            64,
            56.0,
        ));
    }
    bottom_col = bottom_col
        .push(
            row![
                text(format_mm_ss(pos)).size(12).color(p.outline),
                scrubber,
                text(format_mm_ss(duration)).size(12).color(p.outline),
            ]
            .spacing(10)
            .align_y(Vertical::Center),
        )
        .push(transport);

    container(
        column![top_bar, lyrics_area, bottom_col]
            .spacing(16)
            .padding(Padding::from([20, 28])),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .style(move |_| container::Style {
        background: Some(Background::Color(p.page_bg)),
        ..Default::default()
    })
    .into()
}

pub fn view_sync_editor_content<'a>(
    app: &'a RaserApp,
    p: M3Palette,
    editor_content: &'a text_editor::Content,
    sync_lines: &'a [String],
    stamped_times: &'a [Option<f64>],
    current_idx: usize,
    is_syncing: bool,
) -> Element<'a, Message> {
    if is_syncing {
        let mut lines_col = column![].spacing(6);
        for (i, line) in sync_lines.iter().enumerate() {
            let stamped = stamped_times
                .get(i)
                .and_then(|o| *o)
                .map(format_mm_ss)
                .unwrap_or_else(|| "--:--".to_string());
            let active = i == current_idx;
            lines_col = lines_col.push(
                container(
                    row![
                        text(stamped).size(12).color(p.primary).width(56),
                        text(line).size(14).color(if active {
                            p.on_primary_container
                        } else {
                            p.on_surface_variant
                        }),
                    ]
                    .spacing(10),
                )
                .width(Length::Fill)
                .padding([8, 12])
                .style(move |_| {
                    surface_container_style(
                        if active {
                            p.primary_container
                        } else {
                            p.surface_container
                        },
                        10.0,
                    )
                }),
            );
        }
        column![
            text(t(app.lang, "sync_tap_hint")).size(12).color(p.primary),
            scrollable(lines_col).height(240.0),
            row![
                button(text(t(app.lang, "stamp_line")).size(13))
                    .height(38)
                    .padding([0, 18])
                    .style(move |_, status| pill_primary_btn(p, status))
                    .on_press(Message::SyncStampCurrentLine),
                container(text("").size(1)).width(Length::Fill),
                button(text(t(app.lang, "save")).size(13))
                    .height(38)
                    .padding([0, 18])
                    .style(move |_, status| outline_pill_btn(p, true, status))
                    .on_press(Message::SubmitSyncLyrics),
                button(text(t(app.lang, "cancel")).size(13))
                    .height(38)
                    .padding([0, 16])
                    .style(move |_, status| outline_pill_btn(p, false, status))
                    .on_press(Message::CloseModal),
            ]
            .spacing(8),
        ]
        .spacing(12)
        .into()
    } else {
        column![
            text(t(app.lang, "sync_paste_hint"))
                .size(12)
                .color(p.on_surface_variant),
            text_editor(editor_content)
                .height(220.0)
                .on_action(Message::ModalSyncEditorAction),
            row![
                button(text(t(app.lang, "start_sync")).size(13))
                    .height(38)
                    .padding([0, 18])
                    .style(move |_, status| pill_primary_btn(p, status))
                    .on_press(Message::StartSyncTapMode),
                button(text(t(app.lang, "save")).size(13))
                    .height(38)
                    .padding([0, 16])
                    .style(move |_, status| outline_pill_btn(p, false, status))
                    .on_press(Message::SubmitSyncLyrics),
                container(text("").size(1)).width(Length::Fill),
                button(text(t(app.lang, "cancel")).size(13))
                    .height(38)
                    .padding([0, 16])
                    .style(move |_, status| outline_pill_btn(p, false, status))
                    .on_press(Message::CloseModal),
            ]
            .spacing(8),
        ]
        .spacing(12)
        .into()
    }
}
