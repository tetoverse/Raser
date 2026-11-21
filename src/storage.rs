use crate::local_files::LocalTrack;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;

pub const DEFAULT_SIDEBAR_ORDER: [&str; 6] = [
    "discover",
    "favorites",
    "albums",
    "local",
    "history",
    "queue",
];

fn raser_data_dir_override() -> Option<PathBuf> {
    std::env::var("RASER_DATA_DIR")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
}

pub fn get_storage_path() -> PathBuf {
    if let Some(dir) = raser_data_dir_override() {
        return dir.join("storage.json");
    }
    if let Ok(app_data) = std::env::var("APPDATA") {
        if !app_data.trim().is_empty() {
            return PathBuf::from(app_data).join("Raser").join("storage.json");
        }
    }
    PathBuf::from("storage.json")
}

pub fn read_storage_raw() -> String {
    let path = get_storage_path();
    if path.exists() {
        if let Ok(content) = fs::read_to_string(&path) {
            let trimmed = content.trim_start_matches('\u{feff}').trim();
            if !trimmed.is_empty() && (trimmed.starts_with('{') || trimmed.starts_with('[')) {
                return trimmed.to_string();
            }
        }
    }
    "{}".to_string()
}

pub fn read_storage_map() -> HashMap<String, Value> {
    let raw = read_storage_raw();
    serde_json::from_str(&raw).unwrap_or_default()
}

pub fn save_storage_map(data: HashMap<String, Value>) {
    let path = get_storage_path();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(json_str) = serde_json::to_string_pretty(&data) {
        let _ = fs::write(&path, json_str);
    }
}

pub fn get_cache_dir() -> PathBuf {
    if let Some(dir) = raser_data_dir_override() {
        return dir.join("cache");
    }
    if let Ok(local_app_data) = std::env::var("LOCALAPPDATA") {
        if !local_app_data.trim().is_empty() {
            return PathBuf::from(local_app_data).join("Raser").join("cache");
        }
    }
    if let Ok(app_data) = std::env::var("APPDATA") {
        if !app_data.trim().is_empty() {
            return PathBuf::from(app_data).join("Raser").join("cache");
        }
    }
    PathBuf::from("cache")
}

pub fn get_audio_cache_dir() -> PathBuf {
    let dir = get_cache_dir().join("audio");
    let _ = fs::create_dir_all(&dir);
    dir
}

pub fn get_lyrics_cache_dir() -> PathBuf {
    let dir = get_cache_dir().join("lyrics");
    let _ = fs::create_dir_all(&dir);
    dir
}

pub fn sanitize_cache_key(key: &str) -> String {
    key.chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect()
}

pub fn get_cached_audio_path(track_id: &str) -> Option<PathBuf> {
    if track_id.is_empty() {
        return None;
    }
    let clean_id = sanitize_cache_key(track_id);
    let p = get_audio_cache_dir().join(format!("{}.bin", clean_id));
    if p.exists() && p.metadata().map(|m| m.len() > 1024).unwrap_or(false) {
        Some(p)
    } else {
        None
    }
}

pub fn save_cached_audio(track_id: &str, bytes: &[u8]) {
    if track_id.is_empty() || bytes.is_empty() {
        return;
    }
    let clean_id = sanitize_cache_key(track_id);
    let p = get_audio_cache_dir().join(format!("{}.bin", clean_id));
    let _ = fs::write(p, bytes);
}

pub fn get_cached_lyrics(track_id: &str) -> Option<String> {
    if track_id.is_empty() {
        return None;
    }
    let clean_id = sanitize_cache_key(track_id);
    let p = get_lyrics_cache_dir().join(format!("{}.json", clean_id));
    if p.exists() {
        fs::read_to_string(p).ok()
    } else {
        None
    }
}

pub fn save_cached_lyrics(track_id: &str, json_content: &str) {
    if track_id.is_empty() || json_content.is_empty() {
        return;
    }
    let clean_id = sanitize_cache_key(track_id);
    let p = get_lyrics_cache_dir().join(format!("{}.json", clean_id));
    let _ = fs::write(p, json_content);
}

pub fn get_cache_stats() -> (u64, usize) {
    let mut total_bytes = 0u64;
    let mut file_count = 0usize;

    for dir in &[get_audio_cache_dir(), get_lyrics_cache_dir()] {
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                if let Ok(meta) = entry.metadata() {
                    if meta.is_file() {
                        total_bytes += meta.len();
                        file_count += 1;
                    }
                }
            }
        }
    }
    (total_bytes, file_count)
}

pub fn clear_cache() {
    let _ = fs::remove_dir_all(get_cache_dir());
    let _ = fs::create_dir_all(get_audio_cache_dir());
    let _ = fs::create_dir_all(get_lyrics_cache_dir());
}

// --- Typed Domain Structs & v1.2.6 Polymorphic JSON Adapters ---

fn deserialize_lossy_string<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let opt = Option::<Value>::deserialize(deserializer)?;
    Ok(match opt {
        Some(Value::String(s)) => s,
        Some(Value::Number(n)) => n.to_string(),
        Some(Value::Bool(b)) => b.to_string(),
        _ => String::new(),
    })
}

fn deserialize_null_opt_string<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let opt = Option::<Value>::deserialize(deserializer)?;
    Ok(match opt {
        Some(Value::String(s)) => Some(s),
        Some(Value::Number(n)) => Some(n.to_string()),
        Some(Value::Bool(b)) => Some(b.to_string()),
        _ => None,
    })
}

fn deserialize_lossy_f64<'de, D>(deserializer: D) -> Result<f64, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let opt = Option::<Value>::deserialize(deserializer)?;
    Ok(match opt {
        Some(Value::Number(n)) => n.as_f64().filter(|f| f.is_finite()).unwrap_or(0.0),
        Some(Value::String(s)) => s
            .trim()
            .parse::<f64>()
            .ok()
            .filter(|f| f.is_finite())
            .unwrap_or(0.0),
        _ => 0.0,
    })
}

fn deserialize_null_opt_u64<'de, D>(deserializer: D) -> Result<Option<u64>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let opt = Option::<Value>::deserialize(deserializer)?;
    Ok(match opt {
        Some(Value::Number(n)) => n.as_u64().or_else(|| {
            n.as_f64()
                .filter(|f| f.is_finite() && *f >= 0.0)
                .map(|f| f.round() as u64)
        }),
        Some(Value::String(s)) => s.trim().parse::<u64>().ok(),
        _ => None,
    })
}

fn deserialize_resilient_vec<'de, D, T>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: DeserializeOwned,
{
    let opt = Option::<Value>::deserialize(deserializer)?;
    let Some(Value::Array(arr)) = opt else {
        return Ok(Vec::new());
    };
    let mut out = Vec::with_capacity(arr.len());
    for item in arr {
        if let Ok(parsed) = serde_json::from_value::<T>(item) {
            out.push(parsed);
        }
    }
    Ok(out)
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(default)]
pub struct TrackRecord {
    #[serde(deserialize_with = "deserialize_lossy_string")]
    pub id: String,
    #[serde(deserialize_with = "deserialize_lossy_string")]
    pub title: String,
    #[serde(deserialize_with = "deserialize_lossy_string")]
    pub artist: String,
    #[serde(deserialize_with = "deserialize_lossy_string")]
    pub album: String,
    #[serde(deserialize_with = "deserialize_lossy_f64")]
    pub duration: f64,
    #[serde(deserialize_with = "deserialize_lossy_string")]
    pub thumbnail: String,
    #[serde(deserialize_with = "deserialize_lossy_string")]
    pub cover: String,
    #[serde(deserialize_with = "deserialize_lossy_string")]
    pub path: String,
    #[serde(deserialize_with = "deserialize_lossy_string")]
    pub url: String,
    #[serde(deserialize_with = "deserialize_lossy_string")]
    pub platform: String,
    #[serde(rename = "addedAt", default, deserialize_with = "deserialize_null_opt_u64")]
    pub added_at: Option<u64>,
    #[serde(rename = "playedAt", default, deserialize_with = "deserialize_null_opt_u64")]
    pub played_at: Option<u64>,
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

impl TrackRecord {
    pub fn canonical_id(&self) -> String {
        let plat = if self.platform.trim().is_empty() {
            "unknown"
        } else {
            self.platform.trim()
        };
        if !self.id.trim().is_empty() {
            return format!("{}:{}", plat, self.id.trim());
        }
        if !self.url.trim().is_empty() {
            return format!("{}:{}", plat, self.url.trim());
        }
        if !self.path.trim().is_empty() {
            return format!("{}:{}", plat, self.path.trim());
        }
        format!(
            "{}:{}|{}|{}",
            plat,
            self.title.trim(),
            self.artist.trim(),
            self.duration.round() as i64
        )
    }

    pub fn matches(&self, other: &TrackRecord) -> bool {
        if !self.id.is_empty() && !other.id.is_empty() && self.id == other.id {
            let p1 = self.platform.trim();
            let p2 = other.platform.trim();
            if p1.is_empty() || p2.is_empty() || p1.eq_ignore_ascii_case(p2) {
                return true;
            }
            return false;
        }
        if !self.path.is_empty() && !other.path.is_empty() && self.path == other.path {
            return true;
        }
        self.canonical_id() == other.canonical_id()
    }

    pub fn playable_source(&self) -> String {
        if !self.path.trim().is_empty() {
            self.path.clone()
        } else if let Some(fp) = self.extra.get("filePath").and_then(|v| v.as_str()) {
            if !fp.trim().is_empty() {
                return fp.to_string();
            }
            self.url.clone()
        } else {
            self.url.clone()
        }
    }
}

impl From<LocalTrack> for TrackRecord {
    fn from(t: LocalTrack) -> Self {
        Self {
            id: t.id,
            title: t.title,
            artist: t.artist,
            album: t.album,
            duration: 0.0,
            thumbnail: t.thumbnail,
            cover: t.cover,
            path: t.path,
            url: t.url,
            platform: if t.platform.is_empty() {
                "local".to_string()
            } else {
                t.platform
            },
            added_at: Some(now_millis()),
            played_at: None,
            extra: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(default)]
pub struct PlaylistRecord {
    #[serde(deserialize_with = "deserialize_lossy_string")]
    pub id: String,
    #[serde(deserialize_with = "deserialize_lossy_string")]
    pub name: String,
    #[serde(deserialize_with = "deserialize_null_opt_string")]
    pub description: Option<String>,
    #[serde(deserialize_with = "deserialize_null_opt_string")]
    pub cover: Option<String>,
    #[serde(deserialize_with = "deserialize_resilient_vec")]
    pub tracks: Vec<TrackRecord>,
    #[serde(deserialize_with = "deserialize_null_opt_string")]
    pub sort_mode: Option<String>,
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(default)]
pub struct SavedAlbumRecord {
    #[serde(deserialize_with = "deserialize_lossy_string")]
    pub id: String,
    #[serde(deserialize_with = "deserialize_lossy_string")]
    pub title: String,
    #[serde(deserialize_with = "deserialize_lossy_string")]
    pub artist: String,
    #[serde(deserialize_with = "deserialize_null_opt_string")]
    pub cover: Option<String>,
    #[serde(rename = "type", deserialize_with = "deserialize_null_opt_string")]
    pub album_type: Option<String>,
    #[serde(deserialize_with = "deserialize_null_opt_string")]
    pub year: Option<String>,
    #[serde(rename = "trackCount", deserialize_with = "deserialize_null_opt_u64")]
    pub track_count: Option<u64>,
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(default)]
pub struct TrackRename {
    #[serde(deserialize_with = "deserialize_lossy_string")]
    pub title: String,
    #[serde(deserialize_with = "deserialize_lossy_string")]
    pub artist: String,
    #[serde(rename = "updatedAt", default, deserialize_with = "deserialize_null_opt_u64")]
    pub updated_at: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(default)]
pub struct TrackTrim {
    #[serde(deserialize_with = "deserialize_lossy_f64")]
    pub start: f64,
    #[serde(deserialize_with = "deserialize_lossy_f64")]
    pub end: f64,
    #[serde(rename = "updatedAt", default, deserialize_with = "deserialize_null_opt_u64")]
    pub updated_at: Option<u64>,
}

impl TrackTrim {
    pub fn is_active(&self) -> bool {
        self.start.is_finite()
            && self.end.is_finite()
            && self.start >= 0.0
            && self.end > self.start + 1.5
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(default)]
pub struct TrackArt {
    #[serde(deserialize_with = "deserialize_null_opt_string")]
    pub cover: Option<String>,
    #[serde(deserialize_with = "deserialize_null_opt_string")]
    pub banner: Option<String>,
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum RepeatMode {
    #[default]
    Off,
    All,
    One,
}

impl RepeatMode {
    pub fn as_str(self) -> &'static str {
        match self {
            RepeatMode::Off => "off",
            RepeatMode::All => "all",
            RepeatMode::One => "one",
        }
    }

    pub fn from_legacy_str(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "off" | "0" | "false" | "none" => Some(RepeatMode::Off),
            "all" | "2" | "queue" => Some(RepeatMode::All),
            "one" | "1" | "track" | "single" | "true" => Some(RepeatMode::One),
            _ => None,
        }
    }

    pub fn next(self) -> Self {
        match self {
            RepeatMode::Off => RepeatMode::All,
            RepeatMode::All => RepeatMode::One,
            RepeatMode::One => RepeatMode::Off,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum TrackSortMode {
    #[default]
    Manual,
    AddedDesc,
    AddedAsc,
}

impl TrackSortMode {
    pub fn as_str(self) -> &'static str {
        match self {
            TrackSortMode::Manual => "manual",
            TrackSortMode::AddedDesc => "added-desc",
            TrackSortMode::AddedAsc => "added-asc",
        }
    }

    pub fn from_str_lossy(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "added-desc" => TrackSortMode::AddedDesc,
            "added-asc" => TrackSortMode::AddedAsc,
            _ => TrackSortMode::Manual,
        }
    }

    pub fn next(self) -> Self {
        match self {
            TrackSortMode::Manual => TrackSortMode::AddedDesc,
            TrackSortMode::AddedDesc => TrackSortMode::AddedAsc,
            TrackSortMode::AddedAsc => TrackSortMode::Manual,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct AdvancedPrefs {
    pub hide_titlebar: bool,
    pub autohide_timestamps: bool,
    pub classic_sidebar: bool,
    pub autoplay: bool,
    pub reduce_motion: bool,
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

impl Default for AdvancedPrefs {
    fn default() -> Self {
        Self {
            hide_titlebar: false,
            autohide_timestamps: true,
            classic_sidebar: false,
            autoplay: true,
            reduce_motion: false,
            extra: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct AppSettings {
    pub volume: f32,
    pub muted: bool,
    pub shuffle: bool,
    pub repeat_mode: RepeatMode,
    pub speed: f32,
    pub ui_lang: Option<String>,
    pub platform: String,
    pub sidebar_order: Vec<String>,
    pub favorites_sort: TrackSortMode,
    pub playlist_sort: TrackSortMode,
    pub pinned_tracks: Vec<String>,
    pub queue_panel_open: bool,
    pub theme_mode: String,
    pub palette_style: String,
    pub ui_scale: u32,
    pub audio_quality: String,
    pub proxy_url: String,
    pub search_history: Vec<String>,
    pub advanced: AdvancedPrefs,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            volume: 0.85,
            muted: false,
            shuffle: false,
            repeat_mode: RepeatMode::Off,
            speed: 1.0,
            ui_lang: None,
            platform: "ytmusic".to_string(),
            sidebar_order: DEFAULT_SIDEBAR_ORDER.iter().map(|s| (*s).to_string()).collect(),
            favorites_sort: TrackSortMode::Manual,
            playlist_sort: TrackSortMode::Manual,
            pinned_tracks: Vec::new(),
            queue_panel_open: false,
            theme_mode: "dark".to_string(),
            palette_style: "classic".to_string(),
            ui_scale: 100,
            audio_quality: "high".to_string(),
            proxy_url: String::new(),
            search_history: Vec::new(),
            advanced: AdvancedPrefs::default(),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct AppStateSnapshot {
    pub playlists: Vec<PlaylistRecord>,
    pub favorites: Vec<TrackRecord>,
    pub history: Vec<TrackRecord>,
    pub local_tracks: Vec<TrackRecord>,
    pub removed_local_paths: Vec<String>,
    pub saved_albums: Vec<SavedAlbumRecord>,
    pub track_renames: HashMap<String, TrackRename>,
    pub track_trims: HashMap<String, TrackTrim>,
    pub track_art: HashMap<String, TrackArt>,
    pub queue: Vec<TrackRecord>,
    pub last_track: Option<TrackRecord>,
    pub settings: AppSettings,
    pub raw_map: HashMap<String, Value>,
}

fn decode_raw_json_value(val: &Value) -> Option<Value> {
    match val {
        Value::String(s) => {
            let trimmed = s.trim();
            if trimmed.is_empty() {
                return None;
            }
            serde_json::from_str::<Value>(trimmed).ok()
        }
        Value::Null => None,
        other => Some(other.clone()),
    }
}

pub fn decode_value<T: DeserializeOwned>(val: &Value) -> Option<T> {
    match val {
        Value::String(s) => {
            let trimmed = s.trim();
            if trimmed.is_empty() {
                return None;
            }
            if let Ok(parsed) = serde_json::from_str::<T>(trimmed) {
                return Some(parsed);
            }
            serde_json::from_value::<T>(val.clone()).ok()
        }
        Value::Null => None,
        other => serde_json::from_value::<T>(other.clone()).ok(),
    }
}

pub fn decode_resilient_vec<T: DeserializeOwned>(val: &Value) -> Option<Vec<T>> {
    let raw = decode_raw_json_value(val)?;
    let Value::Array(arr) = raw else {
        return None;
    };
    let mut out = Vec::with_capacity(arr.len());
    for item in arr {
        if let Ok(parsed) = serde_json::from_value::<T>(item) {
            out.push(parsed);
        }
    }
    Some(out)
}

pub fn decode_resilient_map<T: DeserializeOwned>(val: &Value) -> Option<HashMap<String, T>> {
    let raw = decode_raw_json_value(val)?;
    let Value::Object(obj) = raw else {
        return None;
    };
    let mut out = HashMap::with_capacity(obj.len());
    for (k, v) in obj {
        if let Ok(parsed) = serde_json::from_value::<T>(v) {
            out.insert(k, parsed);
        }
    }
    Some(out)
}

pub fn decode_json_field<T: DeserializeOwned>(
    map: &HashMap<String, Value>,
    keys: &[&str],
) -> Option<T> {
    for &key in keys {
        if let Some(val) = map.get(key) {
            if let Some(parsed) = decode_value::<T>(val) {
                return Some(parsed);
            }
        }
    }
    None
}

pub fn decode_json_vec_field<T: DeserializeOwned>(
    map: &HashMap<String, Value>,
    keys: &[&str],
) -> Option<Vec<T>> {
    for &key in keys {
        if let Some(val) = map.get(key) {
            if let Some(parsed) = decode_resilient_vec::<T>(val) {
                return Some(parsed);
            }
        }
    }
    None
}

pub fn decode_json_map_field<T: DeserializeOwned>(
    map: &HashMap<String, Value>,
    keys: &[&str],
) -> Option<HashMap<String, T>> {
    for &key in keys {
        if let Some(val) = map.get(key) {
            if let Some(parsed) = decode_resilient_map::<T>(val) {
                return Some(parsed);
            }
        }
    }
    None
}

pub fn decode_string_field(map: &HashMap<String, Value>, keys: &[&str]) -> Option<String> {
    for &key in keys {
        if let Some(val) = map.get(key) {
            match val {
                Value::String(s) => {
                    let trimmed = s.trim();
                    if !trimmed.is_empty() {
                        return Some(trimmed.to_string());
                    }
                }
                Value::Number(n) => return Some(n.to_string()),
                Value::Bool(b) => return Some(b.to_string()),
                _ => {}
            }
        }
    }
    None
}

pub fn decode_bool_field(map: &HashMap<String, Value>, keys: &[&str]) -> Option<bool> {
    for &key in keys {
        if let Some(val) = map.get(key) {
            match val {
                Value::Bool(b) => return Some(*b),
                Value::Number(n) => {
                    if let Some(i) = n.as_i64() {
                        return Some(i != 0);
                    }
                    if let Some(f) = n.as_f64() {
                        return Some(f != 0.0);
                    }
                }
                Value::String(s) => match s.trim().to_ascii_lowercase().as_str() {
                    "1" | "true" | "yes" | "on" => return Some(true),
                    "0" | "false" | "no" | "off" => return Some(false),
                    _ => {}
                },
                _ => {}
            }
        }
    }
    None
}

pub fn decode_f32_field(map: &HashMap<String, Value>, keys: &[&str]) -> Option<f32> {
    for &key in keys {
        if let Some(val) = map.get(key) {
            match val {
                Value::Number(n) => {
                    if let Some(f) = n.as_f64() {
                        if f.is_finite() {
                            return Some(f as f32);
                        }
                    }
                }
                Value::String(s) => {
                    if let Ok(f) = s.trim().parse::<f32>() {
                        if f.is_finite() {
                            return Some(f);
                        }
                    }
                }
                _ => {}
            }
        }
    }
    None
}

pub fn decode_u32_field(map: &HashMap<String, Value>, keys: &[&str]) -> Option<u32> {
    for &key in keys {
        if let Some(val) = map.get(key) {
            match val {
                Value::Number(n) => {
                    if let Some(u) = n.as_u64() {
                        return Some(u as u32);
                    }
                    if let Some(f) = n.as_f64() {
                        if f.is_finite() && f >= 0.0 {
                            return Some(f.round() as u32);
                        }
                    }
                }
                Value::String(s) => {
                    if let Ok(u) = s.trim().parse::<u32>() {
                        return Some(u);
                    }
                    if let Ok(f) = s.trim().parse::<f64>() {
                        if f.is_finite() && f >= 0.0 {
                            return Some(f.round() as u32);
                        }
                    }
                }
                _ => {}
            }
        }
    }
    None
}

pub fn encode_storage_json_string<T: Serialize>(val: &T) -> Value {
    match serde_json::to_string(val) {
        Ok(s) => Value::String(s),
        Err(_) => Value::Null,
    }
}

pub fn now_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn normalize_sidebar_order(raw: Vec<String>) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for item in raw {
        if DEFAULT_SIDEBAR_ORDER.contains(&item.as_str()) && seen.insert(item.clone()) {
            out.push(item);
        }
    }
    for &default_item in &DEFAULT_SIDEBAR_ORDER {
        if seen.insert(default_item.to_string()) {
            out.push(default_item.to_string());
        }
    }
    out
}

impl AppStateSnapshot {
    pub fn load() -> Self {
        let map = read_storage_map();
        Self::from_storage_map(map)
    }

    pub fn from_storage_map(map: HashMap<String, Value>) -> Self {
        let playlists = decode_json_vec_field::<PlaylistRecord>(&map, &["devsize_playlists"])
            .unwrap_or_default();
        let favorites = decode_json_vec_field::<TrackRecord>(&map, &["devsize_favorites"])
            .unwrap_or_default();
        let history = decode_json_vec_field::<TrackRecord>(&map, &["devsize_history"])
            .unwrap_or_default();

        let removed_local_paths =
            decode_json_vec_field::<String>(&map, &["riff_removed_local_paths"])
                .unwrap_or_default();
        let removed_set: HashSet<&str> = removed_local_paths.iter().map(|s| s.as_str()).collect();

        // Match ui/app.js:959 — prefer the first non-empty local-track array across keys
        let mut raw_local = Vec::new();
        for &key in &[
            "riff_local_tracks",
            "raser_local_tracks",
            "devsize_local_tracks",
            "riff_local_files",
            "raser_local_files",
            "devsize_local_files",
        ] {
            if let Some(val) = map.get(key) {
                if let Some(arr) = decode_resilient_vec::<TrackRecord>(val) {
                    if !arr.is_empty() {
                        raw_local = arr;
                        break;
                    }
                }
            }
        }

        let local_tracks = raw_local
            .into_iter()
            .filter(|t| {
                let file_path = t
                    .extra
                    .get("filePath")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let p = if !t.path.is_empty() {
                    t.path.as_str()
                } else if !file_path.is_empty() {
                    file_path
                } else {
                    t.url.as_str()
                };
                let path_removed = !p.is_empty() && removed_set.contains(p);
                let id_removed = !t.id.is_empty() && removed_set.contains(t.id.as_str());
                !path_removed && !id_removed
            })
            .collect();

        let saved_albums = decode_json_vec_field::<SavedAlbumRecord>(
            &map,
            &["riff_saved_albums", "raser_saved_albums", "devsize_saved_albums"],
        )
        .unwrap_or_default();

        let track_renames = decode_json_map_field::<TrackRename>(
            &map,
            &["riff_track_renames", "raser_track_renames", "devsize_track_renames"],
        )
        .unwrap_or_default();

        let track_trims = decode_json_map_field::<TrackTrim>(
            &map,
            &["riff_track_trims", "raser_track_trims", "devsize_track_trims"],
        )
        .unwrap_or_default();

        let track_art = decode_json_map_field::<TrackArt>(
            &map,
            &["riff_track_art", "raser_track_art", "devsize_track_art"],
        )
        .unwrap_or_default();

        let queue =
            decode_json_vec_field::<TrackRecord>(&map, &["devsize_queue"]).unwrap_or_default();
        let last_track = decode_json_field::<TrackRecord>(&map, &["devsize_last_track"]);

        let mut settings = AppSettings::default();
        if let Some(v) = decode_f32_field(&map, &["riff_volume", "raser_volume"]) {
            settings.volume = v.clamp(0.0, 2.0);
        }
        if let Some(m) = decode_bool_field(&map, &["riff_muted", "raser_muted"]) {
            settings.muted = m;
        }
        if let Some(sh) = decode_bool_field(&map, &["riff_shuffle", "raser_shuffle"]) {
            settings.shuffle = sh;
        }
        for &key in &[
            "raser_repeat_mode",
            "riff_repeat_mode",
            "raser_repeat",
            "riff_repeat",
        ] {
            if let Some(rep) = decode_string_field(&map, &[key]) {
                if let Some(mode) = RepeatMode::from_legacy_str(&rep) {
                    settings.repeat_mode = mode;
                    break;
                }
            }
        }
        if let Some(spd) = decode_f32_field(&map, &["riff_speed", "raser_speed"]) {
            settings.speed = spd.clamp(0.25, 4.0);
        }
        settings.ui_lang = decode_string_field(&map, &["riff_ui_lang", "raser_ui_lang"]);
        if let Some(p) = decode_string_field(&map, &["raser_platform", "riff_platform"]) {
            settings.platform = p;
        }
        if let Some(order) =
            decode_json_vec_field::<String>(&map, &["riff_sidebar_order", "raser_sidebar_order"])
        {
            settings.sidebar_order = normalize_sidebar_order(order);
        }
        if let Some(s) = decode_string_field(&map, &["riff_favorites_sort", "raser_favorites_sort"])
        {
            settings.favorites_sort = TrackSortMode::from_str_lossy(&s);
        }
        if let Some(s) = decode_string_field(&map, &["riff_playlist_sort", "raser_playlist_sort"]) {
            settings.playlist_sort = TrackSortMode::from_str_lossy(&s);
        }
        if let Some(pinned) =
            decode_json_vec_field::<String>(&map, &["riff_pinned_tracks", "raser_pinned_tracks"])
        {
            settings.pinned_tracks = pinned;
        }
        if let Some(open) =
            decode_bool_field(&map, &["riff_queue_panel_open", "raser_queue_panel_open"])
        {
            settings.queue_panel_open = open;
        }
        if let Some(tm) = decode_string_field(&map, &["riff_theme_mode", "raser_theme_mode"]) {
            settings.theme_mode = tm;
        }
        if let Some(ps) =
            decode_string_field(&map, &["riff_palette_style", "raser_palette_style"])
        {
            settings.palette_style = ps;
        }
        if let Some(sc) = decode_u32_field(&map, &["devsize_ui_scale"]) {
            settings.ui_scale = sc.clamp(80, 130);
        }
        if let Some(aq) = decode_string_field(&map, &["riff_audio_quality", "raser_audio_quality"]) {
            settings.audio_quality = aq;
        }
        if let Some(px) = decode_string_field(&map, &["riff_proxy_url", "raser_proxy_url", "riff_proxy"]) {
            settings.proxy_url = px;
        }
        if let Some(sh) = decode_json_vec_field::<String>(
            &map,
            &["devsize_search_history", "riff_search_history", "raser_search_history"],
        ) {
            settings.search_history = sh;
        }

        // Load optional fallback riff_advanced_prefs object first, then override with individual riff_adv_<name> keys
        let mut adv = decode_json_field::<AdvancedPrefs>(
            &map,
            &["riff_advanced_prefs", "raser_advanced_prefs"],
        )
        .unwrap_or_default();

        if let Some(v) = decode_bool_field(&map, &["riff_adv_hide_titlebar", "raser_adv_hide_titlebar"]) {
            adv.hide_titlebar = v;
        }
        if let Some(v) = decode_bool_field(
            &map,
            &["riff_adv_autohide_timestamps", "raser_adv_autohide_timestamps"],
        ) {
            adv.autohide_timestamps = v;
        }
        if let Some(v) = decode_bool_field(
            &map,
            &["riff_adv_classic_sidebar", "raser_adv_classic_sidebar"],
        ) {
            adv.classic_sidebar = v;
        }
        if let Some(v) = decode_bool_field(&map, &["riff_adv_autoplay", "raser_adv_autoplay"]) {
            adv.autoplay = v;
        }
        if let Some(v) =
            decode_bool_field(&map, &["riff_adv_reduce_motion", "raser_adv_reduce_motion"])
        {
            adv.reduce_motion = v;
        }
        settings.advanced = adv;

        Self {
            playlists,
            favorites,
            history,
            local_tracks,
            removed_local_paths,
            saved_albums,
            track_renames,
            track_trims,
            track_art,
            queue,
            last_track,
            settings,
            raw_map: map,
        }
    }

    pub fn to_storage_map(&self) -> HashMap<String, Value> {
        let mut map = self.raw_map.clone();

        map.insert(
            "devsize_playlists".to_string(),
            encode_storage_json_string(&self.playlists),
        );
        map.insert(
            "devsize_favorites".to_string(),
            encode_storage_json_string(&self.favorites),
        );
        map.insert(
            "devsize_history".to_string(),
            encode_storage_json_string(&self.history),
        );

        let local_val = encode_storage_json_string(&self.local_tracks);
        map.insert("devsize_local_tracks".to_string(), local_val.clone());
        map.insert("riff_local_tracks".to_string(), local_val.clone());
        if map.contains_key("raser_local_tracks") {
            map.insert("raser_local_tracks".to_string(), local_val.clone());
        }
        if self.local_tracks.is_empty() {
            for k in ["riff_local_files", "raser_local_files", "devsize_local_files"] {
                if map.contains_key(k) {
                    map.insert(k.to_string(), local_val.clone());
                }
            }
        }

        map.insert(
            "riff_removed_local_paths".to_string(),
            encode_storage_json_string(&self.removed_local_paths),
        );
        map.insert(
            "riff_saved_albums".to_string(),
            encode_storage_json_string(&self.saved_albums),
        );
        map.insert(
            "riff_track_renames".to_string(),
            encode_storage_json_string(&self.track_renames),
        );
        map.insert(
            "riff_track_trims".to_string(),
            encode_storage_json_string(&self.track_trims),
        );
        map.insert(
            "riff_track_art".to_string(),
            encode_storage_json_string(&self.track_art),
        );
        map.insert(
            "devsize_queue".to_string(),
            encode_storage_json_string(&self.queue),
        );
        if let Some(ref last) = self.last_track {
            map.insert(
                "devsize_last_track".to_string(),
                encode_storage_json_string(last),
            );
        } else {
            map.remove("devsize_last_track");
        }

        let set_mirrored = |m: &mut HashMap<String, Value>, suffix: &str, v: String| {
            m.insert(format!("riff_{}", suffix), Value::String(v.clone()));
            m.insert(format!("raser_{}", suffix), Value::String(v));
        };

        set_mirrored(&mut map, "volume", format!("{:.4}", self.settings.volume));
        set_mirrored(
            &mut map,
            "muted",
            if self.settings.muted { "1" } else { "0" }.to_string(),
        );
        set_mirrored(
            &mut map,
            "shuffle",
            if self.settings.shuffle { "1" } else { "0" }.to_string(),
        );
        set_mirrored(
            &mut map,
            "repeat_mode",
            self.settings.repeat_mode.as_str().to_string(),
        );
        set_mirrored(&mut map, "speed", format!("{:.2}", self.settings.speed));
        if let Some(ref lang) = self.settings.ui_lang {
            set_mirrored(&mut map, "ui_lang", lang.clone());
        }
        set_mirrored(&mut map, "platform", self.settings.platform.clone());
        set_mirrored(
            &mut map,
            "sidebar_order",
            serde_json::to_string(&self.settings.sidebar_order).unwrap_or_else(|_| "[]".into()),
        );
        set_mirrored(
            &mut map,
            "favorites_sort",
            self.settings.favorites_sort.as_str().to_string(),
        );
        set_mirrored(
            &mut map,
            "playlist_sort",
            self.settings.playlist_sort.as_str().to_string(),
        );
        set_mirrored(
            &mut map,
            "pinned_tracks",
            serde_json::to_string(&self.settings.pinned_tracks).unwrap_or_else(|_| "[]".into()),
        );
        set_mirrored(
            &mut map,
            "queue_panel_open",
            if self.settings.queue_panel_open {
                "1"
            } else {
                "0"
            }
            .to_string(),
        );
        set_mirrored(&mut map, "theme_mode", self.settings.theme_mode.clone());
        set_mirrored(&mut map, "palette_style", self.settings.palette_style.clone());
        map.insert(
            "devsize_ui_scale".to_string(),
            Value::String(self.settings.ui_scale.to_string()),
        );
        set_mirrored(
            &mut map,
            "audio_quality",
            self.settings.audio_quality.clone(),
        );
        set_mirrored(&mut map, "proxy_url", self.settings.proxy_url.clone());
        let sh_json =
            serde_json::to_string(&self.settings.search_history).unwrap_or_else(|_| "[]".into());
        map.insert(
            "devsize_search_history".to_string(),
            Value::String(sh_json.clone()),
        );
        set_mirrored(&mut map, "search_history", sh_json);

        let set_adv = |m: &mut HashMap<String, Value>, name: &str, v: bool| {
            let s = if v { "true" } else { "false" }.to_string();
            m.insert(format!("riff_adv_{}", name), Value::String(s.clone()));
            m.insert(format!("raser_adv_{}", name), Value::String(s));
        };
        set_adv(&mut map, "hide_titlebar", self.settings.advanced.hide_titlebar);
        set_adv(
            &mut map,
            "autohide_timestamps",
            self.settings.advanced.autohide_timestamps,
        );
        set_adv(
            &mut map,
            "classic_sidebar",
            self.settings.advanced.classic_sidebar,
        );
        set_adv(&mut map, "autoplay", self.settings.advanced.autoplay);
        set_adv(&mut map, "reduce_motion", self.settings.advanced.reduce_motion);
        map.insert(
            "riff_advanced_prefs".to_string(),
            encode_storage_json_string(&self.settings.advanced),
        );

        map
    }

    pub fn save(&mut self) {
        let map = self.to_storage_map();
        self.raw_map = map.clone();
        save_storage_map(map);
    }
}
