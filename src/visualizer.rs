use rustfft::num_complex::Complex;
use rustfft::{Fft, FftPlanner};
use std::sync::Arc;

const NUM_BANDS: usize = 10;
const FFT_SIZE: usize = 2048;
const BAR_WIDTH: usize = 5;
const BAND_EDGES: [f32; 11] = [
    20.0, 100.0, 200.0, 400.0, 800.0, 1600.0, 3200.0, 6400.0, 12800.0, 16000.0, 20000.0,
];
const BLOCKS: [&str; 9] = [" ", "▁", "▂", "▃", "▄", "▅", "▆", "▇", "█"];

pub struct Visualizer {
    prev: [f32; NUM_BANDS],
    sample_rate: f32,
    fft: Arc<dyn Fft<f32>>,
    buffer: Vec<Complex<f32>>,
    scratch: Vec<Complex<f32>>,
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

    pub fn render(&self, bands: [f32; NUM_BANDS]) -> String {
        let mut out = String::new();
        for (idx, level) in bands.iter().enumerate() {
            let block_idx = ((*level * (BLOCKS.len() - 1) as f32) as usize).min(BLOCKS.len() - 1);
            let block = BLOCKS[block_idx];
            out.push_str(&block.repeat(BAR_WIDTH));
            if idx < NUM_BANDS - 1 {
                out.push(' ');
            }
        }
        out
    }
}
