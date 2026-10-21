//! Acoustic analysis helpers used for calibration and self-checks:
//! tract transfer-function formants, and LPC formant tracking of rendered audio.

use crate::articulation::{Anatomy, Articulators, area_function};
use crate::dsp::fft;
use crate::synth::INTERNAL_RATE;
use crate::tract::Tract;

/// Formant frequencies of the static tract shape for `art`, from the
/// impulse response of the very same waveguide used for synthesis.
pub fn tract_formants(anat: &Anatomy, nose_cm: f64, art: &Articulators, count: usize) -> Vec<f64> {
    let spec = tract_response_db(anat, nose_cm, art, 16384);
    pick_peaks(&spec, INTERNAL_RATE, 16384, 120.0, 6000.0, count)
}

/// Magnitude response (dB) of the tract, first half of an `nfft` FFT.
pub fn tract_response_db(anat: &Anatomy, nose_cm: f64, art: &Articulators, nfft: usize) -> Vec<f64> {
    let mut tract = Tract::new(INTERNAL_RATE, anat.length_cm, nose_cm);
    let mut areas = Vec::new();
    area_function(art, anat, tract.n, &mut areas);
    tract.snap(&areas, art.velum);
    tract.glottal_reflection = 0.95; // closed phase
    let mut re: Vec<f64> = (0..nfft).map(|i| tract.step(if i == 0 { 1.0 } else { 0.0 })).collect();
    let mut im = vec![0.0; nfft];
    fft(&mut re, &mut im);
    (0..nfft / 2).map(|k| 10.0 * (re[k] * re[k] + im[k] * im[k] + 1e-30).log10()).collect()
}

/// Local maxima of a dB spectrum within [lo, hi] Hz, with parabolic refinement.
pub fn pick_peaks(spec: &[f64], fs: f64, nfft: usize, lo: f64, hi: f64, count: usize) -> Vec<f64> {
    let bin = fs / nfft as f64;
    let (k0, k1) = (((lo / bin) as usize).max(1), ((hi / bin) as usize).min(spec.len() - 2));
    let mut peaks = Vec::new();
    for k in k0..=k1 {
        if spec[k] > spec[k - 1] && spec[k] >= spec[k + 1] {
            let (a, b, c) = (spec[k - 1], spec[k], spec[k + 1]);
            let d = 0.5 * (a - c) / (a - 2.0 * b + c);
            peaks.push((k as f64 + if d.is_finite() { d } else { 0.0 }) * bin);
            if peaks.len() == count {
                break;
            }
        }
    }
    peaks
}

/// Autocorrelation LPC (Levinson–Durbin). Returns a[0..=order], a[0] = 1.
pub fn lpc(frame: &[f64], order: usize) -> Vec<f64> {
    let n = frame.len();
    let mut r = vec![0.0; order + 1];
    for (lag, rv) in r.iter_mut().enumerate() {
        *rv = (lag..n).map(|i| frame[i] * frame[i - lag]).sum();
    }
    r[0] *= 1.0 + 1e-9;
    let mut a = vec![0.0; order + 1];
    a[0] = 1.0;
    let mut err = r[0].max(1e-30);
    for i in 1..=order {
        let mut acc = r[i];
        for j in 1..i {
            acc += a[j] * r[i - j];
        }
        let k = -acc / err;
        let prev = a.clone();
        for j in 1..i {
            a[j] = prev[j] + k * prev[i - j];
        }
        a[i] = k;
        err *= 1.0 - k * k;
        if err <= 0.0 {
            break;
        }
    }
    a
}

/// LPC formant estimate of a voiced segment of audio at rate `fs`
/// (peaks of the LPC envelope, which is robust for our purposes).
pub fn lpc_formants(signal: &[f64], fs: f64, count: usize) -> Vec<f64> {
    let order = (fs / 1000.0) as usize + 2;
    // Pre-emphasis + Hamming window.
    let n = signal.len();
    let frame: Vec<f64> = (0..n)
        .map(|i| {
            let x = signal[i] - if i > 0 { 0.97 * signal[i - 1] } else { 0.0 };
            x * (0.54 - 0.46 * (2.0 * std::f64::consts::PI * i as f64 / (n - 1) as f64).cos())
        })
        .collect();
    let a = lpc(&frame, order);
    let nfft = 8192;
    let mut re = vec![0.0; nfft];
    let mut im = vec![0.0; nfft];
    re[..a.len()].copy_from_slice(&a);
    fft(&mut re, &mut im);
    let env: Vec<f64> = (0..nfft / 2).map(|k| -10.0 * (re[k] * re[k] + im[k] * im[k] + 1e-30).log10()).collect();
    pick_peaks(&env, fs, nfft, 150.0, fs / 2.0 - 500.0, count)
}

pub fn rms(x: &[f32]) -> f64 {
    if x.is_empty() {
        return 0.0;
    }
    (x.iter().map(|&v| (v as f64).powi(2)).sum::<f64>() / x.len() as f64).sqrt()
}
