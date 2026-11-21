use crate::app::{ActiveView, Message, RaserApp};
use crate::i18n::t;
use crate::ui::{
    icon_pill_btn, icon_svg, nav_pill_btn, raser_logo_svg, surface_card_bordered,
    surface_container_style, with_alpha, M3Palette, CORNER_M, CORNER_XL, ICON_ALBUMS, ICON_FOLDER,
    ICON_HEART, ICON_HISTORY, ICON_MUSIC, ICON_PLUS, ICON_QUEUE, ICON_SETTINGS,
    SIDEBAR_CLASSIC_WIDTH, SIDEBAR_CONNECTED_WIDTH,
};
use iced::widget::{button, column, container, scrollable, text, tooltip};
use iced::{Alignment, Border, Color, Element, Length, Padding};

fn nav_icon_for_key(key: &str) -> &'static str {
    match key {
        "favorites" => ICON_HEART,
        "albums" => ICON_ALBUMS,
        "local" => ICON_FOLDER,
        "history" => ICON_HISTORY,
        "queue" => ICON_QUEUE,
        _ => ICON_SETTINGS,
    }
}

fn nav_view_for_key(key: &str) -> ActiveView {
    match key {
        "discover" => ActiveView::Discover,
        "favorites" => ActiveView::Favorites,
        "albums" => ActiveView::Albums,
        "local" => ActiveView::Local,
        "history" => ActiveView::History,
        "queue" => ActiveView::Queue,
        _ => ActiveView::Settings,
    }
}

fn nav_label_key(key: &str) -> &'static str {
    match key {
        "discover" => "home",
        "favorites" => "favorites",
        "albums" => "albums",
        "local" => "local_files",
        "history" => "history",
        "queue" => "queue",
        _ => "settings",
    }
}

fn nav_count_for_key(app: &RaserApp, key: &str) -> Option<usize> {
    let n = match key {
        "favorites" => app.snapshot.favorites.len(),
        "albums" => app.snapshot.saved_albums.len(),
        "local" => app.snapshot.local_tracks.len(),
        "queue" => app.queue.len(),
        _ => 0,
    };
    if n > 0 { Some(n) } else { None }
}

pub fn view_sidebar<'a>(app: &'a RaserApp, p: M3Palette) -> Element<'a, Message> {
    let classic = app.snapshot.settings.advanced.classic_sidebar;
    let width = if classic {
        SIDEBAR_CLASSIC_WIDTH
    } else {
        SIDEBAR_CONNECTED_WIDTH
    };

    let mut nav_col = column![].spacing(12).align_x(Alignment::Center);

    for item_key in &app.snapshot.settings.sidebar_order {
        let target_view = nav_view_for_key(item_key);
        let is_active = app.active_view == target_view;
        let label = t(app.lang, nav_label_key(item_key));
        let count = nav_count_for_key(app, item_key);

        let btn_elem: Element<'a, Message> = if item_key == "discover" {
            let icon_color = if is_active { p.primary } else { p.on_surface_variant };
            let content = container(raser_logo_svg(icon_color, 44.0))
                .width(Length::Fill)
                .height(Length::Fill)
                .align_x(Alignment::Center)
                .align_y(Alignment::Center);

            let b = button(content)
                .width(Length::Fixed(56.0))
                .height(Length::Fixed(56.0))
                .padding(0)
                .style(move |_, status| {
                    let bg = if matches!(status, button::Status::Hovered) {
                        with_alpha(p.on_surface, 0.06)
                    } else {
                        Color::TRANSPARENT
                    };
                    button::Style {
                        background: Some(iced::Background::Color(bg)),
                        text_color: p.on_surface,
                        border: Border {
                            radius: 14.0.into(),
                            ..Default::default()
                        },
                        ..Default::default()
                    }
                })
                .on_press(Message::SwitchView(ActiveView::Discover));

            tooltip(
                b,
                text(label).size(11).color(p.on_surface),
                tooltip::Position::Right,
            )
            .style(move |_| surface_container_style(p.surface_container_high, 8.0))
            .into()
        } else {
            let icon_fg = if is_active { p.on_primary } else { p.on_surface_variant };
            let icon = icon_svg(nav_icon_for_key(item_key), icon_fg, 22.0);
            let content = container(icon)
                .width(Length::Fill)
                .height(Length::Fill)
                .align_x(Alignment::Center)
                .align_y(Alignment::Center);

            let b = button(content)
                .width(Length::Fixed(48.0))
                .height(Length::Fixed(48.0))
                .padding(0)
                .style(move |_, status| nav_pill_btn(p, is_active, status))
                .on_press(Message::SwitchView(target_view));

            let tip_text = match count {
                Some(c) => format!("{} ({})", label, c),
                None => label.to_string(),
            };
            tooltip(
                b,
                text(tip_text).size(11).color(p.on_surface),
                tooltip::Position::Right,
            )
            .style(move |_| surface_container_style(p.surface_container_high, 8.0))
            .into()
        };

        nav_col = nav_col.push(btn_elem);
    }

    // Divider (.rail-divider)
    nav_col = nav_col.push(
        container(text("").size(1))
            .width(Length::Fixed(32.0))
            .height(Length::Fixed(1.0))
            .style(move |_| surface_container_style(p.surface_container_high, 0.0)),
    );

    // Create playlist button (.rail-icon-btn: 48x48 circle in default connected mode, 44x44 r=12 in classic)
    let create_pl_btn: Element<'a, Message> = {
        let icon = icon_svg(ICON_PLUS, p.on_surface_variant, 22.0);
        let content = container(icon)
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(Alignment::Center)
            .align_y(Alignment::Center);
        let btn_size = if classic { 44.0 } else { 48.0 };
        let btn_radius = if classic { CORNER_M } else { 9999.0 };
        let b = button(content)
            .width(Length::Fixed(btn_size))
            .height(Length::Fixed(btn_size))
            .padding(0)
            .style(move |_, status| {
                let bg = match status {
                    button::Status::Hovered => p.surface_container_highest,
                    _ => p.surface_container_high,
                };
                button::Style {
                    background: Some(iced::Background::Color(bg)),
                    text_color: p.on_surface,
                    border: Border {
                        radius: btn_radius.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                }
            })
            .on_press(Message::OpenCreatePlaylistModal);

        tooltip(
            b,
            text(t(app.lang, "create_playlist"))
                .size(11)
                .color(p.on_surface),
            tooltip::Position::Right,
        )
        .style(move |_| surface_container_style(p.surface_container_high, 8.0))
        .into()
    };
    nav_col = nav_col.push(create_pl_btn);

    // Scrollable playlists squares (.rail-playlists-scroll, 44x44 r=12px)
    let mut pl_col = column![].spacing(10).align_x(Alignment::Center);
    for pl in &app.snapshot.playlists {
        let is_active = matches!(&app.active_view, ActiveView::PlaylistDetail(id) if id == &pl.id);
        let pl_id = pl.id.clone();
        let initial = pl
            .name
            .chars()
            .next()
            .map(|c| c.to_uppercase().to_string())
            .unwrap_or_else(|| "P".to_string());

        let content = container(
            text(initial)
                .size(15)
                .color(if is_active { p.primary } else { p.on_surface }),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(Alignment::Center)
        .align_y(Alignment::Center);

        let b = button(content)
            .width(Length::Fixed(44.0))
            .height(Length::Fixed(44.0))
            .padding(0)
            .style(move |_, status| {
                let bg = match status {
                    button::Status::Hovered => p.surface_container_highest,
                    _ => p.surface_container_high,
                };
                button::Style {
                    background: Some(iced::Background::Color(bg)),
                    text_color: p.on_surface,
                    border: Border {
                        color: if is_active {
                            p.primary
                        } else {
                            with_alpha(p.outline_variant, 0.35)
                        },
                        width: if is_active { 2.0 } else { 1.0 },
                        radius: CORNER_M.into(),
                    },
                    ..Default::default()
                }
            })
            .on_press(Message::SwitchView(ActiveView::PlaylistDetail(pl_id)));

        let tip_label = format!(
            "{} ({} {})",
            pl.name,
            pl.tracks.len(),
            t(app.lang, "tracks")
        );
        pl_col = pl_col.push(
            tooltip(
                b,
                text(tip_label).size(11).color(p.on_surface),
                tooltip::Position::Right,
            )
            .style(move |_| surface_container_style(p.surface_container_high, 8.0)),
        );
    }

    let playlists_scroll = scrollable(pl_col)
        .width(Length::Fill)
        .height(Length::Fill);

    // Bottom pinned Settings button (48x48 circle)
    let settings_active = app.active_view == ActiveView::Settings;
    let settings_btn: Element<'a, Message> = {
        let fg = if settings_active {
            p.on_primary
        } else {
            p.on_surface_variant
        };
        let icon = icon_svg(ICON_SETTINGS, fg, 22.0);
        let content = container(icon)
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(Alignment::Center)
            .align_y(Alignment::Center);

        let b = button(content)
            .width(Length::Fixed(48.0))
            .height(Length::Fixed(48.0))
            .padding(0)
            .style(move |_, status| nav_pill_btn(p, settings_active, status))
            .on_press(Message::SwitchView(ActiveView::Settings));

        tooltip(
            b,
            text(t(app.lang, "settings")).size(11).color(p.on_surface),
            tooltip::Position::Right,
        )
        .style(move |_| surface_container_style(p.surface_container_high, 8.0))
        .into()
    };

    let _ = (icon_pill_btn, surface_card_bordered, ICON_MUSIC);

    container(
        column![nav_col, playlists_scroll, settings_btn]
            .spacing(12)
            .align_x(Alignment::Center)
            .padding(if classic {
                Padding::from([16, 0])
            } else {
                Padding {
                    top: 18.0,
                    right: 0.0,
                    bottom: 16.0,
                    left: 0.0,
                }
            }),
    )
    .width(Length::Fixed(width))
    .height(Length::Fill)
    .style(move |_| {
        surface_container_style(
            p.panel_bg,
            if classic { CORNER_XL } else { 0.0 },
        )
    })
    .into()
}
