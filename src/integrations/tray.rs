use crate::taskbar::MediaCommand;
use std::sync::mpsc::Sender;
use tray_icon::menu::{Menu, MenuEvent, MenuId, MenuItem};
use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};

pub struct TrayController {
    _tray: TrayIcon,
    id_show: MenuId,
    id_toggle: MenuId,
    id_next: MenuId,
    id_prev: MenuId,
    id_quit: MenuId,
}

impl TrayController {
    pub fn try_new(is_tr: bool) -> Option<Self> {
        let png_bytes = include_bytes!("../../assets/icon.png");
        let img = image::load_from_memory(png_bytes).ok()?.to_rgba8();
        let (w, h) = img.dimensions();
        let icon = Icon::from_rgba(img.into_raw(), w, h).ok()?;

        let show_label = if is_tr { "Raser'ı Göster" } else { "Show Raser" };
        let toggle_label = if is_tr { "Oynat / Duraklat" } else { "Play / Pause" };
        let next_label = if is_tr { "Sonraki" } else { "Next" };
        let prev_label = if is_tr { "Önceki" } else { "Previous" };
        let quit_label = if is_tr { "Çıkış" } else { "Quit" };

        let item_show = MenuItem::new(show_label, true, None);
        let item_toggle = MenuItem::new(toggle_label, true, None);
        let item_next = MenuItem::new(next_label, true, None);
        let item_prev = MenuItem::new(prev_label, true, None);
        let item_quit = MenuItem::new(quit_label, true, None);

        let id_show = item_show.id().clone();
        let id_toggle = item_toggle.id().clone();
        let id_next = item_next.id().clone();
        let id_prev = item_prev.id().clone();
        let id_quit = item_quit.id().clone();

        let menu = Menu::new();
        let _ = menu.append(&item_show);
        let _ = menu.append(&item_toggle);
        let _ = menu.append(&item_prev);
        let _ = menu.append(&item_next);
        let _ = menu.append(&item_quit);

        let tray = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_tooltip("Raser")
            .with_icon(icon)
            .build()
            .ok()?;

        Some(Self {
            _tray: tray,
            id_show,
            id_toggle,
            id_next,
            id_prev,
            id_quit,
        })
    }

    pub fn poll_events(&self, tx: &Sender<MediaCommand>) {
        while let Ok(ev) = TrayIconEvent::receiver().try_recv() {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = ev
            {
                let _ = tx.send(MediaCommand::ToggleWindow);
            }
        }

        while let Ok(ev) = MenuEvent::receiver().try_recv() {
            if ev.id == self.id_show {
                let _ = tx.send(MediaCommand::ShowWindow);
            } else if ev.id == self.id_toggle {
                let _ = tx.send(MediaCommand::PlayPause);
            } else if ev.id == self.id_next {
                let _ = tx.send(MediaCommand::Next);
            } else if ev.id == self.id_prev {
                let _ = tx.send(MediaCommand::Previous);
            } else if ev.id == self.id_quit {
                let _ = tx.send(MediaCommand::Quit);
            }
        }
    }
}
