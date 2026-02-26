use rand::Rng;

const RAIN_CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789@#$%&*";

#[derive(Clone, Copy)]
struct Cell {
    ch: char,
}

impl Default for Cell {
    fn default() -> Self {
        Self { ch: ' ' }
    }
}

#[derive(Clone, Copy)]
struct Drop {
    x: f32,
    y: f32,
    speed: f32,
    length: usize,
}

impl Drop {
    fn random(width: usize, height: usize) -> Self {
        let mut rng = rand::thread_rng();
        let x = if width == 0 {
            0.0
        } else {
            rng.gen_range(0.0..width as f32)
        };
        let min_y = -(height as f32).max(8.0);
        let y = rng.gen_range(min_y..height as f32);
        Self {
            x,
            y,
            speed: rng.gen_range(0.45..1.45),
            length: rng.gen_range(5..18),
        }
    }

    fn reset(&mut self, width: usize, height: usize) {
        *self = Self::random(width, height);
        self.y = -(self.length as f32);
    }
}

#[derive(Clone, Copy)]
struct Spark {
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    life: f32,
    max_life: f32,
    ch: char,
}

impl Spark {
    fn new(x: f32, y: f32) -> Self {
        let mut rng = rand::thread_rng();
        let angle = rng.gen_range(std::f32::consts::PI..std::f32::consts::TAU);
        let speed = rng.gen_range(0.7..2.2);
        Self {
            x,
            y,
            vx: angle.cos() * speed * 1.6,
            vy: angle.sin() * speed,
            life: rng.gen_range(9.0..18.0),
            max_life: 18.0,
            ch: '•',
        }
    }

    fn update(&mut self) {
        self.x += self.vx;
        self.y += self.vy;
        self.vy += 0.14;
        self.vx *= 0.96;
        self.life -= 1.0;

        let ratio = self.life / self.max_life;
        self.ch = if ratio > 0.66 {
            '✦'
        } else if ratio > 0.33 {
            '•'
        } else {
            '·'
        };
    }
}

pub struct ParticleBackground {
    width: usize,
    height: usize,
    cells: Vec<Cell>,
    drops: Vec<Drop>,
    sparks: Vec<Spark>,
}

impl ParticleBackground {
    pub fn new(width: usize, height: usize) -> Self {
        let mut bg = Self {
            width,
            height,
            cells: vec![Cell::default(); width.saturating_mul(height)],
            drops: Vec::new(),
            sparks: Vec::new(),
        };
        bg.reset_drops();
        bg
    }

    pub fn resize(&mut self, width: usize, height: usize) {
        if self.width == width && self.height == height {
            return;
        }
        self.width = width;
        self.height = height;
        self.cells = vec![Cell::default(); width.saturating_mul(height)];
        self.reset_drops();
        self.sparks.clear();
    }

    pub fn tick(&mut self) {
        if self.width == 0 || self.height == 0 {
            return;
        }
        self.cells.fill(Cell::default());
        self.update_drops();
        self.update_sparks();
    }

    pub fn ch_at(&self, x: usize, y: usize) -> char {
        if x >= self.width || y >= self.height {
            return ' ';
        }
        self.cells[y * self.width + x].ch
    }

    fn reset_drops(&mut self) {
        self.drops.clear();
        if self.width == 0 || self.height == 0 {
            return;
        }
        let target = (self.width / 5).max(8);
        for _ in 0..target {
            self.drops.push(Drop::random(self.width, self.height));
        }
    }

    fn update_drops(&mut self) {
        let mut rng = rand::thread_rng();
        for drop in &mut self.drops {
            drop.y += drop.speed;
            if drop.y - drop.length as f32 > self.height as f32 {
                let burst = rng.gen_range(6..12);
                for _ in 0..burst {
                    self.sparks
                        .push(Spark::new(drop.x, (self.height.saturating_sub(1)) as f32));
                }
                drop.reset(self.width, self.height);
            }

            for i in 0..drop.length {
                let yy = drop.y as isize - i as isize;
                let xx = drop.x as isize;
                if xx < 0 || xx >= self.width as isize || yy < 0 || yy >= self.height as isize {
                    continue;
                }
                let idx = yy as usize * self.width + xx as usize;
                let ch = if i == 0 {
                    '█'
                } else if i > drop.length.saturating_sub(3) {
                    random_tail_char(&mut rng).to_ascii_lowercase()
                } else {
                    random_tail_char(&mut rng)
                };
                self.cells[idx] = Cell { ch };
            }
        }
    }

    fn update_sparks(&mut self) {
        for i in (0..self.sparks.len()).rev() {
            self.sparks[i].update();
            if self.sparks[i].life <= 0.0 {
                self.sparks.remove(i);
                continue;
            }
            let s = self.sparks[i];
            if s.x < 0.0 || s.x >= self.width as f32 || s.y < 0.0 || s.y >= self.height as f32 {
                continue;
            }
            let idx = s.y as usize * self.width + s.x as usize;
            self.cells[idx] = Cell { ch: s.ch };
        }
    }
}

fn random_tail_char(rng: &mut rand::rngs::ThreadRng) -> char {
    RAIN_CHARS[rng.gen_range(0..RAIN_CHARS.len())] as char
}
