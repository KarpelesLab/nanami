//! Small DSP toolkit: RNG, simple filters, FFT, decimation.

use std::f64::consts::PI;

/// xorshift64* — fast, deterministic, good enough for audio noise.
#[derive(Clone)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }
    #[inline]
    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    /// Uniform in [0, 1).
    #[inline]
    pub fn uniform(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 * (1.0 / (1u64 << 53) as f64)
    }
    /// Uniform in [-1, 1).
    #[inline]
    pub fn bipolar(&mut self) -> f64 {
        self.uniform() * 2.0 - 1.0
    }
    /// Approximately Gaussian (Irwin–Hall, 4 terms), unit variance.
    #[inline]
    pub fn gauss(&mut self) -> f64 {
        let s = self.uniform() + self.uniform() + self.uniform() + self.uniform();
        (s - 2.0) * 3f64.sqrt()
    }
}

/// One-pole lowpass: y += a (x - y).
#[derive(Clone, Default)]
pub struct OnePole {
    a: f64,
    y: f64,
}

impl OnePole {
    pub fn new(cutoff_hz: f64, fs: f64) -> Self {
        let mut f = OnePole::default();
        f.set_cutoff(cutoff_hz, fs);
        f
    }
    pub fn set_cutoff(&mut self, cutoff_hz: f64, fs: f64) {
        self.a = 1.0 - (-2.0 * PI * cutoff_hz / fs).exp();
    }
    #[inline]
    pub fn process(&mut self, x: f64) -> f64 {
        self.y += self.a * (x - self.y);
        self.y
    }
}

/// RBJ-cookbook biquad, direct form I.
#[derive(Clone, Default)]
pub struct Biquad {
    b0: f64,
    b1: f64,
    b2: f64,
    a1: f64,
    a2: f64,
    x1: f64,
    x2: f64,
    y1: f64,
    y2: f64,
}

impl Biquad {
    fn from_coeffs(b0: f64, b1: f64, b2: f64, a0: f64, a1: f64, a2: f64) -> Self {
        Biquad {
            b0: b0 / a0,
            b1: b1 / a0,
            b2: b2 / a0,
            a1: a1 / a0,
            a2: a2 / a0,
            ..Default::default()
        }
    }
    pub fn bandpass(f0: f64, q: f64, fs: f64) -> Self {
        let w = 2.0 * PI * f0 / fs;
        let alpha = w.sin() / (2.0 * q);
        Self::from_coeffs(alpha, 0.0, -alpha, 1.0 + alpha, -2.0 * w.cos(), 1.0 - alpha)
    }
    pub fn highpass(f0: f64, q: f64, fs: f64) -> Self {
        let w = 2.0 * PI * f0 / fs;
        let (c, alpha) = (w.cos(), w.sin() / (2.0 * q));
        Self::from_coeffs((1.0 + c) / 2.0, -(1.0 + c), (1.0 + c) / 2.0, 1.0 + alpha, -2.0 * c, 1.0 - alpha)
    }
    pub fn lowpass(f0: f64, q: f64, fs: f64) -> Self {
        let w = 2.0 * PI * f0 / fs;
        let (c, alpha) = (w.cos(), w.sin() / (2.0 * q));
        Self::from_coeffs((1.0 - c) / 2.0, 1.0 - c, (1.0 - c) / 2.0, 1.0 + alpha, -2.0 * c, 1.0 - alpha)
    }
    #[inline]
    pub fn process(&mut self, x: f64) -> f64 {
        let y = self.b0 * x + self.b1 * self.x1 + self.b2 * self.x2 - self.a1 * self.y1 - self.a2 * self.y2;
        self.x2 = self.x1;
        self.x1 = x;
        self.y2 = self.y1;
        self.y1 = y;
        y
    }
}

/// In-place iterative radix-2 FFT. `re.len()` must be a power of two.
pub fn fft(re: &mut [f64], im: &mut [f64]) {
    let n = re.len();
    assert!(n.is_power_of_two() && im.len() == n);
    let mut j = 0;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j |= bit;
        if i < j {
            re.swap(i, j);
            im.swap(i, j);
        }
    }
    let mut len = 2;
    while len <= n {
        let ang = -2.0 * PI / len as f64;
        let (wr, wi) = (ang.cos(), ang.sin());
        for start in (0..n).step_by(len) {
            let (mut cr, mut ci) = (1.0, 0.0);
            for k in 0..len / 2 {
                let (a, b) = (start + k, start + k + len / 2);
                let tr = re[b] * cr - im[b] * ci;
                let ti = re[b] * ci + im[b] * cr;
                re[b] = re[a] - tr;
                im[b] = im[a] - ti;
                re[a] += tr;
                im[a] += ti;
                let ncr = cr * wr - ci * wi;
                ci = cr * wi + ci * wr;
                cr = ncr;
            }
        }
        len <<= 1;
    }
}

/// Windowed-sinc FIR lowpass kernel (Blackman window), cutoff as fraction of fs.
pub fn sinc_lowpass(taps: usize, cutoff: f64) -> Vec<f64> {
    let m = (taps - 1) as f64;
    let mut h: Vec<f64> = (0..taps)
        .map(|i| {
            let x = i as f64 - m / 2.0;
            let sinc = if x == 0.0 { 2.0 * cutoff } else { (2.0 * PI * cutoff * x).sin() / (PI * x) };
            let w = 0.42 - 0.5 * (2.0 * PI * i as f64 / m).cos() + 0.08 * (4.0 * PI * i as f64 / m).cos();
            sinc * w
        })
        .collect();
    let sum: f64 = h.iter().sum();
    h.iter_mut().for_each(|v| *v /= sum);
    h
}

/// Lowpass and decimate by an integer factor.
pub fn decimate(input: &[f64], factor: usize) -> Vec<f64> {
    if factor == 1 {
        return input.to_vec();
    }
    let h = sinc_lowpass(32 * factor + 1, 0.46 / factor as f64);
    let half = h.len() / 2;
    (0..input.len() / factor)
        .map(|o| {
            let c = o * factor;
            let mut acc = 0.0;
            for (k, hk) in h.iter().enumerate() {
                let idx = c as isize + k as isize - half as isize;
                if idx >= 0 && (idx as usize) < input.len() {
                    acc += hk * input[idx as usize];
                }
            }
            acc
        })
        .collect()
}

#[inline]
pub fn clamp01(x: f64) -> f64 {
    x.clamp(0.0, 1.0)
}

#[inline]
pub fn smoothstep(e0: f64, e1: f64, x: f64) -> f64 {
    let t = clamp01((x - e0) / (e1 - e0));
    t * t * (3.0 - 2.0 * t)
}

#[inline]
pub fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}

pub fn semitones_to_ratio(st: f64) -> f64 {
    (st / 12.0).exp2()
}
