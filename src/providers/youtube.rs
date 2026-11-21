use super::{api_base_url, AlbumDetail, ArtistDetail, AudioQuality, DiscoverFeed, Platform, ResolvedStream, TrackItem};
use crate::storage::TrackRecord;
use serde_json::{json, Value};

pub fn is_valid_youtube_id(s: &str) -> bool {
    let bytes = s.as_bytes();
    bytes.len() == 11
        && bytes
            .iter()
            .all(|&b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

pub fn extract_youtube_id(input: &str) -> Option<String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return None;
    }
    if is_valid_youtube_id(trimmed) {
        return Some(trimmed.to_string());
    }

    for marker in ["v=", "youtu.be/", "/shorts/", "/embed/", "/live/"] {
        if let Some(pos) = trimmed.find(marker) {
            let rest = &trimmed[pos + marker.len()..];
            let cand: String = rest
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
                .collect();
            if is_valid_youtube_id(&cand) {
                return Some(cand);
            }
        }
    }
    None
}

pub fn stub_track_from_id(video_id: &str, platform: Platform) -> TrackItem {
    TrackItem {
        id: video_id.to_string(),
        title: format!("YouTube Track ({})", video_id),
        artist: "YouTube".to_string(),
        album: None,
        duration_secs: 0.0,
        thumbnail: Some(format!("https://i.ytimg.com/vi/{}/hqdefault.jpg", video_id)),
        url: format!("https://www.youtube.com/watch?v={}", video_id),
        platform,
    }
}

pub fn normalize_ytm_thumbnail(url: &str) -> String {
    let u = url.trim();
    if u.is_empty() {
        return String::new();
    }
    if let Some(eq_idx) = u.rfind("=w") {
        let suffix = &u[eq_idx + 2..];
        if suffix.contains("-h") {
            return format!("{}=w544-h544", &u[..eq_idx]);
        }
    }
    u.to_string()
}

pub fn parse_ytm_duration(text: &str) -> Option<f64> {
    let clean = text.trim();
    if clean.is_empty() {
        return None;
    }
    let parts: Vec<&str> = clean.split(':').collect();
    if parts.len() < 2 || parts.len() > 3 {
        return None;
    }
    let nums: Vec<u64> = parts
        .iter()
        .map(|p| p.trim().parse::<u64>())
        .collect::<Result<Vec<_>, _>>()
        .ok()?;
    let secs = match nums.as_slice() {
        [m, s] => m * 60 + s,
        [h, m, s] => h * 3600 + m * 60 + s,
        _ => return None,
    };
    if secs > 0 {
        Some(secs as f64)
    } else {
        None
    }
}

fn is_non_song_subtitle(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.contains("bölüm")
        || lower.contains("bolum")
        || lower.contains("episode")
        || lower.contains("podcast")
        || lower.contains("abone")
        || lower.contains("subscriber")
        || lower.contains("aylık kitle")
        || lower.contains("monthly audience")
}

fn runs_text(runs: Option<&Value>) -> String {
    let Some(Value::Array(arr)) = runs else {
        return String::new();
    };
    arr.iter()
        .filter_map(|r| r.get("text").and_then(|v| v.as_str()))
        .collect::<String>()
        .trim()
        .to_string()
}

fn split_subtitle_segments(runs: Option<&Value>) -> Vec<String> {
    let Some(Value::Array(arr)) = runs else {
        return Vec::new();
    };
    let mut segments = Vec::new();
    let mut cur = String::new();
    for r in arr {
        let text = r.get("text").and_then(|v| v.as_str()).unwrap_or("");
        if text == " • " {
            let trimmed = cur.trim().to_string();
            if !trimmed.is_empty() {
                segments.push(trimmed);
            }
            cur.clear();
        } else {
            cur.push_str(text);
        }
    }
    let trimmed = cur.trim().to_string();
    if !trimmed.is_empty() {
        segments.push(trimmed);
    }
    segments
        .into_iter()
        .filter(|s| {
            let l = s.trim().to_ascii_lowercase();
            !l.is_empty() && l != "song" && l != "şarkı" && l != "video"
        })
        .collect()
}

fn extract_last_thumbnail(node: Option<&Value>, video_id: &str) -> String {
    let raw = node
        .and_then(|v| v.get("musicThumbnailRenderer"))
        .and_then(|v| v.get("thumbnail"))
        .and_then(|v| v.get("thumbnails"))
        .and_then(|v| v.as_array())
        .and_then(|arr| arr.last())
        .and_then(|item| item.get("url"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| format!("https://i.ytimg.com/vi/{}/hqdefault.jpg", video_id));
    normalize_ytm_thumbnail(&raw)
}

fn clean_artist_name(raw: &str) -> String {
    let trimmed = raw.trim();
    if let Some(stripped) = trimmed.strip_suffix(" - Topic") {
        return stripped.trim().to_string();
    }
    if let Some(stripped) = trimmed.strip_suffix("- Topic") {
        return stripped.trim().to_string();
    }
    if trimmed.is_empty() {
        "Unknown Artist".to_string()
    } else {
        trimmed.to_string()
    }
}

pub fn parse_ytm_search_json(json: &Value) -> Vec<TrackItem> {
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();

    let sections = json
        .pointer("/contents/tabbedSearchResultsRenderer/tabs/0/tabRenderer/content/sectionListRenderer/contents")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    // Phase 1: Top Result card (`musicCardShelfRenderer`)
    for section in &sections {
        let Some(card) = section.get("musicCardShelfRenderer") else {
            continue;
        };
        let title = runs_text(card.pointer("/title/runs"));
        let mut video_id = card
            .pointer("/onTap/watchEndpoint/videoId")
            .and_then(|v| v.as_str())
            .filter(|s| is_valid_youtube_id(s))
            .map(|s| s.to_string());

        if video_id.is_none() {
            if let Some(buttons) = card.get("buttons").and_then(|v| v.as_array()) {
                for b in buttons {
                    if let Some(vid) = b
                        .pointer("/buttonRenderer/command/watchEndpoint/videoId")
                        .and_then(|v| v.as_str())
                        .filter(|s| is_valid_youtube_id(s))
                    {
                        video_id = Some(vid.to_string());
                        break;
                    }
                }
            }
        }

        if let Some(vid) = video_id {
            if !title.is_empty() && seen.insert(vid.clone()) {
                let segments = split_subtitle_segments(card.pointer("/subtitle/runs"));
                let mut duration = 0.0;
                let mut artist = String::from("Unknown Artist");
                for seg in &segments {
                    if let Some(d) = parse_ytm_duration(seg) {
                        duration = d;
                    } else if artist == "Unknown Artist" {
                        let lower = seg.to_ascii_lowercase();
                        if !lower.contains("views")
                            && !lower.contains("plays")
                            && !lower.contains("görüntüleme")
                            && !lower.contains("dinlenme")
                        {
                            artist = clean_artist_name(seg);
                        }
                    }
                }
                let thumb = extract_last_thumbnail(card.get("thumbnail"), &vid);
                out.push(TrackItem {
                    id: vid.clone(),
                    title,
                    artist,
                    album: None,
                    duration_secs: if duration > 0.0 { duration } else { 180.0 },
                    thumbnail: Some(thumb),
                    url: format!("https://www.youtube.com/watch?v={}", vid),
                    platform: Platform::YouTubeMusic,
                });
            }
        }
    }

    // Phase 2: Standard list items (`musicResponsiveListItemRenderer`)
    for section in &sections {
        let mut items: Vec<&Value> = Vec::new();
        let mut shelf_artist_fallback = String::new();
        if let Some(card) = section.get("musicCardShelfRenderer") {
            shelf_artist_fallback = runs_text(card.pointer("/title/runs"));
            if let Some(arr) = card.get("contents").and_then(|v| v.as_array()) {
                items.extend(arr.iter());
            }
        }
        if let Some(arr) = section
            .pointer("/musicShelfRenderer/contents")
            .and_then(|v| v.as_array())
        {
            items.extend(arr.iter());
        }
        if let Some(arr) = section
            .pointer("/itemSectionRenderer/contents")
            .and_then(|v| v.as_array())
        {
            items.extend(arr.iter());
        }

        for wrap in items {
            let Some(r) = wrap.get("musicResponsiveListItemRenderer") else {
                continue;
            };
            let cols = r
                .get("flexColumns")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default();
            let runs_of = |idx: usize| -> Option<&Value> {
                cols.get(idx)?.pointer("/musicResponsiveListItemFlexColumnRenderer/text/runs")
            };

            let col1_text = runs_text(runs_of(1));
            if is_non_song_subtitle(&col1_text) {
                continue;
            }

            let video_id = r
                .pointer("/playlistItemData/videoId")
                .and_then(|v| v.as_str())
                .or_else(|| {
                    r.pointer("/overlay/musicItemThumbnailOverlayRenderer/content/musicPlayButtonRenderer/playNavigationEndpoint/watchEndpoint/videoId")
                        .and_then(|v| v.as_str())
                });

            let Some(vid) = video_id.filter(|s| is_valid_youtube_id(s)) else {
                continue;
            };
            if seen.contains(vid) {
                continue;
            }

            let title = runs_text(runs_of(0));
            if title.is_empty() {
                continue;
            }

            let mut clean = split_subtitle_segments(runs_of(1));
            let mut duration = 0.0;
            if let Some(last) = clean.last() {
                if let Some(d) = parse_ytm_duration(last) {
                    duration = d;
                    clean.pop();
                }
            }

            let artist = clean
                .first()
                .map(|s| clean_artist_name(s))
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| {
                    if shelf_artist_fallback.is_empty() {
                        "Unknown Artist".to_string()
                    } else {
                        clean_artist_name(&shelf_artist_fallback)
                    }
                });
            let album = clean.get(1).cloned().filter(|s| !s.is_empty());
            let thumb = extract_last_thumbnail(r.get("thumbnail"), vid);

            seen.insert(vid.to_string());
            out.push(TrackItem {
                id: vid.to_string(),
                title,
                artist,
                album,
                duration_secs: if duration > 0.0 { duration } else { 180.0 },
                thumbnail: Some(thumb),
                url: format!("https://www.youtube.com/watch?v={}", vid),
                platform: Platform::YouTubeMusic,
            });
        }
    }

    out
}

pub async fn search_ytm(client: &reqwest::Client, query: &str) -> Result<Vec<TrackItem>, String> {
    for with_params in [false, true] {
        let mut payload = json!({
            "context": {
                "client": {
                    "clientName": "WEB_REMIX",
                    "clientVersion": "1.20240424.01.00",
                    "hl": "tr",
                    "gl": "TR"
                }
            },
            "query": query
        });
        if with_params {
            payload["params"] = json!("Eg-KAQwIABAAGAAgACgB");
        }

        let res = client
            .post("https://music.youtube.com/youtubei/v1/search")
            .header("Content-Type", "application/json")
            .header(
                "User-Agent",
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36",
            )
            .json(&payload)
            .send()
            .await
            .map_err(|e| e.to_string())?;

        if !res.status().is_success() {
            continue;
        }
        let body: Value = res.json().await.map_err(|e| e.to_string())?;
        let songs = parse_ytm_search_json(&body);
        if !songs.is_empty() {
            return Ok(songs);
        }
    }
    Ok(Vec::new())
}

pub fn extract_video_ids_from_web_search(raw_json: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let needle = "\"videoId\":\"";
    let mut idx = 0;
    while let Some(pos) = raw_json[idx..].find(needle) {
        let start = idx + pos + needle.len();
        if start + 11 <= raw_json.len() {
            let cand = &raw_json[start..start + 11];
            if is_valid_youtube_id(cand) && seen.insert(cand.to_string()) {
                out.push(cand.to_string());
            }
        }
        idx = start;
    }
    out
}

pub async fn search_youtube_web(
    client: &reqwest::Client,
    query: &str,
) -> Result<Vec<TrackItem>, String> {
    let payload = json!({
        "context": {
            "client": {
                "clientName": "WEB",
                "clientVersion": "2.20240424.01.00",
                "hl": "en",
                "gl": "US"
            }
        },
        "query": query
    });
    let res = client
        .post("https://www.youtube.com/youtubei/v1/search")
        .header("Content-Type", "application/json")
        .json(&payload)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let raw = res.text().await.map_err(|e| e.to_string())?;
    let ids = extract_video_ids_from_web_search(&raw);
    Ok(ids
        .into_iter()
        .take(15)
        .map(|id| stub_track_from_id(&id, Platform::YouTube))
        .collect())
}

pub async fn search_itunes(
    client: &reqwest::Client,
    query: &str,
    limit: usize,
) -> Result<Vec<TrackItem>, String> {
    let url = format!(
        "https://itunes.apple.com/search?term={}&entity=song&limit={}",
        urlencoding::encode(query),
        limit.clamp(1, 25)
    );
    let res = client.get(url).send().await.map_err(|e| e.to_string())?;
    let data: Value = res.json().await.map_err(|e| e.to_string())?;
    let results = data
        .get("results")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let mut out = Vec::new();
    for item in results {
        let title = item
            .get("trackName")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        let artist = item
            .get("artistName")
            .and_then(|v| v.as_str())
            .unwrap_or("Unknown Artist")
            .trim()
            .to_string();
        if title.is_empty() {
            continue;
        }
        let dur = item
            .get("trackTimeMillis")
            .and_then(|v| v.as_f64())
            .map(|ms| (ms / 1000.0).round())
            .unwrap_or(180.0);
        let thumb = item
            .get("artworkUrl100")
            .and_then(|v| v.as_str())
            .map(|u| u.replace("100x100", "600x600"));
        let id = item
            .get("trackId")
            .and_then(|v| v.as_u64())
            .map(|n| format!("itunes_{}", n))
            .unwrap_or_else(|| format!("itunes_{}", out.len()));
        out.push(TrackItem {
            id,
            title,
            artist,
            album: item
                .get("collectionName")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            duration_secs: dur,
            thumbnail: thumb,
            url: item
                .get("previewUrl")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            platform: Platform::YouTubeMusic,
        });
    }
    Ok(out)
}

pub fn parse_ios_player_stream(
    json: &Value,
    quality: AudioQuality,
) -> Result<ResolvedStream, String> {
    if let Some(status) = json
        .pointer("/playabilityStatus/status")
        .and_then(|v| v.as_str())
    {
        if status != "OK" {
            let reason = json
                .pointer("/playabilityStatus/reason")
                .and_then(|v| v.as_str())
                .unwrap_or("restricted");
            return Err(format!("Innertube unplayable: {} ({})", status, reason));
        }
    }

    let formats = json
        .pointer("/streamingData/adaptiveFormats")
        .and_then(|v| v.as_array())
        .ok_or_else(|| "Innertube response missing adaptiveFormats".to_string())?;

    let mut audio_formats: Vec<&Value> = formats
        .iter()
        .filter(|f| {
            let has_url = f.get("url").and_then(|v| v.as_str()).is_some_and(|u| !u.is_empty());
            let is_audio = f
                .get("mimeType")
                .and_then(|v| v.as_str())
                .is_some_and(|m| m.starts_with("audio/"));
            has_url && is_audio
        })
        .collect();

    if audio_formats.is_empty() {
        return Err("No unencrypted audio formats found".to_string());
    }

    let find_itag = |target: u64| -> Option<&Value> {
        audio_formats
            .iter()
            .copied()
            .find(|f| f.get("itag").and_then(|v| v.as_u64()) == Some(target))
    };

    let selected = match quality {
        AudioQuality::Max => find_itag(251).or_else(|| find_itag(140)),
        AudioQuality::High | AudioQuality::Medium => find_itag(140).or_else(|| find_itag(251)),
        AudioQuality::Low => {
            audio_formats.sort_by_key(|f| {
                f.get("bitrate")
                    .or_else(|| f.get("audioBitrate"))
                    .and_then(|v| v.as_u64())
                    .unwrap_or(u64::MAX)
            });
            audio_formats.first().copied()
        }
    }
    .unwrap_or_else(|| {
        audio_formats.sort_by_key(|f| {
            std::cmp::Reverse(
                f.get("bitrate")
                    .or_else(|| f.get("audioBitrate"))
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0),
            )
        });
        audio_formats[0]
    });

    let stream_url = selected
        .get("url")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "Selected format missing url".to_string())?
        .to_string();
    let itag = selected
        .get("itag")
        .and_then(|v| v.as_u64())
        .map(|n| n as u32);
    let mime_type = selected
        .get("mimeType")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let duration_secs = json
        .pointer("/videoDetails/lengthSeconds")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse::<f64>().ok());

    Ok(ResolvedStream {
        stream_url,
        itag,
        mime_type,
        duration_secs,
        is_hls: false,
    })
}

pub async fn resolve_youtube_stream(
    client: &reqwest::Client,
    video_id: &str,
    quality: AudioQuality,
) -> Result<ResolvedStream, String> {
    if !is_valid_youtube_id(video_id) {
        return Err(format!("Invalid YouTube videoId: {}", video_id));
    }

    // Try direct IOS InnerTube first if track is likely playable directly;
    // note RaserWeb/api/stream handles >1MB Range stitching when IOS URLs cap at 1MB.
    let api_base = api_base_url();
    let proxy_url = format!(
        "{}/api/stream?id={}&platform=ytmusic&quality={}",
        api_base,
        urlencoding::encode(video_id),
        quality.as_str()
    );

    // Quick probe against {api_base}/api/stream-info if hosted backend is configured/reachable,
    // falling back to direct IOS Innertube /player.
    let info_url = format!(
        "{}/api/stream-info?id={}&platform=ytmusic&quality={}",
        api_base,
        urlencoding::encode(video_id),
        quality.as_str()
    );
    if let Ok(resp) = client
        .get(&info_url)
        .timeout(std::time::Duration::from_secs(6))
        .send()
        .await
    {
        if resp.status().is_success() {
            if let Ok(json) = resp.json::<Value>().await {
                if let Some(u) = json.get("url").and_then(|v| v.as_str()) {
                    if !u.is_empty() {
                        let resolved_url = if u.starts_with('/') {
                            format!("{}{}", api_base, u)
                        } else {
                            u.to_string()
                        };
                        return Ok(ResolvedStream {
                            stream_url: resolved_url,
                            itag: None,
                            mime_type: json
                                .get("mimeType")
                                .and_then(|v| v.as_str())
                                .map(|s| s.to_string()),
                            duration_secs: json.get("duration").and_then(|v| v.as_f64()),
                            is_hls: false,
                        });
                    }
                }
            }
        }
    }

    let payload = json!({
        "context": {
            "client": {
                "clientName": "IOS",
                "clientVersion": "20.10.4",
                "deviceMake": "Apple",
                "deviceModel": "iPhone16,2",
                "userAgent": "com.google.ios.youtube/20.10.4",
                "osName": "iOS",
                "osVersion": "18.3.2.22D82",
                "hl": "en",
                "gl": "US"
            }
        },
        "videoId": video_id
    });

    if let Ok(res) = client
        .post("https://www.youtube.com/youtubei/v1/player?prettyPrint=false")
        .header("Content-Type", "application/json")
        .header(
            "User-Agent",
            "com.google.ios.youtube/20.10.4 (iPhone16,2; U; CPU iOS 18_3_2 like Mac OS X;)",
        )
        .json(&payload)
        .send()
        .await
    {
        if res.status().is_success() {
            if let Ok(json) = res.json::<Value>().await {
                if let Ok(stream) = parse_ios_player_stream(&json, quality) {
                    return Ok(stream);
                }
            }
        }
    }

    Ok(ResolvedStream {
        stream_url: proxy_url,
        itag: None,
        mime_type: None,
        duration_secs: None,
        is_hls: false,
    })
}

fn parse_backend_track_array(arr: &[Value], default_platform: Platform) -> Vec<TrackItem> {
    let mut out = Vec::new();
    for item in arr {
        let title = item
            .get("title")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        let id = item
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        if title.is_empty() || id.is_empty() {
            continue;
        }
        let artist = item
            .get("artist")
            .and_then(|v| v.as_str())
            .unwrap_or("Unknown Artist")
            .to_string();
        let album = item
            .get("album")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());
        let duration_secs = item
            .get("duration")
            .and_then(|v| v.as_f64())
            .unwrap_or(180.0);
        let thumbnail = item
            .get("thumbnail")
            .or_else(|| item.get("cover"))
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());
        let url = item
            .get("url")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("https://www.youtube.com/watch?v={}", id));
        let platform = item
            .get("platform")
            .and_then(|v| v.as_str())
            .map(Platform::from_str_lossy)
            .unwrap_or(default_platform);
        out.push(TrackItem {
            id,
            title,
            artist,
            album,
            duration_secs,
            thumbnail,
            url,
            platform,
        });
    }
    out
}

pub async fn fetch_discover_feed(
    client: &reqwest::Client,
    history: &[TrackRecord],
) -> Result<DiscoverFeed, String> {
    let base = api_base_url();
    let mut trending = Vec::new();
    let mut quick_picks = Vec::new();

    if let Ok(res) = client
        .get(format!("{}/api/trending?limit=16", base))
        .timeout(std::time::Duration::from_secs(7))
        .send()
        .await
    {
        if res.status().is_success() {
            if let Ok(json) = res.json::<Value>().await {
                if let Some(arr) = json
                    .get("tracks")
                    .or_else(|| json.get("items"))
                    .and_then(|v| v.as_array())
                {
                    trending = parse_backend_track_array(arr, Platform::YouTubeMusic);
                }
            }
        }
    }

    if !history.is_empty() {
        let seed: Vec<Value> = history
            .iter()
            .take(12)
            .map(|t| json!({ "id": t.id, "title": t.title, "artist": t.artist }))
            .collect();
        if let Ok(res) = client
            .post(format!("{}/api/vibe", base))
            .json(&json!({ "tracks": seed, "limit": 16 }))
            .timeout(std::time::Duration::from_secs(7))
            .send()
            .await
        {
            if res.status().is_success() {
                if let Ok(json) = res.json::<Value>().await {
                    if let Some(arr) = json
                        .get("tracks")
                        .or_else(|| json.get("items"))
                        .and_then(|v| v.as_array())
                    {
                        quick_picks = parse_backend_track_array(arr, Platform::YouTubeMusic);
                    }
                }
            }
        }
    }

    if trending.is_empty() {
        trending = search_ytm(client, "Top Hits 2026")
            .await
            .unwrap_or_default()
            .into_iter()
            .take(12)
            .collect();
    }

    if quick_picks.is_empty() {
        if let Some(seed) = history.first() {
            let q = format!("{} {}", seed.artist, seed.title);
            quick_picks = search_ytm(client, q.trim())
                .await
                .unwrap_or_default()
                .into_iter()
                .take(10)
                .collect();
        }
    }

    Ok(DiscoverFeed {
        trending,
        quick_picks,
    })
}

pub async fn fetch_artist_detail(
    client: &reqwest::Client,
    artist_name: &str,
) -> Result<ArtistDetail, String> {
    let clean = artist_name.trim();
    if clean.is_empty() {
        return Err("Empty artist name".to_string());
    }
    let base = api_base_url();
    if let Ok(res) = client
        .get(format!("{}/api/artist?name={}", base, urlencoding::encode(clean)))
        .timeout(std::time::Duration::from_secs(7))
        .send()
        .await
    {
        if res.status().is_success() {
            if let Ok(json) = res.json::<Value>().await {
                if let Some(arr) = json.get("tracks").and_then(|v| v.as_array()) {
                    let tracks = parse_backend_track_array(arr, Platform::YouTubeMusic);
                    if !tracks.is_empty() {
                        return Ok(ArtistDetail {
                            name: clean.to_string(),
                            thumbnail: json
                                .get("thumbnail")
                                .and_then(|v| v.as_str())
                                .map(|s| s.to_string())
                                .or_else(|| tracks[0].thumbnail.clone()),
                            top_tracks: tracks,
                        });
                    }
                }
            }
        }
    }

    let tracks = search_ytm(client, clean).await?;
    let thumb = tracks.first().and_then(|t| t.thumbnail.clone());
    Ok(ArtistDetail {
        name: clean.to_string(),
        thumbnail: thumb,
        top_tracks: tracks,
    })
}

pub async fn fetch_album_detail(
    client: &reqwest::Client,
    album_title: &str,
    artist_hint: &str,
) -> Result<AlbumDetail, String> {
    let q = format!("{} {}", artist_hint.trim(), album_title.trim())
        .trim()
        .to_string();
    let tracks = search_ytm(client, &q).await?;
    let cover = tracks.first().and_then(|t| t.thumbnail.clone());
    Ok(AlbumDetail {
        id: q.clone(),
        title: album_title.trim().to_string(),
        artist: artist_hint.trim().to_string(),
        cover,
        tracks,
    })
}
