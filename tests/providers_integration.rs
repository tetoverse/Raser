use app_lib::providers::soundcloud::{
    extract_sc_bundle_urls, extract_sc_client_id_from_script, parse_m3u8_segments,
    parse_sc_search_json, select_sc_transcoding,
};
use app_lib::providers::youtube::{
    extract_video_ids_from_web_search, extract_youtube_id, normalize_ytm_thumbnail,
    parse_ios_player_stream, parse_ytm_duration, parse_ytm_search_json,
};
use app_lib::providers::{AudioQuality, Platform};
use app_lib::storage::TrackRecord;
use serde_json::json;

#[test]
fn test_youtube_id_and_thumbnail_normalization() {
    assert_eq!(
        extract_youtube_id("https://www.youtube.com/watch?v=dQw4w9WgXcQ&list=PL123"),
        Some("dQw4w9WgXcQ".to_string())
    );
    assert_eq!(
        extract_youtube_id("https://music.youtube.com/watch?v=abc123_XYZ9"),
        Some("abc123_XYZ9".to_string())
    );
    assert_eq!(
        extract_youtube_id("https://youtu.be/dQw4w9WgXcQ?t=42"),
        Some("dQw4w9WgXcQ".to_string())
    );
    assert_eq!(extract_youtube_id("not-an-id"), None);

    assert_eq!(
        normalize_ytm_thumbnail("https://lh3.googleusercontent.com/abc=w120-h120-l90-rj"),
        "https://lh3.googleusercontent.com/abc=w544-h544"
    );
    assert_eq!(parse_ytm_duration("3:45"), Some(225.0));
    assert_eq!(parse_ytm_duration("1:02:03"), Some(3723.0));
}

#[test]
fn test_parse_ytm_search_json_and_filtering() {
    let payload = json!({
        "contents": {
            "tabbedSearchResultsRenderer": {
                "tabs": [{
                    "tabRenderer": {
                        "content": {
                            "sectionListRenderer": {
                                "contents": [
                                    {
                                        "musicCardShelfRenderer": {
                                            "title": { "runs": [{ "text": "Midnight City" }] },
                                            "subtitle": { "runs": [
                                                { "text": "Song" },
                                                { "text": " • " },
                                                { "text": "M83 - Topic" },
                                                { "text": " • " },
                                                { "text": "4:03" }
                                            ]},
                                            "onTap": { "watchEndpoint": { "videoId": "dX3k_QDnzHE" } },
                                            "thumbnail": {
                                                "musicThumbnailRenderer": {
                                                    "thumbnail": { "thumbnails": [{ "url": "https://img.example.com/a=w60-h60" }] }
                                                }
                                            }
                                        }
                                    },
                                    {
                                        "musicShelfRenderer": {
                                            "contents": [
                                                {
                                                    "musicResponsiveListItemRenderer": {
                                                        "playlistItemData": { "videoId": "PODCAST1234" },
                                                        "flexColumns": [
                                                            { "musicResponsiveListItemFlexColumnRenderer": { "text": { "runs": [{ "text": "Podcast Ep 1" }] } } },
                                                            { "musicResponsiveListItemFlexColumnRenderer": { "text": { "runs": [{ "text": "Podcast • Bölüm 1" }] } } }
                                                        ]
                                                    }
                                                },
                                                {
                                                    "musicResponsiveListItemRenderer": {
                                                        "playlistItemData": { "videoId": "9bZkp7q19f0" },
                                                        "flexColumns": [
                                                            { "musicResponsiveListItemFlexColumnRenderer": { "text": { "runs": [{ "text": "Wait" }] } } },
                                                            { "musicResponsiveListItemFlexColumnRenderer": { "text": { "runs": [
                                                                { "text": "M83" },
                                                                { "text": " • " },
                                                                { "text": "Hurry Up, We're Dreaming" },
                                                                { "text": " • " },
                                                                { "text": "5:43" }
                                                            ] } } }
                                                        ]
                                                    }
                                                }
                                            ]
                                        }
                                    }
                                ]
                            }
                        }
                    }
                }]
            }
        }
    });

    let tracks = parse_ytm_search_json(&payload);
    assert_eq!(tracks.len(), 2);
    assert_eq!(tracks[0].id, "dX3k_QDnzHE");
    assert_eq!(tracks[0].artist, "M83");
    assert_eq!(tracks[0].duration_secs, 243.0);
    assert_eq!(
        tracks[0].thumbnail.as_deref(),
        Some("https://img.example.com/a=w544-h544")
    );
    assert_eq!(tracks[1].id, "9bZkp7q19f0");
    assert_eq!(tracks[1].album.as_deref(), Some("Hurry Up, We're Dreaming"));
    assert_eq!(tracks[1].duration_secs, 343.0);
}

#[test]
fn test_parse_ios_player_stream_itag_selection() {
    let payload = json!({
        "playabilityStatus": { "status": "OK" },
        "videoDetails": { "lengthSeconds": "215" },
        "streamingData": {
            "adaptiveFormats": [
                { "itag": 137, "mimeType": "video/mp4; codecs=\"avc1\"", "url": "https://v.example/137", "bitrate": 4000000 },
                { "itag": 140, "mimeType": "audio/mp4; codecs=\"mp4a.40.2\"", "url": "https://a.example/140", "bitrate": 128000 },
                { "itag": 251, "mimeType": "audio/webm; codecs=\"opus\"", "url": "https://a.example/251", "bitrate": 160000 }
            ]
        }
    });

    let high = parse_ios_player_stream(&payload, AudioQuality::High).unwrap();
    assert_eq!(high.itag, Some(140));
    assert_eq!(high.stream_url, "https://a.example/140");
    assert_eq!(high.duration_secs, Some(215.0));

    let max = parse_ios_player_stream(&payload, AudioQuality::Max).unwrap();
    assert_eq!(max.itag, Some(251));
    assert_eq!(max.stream_url, "https://a.example/251");
}

#[test]
fn test_web_search_video_id_extraction() {
    let raw = r#"{"videoRenderer":{"videoId":"abc12345678"}} {"videoId":"abc12345678"} {"videoId":"xyz98765432"}"#;
    let ids = extract_video_ids_from_web_search(raw);
    assert_eq!(ids, vec!["abc12345678", "xyz98765432"]);
}

#[test]
fn test_soundcloud_client_id_search_and_hls_parsing() {
    let html = r#"<script src="https://a-v2.sndcdn.com/assets/12-abc.js"></script>"#;
    let urls = extract_sc_bundle_urls(html);
    assert_eq!(urls, vec!["https://a-v2.sndcdn.com/assets/12-abc.js"]);

    let js = r#"var x={foo:1,client_id:"AbCdEfGhIjKlMnOpQrStUvWxYz012345",bar:2};"#;
    assert_eq!(
        extract_sc_client_id_from_script(js).as_deref(),
        Some("AbCdEfGhIjKlMnOpQrStUvWxYz012345")
    );

    let sc_search = json!({
        "collection": [
            {
                "id": 987654321,
                "title": "Sunset Drive",
                "duration": 195400,
                "artwork_url": "https://i1.sndcdn.com/artworks-1-large.jpg",
                "permalink_url": "https://soundcloud.com/artist/sunset-drive",
                "user": { "username": "SynthArtist" }
            }
        ]
    });
    let items = parse_sc_search_json(&sc_search);
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].platform, Platform::SoundCloud);
    assert_eq!(items[0].duration_secs, 195.0);
    assert_eq!(
        items[0].thumbnail.as_deref(),
        Some("https://i1.sndcdn.com/artworks-1-t500x500.jpg")
    );

    let tr_rec: TrackRecord = items[0].clone().into();
    assert_eq!(tr_rec.platform, "soundcloud");
    assert_eq!(tr_rec.canonical_id(), "soundcloud:987654321");

    let track_detail = json!({
        "track_authorization": "tok123",
        "media": {
            "transcodings": [
                { "url": "https://api-v2.soundcloud.com/media/hls", "format": { "protocol": "hls", "mime_type": "audio/mpeg" } },
                { "url": "https://api-v2.soundcloud.com/media/prog", "format": { "protocol": "progressive", "mime_type": "audio/mpeg" } }
            ]
        }
    });
    let chosen = select_sc_transcoding(&track_detail).unwrap();
    assert!(!chosen.is_hls);
    assert!(chosen.url.ends_with("/prog"));

    let m3u8 = "#EXTM3U\n#EXTINF:5.0,\nseg0.ts\nhttps://cdn.example/seg1.ts\n";
    let segs = parse_m3u8_segments(m3u8, "https://cf-hls.sndcdn.com/playlist/track.m3u8");
    assert_eq!(
        segs,
        vec![
            "https://cf-hls.sndcdn.com/playlist/seg0.ts",
            "https://cdn.example/seg1.ts"
        ]
    );
}
