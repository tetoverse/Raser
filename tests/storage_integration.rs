use app_lib::storage::{
    clear_cache, get_audio_cache_dir, get_cache_dir, get_cache_stats, get_cached_audio_path,
    get_cached_lyrics, get_storage_path, save_cached_audio, save_cached_lyrics, AppStateSnapshot,
    RepeatMode, TrackSortMode,
};
use serde_json::json;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

static ENV_LOCK: Mutex<()> = Mutex::new(());

struct TempDataDirGuard {
    dir: PathBuf,
    prev: Option<String>,
}

impl TempDataDirGuard {
    fn new(prefix: &str) -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("raser_test_{}_{}", prefix, nanos));
        fs::create_dir_all(&dir).unwrap();
        let prev = std::env::var("RASER_DATA_DIR").ok();
        unsafe {
            std::env::set_var("RASER_DATA_DIR", &dir);
        }
        Self { dir, prev }
    }
}

impl Drop for TempDataDirGuard {
    fn drop(&mut self) {
        unsafe {
            match &self.prev {
                Some(v) => std::env::set_var("RASER_DATA_DIR", v),
                None => std::env::remove_var("RASER_DATA_DIR"),
            }
        }
        let _ = fs::remove_dir_all(&self.dir);
    }
}

#[test]
fn test_raser_data_dir_isolation() {
    let _lock = ENV_LOCK.lock().unwrap();
    let guard = TempDataDirGuard::new("isolation");

    assert_eq!(get_storage_path(), guard.dir.join("storage.json"));
    assert_eq!(get_cache_dir(), guard.dir.join("cache"));
    assert_eq!(get_audio_cache_dir(), guard.dir.join("cache").join("audio"));
}

#[test]
fn test_legacy_v126_stringified_storage_load() {
    let _lock = ENV_LOCK.lock().unwrap();
    let guard = TempDataDirGuard::new("legacy_v126");

    let playlists_str = json!([
        {
            "id": "pl_1",
            "name": "Night Drive",
            "description": "Synthwave",
            "tracks": [
                {
                    "id": "local_abc",
                    "title": "Midnight",
                    "artist": "Klayton",
                    "platform": "local",
                    "url": "http://asset.localhost/C%3A%2FMusic%2FMidnight.mp3",
                    "addedAt": 1700000000u64
                }
            ]
        }
    ])
    .to_string();

    let favorites_str = json!([
        {
            "id": "yt_123",
            "title": "Starlight",
            "artist": "Muse",
            "duration": 240.0,
            "platform": "ytmusic",
            "url": "https://music.youtube.com/watch?v=yt_123",
            "addedAt": 1700000100u64
        }
    ])
    .to_string();

    let history_str = json!([
        {
            "id": "yt_123",
            "title": "Starlight",
            "artist": "Muse",
            "platform": "ytmusic",
            "playedAt": 1700000200u64
        }
    ])
    .to_string();

    let local_tracks_str = json!([
        {
            "id": "local_1",
            "title": "One",
            "artist": "Artist",
            "path": "C:\\Music\\One.mp3",
            "url": "C:\\Music\\One.mp3",
            "platform": "local"
        },
        {
            "id": "local_2",
            "title": "Removed",
            "artist": "Artist",
            "path": "C:\\Music\\Removed.mp3",
            "url": "C:\\Music\\Removed.mp3",
            "platform": "local"
        }
    ])
    .to_string();

    let removed_str = json!(["C:\\Music\\Removed.mp3"]).to_string();
    let saved_albums_str = json!([{"title": "Absolution", "artist": "Muse"}]).to_string();
    let renames_str = json!({
        "ytmusic:yt_123": {"title": "Starlight (Live)", "artist": "Muse", "updatedAt": 1700000300u64}
    })
    .to_string();
    let trims_str = json!({
        "ytmusic:yt_123": {"start": 12.5, "end": 45.0, "updatedAt": 1700000400u64}
    })
    .to_string();
    let art_str = json!({
        "ytmusic:yt_123": {"cover": "https://example.com/c.jpg", "banner": "https://example.com/b.jpg"}
    })
    .to_string();

    let storage_obj = json!({
        "devsize_playlists": playlists_str,
        "devsize_favorites": favorites_str,
        "devsize_history": history_str,
        "devsize_local_tracks": local_tracks_str,
        "riff_removed_local_paths": removed_str,
        "riff_saved_albums": saved_albums_str,
        "riff_track_renames": renames_str,
        "riff_track_trims": trims_str,
        "riff_track_art": art_str,
        "riff_volume": "0.65",
        "riff_muted": "1",
        "riff_shuffle": "1",
        "raser_repeat_mode": "one",
        "riff_ui_lang": "tr",
        "riff_sidebar_order": "[\"favorites\",\"discover\",\"local\"]",
        "riff_favorites_sort": "added-desc",
        "riff_playlist_sort": "added-asc",
        "riff_pinned_tracks": "[\"ytmusic:yt_123\"]",
        "riff_adv_hide_titlebar": "true",
        "riff_adv_autohide_timestamps": "false",
        "raser_firebase_uid": "uid_preserve_me"
    });

    fs::write(
        guard.dir.join("storage.json"),
        serde_json::to_string_pretty(&storage_obj).unwrap(),
    )
    .unwrap();

    let mut snap = AppStateSnapshot::load();
    assert_eq!(snap.playlists.len(), 1);
    assert_eq!(snap.playlists[0].name, "Night Drive");
    assert_eq!(snap.playlists[0].tracks.len(), 1);
    assert_eq!(snap.favorites.len(), 1);
    assert_eq!(snap.history.len(), 1);
    assert_eq!(snap.local_tracks.len(), 1);
    assert_eq!(snap.local_tracks[0].title, "One");
    assert_eq!(snap.saved_albums.len(), 1);
    assert_eq!(
        snap.track_renames["ytmusic:yt_123"].title,
        "Starlight (Live)"
    );
    assert_eq!(snap.track_trims["ytmusic:yt_123"].start, 12.5);
    assert_eq!(snap.track_trims["ytmusic:yt_123"].end, 45.0);
    assert_eq!(
        snap.track_art["ytmusic:yt_123"].cover.as_deref(),
        Some("https://example.com/c.jpg")
    );
    assert!((snap.settings.volume - 0.65).abs() < 0.001);
    assert!(snap.settings.muted);
    assert!(snap.settings.shuffle);
    assert_eq!(snap.settings.repeat_mode, RepeatMode::One);
    assert_eq!(snap.settings.ui_lang.as_deref(), Some("tr"));
    assert_eq!(snap.settings.favorites_sort, TrackSortMode::AddedDesc);
    assert_eq!(snap.settings.playlist_sort, TrackSortMode::AddedAsc);
    assert_eq!(snap.settings.pinned_tracks, vec!["ytmusic:yt_123"]);
    assert!(snap.settings.advanced.hide_titlebar);
    assert!(!snap.settings.advanced.autohide_timestamps);
    assert_eq!(snap.settings.sidebar_order[0], "favorites");
    assert_eq!(snap.settings.sidebar_order[1], "discover");

    // Save and confirm unknown keys & typed changes round-trip cleanly
    snap.settings.volume = 0.90;
    snap.save();

    let reloaded = AppStateSnapshot::load();
    assert!((reloaded.settings.volume - 0.90).abs() < 0.001);
    assert_eq!(
        reloaded.raw_map.get("raser_firebase_uid"),
        Some(&json!("uid_preserve_me"))
    );
    assert_eq!(reloaded.playlists.len(), 1);
}

#[test]
fn test_native_json_storage_load_and_roundtrip() {
    let _lock = ENV_LOCK.lock().unwrap();
    let _guard = TempDataDirGuard::new("native_json");

    let mut raw: HashMap<String, serde_json::Value> = HashMap::new();
    raw.insert(
        "devsize_playlists".into(),
        json!([{"id": "p1", "name": "Native", "tracks": []}]),
    );
    raw.insert("riff_volume".into(), json!(0.42));
    raw.insert("riff_muted".into(), json!(false));
    raw.insert("riff_shuffle".into(), json!(true));
    raw.insert("riff_repeat".into(), json!("all"));
    raw.insert("riff_adv_classic_sidebar".into(), json!(true));

    let snap = AppStateSnapshot::from_storage_map(raw);
    assert_eq!(snap.playlists.len(), 1);
    assert_eq!(snap.playlists[0].name, "Native");
    assert!((snap.settings.volume - 0.42).abs() < 0.001);
    assert!(!snap.settings.muted);
    assert!(snap.settings.shuffle);
    assert_eq!(snap.settings.repeat_mode, RepeatMode::All);
    assert!(snap.settings.advanced.classic_sidebar);
}

#[test]
fn test_cache_stats_and_clear() {
    let _lock = ENV_LOCK.lock().unwrap();
    let _guard = TempDataDirGuard::new("cache_ops");

    // <= 1024 bytes is rejected by get_cached_audio_path
    save_cached_audio("tiny_track", &[1u8; 512]);
    assert!(get_cached_audio_path("tiny_track").is_none());

    // > 1024 bytes is valid cached audio
    save_cached_audio("valid_track/id", &[7u8; 2048]);
    assert!(get_cached_audio_path("valid_track/id").is_some());

    save_cached_lyrics("valid_track/id", r#"{"lrc":"[00:01.00]Hello"}"#);
    assert!(get_cached_lyrics("valid_track/id").is_some());

    let (bytes, count) = get_cache_stats();
    assert!(bytes >= 2048 + 512);
    assert_eq!(count, 3);

    clear_cache();
    let (bytes_after, count_after) = get_cache_stats();
    assert_eq!(bytes_after, 0);
    assert_eq!(count_after, 0);
}
