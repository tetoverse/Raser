use crate::{audio_core, discord};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct QueueTrack {
    #[serde(default)]
    pub id: String,
    #[serde(default, rename = "resolvedId")]
    pub resolved_id: Option<String>,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub artist: String,
    #[serde(default)]
    pub thumbnail: Option<String>,
    #[serde(default)]
    pub duration: Option<f64>,
    #[serde(default)]
    pub platform: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default, rename = "_preloadedStreamUrl")]
    pub preloaded_stream_url: Option<String>,
    #[serde(default, rename = "searchQuery")]
    pub search_query: Option<String>,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

impl QueueTrack {
    pub fn effective_id(&self) -> Option<String> {
        if let Some(ref rid) = self.resolved_id {
            if !rid.is_empty() {
                return Some(rid.clone());
            }
        }
        if !self.id.is_empty() {
            return Some(self.id.clone());
        }
        None
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PlaybackSnapshot {
    #[serde(default)]
    pub queue: Vec<QueueTrack>,
    #[serde(default, rename = "queueIndex")]
    pub queue_index: i64,
    #[serde(default = "default_repeat_mode", rename = "repeatMode")]
    pub repeat_mode: String,
    #[serde(default, rename = "isShuffle")]
    pub is_shuffle: bool,
    #[serde(default = "default_platform")]
    pub platform: String,
    #[serde(default = "default_api_base", rename = "apiBaseUrl")]
    pub api_base_url: String,
    #[serde(default = "default_quality", rename = "audioQuality")]
    pub audio_quality: String,
    #[serde(default = "default_view", rename = "currentView")]
    pub current_view: String,
    #[serde(default, rename = "currentPlaylistId")]
    pub current_playlist_id: Option<String>,
    #[serde(default = "default_true", rename = "discordRpcEnabled")]
    pub discord_rpc_enabled: bool,
    #[serde(default = "default_true", rename = "discordArtistInStatus")]
    pub discord_artist_in_status: bool,
}

fn default_repeat_mode() -> String {
    "off".to_string()
}

fn default_platform() -> String {
    "ytmusic".to_string()
}

fn default_api_base() -> String {
    "https://app.raserapp.online".to_string()
}

fn default_quality() -> String {
    "medium".to_string()
}

fn default_view() -> String {
    "discover".to_string()
}

fn default_true() -> bool {
    true
}

impl Default for PlaybackSnapshot {
    fn default() -> Self {
        Self {
            queue: Vec::new(),
            queue_index: -1,
            repeat_mode: default_repeat_mode(),
            is_shuffle: false,
            platform: default_platform(),
            api_base_url: default_api_base(),
            audio_quality: default_quality(),
            current_view: default_view(),
            current_playlist_id: None,
            discord_rpc_enabled: true,
            discord_artist_in_status: true,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct ResumePayload {
    pub snapshot: PlaybackSnapshot,
    #[serde(rename = "audioState")]
    pub audio_state: audio_core::AudioState,
    #[serde(rename = "wasResumed")]
    pub was_resumed: bool,
}

struct BackgroundQueueInner {
    snapshot: PlaybackSnapshot,
    headless: bool,
    tray_on_close: bool,
    low_ram_tray: bool,
    shuffle_history: Vec<usize>,
    rng_state: u64,
    advancing: bool,
}

pub struct BackgroundQueue {
    inner: Mutex<BackgroundQueueInner>,
}

impl BackgroundQueue {
    pub fn new() -> Self {
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0x9E37_79B9_7F4A_7C15);
        Self {
            inner: Mutex::new(BackgroundQueueInner {
                snapshot: PlaybackSnapshot::default(),
                headless: false,
                tray_on_close: true,
                low_ram_tray: true,
                shuffle_history: Vec::new(),
                rng_state: seed ^ 0xA5A5_5A5A_1234_5678,
                advancing: false,
            }),
        }
    }

    pub fn update_snapshot(&self, mut snap: PlaybackSnapshot) {
        if snap.api_base_url.trim().is_empty() {
            snap.api_base_url = default_api_base();
        }
        if snap.platform.trim().is_empty() {
            snap.platform = default_platform();
        }
        if snap.audio_quality.trim().is_empty() {
            snap.audio_quality = default_quality();
        }
        if let Ok(mut g) = self.inner.lock() {
            if snap.queue_index >= 0 && (snap.queue_index as usize) < snap.queue.len() {
                let idx = snap.queue_index as usize;
                if g.shuffle_history.last().copied() != Some(idx) {
                    g.shuffle_history.push(idx);
                    if g.shuffle_history.len() > 200 {
                        g.shuffle_history.remove(0);
                    }
                }
            } else {
                g.shuffle_history.clear();
            }
            g.snapshot = snap;
        }
    }

    pub fn set_headless(&self, headless: bool) {
        if let Ok(mut g) = self.inner.lock() {
            g.headless = headless;
        }
    }

    pub fn is_headless(&self) -> bool {
        self.inner.lock().map(|g| g.headless).unwrap_or(false)
    }

    pub fn set_tray_on_close(&self, enabled: bool) {
        if let Ok(mut g) = self.inner.lock() {
            g.tray_on_close = enabled;
        }
    }

    pub fn tray_on_close(&self) -> bool {
        self.inner.lock().map(|g| g.tray_on_close).unwrap_or(true)
    }

    pub fn set_low_ram_tray(&self, enabled: bool) {
        if let Ok(mut g) = self.inner.lock() {
            g.low_ram_tray = enabled;
        }
    }

    pub fn low_ram_tray(&self) -> bool {
        self.inner.lock().map(|g| g.low_ram_tray).unwrap_or(true)
    }

    pub fn current_track(&self) -> Option<QueueTrack> {
        let g = self.inner.lock().ok()?;
        if g.snapshot.queue_index >= 0 {
            g.snapshot.queue.get(g.snapshot.queue_index as usize).cloned()
        } else {
            None
        }
    }

    pub fn discord_settings(&self) -> (bool, bool) {
        self.inner
            .lock()
            .map(|g| (g.snapshot.discord_rpc_enabled, g.snapshot.discord_artist_in_status))
            .unwrap_or((true, true))
    }

    pub fn build_resume_payload(&self, audio_state: audio_core::AudioState) -> Option<ResumePayload> {
        let g = self.inner.lock().ok()?;
        let has_track = g.snapshot.queue_index >= 0
            && (g.snapshot.queue_index as usize) < g.snapshot.queue.len();
        if !has_track {
            return None;
        }
        Some(ResumePayload {
            snapshot: g.snapshot.clone(),
            audio_state,
            was_resumed: true,
        })
    }

    pub fn try_begin_advance(&self) -> bool {
        if let Ok(mut g) = self.inner.lock() {
            if g.advancing {
                return false;
            }
            g.advancing = true;
            true
        } else {
            false
        }
    }

    pub fn finish_advance(&self) {
        if let Ok(mut g) = self.inner.lock() {
            g.advancing = false;
        }
    }

    /// Selects the next or previous track according to queue, shuffle, and repeat mode.
    /// Returns `(track, new_index, api_base_url, default_platform, audio_quality)`.
    pub fn pick_step(
        &self,
        direction: i32,
        natural_end: bool,
    ) -> Option<(QueueTrack, usize, String, String, String)> {
        let mut g = self.inner.lock().ok()?;
        let len = g.snapshot.queue.len();
        if len == 0 {
            return None;
        }
        let cur = if g.snapshot.queue_index >= 0 && (g.snapshot.queue_index as usize) < len {
            g.snapshot.queue_index as usize
        } else {
            0
        };

        let target_idx = if natural_end && g.snapshot.repeat_mode == "one" {
            cur
        } else if direction < 0 {
            if g.snapshot.is_shuffle && g.shuffle_history.len() > 1 {
                g.shuffle_history.pop();
                g.shuffle_history.last().copied().unwrap_or(cur)
            } else if cur > 0 {
                cur - 1
            } else if g.snapshot.repeat_mode == "all" {
                len - 1
            } else {
                0
            }
        } else if g.snapshot.is_shuffle && len > 1 {
            // Pick a random index not equal to cur, preferring unplayed items.
            let unplayed: Vec<usize> = (0..len)
                .filter(|i| *i != cur && !g.shuffle_history.contains(i))
                .collect();
            let pool = if !unplayed.is_empty() {
                unplayed
            } else if natural_end && g.snapshot.repeat_mode == "off" {
                return None;
            } else {
                g.shuffle_history.clear();
                (0..len).filter(|i| *i != cur).collect()
            };
            // xorshift64*
            let mut x = g.rng_state;
            x ^= x >> 12;
            x ^= x << 25;
            x ^= x >> 27;
            g.rng_state = x;
            let pick = (x.wrapping_mul(0x2545_F491_4F6C_DD1D) as usize) % pool.len();
            pool[pick]
        } else {
            let next = cur + 1;
            if next < len {
                next
            } else if g.snapshot.repeat_mode == "all" {
                0
            } else {
                return None;
            }
        };

        g.snapshot.queue_index = target_idx as i64;
        if g.shuffle_history.last().copied() != Some(target_idx) {
            g.shuffle_history.push(target_idx);
            if g.shuffle_history.len() > 200 {
                g.shuffle_history.remove(0);
            }
        }

        let track = g.snapshot.queue[target_idx].clone();
        Some((
            track,
            target_idx,
            g.snapshot.api_base_url.clone(),
            g.snapshot.platform.clone(),
            g.snapshot.audio_quality.clone(),
        ))
    }
}

fn ensure_absolute_url(url: &str, api_base: &str) -> String {
    if url.starts_with('/') {
        format!("{}{}", api_base.trim_end_matches('/'), url)
    } else {
        url.to_string()
    }
}

fn strip_local_url_prefix(raw: &str) -> String {
    if let Some(rest) = raw.strip_prefix("file:///") {
        urlencoding::decode(rest)
            .map(|c| c.into_owned())
            .unwrap_or_else(|_| rest.to_string())
            .replace('/', "\\")
    } else if raw.starts_with("http://asset.localhost/") || raw.starts_with("https://asset.localhost/") {
        let stripped = raw
            .trim_start_matches("http://asset.localhost/")
            .trim_start_matches("https://asset.localhost/");
        let decoded = urlencoding::decode(stripped)
            .map(|c| c.into_owned())
            .unwrap_or_else(|_| stripped.to_string());
        decoded.trim_start_matches("\\\\?\\").to_string()
    } else {
        raw.to_string()
    }
}

/// Resolves a track to a playable local path or HTTP stream URL without needing WebView2/JS.
pub fn resolve_track_stream(
    track: &QueueTrack,
    api_base: &str,
    default_platform: &str,
    quality: &str,
) -> Result<(String, Option<String>), String> {
    let base = api_base.trim_end_matches('/');
    let track_id = track.effective_id();

    // 1. Local file track
    if track.platform.as_deref() == Some("local") {
        if let Some(ref p) = track.path {
            if !p.is_empty() {
                return Ok((strip_local_url_prefix(p), track_id));
            }
        }
        if let Some(ref u) = track.url {
            if !u.is_empty() {
                return Ok((strip_local_url_prefix(u), track_id));
            }
        }
        return Err("Local track has no path or url".to_string());
    }

    if let Some(ref p) = track.path {
        if !p.is_empty() && Path::new(p).exists() {
            return Ok((p.clone(), track_id));
        }
    }

    // 2. Preloaded direct stream URL (non-HLS)
    if let Some(ref pre) = track.preloaded_stream_url {
        if !pre.is_empty() && !pre.contains(".m3u8") {
            return Ok((ensure_absolute_url(pre, base), track_id));
        }
    }

    let resolved_id = track_id.clone().unwrap_or_default();
    let resolved_url = track.url.clone().unwrap_or_default();
    let track_platform = track.platform.as_deref().unwrap_or(default_platform);
    let is_soundcloud = track_platform == "soundcloud"
        || resolved_url.contains("soundcloud.com")
        || (default_platform == "soundcloud" && !resolved_url.contains("youtube.com"));

    // 3. SoundCloud resolution via /api/stream-info -> /api/stream fallback for HLS
    if is_soundcloud {
        let search_q = track.search_query.clone().unwrap_or_else(|| {
            if track.artist.is_empty() {
                track.title.clone()
            } else {
                format!("{} - {}", track.artist, track.title)
            }
        });
        let info_url = format!(
            "{}/api/stream-info?url={}&id={}&platform=soundcloud&title={}&artist={}&duration={}&thumbnail={}&quality={}&q={}",
            base,
            urlencoding::encode(&resolved_url),
            urlencoding::encode(&resolved_id),
            urlencoding::encode(&track.title),
            urlencoding::encode(&track.artist),
            track.duration.unwrap_or(0.0),
            urlencoding::encode(track.thumbnail.as_deref().unwrap_or("")),
            urlencoding::encode(quality),
            urlencoding::encode(&search_q),
        );

        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(18))
            .user_agent(concat!("Raser/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|e| format!("HTTP client error: {e}"))?;

        if let Ok(resp) = client.get(&info_url).send() {
            if let Ok(json) = resp.json::<serde_json::Value>() {
                if json.get("success").and_then(|v| v.as_bool()).unwrap_or(false) {
                    if let Some(raw_stream) = json.get("streamUrl").and_then(|v| v.as_str()) {
                        let abs = ensure_absolute_url(raw_stream, base);
                        let is_hls = json.get("isHls").and_then(|v| v.as_bool()).unwrap_or(false)
                            || abs.contains(".m3u8");
                        let final_id = json
                            .get("resolvedId")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string())
                            .or(track_id);
                        if is_hls {
                            let proxy = format!(
                                "{}/api/stream?url={}&platform=soundcloud&id={}",
                                base,
                                urlencoding::encode(if resolved_url.is_empty() { &abs } else { &resolved_url }),
                                urlencoding::encode(&resolved_id),
                            );
                            return Ok((proxy, final_id));
                        }
                        return Ok((abs, final_id));
                    }
                }
            }
        }

        // Fallback to direct /api/stream proxy
        let fallback = format!(
            "{}/api/stream?url={}&platform=soundcloud&id={}&title={}&artist={}",
            base,
            urlencoding::encode(&resolved_url),
            urlencoding::encode(&resolved_id),
            urlencoding::encode(&track.title),
            urlencoding::encode(&track.artist),
        );
        return Ok((fallback, track_id));
    }

    // 4. YouTube / YouTube Music / default stream URL (matches app.js line 5114)
    let mut play_url = format!(
        "{}/api/stream?id={}&title={}&artist={}",
        base,
        urlencoding::encode(&resolved_id),
        urlencoding::encode(&track.title),
        urlencoding::encode(&track.artist),
    );
    if let Some(dur) = track.duration {
        if dur > 0.0 {
            play_url.push_str(&format!("&duration={}", dur));
        }
    }
    if !resolved_url.is_empty() {
        play_url.push_str(&format!("&url={}", urlencoding::encode(&resolved_url)));
    }
    if let Some(ref plat) = track.platform {
        if !plat.is_empty() {
            play_url.push_str(&format!("&platform={}", urlencoding::encode(plat)));
        }
    }
    Ok((play_url, track_id))
}

pub fn update_discord_for_track(bg_queue: &BackgroundQueue, track: &QueueTrack, playing: bool) {
    let (enabled, artist_in_status) = bg_queue.discord_settings();
    if !enabled {
        return;
    }
    if !playing {
        discord::clear_presence();
        return;
    }
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    discord::update_presence(discord::DiscordTrackInfo {
        title: Some(track.title.clone()),
        artist: Some(track.artist.clone()),
        thumbnail: track.thumbnail.clone(),
        start_timestamp: Some(now),
        duration: track.duration,
        url: track.url.clone(),
        artist_in_status: Some(artist_in_status),
    });
}

/// Advances playback completely in Rust while the WebView2 window is destroyed (headless tray mode).
pub fn spawn_headless_step(
    bg_queue: Arc<BackgroundQueue>,
    audio_core: Arc<audio_core::AudioCore>,
    direction: i32,
    natural_end: bool,
) {
    if !bg_queue.try_begin_advance() {
        return;
    }

    tauri::async_runtime::spawn(async move {
        struct Guard(Arc<BackgroundQueue>);
        impl Drop for Guard {
            fn drop(&mut self) {
                self.0.finish_advance();
            }
        }
        let _guard = Guard(Arc::clone(&bg_queue));

        // Previous button: if >3s into track, restart current track just like JS prevTrack()
        if direction < 0 && !natural_end {
            let st = audio_core.get_state();
            if st.position > 3.0 {
                let _ = audio_core.seek_async(0.0).await;
                let _ = audio_core.play();
                return;
            }
        }

        let Some((track, _idx, api_base, default_platform, quality)) =
            bg_queue.pick_step(direction, natural_end)
        else {
            discord::clear_presence();
            return;
        };

        let track_for_resolve = track.clone();
        let resolved = tauri::async_runtime::spawn_blocking(move || {
            resolve_track_stream(&track_for_resolve, &api_base, &default_platform, &quality)
        })
        .await;

        let (stream_url, track_id) = match resolved {
            Ok(Ok(pair)) => pair,
            _ => return,
        };

        if audio_core
            .load_audio_async(stream_url, Some(0.0), track.duration, true, track_id)
            .await
            .is_ok()
        {
            update_discord_for_track(&bg_queue, &track, true);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_track(id: &str, title: &str) -> QueueTrack {
        QueueTrack {
            id: id.to_string(),
            resolved_id: None,
            title: title.to_string(),
            artist: "Artist".to_string(),
            thumbnail: None,
            duration: Some(180.0),
            platform: Some("ytmusic".to_string()),
            url: None,
            path: None,
            preloaded_stream_url: None,
            search_query: None,
            extra: serde_json::Map::new(),
        }
    }

    #[test]
    fn pick_step_handles_sequential_and_repeat_modes() {
        let q = BackgroundQueue::new();
        q.update_snapshot(PlaybackSnapshot {
            queue: vec![sample_track("a", "One"), sample_track("b", "Two")],
            queue_index: 0,
            repeat_mode: "off".to_string(),
            ..Default::default()
        });

        let (t1, idx1, _, _, _) = q.pick_step(1, true).expect("next track");
        assert_eq!(t1.id, "b");
        assert_eq!(idx1, 1);

        // At end with repeat off -> stops
        assert!(q.pick_step(1, true).is_none());

        // Repeat all -> wraps to 0
        q.update_snapshot(PlaybackSnapshot {
            queue: vec![sample_track("a", "One"), sample_track("b", "Two")],
            queue_index: 1,
            repeat_mode: "all".to_string(),
            ..Default::default()
        });
        let (t_wrap, idx_wrap, _, _, _) = q.pick_step(1, true).expect("wrap track");
        assert_eq!(t_wrap.id, "a");
        assert_eq!(idx_wrap, 0);

        // Repeat one on natural end -> stays on same track
        q.update_snapshot(PlaybackSnapshot {
            queue: vec![sample_track("a", "One"), sample_track("b", "Two")],
            queue_index: 1,
            repeat_mode: "one".to_string(),
            ..Default::default()
        });
        let (t_one, idx_one, _, _, _) = q.pick_step(1, true).expect("repeat one");
        assert_eq!(t_one.id, "b");
        assert_eq!(idx_one, 1);
    }

    #[test]
    fn resolve_track_stream_builds_expected_urls() {
        let yt = sample_track("abc123", "Hello World");
        let (url, tid) = resolve_track_stream(&yt, "https://app.raserapp.online", "ytmusic", "high").unwrap();
        assert!(url.starts_with("https://app.raserapp.online/api/stream?id=abc123"));
        assert!(url.contains("title=Hello%20World"));
        assert_eq!(tid.as_deref(), Some("abc123"));

        let mut local = sample_track("loc1", "Local Song");
        local.platform = Some("local".to_string());
        local.path = Some("file:///C:/Music/My%20Song.mp3".to_string());
        let (local_path, _) = resolve_track_stream(&local, "https://app.raserapp.online", "ytmusic", "high").unwrap();
        assert_eq!(local_path, "C:\\Music\\My Song.mp3");
    }
}
