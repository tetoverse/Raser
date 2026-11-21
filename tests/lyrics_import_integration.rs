use app_lib::import::{
    parse_spotify_embed_html, parse_spotify_gdpr_json, parse_spotify_playlist_id, score_track_match,
    ImportedTrack,
};
use app_lib::lyrics::{
    active_lyric_index, active_word_index, is_rtl_language_code, is_rtl_text, parse_lrc,
    serialize_lrc,
};
use app_lib::providers::{Platform, TrackItem};

#[test]
fn test_lrc_parsing_offset_multi_timestamp_word_timing_and_gaps() {
    let raw = r#"
[offset:500]
[00:01.00][00:10.00]Repeated chorus line
[00:02.50]<00:02.50>Hello <00:03.10>world
[00:12.00][Instrumental]
[00:20.50]After gap line
"#;

    let parsed = parse_lrc(raw);
    assert_eq!(parsed.offset_ms, 500);
    assert!(parsed.is_instrumental);
    assert!(!parsed.is_rtl);

    // Chronologically sorted with +500ms offset applied, plus 6s interlude gap between 12.5s and 21.0s
    assert!(parsed.lines.iter().any(|l| l.is_interlude));
    assert_eq!(parsed.lines[0].text, "Repeated chorus line");
    assert!((parsed.lines[0].time_secs - 1.5).abs() < 1e-4);
    assert!((parsed.lines[1].time_secs - 3.0).abs() < 1e-4);
    assert_eq!(parsed.lines[1].words.len(), 2);
    assert_eq!(parsed.lines[1].words[0].text, "Hello");
    assert!((parsed.lines[1].words[1].time_secs - 3.6).abs() < 1e-4);

    assert_eq!(active_lyric_index(&parsed.lines, 3.1), Some(1));
    assert_eq!(active_word_index(&parsed.lines[1], 3.7), Some(1));

    let roundtrip = serialize_lrc(&parsed.lines, 500);
    assert!(roundtrip.contains("[offset:500]"));
    assert!(roundtrip.contains("[00:01.50]Repeated chorus line"));
}

#[test]
fn test_rtl_detection_codes_and_unicode() {
    assert!(is_rtl_language_code("ar-SA"));
    assert!(is_rtl_language_code("fa"));
    assert!(is_rtl_language_code("he"));
    assert!(!is_rtl_language_code("tr-TR"));
    assert!(is_rtl_text("مرحبا بكم في عالم الموسيقى"));
    assert!(!is_rtl_text("Hello world music"));
}

#[test]
fn test_spotify_url_gdpr_embed_and_matching() {
    assert_eq!(
        parse_spotify_playlist_id("https://open.spotify.com/playlist/37i9dQZF1DXcBWIGoYBM5M?si=abc"),
        Some("37i9dQZF1DXcBWIGoYBM5M".to_string())
    );
    assert_eq!(
        parse_spotify_playlist_id("spotify:playlist:37i9dQZF1DXcBWIGoYBM5M"),
        Some("37i9dQZF1DXcBWIGoYBM5M".to_string())
    );
    assert_eq!(parse_spotify_playlist_id("https://example.com/nope"), None);

    let pl_json = r#"{
        "playlists": [{
            "name": "Night Drive",
            "items": [{
                "track": {
                    "trackName": "Midnight City",
                    "artistName": "M83",
                    "albumName": "Hurry Up"
                }
            }]
        }]
    }"#;
    let parsed_pl = parse_spotify_gdpr_json(pl_json).unwrap();
    assert_eq!(parsed_pl.len(), 1);
    assert_eq!(parsed_pl[0].name, "Night Drive");
    assert_eq!(parsed_pl[0].tracks[0].title, "Midnight City");

    let lib_json = r#"{"tracks":[{"track":"Time","artist":"Hans Zimmer","album":"Inception"}]}"#;
    let parsed_lib = parse_spotify_gdpr_json(lib_json).unwrap();
    assert_eq!(parsed_lib[0].name, "Spotify Library");
    assert_eq!(parsed_lib[0].tracks[0].title, "Time");

    let embed_html = r#"<html><script id="__NEXT_DATA__" type="application/json">{"props":{"pageProps":{"state":{"data":{"entity":{"name":"Embed Mix","trackList":[{"title":"Resonance","subtitle":"HOME","duration":212000}]}}}}}}</script></html>"#;
    let embed = parse_spotify_embed_html(embed_html).unwrap();
    assert_eq!(embed.name, "Embed Mix");
    assert_eq!(embed.tracks[0].title, "Resonance");
    assert_eq!(embed.tracks[0].duration_secs, 212.0);

    let target = ImportedTrack {
        title: "Resonance".to_string(),
        artist: "HOME".to_string(),
        album: String::new(),
        duration_secs: 212.0,
    };
    let exact = TrackItem {
        id: "11111111111".to_string(),
        title: "Resonance".to_string(),
        artist: "HOME".to_string(),
        album: None,
        duration_secs: 213.0,
        thumbnail: None,
        url: String::new(),
        platform: Platform::YouTubeMusic,
    };
    let remix = TrackItem {
        id: "22222222222".to_string(),
        title: "Resonance (Slowed Remix)".to_string(),
        artist: "HOME".to_string(),
        album: None,
        duration_secs: 250.0,
        thumbnail: None,
        url: String::new(),
        platform: Platform::YouTubeMusic,
    };
    assert!(score_track_match(&target, &exact) > score_track_match(&target, &remix));
}
