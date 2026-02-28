use rustfft::num_complex::Complex;
use rustfft::{Fft, FftPlanner};
use std::sync::Arc;

const NUM_BANDS: usize = 10;
const FFT_SIZE: usize = 2048;
const BAR_WIDTH: usize = 7;
const BAND_EDGES: [f32; 11] = [
    20.0, 100.0, 200.0, 400.0, 800.0, 1600.0, 3200.0, 6400.0, 12800.0, 16000.0, 20000.0,
];
pub struct Visualizer {
    prev: [f32; NUM_BANDS],
    sample_rate: f32,
    fft: Arc<dyn Fft<f32>>,
    buffer: Vec<Complex<f32>>,
    scratch: Vec<Complex<f32>>,
    mode: VisualizerMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VisualizerMode {
    Neon,
    Bricks,
}

impl Visualizer {
    pub fn new(sample_rate: f32) -> Self {
        let mut planner = FftPlanner::<f32>::new();
        let fft = planner.plan_fft_forward(FFT_SIZE);
        let scratch_len = fft.get_inplace_scratch_len();

        Self {
            prev: [0.0; NUM_BANDS],
            sample_rate,
            fft,
            buffer: vec![Complex::new(0.0, 0.0); FFT_SIZE],
            scratch: vec![Complex::new(0.0, 0.0); scratch_len],
            mode: VisualizerMode::Neon,
        }
    }

    pub fn cycle_mode(&mut self) {
        self.mode = match self.mode {
            VisualizerMode::Neon => VisualizerMode::Bricks,
            VisualizerMode::Bricks => VisualizerMode::Neon,
        };
    }

    pub fn mode_name(&self) -> &str {
        match self.mode {
            VisualizerMode::Neon => "Neon",
            VisualizerMode::Bricks => "Bricks",
        }
    }

    pub fn analyze(&mut self, samples: &[f32]) -> [f32; NUM_BANDS] {
        let mut bands = [0.0; NUM_BANDS];

        if samples.is_empty() {
            for (idx, value) in self.prev.iter_mut().enumerate() {
                *value *= 0.8;
                bands[idx] = *value;
            }
            return bands;
        }

        for entry in &mut self.buffer {
            entry.re = 0.0;
            entry.im = 0.0;
        }

        for (i, sample) in samples.iter().take(FFT_SIZE).enumerate() {
            let w = 0.5
                * (1.0 - (2.0 * std::f32::consts::PI * i as f32 / (FFT_SIZE as f32 - 1.0)).cos());
            self.buffer[i].re = sample * w;
        }

        self.fft
            .process_with_scratch(&mut self.buffer, &mut self.scratch);

        let bin_hz = self.sample_rate / FFT_SIZE as f32;
        let half_len = self.buffer.len() / 2;

        for band in 0..NUM_BANDS {
            let mut lo = (BAND_EDGES[band] / bin_hz) as usize;
            let mut hi = (BAND_EDGES[band + 1] / bin_hz) as usize;

            lo = lo.max(1);
            hi = hi.min(half_len.saturating_sub(1));

            let mut sum = 0.0;
            let mut count = 0usize;
            for bin in lo..=hi {
                sum += self.buffer[bin].norm();
                count += 1;
            }

            if count > 0 {
                sum /= count as f32;
            }

            let mut level = 0.0;
            if sum > 0.0 {
                level = (20.0 * sum.log10() + 10.0) / 50.0;
            }

            level = level.clamp(0.0, 1.0);

            if level > self.prev[band] {
                level = level * 0.6 + self.prev[band] * 0.4;
            } else {
                level = level * 0.25 + self.prev[band] * 0.75;
            }

            self.prev[band] = level;
            bands[band] = level;
        }

        bands
    }

    pub fn render(&self, bands: [f32; NUM_BANDS], phase: u64) -> Vec<String> {
        match self.mode {
            VisualizerMode::Neon => self.render_neon(bands, phase),
            VisualizerMode::Bricks => self.render_bricks(bands),
        }
    }

    fn render_neon(&self, bands: [f32; NUM_BANDS], phase: u64) -> Vec<String> {
        let mut spark = String::new();
        let mut top = String::new();
        let mut mid = String::new();
        let mut low = String::new();

        for (idx, level) in bands.iter().enumerate() {
            spark.push_str(&sparkle_band(*level, phase, idx));
            top.push_str(&level_band(
                *level,
                [0.72, 0.62, 0.52],
                ['█', '▓', '▒'],
                BAR_WIDTH,
            ));
            mid.push_str(&level_band(
                *level,
                [0.52, 0.38, 0.25],
                ['█', '▓', '▒'],
                BAR_WIDTH,
            ));
            low.push_str(&level_band(
                *level,
                [0.28, 0.18, 0.10],
                ['█', '▒', '░'],
                BAR_WIDTH,
            ));

            if idx + 1 < NUM_BANDS {
                spark.push(' ');
                top.push(' ');
                mid.push(' ');
                low.push(' ');
            }
        }

        vec![spark, top, mid, low]
    }

    fn render_bricks(&self, bands: [f32; NUM_BANDS]) -> Vec<String> {
        const ROWS: usize = 4;
        let mut lines = vec![String::new(); ROWS];
        let thresholds = [0.72, 0.50, 0.30, 0.14];

        for (row, threshold) in thresholds.iter().enumerate() {
            for (idx, level) in bands.iter().enumerate() {
                let glyph = if *level >= *threshold {
                    if row == 0 {
                        '█'
                    } else if row == 1 {
                        '▓'
                    } else {
                        '▄'
                    }
                } else {
                    ' '
                };
                lines[row].push_str(&glyph.to_string().repeat(BAR_WIDTH));
                if idx + 1 < NUM_BANDS {
                    lines[row].push(' ');
                }
            }
        }

        lines
    }
}

fn level_band(level: f32, thresholds: [f32; 3], chars: [char; 3], width: usize) -> String {
    let ch = if level >= thresholds[0] {
        chars[0]
    } else if level >= thresholds[1] {
        chars[1]
    } else if level >= thresholds[2] {
        chars[2]
    } else {
        ' '
    };

    if ch == ' ' {
        return " ".repeat(width);
    }
    ch.to_string().repeat(width)
}

fn sparkle_band(level: f32, phase: u64, idx: usize) -> String {
    if level < 0.04 {
        return " ".repeat(BAR_WIDTH);
    }

    let sparkle = if level > 0.65 {
        if (phase + idx as u64 * 3).is_multiple_of(4) {
            '✦'
        } else {
            '•'
        }
    } else if level > 0.28 {
        if (phase + idx as u64 * 5).is_multiple_of(3) {
            '•'
        } else {
            '·'
        }
    } else {
        if (phase + idx as u64 * 7).is_multiple_of(2) {
            '·'
        } else {
            '.'
        }
    };

    let left = BAR_WIDTH / 2;
    let right = BAR_WIDTH.saturating_sub(left + 1);
    format!("{}{}{}", " ".repeat(left), sparkle, " ".repeat(right))
}
