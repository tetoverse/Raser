use crate::storage;
use std::io::{self, Cursor, Read, Seek, SeekFrom};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use rodio::decoder::Decoder;
use rodio::source::Source;
use rodio::stream::{DeviceSinkBuilder, MixerDeviceSink};
use rodio::Player;
use rustfft::num_complex::Complex;
use rustfft::{Fft, FftPlanner};
use serde::{Deserialize, Serialize};

const FFT_SAMPLE_COUNT: usize = 512;
const FFT_BIN_COUNT: usize = 64;
const PROGRESSIVE_PREBUFFER_BYTES: usize = 192 * 1024;
const PROGRESSIVE_SEEK_STALL_TIMEOUT: Duration = Duration::from_secs(15);
const INITIAL_STREAM_CAP_LIMIT: usize = 64 * 1024 * 1024;

#[derive(Clone)]
struct SharedBytes(Arc<Vec<u8>>);

impl AsRef<[u8]> for SharedBytes {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

struct ProgressiveBuffer {
    data: Vec<u8>,
    finished: bool,
    error: Option<String>,
    total_len: Option<u64>,
}

struct SharedProgressive {
    state: Mutex<ProgressiveBuffer>,
    condvar: Condvar,
    load_gen: Arc<AtomicU64>,
}

struct SeekCancelContext {
    seek_gen: Arc<AtomicU64>,
    expected_seek_gen: u64,
    preparing: Arc<AtomicBool>,
}

impl SeekCancelContext {
    fn is_superseded(&self) -> bool {
        self.preparing.load(Ordering::SeqCst)
            && self.seek_gen.load(Ordering::SeqCst) != self.expected_seek_gen
    }
}

fn shared_http_client() -> Result<&'static reqwest::blocking::Client, String> {
    static CLIENT: std::sync::OnceLock<Result<reqwest::blocking::Client, String>> = std::sync::OnceLock::new();
    CLIENT
        .get_or_init(|| {
            reqwest::blocking::Client::builder()
                .timeout(Duration::from_secs(60))
                .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
                .build()
                .map_err(|e| format!("Failed to create HTTP client: {:?}", e))
        })
        .as_ref()
        .map_err(|e| e.clone())
}

struct ProgressiveReader {
    shared: Arc<SharedProgressive>,
    pos: u64,
    expected_gen: u64,
    seek_cancel: Option<SeekCancelContext>,
}

impl ProgressiveReader {
    fn new(shared: Arc<SharedProgressive>, expected_gen: u64) -> Self {
        Self {
            shared,
            pos: 0,
            expected_gen,
            seek_cancel: None,
        }
    }

    fn new_for_seek(
        shared: Arc<SharedProgressive>,
        expected_gen: u64,
        seek_gen: Arc<AtomicU64>,
        expected_seek_gen: u64,
        preparing: Arc<AtomicBool>,
    ) -> Self {
        Self {
            shared,
            pos: 0,
            expected_gen,
            seek_cancel: Some(SeekCancelContext {
                seek_gen,
                expected_seek_gen,
                preparing,
            }),
        }
    }

    fn check_cancelled(&self) -> io::Result<()> {
        if self.shared.load_gen.load(Ordering::SeqCst) != self.expected_gen {
            return Err(io::Error::new(io::ErrorKind::Interrupted, "Superseded by newer load"));
        }
        if let Some(ref cancel) = self.seek_cancel {
            if cancel.is_superseded() {
                return Err(io::Error::new(io::ErrorKind::Interrupted, "Superseded by newer seek"));
            }
        }
        Ok(())
    }

    fn is_preparing_seek(&self) -> bool {
        self.seek_cancel
            .as_ref()
            .map(|c| c.preparing.load(Ordering::SeqCst))
            .unwrap_or(false)
    }
}

impl Read for ProgressiveReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        let mut state = self.shared.state.lock().map_err(|_| {
            io::Error::new(io::ErrorKind::Other, "Progressive buffer lock poisoned")
        })?;
        let mut last_len = state.data.len();
        let mut last_progress = Instant::now();

        loop {
            self.check_cancelled()?;

            let available = state.data.len() as u64;
            if self.pos < available {
                let start = self.pos as usize;
                let to_copy = (state.data.len() - start).min(buf.len());
                buf[..to_copy].copy_from_slice(&state.data[start..start + to_copy]);
                self.pos += to_copy as u64;
                return Ok(to_copy);
            }

            if state.finished {
                return Ok(0);
            }

            if let Some(ref err) = state.error {
                return Err(io::Error::new(io::ErrorKind::Other, err.clone()));
            }

            if state.data.len() != last_len {
                last_len = state.data.len();
                last_progress = Instant::now();
            } else if self.is_preparing_seek()
                && last_progress.elapsed() >= PROGRESSIVE_SEEK_STALL_TIMEOUT
            {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "Timed out waiting for stream data during seek",
                ));
            }

            let (guard, _) = self
                .shared
                .condvar
                .wait_timeout(state, Duration::from_millis(100))
                .map_err(|_| {
                    io::Error::new(io::ErrorKind::Other, "Progressive buffer condvar error")
                })?;
            state = guard;
        }
    }
}

impl Seek for ProgressiveReader {
    fn seek(&mut self, style: SeekFrom) -> io::Result<u64> {
        let mut state = self.shared.state.lock().map_err(|_| {
            io::Error::new(io::ErrorKind::Other, "Progressive buffer lock poisoned")
        })?;
        let mut last_len = state.data.len();
        let mut last_progress = Instant::now();

        let new_pos: i64 = match style {
            SeekFrom::Start(offset) => offset as i64,
            SeekFrom::Current(offset) => (self.pos as i64).saturating_add(offset),
            SeekFrom::End(offset) => {
                loop {
                    self.check_cancelled()?;
                    if let Some(total) = state.total_len {
                        break (total as i64).saturating_add(offset);
                    }
                    if state.finished {
                        break (state.data.len() as i64).saturating_add(offset);
                    }
                    if let Some(ref err) = state.error {
                        return Err(io::Error::new(io::ErrorKind::Other, err.clone()));
                    }
                    if state.data.len() != last_len {
                        last_len = state.data.len();
                        last_progress = Instant::now();
                    } else if self.is_preparing_seek()
                        && last_progress.elapsed() >= PROGRESSIVE_SEEK_STALL_TIMEOUT
                    {
                        return Err(io::Error::new(
                            io::ErrorKind::TimedOut,
                            "Timed out waiting for stream length during seek",
                        ));
                    }
                    let (guard, _) = self
                        .shared
                        .condvar
                        .wait_timeout(state, Duration::from_millis(100))
                        .map_err(|_| {
                            io::Error::new(io::ErrorKind::Other, "Progressive seek condvar error")
                        })?;
                    state = guard;
                }
            }
        };

        if new_pos < 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Invalid seek to negative position",
            ));
        }

        self.pos = new_pos as u64;
        Ok(self.pos)
    }
}

fn extract_track_id_from_url(url: &str) -> Option<String> {
    if let Some(pos) = url.find('?') {
        let query = &url[pos + 1..];
        for pair in query.split('&') {
            let mut parts = pair.splitn(2, '=');
            if let (Some(k), Some(v)) = (parts.next(), parts.next()) {
                if k == "id" || k == "trackId" || k == "v" {
                    let decoded = urlencoding::decode(v).unwrap_or(std::borrow::Cow::Borrowed(v));
                    let trimmed = decoded.trim();
                    if !trimmed.is_empty() {
                        return Some(trimmed.to_string());
                    }
                }
            }
        }
    }
    None
}

// Lock-free wrapper around a rodio Source that downmixes channels to mono
// and pushes real-time PCM samples into an rtrb lock-free ring buffer for FFT analysis
pub struct VisualizerSource<S> {
    inner: S,
    channels: usize,
    sample_accum: f32,
    channel_idx: usize,
    producer: rtrb::Producer<f32>,
    last_sample_rate: u32,
    frames_since_rate_change: u64,
    accum_secs: f64,
    elapsed_secs_bits: Arc<AtomicU64>,
    seek_offset_bits: Arc<AtomicU64>,
}

impl<S> VisualizerSource<S>
where
    S: Source<Item = rodio::Sample>,
{
    fn new(
        inner: S,
        producer: rtrb::Producer<f32>,
        elapsed_secs_bits: Arc<AtomicU64>,
        seek_offset_bits: Arc<AtomicU64>,
    ) -> Self {
        let channels = (inner.channels().get() as usize).max(1);
        let last_sample_rate = inner.sample_rate().get().max(1);
        Self {
            inner,
            channels,
            sample_accum: 0.0,
            channel_idx: 0,
            producer,
            last_sample_rate,
            frames_since_rate_change: 0,
            accum_secs: 0.0,
            elapsed_secs_bits,
            seek_offset_bits,
        }
    }
}

impl<S> Iterator for VisualizerSource<S>
where
    S: Source<Item = rodio::Sample>,
{
    type Item = rodio::Sample;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        let sample = self.inner.next()?;
        let sample_f32 = sample as f32;
        self.sample_accum += sample_f32;
        self.channel_idx += 1;
        if self.channel_idx >= self.channels {
            let mono_sample = self.sample_accum / (self.channels as f32);
            let _ = self.producer.push(mono_sample);
            self.sample_accum = 0.0;
            self.channel_idx = 0;

            let sr = self.inner.sample_rate().get().max(1);
            if sr != self.last_sample_rate {
                self.accum_secs +=
                    self.frames_since_rate_change as f64 / self.last_sample_rate as f64;
                self.frames_since_rate_change = 0;
                self.last_sample_rate = sr;
            }
            self.frames_since_rate_change += 1;
            let elapsed = self.accum_secs + (self.frames_since_rate_change as f64 / sr as f64);
            self.elapsed_secs_bits
                .store(elapsed.to_bits(), Ordering::Relaxed);
        }
        Some(sample)
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}

impl<S> Source for VisualizerSource<S>
where
    S: Source<Item = rodio::Sample>,
{
    #[inline]
    fn current_span_len(&self) -> Option<usize> {
        self.inner.current_span_len()
    }

    #[inline]
    fn channels(&self) -> rodio::ChannelCount {
        self.inner.channels()
    }

    #[inline]
    fn sample_rate(&self) -> rodio::SampleRate {
        self.inner.sample_rate()
    }

    #[inline]
    fn total_duration(&self) -> Option<Duration> {
        self.inner.total_duration()
    }

    #[inline]
    fn try_seek(&mut self, pos: Duration) -> Result<(), rodio::source::SeekError> {
        let res = self.inner.try_seek(pos);
        if res.is_ok() {
            self.sample_accum = 0.0;
            self.channel_idx = 0;
            self.accum_secs = 0.0;
            self.frames_since_rate_change = 0;
            self.last_sample_rate = self.inner.sample_rate().get().max(1);
            self.elapsed_secs_bits
                .store(0.0f64.to_bits(), Ordering::Relaxed);
            self.seek_offset_bits
                .store(pos.as_secs_f64().to_bits(), Ordering::SeqCst);
        }
        res
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioState {
    pub playing: bool,
    pub position: f64,
    pub duration: f64,
    pub volume: f32,
    pub speed: f32,
    pub loading: bool,
    pub ended: bool,
    pub error: Option<String>,
    pub url: Option<String>,
}

pub struct AudioCore {
    sink: Mutex<Option<MixerDeviceSink>>,
    player: Mutex<Option<Player>>,
    consumer: Mutex<Option<rtrb::Consumer<f32>>>,
    recent_samples: Mutex<Vec<f32>>,
    fft_plan: Arc<dyn Fft<f32>>,
    hann_window: Vec<f32>,
    cached_bytes: Arc<Mutex<Option<Arc<Vec<u8>>>>>,
    active_progressive: Arc<Mutex<Option<(Arc<SharedProgressive>, u64)>>>,
    seek_offset_bits: Arc<AtomicU64>,
    source_elapsed_bits: Mutex<Arc<AtomicU64>>,
    current_url: Arc<Mutex<Option<String>>>,
    current_duration: Arc<Mutex<f64>>,
    current_error: Arc<Mutex<Option<String>>>,
    is_playing: Arc<AtomicBool>,
    is_loading: Arc<AtomicBool>,
    has_ended: Arc<AtomicBool>,
    track_has_played: Arc<AtomicBool>,
    volume: Arc<Mutex<f32>>,
    speed: Arc<Mutex<f32>>,
    load_generation: Arc<AtomicU64>,
    seek_generation: Arc<AtomicU64>,
    active_seek_count: Arc<AtomicUsize>,
}

struct SeekCountGuard(Arc<AtomicUsize>);

impl Drop for SeekCountGuard {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

impl AudioCore {
    pub fn new() -> Self {
        let mut planner = FftPlanner::new();
        let fft_plan = planner.plan_fft_forward(FFT_SAMPLE_COUNT);

        let mut hann_window = Vec::with_capacity(FFT_SAMPLE_COUNT);
        for i in 0..FFT_SAMPLE_COUNT {
            let mult = 0.5 * (1.0 - (2.0 * std::f32::consts::PI * i as f32 / (FFT_SAMPLE_COUNT - 1) as f32).cos());
            hann_window.push(mult);
        }

        Self {
            sink: Mutex::new(None),
            player: Mutex::new(None),
            consumer: Mutex::new(None),
            recent_samples: Mutex::new(vec![0.0f32; FFT_SAMPLE_COUNT]),
            fft_plan,
            hann_window,
            cached_bytes: Arc::new(Mutex::new(None)),
            active_progressive: Arc::new(Mutex::new(None)),
            seek_offset_bits: Arc::new(AtomicU64::new(0.0f64.to_bits())),
            source_elapsed_bits: Mutex::new(Arc::new(AtomicU64::new(0.0f64.to_bits()))),
            current_url: Arc::new(Mutex::new(None)),
            current_duration: Arc::new(Mutex::new(0.0)),
            current_error: Arc::new(Mutex::new(None)),
            is_playing: Arc::new(AtomicBool::new(false)),
            is_loading: Arc::new(AtomicBool::new(false)),
            has_ended: Arc::new(AtomicBool::new(false)),
            track_has_played: Arc::new(AtomicBool::new(false)),
            volume: Arc::new(Mutex::new(0.85)),
            speed: Arc::new(Mutex::new(1.0)),
            load_generation: Arc::new(AtomicU64::new(0)),
            seek_generation: Arc::new(AtomicU64::new(0)),
            active_seek_count: Arc::new(AtomicUsize::new(0)),
        }
    }

    fn ensure_player(&self) -> Result<(), String> {
        let mut player_lock = self.player.lock().map_err(|e| e.to_string())?;
        if player_lock.is_some() {
            return Ok(());
        }

        let mut sink_lock = self.sink.lock().map_err(|e| e.to_string())?;
        if sink_lock.is_none() {
            let sink = DeviceSinkBuilder::open_default_sink()
                .map_err(|e| format!("Failed to open default audio sink: {:?}", e))?;
            *sink_lock = Some(sink);
        }

        let sink = sink_lock.as_ref().unwrap();
        let player = Player::connect_new(sink.mixer());
        let vol = *self.volume.lock().map_err(|e| e.to_string())?;
        let spd = *self.speed.lock().map_err(|e| e.to_string())?;
        player.set_volume(vol);
        player.set_speed(spd);
        *player_lock = Some(player);

        Ok(())
    }

    pub fn play(&self) -> Result<(), String> {
        self.ensure_player()?;
        self.is_playing.store(true, Ordering::SeqCst);
        self.has_ended.store(false, Ordering::SeqCst);
        if let Some(player) = self.player.lock().map_err(|e| e.to_string())?.as_ref() {
            player.play();
        }
        Ok(())
    }

    pub fn pause(&self) -> Result<(), String> {
        self.is_playing.store(false, Ordering::SeqCst);
        if let Some(player) = self.player.lock().map_err(|e| e.to_string())?.as_ref() {
            player.pause();
        }
        Ok(())
    }

    pub fn toggle(&self) -> Result<bool, String> {
        if self.is_playing.load(Ordering::SeqCst) {
            self.pause()?;
            Ok(false)
        } else {
            self.play()?;
            Ok(true)
        }
    }

    pub fn set_volume(&self, vol: f32) -> Result<(), String> {
        let clamped = vol.clamp(0.0, 2.0);
        *self.volume.lock().map_err(|e| e.to_string())? = clamped;
        if let Some(player) = self.player.lock().map_err(|e| e.to_string())?.as_ref() {
            player.set_volume(clamped);
        }
        Ok(())
    }

    pub fn set_speed(&self, spd: f32) -> Result<(), String> {
        let clamped = spd.clamp(0.25, 4.0);
        *self.speed.lock().map_err(|e| e.to_string())? = clamped;
        if let Some(player) = self.player.lock().map_err(|e| e.to_string())?.as_ref() {
            player.set_speed(clamped);
        }
        Ok(())
    }

    fn resolve_duration(total_dur: Option<Duration>, duration_hint: Option<f64>) -> f64 {
        let hint = duration_hint.filter(|d| d.is_finite() && *d > 0.0);
        match (total_dur, hint) {
            (Some(total), Some(h)) => {
                let total_secs = total.as_secs_f64();
                // If decoder-reported duration diverges from metadata hint
                // (e.g. VBR/fragmented stream misestimation), prefer the metadata hint.
                if h > 1.0 && (total_secs - h).abs() > (h * 0.15).max(15.0) {
                    h
                } else {
                    total_secs
                }
            }
            (Some(total), None) => total.as_secs_f64(),
            (None, Some(h)) => h,
            (None, None) => 0.0,
        }
    }

    fn clamp_seek_position(position_secs: f64, duration_secs: f64) -> f64 {
        let pos = if position_secs.is_finite() {
            position_secs.max(0.0)
        } else {
            0.0
        };
        if duration_secs.is_finite() && duration_secs > 0.0 {
            pos.min((duration_secs - 0.1).max(0.0))
        } else {
            pos
        }
    }

    fn build_buffered_decoder(
        bytes: &Arc<Vec<u8>>,
        coarse_seek: bool,
    ) -> Result<Decoder<Cursor<SharedBytes>>, rodio::decoder::DecoderError> {
        let byte_len = bytes.len() as u64;
        let cursor = Cursor::new(SharedBytes(Arc::clone(bytes)));
        Decoder::builder()
            .with_data(cursor)
            .with_byte_len(byte_len)
            .with_seekable(true)
            .with_coarse_seek(coarse_seek)
            .build()
    }

    fn is_accurate_seek_unsupported(err: &rodio::source::SeekError) -> bool {
        matches!(
            err,
            rodio::source::SeekError::SymphoniaDecoder(
                rodio::decoder::symphonia::SeekError::AccurateSeekNotSupported
            )
        )
    }

    fn prepare_seeked_buffered_decoder(
        bytes: &Arc<Vec<u8>>,
        target_pos: Duration,
        clamped_pos: f64,
    ) -> Result<(Decoder<Cursor<SharedBytes>>, f64), String> {
        catch_unwind(AssertUnwindSafe(|| {
            let mut decoder = Self::build_buffered_decoder(bytes, false)
                .map_err(|e| format!("Decoder initialization failed during seek: {:?}", e))?;
            if target_pos <= Duration::from_millis(50) {
                return Ok((decoder, 0.0));
            }
            match decoder.try_seek(target_pos) {
                Ok(()) => Ok((decoder, clamped_pos)),
                Err(ref e) if Self::is_accurate_seek_unsupported(e) => {
                    let mut coarse_decoder = Self::build_buffered_decoder(bytes, true)
                        .map_err(|err| format!("Coarse decoder initialization failed: {:?}", err))?;
                    coarse_decoder
                        .try_seek(target_pos)
                        .map(|()| (coarse_decoder, clamped_pos))
                        .map_err(|err| format!("Coarse seek failed: {:?}", err))
                }
                Err(e) => Err(format!("Seek failed: {:?}", e)),
            }
        }))
        .map_err(|_| "Audio decoder panicked during seek".to_string())?
    }

    fn build_progressive_decoder(
        shared_prog: &Arc<SharedProgressive>,
        reader: ProgressiveReader,
        coarse_seek: bool,
    ) -> Result<Decoder<ProgressiveReader>, rodio::decoder::DecoderError> {
        let total_len = shared_prog
            .state
            .lock()
            .ok()
            .and_then(|st| st.total_len.or_else(|| if st.finished { Some(st.data.len() as u64) } else { None }));
        let mut builder = Decoder::builder().with_data(reader).with_seekable(true);
        if let Some(len) = total_len {
            if len > 0 {
                builder = builder.with_byte_len(len);
            }
        }
        if coarse_seek {
            builder = builder.with_coarse_seek(true);
        }
        builder.build()
    }

    fn prepare_seeked_progressive_decoder(
        shared_prog: &Arc<SharedProgressive>,
        prog_gen: u64,
        seek_gen: &Arc<AtomicU64>,
        my_seek_gen: u64,
        target_pos: Duration,
        clamped_pos: f64,
    ) -> Result<(Decoder<ProgressiveReader>, f64, Arc<AtomicBool>), String> {
        catch_unwind(AssertUnwindSafe(|| {
            let preparing = Arc::new(AtomicBool::new(true));
            let reader = ProgressiveReader::new_for_seek(
                Arc::clone(shared_prog),
                prog_gen,
                Arc::clone(seek_gen),
                my_seek_gen,
                Arc::clone(&preparing),
            );
            let mut decoder = Self::build_progressive_decoder(shared_prog, reader, false)
                .map_err(|e| format!("Stream decoder initialization failed during seek: {:?}", e))?;
            if target_pos <= Duration::from_millis(50) {
                return Ok((decoder, 0.0, preparing));
            }
            match decoder.try_seek(target_pos) {
                Ok(()) => Ok((decoder, clamped_pos, preparing)),
                Err(ref e) if Self::is_accurate_seek_unsupported(e) => {
                    let coarse_reader = ProgressiveReader::new_for_seek(
                        Arc::clone(shared_prog),
                        prog_gen,
                        Arc::clone(seek_gen),
                        my_seek_gen,
                        Arc::clone(&preparing),
                    );
                    let mut coarse_decoder = Self::build_progressive_decoder(shared_prog, coarse_reader, true)
                        .map_err(|err| format!("Coarse stream decoder init failed: {:?}", err))?;
                    coarse_decoder
                        .try_seek(target_pos)
                        .map(|()| (coarse_decoder, clamped_pos, preparing))
                        .map_err(|err| format!("Coarse stream seek failed: {:?}", err))
                }
                Err(e) => Err(format!("Stream seek failed: {:?}", e)),
            }
        }))
        .map_err(|_| "Stream decoder panicked during seek".to_string())?
    }

    fn begin_seek_generation(&self) -> u64 {
        let seek_gen = self.seek_generation.fetch_add(1, Ordering::SeqCst) + 1;
        if let Ok(prog_guard) = self.active_progressive.lock() {
            if let Some((ref shared_prog, _)) = *prog_guard {
                shared_prog.condvar.notify_all();
            }
        }
        seek_gen
    }

    fn is_seek_current(&self, my_seek_gen: u64, expected_load_gen: u64) -> bool {
        self.seek_generation.load(Ordering::SeqCst) == my_seek_gen
            && self.load_generation.load(Ordering::SeqCst) == expected_load_gen
    }

    fn commit_seeked_source<S>(
        &self,
        source: S,
        actual_offset: f64,
        my_seek_gen: u64,
        expected_load_gen: u64,
        preparing_flag: Option<&Arc<AtomicBool>>,
    ) -> Result<(), String>
    where
        S: Source<Item = rodio::Sample> + Send + 'static,
    {
        let player_guard = self.player.lock().map_err(|e| e.to_string())?;
        if !self.is_seek_current(my_seek_gen, expected_load_gen) {
            return Err("Seek superseded".to_string());
        }

        if let Some(flag) = preparing_flag {
            flag.store(false, Ordering::SeqCst);
        }

        let (producer, consumer) = rtrb::RingBuffer::new(4096);
        let elapsed_bits = Arc::new(AtomicU64::new(0.0f64.to_bits()));
        let viz_source = VisualizerSource::new(
            source,
            producer,
            Arc::clone(&elapsed_bits),
            Arc::clone(&self.seek_offset_bits),
        );

        if let Some(player) = player_guard.as_ref() {
            player.clear();
            *self.consumer.lock().unwrap_or_else(|e| e.into_inner()) = Some(consumer);
            *self
                .source_elapsed_bits
                .lock()
                .unwrap_or_else(|e| e.into_inner()) = elapsed_bits;
            self.seek_offset_bits
                .store(actual_offset.to_bits(), Ordering::SeqCst);
            self.has_ended.store(false, Ordering::SeqCst);
            if actual_offset <= 1.0 {
                self.track_has_played.store(false, Ordering::SeqCst);
            }
            player.append(viz_source);
            let vol = *self.volume.lock().unwrap_or_else(|e| e.into_inner());
            let spd = *self.speed.lock().unwrap_or_else(|e| e.into_inner());
            player.set_volume(vol);
            player.set_speed(spd);
            if self.is_playing.load(Ordering::SeqCst) {
                player.play();
            } else {
                player.pause();
            }
            Ok(())
        } else {
            Err("Audio player is not initialized".to_string())
        }
    }

    pub async fn seek_async(self: &Arc<Self>, position_secs: f64) -> Result<AudioState, String> {
        let my_seek_gen = self.begin_seek_generation();
        let this = Arc::clone(self);
        tauri::async_runtime::spawn_blocking(move || this.seek_with_gen(position_secs, my_seek_gen))
            .await
            .map_err(|e| format!("Seek task execution error: {:?}", e))?
    }

    pub fn seek(&self, position_secs: f64) -> Result<AudioState, String> {
        let my_seek_gen = self.begin_seek_generation();
        self.seek_with_gen(position_secs, my_seek_gen)
    }

    fn seek_with_gen(&self, position_secs: f64, my_seek_gen: u64) -> Result<AudioState, String> {
        self.active_seek_count.fetch_add(1, Ordering::SeqCst);
        let _seek_guard = SeekCountGuard(Arc::clone(&self.active_seek_count));

        if self.is_loading.load(Ordering::SeqCst) {
            return Err("Audio is still loading".to_string());
        }

        let expected_load_gen = self.load_generation.load(Ordering::SeqCst);
        if !self.is_seek_current(my_seek_gen, expected_load_gen) {
            return Err("Seek superseded".to_string());
        }

        self.ensure_player()?;
        let dur = *self.current_duration.lock().unwrap_or_else(|e| e.into_inner());
        let clamped_pos = Self::clamp_seek_position(position_secs, dur);
        let target_pos = Duration::from_secs_f64(clamped_pos);

        // Priority 1: Reconstruct decoder from cached full bytes in memory using zero-copy SharedBytes.
        if let Some(bytes_arc) = self.cached_bytes.lock().map_err(|e| e.to_string())?.clone() {
            if !self.is_seek_current(my_seek_gen, expected_load_gen) {
                return Err("Seek superseded".to_string());
            }
            let (decoder, actual_offset) =
                Self::prepare_seeked_buffered_decoder(&bytes_arc, target_pos, clamped_pos)?;
            self.commit_seeked_source(decoder, actual_offset, my_seek_gen, expected_load_gen, None)?;
            return Ok(self.get_state());
        }

        // Priority 2: Reconstruct decoder from active progressive stream buffer.
        if let Some((shared_prog, prog_gen)) =
            self.active_progressive.lock().map_err(|e| e.to_string())?.clone()
        {
            if prog_gen == expected_load_gen && self.is_seek_current(my_seek_gen, expected_load_gen) {
                // If progressive download already finished, upgrade directly to zero-copy Buffered seek
                let finished_bytes = shared_prog.state.lock().ok().and_then(|st| {
                    if st.finished && !st.data.is_empty() {
                        Some(Arc::new(st.data.clone()))
                    } else {
                        None
                    }
                });
                if let Some(bytes_arc) = finished_bytes {
                    *self.cached_bytes.lock().map_err(|e| e.to_string())? = Some(Arc::clone(&bytes_arc));
                    let (decoder, actual_offset) =
                        Self::prepare_seeked_buffered_decoder(&bytes_arc, target_pos, clamped_pos)?;
                    self.commit_seeked_source(decoder, actual_offset, my_seek_gen, expected_load_gen, None)?;
                    return Ok(self.get_state());
                }

                let (decoder, actual_offset, preparing) = Self::prepare_seeked_progressive_decoder(
                    &shared_prog,
                    prog_gen,
                    &self.seek_generation,
                    my_seek_gen,
                    target_pos,
                    clamped_pos,
                )?;
                self.commit_seeked_source(
                    decoder,
                    actual_offset,
                    my_seek_gen,
                    expected_load_gen,
                    Some(&preparing),
                )?;
                return Ok(self.get_state());
            }
            return Err("Seek superseded".to_string());
        }

        // Priority 3: Fallback to in-stream try_seek only if neither buffer is available.
        let player_guard = self.player.lock().map_err(|e| e.to_string())?;
        if !self.is_seek_current(my_seek_gen, expected_load_gen) {
            return Err("Seek superseded".to_string());
        }
        let player = player_guard
            .as_ref()
            .ok_or_else(|| "Audio player is not initialized".to_string())?;
        if player.empty() {
            return Err("No active audio source to seek".to_string());
        }
        let spd = *self.speed.lock().unwrap_or_else(|e| e.into_inner());
        let player_target = Duration::from_secs_f64(clamped_pos / spd.max(0.25) as f64);
        player
            .try_seek(player_target)
            .map_err(|e| format!("In-stream seek failed: {:?}", e))?;
        self.seek_offset_bits
            .store(clamped_pos.to_bits(), Ordering::SeqCst);
        self.has_ended.store(false, Ordering::SeqCst);
        if clamped_pos <= 1.0 {
            self.track_has_played.store(false, Ordering::SeqCst);
        }
        if self.is_playing.load(Ordering::SeqCst) {
            player.play();
        }
        drop(player_guard);
        Ok(self.get_state())
    }

    pub async fn load_audio_async(
        self: &Arc<Self>,
        url: String,
        start_pos_secs: Option<f64>,
        duration_hint: Option<f64>,
        autoplay: bool,
        track_id: Option<String>,
    ) -> Result<AudioState, String> {
        let current_gen = self.load_generation.fetch_add(1, Ordering::SeqCst) + 1;
        // Cancel any in-flight seek generation from the previous track
        self.seek_generation.fetch_add(1, Ordering::SeqCst);
        self.is_loading.store(true, Ordering::SeqCst);
        self.has_ended.store(false, Ordering::SeqCst);
        self.track_has_played.store(false, Ordering::SeqCst);
        if autoplay {
            self.is_playing.store(true, Ordering::SeqCst);
        }
        *self.current_error.lock().unwrap_or_else(|e| e.into_inner()) = None;
        *self.current_url.lock().unwrap_or_else(|e| e.into_inner()) = Some(url.clone());
        *self.cached_bytes.lock().unwrap_or_else(|e| e.into_inner()) = None;
        if let Some((old_prog, _)) = self
            .active_progressive
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take()
        {
            old_prog.condvar.notify_all();
        }
        self.seek_offset_bits
            .store(0.0f64.to_bits(), Ordering::SeqCst);
        if let Ok(arc) = self.source_elapsed_bits.lock() {
            arc.store(0.0f64.to_bits(), Ordering::Relaxed);
        }
        if let Some(dur) = duration_hint {
            *self.current_duration.lock().unwrap_or_else(|e| e.into_inner()) = dur;
        } else {
            *self.current_duration.lock().unwrap_or_else(|e| e.into_inner()) = 0.0;
        }

        // Immediately stop previous audio so there is no audio bleed while loading
        if let Ok(player_guard) = self.player.lock() {
            if let Some(player) = player_guard.as_ref() {
                player.clear();
                player.pause();
            }
        }
        // Silence visualizer samples immediately
        if let Ok(mut samples) = self.recent_samples.lock() {
            samples.fill(0.0);
        }

        let this = Arc::clone(self);
        let effective_tid = track_id
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(String::from)
            .or_else(|| extract_track_id_from_url(&url));

        tauri::async_runtime::spawn_blocking(move || {
            let mut cached_bytes_opt = None;
            if let Some(ref tid) = effective_tid {
                if let Some(cached_path) = storage::get_cached_audio_path(tid) {
                    if let Ok(b) = std::fs::read(&cached_path) {
                        log::info!("AudioCore: Loaded track {} from disk cache {:?}", tid, cached_path);
                        cached_bytes_opt = Some(b);
                    }
                }
            }

            enum LoadedDecoder {
                Buffered(Decoder<Cursor<SharedBytes>>),
                Progressive(Decoder<ProgressiveReader>),
            }

            let mut server_duration: Option<f64> = None;

            let loaded_decoder = if cached_bytes_opt.is_none()
                && (url.starts_with("http://") || url.starts_with("https://"))
            {
                let client = match shared_http_client() {
                    Ok(c) => c,
                    Err(e) => return Err(e),
                };

                let resp = match client.get(&url).send() {
                    Ok(r) => r,
                    Err(e) => {
                        let err_msg = format!("Failed to fetch stream URL: {:?}", e);
                        log::error!("Audio load failed for {}: {}", url, err_msg);
                        *this.current_error.lock().unwrap() = Some(err_msg.clone());
                        this.is_loading.store(false, Ordering::SeqCst);
                        this.is_playing.store(false, Ordering::SeqCst);
                        return Err(err_msg);
                    }
                };

                if !resp.status().is_success() {
                    let err_msg = format!(
                        "HTTP error {}: {}",
                        resp.status().as_u16(),
                        resp.status().canonical_reason().unwrap_or("Failed")
                    );
                    log::error!("Audio load failed for {}: {}", url, err_msg);
                    *this.current_error.lock().unwrap() = Some(err_msg.clone());
                    this.is_loading.store(false, Ordering::SeqCst);
                    this.is_playing.store(false, Ordering::SeqCst);
                    return Err(err_msg);
                }

                if this.load_generation.load(Ordering::SeqCst) != current_gen {
                    return Err("Superseded by newer load".to_string());
                }

                *this.cached_bytes.lock().unwrap() = None;

                if let Some(hdr_val) = resp.headers().get("x-audio-duration").and_then(|v| v.to_str().ok()) {
                    if let Ok(parsed_dur) = hdr_val.trim().parse::<f64>() {
                        if parsed_dur.is_finite() && parsed_dur > 0.0 {
                            server_duration = Some(parsed_dur);
                            *this.current_duration.lock().unwrap_or_else(|e| e.into_inner()) = parsed_dur;
                        }
                    }
                }

                let total_len = resp.content_length();
                let initial_cap = total_len
                    .map(|l| (l as usize).min(INITIAL_STREAM_CAP_LIMIT))
                    .unwrap_or(3 * 1024 * 1024);
                let shared = Arc::new(SharedProgressive {
                    state: Mutex::new(ProgressiveBuffer {
                        data: Vec::with_capacity(initial_cap),
                        finished: false,
                        error: None,
                        total_len,
                    }),
                    condvar: Condvar::new(),
                    load_gen: Arc::clone(&this.load_generation),
                });
                *this.active_progressive.lock().unwrap() = Some((Arc::clone(&shared), current_gen));

                let shared_dl = Arc::clone(&shared);
                let this_dl = Arc::clone(&this);
                let tid_dl = effective_tid.clone();
                std::thread::spawn(move || {
                    let mut stream_resp = resp;
                    let mut chunk = [0u8; 32768];
                    loop {
                        if this_dl.load_generation.load(Ordering::SeqCst) != current_gen {
                            shared_dl.condvar.notify_all();
                            return;
                        }
                        match stream_resp.read(&mut chunk) {
                            Ok(0) => {
                                let completed_bytes = {
                                    let mut st = shared_dl.state.lock().unwrap();
                                    st.finished = true;
                                    if st.data.is_empty() {
                                        st.error = Some("Empty stream response".to_string());
                                        None
                                    } else {
                                        Some(st.data.clone())
                                    }
                                };
                                shared_dl.condvar.notify_all();

                                if let Some(full_bytes) = completed_bytes {
                                    if this_dl.load_generation.load(Ordering::SeqCst) == current_gen {
                                        let bytes_arc = Arc::new(full_bytes);
                                        *this_dl.cached_bytes.lock().unwrap() = Some(Arc::clone(&bytes_arc));
                                        // Refine duration from complete buffered audio if current duration was missing or looked misestimated (>10 min)
                                        let cur_d = *this_dl.current_duration.lock().unwrap_or_else(|e| e.into_inner());
                                        if cur_d <= 0.0 || cur_d > 600.0 {
                                            if let Ok(d_check) = AudioCore::build_buffered_decoder(&bytes_arc, false) {
                                                if let Some(exact) = d_check.total_duration() {
                                                    let exact_s = exact.as_secs_f64();
                                                    if exact_s.is_finite() && exact_s > 1.0 && (cur_d <= 0.0 || (cur_d - exact_s).abs() > 15.0) {
                                                        *this_dl.current_duration.lock().unwrap_or_else(|e| e.into_inner()) = exact_s;
                                                    }
                                                }
                                            }
                                        }
                                        if let Some(ref tid) = tid_dl {
                                            storage::save_cached_audio(tid, &bytes_arc);
                                        }
                                    }
                                }
                                return;
                            }
                            Ok(n) => {
                                {
                                    let mut st = shared_dl.state.lock().unwrap();
                                    st.data.extend_from_slice(&chunk[..n]);
                                }
                                shared_dl.condvar.notify_all();
                            }
                            Err(e) => {
                                {
                                    let mut st = shared_dl.state.lock().unwrap();
                                    st.error = Some(format!("Failed to read stream response bytes: {:?}", e));
                                    st.finished = true;
                                }
                                shared_dl.condvar.notify_all();
                                return;
                            }
                        }
                    }
                });

                // Wait only until prebuffer (192 KB) or EOF arrives so playback starts in <1s
                {
                    let mut st = shared.state.lock().unwrap();
                    while st.data.len() < PROGRESSIVE_PREBUFFER_BYTES
                        && !st.finished
                        && st.error.is_none()
                    {
                        if this.load_generation.load(Ordering::SeqCst) != current_gen {
                            return Err("Superseded by newer load".to_string());
                        }
                        let (guard, _) = shared
                            .condvar
                            .wait_timeout(st, Duration::from_millis(100))
                            .unwrap();
                        st = guard;
                    }
                    if this.load_generation.load(Ordering::SeqCst) != current_gen {
                        return Err("Superseded by newer load".to_string());
                    }
                    if st.data.is_empty() {
                        let err_msg = st
                            .error
                            .clone()
                            .unwrap_or_else(|| "Empty stream response".to_string());
                        log::error!("Audio load failed for {}: {}", url, err_msg);
                        *this.current_error.lock().unwrap() = Some(err_msg.clone());
                        this.is_loading.store(false, Ordering::SeqCst);
                        this.is_playing.store(false, Ordering::SeqCst);
                        return Err(err_msg);
                    }
                }

                if let Err(e) = this.ensure_player() {
                    log::error!("Error ensuring audio player: {:?}", e);
                    *this.current_error.lock().unwrap() = Some(e.clone());
                    this.is_loading.store(false, Ordering::SeqCst);
                    return Err(e);
                }

                let reader = ProgressiveReader::new(Arc::clone(&shared), current_gen);
                let decoder = match Self::build_progressive_decoder(&shared, reader, false) {
                    Ok(d) => d,
                    Err(e) => {
                        let err_msg = format!("Audio decoder error for {}: {:?}", url, e);
                        log::error!("{}", err_msg);
                        *this.current_error.lock().unwrap() = Some(err_msg.clone());
                        this.is_loading.store(false, Ordering::SeqCst);
                        return Err(err_msg);
                    }
                };
                LoadedDecoder::Progressive(decoder)
            } else {
                let bytes_res = if let Some(b) = cached_bytes_opt {
                    Ok(b)
                } else {
                    let mut path_str = if let Some(stripped) = url.strip_prefix("file:///") {
                        stripped.replace('/', "\\")
                    } else if let Some(stripped) = url.strip_prefix("file://") {
                        stripped.replace('/', "\\")
                    } else {
                        url.clone()
                    };
                    if let Ok(decoded) = urlencoding::decode(&path_str) {
                        path_str = decoded.into_owned();
                    }
                    match std::fs::read(&path_str) {
                        Ok(b) => Ok(b),
                        Err(e) => Err(format!("Failed to read local file '{}': {:?}", path_str, e)),
                    }
                };

                if this.load_generation.load(Ordering::SeqCst) != current_gen {
                    return Err("Superseded by newer load".to_string());
                }

                let bytes = match bytes_res {
                    Ok(b) => b,
                    Err(err_msg) => {
                        log::error!("Audio load failed for {}: {}", url, err_msg);
                        *this.current_error.lock().unwrap() = Some(err_msg.clone());
                        this.is_loading.store(false, Ordering::SeqCst);
                        this.is_playing.store(false, Ordering::SeqCst);
                        return Err(err_msg);
                    }
                };

                let bytes_arc = Arc::new(bytes);
                *this.cached_bytes.lock().unwrap() = Some(Arc::clone(&bytes_arc));

                if let Err(e) = this.ensure_player() {
                    log::error!("Error ensuring audio player: {:?}", e);
                    *this.current_error.lock().unwrap() = Some(e.clone());
                    this.is_loading.store(false, Ordering::SeqCst);
                    return Err(e);
                }

                let decoder = match Self::build_buffered_decoder(&bytes_arc, false) {
                    Ok(d) => d,
                    Err(e) => {
                        let err_msg = format!("Audio decoder error for {}: {:?}", url, e);
                        log::error!("{}", err_msg);
                        *this.current_error.lock().unwrap_or_else(|e| e.into_inner()) = Some(err_msg.clone());
                        this.is_loading.store(false, Ordering::SeqCst);
                        return Err(err_msg);
                    }
                };
                LoadedDecoder::Buffered(decoder)
            };

            let total_dur = match &loaded_decoder {
                LoadedDecoder::Buffered(d) => d.total_duration(),
                LoadedDecoder::Progressive(d) => d.total_duration(),
            };

            let resolved_dur = if let Some(exact) = server_duration {
                exact
            } else {
                AudioCore::resolve_duration(total_dur, duration_hint)
            };
            if resolved_dur > 0.0 {
                *this.current_duration.lock().unwrap_or_else(|e| e.into_inner()) = resolved_dur;
            }

            let mut initial_offset = 0.0;
            let loaded_decoder = if let Some(start_sec) = start_pos_secs.filter(|s| s.is_finite() && *s > 0.05) {
                let clamped_start = Self::clamp_seek_position(start_sec, resolved_dur);
                let target_start = Duration::from_secs_f64(clamped_start);
                match loaded_decoder {
                    LoadedDecoder::Buffered(mut decoder) => {
                        if decoder.try_seek(target_start).is_ok() {
                            initial_offset = clamped_start;
                            LoadedDecoder::Buffered(decoder)
                        } else if let Ok(fresh) = Self::build_buffered_decoder(&this.cached_bytes.lock().unwrap_or_else(|e| e.into_inner()).as_ref().unwrap(), false) {
                            LoadedDecoder::Buffered(fresh)
                        } else {
                            LoadedDecoder::Buffered(decoder)
                        }
                    }
                    LoadedDecoder::Progressive(mut decoder) => {
                        if decoder.try_seek(target_start).is_ok() {
                            initial_offset = clamped_start;
                        }
                        LoadedDecoder::Progressive(decoder)
                    }
                }
            } else {
                loaded_decoder
            };

            if let Ok(player_guard) = this.player.lock() {
                if this.load_generation.load(Ordering::SeqCst) != current_gen {
                    return Err("Superseded by newer load".to_string());
                }
                if let Some(player) = player_guard.as_ref() {
                    let (producer, consumer) = rtrb::RingBuffer::new(4096);
                    let elapsed_bits = Arc::new(AtomicU64::new(0.0f64.to_bits()));

                    player.clear();
                    *this.consumer.lock().unwrap_or_else(|e| e.into_inner()) = Some(consumer);
                    *this
                        .source_elapsed_bits
                        .lock()
                        .unwrap_or_else(|e| e.into_inner()) = Arc::clone(&elapsed_bits);
                    this.seek_offset_bits
                        .store(initial_offset.to_bits(), Ordering::SeqCst);

                    match loaded_decoder {
                        LoadedDecoder::Buffered(decoder) => {
                            player.append(VisualizerSource::new(
                                decoder,
                                producer,
                                elapsed_bits,
                                Arc::clone(&this.seek_offset_bits),
                            ));
                        }
                        LoadedDecoder::Progressive(decoder) => {
                            player.append(VisualizerSource::new(
                                decoder,
                                producer,
                                elapsed_bits,
                                Arc::clone(&this.seek_offset_bits),
                            ));
                        }
                    }

                    let vol = *this.volume.lock().unwrap_or_else(|e| e.into_inner());
                    let spd = *this.speed.lock().unwrap_or_else(|e| e.into_inner());
                    player.set_volume(vol);
                    player.set_speed(spd);

                    let should_play = autoplay || this.is_playing.load(Ordering::SeqCst);
                    if should_play {
                        player.play();
                        this.is_playing.store(true, Ordering::SeqCst);
                    } else {
                        player.pause();
                        this.is_playing.store(false, Ordering::SeqCst);
                    }
                }
            }

            this.track_has_played.store(false, Ordering::SeqCst);
            this.has_ended.store(false, Ordering::SeqCst);
            this.is_loading.store(false, Ordering::SeqCst);

            Ok(this.get_state())
        })
        .await
        .map_err(|e| format!("Task execution error: {:?}", e))?
    }

    pub fn get_state(&self) -> AudioState {
        let loading = self.is_loading.load(Ordering::SeqCst);
        let seeking = self.active_seek_count.load(Ordering::SeqCst) > 0;
        let (pos, empty) = if loading {
            (0.0, false)
        } else {
            let offset = f64::from_bits(self.seek_offset_bits.load(Ordering::SeqCst));
            let elapsed = self
                .source_elapsed_bits
                .lock()
                .map(|arc| f64::from_bits(arc.load(Ordering::Relaxed)))
                .unwrap_or(0.0);
            let empty = match self.player.try_lock() {
                Ok(player_guard) => player_guard.as_ref().map(|p| p.empty()).unwrap_or(true),
                Err(_) => false,
            };
            (offset + elapsed, empty)
        };

        if !loading && !seeking && pos > 0.1 {
            self.track_has_played.store(true, Ordering::SeqCst);
        }

        let duration = *self.current_duration.lock().unwrap_or_else(|e| e.into_inner());
        let playing = self.is_playing.load(Ordering::SeqCst);
        let vol = *self.volume.lock().unwrap_or_else(|e| e.into_inner());
        let spd = *self.speed.lock().unwrap_or_else(|e| e.into_inner());
        let url = self.current_url.lock().unwrap_or_else(|e| e.into_inner()).clone();
        let error = self.current_error.lock().unwrap_or_else(|e| e.into_inner()).clone();

        let track_played = self.track_has_played.load(Ordering::SeqCst);
        let ended = if !loading && !seeking && empty && track_played {
            self.has_ended.store(true, Ordering::SeqCst);
            self.is_playing.store(false, Ordering::SeqCst);
            true
        } else {
            self.has_ended.load(Ordering::SeqCst)
        };

        AudioState {
            playing: if ended { false } else { playing },
            position: pos,
            duration,
            volume: vol,
            speed: spd,
            loading,
            ended,
            error,
            url,
        }
    }

    pub fn get_frequency_data(&self) -> Vec<u8> {
        let is_playing = self.is_playing.load(Ordering::SeqCst);
        if !is_playing {
            return vec![0u8; FFT_BIN_COUNT];
        }

        let mut samples_guard = match self.recent_samples.lock() {
            Ok(g) => g,
            Err(_) => return vec![0u8; FFT_BIN_COUNT],
        };

        // Drain fresh mono samples from lock-free consumer into history buffer
        if let Ok(mut cons_guard) = self.consumer.lock() {
            if let Some(cons) = cons_guard.as_mut() {
                while let Ok(sample) = cons.pop() {
                    samples_guard.push(sample);
                }
                if samples_guard.len() > FFT_SAMPLE_COUNT {
                    let excess = samples_guard.len() - FFT_SAMPLE_COUNT;
                    samples_guard.drain(0..excess);
                }
            }
        }

        if samples_guard.len() < FFT_SAMPLE_COUNT {
            let needed = FFT_SAMPLE_COUNT - samples_guard.len();
            samples_guard.splice(0..0, std::iter::repeat(0.0f32).take(needed));
        }

        // Apply precomputed Hann window
        let mut complex_buf: Vec<Complex<f32>> = samples_guard
            .iter()
            .zip(self.hann_window.iter())
            .map(|(&s, &w)| Complex {
                re: s * w,
                im: 0.0,
            })
            .collect();

        // Process forward FFT with cached plan
        self.fft_plan.process(&mut complex_buf);

        // Aggregate into 64 frequency bins
        let mut bins = vec![0u8; FFT_BIN_COUNT];
        let useful_fft_bins = FFT_SAMPLE_COUNT / 2; // 256
        let samples_per_bin = (useful_fft_bins / FFT_BIN_COUNT).max(1);

        for i in 0..FFT_BIN_COUNT {
            let mut sum_mag = 0.0f32;
            let start = i * samples_per_bin;
            let end = (start + samples_per_bin).min(useful_fft_bins);
            for k in start..end {
                let c = complex_buf[k];
                sum_mag += (c.re * c.re + c.im * c.im).sqrt();
            }
            let avg_mag = sum_mag / (end - start).max(1) as f32;
            let scaled = ((avg_mag * 35.0).log10() * 120.0).clamp(0.0, 255.0);
            bins[i] = if scaled.is_finite() && scaled > 0.0 {
                scaled as u8
            } else {
                0
            };
        }

        bins
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audio_core_initial_state() {
        let core = AudioCore::new();
        let state = core.get_state();
        assert_eq!(state.playing, false);
        assert_eq!(state.position, 0.0);
        assert_eq!(state.duration, 0.0);
        assert!((state.volume - 0.85).abs() < 0.001);
        assert!((state.speed - 1.0).abs() < 0.001);
        assert_eq!(state.loading, false);
        assert_eq!(state.ended, false);
        assert_eq!(state.error, None);
    }

    #[test]
    fn test_volume_and_speed_clamping() {
        let core = AudioCore::new();
        assert!(core.set_volume(5.0).is_ok());
        let st1 = core.get_state();
        assert_eq!(st1.volume, 2.0);

        assert!(core.set_volume(-1.0).is_ok());
        let st2 = core.get_state();
        assert_eq!(st2.volume, 0.0);

        assert!(core.set_speed(10.0).is_ok());
        let st3 = core.get_state();
        assert_eq!(st3.speed, 4.0);

        assert!(core.set_speed(0.01).is_ok());
        let st4 = core.get_state();
        assert_eq!(st4.speed, 0.25);
    }

    #[test]
    fn test_frequency_data_bins() {
        let core = AudioCore::new();
        let freq = core.get_frequency_data();
        assert_eq!(freq.len(), FFT_BIN_COUNT);
        // While paused, it should return zeroes
        assert!(freq.iter().all(|&v| v == 0));
    }

    #[test]
    fn test_resolve_duration_prefers_hint_on_mismatch() {
        // Metadata says 6:23 (383s), decoder misestimates 13:18 (798s)
        let resolved = AudioCore::resolve_duration(Some(Duration::from_secs(798)), Some(383.0));
        assert!((resolved - 383.0).abs() < 0.001);

        // Compatible decoder duration is preserved
        let compatible = AudioCore::resolve_duration(Some(Duration::from_secs(385)), Some(383.0));
        assert!((compatible - 385.0).abs() < 0.001);

        // Missing decoder duration falls back to hint
        let fallback = AudioCore::resolve_duration(None, Some(240.0));
        assert!((fallback - 240.0).abs() < 0.001);
    }

    #[test]
    fn test_clamp_seek_position() {
        assert_eq!(AudioCore::clamp_seek_position(-5.0, 383.0), 0.0);
        assert!((AudioCore::clamp_seek_position(500.0, 383.0) - 382.9).abs() < 0.001);
        assert!((AudioCore::clamp_seek_position(120.0, 383.0) - 120.0).abs() < 0.001);
        assert_eq!(AudioCore::clamp_seek_position(f64::NAN, 383.0), 0.0);
    }

    fn build_test_wav_bytes(sample_rate: u32, duration_secs: u32) -> Vec<u8> {
        let num_samples = sample_rate * duration_secs;
        let data_size = num_samples * 2;
        let mut wav = Vec::with_capacity(44 + data_size as usize);
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&(36 + data_size).to_le_bytes());
        wav.extend_from_slice(b"WAVEfmt ");
        wav.extend_from_slice(&16u32.to_le_bytes());
        wav.extend_from_slice(&1u16.to_le_bytes()); // PCM
        wav.extend_from_slice(&1u16.to_le_bytes()); // mono
        wav.extend_from_slice(&sample_rate.to_le_bytes());
        wav.extend_from_slice(&(sample_rate * 2).to_le_bytes());
        wav.extend_from_slice(&2u16.to_le_bytes());
        wav.extend_from_slice(&16u16.to_le_bytes());
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&data_size.to_le_bytes());
        for i in 0..num_samples {
            let sample = ((i as f32 * 0.1).sin() * 8000.0) as i16;
            wav.extend_from_slice(&sample.to_le_bytes());
        }
        wav
    }

    fn setup_core_with_headless_player(wav_bytes: Vec<u8>, duration_hint: f64) -> (AudioCore, std::thread::JoinHandle<()>) {
        let core = AudioCore::new();
        let (player, mut queue_rx) = Player::new();
        let pump_handle = std::thread::spawn(move || {
            while queue_rx.next().is_some() {
                std::thread::sleep(Duration::from_micros(50));
            }
        });
        *core.player.lock().unwrap() = Some(player);
        *core.cached_bytes.lock().unwrap() = Some(Arc::new(wav_bytes));
        *core.current_duration.lock().unwrap() = duration_hint;
        (core, pump_handle)
    }

    #[test]
    fn test_seek_updates_state_and_failed_seek_does_not_reset_to_zero() {
        let wav = build_test_wav_bytes(8000, 4);
        let (core, pump) = setup_core_with_headless_player(wav.clone(), 4.0);

        let st = core.seek(2.0).expect("seek to 2.0s should succeed");
        assert!((st.position - 2.0).abs() < 0.05, "expected ~2.0, got {}", st.position);
        assert!(!st.ended);

        // Failed buffered seek (e.g., unreadable/corrupted payload) must return Err
        // and MUST NOT reset playback position to 0.0 or clear the active player source.
        *core.cached_bytes.lock().unwrap() = Some(Arc::new(vec![0u8; 16]));
        let err = core.seek(3.0);
        assert!(err.is_err(), "corrupted buffered seek should fail");
        let after_fail = core.get_state();
        assert!(
            after_fail.position >= 1.95,
            "failed seek must preserve prior position (~2.0s), got {}",
            after_fail.position
        );

        // Failed progressive stream seek (e.g., mid-stream network error) must also preserve position
        *core.cached_bytes.lock().unwrap() = None;
        let load_gen = core.load_generation.load(Ordering::SeqCst);
        let prog = Arc::new(SharedProgressive {
            state: Mutex::new(ProgressiveBuffer {
                data: wav[..256].to_vec(),
                finished: true,
                error: Some("stream reset".to_string()),
                total_len: Some(wav.len() as u64),
            }),
            condvar: Condvar::new(),
            load_gen: Arc::clone(&core.load_generation),
        });
        *core.active_progressive.lock().unwrap() = Some((prog, load_gen));
        let prog_err = core.seek(3.5);
        assert!(prog_err.is_err(), "errored progressive seek should fail");
        assert!(
            core.get_state().position >= 1.95,
            "failed progressive seek must preserve prior position (~2.0s), got {}",
            core.get_state().position
        );

        // Seek while loading must fail without resetting position to 0.0
        *core.active_progressive.lock().unwrap() = None;
        *core.cached_bytes.lock().unwrap() = Some(Arc::new(wav));
        core.is_loading.store(true, Ordering::SeqCst);
        assert!(core.seek(0.5).is_err());
        core.is_loading.store(false, Ordering::SeqCst);
        assert!(core.get_state().position >= 1.95);

        drop(core);
        let _ = pump.join();
    }

    #[test]
    fn test_superseded_seek_does_not_overwrite_newer_seek() {
        let wav = build_test_wav_bytes(8000, 4);
        let (core, pump) = setup_core_with_headless_player(wav, 4.0);

        let st_new = core.seek(3.0).expect("latest seek to 3.0s should succeed");
        assert!((st_new.position - 3.0).abs() < 0.05);

        // Simulate an older seek generation arriving late
        let stale_gen = core.seek_generation.load(Ordering::SeqCst) - 1;
        let stale_res = core.seek_with_gen(0.5, stale_gen);
        assert!(stale_res.is_err());
        let st_after = core.get_state();
        assert!(
            st_after.position >= 2.95,
            "superseded seek must not overwrite position, got {}",
            st_after.position
        );

        drop(core);
        let _ = pump.join();
    }

    #[test]
    fn test_progressive_reader_aborts_when_seek_superseded() {
        let load_gen = Arc::new(AtomicU64::new(1));
        let seek_gen = Arc::new(AtomicU64::new(1));
        let preparing = Arc::new(AtomicBool::new(true));
        let shared = Arc::new(SharedProgressive {
            state: Mutex::new(ProgressiveBuffer {
                data: vec![1u8; 64],
                finished: false,
                error: None,
                total_len: Some(100_000),
            }),
            condvar: Condvar::new(),
            load_gen: Arc::clone(&load_gen),
        });

        let mut reader = ProgressiveReader::new_for_seek(
            Arc::clone(&shared),
            1,
            Arc::clone(&seek_gen),
            1,
            Arc::clone(&preparing),
        );
        reader.seek(SeekFrom::Start(50_000)).unwrap();

        let shared_wakeup = Arc::clone(&shared);
        let seek_gen_wakeup = Arc::clone(&seek_gen);
        let notifier = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(30));
            seek_gen_wakeup.store(2, Ordering::SeqCst);
            shared_wakeup.condvar.notify_all();
        });

        let started = Instant::now();
        let mut buf = [0u8; 16];
        let res = reader.read(&mut buf);
        assert!(res.is_err());
        assert_eq!(res.unwrap_err().kind(), io::ErrorKind::Interrupted);
        assert!(started.elapsed() < Duration::from_secs(1));
        let _ = notifier.join();

        // Once preparing is false (committed active stream), newer seek_gen must NOT interrupt reads
        preparing.store(false, Ordering::SeqCst);
        let mut active_reader = ProgressiveReader::new_for_seek(
            Arc::clone(&shared),
            1,
            Arc::clone(&seek_gen),
            1,
            Arc::clone(&preparing),
        );
        let n = active_reader.read(&mut buf).unwrap();
        assert_eq!(n, 16);
    }

    #[test]
    fn test_visualizer_source_tracks_source_time_across_speed() {
        let wav = Arc::new(build_test_wav_bytes(8000, 2));
        let decoder = AudioCore::build_buffered_decoder(&wav, false).unwrap();
        let (producer, _consumer) = rtrb::RingBuffer::new(4096);
        let elapsed_bits = Arc::new(AtomicU64::new(0.0f64.to_bits()));
        let seek_offset_bits = Arc::new(AtomicU64::new(0.0f64.to_bits()));

        let viz = VisualizerSource::new(
            decoder,
            producer,
            Arc::clone(&elapsed_bits),
            Arc::clone(&seek_offset_bits),
        );
        let mut sped_up = viz.speed(2.0).track_position();
        for _ in 0..8000 {
            assert!(sped_up.next().is_some());
        }

        // TrackPosition sees 0.5s wall time at 2x speed, while VisualizerSource tracks 1.0s source time
        assert!((sped_up.get_pos().as_secs_f64() - 0.5).abs() < 0.02);
        let source_elapsed = f64::from_bits(elapsed_bits.load(Ordering::Relaxed));
        assert!((source_elapsed - 1.0).abs() < 0.02);
    }

    #[test]
    fn test_ended_not_reported_while_seek_active() {
        let core = AudioCore::new();
        core.track_has_played.store(true, Ordering::SeqCst);
        core.active_seek_count.store(1, Ordering::SeqCst);
        let st = core.get_state();
        assert!(!st.ended, "ended must not trigger while a seek is active");
    }
}
