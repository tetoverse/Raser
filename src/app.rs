use crate::audio_core::{AudioCore, AudioState};
use crate::i18n::{detect_os_lang, Lang};
use crate::local_files;
use crate::storage::{
    self, now_millis, AppStateSnapshot, PlaylistRecord, RepeatMode, TrackRecord, TrackRename,
    TrackSortMode, TrackTrim, DEFAULT_SIDEBAR_ORDER,
};
use crate::ui::{
    playbar, sidebar, titlebar, views, M3Palette, CORNER_XL, SIDEBAR_CLASSIC_WIDTH,
};
use iced::keyboard::{self, key::Named, Key, Modifiers};
use iced::mouse;
use iced::widget::{column, container, operation, row, stack};
use iced::window;
use iced::{Background, Element, Event, Length, Padding, Size, Subscription, Task, Theme};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActiveView {
    Discover,
    Favorites,
    Albums,
    Local,
    History,
    Queue,
    PlaylistDetail(String),
    Settings,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsTab {
    Interface,
    Playback,
    Storage,
    About,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueuePanelTab {
    Queue,
    Recent,
}

#[derive(Debug, Clone)]
pub enum ModalState {
    CreatePlaylist {
        name: String,
        description: String,
    },
    RenamePlaylist {
        playlist_id: String,
        name: String,
        description: String,
    },
    RenameTrack {
        track: TrackRecord,
        title: String,
        artist: String,
    },
    TrimTrack {
        track: TrackRecord,
        start_input: String,
        end_input: String,
    },
    AddToPlaylist {
        track: TrackRecord,
    },
}

impl ModalState {
    pub fn has_text_input(&self) -> bool {
        !matches!(self, ModalState::AddToPlaylist { .. })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CliOptions {
    pub initial_view: Option<ActiveView>,
    pub show_help: bool,
    pub headless_check: bool,
}

pub fn parse_view_flag(val: &str) -> Option<ActiveView> {
    match val.trim().to_ascii_lowercase().as_str() {
        "discover" | "home" => Some(ActiveView::Discover),
        "favorites" | "favs" => Some(ActiveView::Favorites),
        "albums" => Some(ActiveView::Albums),
        "local" => Some(ActiveView::Local),
        "history" => Some(ActiveView::History),
        "queue" => Some(ActiveView::Queue),
        "settings" => Some(ActiveView::Settings),
        _ => None,
    }
}

pub fn parse_cli_args<I, S>(args: I) -> CliOptions
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut opts = CliOptions {
        initial_view: None,
        show_help: false,
        headless_check: false,
    };
    let mut iter = args.into_iter().skip(1);
    while let Some(arg) = iter.next() {
        let a = arg.as_ref();
        if a == "--help" || a == "-h" {
            opts.show_help = true;
        } else if a == "--headless-check" {
            opts.headless_check = true;
        } else if a == "--view" {
            if let Some(val) = iter.next() {
                opts.initial_view = parse_view_flag(val.as_ref());
            }
        } else if let Some(val) = a.strip_prefix("--view=") {
            opts.initial_view = parse_view_flag(val);
        }
    }
    opts
}

#[derive(Debug, Clone, PartialEq)]
pub enum SeekCompletion {
    DispatchNext(f64, u64),
    Committed(f64),
    RolledBack(f64),
    StaleIgnored,
}

/// Pure-Rust single-flight coalescing seek coordinator matching `tests/seek-regression.test.js`.
#[derive(Debug, Clone, Default)]
pub struct SeekCoordinator {
    pub is_loading: bool,
    pub in_flight: bool,
    pub pending_target: Option<f64>,
    pub seek_epoch: u64,
    pub last_committed_pos: f64,
    pub scrubber_drag_target: Option<f64>,
}

impl SeekCoordinator {
    pub fn begin_load(&mut self) {
        self.is_loading = true;
        self.in_flight = false;
        self.pending_target = None;
        self.scrubber_drag_target = None;
        self.seek_epoch = self.seek_epoch.wrapping_add(1);
        self.last_committed_pos = 0.0;
    }

    pub fn finish_load(&mut self, initial_pos: f64) -> Option<(f64, u64)> {
        self.is_loading = false;
        self.last_committed_pos = initial_pos;
        if let Some(target) = self.pending_target.take() {
            self.in_flight = true;
            Some((target, self.seek_epoch))
        } else {
            None
        }
    }

    pub fn request_seek(&mut self, target_secs: f64) -> Option<(f64, u64)> {
        let clamped = if target_secs.is_finite() {
            target_secs.max(0.0)
        } else {
            0.0
        };
        self.seek_epoch = self.seek_epoch.wrapping_add(1);
        if self.is_loading || self.in_flight {
            self.pending_target = Some(clamped);
            None
        } else {
            self.in_flight = true;
            Some((clamped, self.seek_epoch))
        }
    }

    pub fn complete_seek(&mut self, epoch: u64, outcome: Result<f64, f64>) -> SeekCompletion {
        self.in_flight = false;
        if let Some(next_target) = self.pending_target.take() {
            self.in_flight = true;
            return SeekCompletion::DispatchNext(next_target, self.seek_epoch);
        }
        if epoch != self.seek_epoch {
            return SeekCompletion::StaleIgnored;
        }
        match outcome {
            Ok(pos) => {
                self.last_committed_pos = pos;
                SeekCompletion::Committed(pos)
            }
            Err(backend_pos) => {
                self.last_committed_pos = backend_pos;
                SeekCompletion::RolledBack(backend_pos)
            }
        }
    }

    pub fn accept_poll_position(&mut self, poll_epoch: u64, pos: f64) -> Option<f64> {
        if self.is_loading
            || self.in_flight
            || self.pending_target.is_some()
            || self.scrubber_drag_target.is_some()
            || poll_epoch != self.seek_epoch
        {
            return None;
        }
        self.last_committed_pos = pos;
        Some(pos)
    }
}

pub fn sort_tracks_with_pins(
    tracks: &[TrackRecord],
    mode: TrackSortMode,
    pinned_ids: &[String],
    query: &str,
    renames: &HashMap<String, TrackRename>,
) -> Vec<TrackRecord> {
    let q = query.trim().to_ascii_lowercase();
    let is_pinned = |t: &TrackRecord| {
        let cid = t.canonical_id();
        pinned_ids.iter().any(|p| p == &cid || (!t.id.is_empty() && p == &t.id))
    };

    let mut filtered: Vec<(usize, bool, TrackRecord)> = tracks
        .iter()
        .enumerate()
        .filter(|(_, t)| {
            if q.is_empty() {
                return true;
            }
            let cid = t.canonical_id();
            let (title, artist) = if let Some(r) = renames.get(&cid) {
                (r.title.as_str(), r.artist.as_str())
            } else {
                (t.title.as_str(), t.artist.as_str())
            };
            title.to_ascii_lowercase().contains(&q)
                || artist.to_ascii_lowercase().contains(&q)
                || t.album.to_ascii_lowercase().contains(&q)
        })
        .map(|(i, t)| (i, is_pinned(t), t.clone()))
        .collect();

    filtered.sort_by(|(idx_a, pin_a, a), (idx_b, pin_b, b)| {
        pin_b.cmp(pin_a).then_with(|| match mode {
            TrackSortMode::Manual => idx_a.cmp(idx_b),
            TrackSortMode::AddedDesc => b
                .added_at
                .unwrap_or(0)
                .cmp(&a.added_at.unwrap_or(0))
                .then_with(|| idx_a.cmp(idx_b)),
            TrackSortMode::AddedAsc => a
                .added_at
                .unwrap_or(0)
                .cmp(&b.added_at.unwrap_or(0))
                .then_with(|| idx_a.cmp(idx_b)),
        })
    });

    filtered.into_iter().map(|(_, _, t)| t).collect()
}

pub fn evaluate_trim_jump(trim: &TrackTrim, pos_secs: f64) -> Option<f64> {
    if trim.is_active() && pos_secs < trim.start - 0.25 {
        Some(trim.start)
    } else {
        None
    }
}

pub fn evaluate_trim_end(trim: &TrackTrim, pos_secs: f64) -> bool {
    trim.is_active() && pos_secs >= trim.end
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShortcutAction {
    TogglePlayPause,
    NextTrack,
    PrevTrack,
    SeekRelative(i32),
    VolumeDelta(i8),
    ToggleMute,
    ToggleShuffle,
    CycleRepeat,
    ToggleMiniPlayer,
    ToggleFullscreen,
    FocusSearch,
    EscapeUnwind,
}

pub fn resolve_shortcut(
    key: &Key,
    modifiers: Modifiers,
    input_focused: bool,
    modal_open: bool,
) -> Option<ShortcutAction> {
    if let Key::Named(Named::Escape) = key {
        return Some(ShortcutAction::EscapeUnwind);
    }
    if let Key::Named(Named::F11) = key {
        return Some(ShortcutAction::ToggleFullscreen);
    }
    if modal_open {
        return None;
    }

    let ctrl = modifiers.control() || modifiers.command();
    let shift = modifiers.shift();

    if ctrl {
        if let Key::Character(c) = key {
            if c.eq_ignore_ascii_case("k") {
                return Some(ShortcutAction::FocusSearch);
            }
        }
    }

    if input_focused {
        return None;
    }

    match key {
        Key::Named(Named::Space) if !ctrl && !shift && !modifiers.alt() => {
            Some(ShortcutAction::TogglePlayPause)
        }
        Key::Named(Named::ArrowRight) if ctrl => Some(ShortcutAction::NextTrack),
        Key::Named(Named::ArrowLeft) if ctrl => Some(ShortcutAction::PrevTrack),
        Key::Named(Named::ArrowRight) if shift => Some(ShortcutAction::SeekRelative(5)),
        Key::Named(Named::ArrowLeft) if shift => Some(ShortcutAction::SeekRelative(-5)),
        Key::Named(Named::ArrowUp) if ctrl => Some(ShortcutAction::VolumeDelta(5)),
        Key::Named(Named::ArrowDown) if ctrl => Some(ShortcutAction::VolumeDelta(-5)),
        Key::Character(c) if !ctrl && !modifiers.alt() => match c.to_ascii_lowercase().as_str() {
            "m" => Some(ShortcutAction::ToggleMute),
            "h" => Some(ShortcutAction::ToggleShuffle),
            "r" => Some(ShortcutAction::CycleRepeat),
            "p" => Some(ShortcutAction::ToggleMiniPlayer),
            _ => None,
        },
        _ => None,
    }
}

pub fn format_mm_ss(secs: f64) -> String {
    if !secs.is_finite() || secs <= 0.0 {
        return "0:00".to_string();
    }
    let total = secs.floor() as u64;
    let mins = total / 60;
    let rem = total % 60;
    format!("{}:{:02}", mins, rem)
}

pub fn format_bytes(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{} B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else if bytes < 1024 * 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.2} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}

#[derive(Debug, Clone)]
pub enum Message {
    Noop,
    WindowOpened(Option<window::Id>),
    DragWindow,
    MinimizeWindow,
    ToggleMaximizeWindow,
    ToggleFullscreen,
    CloseWindow,
    ToggleMiniPlayer,
    TitlebarHoverEnter,
    TitlebarHoverExit,
    TitlebarHideCheck,

    SwitchView(ActiveView),
    NavigateBack,
    SearchQueryChanged(String),
    SearchFocused(bool),
    SearchFocusResolved(bool),
    FocusSearchBox,

    PlayTracks(Vec<TrackRecord>, usize),
    PlayPlaylist(String),
    JumpToQueueIndex(usize),
    TogglePlayPause,
    NextTrack,
    PrevTrack,
    ToggleShuffle,
    CycleRepeat,
    SetVolume(f32),
    ToggleMute,
    SetSpeed(f32),
    CycleSpeed,

    ScrubberDrag(f64),
    ScrubberCommit(f64),
    AudioLoadFinished(u64, Result<AudioState, String>),
    AudioSeekFinished(u64, Result<AudioState, String>),
    Tick,

    PickLocalFiles,
    PickLocalFolder,
    LocalFilesPicked(Vec<TrackRecord>),
    RemoveLocalTrack(TrackRecord),

    ToggleFavorite(TrackRecord),
    TogglePinTrack(TrackRecord),
    CycleFavoritesSort,
    CyclePlaylistSort(String),
    ClearHistory,
    ClearQueue,
    RemoveFromQueue(usize),

    ToggleQueuePanel,
    SelectQueuePanelTab(QueuePanelTab),

    OpenCreatePlaylistModal,
    OpenRenamePlaylistModal(String),
    DeletePlaylist(String),
    OpenRenameTrackModal(TrackRecord),
    OpenTrimTrackModal(TrackRecord),
    OpenAddToPlaylistModal(TrackRecord),
    CloseModal,

    ModalPlaylistNameChanged(String),
    ModalPlaylistDescChanged(String),
    SubmitCreatePlaylist,
    SubmitRenamePlaylist,

    ModalTrackTitleChanged(String),
    ModalTrackArtistChanged(String),
    SubmitRenameTrack,

    ModalTrimStartChanged(String),
    ModalTrimEndChanged(String),
    SubmitSaveTrim,
    SubmitResetTrim,
    AddTrackToPlaylist(String, TrackRecord),
    RemoveTrackFromPlaylist(String, TrackRecord),

    SelectSettingsTab(SettingsTab),
    SetLang(Lang),
    SetThemeMode(String),
    ToggleHideTitlebar,
    ToggleAutohideTimestamps,
    ToggleClassicSidebar,
    ToggleAutoplay,
    MoveSidebarItem(usize, bool),
    ResetSidebarOrder,
    ClearCache,
    OpenCacheFolder,

    SystemEvent(Event),
}

pub struct RaserApp {
    pub audio: Arc<AudioCore>,
    pub snapshot: AppStateSnapshot,
    pub lang: Lang,
    pub active_view: ActiveView,
    pub previous_view: Option<ActiveView>,
    pub settings_tab: SettingsTab,
    pub queue_panel_tab: QueuePanelTab,
    pub search_query: String,
    pub search_focused: bool,
    pub focus_search_pending: bool,

    pub queue: Vec<TrackRecord>,
    pub queue_index: Option<usize>,
    pub play_history: Vec<usize>,
    pub future_history: Vec<usize>,
    pub last_prev_click: Option<Instant>,

    pub audio_state: AudioState,
    pub seek_coordinator: SeekCoordinator,
    pub load_epoch: u64,
    pub last_ended_reported: bool,
    pub scrubber_wave_phase: f32,

    pub main_window_id: Option<window::Id>,
    pub is_mini_player: bool,
    pub is_fullscreen: bool,
    pub is_maximized: bool,
    pub titlebar_revealed: bool,
    pub titlebar_hover_left_at: Option<Instant>,

    pub modal: Option<ModalState>,
    pub cache_stats: (u64, usize),
}

impl RaserApp {
    pub fn boot(initial_view: Option<ActiveView>) -> (Self, Task<Message>) {
        let mut snapshot = AppStateSnapshot::load();
        let lang = snapshot
            .settings
            .ui_lang
            .as_deref()
            .map(Lang::from_code)
            .unwrap_or_else(detect_os_lang);

        let audio = Arc::new(AudioCore::new());
        let initial_vol = if snapshot.settings.muted {
            0.0
        } else {
            snapshot.settings.volume.clamp(0.0, 1.0)
        };
        let _ = audio.set_volume(initial_vol);
        let _ = audio.set_speed(snapshot.settings.speed);

        let queue = snapshot.queue.clone();
        let queue_index = if !queue.is_empty() { Some(0) } else { None };
        let audio_state = audio.get_state();
        let cache_stats = storage::get_cache_stats();

        // Ensure snapshot has normalized settings on first boot if needed
        if snapshot.settings.sidebar_order.is_empty() {
            snapshot.settings.sidebar_order =
                DEFAULT_SIDEBAR_ORDER.iter().map(|s| (*s).to_string()).collect();
        }

        let app = Self {
            audio,
            snapshot,
            lang,
            active_view: initial_view.unwrap_or(ActiveView::Discover),
            previous_view: None,
            settings_tab: SettingsTab::Interface,
            queue_panel_tab: QueuePanelTab::Queue,
            search_query: String::new(),
            search_focused: false,
            focus_search_pending: false,
            queue,
            queue_index,
            play_history: Vec::new(),
            future_history: Vec::new(),
            last_prev_click: None,
            audio_state,
            seek_coordinator: SeekCoordinator::default(),
            load_epoch: 0,
            last_ended_reported: false,
            scrubber_wave_phase: 0.0,
            main_window_id: None,
            is_mini_player: false,
            is_fullscreen: false,
            is_maximized: false,
            titlebar_revealed: false,
            titlebar_hover_left_at: None,
            modal: None,
            cache_stats,
        };

        (app, window::oldest().map(Message::WindowOpened))
    }

    pub fn palette(&self) -> M3Palette {
        M3Palette::from_mode(&self.snapshot.settings.theme_mode)
    }

    pub fn theme(&self) -> Theme {
        self.palette().to_iced_theme()
    }

    pub fn title(&self) -> String {
        if let Some(tr) = self.current_track() {
            format!("{} — Raser", self.display_title(&tr))
        } else {
            "Raser".to_string()
        }
    }

    pub fn current_track(&self) -> Option<TrackRecord> {
        self.queue_index.and_then(|i| self.queue.get(i).cloned())
    }

    pub fn display_title(&self, tr: &TrackRecord) -> String {
        let cid = tr.canonical_id();
        if let Some(ren) = self.snapshot.track_renames.get(&cid) {
            if !ren.title.trim().is_empty() {
                return ren.title.clone();
            }
        }
        if tr.title.trim().is_empty() {
            "Unknown Track".to_string()
        } else {
            tr.title.clone()
        }
    }

    pub fn display_artist(&self, tr: &TrackRecord) -> String {
        let cid = tr.canonical_id();
        if let Some(ren) = self.snapshot.track_renames.get(&cid) {
            if !ren.artist.trim().is_empty() {
                return ren.artist.clone();
            }
        }
        if tr.artist.trim().is_empty() {
            "Unknown Artist".to_string()
        } else {
            tr.artist.clone()
        }
    }

    pub fn is_favorite(&self, tr: &TrackRecord) -> bool {
        self.snapshot.favorites.iter().any(|f| f.matches(tr))
    }

    pub fn is_pinned(&self, tr: &TrackRecord) -> bool {
        let cid = tr.canonical_id();
        self.snapshot
            .settings
            .pinned_tracks
            .iter()
            .any(|p| p == &cid || (!tr.id.is_empty() && p == &tr.id))
    }

    pub fn effective_duration(&self) -> f64 {
        if self.audio_state.duration > 0.0 {
            self.audio_state.duration
        } else {
            self.current_track().map(|t| t.duration).unwrap_or(0.0)
        }
    }

    pub fn displayed_position(&self) -> f64 {
        if let Some(drag) = self.seek_coordinator.scrubber_drag_target {
            drag
        } else if self.seek_coordinator.in_flight || self.seek_coordinator.pending_target.is_some()
        {
            self.seek_coordinator
                .pending_target
                .unwrap_or(self.seek_coordinator.last_committed_pos)
        } else {
            self.audio_state.position
        }
    }

    fn record_history(&mut self, tr: &TrackRecord) {
        let mut entry = tr.clone();
        entry.played_at = Some(now_millis());
        self.snapshot.history.retain(|h| !h.matches(&entry));
        self.snapshot.history.insert(0, entry.clone());
        if self.snapshot.history.len() > 100 {
            self.snapshot.history.truncate(100);
        }
        self.snapshot.last_track = Some(entry);
    }

    fn load_track_task(&mut self, tr: &TrackRecord, record_history: bool) -> Task<Message> {
        let url = tr.playable_source();
        if url.trim().is_empty() {
            return Task::none();
        }
        if record_history {
            self.record_history(tr);
            self.snapshot.queue = self.queue.clone();
            self.snapshot.save();
        }

        self.load_epoch = self.load_epoch.wrapping_add(1);
        let my_load_epoch = self.load_epoch;
        self.seek_coordinator.begin_load();
        self.last_ended_reported = false;

        let cid = tr.canonical_id();
        let start_pos = self
            .snapshot
            .track_trims
            .get(&cid)
            .and_then(|trim| if trim.is_active() { Some(trim.start) } else { None });
        let dur_hint = if tr.duration > 0.0 {
            Some(tr.duration)
        } else {
            None
        };
        let track_id = if !tr.id.is_empty() {
            Some(tr.id.clone())
        } else {
            None
        };

        let audio = Arc::clone(&self.audio);
        Task::perform(
            async move {
                audio
                    .load_audio_async(url, start_pos, dur_hint, true, track_id)
                    .await
            },
            move |res| Message::AudioLoadFinished(my_load_epoch, res),
        )
    }

    fn start_loading_current(&mut self) -> Task<Message> {
        let Some(tr) = self.current_track() else {
            return Task::none();
        };
        self.load_track_task(&tr, true)
    }

    fn dispatch_seek_task(&self, target_secs: f64, epoch: u64) -> Task<Message> {
        let audio = Arc::clone(&self.audio);
        Task::perform(
            async move { audio.seek_async(target_secs).await },
            move |res| Message::AudioSeekFinished(epoch, res),
        )
    }

    fn request_seek(&mut self, target_secs: f64) -> Task<Message> {
        let dur = self.effective_duration();
        let clamped = if dur > 0.0 {
            target_secs.clamp(0.0, (dur - 0.1).max(0.0))
        } else {
            target_secs.max(0.0)
        };
        if let Some((pos, epoch)) = self.seek_coordinator.request_seek(clamped) {
            self.dispatch_seek_task(pos, epoch)
        } else {
            Task::none()
        }
    }

    fn play_next(&mut self) -> Task<Message> {
        if self.queue.is_empty() {
            return Task::none();
        }
        while let Some(next_idx) = self.future_history.pop() {
            if next_idx < self.queue.len() {
                if let Some(cur) = self.queue_index {
                    if cur < self.queue.len() {
                        self.play_history.push(cur);
                    }
                }
                self.queue_index = Some(next_idx);
                return self.start_loading_current();
            }
        }

        let cur = self.queue_index.unwrap_or(0);
        let len = self.queue.len();
        let next_idx = if self.snapshot.settings.shuffle && len > 1 {
            let seed = now_millis() as usize;
            let cand = seed % len;
            if cand == cur {
                (cur + 1) % len
            } else {
                cand
            }
        } else if cur + 1 < len {
            cur + 1
        } else if self.snapshot.settings.repeat_mode == RepeatMode::All {
            0
        } else {
            let _ = self.audio.pause();
            self.audio_state = self.audio.get_state();
            return Task::none();
        };

        self.play_history.push(cur);
        self.queue_index = Some(next_idx);
        self.start_loading_current()
    }

    fn play_prev(&mut self) -> Task<Message> {
        if self.queue.is_empty() {
            return Task::none();
        }
        let now = Instant::now();
        let recent_click = self
            .last_prev_click
            .map(|t| now.duration_since(t) < Duration::from_secs(2))
            .unwrap_or(false);
        self.last_prev_click = Some(now);

        let pos = self.displayed_position();
        if pos > 3.0 && !recent_click {
            return self.request_seek(0.0);
        }

        while let Some(prev_idx) = self.play_history.pop() {
            if prev_idx < self.queue.len() {
                if let Some(cur) = self.queue_index {
                    if cur < self.queue.len() {
                        self.future_history.push(cur);
                    }
                }
                self.queue_index = Some(prev_idx);
                return self.start_loading_current();
            }
        }

        let cur = self.queue_index.unwrap_or(0);
        let prev_idx = if cur > 0 {
            cur - 1
        } else if self.snapshot.settings.repeat_mode == RepeatMode::All {
            self.queue.len().saturating_sub(1)
        } else {
            return self.request_seek(0.0);
        };
        self.future_history.push(cur);
        self.queue_index = Some(prev_idx);
        self.start_loading_current()
    }

    fn unfocus_search(&mut self) -> Task<Message> {
        self.focus_search_pending = false;
        if self.search_focused {
            self.search_focused = false;
            operation::focus(iced::widget::Id::new("raser-unfocus-sentinel"))
        } else {
            Task::none()
        }
    }

    fn switch_view(&mut self, next: ActiveView) -> Task<Message> {
        if self.active_view != next {
            self.previous_view = Some(self.active_view.clone());
            self.active_view = next;
        }
        self.unfocus_search()
    }

    fn navigate_back(&mut self) {
        if let Some(prev) = self.previous_view.take() {
            self.active_view = prev;
        } else {
            self.active_view = ActiveView::Discover;
        }
    }

    fn execute_shortcut(&mut self, action: ShortcutAction) -> Task<Message> {
        match action {
            ShortcutAction::TogglePlayPause => self.update(Message::TogglePlayPause),
            ShortcutAction::NextTrack => self.update(Message::NextTrack),
            ShortcutAction::PrevTrack => self.update(Message::PrevTrack),
            ShortcutAction::SeekRelative(delta) => {
                let target = (self.displayed_position() + delta as f64).max(0.0);
                self.request_seek(target)
            }
            ShortcutAction::VolumeDelta(delta) => {
                self.snapshot.settings.muted = false;
                let next = (self.snapshot.settings.volume + delta as f32 / 100.0).clamp(0.0, 1.0);
                self.update(Message::SetVolume(next))
            }
            ShortcutAction::ToggleMute => self.update(Message::ToggleMute),
            ShortcutAction::ToggleShuffle => self.update(Message::ToggleShuffle),
            ShortcutAction::CycleRepeat => self.update(Message::CycleRepeat),
            ShortcutAction::ToggleMiniPlayer => self.update(Message::ToggleMiniPlayer),
            ShortcutAction::ToggleFullscreen => self.update(Message::ToggleFullscreen),
            ShortcutAction::FocusSearch => self.update(Message::FocusSearchBox),
            ShortcutAction::EscapeUnwind => {
                if self.modal.is_some() {
                    self.modal = None;
                    Task::none()
                } else if self.search_focused {
                    self.unfocus_search()
                } else if matches!(self.active_view, ActiveView::PlaylistDetail(_)) {
                    self.navigate_back();
                    Task::none()
                } else {
                    Task::none()
                }
            }
        }
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Noop => Task::none(),
            Message::WindowOpened(id) => {
                self.main_window_id = id;
                Task::none()
            }
            Message::DragWindow => {
                if let Some(id) = self.main_window_id {
                    window::drag(id)
                } else {
                    window::oldest().then(|opt| opt.map(window::drag).unwrap_or_else(Task::none))
                }
            }
            Message::MinimizeWindow => {
                if let Some(id) = self.main_window_id {
                    window::minimize(id, true)
                } else {
                    Task::none()
                }
            }
            Message::ToggleMaximizeWindow => {
                self.is_maximized = !self.is_maximized;
                if let Some(id) = self.main_window_id {
                    window::toggle_maximize(id)
                } else {
                    Task::none()
                }
            }
            Message::ToggleFullscreen => {
                self.is_fullscreen = !self.is_fullscreen;
                let mode = if self.is_fullscreen {
                    window::Mode::Fullscreen
                } else {
                    window::Mode::Windowed
                };
                if let Some(id) = self.main_window_id {
                    window::set_mode(id, mode)
                } else {
                    Task::none()
                }
            }
            Message::CloseWindow => {
                self.snapshot.save();
                if let Some(id) = self.main_window_id {
                    window::close(id)
                } else {
                    iced::exit()
                }
            }
            Message::ToggleMiniPlayer => {
                self.is_mini_player = !self.is_mini_player;
                if let Some(id) = self.main_window_id {
                    let target = if self.is_mini_player {
                        Size::new(400.0, 560.0)
                    } else {
                        Size::new(1280.0, 820.0)
                    };
                    window::resize(id, target)
                } else {
                    Task::none()
                }
            }
            Message::TitlebarHoverEnter => {
                self.titlebar_revealed = true;
                self.titlebar_hover_left_at = None;
                Task::none()
            }
            Message::TitlebarHoverExit => {
                self.titlebar_hover_left_at = Some(Instant::now());
                Task::none()
            }
            Message::TitlebarHideCheck => {
                if let Some(left_at) = self.titlebar_hover_left_at {
                    if left_at.elapsed() >= Duration::from_millis(800) {
                        self.titlebar_revealed = false;
                        self.titlebar_hover_left_at = None;
                    }
                }
                Task::none()
            }
            Message::SwitchView(v) => self.switch_view(v),
            Message::NavigateBack => {
                self.navigate_back();
                Task::none()
            }
            Message::SearchQueryChanged(q) => {
                self.search_query = q;
                self.search_focused = true;
                Task::none()
            }
            Message::SearchFocused(f) => {
                if !self.focus_search_pending || f {
                    self.search_focused = f;
                }
                Task::none()
            }
            Message::SearchFocusResolved(f) => {
                if self.focus_search_pending {
                    self.focus_search_pending = false;
                    self.search_focused = f;
                }
                Task::none()
            }
            Message::FocusSearchBox => {
                self.search_focused = true;
                self.focus_search_pending = true;
                operation::focus(views::search_input_id()).chain(
                    operation::is_focused(views::search_input_id())
                        .map(Message::SearchFocusResolved),
                )
            }
            Message::PlayTracks(tracks, idx) => {
                if tracks.is_empty() {
                    return Task::none();
                }
                let unfocus = self.unfocus_search();
                self.queue = tracks;
                self.queue_index = Some(idx.min(self.queue.len() - 1));
                self.play_history.clear();
                self.future_history.clear();
                Task::batch([unfocus, self.start_loading_current()])
            }
            Message::PlayPlaylist(pid) => {
                if let Some(pl) = self.snapshot.playlists.iter().find(|p| p.id == pid) {
                    if !pl.tracks.is_empty() {
                        let tracks = pl.tracks.clone();
                        return self.update(Message::PlayTracks(tracks, 0));
                    }
                }
                Task::none()
            }
            Message::JumpToQueueIndex(idx) => {
                if idx < self.queue.len() {
                    if let Some(cur) = self.queue_index {
                        if cur != idx {
                            self.play_history.push(cur);
                        }
                    }
                    self.queue_index = Some(idx);
                    return self.start_loading_current();
                }
                Task::none()
            }
            Message::TogglePlayPause => {
                if self.seek_coordinator.is_loading {
                    return Task::none();
                }
                if !self.queue.is_empty() {
                    if self.current_track().is_none()
                        || self.audio_state.url.is_none()
                        || self.audio_state.ended
                        || self.audio_state.error.is_some()
                    {
                        if self.current_track().is_none() {
                            self.queue_index = Some(0);
                        }
                        return self.start_loading_current();
                    }
                } else if self.audio_state.url.is_none() || self.audio_state.error.is_some() {
                    return Task::none();
                }
                let _ = self.audio.toggle();
                self.audio_state = self.audio.get_state();
                Task::none()
            }
            Message::NextTrack => self.play_next(),
            Message::PrevTrack => self.play_prev(),
            Message::ToggleShuffle => {
                self.snapshot.settings.shuffle = !self.snapshot.settings.shuffle;
                self.snapshot.save();
                Task::none()
            }
            Message::CycleRepeat => {
                self.snapshot.settings.repeat_mode = self.snapshot.settings.repeat_mode.next();
                self.snapshot.save();
                Task::none()
            }
            Message::SetVolume(vol) => {
                let v = vol.clamp(0.0, 1.0);
                self.snapshot.settings.volume = v;
                if v > 0.001 {
                    self.snapshot.settings.muted = false;
                }
                let _ = self.audio.set_volume(v);
                self.audio_state = self.audio.get_state();
                self.snapshot.save();
                Task::none()
            }
            Message::ToggleMute => {
                self.snapshot.settings.muted = !self.snapshot.settings.muted;
                let applied = if self.snapshot.settings.muted {
                    0.0
                } else {
                    self.snapshot.settings.volume.clamp(0.0, 1.0)
                };
                let _ = self.audio.set_volume(applied);
                self.audio_state = self.audio.get_state();
                self.snapshot.save();
                Task::none()
            }
            Message::SetSpeed(spd) => {
                let clamped = spd.clamp(0.25, 4.0);
                self.snapshot.settings.speed = clamped;
                let _ = self.audio.set_speed(clamped);
                self.audio_state = self.audio.get_state();
                self.snapshot.save();
                Task::none()
            }
            Message::CycleSpeed => {
                let presets = [0.5f32, 0.75, 1.0, 1.25, 1.5, 2.0];
                let cur = self.snapshot.settings.speed;
                let next = presets
                    .iter()
                    .copied()
                    .find(|&p| p > cur + 0.05)
                    .unwrap_or(presets[0]);
                self.update(Message::SetSpeed(next))
            }
            Message::ScrubberDrag(target) => {
                self.seek_coordinator.scrubber_drag_target = Some(target);
                Task::none()
            }
            Message::ScrubberCommit(target) => {
                self.seek_coordinator.scrubber_drag_target = None;
                self.request_seek(target)
            }
            Message::AudioLoadFinished(epoch, res) => {
                if epoch != self.load_epoch {
                    return Task::none();
                }
                match res {
                    Ok(st) => {
                        let initial_pos = st.position;
                        if let Some(cur) = self.queue_index.and_then(|i| self.queue.get_mut(i)) {
                            if cur.duration <= 0.0 && st.duration > 0.0 {
                                cur.duration = st.duration;
                            }
                        }
                        self.audio_state = st;
                        if let Some((next_pos, seek_epoch)) =
                            self.seek_coordinator.finish_load(initial_pos)
                        {
                            return self.dispatch_seek_task(next_pos, seek_epoch);
                        }
                    }
                    Err(e) => {
                        let _ = self.seek_coordinator.finish_load(0.0);
                        self.audio_state = self.audio.get_state();
                        self.audio_state.error = Some(e);
                    }
                }
                Task::none()
            }
            Message::AudioSeekFinished(epoch, res) => {
                let outcome = match res {
                    Ok(st) => {
                        self.audio_state = st.clone();
                        Ok(st.position)
                    }
                    Err(_) => {
                        let st = self.audio.get_state();
                        self.audio_state = st.clone();
                        Err(st.position)
                    }
                };
                match self.seek_coordinator.complete_seek(epoch, outcome) {
                    SeekCompletion::DispatchNext(next_pos, next_epoch) => {
                        self.dispatch_seek_task(next_pos, next_epoch)
                    }
                    SeekCompletion::Committed(_)
                    | SeekCompletion::RolledBack(_)
                    | SeekCompletion::StaleIgnored => Task::none(),
                }
            }
            Message::Tick => {
                let st = self.audio.get_state();
                if st.playing {
                    self.scrubber_wave_phase =
                        (self.scrubber_wave_phase + 0.14) % std::f32::consts::TAU;
                }
                if let Some(pos) = self
                    .seek_coordinator
                    .accept_poll_position(self.seek_coordinator.seek_epoch, st.position)
                {
                    let _ = pos;
                }
                let prev_ended = self.last_ended_reported;
                self.last_ended_reported = st.ended;
                self.audio_state = st.clone();

                // Enforce active TrackTrim during playback
                if st.playing && !st.loading && !self.seek_coordinator.in_flight {
                    if let Some(tr) = self.current_track() {
                        if let Some(trim) = self.snapshot.track_trims.get(&tr.canonical_id()) {
                            if let Some(jump) = evaluate_trim_jump(trim, st.position) {
                                return self.request_seek(jump);
                            }
                            if evaluate_trim_end(trim, st.position) {
                                if self.snapshot.settings.repeat_mode == RepeatMode::One {
                                    return self.request_seek(trim.start);
                                } else {
                                    return self.play_next();
                                }
                            }
                        }
                    }
                }

                // Auto-advance on track end rising edge
                if st.ended && !prev_ended {
                    if self.snapshot.settings.repeat_mode == RepeatMode::One {
                        if let Some(tr) = self.current_track() {
                            return self.load_track_task(&tr, false);
                        }
                    } else if self.snapshot.settings.advanced.autoplay
                        || self.snapshot.settings.repeat_mode == RepeatMode::All
                        || self
                            .queue_index
                            .map(|i| i + 1 < self.queue.len())
                            .unwrap_or(false)
                    {
                        return self.play_next();
                    }
                }

                if self.titlebar_hover_left_at.is_some() {
                    return self.update(Message::TitlebarHideCheck);
                }

                Task::none()
            }
            Message::PickLocalFiles => Task::perform(
                async {
                    local_files::pick_files_async()
                        .await
                        .into_iter()
                        .map(TrackRecord::from)
                        .collect()
                },
                Message::LocalFilesPicked,
            ),
            Message::PickLocalFolder => Task::perform(
                async {
                    local_files::pick_folder_async()
                        .await
                        .into_iter()
                        .map(TrackRecord::from)
                        .collect()
                },
                Message::LocalFilesPicked,
            ),
            Message::LocalFilesPicked(items) => {
                if !items.is_empty() {
                    for item in items {
                        self.snapshot
                            .removed_local_paths
                            .retain(|p| p != &item.path && p != &item.url);
                        if !self
                            .snapshot
                            .local_tracks
                            .iter()
                            .any(|existing| existing.matches(&item))
                        {
                            self.snapshot.local_tracks.push(item);
                        }
                    }
                    self.snapshot.save();
                }
                Task::none()
            }
            Message::RemoveLocalTrack(tr) => {
                let p = if !tr.path.is_empty() {
                    tr.path.clone()
                } else {
                    let ps = tr.playable_source();
                    if !ps.is_empty() {
                        ps
                    } else {
                        tr.url.clone()
                    }
                };
                if !p.is_empty() && !self.snapshot.removed_local_paths.contains(&p) {
                    self.snapshot.removed_local_paths.push(p);
                }
                self.snapshot.local_tracks.retain(|t| !t.matches(&tr));
                self.snapshot.save();
                Task::none()
            }
            Message::ToggleFavorite(tr) => {
                if let Some(pos) = self.snapshot.favorites.iter().position(|f| f.matches(&tr)) {
                    self.snapshot.favorites.remove(pos);
                } else {
                    let mut added = tr;
                    added.added_at = Some(now_millis());
                    self.snapshot.favorites.insert(0, added);
                }
                self.snapshot.save();
                Task::none()
            }
            Message::TogglePinTrack(tr) => {
                let cid = tr.canonical_id();
                let was_pinned = self
                    .snapshot
                    .settings
                    .pinned_tracks
                    .iter()
                    .any(|p| p == &cid || (!tr.id.is_empty() && p == &tr.id));
                if was_pinned {
                    self.snapshot
                        .settings
                        .pinned_tracks
                        .retain(|p| p != &cid && (tr.id.is_empty() || p != &tr.id));
                } else {
                    self.snapshot.settings.pinned_tracks.push(cid);
                }
                self.snapshot.save();
                Task::none()
            }
            Message::CycleFavoritesSort => {
                self.snapshot.settings.favorites_sort =
                    self.snapshot.settings.favorites_sort.next();
                self.snapshot.save();
                Task::none()
            }
            Message::CyclePlaylistSort(pid) => {
                if let Some(pl) = self.snapshot.playlists.iter_mut().find(|p| p.id == pid) {
                    let cur = pl
                        .sort_mode
                        .as_deref()
                        .map(TrackSortMode::from_str_lossy)
                        .unwrap_or(self.snapshot.settings.playlist_sort);
                    let next = cur.next();
                    pl.sort_mode = Some(next.as_str().to_string());
                    self.snapshot.settings.playlist_sort = next;
                    self.snapshot.save();
                }
                Task::none()
            }
            Message::ClearHistory => {
                self.snapshot.history.clear();
                self.snapshot.save();
                Task::none()
            }
            Message::ClearQueue => {
                self.queue.clear();
                self.queue_index = None;
                self.play_history.clear();
                self.future_history.clear();
                self.load_epoch = self.load_epoch.wrapping_add(1);
                self.seek_coordinator = SeekCoordinator::default();
                self.last_ended_reported = false;
                self.audio.stop();
                self.audio_state = self.audio.get_state();
                self.snapshot.queue.clear();
                self.snapshot.save();
                Task::none()
            }
            Message::RemoveFromQueue(idx) => {
                if idx < self.queue.len() {
                    let removed_active = self.queue_index == Some(idx);
                    let was_playing = self.audio_state.playing || self.audio_state.loading;
                    self.queue.remove(idx);

                    let remap = |stack: &mut Vec<usize>| {
                        stack.retain_mut(|i| {
                            if *i == idx {
                                false
                            } else {
                                if *i > idx {
                                    *i -= 1;
                                }
                                true
                            }
                        });
                    };
                    remap(&mut self.play_history);
                    remap(&mut self.future_history);

                    if self.queue.is_empty() {
                        self.queue_index = None;
                        self.play_history.clear();
                        self.future_history.clear();
                        self.load_epoch = self.load_epoch.wrapping_add(1);
                        self.seek_coordinator = SeekCoordinator::default();
                        self.last_ended_reported = false;
                        self.audio.stop();
                        self.audio_state = self.audio.get_state();
                        self.snapshot.queue.clear();
                        self.snapshot.save();
                        return Task::none();
                    }

                    if let Some(cur) = self.queue_index {
                        if idx < cur {
                            self.queue_index = Some(cur - 1);
                        } else if removed_active {
                            self.queue_index = Some(cur.min(self.queue.len() - 1));
                            self.load_epoch = self.load_epoch.wrapping_add(1);
                            self.seek_coordinator = SeekCoordinator::default();
                            self.last_ended_reported = false;
                            self.audio.stop();
                            self.audio_state = self.audio.get_state();
                            self.snapshot.queue = self.queue.clone();
                            self.snapshot.save();
                            if was_playing {
                                return self.start_loading_current();
                            }
                            return Task::none();
                        }
                    }

                    self.snapshot.queue = self.queue.clone();
                    self.snapshot.save();
                }
                Task::none()
            }
            Message::ToggleQueuePanel => {
                self.snapshot.settings.queue_panel_open = !self.snapshot.settings.queue_panel_open;
                self.snapshot.save();
                Task::none()
            }
            Message::SelectQueuePanelTab(tab) => {
                self.queue_panel_tab = tab;
                Task::none()
            }
            Message::OpenCreatePlaylistModal => {
                self.modal = Some(ModalState::CreatePlaylist {
                    name: String::new(),
                    description: String::new(),
                });
                self.unfocus_search()
            }
            Message::OpenRenamePlaylistModal(pid) => {
                if let Some(pl) = self.snapshot.playlists.iter().find(|p| p.id == pid) {
                    self.modal = Some(ModalState::RenamePlaylist {
                        playlist_id: pid,
                        name: pl.name.clone(),
                        description: pl.description.clone().unwrap_or_default(),
                    });
                }
                self.unfocus_search()
            }
            Message::DeletePlaylist(pid) => {
                self.snapshot.playlists.retain(|p| p.id != pid);
                if matches!(&self.active_view, ActiveView::PlaylistDetail(id) if id == &pid) {
                    self.active_view = ActiveView::Discover;
                }
                self.snapshot.save();
                Task::none()
            }
            Message::OpenRenameTrackModal(tr) => {
                let title = self.display_title(&tr);
                let artist = self.display_artist(&tr);
                self.modal = Some(ModalState::RenameTrack {
                    track: tr,
                    title,
                    artist,
                });
                self.unfocus_search()
            }
            Message::OpenTrimTrackModal(tr) => {
                let cid = tr.canonical_id();
                let (s, e) = self
                    .snapshot
                    .track_trims
                    .get(&cid)
                    .map(|tm| (format!("{:.1}", tm.start), format!("{:.1}", tm.end)))
                    .unwrap_or_else(|| {
                        let dur = if tr.duration > 0.0 {
                            tr.duration
                        } else {
                            self.effective_duration()
                        };
                        ("0.0".to_string(), format!("{:.1}", dur.max(0.0)))
                    });
                self.modal = Some(ModalState::TrimTrack {
                    track: tr,
                    start_input: s,
                    end_input: e,
                });
                self.unfocus_search()
            }
            Message::OpenAddToPlaylistModal(tr) => {
                self.modal = Some(ModalState::AddToPlaylist { track: tr });
                self.unfocus_search()
            }
            Message::CloseModal => {
                self.modal = None;
                Task::none()
            }
            Message::ModalPlaylistNameChanged(val) => {
                if let Some(
                    ModalState::CreatePlaylist { name, .. }
                    | ModalState::RenamePlaylist { name, .. },
                ) = &mut self.modal
                {
                    *name = val;
                }
                Task::none()
            }
            Message::ModalPlaylistDescChanged(val) => {
                if let Some(
                    ModalState::CreatePlaylist { description, .. }
                    | ModalState::RenamePlaylist { description, .. },
                ) = &mut self.modal
                {
                    *description = val;
                }
                Task::none()
            }
            Message::SubmitCreatePlaylist => {
                if let Some(ModalState::CreatePlaylist { name, description }) = self.modal.take() {
                    let trimmed = name.trim();
                    if !trimmed.is_empty() {
                        let id = format!("pl_{}", now_millis());
                        let desc = if description.trim().is_empty() {
                            None
                        } else {
                            Some(description.trim().to_string())
                        };
                        self.snapshot.playlists.push(PlaylistRecord {
                            id: id.clone(),
                            name: trimmed.to_string(),
                            description: desc,
                            cover: None,
                            tracks: Vec::new(),
                            sort_mode: None,
                            extra: HashMap::new(),
                        });
                        self.snapshot.save();
                        return self.switch_view(ActiveView::PlaylistDetail(id));
                    }
                }
                Task::none()
            }
            Message::SubmitRenamePlaylist => {
                if let Some(ModalState::RenamePlaylist {
                    playlist_id,
                    name,
                    description,
                }) = self.modal.take()
                {
                    let trimmed = name.trim();
                    if !trimmed.is_empty() {
                        if let Some(pl) =
                            self.snapshot.playlists.iter_mut().find(|p| p.id == playlist_id)
                        {
                            pl.name = trimmed.to_string();
                            pl.description = if description.trim().is_empty() {
                                None
                            } else {
                                Some(description.trim().to_string())
                            };
                            self.snapshot.save();
                        }
                    }
                }
                Task::none()
            }
            Message::ModalTrackTitleChanged(val) => {
                if let Some(ModalState::RenameTrack { title, .. }) = &mut self.modal {
                    *title = val;
                }
                Task::none()
            }
            Message::ModalTrackArtistChanged(val) => {
                if let Some(ModalState::RenameTrack { artist, .. }) = &mut self.modal {
                    *artist = val;
                }
                Task::none()
            }
            Message::SubmitRenameTrack => {
                if let Some(ModalState::RenameTrack {
                    track,
                    title,
                    artist,
                }) = self.modal.take()
                {
                    let cid = track.canonical_id();
                    self.snapshot.track_renames.insert(
                        cid,
                        TrackRename {
                            title: title.trim().to_string(),
                            artist: artist.trim().to_string(),
                            updated_at: Some(now_millis()),
                        },
                    );
                    self.snapshot.save();
                }
                Task::none()
            }
            Message::ModalTrimStartChanged(val) => {
                if let Some(ModalState::TrimTrack { start_input, .. }) = &mut self.modal {
                    *start_input = val;
                }
                Task::none()
            }
            Message::ModalTrimEndChanged(val) => {
                if let Some(ModalState::TrimTrack { end_input, .. }) = &mut self.modal {
                    *end_input = val;
                }
                Task::none()
            }
            Message::SubmitSaveTrim => {
                if let Some(ModalState::TrimTrack {
                    track,
                    start_input,
                    end_input,
                }) = self.modal.take()
                {
                    let parsed_start = start_input
                        .trim()
                        .parse::<f64>()
                        .ok()
                        .filter(|v| v.is_finite());
                    let parsed_end = end_input
                        .trim()
                        .parse::<f64>()
                        .ok()
                        .filter(|v| v.is_finite());
                    if let (Some(s), Some(e)) = (parsed_start, parsed_end) {
                        let start = s.max(0.0);
                        let end = e.max(0.0);
                        let trim = TrackTrim {
                            start,
                            end,
                            updated_at: Some(now_millis()),
                        };
                        let cid = track.canonical_id();
                        if trim.is_active() {
                            self.snapshot.track_trims.insert(cid, trim);
                        } else {
                            self.snapshot.track_trims.remove(&cid);
                        }
                        self.snapshot.save();
                    }
                }
                Task::none()
            }
            Message::SubmitResetTrim => {
                if let Some(ModalState::TrimTrack { track, .. }) = self.modal.take() {
                    self.snapshot.track_trims.remove(&track.canonical_id());
                    self.snapshot.save();
                }
                Task::none()
            }
            Message::AddTrackToPlaylist(pid, tr) => {
                self.modal = None;
                if let Some(pl) = self.snapshot.playlists.iter_mut().find(|p| p.id == pid) {
                    if !pl.tracks.iter().any(|existing| existing.matches(&tr)) {
                        let mut added = tr;
                        added.added_at = Some(now_millis());
                        pl.tracks.push(added);
                        self.snapshot.save();
                    }
                }
                Task::none()
            }
            Message::RemoveTrackFromPlaylist(pid, tr) => {
                if let Some(pl) = self.snapshot.playlists.iter_mut().find(|p| p.id == pid) {
                    if let Some(pos) = pl.tracks.iter().position(|existing| existing.matches(&tr)) {
                        pl.tracks.remove(pos);
                        self.snapshot.save();
                    }
                }
                Task::none()
            }
            Message::SelectSettingsTab(tab) => {
                self.settings_tab = tab;
                Task::none()
            }
            Message::SetLang(lang) => {
                self.lang = lang;
                self.snapshot.settings.ui_lang = Some(lang.as_str().to_string());
                self.snapshot.save();
                Task::none()
            }
            Message::SetThemeMode(mode) => {
                self.snapshot.settings.theme_mode = mode;
                self.snapshot.save();
                Task::none()
            }
            Message::ToggleHideTitlebar => {
                self.snapshot.settings.advanced.hide_titlebar =
                    !self.snapshot.settings.advanced.hide_titlebar;
                if !self.snapshot.settings.advanced.hide_titlebar {
                    self.titlebar_revealed = false;
                }
                self.snapshot.save();
                Task::none()
            }
            Message::ToggleAutohideTimestamps => {
                self.snapshot.settings.advanced.autohide_timestamps =
                    !self.snapshot.settings.advanced.autohide_timestamps;
                self.snapshot.save();
                Task::none()
            }
            Message::ToggleClassicSidebar => {
                self.snapshot.settings.advanced.classic_sidebar =
                    !self.snapshot.settings.advanced.classic_sidebar;
                self.snapshot.save();
                Task::none()
            }
            Message::ToggleAutoplay => {
                self.snapshot.settings.advanced.autoplay =
                    !self.snapshot.settings.advanced.autoplay;
                self.snapshot.save();
                Task::none()
            }
            Message::MoveSidebarItem(idx, up) => {
                let order = &mut self.snapshot.settings.sidebar_order;
                if up && idx > 0 && idx < order.len() {
                    order.swap(idx, idx - 1);
                    self.snapshot.save();
                } else if !up && idx + 1 < order.len() {
                    order.swap(idx, idx + 1);
                    self.snapshot.save();
                }
                Task::none()
            }
            Message::ResetSidebarOrder => {
                self.snapshot.settings.sidebar_order =
                    DEFAULT_SIDEBAR_ORDER.iter().map(|s| (*s).to_string()).collect();
                self.snapshot.save();
                Task::none()
            }
            Message::ClearCache => {
                storage::clear_cache();
                self.cache_stats = storage::get_cache_stats();
                Task::none()
            }
            Message::OpenCacheFolder => {
                let p = storage::get_cache_dir();
                let _ = std::fs::create_dir_all(&p);
                #[cfg(target_os = "windows")]
                {
                    let _ = std::process::Command::new("rundll32")
                        .args(["url.dll,FileProtocolHandler", &p.to_string_lossy()])
                        .spawn();
                }
                #[cfg(target_os = "linux")]
                {
                    let _ = std::process::Command::new("xdg-open").arg(&p).spawn();
                }
                #[cfg(target_os = "macos")]
                {
                    let _ = std::process::Command::new("open").arg(&p).spawn();
                }
                Task::none()
            }
            Message::SystemEvent(ev) => match ev {
                Event::Keyboard(keyboard::Event::KeyPressed { key, modifiers, .. }) => {
                    let modal_open = self.modal.is_some();
                    let input_focused = self.search_focused
                        || self.modal.as_ref().map(|m| m.has_text_input()).unwrap_or(false);
                    if let Some(action) =
                        resolve_shortcut(&key, modifiers, input_focused, modal_open)
                    {
                        self.execute_shortcut(action)
                    } else if !modal_open && matches!(key, Key::Named(Named::Tab)) {
                        operation::is_focused(views::search_input_id()).map(Message::SearchFocused)
                    } else {
                        Task::none()
                    }
                }
                Event::Mouse(mouse::Event::ButtonPressed(btn)) => {
                    if btn == mouse::Button::Back || btn == mouse::Button::Other(3) {
                        self.navigate_back();
                    }
                    if self.modal.is_none() && !self.focus_search_pending {
                        operation::is_focused(views::search_input_id()).map(Message::SearchFocused)
                    } else {
                        Task::none()
                    }
                }
                _ => Task::none(),
            },
        }
    }

    pub fn subscription(&self) -> Subscription<Message> {
        let interval = if self.audio_state.playing || self.audio_state.loading {
            Duration::from_millis(55)
        } else {
            Duration::from_millis(280)
        };
        let tick_sub = iced::time::every(interval).map(|_| Message::Tick);
        let events_sub = iced::event::listen().map(Message::SystemEvent);
        Subscription::batch([tick_sub, events_sub])
    }

    pub fn view(&self) -> Element<'_, Message> {
        let p = self.palette();

        let base_shell: Element<'_, Message> = if self.is_mini_player {
            column![
                titlebar::view_titlebar(self, p),
                playbar::view_mini_player(self, p),
            ]
            .into()
        } else if self.snapshot.settings.advanced.classic_sidebar {
            // Classic Floating Sidebar Layout (ui/styles.css:11010-11038)
            column![
                titlebar::view_titlebar(self, p),
                views::view_top_nav(self, p),
                row![
                    sidebar::view_sidebar(self, p),
                    views::view_main_content(self, p),
                ]
                .spacing(16)
                .padding([0, 16])
                .height(Length::Fill),
                container(playbar::view_playbar(self, p)).padding(Padding {
                    top: 12.0,
                    right: 16.0,
                    bottom: 16.0,
                    left: 16.0,
                }),
            ]
            .into()
        } else {
            // Default Connected-Rail Desktop Layout (ui/styles.css:10814-11008)
            let top_body = row![
                sidebar::view_sidebar(self, p),
                column![
                    titlebar::view_titlebar(self, p),
                    views::view_top_nav(self, p),
                    views::view_main_content(self, p),
                ]
                .width(Length::Fill)
                .height(Length::Fill),
            ]
            .width(Length::Fill)
            .height(Length::Fill);

            column![top_body, playbar::view_playbar(self, p)]
                .width(Length::Fill)
                .height(Length::Fill)
                .into()
        };

        let root_bg = container(base_shell)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(move |_| container::Style {
                background: Some(Background::Color(p.page_bg)),
                ..Default::default()
            });

        if let Some(ref modal) = self.modal {
            stack![root_bg, views::view_modal(self, p, modal)]
                .width(Length::Fill)
                .height(Length::Fill)
                .into()
        } else {
            let _ = (CORNER_XL, SIDEBAR_CLASSIC_WIDTH);
            root_bg.into()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_track(id: &str, title: &str, added: u64) -> TrackRecord {
        TrackRecord {
            id: id.to_string(),
            title: title.to_string(),
            artist: "Artist".to_string(),
            platform: "local".to_string(),
            added_at: Some(added),
            ..Default::default()
        }
    }

    #[test]
    fn test_track_sort_and_pinned_float() {
        let t1 = make_track("1", "Alpha", 100);
        let t2 = make_track("2", "Beta", 300);
        let t3 = make_track("3", "Gamma", 200);
        let tracks = vec![t1.clone(), t2.clone(), t3.clone()];
        let pinned = vec![t1.canonical_id()];

        let desc = sort_tracks_with_pins(
            &tracks,
            TrackSortMode::AddedDesc,
            &pinned,
            "",
            &HashMap::new(),
        );
        assert_eq!(desc.iter().map(|t| t.id.as_str()).collect::<Vec<_>>(), vec!["1", "2", "3"]);

        let asc = sort_tracks_with_pins(
            &tracks,
            TrackSortMode::AddedAsc,
            &pinned,
            "",
            &HashMap::new(),
        );
        assert_eq!(asc.iter().map(|t| t.id.as_str()).collect::<Vec<_>>(), vec!["1", "3", "2"]);

        let manual = sort_tracks_with_pins(
            &tracks,
            TrackSortMode::Manual,
            &[t3.canonical_id()],
            "",
            &HashMap::new(),
        );
        assert_eq!(manual.iter().map(|t| t.id.as_str()).collect::<Vec<_>>(), vec!["3", "1", "2"]);
    }

    #[test]
    fn test_track_trim_bounds_and_enforcement() {
        let trim = TrackTrim {
            start: 10.0,
            end: 25.0,
            updated_at: None,
        };
        assert!(trim.is_active());
        assert_eq!(evaluate_trim_jump(&trim, 2.0), Some(10.0));
        assert_eq!(evaluate_trim_jump(&trim, 9.8), None);
        assert!(!evaluate_trim_end(&trim, 24.5));
        assert!(evaluate_trim_end(&trim, 25.0));

        let inactive = TrackTrim {
            start: 10.0,
            end: 11.0,
            updated_at: None,
        };
        assert!(!inactive.is_active());
        assert_eq!(evaluate_trim_jump(&inactive, 0.0), None);
        assert!(!evaluate_trim_end(&inactive, 20.0));
    }

    #[test]
    fn test_keyboard_shortcut_suppression_when_input_focused() {
        assert_eq!(
            resolve_shortcut(&Key::Named(Named::Space), Modifiers::empty(), false, false),
            Some(ShortcutAction::TogglePlayPause)
        );
        assert_eq!(
            resolve_shortcut(&Key::Named(Named::Space), Modifiers::empty(), true, false),
            None
        );
        assert_eq!(
            resolve_shortcut(&Key::Character("m".into()), Modifiers::empty(), true, false),
            None
        );
        assert_eq!(
            resolve_shortcut(&Key::Named(Named::ArrowRight), Modifiers::CTRL, true, false),
            None
        );
        assert_eq!(
            resolve_shortcut(&Key::Character("k".into()), Modifiers::CTRL, true, false),
            Some(ShortcutAction::FocusSearch)
        );
        assert_eq!(
            resolve_shortcut(&Key::Named(Named::Escape), Modifiers::empty(), true, true),
            Some(ShortcutAction::EscapeUnwind)
        );
        assert_eq!(
            resolve_shortcut(&Key::Named(Named::Space), Modifiers::empty(), false, true),
            None
        );
    }

    #[test]
    fn test_cli_view_flag_parsing() {
        let opts1 = parse_cli_args(["raser", "--view", "favorites"]);
        assert_eq!(opts1.initial_view, Some(ActiveView::Favorites));
        assert!(!opts1.show_help);

        let opts2 = parse_cli_args(["raser", "--view=settings", "--headless-check"]);
        assert_eq!(opts2.initial_view, Some(ActiveView::Settings));
        assert!(opts2.headless_check);

        let opts3 = parse_cli_args(["raser", "--view", "unknown-view"]);
        assert_eq!(opts3.initial_view, None);

        let opts4 = parse_cli_args(["raser", "--help"]);
        assert!(opts4.show_help);
    }

    static TEST_ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    struct AppTestDirGuard {
        dir: std::path::PathBuf,
        prev: Option<String>,
    }

    impl AppTestDirGuard {
        fn new(prefix: &str) -> Self {
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let dir = std::env::temp_dir().join(format!("raser_app_test_{}_{}", prefix, nanos));
            std::fs::create_dir_all(&dir).unwrap();
            let prev = std::env::var("RASER_DATA_DIR").ok();
            unsafe {
                std::env::set_var("RASER_DATA_DIR", &dir);
            }
            Self { dir, prev }
        }
    }

    impl Drop for AppTestDirGuard {
        fn drop(&mut self) {
            unsafe {
                match &self.prev {
                    Some(v) => std::env::set_var("RASER_DATA_DIR", v),
                    None => std::env::remove_var("RASER_DATA_DIR"),
                }
            }
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    fn make_playable_track(id: &str, title: &str) -> TrackRecord {
        TrackRecord {
            id: id.to_string(),
            title: title.to_string(),
            artist: "Artist".to_string(),
            platform: "local".to_string(),
            url: format!("C:\\Music\\{}.mp3", id),
            path: format!("C:\\Music\\{}.mp3", id),
            duration: 180.0,
            ..Default::default()
        }
    }

    #[test]
    fn test_queue_history_remap_active_removal_and_clear() {
        let _lock = TEST_ENV_LOCK.lock().unwrap();
        let _guard = AppTestDirGuard::new("queue_history");
        let (mut app, _) = RaserApp::boot(None);

        let t1 = make_playable_track("t1", "One");
        let t2 = make_playable_track("t2", "Two");
        let t3 = make_playable_track("t3", "Three");

        let _ = app.update(Message::PlayTracks(vec![t1.clone(), t2.clone(), t3.clone()], 2));
        let _ = app.update(Message::JumpToQueueIndex(0));
        let _ = app.update(Message::JumpToQueueIndex(1));
        assert_eq!(app.play_history, vec![2, 0]);
        assert_eq!(app.queue_index, Some(1));

        // Remove index 2 (which is in play_history); play_history must prune 2 -> [0]
        let _ = app.update(Message::RemoveFromQueue(2));
        assert_eq!(app.queue.len(), 2);
        assert_eq!(app.play_history, vec![0]);

        let _ = app.update(Message::PrevTrack);
        assert_eq!(app.queue_index, Some(0));
        assert!(app.current_track().is_some());

        let _ = app.update(Message::PrevTrack);
        assert!(app.queue_index.map(|i| i < app.queue.len()).unwrap_or(false));
        assert!(app.current_track().is_some());

        // Removing active track resets/syncs audio state rather than leaving stale playing flag
        app.audio_state.playing = true;
        app.audio_state.duration = 999.0;
        let _ = app.update(Message::RemoveFromQueue(0));
        assert_eq!(app.queue.len(), 1);
        assert_eq!(app.queue_index, Some(0));
        assert!(!app.audio_state.playing);
        assert_eq!(app.audio_state.duration, 0.0);

        // ClearQueue must clear queue, queue_index, play_history, future_history, and audio state
        app.play_history.push(0);
        app.future_history.push(0);
        let _ = app.update(Message::ClearQueue);
        assert!(app.queue.is_empty());
        assert_eq!(app.queue_index, None);
        assert!(app.play_history.is_empty());
        assert!(app.future_history.is_empty());
        assert!(!app.audio_state.playing);
    }

    #[test]
    fn test_submit_save_trim_rejects_non_finite() {
        let _lock = TEST_ENV_LOCK.lock().unwrap();
        let _guard = AppTestDirGuard::new("trim_nonfinite");
        let (mut app, _) = RaserApp::boot(None);
        let tr = make_playable_track("trim_1", "TrimSong");
        let cid = tr.canonical_id();

        for bad in ["inf", "-inf", "NaN", "not_a_number"] {
            let _ = app.update(Message::OpenTrimTrackModal(tr.clone()));
            let _ = app.update(Message::ModalTrimStartChanged("0.0".to_string()));
            let _ = app.update(Message::ModalTrimEndChanged(bad.to_string()));
            let _ = app.update(Message::SubmitSaveTrim);
            assert!(
                !app.snapshot.track_trims.contains_key(&cid),
                "non-finite/invalid trim '{}' must not be saved",
                bad
            );
        }

        // Valid finite trim saves cleanly
        let _ = app.update(Message::OpenTrimTrackModal(tr.clone()));
        let _ = app.update(Message::ModalTrimStartChanged("5.0".to_string()));
        let _ = app.update(Message::ModalTrimEndChanged("25.0".to_string()));
        let _ = app.update(Message::SubmitSaveTrim);
        let saved = app.snapshot.track_trims.get(&cid).expect("valid trim saved");
        assert!(saved.is_active());
        assert_eq!(saved.start, 5.0);
        assert_eq!(saved.end, 25.0);

        // Non-finite input must not overwrite an existing valid trim either
        let _ = app.update(Message::OpenTrimTrackModal(tr.clone()));
        let _ = app.update(Message::ModalTrimStartChanged("NaN".to_string()));
        let _ = app.update(Message::ModalTrimEndChanged("inf".to_string()));
        let _ = app.update(Message::SubmitSaveTrim);
        assert_eq!(app.snapshot.track_trims.get(&cid).unwrap().end, 25.0);
    }

    #[test]
    fn test_repeat_one_auto_restart_bumps_load_epoch_and_seek_coordinator() {
        let _lock = TEST_ENV_LOCK.lock().unwrap();
        let _guard = AppTestDirGuard::new("repeat_one_epoch");
        let (mut app, _) = RaserApp::boot(None);
        let tr = make_playable_track("rep1", "LoopSong");

        let _ = app.update(Message::PlayTracks(vec![tr], 0));
        app.snapshot.settings.repeat_mode = RepeatMode::One;
        app.seek_coordinator.finish_load(0.0);
        app.last_ended_reported = false;
        let epoch_before = app.load_epoch;
        let seek_epoch_before = app.seek_coordinator.seek_epoch;

        app.audio.force_ended_for_test();
        let _ = app.update(Message::Tick);

        assert_eq!(app.load_epoch, epoch_before.wrapping_add(1));
        assert!(app.seek_coordinator.is_loading);
        assert_eq!(app.seek_coordinator.seek_epoch, seek_epoch_before.wrapping_add(1));
        assert!(!app.last_ended_reported);
    }

    #[test]
    fn test_toggle_pin_track_unpins_legacy_bare_id() {
        let _lock = TEST_ENV_LOCK.lock().unwrap();
        let _guard = AppTestDirGuard::new("legacy_pin");
        let (mut app, _) = RaserApp::boot(None);
        let tr = make_playable_track("bare_99", "PinnedLegacy");

        app.snapshot.settings.pinned_tracks = vec!["bare_99".to_string()];
        assert!(app.is_pinned(&tr));

        // Unpinning a legacy bare ID must remove it rather than pushing canonical_id
        let _ = app.update(Message::TogglePinTrack(tr.clone()));
        assert!(!app.is_pinned(&tr));
        assert!(app.snapshot.settings.pinned_tracks.is_empty());

        // Pinning again uses canonical_id and unpins cleanly
        let _ = app.update(Message::TogglePinTrack(tr.clone()));
        assert!(app.is_pinned(&tr));
        let _ = app.update(Message::TogglePinTrack(tr.clone()));
        assert!(!app.is_pinned(&tr));
    }

    #[test]
    fn test_escape_and_view_switch_clear_search_focused() {
        let _lock = TEST_ENV_LOCK.lock().unwrap();
        let _guard = AppTestDirGuard::new("search_focus");
        let (mut app, _) = RaserApp::boot(None);

        let _ = app.update(Message::FocusSearchBox);
        assert!(app.search_focused);

        let _ = app.update(Message::SystemEvent(Event::Keyboard(
            keyboard::Event::KeyPressed {
                key: Key::Named(Named::Escape),
                modified_key: Key::Named(Named::Escape),
                physical_key: keyboard::key::Physical::Code(keyboard::key::Code::Escape),
                location: keyboard::Location::Standard,
                modifiers: Modifiers::empty(),
                text: None,
                repeat: false,
            },
        )));
        assert!(!app.search_focused);

        let _ = app.update(Message::FocusSearchBox);
        assert!(app.search_focused);
        let _ = app.update(Message::SwitchView(ActiveView::Favorites));
        assert!(!app.search_focused);
    }

    fn build_app_test_wav(path: &std::path::Path, duration_secs: u32) {
        let sample_rate = 44100u32;
        let num_samples = sample_rate * duration_secs;
        let data_size = num_samples * 2;
        let mut wav = Vec::with_capacity(44 + data_size as usize);
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&(36 + data_size).to_le_bytes());
        wav.extend_from_slice(b"WAVEfmt ");
        wav.extend_from_slice(&16u32.to_le_bytes());
        wav.extend_from_slice(&1u16.to_le_bytes());
        wav.extend_from_slice(&1u16.to_le_bytes());
        wav.extend_from_slice(&sample_rate.to_le_bytes());
        wav.extend_from_slice(&(sample_rate * 2).to_le_bytes());
        wav.extend_from_slice(&2u16.to_le_bytes());
        wav.extend_from_slice(&16u16.to_le_bytes());
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&data_size.to_le_bytes());
        for i in 0..num_samples {
            let t = i as f32 / sample_rate as f32;
            let sample = ((2.0 * std::f32::consts::PI * 440.0 * t).sin() * 8000.0) as i16;
            wav.extend_from_slice(&sample.to_le_bytes());
        }
        std::fs::write(path, &wav).unwrap();
    }

    #[test]
    fn test_remove_paused_active_track_does_not_auto_advance_on_tick() {
        let _lock = TEST_ENV_LOCK.lock().unwrap();
        let guard = AppTestDirGuard::new("remove_paused_tick");
        let wav_path = guard.dir.join("track_a.wav");
        build_app_test_wav(&wav_path, 3);

        let mut track_a = make_playable_track("A", "Track A");
        track_a.path = wav_path.to_string_lossy().to_string();
        track_a.url = track_a.path.clone();
        track_a.duration = 3.0;
        let track_b = make_playable_track("B", "Track B");
        let track_c = make_playable_track("C", "Track C");

        let (mut app, _) = RaserApp::boot(None);
        let _ = app.audio.set_volume(0.0);
        app.queue = vec![track_a.clone(), track_b, track_c];
        app.queue_index = Some(0);

        let rt = tokio::runtime::Runtime::new().unwrap();
        let load_res = rt.block_on(app.audio.load_audio_async(
            track_a.playable_source(),
            None,
            Some(3.0),
            true,
            Some(track_a.id.clone()),
        ));

        match load_res {
            Ok(_) => {
                for _ in 0..24 {
                    if app.audio.get_state().position > 0.16 {
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(25));
                }
                app.audio_state = app.audio.get_state();
                assert!(
                    app.audio_state.position > 0.15,
                    "expected >0.15s playback before pause, got {}",
                    app.audio_state.position
                );

                // Pause via TogglePlayPause
                let _ = app.update(Message::TogglePlayPause);
                assert!(!app.audio_state.playing);

                // Remove paused active track "A" and run Tick
                let _ = app.update(Message::RemoveFromQueue(0));
                let _ = app.update(Message::Tick);

                assert_eq!(app.queue_index, Some(0));
                assert_eq!(
                    app.current_track().as_ref().map(|t| t.id.as_str()),
                    Some("B")
                );
                assert!(!app.seek_coordinator.is_loading);
                assert!(!app.audio_state.ended);
                assert_eq!(app.audio_state.position, 0.0);
            }
            Err(e) if e.contains("default audio sink") => {
                // Headless CI without audio device: still verify RemoveFromQueue + Tick doesn't advance
                let _ = app.update(Message::RemoveFromQueue(0));
                let _ = app.update(Message::Tick);
                assert_eq!(app.queue_index, Some(0));
                assert_eq!(
                    app.current_track().as_ref().map(|t| t.id.as_str()),
                    Some("B")
                );
                assert!(!app.seek_coordinator.is_loading);
                assert!(!app.audio_state.ended);
            }
            Err(e) => panic!("Unexpected load_audio_async error: {}", e),
        }
    }

    #[test]
    fn test_toggle_play_pause_loads_unloaded_queue_track_on_boot_and_paused_remove() {
        let _lock = TEST_ENV_LOCK.lock().unwrap();
        let guard = AppTestDirGuard::new("toggle_play_unloaded");
        let wav_path = guard.dir.join("boot_track.wav");
        build_app_test_wav(&wav_path, 2);

        let mut track_a = make_playable_track("A", "Track A");
        track_a.path = wav_path.to_string_lossy().to_string();
        track_a.url = track_a.path.clone();
        track_a.duration = 2.0;
        let track_b = make_playable_track("B", "Track B");

        // (a) Cold boot with persisted devsize_queue (url.is_none())
        let storage_json = serde_json::json!({
            "devsize_queue": serde_json::to_string(&vec![&track_a, &track_b]).unwrap()
        });
        std::fs::write(
            guard.dir.join("storage.json"),
            serde_json::to_string(&storage_json).unwrap(),
        )
        .unwrap();

        let (mut app, _) = RaserApp::boot(None);
        assert_eq!(app.queue_index, Some(0));
        assert!(app.audio_state.url.is_none());
        assert!(!app.seek_coordinator.is_loading);

        let _ = app.update(Message::TogglePlayPause);
        assert!(app.seek_coordinator.is_loading);
        assert!(!app.audio_state.playing);

        // Second TogglePlayPause while still loading (url.is_none()) must not corrupt playing
        let _ = app.update(Message::TogglePlayPause);
        assert!(app.seek_coordinator.is_loading);
        assert!(!app.audio_state.playing);

        // (b) Paused RemoveFromQueue(0) then TogglePlayPause initiates load for Track B
        let (mut app2, _) = RaserApp::boot(None);
        let _ = app2.audio.set_volume(0.0);
        app2.queue = vec![track_a.clone(), track_b];
        app2.queue_index = Some(0);

        let rt = tokio::runtime::Runtime::new().unwrap();
        let _ = rt.block_on(app2.audio.load_audio_async(
            track_a.playable_source(),
            None,
            Some(2.0),
            false,
            Some(track_a.id.clone()),
        ));
        app2.audio_state = app2.audio.get_state();
        assert!(!app2.audio_state.playing);

        let _ = app2.update(Message::RemoveFromQueue(0));
        assert_eq!(app2.queue_index, Some(0));
        assert_eq!(
            app2.current_track().as_ref().map(|t| t.id.as_str()),
            Some("B")
        );
        assert!(app2.audio_state.url.is_none());
        assert!(!app2.seek_coordinator.is_loading);

        let _ = app2.update(Message::TogglePlayPause);
        assert!(app2.seek_coordinator.is_loading);
        assert!(!app2.audio_state.playing);
    }

    #[test]
    fn test_remove_local_track_records_legacy_extra_filepath() {
        let _lock = TEST_ENV_LOCK.lock().unwrap();
        let _guard = AppTestDirGuard::new("remove_local_filepath");
        let (mut app, _) = RaserApp::boot(None);

        let mut extra = HashMap::new();
        extra.insert(
            "filePath".to_string(),
            serde_json::Value::String(r"C:\Music\Legacy\Song.mp3".to_string()),
        );
        let legacy_track = TrackRecord {
            id: "legacy_fp_1".to_string(),
            title: "Legacy Song".to_string(),
            artist: "Legacy Artist".to_string(),
            platform: "local".to_string(),
            path: String::new(),
            url: String::new(),
            extra,
            ..Default::default()
        };

        app.snapshot.local_tracks = vec![legacy_track.clone()];
        app.snapshot.save();

        let _ = app.update(Message::RemoveLocalTrack(legacy_track));
        assert!(app.snapshot.local_tracks.is_empty());
        assert!(app
            .snapshot
            .removed_local_paths
            .contains(&r"C:\Music\Legacy\Song.mp3".to_string()));

        // Ensure reloaded snapshot keeps the legacy track filtered out
        let reloaded = AppStateSnapshot::load();
        assert!(reloaded.local_tracks.is_empty());
        assert!(reloaded
            .removed_local_paths
            .contains(&r"C:\Music\Legacy\Song.mp3".to_string()));
    }

    #[test]
    fn test_focus_search_box_ignores_stale_system_event_unfocus() {
        let _lock = TEST_ENV_LOCK.lock().unwrap();
        let _guard = AppTestDirGuard::new("search_focus_race");
        let (mut app, _) = RaserApp::boot(None);

        let _ = app.update(Message::FocusSearchBox);
        assert!(app.search_focused);
        assert!(app.focus_search_pending);

        // Stale SearchFocused(false) racing on the same click must not clear search_focused
        let _ = app.update(Message::SearchFocused(false));
        assert!(app.search_focused);

        // Once FocusSearchBox's chained focus check resolves, pending flag clears
        let _ = app.update(Message::SearchFocusResolved(true));
        assert!(app.search_focused);
        assert!(!app.focus_search_pending);

        // Subsequent click outside can now unfocus normally
        let _ = app.update(Message::SearchFocused(false));
        assert!(!app.search_focused);
    }
}
