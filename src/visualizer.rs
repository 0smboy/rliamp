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
    wave_buf: Vec<f32>,
    frame: u64,
    rows: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VisualizerMode {
    Neon,
    Bricks,
    Columns,
    Wave,
    Scatter,
    Flame,
    Retro,
    None,
}

const BRAILLE_BITS: [[u32; 2]; 4] = [[0x01, 0x08], [0x02, 0x10], [0x04, 0x20], [0x40, 0x80]];

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
            wave_buf: Vec::new(),
            frame: 0,
            rows: 4,
        }
    }

    pub fn set_rows(&mut self, rows: usize) {
        self.rows = rows.clamp(2, 32);
    }

    pub fn cycle_mode(&mut self) {
        self.mode = match self.mode {
            VisualizerMode::Neon => VisualizerMode::Bricks,
            VisualizerMode::Bricks => VisualizerMode::Columns,
            VisualizerMode::Columns => VisualizerMode::Wave,
            VisualizerMode::Wave => VisualizerMode::Scatter,
            VisualizerMode::Scatter => VisualizerMode::Flame,
            VisualizerMode::Flame => VisualizerMode::Retro,
            VisualizerMode::Retro => VisualizerMode::None,
            VisualizerMode::None => VisualizerMode::Neon,
        };
    }

    pub fn mode_name(&self) -> &str {
        match self.mode {
            VisualizerMode::Neon => "Neon",
            VisualizerMode::Bricks => "Bricks",
            VisualizerMode::Columns => "Columns",
            VisualizerMode::Wave => "Wave",
            VisualizerMode::Scatter => "Scatter",
            VisualizerMode::Flame => "Flame",
            VisualizerMode::Retro => "Retro",
            VisualizerMode::None => "Off",
        }
    }

    pub fn is_disabled(&self) -> bool {
        matches!(self.mode, VisualizerMode::None)
    }

    pub fn is_retro(&self) -> bool {
        matches!(self.mode, VisualizerMode::Retro)
    }

    pub fn analyze(&mut self, samples: &[f32]) -> [f32; NUM_BANDS] {
        let mut bands = [0.0; NUM_BANDS];
        if matches!(self.mode, VisualizerMode::None) {
            self.wave_buf.clear();
            self.prev = [0.0; NUM_BANDS];
            return bands;
        }
        self.frame = self.frame.wrapping_add(1);

        self.wave_buf.clear();
        self.wave_buf.extend(samples.iter().take(FFT_SIZE).copied());

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
            VisualizerMode::Columns => self.render_columns(bands),
            VisualizerMode::Wave => self.render_wave(),
            VisualizerMode::Scatter => self.render_scatter(bands),
            VisualizerMode::Flame => self.render_flame(bands),
            VisualizerMode::Retro => self.render_retro(bands),
            VisualizerMode::None => self.render_none(),
        }
    }

    fn render_none(&self) -> Vec<String> {
        let rows = self.rows.max(2);
        let width = NUM_BANDS * BAR_WIDTH + (NUM_BANDS - 1);
        vec![" ".repeat(width); rows]
    }

    fn render_neon(&self, bands: [f32; NUM_BANDS], phase: u64) -> Vec<String> {
        let rows = self.rows.max(2);
        let mut spark = String::new();

        for (idx, level) in bands.iter().enumerate() {
            spark.push_str(&sparkle_band(*level, phase, idx));

            if idx + 1 < NUM_BANDS {
                spark.push(' ');
            }
        }

        let mut lines = vec![spark];
        let bar_rows = rows - 1;
        for row in 0..bar_rows {
            let row_bottom = (bar_rows - 1 - row) as f32 / bar_rows as f32;
            let row_top = (bar_rows - row) as f32 / bar_rows as f32;
            let mut line = String::new();
            for (idx, level) in bands.iter().enumerate() {
                let block = if *level >= row_top {
                    '█'
                } else if *level > row_bottom {
                    let frac = (*level - row_bottom) / (row_top - row_bottom);
                    let i = (frac * 7.0).clamp(1.0, 7.0) as usize;
                    [' ', '▁', '▂', '▃', '▄', '▅', '▆', '▇'][i]
                } else {
                    ' '
                };
                line.push_str(&block.to_string().repeat(BAR_WIDTH));
                if idx + 1 < NUM_BANDS {
                    line.push(' ');
                }
            }
            lines.push(line);
        }
        lines
    }

    fn render_bricks(&self, bands: [f32; NUM_BANDS]) -> Vec<String> {
        let rows = self.rows.max(2);
        let mut lines = vec![String::new(); rows];
        for row in 0..rows {
            let threshold = (rows - 1 - row) as f32 / rows as f32;
            for (idx, level) in bands.iter().enumerate() {
                let glyph = if *level >= threshold {
                    if row <= rows / 4 {
                        '█'
                    } else if row <= rows / 2 {
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

    fn render_columns(&self, bands: [f32; NUM_BANDS]) -> Vec<String> {
        let rows = self.rows.max(2);
        const COLS_PER_BAND: usize = 5;
        let mut lines = vec![String::new(); rows];

        let mut cols = vec![0.0; NUM_BANDS * COLS_PER_BAND];
        for (band, level) in bands.iter().enumerate() {
            let next = if band + 1 < NUM_BANDS {
                bands[band + 1]
            } else {
                *level
            };
            for c in 0..COLS_PER_BAND {
                let t = c as f32 / COLS_PER_BAND as f32;
                cols[band * COLS_PER_BAND + c] = *level * (1.0 - t) + next * t;
            }
        }

        for row in 0..rows {
            let row_bottom = (rows - 1 - row) as f32 / rows as f32;
            let row_top = (rows - row) as f32 / rows as f32;

            for band in 0..NUM_BANDS {
                for c in 0..COLS_PER_BAND {
                    let level = cols[band * COLS_PER_BAND + c];
                    let ch = if level >= row_top {
                        '█'
                    } else if level > row_bottom {
                        let frac = (level - row_bottom) / (row_top - row_bottom);
                        let idx = (frac * 7.0).clamp(1.0, 7.0) as usize;
                        [' ', '▁', '▂', '▃', '▄', '▅', '▆', '▇'][idx]
                    } else {
                        ' '
                    };
                    lines[row].push(ch);
                }
                if band + 1 < NUM_BANDS {
                    lines[row].push(' ');
                }
            }
        }

        lines
    }

    fn render_wave(&self) -> Vec<String> {
        let rows = self.rows.max(2);
        const CHAR_COLS: usize = NUM_BANDS * BAR_WIDTH + (NUM_BANDS - 1);
        let dot_rows = rows * 4;
        let dot_cols = CHAR_COLS * 2;
        let mut lines = vec![String::new(); rows];

        let mut y_pos = vec![dot_rows / 2; dot_cols];
        if !self.wave_buf.is_empty() {
            for x in 0..dot_cols {
                let idx = x * self.wave_buf.len() / dot_cols;
                let idx = idx.min(self.wave_buf.len().saturating_sub(1));
                let sample = self.wave_buf[idx].clamp(-1.0, 1.0);
                let y = ((1.0 - sample) * (dot_rows.saturating_sub(1)) as f32 * 0.5) as isize;
                y_pos[x] = y.clamp(0, dot_rows.saturating_sub(1) as isize) as usize;
            }
        }

        for row in 0..rows {
            let dot_row_start = row * 4;
            for ch in 0..CHAR_COLS {
                let dot_col_start = ch * 2;
                let mut cell = 0x2800u32;
                for dc in 0..2 {
                    let x = dot_col_start + dc;
                    let y = y_pos[x];
                    let prev = if x > 0 { y_pos[x - 1] } else { y };
                    let lo = y.min(prev);
                    let hi = y.max(prev);
                    for dr in 0..4 {
                        let dot_y = dot_row_start + dr;
                        if dot_y >= lo && dot_y <= hi {
                            cell |= BRAILLE_BITS[dr][dc];
                        }
                    }
                }
                lines[row].push(char::from_u32(cell).unwrap_or(' '));
            }
        }

        lines
    }

    fn render_scatter(&self, bands: [f32; NUM_BANDS]) -> Vec<String> {
        let rows = self.rows.max(2);
        const CHAR_COLS: usize = NUM_BANDS * BAR_WIDTH + (NUM_BANDS - 1);
        let dot_rows = rows * 4;
        let mut lines = vec![String::new(); rows];

        for row in 0..rows {
            for col in 0..CHAR_COLS {
                let band = (col * NUM_BANDS / CHAR_COLS).min(NUM_BANDS - 1);
                let level = bands[band];
                let mut cell = 0x2800u32;
                for dr in 0..4 {
                    for dc in 0..2 {
                        let dot_row = row * 4 + dr;
                        let h = hash01(self.frame, band, dot_row, col * 2 + dc);
                        let gravity = 0.55 + 0.45 * (dot_row as f32 / (dot_rows - 1) as f32);
                        let threshold = (level * level * gravity).clamp(0.0, 1.0);
                        if h < threshold {
                            cell |= BRAILLE_BITS[dr][dc];
                        }
                    }
                }
                lines[row].push(char::from_u32(cell).unwrap_or(' '));
            }
        }

        lines
    }

    fn render_flame(&self, bands: [f32; NUM_BANDS]) -> Vec<String> {
        let rows = self.rows.max(2);
        let dot_rows = rows * 4;
        let mut lines = vec![String::new(); rows];

        for row in 0..rows {
            for band in 0..NUM_BANDS {
                for c in 0..BAR_WIDTH {
                    let mut cell = 0x2800u32;
                    for dr in 0..4 {
                        for dc in 0..2 {
                            let dot_row = row * 4 + dr;
                            let dot_col = c * 2 + dc;

                            let flame_y = (dot_rows - 1 - dot_row) as f32 / (dot_rows - 1) as f32;
                            if flame_y > bands[band] {
                                continue;
                            }

                            let t = self.frame as f32 * 0.3;
                            let wobble = (t + flame_y * 6.0 + band as f32 * 2.1).sin() * 1.5;
                            let center_col = BAR_WIDTH as f32;
                            let tip_narrow = 1.0 - flame_y / bands[band].max(0.01);
                            let flame_width = (0.3 + 0.7 * tip_narrow) * center_col;

                            let dist = (dot_col as f32 - center_col + 0.5 - wobble).abs();
                            if dist < flame_width {
                                let edge = dist / flame_width.max(0.01);
                                let h = hash01(self.frame.wrapping_add(31), band, dot_row, dot_col);
                                if edge < 0.7 || h < 0.6 {
                                    cell |= BRAILLE_BITS[dr][dc];
                                }
                            }
                        }
                    }
                    lines[row].push(char::from_u32(cell).unwrap_or(' '));
                }
                if band + 1 < NUM_BANDS {
                    lines[row].push(' ');
                }
            }
        }

        lines
    }

    fn render_retro(&self, bands: [f32; NUM_BANDS]) -> Vec<String> {
        const CHAR_COLS: usize = NUM_BANDS * BAR_WIDTH + (NUM_BANDS - 1);
        let rows = self.rows.max(4);
        let dot_rows = rows * 4;
        let dot_cols = CHAR_COLS * 2;

        let mut horizon_dot = dot_rows * 2 / 5;
        if horizon_dot < 2 {
            horizon_dot = 2;
        }
        let floor_rows = dot_rows.saturating_sub(horizon_dot);
        let center_x = (dot_cols.saturating_sub(1)) as f32 * 0.5;

        // 0=empty, 1=grid, 2=wave, 3=sun
        let mut dots = vec![vec![0u8; dot_cols]; dot_rows];

        // Sun (striped semicircle).
        let sun_r = horizon_dot as f32 * 0.85;
        for dy in 0..horizon_dot {
            let row_dist = (horizon_dot - dy) as f32;
            if row_dist > sun_r {
                continue;
            }
            let half_w = (sun_r * sun_r - row_dist * row_dist).sqrt();

            if row_dist < sun_r * 0.5 {
                let stripe_w = ((sun_r * 0.15) as usize).max(1);
                if (row_dist as usize / stripe_w) % 2 == 1 {
                    continue;
                }
            }

            let left = (center_x - half_w).floor().max(0.0) as usize;
            let right = (center_x + half_w)
                .ceil()
                .min(dot_cols.saturating_sub(1) as f32) as usize;
            for dx in left..=right {
                dots[dy][dx] = 3;
            }
        }

        // Horizon line.
        for dx in 0..dot_cols {
            dots[horizon_dot][dx] = 1;
        }

        // Perspective floor vertical lines.
        const NUM_V_LINES: usize = 18;
        for i in 0..=NUM_V_LINES {
            let bottom_x = i as f32 * (dot_cols.saturating_sub(1)) as f32 / NUM_V_LINES as f32;
            for dy in (horizon_dot + 1)..dot_rows {
                let t = (dy - horizon_dot) as f32 / floor_rows.saturating_sub(1).max(1) as f32;
                let screen_x = center_x + (bottom_x - center_x) * t;
                let ix = screen_x.round() as isize;
                if ix >= 0 && (ix as usize) < dot_cols {
                    dots[dy][ix as usize] = 1;
                }
            }
        }

        // Perspective floor horizontal lines with scroll.
        let scroll = (self.frame as f32 * 0.08) % 1.0;
        const NUM_H_LINES: usize = 10;
        for i in 0..NUM_H_LINES {
            let mut z = (i as f32 + scroll) / NUM_H_LINES as f32;
            if z > 1.0 {
                z -= 1.0;
            }
            let dy =
                horizon_dot + 1 + (z * z * floor_rows.saturating_sub(2).max(1) as f32) as usize;
            if dy > horizon_dot && dy < dot_rows {
                for dx in 0..dot_cols {
                    dots[dy][dx] = 1;
                }
            }
        }

        // Audio wave near horizon.
        let mut wave_y = vec![0usize; dot_cols];
        let max_wave = horizon_dot as f32 * 0.85;
        for dx in 0..dot_cols {
            let band_f =
                dx as f32 / dot_cols.saturating_sub(1).max(1) as f32 * (NUM_BANDS - 1) as f32;
            let bi = band_f.floor() as usize;
            let frac = band_f - bi as f32;
            let t = (1.0 - (frac * std::f32::consts::PI).cos()) * 0.5;

            let mut level = if bi >= NUM_BANDS - 1 {
                bands[NUM_BANDS - 1]
            } else {
                bands[bi] * (1.0 - t) + bands[bi + 1] * t
            };
            level = level.max(0.03);

            let wy = horizon_dot as isize - (level * max_wave) as isize;
            wave_y[dx] = wy.clamp(0, dot_rows.saturating_sub(1) as isize) as usize;
        }
        for dx in 0..dot_cols {
            let y = wave_y[dx];
            dots[y][dx] = 2;
            if dx > 0 {
                let lo = y.min(wave_y[dx - 1]);
                let hi = y.max(wave_y[dx - 1]);
                for fy in lo..=hi {
                    dots[fy][dx] = 2;
                }
            }
        }

        // Render braille cells. Use wave/sun marker glyphs so UI colorizer can apply
        // fixed retro colors with correct priority.
        let mut lines = vec![String::new(); rows];
        for row in 0..rows {
            let mut line = String::with_capacity(CHAR_COLS);
            let base = row * 4;
            for ch in 0..CHAR_COLS {
                let col_base = ch * 2;
                let mut cell = 0x2800u32;
                let mut has_wave = false;
                let mut has_sun = false;

                for dr in 0..4 {
                    for dc in 0..2 {
                        let dy = base + dr;
                        let dx = col_base + dc;
                        if dy >= dot_rows || dx >= dot_cols {
                            continue;
                        }
                        match dots[dy][dx] {
                            1 => cell |= BRAILLE_BITS[dr][dc],
                            2 => {
                                cell |= BRAILLE_BITS[dr][dc];
                                has_wave = true;
                            }
                            3 => {
                                cell |= BRAILLE_BITS[dr][dc];
                                has_sun = true;
                            }
                            _ => {}
                        }
                    }
                }

                let glyph = if has_wave {
                    '•'
                } else if has_sun {
                    '·'
                } else {
                    char::from_u32(cell).unwrap_or(' ')
                };
                line.push(glyph);
            }
            lines[row] = line;
        }

        lines
    }
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

fn hash01(frame: u64, band: usize, row: usize, col: usize) -> f32 {
    let mut z =
        frame ^ ((band as u64) << 42) ^ ((row as u64) << 21) ^ (col as u64) ^ 0x9E3779B97F4A7C15;
    z = z.wrapping_add(0x9E3779B97F4A7C15);
    z ^= z >> 30;
    z = z.wrapping_mul(0xBF58476D1CE4E5B9);
    z ^= z >> 27;
    z = z.wrapping_mul(0x94D049BB133111EB);
    z ^= z >> 31;
    ((z >> 11) as f64 / ((1u64 << 53) - 1) as f64) as f32
}
