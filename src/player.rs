use crate::ytdlp;
use anyhow::{anyhow, Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{
    BufferSize, Sample, SampleFormat, SampleRate, StreamConfig, SupportedBufferSize,
    SupportedStreamConfig, SupportedStreamConfigRange,
};
use std::array;
use std::fs::File;
use std::io::{ErrorKind, Read};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread;
use std::time::{Duration, Instant};
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::{DecoderOptions, CODEC_TYPE_NULL};
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;
use symphonia::default::{get_codecs, get_probe};

pub const EQ_FREQS: [f32; 10] = [
    70.0, 180.0, 320.0, 600.0, 1000.0, 3000.0, 6000.0, 12000.0, 14000.0, 16000.0,
];

pub const SUPPORTED_EXTS: &[&str] = &[
    ".mp3", ".wav", ".flac", ".ogg", ".m4a", ".aac", ".m4b", ".m4p", ".alac", ".wma", ".opus",
];

const FFMPEG_DECODE_TIMEOUT: Duration = Duration::from_secs(180);
const FFMPEG_MAX_PCM_BYTES: usize = 512 * 1024 * 1024;
const FFMPEG_MAX_STDERR_BYTES: usize = 256 * 1024;
const STREAM_CHUNK_SECONDS: u32 = 150;
const STREAM_RETRY_ATTEMPTS: usize = 3;
const STREAM_MIN_FRAMES: usize = 44_100 * 3;

#[derive(Debug, Clone, Copy)]
pub struct PlayerOptions {
    pub sample_rate: Option<u32>,
    pub buffer_ms: Option<u32>,
    pub resample_quality: u8,
    pub bit_depth: u16,
}

impl Default for PlayerOptions {
    fn default() -> Self {
        Self {
            sample_rate: None,
            buffer_ms: None,
            resample_quality: 2,
            bit_depth: 32,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ResampleMode {
    Nearest,
    Linear,
    Cubic,
    Lanczos,
}

impl ResampleMode {
    fn from_level(level: u8) -> Self {
        match level.clamp(1, 4) {
            1 => Self::Nearest,
            2 => Self::Linear,
            3 => Self::Cubic,
            _ => Self::Lanczos,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Nearest => "Nearest",
            Self::Linear => "Linear",
            Self::Cubic => "Cubic",
            Self::Lanczos => "Lanczos",
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum FfmpegPcmFormat {
    S16,
    F32,
}

impl FfmpegPcmFormat {
    fn from_bit_depth(bit_depth: u16) -> Self {
        if bit_depth <= 16 {
            Self::S16
        } else {
            Self::F32
        }
    }

    fn bit_depth(self) -> u16 {
        match self {
            Self::S16 => 16,
            Self::F32 => 32,
        }
    }

    fn ffmpeg_muxer(self) -> &'static str {
        match self {
            Self::S16 => "s16le",
            Self::F32 => "f32le",
        }
    }

    fn ffmpeg_codec(self) -> &'static str {
        match self {
            Self::S16 => "pcm_s16le",
            Self::F32 => "pcm_f32le",
        }
    }
}

pub trait NativeSource: Send {
    fn next_stereo(&mut self) -> (f32, f32);
    fn position(&self) -> Duration;
    fn duration(&self) -> Duration;
    fn seek(&mut self, target: Duration) -> Result<()>;
    fn play(&mut self) -> Result<()> {
        Ok(())
    }
    fn pause(&mut self) -> Result<()> {
        Ok(())
    }
    fn stop(&mut self) -> Result<()> {
        Ok(())
    }
    fn is_finished(&self) -> bool {
        false
    }
    fn close(&mut self) {}
}

pub struct Player {
    state: Arc<Mutex<PlaybackState>>,
    _stream: cpal::Stream,
    output_sample_rate: f32,
    output_buffer_ms: Option<u32>,
    ffmpeg_format: FfmpegPcmFormat,
}

impl Player {
    pub fn new(options: PlayerOptions) -> Result<Self> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or_else(|| anyhow!("no default output audio device found"))?;

        let default_supported = device
            .default_output_config()
            .context("failed to query default output config")?;
        let supported = select_output_config(&device, default_supported, options.sample_rate)?;

        let sample_format = supported.sample_format();
        let (config, actual_buffer_ms) = finalize_stream_config(&supported, options.buffer_ms);
        let out_sr = config.sample_rate.0 as f32;
        let channels = config.channels as usize;
        let resample_mode = ResampleMode::from_level(options.resample_quality);
        let ffmpeg_format = FfmpegPcmFormat::from_bit_depth(options.bit_depth);

        let state = Arc::new(Mutex::new(PlaybackState::new(out_sr, resample_mode)));
        let stream = match sample_format {
            SampleFormat::F32 => {
                build_output_stream::<f32>(&device, &config, channels, state.clone())?
            }
            SampleFormat::I16 => {
                build_output_stream::<i16>(&device, &config, channels, state.clone())?
            }
            SampleFormat::U16 => {
                build_output_stream::<u16>(&device, &config, channels, state.clone())?
            }
        };

        stream.play().context("failed to start output stream")?;

        Ok(Self {
            state,
            _stream: stream,
            output_sample_rate: out_sr,
            output_buffer_ms: actual_buffer_ms,
            ffmpeg_format,
        })
    }

    pub fn output_sample_rate(&self) -> f32 {
        self.output_sample_rate
    }

    pub fn output_buffer_ms(&self) -> Option<u32> {
        self.output_buffer_ms
    }

    pub fn resample_quality_label(&self) -> &'static str {
        let state = lock_unpoison(&self.state);
        state.resample_mode.label()
    }

    pub fn ffmpeg_bit_depth(&self) -> u16 {
        self.ffmpeg_format.bit_depth()
    }

    pub fn play_async(&self, path: &str, realtime: bool) {
        let path = path.to_string();
        self.spawn_async_load(move |ffmpeg_format| decode_audio(&path, realtime, ffmpeg_format));
    }

    pub fn play_ytdlp_async(&self, page_url: &str) {
        let page_url = page_url.to_string();
        self.spawn_async_load(move |ffmpeg_format| {
            let stream_url = ytdlp::resolve_stream_url(&page_url)?;
            decode_audio(&stream_url, false, ffmpeg_format)
        });
    }

    pub fn play_native_async<F>(&self, loader: F)
    where
        F: FnOnce() -> Result<Box<dyn NativeSource>> + Send + 'static,
    {
        let state = self.state.clone();
        let load_token = {
            let mut state = lock_unpoison(&state);
            state.prepare_for_new_load();
            state.load_token = state.load_token.wrapping_add(1);
            state.preload_token = state.preload_token.wrapping_add(1);
            state.load_token
        };

        thread::spawn(move || {
            let source = loader();
            let mut state = lock_unpoison(&state);
            if state.load_token != load_token {
                return;
            }
            state.loading = false;
            state.loading_started_at = None;
            match source {
                Ok(source) => {
                    state.last_error = None;
                    apply_native_source(&mut state, source);
                }
                Err(err) => {
                    state.clear_active_source();
                    state.track_done = false;
                    state.playing = false;
                    state.paused = false;
                    state.tap.clear();
                    state.reset_filters();
                    state.last_error = Some(err.to_string());
                }
            }
        });
    }

    pub fn preload_async(&self, path: &str) {
        let path = path.to_string();
        let state = self.state.clone();
        let ffmpeg_format = self.ffmpeg_format;
        let (load_token, preload_token) = {
            let mut state = lock_unpoison(&state);
            state.preload_token = state.preload_token.wrapping_add(1);
            (state.load_token, state.preload_token)
        };

        thread::spawn(move || {
            let decoded = decode_audio(&path, false, ffmpeg_format).map(Arc::new).ok();
            let mut state = lock_unpoison(&state);
            if state.load_token != load_token || state.preload_token != preload_token {
                return;
            }
            state.preloaded = decoded;
        });
    }

    pub fn clear_preload(&self) {
        let mut state = lock_unpoison(&self.state);
        state.preload_token = state.preload_token.wrapping_add(1);
        state.preloaded = None;
        state.gapless_advanced = false;
    }

    pub fn take_gapless_advanced(&self) -> bool {
        let mut state = lock_unpoison(&self.state);
        let advanced = state.gapless_advanced;
        state.gapless_advanced = false;
        advanced
    }

    pub fn toggle_pause(&self) {
        let mut state = lock_unpoison(&self.state);
        if !state.playing {
            return;
        }

        let was_paused = state.paused;
        if let Some(ActiveSource::Native(source)) = state.source.as_mut() {
            let result = if was_paused {
                source.play()
            } else {
                source.pause()
            };
            if let Err(err) = result {
                state.last_error = Some(err.to_string());
                return;
            }
        }

        state.paused = !state.paused;
    }

    pub fn stop(&self) {
        let mut state = lock_unpoison(&self.state);
        state.load_token = state.load_token.wrapping_add(1);
        state.preload_token = state.preload_token.wrapping_add(1);
        state.loading = false;
        state.loading_started_at = None;
        state.clear_active_source();
        state.preloaded = None;
        state.track_done = false;
        state.playing = false;
        state.paused = false;
        state.gapless_advanced = false;
        state.tap.clear();
        state.reset_filters();
        state.last_error = None;
    }

    pub fn seek(&self, delta: Duration, backward: bool) {
        let mut state = lock_unpoison(&self.state);
        match state.source.as_mut() {
            Some(ActiveSource::Decoded(cursor)) => {
                let mut pos = cursor.src_pos;
                let delta_frames = delta.as_secs_f64() * cursor.track.sample_rate as f64;
                if backward {
                    pos -= delta_frames;
                } else {
                    pos += delta_frames;
                }

                let max_pos = cursor.track.frames.saturating_sub(1) as f64;
                cursor.src_pos = pos.clamp(0.0, max_pos);
                state.track_done = false;
            }
            Some(ActiveSource::Native(source)) => {
                let current = source.position();
                let duration = source.duration();
                let mut target = if backward {
                    current.saturating_sub(delta)
                } else {
                    current.saturating_add(delta)
                };
                if !duration.is_zero() {
                    target = target.min(duration.saturating_sub(Duration::from_millis(1)));
                }
                if let Err(err) = source.seek(target) {
                    state.last_error = Some(err.to_string());
                    return;
                }
                state.track_done = false;
            }
            None => {}
        }
    }

    pub fn position(&self) -> Duration {
        let state = lock_unpoison(&self.state);
        match state.source.as_ref() {
            Some(ActiveSource::Decoded(cursor)) => {
                Duration::from_secs_f64(cursor.src_pos / cursor.track.sample_rate as f64)
            }
            Some(ActiveSource::Native(source)) => source.position(),
            None => Duration::ZERO,
        }
    }

    pub fn duration(&self) -> Duration {
        let state = lock_unpoison(&self.state);
        match state.source.as_ref() {
            Some(ActiveSource::Decoded(cursor)) => Duration::from_secs_f64(
                cursor.track.frames as f64 / cursor.track.sample_rate as f64,
            ),
            Some(ActiveSource::Native(source)) => source.duration(),
            None => Duration::ZERO,
        }
    }

    pub fn set_volume(&self, db: f32) {
        let mut state = lock_unpoison(&self.state);
        state.volume_db = db.clamp(-30.0, 6.0);
    }

    pub fn volume(&self) -> f32 {
        lock_unpoison(&self.state).volume_db
    }

    pub fn set_eq_band(&self, band: usize, db: f32) {
        if band >= 10 {
            return;
        }
        let mut state = lock_unpoison(&self.state);
        state.eq_bands[band] = db.clamp(-12.0, 12.0);
    }

    pub fn toggle_mono(&self) {
        let mut state = lock_unpoison(&self.state);
        state.mono = !state.mono;
    }

    pub fn mono(&self) -> bool {
        lock_unpoison(&self.state).mono
    }

    pub fn eq_bands(&self) -> [f32; 10] {
        lock_unpoison(&self.state).eq_bands
    }

    pub fn is_playing(&self) -> bool {
        lock_unpoison(&self.state).playing
    }

    pub fn is_loading(&self) -> bool {
        lock_unpoison(&self.state).loading
    }

    pub fn loading_elapsed(&self) -> Option<Duration> {
        lock_unpoison(&self.state)
            .loading_started_at
            .map(|started| started.elapsed())
    }

    pub fn is_paused(&self) -> bool {
        lock_unpoison(&self.state).paused
    }

    pub fn track_done(&self) -> bool {
        lock_unpoison(&self.state).track_done
    }

    pub fn samples(&self, n: usize) -> Vec<f32> {
        lock_unpoison(&self.state).tap.samples(n)
    }

    pub fn take_error(&self) -> Option<String> {
        lock_unpoison(&self.state).last_error.take()
    }

    pub fn close(&self) {
        self.stop();
    }

    fn spawn_async_load<F>(&self, loader: F)
    where
        F: FnOnce(FfmpegPcmFormat) -> Result<DecodedTrack> + Send + 'static,
    {
        let state = self.state.clone();
        let ffmpeg_format = self.ffmpeg_format;
        let load_token = {
            let mut state = lock_unpoison(&state);
            state.prepare_for_new_load();
            state.load_token = state.load_token.wrapping_add(1);
            state.preload_token = state.preload_token.wrapping_add(1);
            state.load_token
        };

        thread::spawn(move || {
            let decoded = loader(ffmpeg_format).map(Arc::new);
            let mut state = lock_unpoison(&state);
            if state.load_token != load_token {
                return;
            }
            state.loading = false;
            state.loading_started_at = None;
            match decoded {
                Ok(track) => {
                    state.last_error = None;
                    apply_decoded_track(&mut state, track);
                }
                Err(err) => {
                    state.clear_active_source();
                    state.track_done = false;
                    state.playing = false;
                    state.paused = false;
                    state.tap.clear();
                    state.reset_filters();
                    state.last_error = Some(err.to_string());
                }
            }
        });
    }
}

fn select_output_config(
    device: &cpal::Device,
    default_config: SupportedStreamConfig,
    requested_sample_rate: Option<u32>,
) -> Result<SupportedStreamConfig> {
    let Some(target_rate) = requested_sample_rate else {
        return Ok(default_config);
    };

    let mut best: Option<(u64, SupportedStreamConfig)> = None;
    let default_channels = default_config.channels();
    let default_format = default_config.sample_format();
    let default_rate = default_config.sample_rate().0;

    let ranges = device
        .supported_output_configs()
        .context("failed to query supported output configs")?;
    for range in ranges {
        let config = build_supported_config(range.clone(), target_rate);
        let score = config_score(
            &config,
            target_rate,
            default_channels,
            default_format,
            default_rate,
        );

        let replace = match &best {
            Some((best_score, _)) => score < *best_score,
            None => true,
        };
        if replace {
            best = Some((score, config));
        }
    }

    Ok(best.map(|(_, config)| config).unwrap_or(default_config))
}

fn build_supported_config(
    range: SupportedStreamConfigRange,
    target_rate: u32,
) -> SupportedStreamConfig {
    let clamped = target_rate.clamp(range.min_sample_rate().0, range.max_sample_rate().0);
    range.with_sample_rate(SampleRate(clamped))
}

fn config_score(
    config: &SupportedStreamConfig,
    target_rate: u32,
    default_channels: u16,
    default_format: SampleFormat,
    default_rate: u32,
) -> u64 {
    let rate = config.sample_rate().0;
    let rate_penalty = rate.abs_diff(target_rate) as u64;
    let default_rate_penalty = rate.abs_diff(default_rate) as u64;
    let channel_penalty = if config.channels() == default_channels {
        0
    } else if config.channels() == 2 {
        1
    } else if config.channels() == 1 {
        2
    } else {
        3 + u64::from(config.channels().abs_diff(default_channels))
    };
    let format_penalty = if config.sample_format() == default_format {
        0
    } else {
        sample_format_rank(config.sample_format())
    };

    rate_penalty * 10_000 + channel_penalty * 100 + format_penalty * 10 + default_rate_penalty
}

fn sample_format_rank(format: SampleFormat) -> u64 {
    match format {
        SampleFormat::F32 => 0,
        SampleFormat::I16 => 1,
        SampleFormat::U16 => 2,
    }
}

fn finalize_stream_config(
    supported: &SupportedStreamConfig,
    requested_buffer_ms: Option<u32>,
) -> (StreamConfig, Option<u32>) {
    let mut config = supported.config();
    let Some(buffer_ms) = requested_buffer_ms else {
        return (config, None);
    };

    let Some(actual_buffer_ms) = buffer_ms_from_supported(supported, buffer_ms, &mut config) else {
        return (config, None);
    };

    (config, Some(actual_buffer_ms))
}

fn buffer_ms_from_supported(
    supported: &SupportedStreamConfig,
    buffer_ms: u32,
    config: &mut StreamConfig,
) -> Option<u32> {
    let SupportedBufferSize::Range { min, max } = supported.buffer_size() else {
        return None;
    };

    let sample_rate = supported.sample_rate().0.max(1);
    let requested_frames =
        ((u64::from(sample_rate) * u64::from(buffer_ms)).saturating_add(999)) / 1000;
    let clamped_frames = requested_frames
        .clamp(u64::from(*min), u64::from(*max))
        .max(1) as u32;
    config.buffer_size = BufferSize::Fixed(clamped_frames);

    Some(((u64::from(clamped_frames) * 1000) / u64::from(sample_rate)).max(1) as u32)
}

fn build_output_stream<T>(
    device: &cpal::Device,
    config: &StreamConfig,
    channels: usize,
    state: Arc<Mutex<PlaybackState>>,
) -> Result<cpal::Stream>
where
    T: Sample,
{
    let err_fn = |err| eprintln!("audio stream error: {err}");

    let stream = device
        .build_output_stream(
            config,
            move |output: &mut [T], _| {
                let mut guard = lock_unpoison(&state);
                write_output(&mut guard, output, channels);
            },
            err_fn,
        )
        .context("failed to build audio output stream")?;

    Ok(stream)
}

fn write_output<T>(state: &mut PlaybackState, output: &mut [T], channels: usize)
where
    T: Sample,
{
    if channels == 0 {
        return;
    }

    for frame in output.chunks_mut(channels) {
        let (mut left, mut right) = state.next_stereo();

        for (idx, filter) in state.filters.iter_mut().enumerate() {
            let gain = state.eq_bands[idx];
            left = filter.process(left, 0, gain);
            right = filter.process(right, 1, gain);
        }

        let gain = db_to_gain(state.volume_db);
        left *= gain;
        right *= gain;

        if state.mono {
            let mid = (left + right) * 0.5;
            left = mid;
            right = mid;
        }

        state.tap.push((left + right) * 0.5);

        for (ch, sample) in frame.iter_mut().enumerate() {
            let value = match ch {
                0 => left,
                1 => right,
                _ => (left + right) * 0.5,
            };
            *sample = Sample::from::<f32>(&value);
        }
    }
}

fn db_to_gain(db: f32) -> f32 {
    10.0_f32.powf(db / 20.0)
}

fn lock_unpoison<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn apply_decoded_track(state: &mut PlaybackState, decoded: Arc<DecodedTrack>) {
    state.clear_active_source();
    state.source = Some(ActiveSource::Decoded(DecodedCursor {
        src_pos: 0.0,
        src_step: decoded.sample_rate as f64 / state.output_sample_rate as f64,
        track: decoded,
    }));
    state.preloaded = None;
    state.track_done = false;
    state.loading = false;
    state.loading_started_at = None;
    state.playing = true;
    state.paused = false;
    state.tap.clear();
    state.reset_filters();
}

fn apply_native_source(state: &mut PlaybackState, source: Box<dyn NativeSource>) {
    state.clear_active_source();
    state.source = Some(ActiveSource::Native(source));
    state.preloaded = None;
    state.track_done = false;
    state.loading = false;
    state.loading_started_at = None;
    state.playing = true;
    state.paused = false;
    state.tap.clear();
    state.reset_filters();
}

struct DecodedCursor {
    track: Arc<DecodedTrack>,
    src_pos: f64,
    src_step: f64,
}

enum ActiveSource {
    Decoded(DecodedCursor),
    Native(Box<dyn NativeSource>),
}

struct PlaybackState {
    output_sample_rate: f32,
    resample_mode: ResampleMode,
    source: Option<ActiveSource>,
    preloaded: Option<Arc<DecodedTrack>>,
    volume_db: f32,
    eq_bands: [f32; 10],
    filters: [Biquad; 10],
    tap: RingTap,
    playing: bool,
    paused: bool,
    track_done: bool,
    mono: bool,
    loading: bool,
    loading_started_at: Option<Instant>,
    load_token: u64,
    preload_token: u64,
    gapless_advanced: bool,
    last_error: Option<String>,
}

impl PlaybackState {
    fn new(output_sample_rate: f32, resample_mode: ResampleMode) -> Self {
        Self {
            output_sample_rate,
            resample_mode,
            source: None,
            preloaded: None,
            volume_db: 0.0,
            eq_bands: [0.0; 10],
            filters: array::from_fn(|i| Biquad::new(EQ_FREQS[i], 1.4, output_sample_rate)),
            tap: RingTap::new(4096),
            playing: false,
            paused: false,
            track_done: false,
            mono: false,
            loading: false,
            loading_started_at: None,
            load_token: 0,
            preload_token: 0,
            gapless_advanced: false,
            last_error: None,
        }
    }

    fn clear_active_source(&mut self) {
        if let Some(mut source) = self.source.take() {
            if let ActiveSource::Native(native) = &mut source {
                let _ = native.stop();
                native.close();
            }
        }
    }

    fn prepare_for_new_load(&mut self) {
        self.loading = true;
        self.loading_started_at = Some(Instant::now());
        self.last_error = None;
        self.playing = false;
        self.paused = false;
        self.track_done = false;
        self.preloaded = None;
        self.gapless_advanced = false;
        self.tap.clear();
        self.clear_active_source();
    }

    fn reset_filters(&mut self) {
        for filter in &mut self.filters {
            filter.reset();
        }
    }

    fn next_stereo(&mut self) -> (f32, f32) {
        if !self.playing || self.paused || self.track_done {
            return (0.0, 0.0);
        }

        loop {
            let Some(source) = self.source.as_mut() else {
                return (0.0, 0.0);
            };

            if let ActiveSource::Native(source) = source {
                let out = source.next_stereo();
                if source.is_finished() {
                    self.track_done = true;
                }
                return out;
            }

            let track_frames = match source {
                ActiveSource::Decoded(cursor) => cursor.track.frames,
                ActiveSource::Native(_) => unreachable!(),
            };

            if track_frames == 0 {
                if self.advance_to_preloaded() {
                    continue;
                }
                self.track_done = true;
                return (0.0, 0.0);
            }

            let last_frame = track_frames.saturating_sub(1) as f64;
            let src_pos = match self.source.as_ref() {
                Some(ActiveSource::Decoded(cursor)) => cursor.src_pos,
                _ => 0.0,
            };
            if src_pos >= last_frame {
                if self.advance_to_preloaded() {
                    continue;
                }
                self.track_done = true;
                return (0.0, 0.0);
            }

            let out = match self.source.as_mut() {
                Some(ActiveSource::Decoded(cursor)) => {
                    let out = cursor.track.sample_at(cursor.src_pos, self.resample_mode);
                    cursor.src_pos += cursor.src_step;
                    out
                }
                _ => (0.0, 0.0),
            };

            let src_pos = match self.source.as_ref() {
                Some(ActiveSource::Decoded(cursor)) => cursor.src_pos,
                _ => 0.0,
            };
            if src_pos >= last_frame && !self.advance_to_preloaded() {
                self.track_done = true;
            }

            return out;
        }
    }

    fn advance_to_preloaded(&mut self) -> bool {
        let Some(next) = self.preloaded.take() else {
            return false;
        };

        self.clear_active_source();
        self.source = Some(ActiveSource::Decoded(DecodedCursor {
            src_pos: 0.0,
            src_step: next.sample_rate as f64 / self.output_sample_rate as f64,
            track: next,
        }));
        self.track_done = false;
        self.playing = true;
        self.paused = false;
        self.reset_filters();
        self.gapless_advanced = true;
        true
    }
}

struct RingTap {
    buf: Vec<f32>,
    pos: usize,
}

impl RingTap {
    fn new(size: usize) -> Self {
        Self {
            buf: vec![0.0; size],
            pos: 0,
        }
    }

    fn push(&mut self, sample: f32) {
        if self.buf.is_empty() {
            return;
        }
        self.buf[self.pos] = sample;
        self.pos = (self.pos + 1) % self.buf.len();
    }

    fn clear(&mut self) {
        self.buf.fill(0.0);
        self.pos = 0;
    }

    fn samples(&self, n: usize) -> Vec<f32> {
        if self.buf.is_empty() {
            return Vec::new();
        }

        let n = n.min(self.buf.len());
        let start = (self.pos + self.buf.len() - n) % self.buf.len();
        let mut out = Vec::with_capacity(n);
        for i in 0..n {
            out.push(self.buf[(start + i) % self.buf.len()]);
        }
        out
    }
}

#[derive(Clone)]
struct DecodedTrack {
    samples: Arc<[f32]>,
    frames: usize,
    sample_rate: f32,
}

impl DecodedTrack {
    fn sample_at(&self, frame: f64, mode: ResampleMode) -> (f32, f32) {
        if self.frames == 0 {
            return (0.0, 0.0);
        }

        match mode {
            ResampleMode::Nearest => self.sample_nearest(frame),
            ResampleMode::Linear => self.sample_linear(frame),
            ResampleMode::Cubic => self.sample_cubic(frame),
            ResampleMode::Lanczos => self.sample_lanczos(frame),
        }
    }

    fn sample_nearest(&self, frame: f64) -> (f32, f32) {
        self.frame_pair(frame.round() as isize)
    }

    fn sample_linear(&self, frame: f64) -> (f32, f32) {
        let i0 = frame.floor() as isize;
        let i1 = i0 + 1;
        let t = (frame - i0 as f64) as f32;
        let (l0, r0) = self.frame_pair(i0);
        let (l1, r1) = self.frame_pair(i1);
        (l0 + (l1 - l0) * t, r0 + (r1 - r0) * t)
    }

    fn sample_cubic(&self, frame: f64) -> (f32, f32) {
        let i1 = frame.floor() as isize;
        let t = (frame - i1 as f64) as f32;
        let p0 = self.frame_pair(i1 - 1);
        let p1 = self.frame_pair(i1);
        let p2 = self.frame_pair(i1 + 1);
        let p3 = self.frame_pair(i1 + 2);
        (
            catmull_rom(p0.0, p1.0, p2.0, p3.0, t).clamp(-1.0, 1.0),
            catmull_rom(p0.1, p1.1, p2.1, p3.1, t).clamp(-1.0, 1.0),
        )
    }

    fn sample_lanczos(&self, frame: f64) -> (f32, f32) {
        let center = frame.floor() as isize;
        let mut sum_l = 0.0f32;
        let mut sum_r = 0.0f32;
        let mut sum_w = 0.0f32;

        for idx in (center - 2)..=(center + 3) {
            let x = frame - idx as f64;
            let weight = lanczos_weight(x, 3.0) as f32;
            if weight.abs() <= f32::EPSILON {
                continue;
            }
            let (left, right) = self.frame_pair(idx);
            sum_l += left * weight;
            sum_r += right * weight;
            sum_w += weight;
        }

        if sum_w.abs() <= f32::EPSILON {
            return self.frame_pair(center);
        }

        (
            (sum_l / sum_w).clamp(-1.0, 1.0),
            (sum_r / sum_w).clamp(-1.0, 1.0),
        )
    }

    fn frame_pair(&self, frame: isize) -> (f32, f32) {
        let idx = frame.clamp(0, self.frames.saturating_sub(1) as isize) as usize;
        let base = idx * 2;
        let left = self.samples.get(base).copied().unwrap_or(0.0);
        let right = self.samples.get(base + 1).copied().unwrap_or(left);
        (left, right)
    }
}

fn catmull_rom(p0: f32, p1: f32, p2: f32, p3: f32, t: f32) -> f32 {
    0.5 * ((2.0 * p1)
        + (-p0 + p2) * t
        + (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3) * t * t
        + (-p0 + 3.0 * p1 - 3.0 * p2 + p3) * t * t * t)
}

fn lanczos_weight(x: f64, a: f64) -> f64 {
    let x = x.abs();
    if x <= f64::EPSILON {
        return 1.0;
    }
    if x >= a {
        return 0.0;
    }

    sinc(std::f64::consts::PI * x) * sinc(std::f64::consts::PI * x / a)
}

fn sinc(x: f64) -> f64 {
    if x.abs() <= f64::EPSILON {
        1.0
    } else {
        x.sin() / x
    }
}

fn decode_audio(
    path: &str,
    realtime: bool,
    ffmpeg_format: FfmpegPcmFormat,
) -> Result<DecodedTrack> {
    if is_url(path) {
        if realtime {
            return decode_stream_chunk_ffmpeg(path, ffmpeg_format);
        }
        return decode_audio_ffmpeg(path, ffmpeg_format);
    }

    if prefers_ffmpeg_decode(path) {
        if let Ok(track) = decode_audio_ffmpeg(path, ffmpeg_format) {
            return Ok(track);
        }
    }

    match decode_audio_symphonia(path) {
        Ok(track) => Ok(track),
        Err(symphonia_err) => decode_audio_ffmpeg(path, ffmpeg_format).map_err(|ffmpeg_err| {
            anyhow!(
                "decode failed with symphonia ({symphonia_err}); ffmpeg fallback also failed ({ffmpeg_err})"
            )
        }),
    }
}

fn decode_audio_symphonia(path: &str) -> Result<DecodedTrack> {
    let file = File::open(path).with_context(|| format!("failed to open file: {path}"))?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());

    let mut hint = Hint::new();
    if let Some(ext) = Path::new(path).extension().and_then(|s| s.to_str()) {
        hint.with_extension(ext);
    }

    let probed = get_probe().format(
        &hint,
        mss,
        &FormatOptions::default(),
        &MetadataOptions::default(),
    )?;

    let mut format = probed.format;

    let track = format
        .tracks()
        .iter()
        .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
        .or_else(|| format.default_track())
        .ok_or_else(|| anyhow!("no decodable audio track: {path}"))?;

    let track_id = track.id;
    let codec_params = track.codec_params.clone();
    let mut decoder = get_codecs().make(&codec_params, &DecoderOptions::default())?;

    let mut out = Vec::<f32>::new();
    let mut decoded_sample_rate: Option<u32> = None;

    loop {
        let packet = match format.next_packet() {
            Ok(packet) => packet,
            Err(SymphoniaError::IoError(err)) if is_stream_end(&err) => break,
            Err(err) => return Err(err.into()),
        };

        if packet.track_id() != track_id {
            continue;
        }

        let decoded = match decoder.decode(&packet) {
            Ok(decoded) => decoded,
            Err(SymphoniaError::DecodeError(_)) => continue,
            Err(SymphoniaError::IoError(err)) if is_stream_end(&err) => break,
            Err(err) => return Err(err.into()),
        };

        let spec = *decoded.spec();
        let sample_rate = spec.rate;
        if let Some(sr) = decoded_sample_rate {
            if sr != sample_rate {
                return Err(anyhow!(
                    "unsupported dynamic sample rate in stream: {sr} -> {sample_rate}"
                ));
            }
        } else {
            decoded_sample_rate = Some(sample_rate);
        }

        let channels = spec.channels.count().max(1);

        let mut sample_buf = SampleBuffer::<f32>::new(decoded.capacity() as u64, spec);
        sample_buf.copy_interleaved_ref(decoded);

        for frame in sample_buf.samples().chunks(channels) {
            let (left, right) = downmix_to_stereo(frame);
            out.push(left);
            out.push(right);
        }
    }

    if out.is_empty() {
        return Err(anyhow!("decoded audio buffer is empty: {path}"));
    }

    let sample_rate =
        decoded_sample_rate.ok_or_else(|| anyhow!("missing decoded sample rate: {path}"))?;
    let frames = out.len() / 2;

    Ok(DecodedTrack {
        samples: Arc::from(out.into_boxed_slice()),
        frames,
        sample_rate: sample_rate as f32,
    })
}

fn downmix_to_stereo(frame: &[f32]) -> (f32, f32) {
    match frame.len() {
        0 => (0.0, 0.0),
        1 => (frame[0], frame[0]),
        2 => (frame[0], frame[1]),
        _ => {
            // Assume AAC canonical order for 5.1 when present: L R C LFE Ls Rs.
            let l = frame[0];
            let r = frame[1];
            let c = frame.get(2).copied().unwrap_or(0.0);
            let lfe = frame.get(3).copied().unwrap_or(0.0);
            let ls = frame.get(4).copied().unwrap_or(0.0);
            let rs = frame.get(5).copied().unwrap_or(0.0);

            let mut out_l = l + 0.707 * c + 0.5 * lfe + 0.707 * ls;
            let mut out_r = r + 0.707 * c + 0.5 * lfe + 0.707 * rs;

            if frame.len() > 6 {
                let extras = &frame[6..];
                let avg = extras.iter().copied().sum::<f32>() / extras.len() as f32;
                out_l += 0.3 * avg;
                out_r += 0.3 * avg;
            }

            (out_l.clamp(-1.0, 1.0), out_r.clamp(-1.0, 1.0))
        }
    }
}

fn decode_stream_chunk_ffmpeg(path: &str, ffmpeg_format: FfmpegPcmFormat) -> Result<DecodedTrack> {
    let mut last_err: Option<anyhow::Error> = None;
    for _ in 0..STREAM_RETRY_ATTEMPTS {
        match decode_audio_ffmpeg_inner(path, Some(STREAM_CHUNK_SECONDS), true, ffmpeg_format) {
            Ok(track) if track.frames >= STREAM_MIN_FRAMES => return Ok(track),
            Ok(track) => {
                last_err = Some(anyhow!(
                    "ffmpeg stream chunk too short: {} frames",
                    track.frames
                ));
            }
            Err(err) => {
                last_err = Some(err);
            }
        }
        thread::sleep(Duration::from_millis(350));
    }

    Err(last_err.unwrap_or_else(|| anyhow!("ffmpeg stream chunk decode failed")))
}

fn decode_audio_ffmpeg(path: &str, ffmpeg_format: FfmpegPcmFormat) -> Result<DecodedTrack> {
    decode_audio_ffmpeg_inner(path, None, false, ffmpeg_format)
}

fn decode_audio_ffmpeg_inner(
    path: &str,
    max_seconds: Option<u32>,
    stream_mode: bool,
    ffmpeg_format: FfmpegPcmFormat,
) -> Result<DecodedTrack> {
    let mut ffmpeg_args: Vec<String> = vec!["-v".into(), "error".into()];
    if stream_mode {
        ffmpeg_args.extend([
            "-reconnect".into(),
            "1".into(),
            "-reconnect_streamed".into(),
            "1".into(),
            "-reconnect_delay_max".into(),
            "2".into(),
            "-rw_timeout".into(),
            "15000000".into(),
        ]);
        if let Some(fmt) = detect_stream_input_format(path) {
            ffmpeg_args.extend(["-f".into(), fmt.into()]);
        }
    }

    ffmpeg_args.extend(["-i".into(), path.to_string(), "-vn".into()]);
    if let Some(seconds) = max_seconds {
        ffmpeg_args.extend(["-t".into(), seconds.to_string()]);
    }
    ffmpeg_args.extend([
        "-f".into(),
        ffmpeg_format.ffmpeg_muxer().into(),
        "-acodec".into(),
        ffmpeg_format.ffmpeg_codec().into(),
        "-ac".into(),
        "2".into(),
        "-ar".into(),
        "44100".into(),
        "-".into(),
    ]);

    let mut child = Command::new("ffmpeg")
        .args(&ffmpeg_args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("failed to invoke ffmpeg for: {path}"))?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| anyhow!("failed to capture ffmpeg stdout"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| anyhow!("failed to capture ffmpeg stderr"))?;

    let stdout_reader =
        thread::spawn(move || read_to_end_limited(stdout, FFMPEG_MAX_PCM_BYTES, "ffmpeg stdout"));
    let stderr_reader = thread::spawn(move || {
        read_to_end_limited(stderr, FFMPEG_MAX_STDERR_BYTES, "ffmpeg stderr")
    });

    let timeout = ffmpeg_decode_timeout(max_seconds, stream_mode);
    let status = wait_child_with_timeout(&mut child, timeout)
        .with_context(|| format!("ffmpeg decode timeout for: {path}"))?;

    let stdout_bytes = stdout_reader
        .join()
        .map_err(|_| anyhow!("ffmpeg stdout reader thread panicked"))??;
    let stderr_bytes = stderr_reader
        .join()
        .map_err(|_| anyhow!("ffmpeg stderr reader thread panicked"))??;

    if !status.success() {
        let err = String::from_utf8_lossy(&stderr_bytes).trim().to_string();
        if !stream_mode {
            return Err(anyhow!("ffmpeg decode failed: {err}"));
        }
    }

    if stdout_bytes.is_empty() {
        return Err(anyhow!("ffmpeg decode produced no samples: {path}"));
    }

    let sample_width = match ffmpeg_format {
        FfmpegPcmFormat::S16 => 2,
        FfmpegPcmFormat::F32 => 4,
    };
    let mut samples = Vec::<f32>::with_capacity(stdout_bytes.len() / sample_width);
    match ffmpeg_format {
        FfmpegPcmFormat::S16 => {
            for chunk in stdout_bytes.chunks_exact(2) {
                let sample = i16::from_le_bytes([chunk[0], chunk[1]]) as f32 / i16::MAX as f32;
                samples.push(sample.clamp(-1.0, 1.0));
            }
        }
        FfmpegPcmFormat::F32 => {
            for chunk in stdout_bytes.chunks_exact(4) {
                samples.push(f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]));
            }
        }
    }

    if samples.len() < 2 {
        return Err(anyhow!("ffmpeg decode output too short: {path}"));
    }

    if samples.len() % 2 != 0 {
        samples.pop();
    }

    let frames = samples.len() / 2;

    Ok(DecodedTrack {
        samples: Arc::from(samples.into_boxed_slice()),
        frames,
        sample_rate: 44100.0,
    })
}

fn ffmpeg_decode_timeout(max_seconds: Option<u32>, stream_mode: bool) -> Duration {
    if !stream_mode {
        return FFMPEG_DECODE_TIMEOUT;
    }
    let chunk_seconds = max_seconds.unwrap_or(STREAM_CHUNK_SECONDS) as u64;
    Duration::from_secs((chunk_seconds + 180).max(FFMPEG_DECODE_TIMEOUT.as_secs()))
}

fn detect_stream_input_format(url: &str) -> Option<&'static str> {
    let url_lower = url.to_ascii_lowercase();
    if url_lower.contains(".aac") || url_lower.contains("aacp") {
        return Some("aac");
    }
    if url_lower.contains(".mp3") {
        return Some("mp3");
    }
    if url_lower.contains(".ogg") || url_lower.contains(".opus") {
        return Some("ogg");
    }

    let mut content_type = head_content_type(url);
    if content_type.is_none() {
        content_type = probe_content_type(url);
    }
    let ct = content_type?.to_ascii_lowercase();

    if ct.contains("aac") || ct.contains("aacp") {
        return Some("aac");
    }
    if ct.contains("mpeg") || ct.contains("mp3") {
        return Some("mp3");
    }
    if ct.contains("ogg") || ct.contains("opus") {
        return Some("ogg");
    }
    if ct.contains("flac") {
        return Some("flac");
    }
    if ct.contains("wav") {
        return Some("wav");
    }
    if ct.contains("mp4") || ct.contains("m4a") {
        return Some("mp4");
    }

    None
}

fn head_content_type(url: &str) -> Option<String> {
    let resp = ureq::head(url)
        .timeout(Duration::from_secs(8))
        .call()
        .ok()?;
    resp.header("Content-Type").map(str::to_string)
}

fn probe_content_type(url: &str) -> Option<String> {
    let resp = ureq::get(url)
        .timeout(Duration::from_secs(8))
        .set("Range", "bytes=0-0")
        .call()
        .ok()?;
    resp.header("Content-Type").map(str::to_string)
}

fn wait_child_with_timeout(
    child: &mut std::process::Child,
    timeout: Duration,
) -> Result<std::process::ExitStatus> {
    let started = Instant::now();
    loop {
        if let Some(status) = child
            .try_wait()
            .map_err(|err| anyhow!("failed waiting ffmpeg process: {err}"))?
        {
            return Ok(status);
        }
        if started.elapsed() >= timeout {
            let _ = child.kill();
            let _ = child.wait();
            return Err(anyhow!("ffmpeg process exceeded timeout of {:?}", timeout));
        }
        thread::sleep(Duration::from_millis(20));
    }
}

fn read_to_end_limited<R: Read>(mut reader: R, limit: usize, label: &str) -> Result<Vec<u8>> {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 8192];
    loop {
        let n = reader
            .read(&mut chunk)
            .map_err(|err| anyhow!("read error on {label}: {err}"))?;
        if n == 0 {
            break;
        }
        if buf.len().saturating_add(n) > limit {
            return Err(anyhow!("{label} exceeded limit of {limit} bytes"));
        }
        buf.extend_from_slice(&chunk[..n]);
    }
    Ok(buf)
}

fn prefers_ffmpeg_decode(path: &str) -> bool {
    let Some(ext) = Path::new(path).extension().and_then(|s| s.to_str()) else {
        return false;
    };

    matches!(
        ext.to_ascii_lowercase().as_str(),
        "m4a" | "aac" | "m4b" | "m4p" | "mp4" | "alac" | "wma" | "opus"
    )
}

pub fn is_supported_path(path: &Path) -> bool {
    let Some(ext) = path.extension().and_then(|s| s.to_str()) else {
        return false;
    };
    let ext = format!(".{}", ext.to_ascii_lowercase());
    SUPPORTED_EXTS.contains(&ext.as_str())
}

fn is_url(path: &str) -> bool {
    path.starts_with("http://") || path.starts_with("https://")
}

fn is_stream_end(err: &std::io::Error) -> bool {
    err.kind() == ErrorKind::UnexpectedEof
        || err.to_string().to_lowercase().contains("end of stream")
}

struct Biquad {
    freq: f32,
    q: f32,
    sr: f32,
    x1: [f32; 2],
    x2: [f32; 2],
    y1: [f32; 2],
    y2: [f32; 2],
    last_gain: f32,
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    inited: bool,
}

impl Biquad {
    fn new(freq: f32, q: f32, sr: f32) -> Self {
        Self {
            freq,
            q,
            sr,
            x1: [0.0; 2],
            x2: [0.0; 2],
            y1: [0.0; 2],
            y2: [0.0; 2],
            last_gain: 0.0,
            b0: 1.0,
            b1: 0.0,
            b2: 0.0,
            a1: 0.0,
            a2: 0.0,
            inited: false,
        }
    }

    fn reset(&mut self) {
        self.x1 = [0.0; 2];
        self.x2 = [0.0; 2];
        self.y1 = [0.0; 2];
        self.y2 = [0.0; 2];
        self.inited = false;
        self.last_gain = 0.0;
    }

    fn process(&mut self, sample: f32, channel: usize, gain_db: f32) -> f32 {
        if gain_db.abs() < 0.1 {
            return sample;
        }

        self.calc_coeffs(gain_db);

        let y = self.b0 * sample + self.b1 * self.x1[channel] + self.b2 * self.x2[channel]
            - self.a1 * self.y1[channel]
            - self.a2 * self.y2[channel];

        self.x2[channel] = self.x1[channel];
        self.x1[channel] = sample;
        self.y2[channel] = self.y1[channel];
        self.y1[channel] = y;

        y
    }

    fn calc_coeffs(&mut self, gain_db: f32) {
        if self.inited && (gain_db - self.last_gain).abs() < f32::EPSILON {
            return;
        }

        self.last_gain = gain_db;
        self.inited = true;

        let a = 10.0_f32.powf(gain_db / 40.0);
        let w0 = 2.0 * std::f32::consts::PI * self.freq / self.sr;
        let sin_w0 = w0.sin();
        let cos_w0 = w0.cos();
        let alpha = sin_w0 / (2.0 * self.q);

        let b0 = 1.0 + alpha * a;
        let b1 = -2.0 * cos_w0;
        let b2 = 1.0 - alpha * a;
        let a0 = 1.0 + alpha / a;
        let a1 = -2.0 * cos_w0;
        let a2 = 1.0 - alpha / a;

        self.b0 = b0 / a0;
        self.b1 = b1 / a0;
        self.b2 = b2 / a0;
        self.a1 = a1 / a0;
        self.a2 = a2 / a0;
    }
}
