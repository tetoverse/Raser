pub mod soundcloud;
pub mod youtube;

use crate::storage::{now_millis, TrackRecord};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Platform {
    Local,
    #[default]
    YouTubeMusic,
    YouTube,
    SoundCloud,
}

impl Platform {
    pub fn as_str(self) -> &'static str {
        match self {
            Platform::Local => "local",
            Platform::YouTubeMusic => "ytmusic",
            Platform::YouTube => "youtube",
            Platform::SoundCloud => "soundcloud",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Platform::Local => "Local",
            Platform::YouTubeMusic => "YT Music",
            Platform::YouTube => "YouTube",
            Platform::SoundCloud => "SoundCloud",
        }
    }

    pub fn from_str_lossy(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "local" => Platform::Local,
            "youtube" | "yt" => Platform::YouTube,
            "soundcloud" | "sc" => Platform::SoundCloud,
            _ => Platform::YouTubeMusic,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum AudioQuality {
    Low,
    Medium,
    #[default]
    High,
    Max,
}

impl AudioQuality {
    pub fn as_str(self) -> &'static str {
        match self {
            AudioQuality::Low => "low",
            AudioQuality::Medium => "medium",
            AudioQuality::High => "high",
            AudioQuality::Max => "max",
        }
    }

    pub fn from_str_lossy(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "low" => AudioQuality::Low,
            "medium" => AudioQuality::Medium,
            "max" | "opus" => AudioQuality::Max,
            _ => AudioQuality::High,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TrackItem {
    pub id: String,
    pub title: String,
    pub artist: String,
    pub album: Option<String>,
    pub duration_secs: f64,
    pub thumbnail: Option<String>,
    pub url: String,
    pub platform: Platform,
}

impl From<TrackItem> for TrackRecord {
    fn from(item: TrackItem) -> Self {
        Self {
            id: item.id,
            title: item.title,
            artist: item.artist,
            album: item.album.unwrap_or_default(),
            duration: item.duration_secs,
            thumbnail: item.thumbnail.clone().unwrap_or_default(),
            cover: item.thumbnail.unwrap_or_default(),
            path: String::new(),
            url: item.url,
            platform: item.platform.as_str().to_string(),
            added_at: Some(now_millis()),
            played_at: None,
            extra: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ResolvedStream {
    pub stream_url: String,
    pub itag: Option<u32>,
    pub mime_type: Option<String>,
    pub duration_secs: Option<f64>,
    pub is_hls: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct DiscoverFeed {
    pub trending: Vec<TrackItem>,
    pub quick_picks: Vec<TrackItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct ArtistDetail {
    pub name: String,
    pub thumbnail: Option<String>,
    pub top_tracks: Vec<TrackItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct AlbumDetail {
    pub id: String,
    pub title: String,
    pub artist: String,
    pub cover: Option<String>,
    pub tracks: Vec<TrackItem>,
}

pub fn api_base_url() -> String {
    if let Ok(base) = std::env::var("RASER_API_BASE") {
        let trimmed = base.trim().trim_end_matches('/');
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }
    "https://app.raserapp.online".to_string()
}

pub fn build_http_client(proxy: Option<&str>) -> Result<reqwest::Client, String> {
    let mut builder = reqwest::Client::builder()
        .timeout(Duration::from_secs(12))
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36");

    if let Some(proxy_str) = proxy.map(str::trim).filter(|s| !s.is_empty()) {
        let parsed = reqwest::Proxy::all(proxy_str)
            .map_err(|e| format!("Invalid proxy URL '{}': {}", proxy_str, e))?;
        builder = builder.proxy(parsed);
    }

    builder.build().map_err(|e| e.to_string())
}

pub async fn search_tracks(
    client: &reqwest::Client,
    query: &str,
    platform: Platform,
    limit: usize,
    offset: usize,
) -> Result<Vec<TrackItem>, String> {
    let q = query.trim();
    if q.is_empty() {
        return Ok(Vec::new());
    }
    let lim = limit.clamp(1, 50);

    match platform {
        Platform::YouTubeMusic => {
            if let Some(vid) = youtube::extract_youtube_id(q) {
                return Ok(vec![youtube::stub_track_from_id(&vid, Platform::YouTubeMusic)]);
            }
            match youtube::search_ytm(client, q).await {
                Ok(items) if !items.is_empty() => Ok(items.into_iter().skip(offset).take(lim).collect()),
                _ => {
                    let web_items = youtube::search_youtube_web(client, q).await.unwrap_or_default();
                    if !web_items.is_empty() {
                        return Ok(web_items.into_iter().skip(offset).take(lim).collect());
                    }
                    let itunes = youtube::search_itunes(client, q, lim).await.unwrap_or_default();
                    Ok(itunes.into_iter().skip(offset).take(lim).collect())
                }
            }
        }
        Platform::YouTube => {
            if let Some(vid) = youtube::extract_youtube_id(q) {
                return Ok(vec![youtube::stub_track_from_id(&vid, Platform::YouTube)]);
            }
            let web_items = youtube::search_youtube_web(client, q).await?;
            if !web_items.is_empty() {
                return Ok(web_items.into_iter().skip(offset).take(lim).collect());
            }
            let ytm_items = youtube::search_ytm(client, q).await.unwrap_or_default();
            Ok(ytm_items.into_iter().skip(offset).take(lim).collect())
        }
        Platform::SoundCloud => {
            let sc_items = soundcloud::search_soundcloud(client, q, lim + offset).await?;
            Ok(sc_items.into_iter().skip(offset).take(lim).collect())
        }
        Platform::Local => Ok(Vec::new()),
    }
}

pub async fn resolve_stream_url(
    client: &reqwest::Client,
    track: &TrackRecord,
    quality: AudioQuality,
) -> Result<ResolvedStream, String> {
    let plat = Platform::from_str_lossy(&track.platform);
    let source = track.playable_source();

    if plat == Platform::Local
        || (!track.path.trim().is_empty() && std::path::Path::new(&track.path).exists())
    {
        return Ok(ResolvedStream {
            stream_url: source,
            itag: None,
            mime_type: None,
            duration_secs: if track.duration > 0.0 {
                Some(track.duration)
            } else {
                None
            },
            is_hls: false,
        });
    }

    if plat == Platform::SoundCloud
        || track.url.contains("soundcloud.com")
        || source.contains("soundcloud.com")
    {
        let target = if !track.url.trim().is_empty() {
            track.url.trim()
        } else {
            track.id.trim()
        };
        let fallback_q = format!("{} {}", track.artist, track.title);
        if let Ok(resolved) =
            soundcloud::resolve_soundcloud_stream(client, target, fallback_q.trim()).await
        {
            return Ok(resolved);
        }
    }

    let maybe_vid = youtube::extract_youtube_id(&track.id)
        .or_else(|| youtube::extract_youtube_id(&track.url))
        .or_else(|| youtube::extract_youtube_id(&source));

    if let Some(vid) = maybe_vid {
        return youtube::resolve_youtube_stream(client, &vid, quality).await;
    }

    // Search fallback for tracks imported without videoId (e.g. Spotify/iTunes stubs)
    let search_q = format!("{} {}", track.artist, track.title).trim().to_string();
    if !search_q.is_empty() {
        let candidates = search_tracks(client, &search_q, Platform::YouTubeMusic, 5, 0).await?;
        if let Some(first) = candidates.into_iter().next() {
            if let Some(vid) = youtube::extract_youtube_id(&first.id) {
                return youtube::resolve_youtube_stream(client, &vid, quality).await;
            }
        }
    }

    if source.starts_with("http://") || source.starts_with("https://") {
        return Ok(ResolvedStream {
            stream_url: source,
            itag: None,
            mime_type: None,
            duration_secs: None,
            is_hls: false,
        });
    }

    Err("Could not resolve playable stream URL".to_string())
}
