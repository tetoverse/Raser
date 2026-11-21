use discord_rich_presence::{DiscordIpc, DiscordIpcClient};
use serde_json::json;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

const CLIENT_ID: &str = "1566492575345676440";

static RPC_CLIENT: Mutex<Option<DiscordIpcClient>> = Mutex::new(None);

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct DiscordTrackInfo {
    pub title: Option<String>,
    pub artist: Option<String>,
    pub thumbnail: Option<String>,
    #[serde(rename = "startTimestamp")]
    pub start_timestamp: Option<i64>,
    pub duration: Option<f64>,
    pub url: Option<String>,
    #[serde(rename = "artistInStatus")]
    pub artist_in_status: Option<bool>,
}

pub fn update_presence(track: DiscordTrackInfo) {
    std::thread::spawn(move || {
        let title = track.title.unwrap_or_else(|| "Unknown Track".into());
        let artist = track.artist.unwrap_or_else(|| "Unknown Artist".into());
        let show_artist = track.artist_in_status.unwrap_or(true);
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        let start = track.start_timestamp.unwrap_or(now);

        let mut lock = match RPC_CLIENT.lock() {
            Ok(l) => l,
            Err(_) => return,
        };

        if lock.is_none() {
            if let Ok(mut client) = DiscordIpcClient::new(CLIENT_ID) {
                if client.connect().is_ok() {
                    *lock = Some(client);
                } else {
                    return;
                }
            } else {
                return;
            }
        }

        if let Some(client) = lock.as_mut() {
            let mut timestamps_val = json!({ "start": start });
            if let Some(dur) = track.duration {
                if dur > 0.0 {
                    timestamps_val = json!({
                        "start": start,
                        "end": start + dur as i64
                    });
                }
            }

            let mut assets_val = json!({});
            if let Some(thumb) = &track.thumbnail {
                if !thumb.is_empty() {
                    assets_val["large_image"] = json!(thumb);
                }
            }

            // status_display_type:
            // 0: Application Name ("Raser")
            // 1: state ("Sanatçı")
            // 2: details ("Şarkı")
            let status_display_type = if show_artist && !artist.is_empty() { 1 } else { 0 };

            let mut activity_obj = json!({
                "type": 2, // Listening
                "status_display_type": status_display_type,
                "details": title,
                "timestamps": timestamps_val,
                "assets": assets_val
            });

            if !artist.is_empty() {
                activity_obj["state"] = json!(artist);
            }

            if let Some(u) = &track.url {
                if u.starts_with("https://") {
                    activity_obj["buttons"] = json!([
                        {
                            "label": "Listen",
                            "url": u
                        }
                    ]);
                }
            }

            let nonce = format!("{}-{}", std::process::id(), now);
            let payload = json!({
                "cmd": "SET_ACTIVITY",
                "args": {
                    "pid": std::process::id(),
                    "activity": activity_obj
                },
                "nonce": nonce
            });

            // Opcode 1 for frame payload
            if client.send(payload, 1).is_err() {
                let _ = client.close();
                *lock = None;
            }
        }
    });
}

pub fn clear_presence() {
    std::thread::spawn(|| {
        let mut lock = match RPC_CLIENT.lock() {
            Ok(l) => l,
            Err(_) => return,
        };
        if let Some(client) = lock.as_mut() {
            let _ = client.clear_activity();
        }
    });
}
