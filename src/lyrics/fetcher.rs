use super::lrc::{parse_lrc, ParsedLyrics};
use super::rtl::is_rtl_text;
use crate::providers::youtube::extract_youtube_id;
use crate::storage::{self, TrackRecord};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CachedLyricsRecord {
    pub raw_lrc: String,
    pub plain_text: String,
    pub intro_offset_secs: f64,
    pub source: String,
}

pub fn compute_smart_intro_offset(first_vocal_secs: Option<f64>, first_lrc_secs: Option<f64>) -> f64 {
    let (Some(vocal), Some(lrc)) = (first_vocal_secs, first_lrc_secs) else {
        return 0.0;
    };
    let diff = vocal - lrc;
    if diff.abs() >= 0.4 && diff.abs() <= 90.0 {
        diff
    } else {
        0.0
    }
}

pub fn apply_intro_offset(mut parsed: ParsedLyrics, offset_secs: f64) -> ParsedLyrics {
    if offset_secs.abs() < 0.05 {
        return parsed;
    }
    for line in &mut parsed.lines {
        line.time_secs = (line.time_secs + offset_secs).max(0.0);
        for w in &mut line.words {
            w.time_secs = (w.time_secs + offset_secs).max(0.0);
        }
    }
    parsed
}

pub fn parse_json3_first_vocal_secs(json3: &Value) -> Option<f64> {
    let events = json3.get("events")?.as_array()?;
    for ev in events {
        let segs = ev.get("segs").and_then(|v| v.as_array());
        let text: String = segs
            .map(|arr| {
                arr.iter()
                    .filter_map(|s| s.get("utf8").and_then(|u| u.as_str()))
                    .collect()
            })
            .unwrap_or_default();
        let trimmed = text.trim();
        if trimmed.is_empty() || trimmed.starts_with('[') || trimmed == "♪" {
            continue;
        }
        if let Some(ms) = ev.get("tStartMs").and_then(|v| v.as_f64()) {
            return Some(ms / 1000.0);
        }
    }
    None
}

async fn fetch_yt_first_vocal_secs(client: &reqwest::Client, video_id: &str) -> Option<f64> {
    let payload = json!({
        "context": {
            "client": {
                "clientName": "IOS",
                "clientVersion": "20.10.4",
                "hl": "en",
                "gl": "US"
            }
        },
        "videoId": video_id
    });
    let res = client
        .post("https://www.youtube.com/youtubei/v1/player?prettyPrint=false")
        .json(&payload)
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await
        .ok()?;
    let json: Value = res.json().await.ok()?;
    let base_url = json
        .pointer("/captions/playerCaptionsTracklistRenderer/captionTracks/0/baseUrl")
        .and_then(|v| v.as_str())?;
    let cap_url = format!("{}&fmt=json3", base_url);
    let cap_res = client
        .get(cap_url)
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await
        .ok()?;
    let cap_json: Value = cap_res.json().await.ok()?;
    parse_json3_first_vocal_secs(&cap_json)
}

pub async fn fetch_lyrics_for_track(
    client: &reqwest::Client,
    track: &TrackRecord,
) -> Result<ParsedLyrics, String> {
    let cache_key = if !track.id.trim().is_empty() {
        track.id.clone()
    } else {
        track.canonical_id()
    };

    if let Some(cached_str) = storage::get_cached_lyrics(&cache_key) {
        if let Ok(rec) = serde_json::from_str::<CachedLyricsRecord>(&cached_str) {
            if !rec.raw_lrc.trim().is_empty() {
                let parsed = parse_lrc(&rec.raw_lrc);
                return Ok(apply_intro_offset(parsed, rec.intro_offset_secs));
            } else if !rec.plain_text.trim().is_empty() {
                return Ok(ParsedLyrics {
                    offset_ms: 0,
                    lines: Vec::new(),
                    plain_text: rec.plain_text.clone(),
                    is_instrumental: false,
                    is_rtl: is_rtl_text(&rec.plain_text),
                });
            }
        }
    }

    let mut raw_lrc = String::new();
    let mut plain_text = String::new();
    let dur_param = if track.duration > 0.0 {
        format!("&duration={}", track.duration.round() as u64)
    } else {
        String::new()
    };
    let get_url = format!(
        "https://lrclib.net/api/get?track_name={}&artist_name={}{}",
        urlencoding::encode(track.title.trim()),
        urlencoding::encode(track.artist.trim()),
        dur_param
    );

    if let Ok(res) = client
        .get(&get_url)
        .header("User-Agent", "RaserMusicApp/1.2")
        .timeout(std::time::Duration::from_secs(6))
        .send()
        .await
    {
        if res.status().is_success() {
            if let Ok(j) = res.json::<Value>().await {
                raw_lrc = j
                    .get("syncedLyrics")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                plain_text = j
                    .get("plainLyrics")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
            }
        }
    }

    if raw_lrc.trim().is_empty() && plain_text.trim().is_empty() {
        let q = format!("{} {}", track.artist.trim(), track.title.trim());
        let search_url = format!(
            "https://lrclib.net/api/search?q={}",
            urlencoding::encode(q.trim())
        );
        if let Ok(res) = client
            .get(&search_url)
            .header("User-Agent", "RaserMusicApp/1.2")
            .timeout(std::time::Duration::from_secs(6))
            .send()
            .await
        {
            if res.status().is_success() {
                if let Ok(Value::Array(arr)) = res.json::<Value>().await {
                    if let Some(first) = arr.first() {
                        raw_lrc = first
                            .get("syncedLyrics")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        plain_text = first
                            .get("plainLyrics")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                    }
                }
            }
        }
    }

    if raw_lrc.trim().is_empty() && plain_text.trim().is_empty() {
        return Err("No lyrics found".to_string());
    }

    let mut parsed = if !raw_lrc.trim().is_empty() {
        parse_lrc(&raw_lrc)
    } else {
        ParsedLyrics {
            offset_ms: 0,
            lines: Vec::new(),
            plain_text: plain_text.clone(),
            is_instrumental: false,
            is_rtl: is_rtl_text(&plain_text),
        }
    };

    let mut intro_offset = 0.0;
    if !parsed.lines.is_empty() {
        if let Some(vid) = extract_youtube_id(&track.id).or_else(|| extract_youtube_id(&track.url))
        {
            let first_vocal = fetch_yt_first_vocal_secs(client, &vid).await;
            let first_lrc = parsed.lines.first().map(|l| l.time_secs);
            intro_offset = compute_smart_intro_offset(first_vocal, first_lrc);
            parsed = apply_intro_offset(parsed, intro_offset);
        }
    }

    let rec = CachedLyricsRecord {
        raw_lrc,
        plain_text: parsed.plain_text.clone(),
        intro_offset_secs: intro_offset,
        source: "lrclib".to_string(),
    };
    if let Ok(serialized) = serde_json::to_string(&rec) {
        storage::save_cached_lyrics(&cache_key, &serialized);
    }

    Ok(parsed)
}
