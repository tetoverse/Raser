use app_lib::storage::{
    clear_cache, get_audio_cache_dir, get_cache_dir, get_cache_stats, get_cached_audio_path,
    get_cached_lyrics, get_storage_path, save_cached_audio, save_cached_lyrics, AppStateSnapshot,
    RepeatMode, TrackRecord, TrackSortMode,
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

#[test]
fn test_legacy_null_fields_and_per_element_resilience_roundtrip() {
    let _lock = ENV_LOCK.lock().unwrap();
    let guard = TempDataDirGuard::new("null_fields");

    let playlists_str = json!([
        {
            "id": "pl_1",
            "name": "Keep Me",
            "description": null,
            "cover": null,
            "sort_mode": null,
            "tracks": [
                {
                    "id": "1",
                    "title": "Valid",
                    "artist": "A",
                    "album": null,
                    "duration": 120.0,
                    "thumbnail": null,
                    "cover": null,
                    "path": null,
                    "url": "C:\\Music\\Valid.mp3",
                    "platform": "local",
                    "addedAt": null
                }
            ]
        },
        {
            "id": "pl_2",
            "name": "With Null",
            "tracks": [
                {
                    "id": "2",
                    "title": null,
                    "artist": null,
                    "album": null,
                    "duration": null,
                    "platform": "ytmusic"
                },
                "unparseable_track_entry",
                null
            ]
        },
        {
            "id": "pl_null_tracks",
            "name": "Null Tracks",
            "tracks": null
        },
        "corrupt_playlist_entry",
        12345
    ])
    .to_string();

    let favorites_str = json!([
        {
            "id": "fav_1",
            "title": "Fav",
            "artist": null,
            "album": null,
            "duration": null,
            "platform": "ytmusic"
        },
        null,
        "bad_fav"
    ])
    .to_string();

    let saved_albums_str = json!([
        {
            "id": "alb_1",
            "title": "Album One",
            "artist": null,
            "cover": null,
            "year": 2024,
            "trackCount": null
        },
        "bad_album"
    ])
    .to_string();

    let renames_str = json!({
        "ytmusic:fav_1": {"title": "Renamed", "artist": null, "updatedAt": null},
        "bad_entry": "not_an_object"
    })
    .to_string();

    let trims_str = json!({
        "ytmusic:fav_1": {"start": null, "end": 45.0, "updatedAt": null},
        "bad_trim": 42
    })
    .to_string();

    let art_str = json!({
        "ytmusic:fav_1": {"cover": null, "banner": "https://example.com/b.jpg"},
        "bad_art": []
    })
    .to_string();

    let storage_obj = json!({
        "devsize_playlists": playlists_str,
        "devsize_favorites": favorites_str,
        "riff_saved_albums": saved_albums_str,
        "riff_track_renames": renames_str,
        "riff_track_trims": trims_str,
        "riff_track_art": art_str
    });

    fs::write(
        guard.dir.join("storage.json"),
        serde_json::to_string_pretty(&storage_obj).unwrap(),
    )
    .unwrap();

    let mut snap = AppStateSnapshot::load();
    assert_eq!(snap.playlists.len(), 3);
    assert_eq!(snap.playlists[0].name, "Keep Me");
    assert_eq!(snap.playlists[0].tracks.len(), 1);
    assert_eq!(snap.playlists[1].name, "With Null");
    assert_eq!(snap.playlists[1].tracks.len(), 1);
    assert_eq!(snap.playlists[1].tracks[0].duration, 0.0);
    assert_eq!(snap.playlists[2].tracks.len(), 0);

    assert_eq!(snap.favorites.len(), 1);
    assert_eq!(snap.favorites[0].title, "Fav");
    assert_eq!(snap.saved_albums.len(), 1);
    assert_eq!(snap.saved_albums[0].year.as_deref(), Some("2024"));
    assert_eq!(snap.track_renames.len(), 1);
    assert_eq!(snap.track_trims["ytmusic:fav_1"].end, 45.0);
    assert_eq!(
        snap.track_art["ytmusic:fav_1"].banner.as_deref(),
        Some("https://example.com/b.jpg")
    );

    snap.save();
    let reloaded = AppStateSnapshot::load();
    assert_eq!(reloaded.playlists.len(), 3);
    assert_eq!(reloaded.playlists[1].tracks.len(), 1);
    assert_eq!(reloaded.favorites.len(), 1);
    assert_eq!(reloaded.saved_albums.len(), 1);
}

#[test]
fn test_local_tracks_nonempty_precedence_and_removed_matching() {
    let _lock = ENV_LOCK.lock().unwrap();
    let _guard = TempDataDirGuard::new("local_precedence");

    let mut raw: HashMap<String, serde_json::Value> = HashMap::new();
    raw.insert("devsize_local_tracks".into(), json!("[]"));
    raw.insert(
        "riff_local_tracks".into(),
        json!([
            {"id": "loc1", "title": "Keep", "path": "C:\\Music\\Keep.mp3", "platform": "local"},
            {"id": "loc_fp", "title": "Removed By FilePath", "filePath": "C:\\Music\\GoneFP.mp3", "platform": "local"},
            {"id": "loc_id_removed", "title": "Removed By Id", "path": "C:\\Music\\GoneId.mp3", "platform": "local"}
        ])
        .to_string()
        .into(),
    );
    raw.insert(
        "riff_removed_local_paths".into(),
        json!(["C:\\Music\\GoneFP.mp3", "loc_id_removed"])
            .to_string()
            .into(),
    );

    let snap = AppStateSnapshot::from_storage_map(raw);
    assert_eq!(snap.local_tracks.len(), 1);
    assert_eq!(snap.local_tracks[0].id, "loc1");

    // Empty riff_local_tracks should fall back to populated devsize_local_tracks
    let mut fallback_raw: HashMap<String, serde_json::Value> = HashMap::new();
    fallback_raw.insert("riff_local_tracks".into(), json!("[]"));
    fallback_raw.insert(
        "devsize_local_tracks".into(),
        json!([{"id": "loc2", "title": "Fallback", "path": "C:\\Music\\F.mp3", "platform": "local"}])
            .to_string()
            .into(),
    );
    let snap2 = AppStateSnapshot::from_storage_map(fallback_raw);
    assert_eq!(snap2.local_tracks.len(), 1);
    assert_eq!(snap2.local_tracks[0].id, "loc2");
}

#[test]
fn test_track_record_matches_respects_differing_platforms() {
    let yt = TrackRecord {
        id: "shared_123".to_string(),
        platform: "ytmusic".to_string(),
        title: "Song A".to_string(),
        ..Default::default()
    };
    let sc = TrackRecord {
        id: "shared_123".to_string(),
        platform: "soundcloud".to_string(),
        title: "Song B".to_string(),
        ..Default::default()
    };
    let no_plat = TrackRecord {
        id: "shared_123".to_string(),
        platform: String::new(),
        ..Default::default()
    };
    let yt_upper = TrackRecord {
        id: "shared_123".to_string(),
        platform: "YTMusic".to_string(),
        ..Default::default()
    };

    assert!(!yt.matches(&sc));
    assert!(yt.matches(&no_plat));
    assert!(yt.matches(&yt_upper));
}

#[test]
fn test_repeat_mode_fallback_on_unrecognized_primary_key() {
    let mut raw: HashMap<String, serde_json::Value> = HashMap::new();
    raw.insert("raser_repeat_mode".into(), json!("bogus_value"));
    raw.insert("riff_repeat".into(), json!("1"));

    let snap = AppStateSnapshot::from_storage_map(raw);
    assert_eq!(snap.settings.repeat_mode, RepeatMode::One);

    let mut raw2: HashMap<String, serde_json::Value> = HashMap::new();
    raw2.insert("raser_repeat_mode".into(), json!("invalid"));
    raw2.insert("riff_repeat_mode".into(), json!("all"));
    let snap2 = AppStateSnapshot::from_storage_map(raw2);
    assert_eq!(snap2.settings.repeat_mode, RepeatMode::All);
}
