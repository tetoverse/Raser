use crate::app::{Message, RaserApp};
use crate::i18n::t;
use crate::ui::{
    icon_svg, raser_logo_svg, titlebar_close_btn, transparent_icon_btn, M3Palette, ICON_CLOSE,
    ICON_FULLSCREEN, ICON_MAXIMIZE, ICON_MINIMIZE, ICON_MINI_PLAYER, TITLEBAR_HEIGHT,
};
use iced::alignment::Vertical;
use iced::widget::{button, container, mouse_area, row, stack, text, tooltip};
use iced::{Alignment, Element, Length};

pub fn view_titlebar<'a>(app: &'a RaserApp, p: M3Palette) -> Element<'a, Message> {
    if app.snapshot.settings.advanced.hide_titlebar && !app.titlebar_revealed {
        return mouse_area(
            container(text("").size(1))
                .width(Length::Fill)
                .height(Length::Fixed(6.0)),
        )
        .on_enter(Message::TitlebarHoverEnter)
        .into();
    }

    let drag_region = mouse_area(
        container(
            row![
                raser_logo_svg(p.primary, 18.0),
                text("Raser")
                    .size(12)
                    .color(p.on_surface_variant),
            ]
            .spacing(8)
            .align_y(Vertical::Center),
        )
        .width(Length::Fill)
        .height(Length::Fixed(TITLEBAR_HEIGHT))
        .align_x(Alignment::Center)
        .align_y(Alignment::Center),
    )
    .on_press(Message::DragWindow);

    let ctrl_btn = |icon: &'static str, msg: Message, tip_key: &'static str| {
        tooltip(
            button(
                container(icon_svg(icon, p.on_surface_variant, 14.0))
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .align_x(Alignment::Center)
                    .align_y(Alignment::Center),
            )
            .width(30)
            .height(30)
            .padding(0)
            .style(move |_, status| transparent_icon_btn(p, false, status))
            .on_press(msg),
            text(t(app.lang, tip_key)).size(11).color(p.on_surface),
            tooltip::Position::Bottom,
        )
        .style(move |_| crate::ui::surface_container_style(p.surface_container_high, 8.0))
    };

    let close_btn = tooltip(
        button(
            container(icon_svg(ICON_CLOSE, p.on_surface_variant, 14.0))
                .width(Length::Fill)
                .height(Length::Fill)
                .align_x(Alignment::Center)
                .align_y(Alignment::Center),
        )
        .width(30)
        .height(30)
        .padding(0)
        .style(move |_, status| titlebar_close_btn(p, status))
        .on_press(Message::CloseWindow),
        text(t(app.lang, "close")).size(11).color(p.on_surface),
        tooltip::Position::Bottom,
    )
    .style(move |_| crate::ui::surface_container_style(p.surface_container_high, 8.0));

    let controls = row![
        ctrl_btn(ICON_MINI_PLAYER, Message::ToggleMiniPlayer, "mini_player"),
        ctrl_btn(ICON_FULLSCREEN, Message::ToggleFullscreen, "fullscreen"),
        ctrl_btn(ICON_MINIMIZE, Message::MinimizeWindow, "minimize"),
        ctrl_btn(ICON_MAXIMIZE, Message::ToggleMaximizeWindow, "maximize"),
        close_btn,
    ]
    .spacing(6)
    .align_y(Vertical::Center);

    let bar = container(
        stack![
            drag_region,
            row![
                container(text("").size(1)).width(Length::Fill),
                controls,
            ]
            .height(Length::Fixed(TITLEBAR_HEIGHT))
            .align_y(Vertical::Center)
            .padding([0, 16]),
        ]
        .width(Length::Fill)
        .height(Length::Fixed(TITLEBAR_HEIGHT)),
    )
    .width(Length::Fill)
    .height(Length::Fixed(TITLEBAR_HEIGHT))
    .style(move |_| {
        if app.snapshot.settings.advanced.hide_titlebar && app.titlebar_revealed {
            crate::ui::surface_card_bordered(p.surface_container_low, p.outline_variant, 0.0)
        } else {
            crate::ui::surface_container_style(iced::Color::TRANSPARENT, 0.0)
        }
    });

    if app.snapshot.settings.advanced.hide_titlebar {
        mouse_area(bar)
            .on_exit(Message::TitlebarHoverExit)
            .into()
    } else {
        bar.into()
    }
}
