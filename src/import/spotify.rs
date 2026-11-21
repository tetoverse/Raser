use crate::providers::{self, Platform, TrackItem};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ImportedTrack {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub duration_secs: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct ImportedPlaylist {
    pub name: String,
    pub tracks: Vec<ImportedTrack>,
}

pub fn parse_spotify_playlist_id(input: &str) -> Option<String> {
    let trimmed = input.trim();
    if let Some(rest) = trimmed.strip_prefix("spotify:playlist:") {
        let id: String = rest
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric())
            .collect();
        if id.len() == 22 {
            return Some(id);
        }
    }
    for marker in ["/playlist/", "/embed/playlist/"] {
        if let Some(pos) = trimmed.find(marker) {
            let rest = &trimmed[pos + marker.len()..];
            let id: String = rest
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric())
                .collect();
            if id.len() == 22 {
                return Some(id);
            }
        }
    }
    if trimmed.len() == 22 && trimmed.bytes().all(|b| b.is_ascii_alphanumeric()) {
        return Some(trimmed.to_string());
    }
    None
}

pub fn parse_spotify_gdpr_json(raw: &str) -> Result<Vec<ImportedPlaylist>, String> {
    let json: Value = serde_json::from_str(raw.trim_start_matches('\u{feff}'))
        .map_err(|e| format!("Invalid JSON: {}", e))?;
    let mut out = Vec::new();

    // Case 1: Playlist1.json -> { "playlists": [{ "name": ..., "items": [{ "track": { "trackName", "artistName", "albumName" } }] }] }
    if let Some(playlists) = json.get("playlists").and_then(|v| v.as_array()) {
        for pl in playlists {
            let name = pl
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("Imported Playlist")
                .trim()
                .to_string();
            let mut tracks = Vec::new();
            if let Some(items) = pl.get("items").and_then(|v| v.as_array()) {
                for item in items {
                    let tr = item.get("track").unwrap_or(item);
                    let title = tr
                        .get("trackName")
                        .or_else(|| tr.get("name"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .trim()
                        .to_string();
                    let artist = tr
                        .get("artistName")
                        .or_else(|| tr.get("artist"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("Unknown Artist")
                        .trim()
                        .to_string();
                    let album = tr
                        .get("albumName")
                        .or_else(|| tr.get("album"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .trim()
                        .to_string();
                    if !title.is_empty() {
                        tracks.push(ImportedTrack {
                            title,
                            artist,
                            album,
                            duration_secs: 0.0,
                        });
                    }
                }
            }
            if !tracks.is_empty() {
                out.push(ImportedPlaylist { name, tracks });
            }
        }
    }

    // Case 2: YourLibrary.json -> { "tracks": [{ "track", "artist", "album" }] }
    if let Some(lib_tracks) = json.get("tracks").and_then(|v| v.as_array()) {
        let mut tracks = Vec::new();
        for item in lib_tracks {
            let title = item
                .get("track")
                .or_else(|| item.get("trackName"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            let artist = item
                .get("artist")
                .or_else(|| item.get("artistName"))
                .and_then(|v| v.as_str())
                .unwrap_or("Unknown Artist")
                .trim()
                .to_string();
            let album = item
                .get("album")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            if !title.is_empty() {
                tracks.push(ImportedTrack {
                    title,
                    artist,
                    album,
                    duration_secs: 0.0,
                });
            }
        }
        if !tracks.is_empty() {
            out.push(ImportedPlaylist {
                name: "Spotify Library".to_string(),
                tracks,
            });
        }
    }

    if out.is_empty() {
        return Err("No playlists or tracks found in Spotify JSON".to_string());
    }
    Ok(out)
}

fn extract_script_blocks(html: &str) -> Vec<String> {
    let mut blocks = Vec::new();
    let lower = html.to_ascii_lowercase();
    let mut pos = 0usize;
    while let Some(start_rel) = lower[pos..].find("<script") {
        let tag_start = pos + start_rel;
        let Some(open_end_rel) = html[tag_start..].find('>') else {
            break;
        };
        let content_start = tag_start + open_end_rel + 1;
        let Some(close_rel) = lower[content_start..].find("</script>") else {
            break;
        };
        let header = &lower[tag_start..content_start];
        if header.contains("__next_data__")
            || header.contains("initial-state")
            || header.contains("session")
            || header.contains("application/json")
        {
            blocks.push(html[content_start..content_start + close_rel].trim().to_string());
        }
        pos = content_start + close_rel + "</script>".len();
    }
    blocks
}

pub fn parse_spotify_embed_html(html: &str) -> Result<ImportedPlaylist, String> {
    for block in extract_script_blocks(html) {
        let Ok(json) = serde_json::from_str::<Value>(&block) else {
            continue;
        };
        let entity = json
            .pointer("/props/pageProps/state/data/entity")
            .or_else(|| json.pointer("/data/entity"))
            .unwrap_or(&json);
        let name = entity
            .get("name")
            .or_else(|| entity.get("title"))
            .and_then(|v| v.as_str())
            .unwrap_or("Spotify Playlist")
            .trim()
            .to_string();

        if let Some(track_list) = entity.get("trackList").and_then(|v| v.as_array()) {
            let mut tracks = Vec::with_capacity(track_list.len());
            for item in track_list {
                let title = item
                    .get("title")
                    .or_else(|| item.get("name"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .trim()
                    .to_string();
                let artist = item
                    .get("subtitle")
                    .or_else(|| item.get("artist"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("Unknown Artist")
                    .replace('\u{00a0}', " ")
                    .trim()
                    .to_string();
                let duration_secs = item
                    .get("duration")
                    .and_then(|v| v.as_f64())
                    .map(|ms| if ms > 1000.0 { ms / 1000.0 } else { ms })
                    .unwrap_or(0.0);
                if !title.is_empty() {
                    tracks.push(ImportedTrack {
                        title,
                        artist,
                        album: String::new(),
                        duration_secs,
                    });
                }
            }
            if !tracks.is_empty() {
                return Ok(ImportedPlaylist { name, tracks });
            }
        }
    }
    Err("Could not extract trackList from Spotify embed HTML".to_string())
}

pub async fn fetch_spotify_playlist(
    client: &reqwest::Client,
    url_or_id: &str,
    oauth_token: Option<&str>,
) -> Result<ImportedPlaylist, String> {
    let id = parse_spotify_playlist_id(url_or_id)
        .ok_or_else(|| "Invalid Spotify playlist URL or ID".to_string())?;

    if let Some(tok) = oauth_token.map(str::trim).filter(|s| !s.is_empty()) {
        let mut next_url = Some(format!("https://api.spotify.com/v1/playlists/{}", id));
        let mut name = String::from("Spotify Playlist");
        let mut tracks = Vec::new();
        while let Some(u) = next_url.take() {
            let res = client
                .get(&u)
                .bearer_auth(tok)
                .send()
                .await
                .map_err(|e| e.to_string())?;
            if res.status().is_success() {
                let json: Value = res.json().await.map_err(|e| e.to_string())?;
                if let Some(n) = json.get("name").and_then(|v| v.as_str()) {
                    name = n.trim().to_string();
                }
                let tracks_obj = json.get("tracks").unwrap_or(&json);
                if let Some(items) = tracks_obj.get("items").and_then(|v| v.as_array()) {
                    for it in items {
                        let Some(tr) = it.get("track") else { continue };
                        let title = tr
                            .get("name")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .trim()
                            .to_string();
                        if title.is_empty() {
                            continue;
                        }
                        let artist = tr
                            .get("artists")
                            .and_then(|v| v.as_array())
                            .map(|arr| {
                                arr.iter()
                                    .filter_map(|a| a.get("name").and_then(|n| n.as_str()))
                                    .collect::<Vec<_>>()
                                    .join(", ")
                            })
                            .unwrap_or_else(|| "Unknown Artist".to_string());
                        let album = tr
                            .pointer("/album/name")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        let duration_secs = tr
                            .get("duration_ms")
                            .and_then(|v| v.as_f64())
                            .map(|ms| ms / 1000.0)
                            .unwrap_or(0.0);
                        tracks.push(ImportedTrack {
                            title,
                            artist,
                            album,
                            duration_secs,
                        });
                    }
                }
                next_url = tracks_obj
                    .get("next")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string());
            } else {
                break;
            }
        }
        if !tracks.is_empty() {
            return Ok(ImportedPlaylist { name, tracks });
        }
    }

    let embed_url = format!("https://open.spotify.com/embed/playlist/{}", id);
    let uas = [
        "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36",
        "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:125.0) Gecko/20100101 Firefox/125.0",
    ];
    for (idx, ua) in uas.iter().enumerate() {
        let res = client
            .get(&embed_url)
            .header("User-Agent", *ua)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        let status = res.status().as_u16();
        if (status == 403 || status == 429) && idx == 0 {
            tokio::time::sleep(std::time::Duration::from_millis(800)).await;
            continue;
        }
        if !res.status().is_success() {
            return Err(format!("Spotify embed returned HTTP {}", status));
        }
        let html = res.text().await.map_err(|e| e.to_string())?;
        return parse_spotify_embed_html(&html);
    }

    Err("Failed to fetch Spotify playlist".to_string())
}

pub fn score_track_match(target: &ImportedTrack, cand: &TrackItem) -> i32 {
    let mut score = 0i32;
    let t_title = target.title.trim().to_ascii_lowercase();
    let t_artist = target.artist.trim().to_ascii_lowercase();
    let c_title = cand.title.trim().to_ascii_lowercase();
    let c_artist = cand.artist.trim().to_ascii_lowercase();

    if c_title == t_title {
        score += 50;
    } else if c_title.contains(&t_title) || t_title.contains(&c_title) {
        score += 25;
    }

    if !t_artist.is_empty() && (c_artist.contains(&t_artist) || t_artist.contains(&c_artist)) {
        score += 35;
    }

    if target.duration_secs > 1.0 && cand.duration_secs > 1.0 {
        let diff = (target.duration_secs - cand.duration_secs).abs();
        if diff <= 5.0 {
            score += 40;
        } else if diff <= 15.0 {
            score += 15;
        } else {
            score -= (diff.min(60.0)) as i32;
        }
    }

    for kw in ["remix", "live", "cover", "karaoke", "nightcore", "slowed"] {
        if c_title.contains(kw) && !t_title.contains(kw) {
            score -= 45;
        }
    }

    score
}

pub async fn match_imported_tracks(
    client: &reqwest::Client,
    tracks: &[ImportedTrack],
    platform: Platform,
) -> Vec<TrackItem> {
    let mut matched = Vec::new();
    for tr in tracks {
        let query = format!("{} {}", tr.artist, tr.title);
        if let Ok(cands) = providers::search_tracks(client, query.trim(), platform, 6, 0).await {
            if let Some(best) = cands
                .into_iter()
                .max_by_key(|c| score_track_match(tr, c))
            {
                matched.push(best);
            }
        }
    }
    matched
}
