use crate::taskbar::MediaCommand;
use global_hotkey::hotkey::{Code, HotKey};
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
use std::sync::mpsc::Sender;

pub struct HotkeyController {
    _manager: GlobalHotKeyManager,
    id_play_pause: u32,
    id_next: u32,
    id_prev: u32,
    id_stop: u32,
}

impl HotkeyController {
    pub fn try_new() -> Option<Self> {
        let manager = GlobalHotKeyManager::new().ok()?;
        let hk_play_pause = HotKey::new(None, Code::MediaPlayPause);
        let hk_next = HotKey::new(None, Code::MediaTrackNext);
        let hk_prev = HotKey::new(None, Code::MediaTrackPrevious);
        let hk_stop = HotKey::new(None, Code::MediaStop);

        let _ = manager.register(hk_play_pause);
        let _ = manager.register(hk_next);
        let _ = manager.register(hk_prev);
        let _ = manager.register(hk_stop);

        Some(Self {
            _manager: manager,
            id_play_pause: hk_play_pause.id(),
            id_next: hk_next.id(),
            id_prev: hk_prev.id(),
            id_stop: hk_stop.id(),
        })
    }

    pub fn poll_events(&self, tx: &Sender<MediaCommand>) {
        while let Ok(ev) = GlobalHotKeyEvent::receiver().try_recv() {
            if ev.state != HotKeyState::Pressed {
                continue;
            }
            if ev.id == self.id_play_pause {
                let _ = tx.send(MediaCommand::PlayPause);
            } else if ev.id == self.id_next {
                let _ = tx.send(MediaCommand::Next);
            } else if ev.id == self.id_prev {
                let _ = tx.send(MediaCommand::Previous);
            } else if ev.id == self.id_stop {
                let _ = tx.send(MediaCommand::Stop);
            }
        }
    }
}
