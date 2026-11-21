use app_lib::app::{SeekCompletion, SeekCoordinator};
use app_lib::audio_core::{normalize_audio_url, AudioCore, AudioSourceKind, VisualizerSource};
use rodio::decoder::Decoder;
use rodio::source::Source;
use std::fs;
use std::io::Cursor;
use std::path::PathBuf;
use std::sync::atomic::AtomicU64;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

fn build_sine_wav_bytes(sample_rate: u32, duration_secs: u32, freq_hz: f32) -> Vec<u8> {
    let num_samples = sample_rate * duration_secs;
    let data_size = num_samples * 2;
    let mut wav = Vec::with_capacity(44 + data_size as usize);
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + data_size).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&sample_rate.to_le_bytes());
    wav.extend_from_slice(&(sample_rate * 2).to_le_bytes());
    wav.extend_from_slice(&2u16.to_le_bytes());
    wav.extend_from_slice(&16u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data_size.to_le_bytes());
    for i in 0..num_samples {
        let t = i as f32 / sample_rate as f32;
        let sample = ((2.0 * std::f32::consts::PI * freq_hz * t).sin() * 16000.0) as i16;
        wav.extend_from_slice(&sample.to_le_bytes());
    }
    wav
}

#[test]
fn test_normalize_audio_url_legacy_and_local() {
    assert_eq!(
        normalize_audio_url("https://example.com/stream.mp3"),
        AudioSourceKind::RemoteHttp("https://example.com/stream.mp3".to_string())
    );
    assert_eq!(
        normalize_audio_url("http://example.com:8080/audio"),
        AudioSourceKind::RemoteHttp("http://example.com:8080/audio".to_string())
    );

    let legacy = normalize_audio_url("http://asset.localhost/C%3A%2FMusic%2Fa%20b.mp3");
    match legacy {
        AudioSourceKind::LocalPath(p) => {
            let s = p.to_string_lossy();
            assert!(s.contains("Music"));
            assert!(s.ends_with("a b.mp3"));
        }
        other => panic!("expected LocalPath, got {:?}", other),
    }

    let legacy_https = normalize_audio_url("https://asset.localhost/C:/Music/song%202.flac");
    match legacy_https {
        AudioSourceKind::LocalPath(p) => {
            assert!(p.to_string_lossy().ends_with("song 2.flac"));
        }
        other => panic!("expected LocalPath, got {:?}", other),
    }

    let file_uri = normalize_audio_url("file:///C:/Music/My%20Track.wav");
    match file_uri {
        AudioSourceKind::LocalPath(p) => {
            assert!(p.to_string_lossy().ends_with("My Track.wav"));
        }
        other => panic!("expected LocalPath, got {:?}", other),
    }

    let raw_pct = normalize_audio_url(r"C:\Music\100% Pure.mp3");
    assert_eq!(
        raw_pct,
        AudioSourceKind::LocalPath(PathBuf::from(r"C:\Music\100% Pure.mp3"))
    );

    // Non-existent raw local path with %20 MUST stay literal (never url-decoded)
    let raw_50cent = normalize_audio_url(r"C:\Music\50%20Cent - In Da Club.mp3");
    assert_eq!(
        raw_50cent,
        AudioSourceKind::LocalPath(PathBuf::from(r"C:\Music\50%20Cent - In Da Club.mp3"))
    );

    // Case-insensitive HTTP/HTTPS schemes preserving original suffix casing
    assert_eq!(
        normalize_audio_url("HTTPS://Example.COM/Stream?Id=AbC"),
        AudioSourceKind::RemoteHttp("HTTPS://Example.COM/Stream?Id=AbC".to_string())
    );
    assert_eq!(
        normalize_audio_url("HTTP://Example.COM/Audio"),
        AudioSourceKind::RemoteHttp("HTTP://Example.COM/Audio".to_string())
    );

    // Case-insensitive asset.localhost and file:// prefixes preserving suffix casing
    let upper_asset = normalize_audio_url("HTTP://ASSET.LOCALHOST/C%3A%2FMusic%2F50%20Cent.mp3");
    match upper_asset {
        AudioSourceKind::LocalPath(p) => {
            assert!(p.to_string_lossy().ends_with("50 Cent.mp3"));
        }
        other => panic!("expected LocalPath, got {:?}", other),
    }

    let upper_file = normalize_audio_url("FILE:///C:/Users/Test/My%20Song.wav");
    match upper_file {
        AudioSourceKind::LocalPath(p) => {
            assert!(p.to_string_lossy().ends_with("My Song.wav"));
        }
        other => panic!("expected LocalPath, got {:?}", other),
    }
}

struct TempFileGuard(PathBuf);

impl Drop for TempFileGuard {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

#[tokio::test]
async fn test_load_and_seek_local_wav_file() {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let wav_path = std::env::temp_dir().join(format!("raser_audio_test_{}.wav", nanos));
    let _guard = TempFileGuard(wav_path.clone());
    let wav_bytes = build_sine_wav_bytes(44100, 3, 440.0);
    fs::write(&wav_path, &wav_bytes).unwrap();

    let core = Arc::new(AudioCore::new());
    let res = core
        .load_audio_async(
            wav_path.to_string_lossy().to_string(),
            None,
            Some(3.0),
            false,
            None,
        )
        .await;

    match res {
        Ok(st) => {
            assert!((st.duration - 3.0).abs() < 0.25);
            assert!(!st.playing);
            assert!(core.set_volume(0.5).is_ok());
            assert!(core.set_speed(1.5).is_ok());
            let seeked = core.seek_async(1.5).await.expect("WAV seek should succeed");
            assert!((seeked.position - 1.5).abs() < 0.15);
            assert!((seeked.volume - 0.5).abs() < 0.01);
            assert!((seeked.speed - 1.5).abs() < 0.01);
        }
        Err(e) if e.contains("default audio sink") => {
            // Headless CI without audio output device: verify decoder + VisualizerSource directly
            let decoder = Decoder::new(Cursor::new(wav_bytes)).unwrap();
            assert!((decoder.total_duration().unwrap().as_secs_f64() - 3.0).abs() < 0.25);
        }
        Err(e) => panic!("Unexpected load_audio_async error: {}", e),
    }
}

#[tokio::test]
async fn test_visualizer_fft_produces_nonzero_bins_from_sine() {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let wav_path = std::env::temp_dir().join(format!("raser_fft_test_{}.wav", nanos));
    let _guard = TempFileGuard(wav_path.clone());
    let wav_bytes = build_sine_wav_bytes(44100, 3, 1000.0);
    fs::write(&wav_path, &wav_bytes).unwrap();

    let core = Arc::new(AudioCore::new());
    let _ = core.set_volume(0.0); // silent during automated test
    let res = core
        .load_audio_async(
            wav_path.to_string_lossy().to_string(),
            None,
            Some(3.0),
            true,
            None,
        )
        .await;

    if res.is_ok() {
        tokio::time::sleep(std::time::Duration::from_millis(80)).await;
        let bins = core.get_frequency_data();
        assert_eq!(bins.len(), 64);
        assert!(
            bins.iter().any(|&b| b > 0),
            "Expected non-zero FFT energy bins for 1kHz sine wave, got {:?}",
            bins
        );
        let _ = core.pause();
    } else {
        // Fallback if no audio output device exists in environment
        let decoder = Decoder::new(Cursor::new(wav_bytes)).unwrap();
        let (producer, mut consumer) = rtrb::RingBuffer::new(4096);
        let elapsed = Arc::new(AtomicU64::new(0.0f64.to_bits()));
        let offset = Arc::new(AtomicU64::new(0.0f64.to_bits()));
        let mut viz = VisualizerSource::new(decoder, producer, elapsed, offset);
        for _ in 0..1024 {
            let _ = viz.next();
        }
        let mut drained = Vec::new();
        while let Ok(s) = consumer.pop() {
            drained.push(s);
        }
        assert!(drained.len() >= 512);
        assert!(drained.iter().any(|&s| s.abs() > 0.05));
    }
}

#[test]
fn test_seek_coordinator_coalescing_parity() {
    // 1. Rapid 8-drag coalescing to single-flight latest-wins
    let mut coord = SeekCoordinator::default();
    let first = coord.request_seek(10.0);
    assert_eq!(first, Some((10.0, 1)));
    assert!(coord.in_flight);

    for step in [20.0, 35.0, 42.0, 50.0, 61.0, 77.0, 88.5] {
        assert_eq!(coord.request_seek(step), None);
    }
    assert_eq!(coord.seek_epoch, 8);
    assert_eq!(coord.pending_target, Some(88.5));

    // When first seek completes, coordinator must dispatch latest coalesced target (88.5)
    let next = coord.complete_seek(1, Ok(10.0));
    assert_eq!(next, SeekCompletion::DispatchNext(88.5, 8));

    let done = coord.complete_seek(8, Ok(88.5));
    assert_eq!(done, SeekCompletion::Committed(88.5));
    assert!(!coord.in_flight);

    // 2. Stale poll rejection by seek_epoch and while dragging
    assert_eq!(coord.accept_poll_position(7, 10.0), None);
    coord.scrubber_drag_target = Some(55.0);
    assert_eq!(coord.accept_poll_position(8, 88.5), None);
    coord.scrubber_drag_target = None;
    assert_eq!(coord.accept_poll_position(8, 89.0), Some(89.0));

    // 3. Failed seek rolls back to backend position without resetting to 0.0
    let req = coord.request_seek(120.0).unwrap();
    let rollback = coord.complete_seek(req.1, Err(89.0));
    assert_eq!(rollback, SeekCompletion::RolledBack(89.0));
    assert!((coord.last_committed_pos - 89.0).abs() < 0.001);

    // 4. Seek while loading queues behind load completion
    coord.begin_load();
    assert_eq!(coord.request_seek(44.0), None);
    assert_eq!(coord.request_seek(67.0), None);
    let queued = coord.finish_load(0.0);
    assert_eq!(queued, Some((67.0, coord.seek_epoch)));
}
