use crate::app::{
    format_bytes, format_mm_ss, sort_tracks_with_pins, ActiveView, Message, ModalState,
    QueuePanelTab, RaserApp, SettingsTab,
};
use crate::i18n::t;
use crate::storage::{TrackRecord, TrackSortMode};
use crate::ui::{
    icon_pill_btn, icon_svg, m3_slider_style, modal_card_style, modal_input_style,
    outline_pill_btn, pill_primary_btn, search_input_style, surface_card_bordered,
    surface_container_style, track_row_btn, transparent_icon_btn, with_alpha, M3Palette, CORNER_L,
    CORNER_M, CORNER_XL, ICON_ALBUMS, ICON_BACK, ICON_CLOSE, ICON_EDIT, ICON_FOLDER, ICON_HEART,
    ICON_HEART_FILLED, ICON_HISTORY, ICON_MUSIC, ICON_PIN, ICON_PLAY, ICON_PLUS, ICON_QUEUE,
    ICON_SCISSORS, ICON_SEARCH, ICON_TRASH, RIGHT_PANEL_WIDTH, TOP_NAV_HEIGHT,
};
use iced::alignment::Vertical;
use iced::widget::{
    button, column, container, mouse_area, row, scrollable, slider, text, text_input, tooltip, Id,
};
use iced::{Alignment, Background, Border, Color, Element, Length, Padding};

pub fn search_input_id() -> Id {
    Id::new("raser-main-search-input")
}

pub fn view_top_nav<'a>(app: &'a RaserApp, p: M3Palette) -> Element<'a, Message> {
    let search_bar = mouse_area(
        container(
            row![
                icon_svg(ICON_SEARCH, p.on_surface_variant, 18.0),
                text_input(t(app.lang, "search_placeholder"), &app.search_query)
                    .id(search_input_id())
                    .on_input(Message::SearchQueryChanged)
                    .size(15)
                    .padding([10, 8])
                    .style(move |_, status| search_input_style(p, status)),
            ]
            .spacing(8)
            .align_y(Vertical::Center)
            .padding([0, 16]),
        )
        .width(Length::Fill)
        .max_width(560.0)
        .height(Length::Fixed(44.0))
        .align_y(Vertical::Center)
        .style(move |_| surface_container_style(p.surface_container, 9999.0)),
    )
    .on_press(Message::FocusSearchBox);

    let mut search_row = row![search_bar].align_y(Vertical::Center);
    if !app.search_query.is_empty() {
        search_row = search_row.push(
            button(
                container(icon_svg(ICON_CLOSE, p.on_surface_variant, 14.0))
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .align_x(Alignment::Center)
                    .align_y(Alignment::Center),
            )
            .width(32)
            .height(32)
            .padding(0)
            .style(move |_, status| transparent_icon_btn(p, false, status))
            .on_press(Message::SearchQueryChanged(String::new())),
        );
    }

    let status_pill = container(
        row![
            container(text("").size(1))
                .width(8)
                .height(8)
                .style(move |_| surface_container_style(p.primary, 999.0)),
            text(format!(
                "{} {}",
                app.snapshot.local_tracks.len() + app.snapshot.favorites.len(),
                t(app.lang, "tracks")
            ))
            .size(12)
            .color(p.on_surface_variant),
        ]
        .spacing(8)
        .align_y(Vertical::Center),
    )
    .height(Length::Fixed(36.0))
    .padding([0, 14])
    .align_y(Vertical::Center)
    .style(move |_| {
        surface_card_bordered(
            p.surface_container,
            with_alpha(p.outline_variant, 0.45),
            9999.0,
        )
    });

    container(
        row![
            container(text("").size(1)).width(Length::FillPortion(1)),
            container(search_row)
                .width(Length::FillPortion(3))
                .align_x(Alignment::Center),
            container(status_pill)
                .width(Length::FillPortion(1))
                .align_x(Alignment::End),
        ]
        .spacing(16)
        .align_y(Vertical::Center),
    )
    .width(Length::Fill)
    .height(Length::Fixed(TOP_NAV_HEIGHT))
    .padding([0, 24])
    .align_y(Vertical::Center)
    .into()
}

pub fn view_main_content<'a>(app: &'a RaserApp, p: M3Palette) -> Element<'a, Message> {
    let classic = app.snapshot.settings.advanced.classic_sidebar;
    let body: Element<'a, Message> = match &app.active_view {
        ActiveView::Discover => view_discover(app, p),
        ActiveView::Favorites => view_favorites(app, p),
        ActiveView::Albums => view_albums(app, p),
        ActiveView::Local => view_local(app, p),
        ActiveView::History => view_history(app, p),
        ActiveView::Queue => view_queue(app, p),
        ActiveView::PlaylistDetail(id) => view_playlist_detail(app, p, id),
        ActiveView::Settings => view_settings(app, p),
    };

    let mut content_row = row![
        container(scrollable(body).width(Length::Fill).height(Length::Fill))
            .width(Length::Fill)
            .height(Length::Fill)
            .padding(if classic {
                Padding::from([24, 32])
            } else {
                Padding {
                    top: 8.0,
                    right: 28.0,
                    bottom: 20.0,
                    left: 28.0,
                }
            })
            .style(move |_| {
                surface_container_style(
                    if classic {
                        p.surface_container_low
                    } else {
                        Color::TRANSPARENT
                    },
                    if classic { CORNER_XL } else { 0.0 },
                )
            })
    ]
    .spacing(12)
    .width(Length::Fill)
    .height(Length::Fill);

    if app.snapshot.settings.queue_panel_open {
        content_row = content_row.push(view_right_panel(app, p));
    }

    content_row.into()
}

fn view_empty_state<'a>(
    p: M3Palette,
    icon: &'static str,
    title: &'a str,
    subtitle: &'a str,
) -> Element<'a, Message> {
    container(
        column![
            container(icon_svg(icon, p.primary, 32.0))
                .width(72)
                .height(72)
                .align_x(Alignment::Center)
                .align_y(Alignment::Center)
                .style(move |_| surface_container_style(p.surface_container_high, 9999.0)),
            text(title).size(16).color(p.on_surface),
            text(subtitle).size(13).color(p.outline),
        ]
        .spacing(10)
        .align_x(Alignment::Center),
    )
    .width(Length::Fill)
    .padding([56, 24])
    .align_x(Alignment::Center)
    .into()
}

fn m3_switch<'a>(p: M3Palette, active: bool, on_toggle: Message) -> Element<'a, Message> {
    let track = button(
        container(
            container(text("").size(1))
                .width(if active { 22 } else { 16 })
                .height(if active { 22 } else { 16 })
                .style(move |_| {
                    surface_container_style(
                        if active { p.on_primary } else { p.outline },
                        9999.0,
                    )
                }),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .padding([0, 4])
        .align_x(if active {
            Alignment::End
        } else {
            Alignment::Start
        })
        .align_y(Vertical::Center),
    )
    .width(52)
    .height(32)
    .padding(0)
    .style(move |_, _| button::Style {
        background: Some(Background::Color(if active {
            p.primary
        } else {
            p.surface_container_highest
        })),
        border: Border {
            color: if active { Color::TRANSPARENT } else { p.outline },
            width: if active { 0.0 } else { 2.0 },
            radius: 9999.0.into(),
        },
        ..Default::default()
    })
    .on_press(on_toggle);

    track.into()
}

#[derive(Clone, Copy)]
enum TrackContext {
    Library,
    Local,
    Playlist,
    Queue,
    History,
}

fn view_track_list<'a>(
    app: &'a RaserApp,
    p: M3Palette,
    tracks: &[TrackRecord],
    ctx: TrackContext,
    playlist_id: Option<&str>,
) -> Element<'a, Message> {
    let mut col = column![].spacing(4).width(Length::Fill);
    let active_track = app.current_track();

    for (idx, tr) in tracks.iter().enumerate() {
        let is_playing_this = active_track
            .as_ref()
            .map(|cur| cur.matches(tr))
            .unwrap_or(false);
        let is_fav = app.is_favorite(tr);
        let is_pinned = app.is_pinned(tr);
        let title = app.display_title(tr);
        let artist = app.display_artist(tr);
        let dur_str = if tr.duration > 0.0 {
            format_mm_ss(tr.duration)
        } else {
            "--:--".to_string()
        };

        let tracks_vec = tracks.to_vec();
        let play_msg = match ctx {
            TrackContext::Queue => Message::JumpToQueueIndex(idx),
            _ => Message::PlayTracks(tracks_vec, idx),
        };

        let row_num = container(
            text(if is_playing_this {
                "▶".to_string()
            } else {
                (idx + 1).to_string()
            })
            .size(12)
            .color(if is_playing_this {
                p.primary
            } else {
                p.outline
            }),
        )
        .width(24)
        .align_x(Alignment::Center);

        let thumb = container(icon_svg(
            ICON_MUSIC,
            if is_playing_this {
                p.primary
            } else {
                p.outline
            },
            18.0,
        ))
        .width(44)
        .height(44)
        .align_x(Alignment::Center)
        .align_y(Alignment::Center)
        .style(move |_| surface_container_style(p.surface_container_high, CORNER_M));

        let info = column![
            text(title).size(14).color(if is_playing_this {
                p.primary
            } else {
                p.on_surface
            }),
            text(artist).size(12).color(p.on_surface_variant),
        ]
        .spacing(3)
        .width(Length::Fill);

        let dur = text(dur_str).size(12).color(p.outline);

        let mut actions = row![].spacing(4).align_y(Vertical::Center);

        if matches!(ctx, TrackContext::Library | TrackContext::Playlist) {
            let pin_btn = tooltip(
                button(
                    container(icon_svg(
                        ICON_PIN,
                        if is_pinned {
                            p.primary
                        } else {
                            p.on_surface_variant
                        },
                        14.0,
                    ))
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .align_x(Alignment::Center)
                    .align_y(Alignment::Center),
                )
                .width(30)
                .height(30)
                .padding(0)
                .style(move |_, status| transparent_icon_btn(p, is_pinned, status))
                .on_press(Message::TogglePinTrack(tr.clone())),
                text(t(app.lang, if is_pinned { "unpin_track" } else { "pin_track" }))
                    .size(11)
                    .color(p.on_surface),
                tooltip::Position::Top,
            )
            .style(move |_| surface_container_style(p.surface_container_high, 8.0));
            actions = actions.push(pin_btn);
        }

        let fav_btn = button(
            container(icon_svg(
                if is_fav {
                    ICON_HEART_FILLED
                } else {
                    ICON_HEART
                },
                if is_fav {
                    p.primary
                } else {
                    p.on_surface_variant
                },
                14.0,
            ))
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(Alignment::Center)
            .align_y(Alignment::Center),
        )
        .width(30)
        .height(30)
        .padding(0)
        .style(move |_, status| transparent_icon_btn(p, false, status))
        .on_press(Message::ToggleFavorite(tr.clone()));

        let add_pl_btn = tooltip(
            button(
                container(icon_svg(ICON_PLUS, p.on_surface_variant, 14.0))
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .align_x(Alignment::Center)
                    .align_y(Alignment::Center),
            )
            .width(30)
            .height(30)
            .padding(0)
            .style(move |_, status| transparent_icon_btn(p, false, status))
            .on_press(Message::OpenAddToPlaylistModal(tr.clone())),
            text(t(app.lang, "add_to_playlist"))
                .size(11)
                .color(p.on_surface),
            tooltip::Position::Top,
        )
        .style(move |_| surface_container_style(p.surface_container_high, 8.0));

        let rename_btn = button(
            container(icon_svg(ICON_EDIT, p.on_surface_variant, 14.0))
                .width(Length::Fill)
                .height(Length::Fill)
                .align_x(Alignment::Center)
                .align_y(Alignment::Center),
        )
        .width(30)
        .height(30)
        .padding(0)
        .style(move |_, status| transparent_icon_btn(p, false, status))
        .on_press(Message::OpenRenameTrackModal(tr.clone()));

        let trim_btn = button(
            container(icon_svg(ICON_SCISSORS, p.on_surface_variant, 14.0))
                .width(Length::Fill)
                .height(Length::Fill)
                .align_x(Alignment::Center)
                .align_y(Alignment::Center),
        )
        .width(30)
        .height(30)
        .padding(0)
        .style(move |_, status| transparent_icon_btn(p, false, status))
        .on_press(Message::OpenTrimTrackModal(tr.clone()));

        actions = actions
            .push(fav_btn)
            .push(add_pl_btn)
            .push(rename_btn)
            .push(trim_btn);

        match ctx {
            TrackContext::Local => {
                actions = actions.push(
                    button(
                        container(icon_svg(ICON_TRASH, p.error, 14.0))
                            .width(Length::Fill)
                            .height(Length::Fill)
                            .align_x(Alignment::Center)
                            .align_y(Alignment::Center),
                    )
                    .width(30)
                    .height(30)
                    .padding(0)
                    .style(move |_, status| transparent_icon_btn(p, false, status))
                    .on_press(Message::RemoveLocalTrack(tr.clone())),
                );
            }
            TrackContext::Playlist => {
                if let Some(pid) = playlist_id {
                    actions = actions.push(
                        button(
                            container(icon_svg(ICON_TRASH, p.error, 14.0))
                                .width(Length::Fill)
                                .height(Length::Fill)
                                .align_x(Alignment::Center)
                                .align_y(Alignment::Center),
                        )
                        .width(30)
                        .height(30)
                        .padding(0)
                        .style(move |_, status| transparent_icon_btn(p, false, status))
                        .on_press(Message::RemoveTrackFromPlaylist(pid.to_string(), tr.clone())),
                    );
                }
            }
            TrackContext::Queue => {
                actions = actions.push(
                    button(
                        container(icon_svg(ICON_CLOSE, p.error, 14.0))
                            .width(Length::Fill)
                            .height(Length::Fill)
                            .align_x(Alignment::Center)
                            .align_y(Alignment::Center),
                    )
                    .width(30)
                    .height(30)
                    .padding(0)
                    .style(move |_, status| transparent_icon_btn(p, false, status))
                    .on_press(Message::RemoveFromQueue(idx)),
                );
            }
            TrackContext::Library | TrackContext::History => {}
        }

        let main_click = button(
            row![row_num, thumb, info, dur]
                .spacing(14)
                .align_y(Vertical::Center)
                .width(Length::Fill),
        )
        .width(Length::Fill)
        .height(Length::Fixed(64.0))
        .padding([10, 14])
        .style(move |_, status| track_row_btn(p, is_playing_this, status))
        .on_press(play_msg);

        let full_row = container(
            row![main_click, actions]
                .spacing(8)
                .align_y(Vertical::Center)
                .padding(Padding {
                    top: 0.0,
                    right: 8.0,
                    bottom: 0.0,
                    left: 0.0,
                }),
        )
        .width(Length::Fill)
        .style(move |_| {
            surface_container_style(
                if is_playing_this {
                    p.surface_container_high
                } else {
                    Color::TRANSPARENT
                },
                20.0,
            )
        });

        col = col.push(full_row);
    }

    col.into()
}

fn view_discover<'a>(app: &'a RaserApp, p: M3Palette) -> Element<'a, Message> {
    let quick_tile = |title_key: &'static str, count: usize, view: ActiveView, icon: &'static str| {
        button(
            row![
                container(icon_svg(icon, p.primary, 22.0))
                    .width(64)
                    .height(64)
                    .align_x(Alignment::Center)
                    .align_y(Alignment::Center)
                    .style(move |_| surface_container_style(p.surface_container_highest, CORNER_M)),
                column![
                    text(t(app.lang, title_key)).size(15).color(p.on_surface),
                    text(format!("{} {}", count, t(app.lang, "tracks")))
                        .size(12)
                        .color(p.on_surface_variant),
                ]
                .spacing(2),
            ]
            .spacing(12)
            .align_y(Vertical::Center),
        )
        .width(Length::Fill)
        .height(Length::Fixed(64.0))
        .padding(0)
        .style(move |_, status| {
            let bg = match status {
                button::Status::Hovered => p.surface_container_highest,
                _ => with_alpha(p.surface_container_high, 0.72),
            };
            button::Style {
                background: Some(Background::Color(bg)),
                text_color: p.on_surface,
                border: Border {
                    radius: CORNER_M.into(),
                    ..Default::default()
                },
                ..Default::default()
            }
        })
        .on_press(Message::SwitchView(view))
    };

    let quick_grid = row![
        quick_tile("favorites", app.snapshot.favorites.len(), ActiveView::Favorites, ICON_HEART_FILLED),
        quick_tile("local_files", app.snapshot.local_tracks.len(), ActiveView::Local, ICON_FOLDER),
        quick_tile("history", app.snapshot.history.len(), ActiveView::History, ICON_HISTORY),
    ]
    .spacing(12);

    let mix_card = |badge: &'a str, title: String, sub: String, on_click: Message| {
        button(
            column![
                container(text(badge).size(10).color(p.primary))
                    .padding([3, 10])
                    .style(move |_| surface_container_style(p.surface_container_highest, 9999.0)),
                text(title).size(15).color(p.on_surface),
                text(sub).size(12).color(p.on_surface_variant),
            ]
            .spacing(8),
        )
        .width(Length::Fill)
        .padding(20)
        .style(move |_, status| {
            let bg = match status {
                button::Status::Hovered => p.surface_container_high,
                _ => p.surface_container,
            };
            button::Style {
                background: Some(Background::Color(bg)),
                text_color: p.on_surface,
                border: Border {
                    radius: CORNER_L.into(),
                    ..Default::default()
                },
                ..Default::default()
            }
        })
        .on_press(on_click)
    };

    let cards = row![
        mix_card(
            "M1",
            t(app.lang, "local_files").to_string(),
            format!("{} {}", app.snapshot.local_tracks.len(), t(app.lang, "tracks")),
            Message::SwitchView(ActiveView::Local),
        ),
        mix_card(
            "M3",
            t(app.lang, "playlists").to_string(),
            format!("{} {}", app.snapshot.playlists.len(), t(app.lang, "playlists")),
            Message::OpenCreatePlaylistModal,
        ),
        mix_card(
            "Q",
            t(app.lang, "queue").to_string(),
            format!("{} {}", app.queue.len(), t(app.lang, "tracks")),
            Message::SwitchView(ActiveView::Queue),
        ),
    ]
    .spacing(16);

    let mut content = column![
        row![
            text(t(app.lang, "home")).size(24).color(p.on_surface),
            container(text("").size(1)).width(Length::Fill),
            text(t(app.lang, "discover_local_hint"))
                .size(12)
                .color(p.outline),
        ]
        .align_y(Vertical::Center),
        quick_grid,
        text(t(app.lang, "for_you")).size(16).color(p.on_surface),
        cards,
    ]
    .spacing(20);

    if !app.snapshot.history.is_empty() {
        let recent_slice: Vec<TrackRecord> =
            app.snapshot.history.iter().take(6).cloned().collect();
        content = content
            .push(text(t(app.lang, "jump_back_in")).size(16).color(p.on_surface))
            .push(view_track_list(
                app,
                p,
                &recent_slice,
                TrackContext::History,
                None,
            ));
    }

    content.into()
}

fn sort_label(app: &RaserApp, mode: TrackSortMode) -> &'static str {
    match mode {
        TrackSortMode::Manual => t(app.lang, "sort_manual"),
        TrackSortMode::AddedDesc => t(app.lang, "sort_added_desc"),
        TrackSortMode::AddedAsc => t(app.lang, "sort_added_asc"),
    }
}

fn view_favorites<'a>(app: &'a RaserApp, p: M3Palette) -> Element<'a, Message> {
    let sort_mode = app.snapshot.settings.favorites_sort;
    let filtered = sort_tracks_with_pins(
        &app.snapshot.favorites,
        sort_mode,
        &app.snapshot.settings.pinned_tracks,
        &app.search_query,
        &app.snapshot.track_renames,
    );

    let header = row![
        column![
            text(t(app.lang, "favorites")).size(22).color(p.on_surface),
            text(format!(
                "{} {}",
                app.snapshot.favorites.len(),
                t(app.lang, "tracks")
            ))
            .size(12)
            .color(p.outline),
        ]
        .spacing(2),
        container(text("").size(1)).width(Length::Fill),
        button(text(sort_label(app, sort_mode)).size(13))
            .height(40)
            .padding([0, 18])
            .style(move |_, status| outline_pill_btn(p, false, status))
            .on_press(Message::CycleFavoritesSort),
    ]
    .align_y(Vertical::Center);

    let list_elem = if filtered.is_empty() {
        view_empty_state(
            p,
            ICON_HEART,
            t(app.lang, "empty_favorites_title"),
            t(app.lang, "empty_favorites_sub"),
        )
    } else {
        view_track_list(app, p, &filtered, TrackContext::Library, None)
    };

    column![header, list_elem].spacing(18).into()
}

fn view_albums<'a>(app: &'a RaserApp, p: M3Palette) -> Element<'a, Message> {
    let header = row![
        text(t(app.lang, "albums")).size(22).color(p.on_surface),
        container(text("").size(1)).width(Length::Fill),
        text(format!("{}", app.snapshot.saved_albums.len()))
            .size(12)
            .color(p.outline),
    ]
    .align_y(Vertical::Center);

    if app.snapshot.saved_albums.is_empty() {
        return column![
            header,
            view_empty_state(
                p,
                ICON_ALBUMS,
                t(app.lang, "empty_albums_title"),
                t(app.lang, "empty_albums_sub"),
            )
        ]
        .spacing(18)
        .into();
    }

    let mut cards = row![].spacing(16);
    for album in app.snapshot.saved_albums.iter().take(12) {
        let title = if album.title.trim().is_empty() {
            "Album"
        } else {
            album.title.as_str()
        };
        let artist = if album.artist.trim().is_empty() {
            "Artist"
        } else {
            album.artist.as_str()
        };
        cards = cards.push(
            container(
                column![
                    container(icon_svg(ICON_ALBUMS, p.primary, 40.0))
                        .width(140)
                        .height(140)
                        .align_x(Alignment::Center)
                        .align_y(Alignment::Center)
                        .style(move |_| surface_container_style(p.surface_container_high, CORNER_L)),
                    text(title).size(13).color(p.on_surface),
                    text(artist).size(12).color(p.on_surface_variant),
                ]
                .spacing(6),
            )
            .width(140),
        );
    }

    column![header, cards].spacing(18).into()
}

fn view_local<'a>(app: &'a RaserApp, p: M3Palette) -> Element<'a, Message> {
    let filtered = sort_tracks_with_pins(
        &app.snapshot.local_tracks,
        TrackSortMode::Manual,
        &[],
        &app.search_query,
        &app.snapshot.track_renames,
    );

    let header = row![
        column![
            text(t(app.lang, "local_files")).size(22).color(p.on_surface),
            text(format!(
                "{} {}",
                app.snapshot.local_tracks.len(),
                t(app.lang, "tracks")
            ))
            .size(12)
            .color(p.outline),
        ]
        .spacing(2),
        container(text("").size(1)).width(Length::Fill),
        button(
            row![
                icon_svg(ICON_PLUS, p.on_primary, 16.0),
                text(t(app.lang, "add_files")).size(13),
            ]
            .spacing(8)
            .align_y(Vertical::Center),
        )
        .height(40)
        .padding([0, 20])
        .style(move |_, status| pill_primary_btn(p, status))
        .on_press(Message::PickLocalFiles),
        button(
            row![
                icon_svg(ICON_FOLDER, p.on_surface_variant, 16.0),
                text(t(app.lang, "add_folder")).size(13),
            ]
            .spacing(8)
            .align_y(Vertical::Center),
        )
        .height(40)
        .padding([0, 18])
        .style(move |_, status| outline_pill_btn(p, false, status))
        .on_press(Message::PickLocalFolder),
    ]
    .spacing(10)
    .align_y(Vertical::Center);

    let list_elem = if filtered.is_empty() {
        view_empty_state(
            p,
            ICON_FOLDER,
            t(app.lang, "empty_local_title"),
            t(app.lang, "empty_local_sub"),
        )
    } else {
        view_track_list(app, p, &filtered, TrackContext::Local, None)
    };

    column![header, list_elem].spacing(18).into()
}

fn view_history<'a>(app: &'a RaserApp, p: M3Palette) -> Element<'a, Message> {
    let filtered = sort_tracks_with_pins(
        &app.snapshot.history,
        TrackSortMode::Manual,
        &[],
        &app.search_query,
        &app.snapshot.track_renames,
    );

    let header = row![
        text(t(app.lang, "history")).size(22).color(p.on_surface),
        container(text("").size(1)).width(Length::Fill),
        button(text(t(app.lang, "clear_history")).size(13))
            .height(40)
            .padding([0, 18])
            .style(move |_, status| outline_pill_btn(p, false, status))
            .on_press(Message::ClearHistory),
    ]
    .align_y(Vertical::Center);

    let list_elem = if filtered.is_empty() {
        view_empty_state(
            p,
            ICON_HISTORY,
            t(app.lang, "empty_history_title"),
            t(app.lang, "empty_history_sub"),
        )
    } else {
        view_track_list(app, p, &filtered, TrackContext::History, None)
    };

    column![header, list_elem].spacing(18).into()
}

fn view_queue<'a>(app: &'a RaserApp, p: M3Palette) -> Element<'a, Message> {
    let header = row![
        text(t(app.lang, "queue")).size(22).color(p.on_surface),
        container(text("").size(1)).width(Length::Fill),
        button(text(t(app.lang, "clear_queue")).size(13))
            .height(40)
            .padding([0, 18])
            .style(move |_, status| outline_pill_btn(p, false, status))
            .on_press(Message::ClearQueue),
    ]
    .align_y(Vertical::Center);

    let list_elem = if app.queue.is_empty() {
        view_empty_state(
            p,
            ICON_QUEUE,
            t(app.lang, "empty_queue_title"),
            t(app.lang, "empty_queue_sub"),
        )
    } else {
        view_track_list(app, p, &app.queue, TrackContext::Queue, None)
    };

    column![header, list_elem].spacing(18).into()
}

fn view_playlist_detail<'a>(
    app: &'a RaserApp,
    p: M3Palette,
    playlist_id: &str,
) -> Element<'a, Message> {
    let Some(pl) = app.snapshot.playlists.iter().find(|pl| pl.id == playlist_id) else {
        return view_discover(app, p);
    };

    let sort_mode = pl
        .sort_mode
        .as_deref()
        .map(TrackSortMode::from_str_lossy)
        .unwrap_or(app.snapshot.settings.playlist_sort);

    let filtered = sort_tracks_with_pins(
        &pl.tracks,
        sort_mode,
        &app.snapshot.settings.pinned_tracks,
        &app.search_query,
        &app.snapshot.track_renames,
    );

    let back_btn = button(
        container(icon_svg(ICON_BACK, p.on_surface_variant, 18.0))
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(Alignment::Center)
            .align_y(Alignment::Center),
    )
    .width(40)
    .height(40)
    .padding(0)
    .style(move |_, status| icon_pill_btn(p, false, status))
    .on_press(Message::NavigateBack);

    let cover = container(icon_svg(ICON_MUSIC, p.primary, 52.0))
        .width(160)
        .height(160)
        .align_x(Alignment::Center)
        .align_y(Alignment::Center)
        .style(move |_| surface_container_style(p.surface_container_high, CORNER_L));

    let desc_text = pl.description.as_deref().unwrap_or("");
    let pl_id_play = pl.id.clone();
    let pl_id_sort = pl.id.clone();
    let pl_id_ren = pl.id.clone();
    let pl_id_del = pl.id.clone();

    let actions = row![
        button(
            row![
                icon_svg(ICON_PLAY, p.on_primary, 16.0),
                text(t(app.lang, "play")).size(14),
            ]
            .spacing(8)
            .align_y(Vertical::Center),
        )
        .height(40)
        .padding([0, 22])
        .style(move |_, status| pill_primary_btn(p, status))
        .on_press(Message::PlayPlaylist(pl_id_play)),
        button(text(sort_label(app, sort_mode)).size(13))
            .height(40)
            .padding([0, 18])
            .style(move |_, status| outline_pill_btn(p, false, status))
            .on_press(Message::CyclePlaylistSort(pl_id_sort)),
        button(
            row![
                icon_svg(ICON_EDIT, p.on_surface_variant, 14.0),
                text(t(app.lang, "rename_playlist")).size(13),
            ]
            .spacing(6)
            .align_y(Vertical::Center),
        )
        .height(40)
        .padding([0, 16])
        .style(move |_, status| outline_pill_btn(p, false, status))
        .on_press(Message::OpenRenamePlaylistModal(pl_id_ren)),
        button(
            container(icon_svg(ICON_TRASH, p.error, 16.0))
                .width(Length::Fill)
                .height(Length::Fill)
                .align_x(Alignment::Center)
                .align_y(Alignment::Center),
        )
        .width(40)
        .height(40)
        .padding(0)
        .style(move |_, status| icon_pill_btn(p, false, status))
        .on_press(Message::DeletePlaylist(pl_id_del)),
    ]
    .spacing(12)
    .align_y(Vertical::Center);

    let header_card = container(
        row![
            cover,
            column![
                text(t(app.lang, "playlist").to_uppercase())
                    .size(11)
                    .color(p.on_surface_variant),
                text(&pl.name).size(32).color(p.on_surface),
                text(desc_text).size(13).color(p.on_surface_variant),
                text(format!("{} {}", pl.tracks.len(), t(app.lang, "tracks")))
                    .size(13)
                    .color(p.on_surface_variant),
                actions,
            ]
            .spacing(8)
        ]
        .spacing(24)
        .align_y(Vertical::Bottom),
    )
    .width(Length::Fill)
    .padding(Padding {
        top: 24.0,
        right: 24.0,
        bottom: 20.0,
        left: 24.0,
    })
    .style(move |_| surface_container_style(p.surface_container, CORNER_XL));

    let list_elem = if filtered.is_empty() {
        view_empty_state(
            p,
            ICON_MUSIC,
            t(app.lang, "empty_playlist_title"),
            t(app.lang, "empty_playlist_sub"),
        )
    } else {
        view_track_list(
            app,
            p,
            &filtered,
            TrackContext::Playlist,
            Some(&pl.id),
        )
    };

    column![back_btn, header_card, list_elem]
        .spacing(16)
        .into()
}

fn view_settings<'a>(app: &'a RaserApp, p: M3Palette) -> Element<'a, Message> {
    let tab_btn = |tab: SettingsTab, label_key: &'static str| {
        let active = app.settings_tab == tab;
        button(
            container(
                text(t(app.lang, label_key))
                    .size(14)
                    .color(if active {
                        p.on_primary
                    } else {
                        p.on_surface_variant
                    }),
            )
            .width(Length::Fill)
            .align_y(Vertical::Center),
        )
        .width(Length::Fill)
        .height(48)
        .padding([0, 16])
        .style(move |_, status| {
            let bg = if active {
                p.primary
            } else if matches!(status, button::Status::Hovered) {
                with_alpha(p.on_surface, 0.08)
            } else {
                Color::TRANSPARENT
            };
            button::Style {
                background: Some(Background::Color(bg)),
                text_color: if active {
                    p.on_primary
                } else {
                    p.on_surface_variant
                },
                border: Border {
                    radius: 9999.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            }
        })
        .on_press(Message::SelectSettingsTab(tab))
    };

    let nav = container(
        column![
            tab_btn(SettingsTab::Interface, "interface"),
            tab_btn(SettingsTab::Playback, "playback"),
            tab_btn(SettingsTab::Storage, "storage"),
            tab_btn(SettingsTab::About, "about"),
        ]
        .spacing(4),
    )
    .width(Length::Fixed(204.0))
    .padding(8)
    .style(move |_| surface_container_style(p.surface_container_low, 24.0));

    let setting_card = |content: Element<'a, Message>| {
        container(content)
            .width(Length::Fill)
            .padding([14, 18])
            .style(move |_| surface_container_style(p.surface_container, CORNER_L))
    };

    let pane: Element<'a, Message> = match app.settings_tab {
        SettingsTab::Interface => {
            let lang_card = setting_card(
                row![
                    text(t(app.lang, "interface_language"))
                        .size(14)
                        .color(p.on_surface),
                    container(text("").size(1)).width(Length::Fill),
                    button(text("English").size(13))
                        .height(36)
                        .padding([0, 16])
                        .style(move |_, status| {
                            outline_pill_btn(p, app.lang == crate::i18n::Lang::En, status)
                        })
                        .on_press(Message::SetLang(crate::i18n::Lang::En)),
                    button(text("Türkçe").size(13))
                        .height(36)
                        .padding([0, 16])
                        .style(move |_, status| {
                            outline_pill_btn(p, app.lang == crate::i18n::Lang::Tr, status)
                        })
                        .on_press(Message::SetLang(crate::i18n::Lang::Tr)),
                ]
                .spacing(8)
                .align_y(Vertical::Center)
                .into(),
            );

            let is_light = app.snapshot.settings.theme_mode.eq_ignore_ascii_case("light");
            let theme_card = setting_card(
                row![
                    text(t(app.lang, "theme_mode")).size(14).color(p.on_surface),
                    container(text("").size(1)).width(Length::Fill),
                    button(text(t(app.lang, "dark")).size(13))
                        .height(36)
                        .padding([0, 16])
                        .style(move |_, status| outline_pill_btn(p, !is_light, status))
                        .on_press(Message::SetThemeMode("dark".to_string())),
                    button(text(t(app.lang, "light")).size(13))
                        .height(36)
                        .padding([0, 16])
                        .style(move |_, status| outline_pill_btn(p, is_light, status))
                        .on_press(Message::SetThemeMode("light".to_string())),
                ]
                .spacing(8)
                .align_y(Vertical::Center)
                .into(),
            );

            let hide_tb_card = setting_card(
                row![
                    column![
                        text(t(app.lang, "hide_titlebar")).size(14).color(p.on_surface),
                        text(t(app.lang, "hide_titlebar_desc"))
                            .size(12)
                            .color(p.outline),
                    ]
                    .spacing(3)
                    .width(Length::Fill),
                    m3_switch(
                        p,
                        app.snapshot.settings.advanced.hide_titlebar,
                        Message::ToggleHideTitlebar,
                    ),
                ]
                .spacing(12)
                .align_y(Vertical::Center)
                .into(),
            );

            let autohide_ts_card = setting_card(
                row![
                    column![
                        text(t(app.lang, "autohide_timestamps"))
                            .size(14)
                            .color(p.on_surface),
                        text(t(app.lang, "autohide_timestamps_desc"))
                            .size(12)
                            .color(p.outline),
                    ]
                    .spacing(3)
                    .width(Length::Fill),
                    m3_switch(
                        p,
                        app.snapshot.settings.advanced.autohide_timestamps,
                        Message::ToggleAutohideTimestamps,
                    ),
                ]
                .spacing(12)
                .align_y(Vertical::Center)
                .into(),
            );

            let classic_sb_card = setting_card(
                row![
                    column![
                        text(t(app.lang, "classic_sidebar"))
                            .size(14)
                            .color(p.on_surface),
                        text(t(app.lang, "classic_sidebar_desc"))
                            .size(12)
                            .color(p.outline),
                    ]
                    .spacing(3)
                    .width(Length::Fill),
                    m3_switch(
                        p,
                        app.snapshot.settings.advanced.classic_sidebar,
                        Message::ToggleClassicSidebar,
                    ),
                ]
                .spacing(12)
                .align_y(Vertical::Center)
                .into(),
            );

            let mut order_rows = column![].spacing(6);
            let order_len = app.snapshot.settings.sidebar_order.len();
            for (idx, item) in app.snapshot.settings.sidebar_order.iter().enumerate() {
                let label_key = match item.as_str() {
                    "discover" => "home",
                    "local" => "local_files",
                    other => other,
                };
                order_rows = order_rows.push(
                    row![
                        text(t(app.lang, label_key)).size(13).color(p.on_surface),
                        container(text("").size(1)).width(Length::Fill),
                        button(text(t(app.lang, "up")).size(11))
                            .height(28)
                            .padding([0, 10])
                            .style(move |_, status| outline_pill_btn(p, false, status))
                            .on_press(Message::MoveSidebarItem(idx, true)),
                        button(text(t(app.lang, "down")).size(11))
                            .height(28)
                            .padding([0, 10])
                            .style(move |_, status| outline_pill_btn(p, false, status))
                            .on_press(Message::MoveSidebarItem(
                                idx.min(order_len.saturating_sub(1)),
                                false,
                            )),
                    ]
                    .spacing(6)
                    .align_y(Vertical::Center),
                );
            }
            let sidebar_order_card = setting_card(
                column![
                    row![
                        text(t(app.lang, "sidebar_order"))
                            .size(14)
                            .color(p.on_surface),
                        container(text("").size(1)).width(Length::Fill),
                        button(text(t(app.lang, "reset")).size(12))
                            .height(30)
                            .padding([0, 12])
                            .style(move |_, status| outline_pill_btn(p, false, status))
                            .on_press(Message::ResetSidebarOrder),
                    ]
                    .align_y(Vertical::Center),
                    order_rows,
                ]
                .spacing(10)
                .into(),
            );

            column![
                lang_card,
                theme_card,
                hide_tb_card,
                autohide_ts_card,
                classic_sb_card,
                sidebar_order_card,
            ]
            .spacing(10)
            .into()
        }
        SettingsTab::Playback => {
            let autoplay_card = setting_card(
                row![
                    column![
                        text(t(app.lang, "autoplay")).size(14).color(p.on_surface),
                        text(t(app.lang, "autoplay_desc"))
                            .size(12)
                            .color(p.outline),
                    ]
                    .spacing(3)
                    .width(Length::Fill),
                    m3_switch(
                        p,
                        app.snapshot.settings.advanced.autoplay,
                        Message::ToggleAutoplay,
                    ),
                ]
                .spacing(12)
                .align_y(Vertical::Center)
                .into(),
            );

            let speeds = [0.5f32, 0.75, 1.0, 1.25, 1.5, 2.0];
            let mut spd_row = row![
                text(t(app.lang, "speed")).size(14).color(p.on_surface),
                container(text("").size(1)).width(Length::Fill),
            ]
            .spacing(6)
            .align_y(Vertical::Center);
            for &sp in &speeds {
                let active = (app.snapshot.settings.speed - sp).abs() < 0.01;
                spd_row = spd_row.push(
                    button(text(format!("{}x", sp)).size(12))
                        .height(32)
                        .padding([0, 12])
                        .style(move |_, status| outline_pill_btn(p, active, status))
                        .on_press(Message::SetSpeed(sp)),
                );
            }
            let speed_card = setting_card(spd_row.into());

            let vol = app.snapshot.settings.volume.clamp(0.0, 1.0);
            let vol_card = setting_card(
                column![
                    row![
                        text("Volume").size(14).color(p.on_surface),
                        container(text("").size(1)).width(Length::Fill),
                        text(format!("{}%", (vol * 100.0).round() as u32))
                            .size(13)
                            .color(p.primary),
                    ],
                    slider(0.0..=1.0, vol, Message::SetVolume)
                        .step(0.01_f32)
                        .style(move |_, status| m3_slider_style(p, status)),
                ]
                .spacing(8)
                .into(),
            );

            column![autoplay_card, speed_card, vol_card]
                .spacing(10)
                .into()
        }
        SettingsTab::Storage => {
            let (bytes, count) = app.cache_stats;
            let cache_dir = crate::storage::get_cache_dir().to_string_lossy().to_string();
            let cache_card = setting_card(
                column![
                    text(t(app.lang, "cache_storage")).size(15).color(p.on_surface),
                    text(t(app.lang, "cache_hint")).size(12).color(p.outline),
                    text(format!("{} ({} files)", format_bytes(bytes), count))
                        .size(14)
                        .color(p.primary),
                    text(cache_dir).size(11).color(p.on_surface_variant),
                    row![
                        button(text(t(app.lang, "clear_cache")).size(13))
                            .height(38)
                            .padding([0, 18])
                            .style(move |_, status| pill_primary_btn(p, status))
                            .on_press(Message::ClearCache),
                        button(text(t(app.lang, "open_cache_folder")).size(13))
                            .height(38)
                            .padding([0, 18])
                            .style(move |_, status| outline_pill_btn(p, false, status))
                            .on_press(Message::OpenCacheFolder),
                    ]
                    .spacing(10),
                ]
                .spacing(10)
                .into(),
            );
            column![cache_card].spacing(10).into()
        }
        SettingsTab::About => {
            let about_card = setting_card(
                column![
                    text("Raser").size(20).color(p.on_surface),
                    text(format!(
                        "{}: {}",
                        t(app.lang, "version_label"),
                        crate::updater::CURRENT_VERSION
                    ))
                    .size(13)
                    .color(p.primary),
                    text(format!(
                        "{}: iced 0.14 + rodio 0.23 (Pure Rust)",
                        t(app.lang, "engine_label")
                    ))
                    .size(13)
                    .color(p.on_surface_variant),
                ]
                .spacing(6)
                .into(),
            );
            let shortcuts_card = setting_card(
                column![
                    text(t(app.lang, "keyboard_shortcuts"))
                        .size(14)
                        .color(p.on_surface),
                    text("Space: Play / Pause  |  Ctrl+Left/Right: Prev / Next")
                        .size(12)
                        .color(p.on_surface_variant),
                    text("Shift+Left/Right: -5s / +5s  |  Ctrl+Up/Down: Volume")
                        .size(12)
                        .color(p.on_surface_variant),
                    text("M: Mute  |  H: Shuffle  |  R: Repeat  |  P: Mini Player")
                        .size(12)
                        .color(p.on_surface_variant),
                    text("Ctrl+K: Search  |  F11: Fullscreen  |  Esc: Close / Back")
                        .size(12)
                        .color(p.on_surface_variant),
                ]
                .spacing(6)
                .into(),
            );
            column![about_card, shortcuts_card].spacing(10).into()
        }
    };

    column![
        text(t(app.lang, "settings")).size(22).color(p.on_surface),
        row![nav, container(pane).width(Length::Fill)]
            .spacing(20)
            .align_y(Vertical::Top),
    ]
    .spacing(18)
    .into()
}

pub fn view_right_panel<'a>(app: &'a RaserApp, p: M3Palette) -> Element<'a, Message> {
    let header = row![
        text(t(app.lang, "now_playing")).size(14).color(p.on_surface),
        container(text("").size(1)).width(Length::Fill),
        button(
            container(icon_svg(ICON_CLOSE, p.on_surface_variant, 15.0))
                .width(Length::Fill)
                .height(Length::Fill)
                .align_x(Alignment::Center)
                .align_y(Alignment::Center),
        )
        .width(32)
        .height(32)
        .padding(0)
        .style(move |_, status| transparent_icon_btn(p, false, status))
        .on_press(Message::ToggleQueuePanel),
    ]
    .align_y(Vertical::Center);

    let tabs = row![
        button(text(t(app.lang, "queue")).size(12))
            .height(32)
            .padding([0, 14])
            .style(move |_, status| {
                outline_pill_btn(p, app.queue_panel_tab == QueuePanelTab::Queue, status)
            })
            .on_press(Message::SelectQueuePanelTab(QueuePanelTab::Queue)),
        button(text(t(app.lang, "recent")).size(12))
            .height(32)
            .padding([0, 14])
            .style(move |_, status| {
                outline_pill_btn(p, app.queue_panel_tab == QueuePanelTab::Recent, status)
            })
            .on_press(Message::SelectQueuePanelTab(QueuePanelTab::Recent)),
    ]
    .spacing(8);

    let items: Vec<(usize, TrackRecord)> = match app.queue_panel_tab {
        QueuePanelTab::Queue => app.queue.iter().cloned().enumerate().take(50).collect(),
        QueuePanelTab::Recent => app
            .snapshot
            .history
            .iter()
            .cloned()
            .enumerate()
            .take(50)
            .collect(),
    };

    let mut list_col = column![].spacing(4);
    for (idx, tr) in items {
        let is_cur = app.queue_panel_tab == QueuePanelTab::Queue
            && app.queue_index == Some(idx);
        let title = app.display_title(&tr);
        let artist = app.display_artist(&tr);
        let msg = match app.queue_panel_tab {
            QueuePanelTab::Queue => Message::JumpToQueueIndex(idx),
            QueuePanelTab::Recent => Message::PlayTracks(vec![tr.clone()], 0),
        };
        list_col = list_col.push(
            button(
                column![
                    text(title).size(13).color(if is_cur {
                        p.primary
                    } else {
                        p.on_surface
                    }),
                    text(artist).size(11).color(p.on_surface_variant),
                ]
                .spacing(2),
            )
            .width(Length::Fill)
            .padding([8, 12])
            .style(move |_, status| track_row_btn(p, is_cur, status))
            .on_press(msg),
        );
    }

    container(
        column![
            header,
            tabs,
            scrollable(list_col).width(Length::Fill).height(Length::Fill),
        ]
        .spacing(14),
    )
    .width(Length::Fixed(RIGHT_PANEL_WIDTH))
    .height(Length::Fill)
    .padding(20)
    .style(move |_| surface_container_style(p.surface_container_low, CORNER_XL))
    .into()
}

pub fn view_modal<'a>(
    app: &'a RaserApp,
    p: M3Palette,
    modal: &'a ModalState,
) -> Element<'a, Message> {
    let card_content: Element<'a, Message> = match modal {
        ModalState::CreatePlaylist { name, description } => column![
            text(t(app.lang, "create_playlist"))
                .size(18)
                .color(p.on_surface),
            text_input(t(app.lang, "playlist_name"), name)
                .on_input(Message::ModalPlaylistNameChanged)
                .padding([12, 16])
                .style(move |_, status| modal_input_style(p, status)),
            text_input(t(app.lang, "description"), description)
                .on_input(Message::ModalPlaylistDescChanged)
                .padding([12, 16])
                .style(move |_, status| modal_input_style(p, status)),
            row![
                container(text("").size(1)).width(Length::Fill),
                button(text(t(app.lang, "cancel")).size(13))
                    .height(38)
                    .padding([0, 18])
                    .style(move |_, status| outline_pill_btn(p, false, status))
                    .on_press(Message::CloseModal),
                button(text(t(app.lang, "create")).size(13))
                    .height(38)
                    .padding([0, 20])
                    .style(move |_, status| pill_primary_btn(p, status))
                    .on_press(Message::SubmitCreatePlaylist),
            ]
            .spacing(10),
        ]
        .spacing(14)
        .into(),

        ModalState::RenamePlaylist {
            playlist_id: _,
            name,
            description,
        } => column![
            text(t(app.lang, "rename_playlist"))
                .size(18)
                .color(p.on_surface),
            text_input(t(app.lang, "playlist_name"), name)
                .on_input(Message::ModalPlaylistNameChanged)
                .padding([12, 16])
                .style(move |_, status| modal_input_style(p, status)),
            text_input(t(app.lang, "description"), description)
                .on_input(Message::ModalPlaylistDescChanged)
                .padding([12, 16])
                .style(move |_, status| modal_input_style(p, status)),
            row![
                container(text("").size(1)).width(Length::Fill),
                button(text(t(app.lang, "cancel")).size(13))
                    .height(38)
                    .padding([0, 18])
                    .style(move |_, status| outline_pill_btn(p, false, status))
                    .on_press(Message::CloseModal),
                button(text(t(app.lang, "save")).size(13))
                    .height(38)
                    .padding([0, 20])
                    .style(move |_, status| pill_primary_btn(p, status))
                    .on_press(Message::SubmitRenamePlaylist),
            ]
            .spacing(10),
        ]
        .spacing(14)
        .into(),

        ModalState::RenameTrack {
            track: _,
            title,
            artist,
        } => column![
            text(t(app.lang, "rename_track"))
                .size(18)
                .color(p.on_surface),
            text_input(t(app.lang, "track_title"), title)
                .on_input(Message::ModalTrackTitleChanged)
                .padding([12, 16])
                .style(move |_, status| modal_input_style(p, status)),
            text_input(t(app.lang, "track_artist"), artist)
                .on_input(Message::ModalTrackArtistChanged)
                .padding([12, 16])
                .style(move |_, status| modal_input_style(p, status)),
            row![
                container(text("").size(1)).width(Length::Fill),
                button(text(t(app.lang, "cancel")).size(13))
                    .height(38)
                    .padding([0, 18])
                    .style(move |_, status| outline_pill_btn(p, false, status))
                    .on_press(Message::CloseModal),
                button(text(t(app.lang, "save")).size(13))
                    .height(38)
                    .padding([0, 20])
                    .style(move |_, status| pill_primary_btn(p, status))
                    .on_press(Message::SubmitRenameTrack),
            ]
            .spacing(10),
        ]
        .spacing(14)
        .into(),

        ModalState::TrimTrack {
            track,
            start_input,
            end_input,
        } => column![
            text(t(app.lang, "trim_best_part"))
                .size(18)
                .color(p.on_surface),
            text(format!("{} — {}", app.display_title(track), app.display_artist(track)))
                .size(13)
                .color(p.primary),
            text(t(app.lang, "trim_desc"))
                .size(12)
                .color(p.on_surface_variant),
            row![
                column![
                    text(t(app.lang, "trim_start"))
                        .size(11)
                        .color(p.on_surface_variant),
                    text_input("0.0", start_input)
                        .on_input(Message::ModalTrimStartChanged)
                        .padding([10, 14])
                        .style(move |_, status| modal_input_style(p, status)),
                ]
                .spacing(4)
                .width(Length::Fill),
                column![
                    text(t(app.lang, "trim_end"))
                        .size(11)
                        .color(p.on_surface_variant),
                    text_input("0.0", end_input)
                        .on_input(Message::ModalTrimEndChanged)
                        .padding([10, 14])
                        .style(move |_, status| modal_input_style(p, status)),
                ]
                .spacing(4)
                .width(Length::Fill),
            ]
            .spacing(12),
            row![
                button(text(t(app.lang, "reset_trim")).size(13))
                    .height(38)
                    .padding([0, 16])
                    .style(move |_, status| outline_pill_btn(p, false, status))
                    .on_press(Message::SubmitResetTrim),
                container(text("").size(1)).width(Length::Fill),
                button(text(t(app.lang, "cancel")).size(13))
                    .height(38)
                    .padding([0, 18])
                    .style(move |_, status| outline_pill_btn(p, false, status))
                    .on_press(Message::CloseModal),
                button(text(t(app.lang, "save_trim")).size(13))
                    .height(38)
                    .padding([0, 20])
                    .style(move |_, status| pill_primary_btn(p, status))
                    .on_press(Message::SubmitSaveTrim),
            ]
            .spacing(8),
        ]
        .spacing(12)
        .into(),

        ModalState::AddToPlaylist { track } => {
            let mut pl_col = column![].spacing(6);
            for pl in &app.snapshot.playlists {
                let pid = pl.id.clone();
                let tr_clone = track.clone();
                pl_col = pl_col.push(
                    button(
                        row![
                            text(&pl.name).size(14).color(p.on_surface),
                            container(text("").size(1)).width(Length::Fill),
                            text(format!("{} {}", pl.tracks.len(), t(app.lang, "tracks")))
                                .size(12)
                                .color(p.outline),
                        ]
                        .align_y(Vertical::Center),
                    )
                    .width(Length::Fill)
                    .padding([12, 16])
                    .style(move |_, status| track_row_btn(p, false, status))
                    .on_press(Message::AddTrackToPlaylist(pid, tr_clone)),
                );
            }
            column![
                text(t(app.lang, "add_to_playlist"))
                    .size(18)
                    .color(p.on_surface),
                scrollable(pl_col).height(260.0),
                row![
                    button(text(t(app.lang, "new_playlist")).size(13))
                        .height(38)
                        .padding([0, 16])
                        .style(move |_, status| outline_pill_btn(p, false, status))
                        .on_press(Message::OpenCreatePlaylistModal),
                    container(text("").size(1)).width(Length::Fill),
                    button(text(t(app.lang, "cancel")).size(13))
                        .height(38)
                        .padding([0, 18])
                        .style(move |_, status| outline_pill_btn(p, false, status))
                        .on_press(Message::CloseModal),
                ]
                .spacing(8),
            ]
            .spacing(14)
            .into()
        }
    };

    let card = mouse_area(
        container(card_content)
            .width(Length::Fixed(440.0))
            .padding(24)
            .style(move |_| modal_card_style(p)),
    )
    .on_press(Message::Noop);

    mouse_area(
        container(card)
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(Alignment::Center)
            .align_y(Alignment::Center)
            .style(|_| container::Style {
                background: Some(Background::Color(Color::from_rgba(0.0, 0.0, 0.0, 0.48))),
                ..Default::default()
            }),
    )
    .on_press(Message::CloseModal)
    .into()
}
