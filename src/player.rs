use anyhow::{anyhow, Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Sample, SampleFormat, StreamConfig};
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

pub struct Player {
    state: Arc<Mutex<PlaybackState>>,
    _stream: cpal::Stream,
    output_sample_rate: f32,
}

impl Player {
    pub fn new() -> Result<Self> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or_else(|| anyhow!("no default output audio device found"))?;

        let supported = device
            .default_output_config()
            .context("failed to query default output config")?;

        let sample_format = supported.sample_format();
        let config: StreamConfig = supported.config();
        let out_sr = config.sample_rate.0 as f32;
        let channels = config.channels as usize;

        let state = Arc::new(Mutex::new(PlaybackState::new(out_sr)));
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
        })
    }

    pub fn output_sample_rate(&self) -> f32 {
        self.output_sample_rate
    }

    pub fn play_async(&self, path: &str) {
        let path = path.to_string();
        let state = self.state.clone();
        let load_token = {
            let mut state = lock_unpoison(&state);
            state.load_token = state.load_token.wrapping_add(1);
            state.preload_token = state.preload_token.wrapping_add(1);
            state.loading = true;
            state.last_error = None;
            state.playing = false;
            state.paused = false;
            state.track_done = false;
            state.src_pos = 0.0;
            state.preloaded = None;
            state.gapless_advanced = false;
            state.tap.clear();
            state.load_token
        };

        thread::spawn(move || {
            let decoded = decode_audio(&path).map(Arc::new);
            let mut state = lock_unpoison(&state);
            if state.load_token != load_token {
                return;
            }
            state.loading = false;
            match decoded {
                Ok(track) => {
                    state.last_error = None;
                    apply_decoded_track(&mut state, track);
                }
                Err(err) => {
                    state.track = None;
                    state.src_pos = 0.0;
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
        let (load_token, preload_token) = {
            let mut state = lock_unpoison(&state);
            state.preload_token = state.preload_token.wrapping_add(1);
            (state.load_token, state.preload_token)
        };

        thread::spawn(move || {
            let decoded = decode_audio(&path).map(Arc::new).ok();
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
        if state.playing {
            state.paused = !state.paused;
        }
    }

    pub fn stop(&self) {
        let mut state = lock_unpoison(&self.state);
        state.load_token = state.load_token.wrapping_add(1);
        state.preload_token = state.preload_token.wrapping_add(1);
        state.loading = false;
        state.track = None;
        state.preloaded = None;
        state.src_pos = 0.0;
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
        let Some(track) = state.track.as_ref() else {
            return;
        };

        let mut pos = state.src_pos;
        let delta_frames = delta.as_secs_f64() * track.sample_rate as f64;
        if backward {
            pos -= delta_frames;
        } else {
            pos += delta_frames;
        }

        let max_pos = track.frames.saturating_sub(1) as f64;
        state.src_pos = pos.clamp(0.0, max_pos);
        state.track_done = false;
    }

    pub fn position(&self) -> Duration {
        let state = lock_unpoison(&self.state);
        let Some(track) = state.track.as_ref() else {
            return Duration::ZERO;
        };
        Duration::from_secs_f64(state.src_pos / track.sample_rate as f64)
    }

    pub fn duration(&self) -> Duration {
        let state = lock_unpoison(&self.state);
        let Some(track) = state.track.as_ref() else {
            return Duration::ZERO;
        };
        Duration::from_secs_f64(track.frames as f64 / track.sample_rate as f64)
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
    state.track = Some(decoded.clone());
    state.src_pos = 0.0;
    state.src_step = decoded.sample_rate as f64 / state.output_sample_rate as f64;
    state.track_done = false;
    state.playing = true;
    state.paused = false;
    state.tap.clear();
    state.reset_filters();
}

struct PlaybackState {
    output_sample_rate: f32,
    track: Option<Arc<DecodedTrack>>,
    preloaded: Option<Arc<DecodedTrack>>,
    src_pos: f64,
    src_step: f64,
    volume_db: f32,
    eq_bands: [f32; 10],
    filters: [Biquad; 10],
    tap: RingTap,
    playing: bool,
    paused: bool,
    track_done: bool,
    mono: bool,
    loading: bool,
    load_token: u64,
    preload_token: u64,
    gapless_advanced: bool,
    last_error: Option<String>,
}

impl PlaybackState {
    fn new(output_sample_rate: f32) -> Self {
        Self {
            output_sample_rate,
            track: None,
            preloaded: None,
            src_pos: 0.0,
            src_step: 1.0,
            volume_db: 0.0,
            eq_bands: [0.0; 10],
            filters: array::from_fn(|i| Biquad::new(EQ_FREQS[i], 1.4, output_sample_rate)),
            tap: RingTap::new(4096),
            playing: false,
            paused: false,
            track_done: false,
            mono: false,
            loading: false,
            load_token: 0,
            preload_token: 0,
            gapless_advanced: false,
            last_error: None,
        }
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
            let Some(track_frames) = self.track.as_ref().map(|track| track.frames) else {
                return (0.0, 0.0);
            };

            if track_frames == 0 {
                if self.advance_to_preloaded() {
                    continue;
                }
                self.track_done = true;
                return (0.0, 0.0);
            }

            let last_frame = track_frames.saturating_sub(1) as f64;
            if self.src_pos >= last_frame {
                if self.advance_to_preloaded() {
                    continue;
                }
                self.track_done = true;
                return (0.0, 0.0);
            }

            let out = self
                .track
                .as_ref()
                .map(|track| track.sample_at(self.src_pos))
                .unwrap_or((0.0, 0.0));
            self.src_pos += self.src_step;

            if self.src_pos >= last_frame && !self.advance_to_preloaded() {
                self.track_done = true;
            }

            return out;
        }
    }

    fn advance_to_preloaded(&mut self) -> bool {
        let Some(next) = self.preloaded.take() else {
            return false;
        };

        self.track = Some(next.clone());
        self.src_pos = 0.0;
        self.src_step = next.sample_rate as f64 / self.output_sample_rate as f64;
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
    fn sample_at(&self, frame: f64) -> (f32, f32) {
        if self.frames == 0 {
            return (0.0, 0.0);
        }

        let i0 = frame.floor() as usize;
        let i1 = (i0 + 1).min(self.frames.saturating_sub(1));
        let t = (frame - i0 as f64) as f32;

        let base0 = i0 * 2;
        let base1 = i1 * 2;

        let l0 = self.samples.get(base0).copied().unwrap_or(0.0);
        let r0 = self.samples.get(base0 + 1).copied().unwrap_or(0.0);
        let l1 = self.samples.get(base1).copied().unwrap_or(l0);
        let r1 = self.samples.get(base1 + 1).copied().unwrap_or(r0);

        (l0 + (l1 - l0) * t, r0 + (r1 - r0) * t)
    }
}

fn decode_audio(path: &str) -> Result<DecodedTrack> {
    if is_url(path) {
        return decode_audio_ffmpeg(path);
    }

    if prefers_ffmpeg_decode(path) {
        if let Ok(track) = decode_audio_ffmpeg(path) {
            return Ok(track);
        }
    }

    match decode_audio_symphonia(path) {
        Ok(track) => Ok(track),
        Err(symphonia_err) => decode_audio_ffmpeg(path).map_err(|ffmpeg_err| {
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

fn decode_audio_ffmpeg(path: &str) -> Result<DecodedTrack> {
    let mut child = Command::new("ffmpeg")
        .args([
            "-v",
            "error",
            "-i",
            path,
            "-vn",
            "-f",
            "f32le",
            "-acodec",
            "pcm_f32le",
            "-ac",
            "2",
            "-ar",
            "44100",
            "-",
        ])
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

    let status = wait_child_with_timeout(&mut child, FFMPEG_DECODE_TIMEOUT)
        .with_context(|| format!("ffmpeg decode timeout for: {path}"))?;

    let stdout_bytes = stdout_reader
        .join()
        .map_err(|_| anyhow!("ffmpeg stdout reader thread panicked"))??;
    let stderr_bytes = stderr_reader
        .join()
        .map_err(|_| anyhow!("ffmpeg stderr reader thread panicked"))??;

    if !status.success() {
        let err = String::from_utf8_lossy(&stderr_bytes).trim().to_string();
        return Err(anyhow!("ffmpeg decode failed: {err}"));
    }

    if stdout_bytes.is_empty() {
        return Err(anyhow!("ffmpeg decode produced no samples: {path}"));
    }

    let mut samples = Vec::<f32>::with_capacity(stdout_bytes.len() / 4);
    for chunk in stdout_bytes.chunks_exact(4) {
        samples.push(f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]));
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
