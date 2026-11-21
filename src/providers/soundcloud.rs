use super::{Platform, ResolvedStream, TrackItem};
use serde_json::Value;
use std::sync::Mutex;
use std::time::{Duration, Instant};

pub const DEFAULT_SC_CLIENT_ID: &str = "qLcgHhXgeEFFaSolTB9I71LBsqncsD93";

struct ScClientIdCache {
    value: String,
    fetched_at: Option<Instant>,
}

static SC_CLIENT_ID_CACHE: Mutex<ScClientIdCache> = Mutex::new(ScClientIdCache {
    value: String::new(),
    fetched_at: None,
});

pub fn extract_sc_client_id_from_script(js: &str) -> Option<String> {
    let mut search_from = 0usize;
    while let Some(pos) = js[search_from..].find("client_id") {
        let abs = search_from + pos + "client_id".len();
        let tail = &js[abs..];
        let trimmed = tail.trim_start();
        if let Some(after_op) = trimmed
            .strip_prefix(':')
            .or_else(|| trimmed.strip_prefix('='))
        {
            let after_ws = after_op.trim_start();
            let quote = after_ws.chars().next();
            if quote == Some('"') || quote == Some('\'') {
                let inner = &after_ws[1..];
                if let Some(end_q) = inner.find(quote.unwrap()) {
                    let candidate = &inner[..end_q];
                    if candidate.len() == 32
                        && candidate.bytes().all(|b| b.is_ascii_alphanumeric())
                    {
                        return Some(candidate.to_string());
                    }
                }
            }
        }
        search_from = abs;
    }
    None
}

pub fn extract_sc_bundle_urls(html: &str) -> Vec<String> {
    let mut out = Vec::new();
    let prefix = "https://a-v2.sndcdn.com/assets/";
    let mut idx = 0usize;
    while let Some(pos) = html[idx..].find(prefix) {
        let start = idx + pos;
        if let Some(end) = html[start..].find(".js") {
            let url = &html[start..start + end + 3];
            if !url.contains('"') && !url.contains('\'') && !url.contains(' ') {
                out.push(url.to_string());
            }
            idx = start + end + 3;
        } else {
            break;
        }
    }
    out
}

pub async fn get_sc_client_id(client: &reqwest::Client, force_refresh: bool) -> String {
    if !force_refresh {
        if let Ok(lock) = SC_CLIENT_ID_CACHE.lock() {
            if !lock.value.is_empty()
                && lock
                    .fetched_at
                    .is_some_and(|t| t.elapsed() < Duration::from_secs(30 * 60))
            {
                return lock.value.clone();
            }
        }
    }

    if let Ok(res) = client
        .get("https://soundcloud.com")
        .timeout(Duration::from_secs(4))
        .send()
        .await
    {
        if let Ok(html) = res.text().await {
            let scripts = extract_sc_bundle_urls(&html);
            for url in scripts.iter().rev().take(5) {
                if let Ok(js_res) = client.get(url).timeout(Duration::from_secs(4)).send().await {
                    if let Ok(js) = js_res.text().await {
                        if let Some(cid) = extract_sc_client_id_from_script(&js) {
                            if let Ok(mut lock) = SC_CLIENT_ID_CACHE.lock() {
                                lock.value = cid.clone();
                                lock.fetched_at = Some(Instant::now());
                            }
                            return cid;
                        }
                    }
                }
            }
        }
    }

    if let Ok(mut lock) = SC_CLIENT_ID_CACHE.lock() {
        if lock.value.is_empty() {
            lock.value = DEFAULT_SC_CLIENT_ID.to_string();
        }
        lock.fetched_at = Some(Instant::now());
        return lock.value.clone();
    }
    DEFAULT_SC_CLIENT_ID.to_string()
}

pub fn parse_sc_search_json(json: &Value) -> Vec<TrackItem> {
    let Some(items) = json.get("collection").and_then(|v| v.as_array()) else {
        return Vec::new();
    };
    let mut out = Vec::with_capacity(items.len());
    for item in items {
        let id = item
            .get("id")
            .and_then(|v| v.as_u64().map(|n| n.to_string()).or_else(|| v.as_str().map(|s| s.to_string())))
            .unwrap_or_default();
        let title = item
            .get("title")
            .and_then(|v| v.as_str())
            .unwrap_or("Untitled")
            .trim()
            .to_string();
        if id.is_empty() || title.is_empty() {
            continue;
        }
        let artist = item
            .pointer("/user/username")
            .or_else(|| item.get("uploader"))
            .and_then(|v| v.as_str())
            .unwrap_or("Unknown Artist")
            .trim()
            .to_string();
        let duration_secs = item
            .get("duration")
            .and_then(|v| v.as_f64())
            .map(|ms| (ms / 1000.0).round())
            .unwrap_or(0.0);
        let thumbnail = item
            .get("artwork_url")
            .or_else(|| item.pointer("/user/avatar_url"))
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(|s| s.replace("-large.", "-t500x500."));
        let url = item
            .get("permalink_url")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        out.push(TrackItem {
            id,
            title,
            artist,
            album: None,
            duration_secs,
            thumbnail,
            url,
            platform: Platform::SoundCloud,
        });
    }
    out
}

pub async fn search_soundcloud(
    client: &reqwest::Client,
    query: &str,
    limit: usize,
) -> Result<Vec<TrackItem>, String> {
    let mut cid = get_sc_client_id(client, false).await;
    for attempt in 0..2 {
        let url = format!(
            "https://api-v2.soundcloud.com/search/tracks?q={}&limit={}&client_id={}",
            urlencoding::encode(query),
            limit.clamp(1, 50),
            urlencoding::encode(&cid)
        );
        let res = client
            .get(url)
            .timeout(Duration::from_secs(6))
            .send()
            .await
            .map_err(|e| e.to_string())?;
        if (res.status().as_u16() == 401 || res.status().as_u16() == 403) && attempt == 0 {
            cid = get_sc_client_id(client, true).await;
            continue;
        }
        if !res.status().is_success() {
            return Err(format!("SoundCloud API returned HTTP {}", res.status()));
        }
        let body: Value = res.json().await.map_err(|e| e.to_string())?;
        return Ok(parse_sc_search_json(&body));
    }
    Ok(Vec::new())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScTranscodingChoice {
    pub url: String,
    pub is_hls: bool,
    pub track_authorization: Option<String>,
}

pub fn select_sc_transcoding(track_json: &Value) -> Option<ScTranscodingChoice> {
    let list = track_json
        .pointer("/media/transcodings")
        .and_then(|v| v.as_array())?;
    let auth = track_json
        .get("track_authorization")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());

    let progressive = list.iter().find(|t| {
        t.pointer("/format/protocol")
            .and_then(|v| v.as_str())
            == Some("progressive")
    });
    let hls = list.iter().find(|t| {
        let is_hls = t.pointer("/format/protocol").and_then(|v| v.as_str()) == Some("hls");
        let mime = t
            .pointer("/format/mime_type")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        is_hls && !mime.to_ascii_lowercase().contains("encrypted")
    });

    let chosen = progressive.or(hls).or_else(|| list.first())?;
    let url = chosen.get("url")?.as_str()?.trim();
    if url.is_empty() {
        return None;
    }
    let is_hls = chosen
        .pointer("/format/protocol")
        .and_then(|v| v.as_str())
        == Some("hls");
    Some(ScTranscodingChoice {
        url: url.to_string(),
        is_hls,
        track_authorization: auth,
    })
}

pub fn parse_m3u8_segments(m3u8_body: &str, base_url: &str) -> Vec<String> {
    let base_prefix = base_url
        .rfind('/')
        .map(|idx| &base_url[..=idx])
        .unwrap_or(base_url);
    let mut out = Vec::new();
    for line in m3u8_body.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
            out.push(trimmed.to_string());
        } else if trimmed.starts_with('/') {
            if let Some(scheme_end) = base_url.find("://") {
                if let Some(host_end) = base_url[scheme_end + 3..].find('/') {
                    let origin = &base_url[..scheme_end + 3 + host_end];
                    out.push(format!("{}{}", origin, trimmed));
                    continue;
                }
            }
            out.push(trimmed.to_string());
        } else {
            out.push(format!("{}{}", base_prefix, trimmed));
        }
    }
    out
}

fn extract_numeric_sc_id(s: &str) -> Option<&str> {
    let t = s.trim();
    if t.len() >= 6 && t.bytes().all(|b| b.is_ascii_digit()) {
        return Some(t);
    }
    if let Some(idx) = t.find("tracks/") {
        let rest = &t[idx + "tracks/".len()..];
        let digits_len = rest.bytes().take_while(|b| b.is_ascii_digit()).count();
        if digits_len >= 6 {
            return Some(&rest[..digits_len]);
        }
    }
    None
}

async fn resolve_transcoding_url(
    client: &reqwest::Client,
    track_json: &Value,
    cid: &str,
) -> Option<ResolvedStream> {
    let choice = select_sc_transcoding(track_json)?;
    let sep = if choice.url.contains('?') { '&' } else { '?' };
    let mut req_url = format!(
        "{}{}client_id={}",
        choice.url,
        sep,
        urlencoding::encode(cid)
    );
    if let Some(auth) = choice.track_authorization {
        req_url.push_str("&track_authorization=");
        req_url.push_str(&urlencoding::encode(&auth));
    }
    let res = client.get(req_url).send().await.ok()?;
    if !res.status().is_success() {
        return None;
    }
    let j: Value = res.json().await.ok()?;
    let stream_url = j.get("url").and_then(|v| v.as_str())?.trim().to_string();
    if stream_url.is_empty() {
        return None;
    }
    let duration_secs = track_json
        .get("duration")
        .and_then(|v| v.as_f64())
        .map(|ms| ms / 1000.0);
    Some(ResolvedStream {
        stream_url,
        itag: None,
        mime_type: None,
        duration_secs,
        is_hls: choice.is_hls,
    })
}

pub async fn resolve_soundcloud_stream(
    client: &reqwest::Client,
    sc_url_or_id: &str,
    query_fallback: &str,
) -> Result<ResolvedStream, String> {
    let mut cid = get_sc_client_id(client, false).await;
    let target = sc_url_or_id.trim();

    for attempt in 0..2 {
        if let Some(id) = extract_numeric_sc_id(target) {
            let r = client
                .get(format!(
                    "https://api-v2.soundcloud.com/tracks/{}?client_id={}",
                    id,
                    urlencoding::encode(&cid)
                ))
                .send()
                .await
                .map_err(|e| e.to_string())?;
            if (r.status().as_u16() == 401 || r.status().as_u16() == 403) && attempt == 0 {
                cid = get_sc_client_id(client, true).await;
                continue;
            }
            if r.status().is_success() {
                if let Ok(t_obj) = r.json::<Value>().await {
                    if let Some(stream) = resolve_transcoding_url(client, &t_obj, &cid).await {
                        return Ok(stream);
                    }
                }
            }
        } else if target.starts_with("http") {
            let r = client
                .get(format!(
                    "https://api-v2.soundcloud.com/resolve?url={}&client_id={}",
                    urlencoding::encode(target),
                    urlencoding::encode(&cid)
                ))
                .send()
                .await
                .map_err(|e| e.to_string())?;
            if (r.status().as_u16() == 401 || r.status().as_u16() == 403) && attempt == 0 {
                cid = get_sc_client_id(client, true).await;
                continue;
            }
            if r.status().is_success() {
                if let Ok(t_obj) = r.json::<Value>().await {
                    if let Some(stream) = resolve_transcoding_url(client, &t_obj, &cid).await {
                        return Ok(stream);
                    }
                }
            }
        }

        if !query_fallback.trim().is_empty() {
            let sr = client
                .get(format!(
                    "https://api-v2.soundcloud.com/search/tracks?q={}&limit=5&client_id={}",
                    urlencoding::encode(query_fallback),
                    urlencoding::encode(&cid)
                ))
                .send()
                .await
                .map_err(|e| e.to_string())?;
            if (sr.status().as_u16() == 401 || sr.status().as_u16() == 403) && attempt == 0 {
                cid = get_sc_client_id(client, true).await;
                continue;
            }
            if sr.status().is_success() {
                if let Ok(sj) = sr.json::<Value>().await {
                    if let Some(col) = sj.get("collection").and_then(|v| v.as_array()) {
                        let pick = col
                            .iter()
                            .find(|t| {
                                t.get("duration").and_then(|v| v.as_f64()).unwrap_or(0.0) > 30000.0
                                    && !t.get("snipped").and_then(|v| v.as_bool()).unwrap_or(false)
                            })
                            .or_else(|| col.first());
                        if let Some(t_obj) = pick {
                            if let Some(stream) =
                                resolve_transcoding_url(client, t_obj, &cid).await
                            {
                                return Ok(stream);
                            }
                        }
                    }
                }
            }
        }
        break;
    }

    Err("SoundCloud stream could not be resolved".to_string())
}
