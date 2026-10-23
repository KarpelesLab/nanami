//! Glottal source: Liljencrants–Fant (LF) flow-derivative model driven by
//! Fant's (1995) single "Rd" voice-quality parameter, with jitter, shimmer,
//! flutter and creak. Produces the glottal flow derivative (what excites the
//! vocal tract once lip radiation is folded in) and the normalized flow
//! shape, used to modulate aspiration noise.
//!
//! References:
//! - Fant, Liljencrants & Lin (1985), "A four-parameter model of glottal flow".
//! - Fant (1995), "The LF-model revisited. Transformations and frequency domain analysis".
//! - Klatt & Klatt (1990) for the flutter formulation.

use crate::dsp::Rng;
use std::f64::consts::PI;

/// LF pulse for a normalized period T0 = 1 and excitation strength Ee = 1.
#[derive(Clone, Debug)]
pub struct LfShape {
    pub tp: f64,
    pub te: f64,
    pub ta: f64,
    alpha: f64,
    eps: f64,
    omega: f64,
    e0: f64,
    /// Flow at te (start of return phase), used to integrate the return.
    u_te: f64,
    /// Peak flow, for normalization.
    u_peak: f64,
}

impl LfShape {
    /// Build an LF pulse from Rd (0.3 = pressed/tense, 1.0 = modal, 2.7 = breathy).
    pub fn from_rd(rd: f64) -> LfShape {
        Self::from_rd_closure(rd, 1.0)
    }

    /// Like `from_rd`, with the return phase (Ta) scaled by `closure`
    /// (< 1 = more abrupt closure → more high-frequency harmonic energy).
    pub fn from_rd_closure(rd: f64, closure: f64) -> LfShape {
        let rd = rd.clamp(0.3, 2.7);
        // Fant (1995) statistical regressions.
        let ra = (-1.0 + 4.8 * rd) / 100.0 * closure.clamp(0.1, 2.0);
        let rk = (22.4 + 11.8 * rd) / 100.0;
        let rg = rk / (4.0 * (0.11 * rd / (0.5 + 1.2 * rk) - ra));
        let tp = 1.0 / (2.0 * rg);
        let te = (tp * (1.0 + rk)).min(0.98);
        let ta = ra.max(0.002).min((1.0 - te) * 0.9);
        Self::from_timing(tp, te, ta)
    }

    pub fn from_timing(tp: f64, te: f64, ta: f64) -> LfShape {
        let d = 1.0 - te;
        // Return phase: eps * ta = 1 - exp(-eps * (1 - te)), Newton from 1/ta.
        let mut eps = 1.0 / ta;
        for _ in 0..50 {
            let f = eps * ta - 1.0 + (-eps * d).exp();
            let fp = ta - d * (-eps * d).exp();
            let next = eps - f / fp;
            if !next.is_finite() || next <= 0.0 {
                break;
            }
            if (next - eps).abs() < 1e-12 * eps {
                eps = next;
                break;
            }
            eps = next;
        }
        let omega = PI / tp;
        let (s, c) = (omega * te).sin_cos();
        // Integral of the return phase (negative flow decrease).
        let ed = (-eps * d).exp();
        let i2 = -(1.0 / (eps * ta)) * ((1.0 - ed) / eps - d * ed);
        // Net-flow condition: integral over the period of E is zero.
        let balance = |alpha: f64| -> f64 {
            let i1 = -(alpha * s - omega * c + omega * (-alpha * te).exp()) / (s * (alpha * alpha + omega * omega));
            i1 + i2
        };
        let alpha = find_root(balance, -60.0, 600.0).unwrap_or(0.0);
        let e0 = -1.0 / ((alpha * te).exp() * s);
        let mut shape = LfShape { tp, te, ta, alpha, eps, omega, e0, u_te: 0.0, u_peak: 1.0 };
        shape.u_te = shape.open_flow(te);
        shape.u_peak = shape.open_flow(tp).max(1e-9);
        shape
    }

    fn open_flow(&self, t: f64) -> f64 {
        let (a, w) = (self.alpha, self.omega);
        let (s, c) = (w * t).sin_cos();
        self.e0 * ((a * t).exp() * (a * s - w * c) + w) / (a * a + w * w)
    }

    /// Flow derivative at normalized time t in [0, 1).
    #[inline]
    pub fn derivative(&self, t: f64) -> f64 {
        if t <= self.te {
            self.e0 * (self.alpha * t).exp() * (self.omega * t).sin()
        } else {
            let d = 1.0 - self.te;
            -(1.0 / (self.eps * self.ta)) * ((-self.eps * (t - self.te)).exp() - (-self.eps * d).exp())
        }
    }

    /// Glottal flow at normalized time t, normalized to peak 1.
    #[inline]
    pub fn flow(&self, t: f64) -> f64 {
        let u = if t <= self.te {
            self.open_flow(t)
        } else {
            let (d, x) = (1.0 - self.te, t - self.te);
            self.u_te - (1.0 / (self.eps * self.ta)) * ((1.0 - (-self.eps * x).exp()) / self.eps - x * (-self.eps * d).exp())
        };
        (u / self.u_peak).max(0.0)
    }

    /// Peak flow per unit Ee for a 1-second period (scales as T0 for real periods).
    pub fn peak_flow(&self) -> f64 {
        self.u_peak
    }
}

/// Scan for a sign change then bisect.
fn find_root(f: impl Fn(f64) -> f64, lo: f64, hi: f64) -> Option<f64> {
    let steps = 400;
    let mut a = lo;
    let mut fa = f(a);
    for i in 1..=steps {
        let b = lo + (hi - lo) * i as f64 / steps as f64;
        let fb = f(b);
        if fa.is_finite() && fb.is_finite() && fa.signum() != fb.signum() {
            let (mut x0, mut x1, mut f0) = (a, b, fa);
            for _ in 0..80 {
                let m = 0.5 * (x0 + x1);
                let fm = f(m);
                if fm.signum() == f0.signum() {
                    x0 = m;
                    f0 = fm;
                } else {
                    x1 = m;
                }
            }
            return Some(0.5 * (x0 + x1));
        }
        a = b;
        fa = fb;
    }
    None
}

/// Per-sample voice-source controls.
#[derive(Clone, Debug)]
pub struct SourceParams {
    pub f0: f64,
    pub rd: f64,
    /// Vibration amplitude 0..1 (from aerodynamics + adduction).
    pub amplitude: f64,
    /// Relative jitter (period perturbation), e.g. 0.005.
    pub jitter: f64,
    /// Relative shimmer (amplitude perturbation), e.g. 0.03.
    pub shimmer: f64,
    /// Klatt-style flutter amount (0..1 = 0..±1% slow F0 wander).
    pub flutter: f64,
    /// Creak / vocal fry 0..1 (irregular, damped pulses).
    pub creak: f64,
    /// Return-phase scale (1 = Fant's Rd regression).
    pub closure: f64,
}

pub struct Glottis {
    fs: f64,
    phase: f64,
    shape: LfShape,
    shape_rd: f64,
    shape_closure: f64,
    /// Values latched at the start of each period.
    period_f0: f64,
    period_amp: f64,
    alt: bool,
    time: f64,
    rng: Rng,
}

pub struct GlottisOut {
    /// Flow derivative (arbitrary units, ~ m^3/s^2 scaled).
    pub dflow: f64,
    /// Normalized pulsatile flow 0..1 (0 when closed).
    pub flow: f64,
}

impl Glottis {
    pub fn new(fs: f64, seed: u64) -> Self {
        Glottis {
            fs,
            phase: 0.0,
            shape: LfShape::from_rd(1.0),
            shape_rd: 1.0,
            shape_closure: 1.0,
            period_f0: 120.0,
            period_amp: 0.0,
            alt: false,
            time: 0.0,
            rng: Rng::new(seed),
        }
    }

    pub fn step(&mut self, p: &SourceParams) -> GlottisOut {
        self.time += 1.0 / self.fs;
        if self.phase == 0.0 || self.phase >= 1.0 {
            self.phase = self.phase.fract();
            self.start_period(p);
        }
        let t = self.phase;
        // Fade amplitude within the period toward the requested level so that
        // voicing onsets/offsets are not quantized to period boundaries.
        let a = self.period_amp;
        let out = GlottisOut {
            dflow: a * self.shape.derivative(t) * self.period_f0,
            flow: a * self.shape.flow(t),
        };
        self.phase += self.period_f0 / self.fs;
        out
    }

    fn start_period(&mut self, p: &SourceParams) {
        let rd = (p.rd + p.creak * -0.4).clamp(0.3, 2.7);
        if (rd - self.shape_rd).abs() > 0.005 || p.closure != self.shape_closure {
            self.shape = LfShape::from_rd_closure(rd, p.closure);
            self.shape_closure = p.closure;
            self.shape_rd = rd;
        }
        let t = self.time;
        let flutter = p.flutter * 0.01 * ((2.0 * PI * 12.7 * t).sin() + (2.0 * PI * 7.1 * t).sin() + (2.0 * PI * 4.7 * t).sin()) / 3.0;
        let jitter = p.jitter * self.rng.gauss();
        let mut f0 = p.f0 * (1.0 + flutter + jitter);
        let mut amp = p.amplitude * (1.0 + p.shimmer * self.rng.gauss()).max(0.0);
        if p.creak > 0.0 {
            // Fry: lowered, irregular periods with alternating strong/weak pulses.
            self.alt = !self.alt;
            f0 *= 1.0 - p.creak * (0.35 + 0.35 * self.rng.uniform());
            if self.alt {
                amp *= 1.0 - 0.6 * p.creak;
            }
        }
        self.period_f0 = f0.clamp(20.0, 2000.0);
        self.period_amp = amp;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lf_zero_net_flow_and_negative_peak() {
        for &rd in &[0.3, 0.6, 1.0, 1.5, 2.0, 2.7] {
            let s = LfShape::from_rd(rd);
            let n = 20000;
            let mut sum = 0.0;
            let mut min: f64 = 0.0;
            for i in 0..n {
                let e = s.derivative(i as f64 / n as f64);
                sum += e / n as f64;
                min = min.min(e);
            }
            assert!(sum.abs() < 1e-3, "rd {rd}: net flow {sum}");
            assert!((min + 1.0).abs() < 0.02, "rd {rd}: Ee {min}");
            assert!(s.flow(0.999) < 0.02, "rd {rd}: flow returns to zero");
        }
    }
}
