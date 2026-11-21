use std::path::{Path, PathBuf};

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq)]
pub struct LocalTrack {
    pub id: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub path: String,
    pub url: String,
    pub thumbnail: String,
    pub cover: String,
    pub platform: String,
}

pub fn path_to_local_track(path: &Path) -> LocalTrack {
    let file_stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "Unknown Track".into());

    let (artist, title) = if let Some((a, t)) = file_stem.split_once(" - ") {
        (a.trim().to_string(), t.trim().to_string())
    } else {
        ("Unknown Artist".to_string(), file_stem)
    };

    let path_str = path.to_string_lossy().to_string();

    // Hash path for consistent unique ID
    let mut hash: u32 = 5381;
    for b in path_str.bytes() {
        hash = ((hash << 5).wrapping_add(hash)).wrapping_add(b as u32);
    }
    let id = format!("local_{:x}", hash);

    let url = path_str.clone();

    LocalTrack {
        id,
        title,
        artist,
        album: "".into(),
        path: path_str,
        url,
        thumbnail: "".into(),
        cover: "".into(),
        platform: "local".into(),
    }
}

pub fn is_supported_audio_file(path: &Path) -> bool {
    path.extension()
        .and_then(|s| s.to_str())
        .map(|ext| {
            matches!(
                ext.to_ascii_lowercase().as_str(),
                "mp3" | "flac" | "wav" | "ogg" | "m4a" | "aac" | "opus" | "wma"
            )
        })
        .unwrap_or(false)
}

pub fn pick_files() -> Vec<LocalTrack> {
    let files = rfd::FileDialog::new()
        .add_filter(
            "Audio Files",
            &["mp3", "flac", "wav", "ogg", "m4a", "aac", "opus", "wma"],
        )
        .pick_files();

    match files {
        Some(paths) => paths.iter().map(|p| path_to_local_track(p)).collect(),
        None => Vec::new(),
    }
}

pub async fn pick_files_async() -> Vec<LocalTrack> {
    let files = rfd::AsyncFileDialog::new()
        .add_filter(
            "Audio Files",
            &["mp3", "flac", "wav", "ogg", "m4a", "aac", "opus", "wma"],
        )
        .pick_files()
        .await;

    match files {
        Some(handles) => handles
            .into_iter()
            .map(|h| path_to_local_track(h.path()))
            .collect(),
        None => Vec::new(),
    }
}

pub fn scan_dir_recursive(dir: &Path, paths: &mut Vec<PathBuf>) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                scan_dir_recursive(&path, paths);
            } else if is_supported_audio_file(&path) {
                paths.push(path);
            }
        }
    }
}

pub fn scan_folder(dir: &Path) -> Vec<LocalTrack> {
    let mut paths = Vec::new();
    scan_dir_recursive(dir, &mut paths);
    paths.sort();
    paths.iter().map(|p| path_to_local_track(p)).collect()
}

pub fn pick_folder() -> Vec<LocalTrack> {
    let folder = rfd::FileDialog::new().pick_folder();
    match folder {
        Some(dir) => scan_folder(&dir),
        None => Vec::new(),
    }
}

pub async fn pick_folder_async() -> Vec<LocalTrack> {
    let folder = rfd::AsyncFileDialog::new().pick_folder().await;
    match folder {
        Some(handle) => {
            let dir = handle.path().to_path_buf();
            tokio::task::spawn_blocking(move || scan_folder(&dir))
                .await
                .unwrap_or_default()
        }
        None => Vec::new(),
    }
}
