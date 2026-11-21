use app_lib::local_files::{path_to_local_track, scan_folder};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

struct TempDirGuard(PathBuf);

impl TempDirGuard {
    fn new(name: &str) -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("raser_local_test_{}_{}", name, nanos));
        fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }
}

impl Drop for TempDirGuard {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn test_scan_dir_recursive_filters_supported_extensions() {
    let guard = TempDirGuard::new("scan_ext");
    let nested = guard.0.join("nested").join("deep");
    fs::create_dir_all(&nested).unwrap();

    let supported = [
        guard.0.join("a.mp3"),
        guard.0.join("b.FLAC"),
        guard.0.join("c.wav"),
        nested.join("d.ogg"),
        nested.join("e.m4a"),
        nested.join("f.aac"),
        nested.join("g.opus"),
        nested.join("h.wma"),
    ];
    let ignored = [
        guard.0.join("notes.txt"),
        guard.0.join("cover.jpg"),
        nested.join("readme.md"),
        nested.join("no_ext"),
    ];

    for p in supported.iter().chain(ignored.iter()) {
        fs::write(p, b"dummy").unwrap();
    }

    let found = scan_folder(&guard.0);
    assert_eq!(found.len(), 8);
    for tr in &found {
        assert!(!tr.url.contains("asset.localhost"));
        assert_eq!(tr.url, tr.path);
        assert_eq!(tr.platform, "local");
    }
}

#[test]
fn test_path_to_local_track_parsing_and_no_asset_localhost() {
    let p1 = PathBuf::from(r"C:\Music\Daft Punk - Harder Better Faster.mp3");
    let t1 = path_to_local_track(&p1);
    assert_eq!(t1.artist, "Daft Punk");
    assert_eq!(t1.title, "Harder Better Faster");
    assert!(t1.id.starts_with("local_"));
    assert_eq!(t1.url, t1.path);
    assert!(!t1.url.contains("asset.localhost"));

    // Deterministic ID
    let t1_again = path_to_local_track(&p1);
    assert_eq!(t1.id, t1_again.id);

    // Fallback when " - " is absent
    let p2 = PathBuf::from(r"C:\Music\AmbientTrack.flac");
    let t2 = path_to_local_track(&p2);
    assert_eq!(t2.artist, "Unknown Artist");
    assert_eq!(t2.title, "AmbientTrack");
    assert_ne!(t1.id, t2.id);
}
