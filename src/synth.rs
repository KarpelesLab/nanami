//! The synthesis engine: lungs → glottis → vocal tract → radiation.
//!
//! Aerodynamics follow the quasi-steady orifice model used in articulatory
//! synthesizers (Stevens 1971, 1998; Maeda 1982; Birkholz 2007): flow through
//! the glottis and through the narrowest supraglottal constriction obeys
//! Bernoulli's law, and the oral pressure behind the constriction integrates
//! their difference through the (wall-dominated) cavity compliance. Turbulence
//! noise is generated where the Reynolds number exceeds a critical value, with
//! source strength ∝ Re² − Re_c² (Flanagan & Cherry 1969; Sondhi & Schroeter 1987).
//! This makes stop bursts, devoicing during closures, aspiration and frication
//! emerge from the articulation instead of being scripted.

use crate::articulation::{Articulators, area_function, min_constriction};
use crate::dsp::{Biquad, OnePole, Rng, smoothstep};
use crate::glottis::{Glottis, SourceParams};
use crate::tract::Tract;
use crate::voice::Voice;

pub const INTERNAL_RATE: f64 = 96_000.0;
pub const OUTPUT_RATE: u32 = 48_000;
/// Control (articulatory update) rate.
pub const CONTROL_RATE: f64 = 2_000.0;
pub const BLOCK: usize = (INTERNAL_RATE / CONTROL_RATE) as usize;

const RHO: f64 = 1.14; // kg/m³
const NU: f64 = 1.6e-5; // kinematic viscosity of air, m²/s
const RE_CRIT: f64 = 1800.0;
/// Effective oral cavity compliance incl. yielding walls (m³/Pa).
const COMPLIANCE: f64 = 6e-9;
/// Turbulence source gains (calibrated against fricative-to-vowel level norms).
const FRIC_GAIN: f64 = 0.6;
const OBSTACLE_GAIN: f64 = 4.4;
const ASP_GAIN: f64 = 0.6;
const BURST_GAIN: f64 = 6.0;
/// Peak glottal opening area during modal vibration (cm²).
const GLOTTAL_PEAK_AREA: f64 = 0.15;

/// One control frame (every 1/CONTROL_RATE seconds).
#[derive(Clone, Debug)]
pub struct ControlFrame {
    pub art: Articulators,
    pub f0: f64,
    pub rd: f64,
    /// Extra breathiness 0..1 (whisper-ish leak on top of the voice's own).
    pub breath: f64,
    pub creak: f64,
    /// Multiplier on the voice's jitter/shimmer (e.g. fear/agitation).
    pub perturbation: f64,
}

/// Diagnostic per-frame output of the aerodynamic model.
#[derive(Clone, Debug, Default)]
pub struct AeroTrace {
    pub oral_pressure: f64,
    pub glottal_flow: f64,
    pub voicing: f64,
    pub frication: f64,
}

pub struct Engine {
    pub voice: Voice,
    pub tract: Tract,
    glottis: Glottis,
    rng: Rng,
    areas: Vec<f64>,
    po: f64,
    vib: f64,
    uc_prev: f64,
    /// Recency of a complete closure (1 = closed now, decays after release).
    closure: f64,
    closure_decay: f64,
    asp_hp: Biquad,
    fric_hp: Biquad,
    fric_lp: OnePole,
    pub trace: Vec<AeroTrace>,
    pub keep_trace: bool,
}

impl Engine {
    pub fn new(voice: Voice, seed: u64) -> Engine {
        let tract = Tract::new(INTERNAL_RATE, voice.anatomy.length_cm, voice.nose_cm);
        Engine {
            tract,
            glottis: Glottis::new(INTERNAL_RATE, seed ^ 0xA5A5),
            rng: Rng::new(seed),
            areas: Vec::new(),
            po: 0.0,
            vib: 0.0,
            uc_prev: 0.0,
            closure: 0.0,
            closure_decay: (-1.0 / (0.008 * INTERNAL_RATE)).exp(),
            asp_hp: Biquad::highpass(400.0, 0.6, INTERNAL_RATE),
            fric_hp: Biquad::highpass(300.0, 0.6, INTERNAL_RATE),
            fric_lp: OnePole::new(12_000.0, INTERNAL_RATE),
            trace: Vec::new(),
            keep_trace: false,
            voice,
        }
    }

    /// Render control frames to audio at INTERNAL_RATE.
    pub fn render_internal(&mut self, frames: &[ControlFrame]) -> Vec<f64> {
        let mut out = Vec::with_capacity(frames.len() * BLOCK);
        let n = self.tract.n;
        let mut first = true;
        for frame in frames {
            area_function(&frame.art, &self.voice.anatomy, n, &mut self.areas);
            if first {
                self.tract.snap(&self.areas, frame.art.velum);
                first = false;
            } else {
                self.tract.set_target(&self.areas, frame.art.velum, BLOCK);
            }
            self.render_block(frame, &mut out);
        }
        out
    }

    fn render_block(&mut self, frame: &ControlFrame, out: &mut Vec<f64>) {
        let v = &self.voice;
        let art = &frame.art;
        let dt = 1.0 / INTERNAL_RATE;
        let (imin, amin) = min_constriction(&self.areas, &v.anatomy);
        let n = self.tract.n;
        let teeth = n as f64 - 1.0 - 0.9 / self.tract.section_cm();
        // The jet (and its turbulence) forms where the flow leaves the
        // constriction, i.e. at the downstream end of the narrow channel.
        let mut iexit = imin;
        while iexit + 1 < n && self.areas[iexit + 1] < 1.5 * amin.max(0.02) {
            iexit += 1;
        }
        // The jet's pressure fluctuations radiate where it has expanded into
        // the wider cavity in front (Stevens 1998, ch. 3: ~1 cm downstream),
        // so its coupling to the front cavity doesn't swing with small
        // changes of the channel itself.
        let mut ijet = iexit + 1;
        while ijet + 1 < n && self.areas[ijet] < (4.0 * amin).max(0.6) {
            ijet += 1;
        }
        let abd = art.glottis.max(0.0);
        let leak = 0.02 * v.breathiness + 0.06 * frame.breath;
        // Glottal termination: the glottis is a Bernoulli resistance
        // R_g = √(2ρΔp)/A_g against the tract's ρc/A₀, so the reflection is
        // ≈ +0.95 with the folds closed and drops toward 0 at peak opening,
        // damping the formants once per cycle as in real voices.
        let a0 = self.areas.first().copied().unwrap_or(1.5).max(0.1);
        let static_ag = abd + leak + 0.06 * frame.breath;
        // Adduction: vibration needs the folds approximated (or not pressed shut).
        let adduction = if art.glottis >= 0.0 {
            1.0 - smoothstep(0.03, 0.16, art.glottis)
        } else {
            (1.0 + art.glottis).max(0.0)
        };
        let ps = art.lung.max(0.0);
        let escape = amin + if self.tract.velum_index < imin { art.velum } else { 0.0 };
        let vib_a = 1.0 - (-dt / 0.004f64).exp();
        let perturb = frame.perturbation.max(0.0);
        let src = SourceParams {
            f0: frame.f0,
            rd: frame.rd + 1.2 * frame.breath + 3.0 * abd.min(0.2),
            amplitude: 0.0,
            jitter: v.jitter * perturb,
            shimmer: v.shimmer * perturb,
            flutter: v.flutter,
            creak: frame.creak,
        };
        let mut src = src;
        let mut fric_sum = 0.0;
        let mut trace_ug = 0.0;
        for _ in 0..BLOCK {
            // --- Aerodynamics (cm² → m²) ---
            let mut ag = abd + leak + 0.035 * self.vib;
            if art.glottis < 0.0 {
                ag *= (1.0 + art.glottis).max(0.0);
            }
            let agm = ag * 1e-4;
            let aem = escape * 1e-4;
            let denom = agm * agm + aem * aem;
            if denom > 0.0 {
                let po_eq = ps * agm * agm / denom;
                let g = agm / (2.0 * RHO * (ps - self.po).max(1.0)).sqrt() + aem / (2.0 * RHO * self.po.max(1.0)).sqrt();
                // Active expansion during voiced obstruents (Westbury 1983)
                // makes room for the glottal flow so voicing can continue.
                let tau = COMPLIANCE * (1.0 + 4.0 * art.expansion) / g.max(1e-12);
                self.po = po_eq + (self.po - po_eq) * (-dt / tau).exp();
            }
            let ptrans = (ps - self.po).max(0.0);
            let ug = agm * (2.0 * ptrans / RHO).sqrt();
            let uc = amin * 1e-4 * (2.0 * self.po.max(0.0) / RHO).sqrt();

            // --- Phonation ---
            let drive = smoothstep(120.0, 420.0, ptrans) * (ptrans / 800.0).powf(0.6);
            self.vib += vib_a * (adduction * drive - self.vib);
            src.amplitude = self.vib;
            let g = self.glottis.step(&src);
            let ag_t = static_ag + GLOTTAL_PEAK_AREA * self.vib.min(1.0) * g.flow.clamp(0.0, 1.0);
            let rg = (2.0 * RHO * (ps - self.po).max(50.0)).sqrt() * a0;
            let z0 = RHO * crate::tract::SPEED_OF_SOUND * ag_t.max(1e-4);
            self.tract.glottal_reflection = ((rg - z0) / (rg + z0)).clamp(-0.3, 0.96);

            // --- Aspiration (turbulence just above the glottis) ---
            let re_g = if agm > 0.0 { ug / (NU * agm.sqrt()) } else { 0.0 };
            let turb_g = ((re_g / RE_CRIT).powi(2) - 1.0).max(0.0);
            // Pulsatile modulation while vibrating (noise bursts at peak flow).
            let pulse = 1.0 - self.vib.min(1.0) * 0.95 * (1.0 - g.flow.min(1.0));
            let asp = self.asp_hp.process(self.rng.gauss()) * turb_g * pulse * ASP_GAIN * v.aspiration_gain;

            // --- Frication at the narrowest supraglottal constriction ---
            if amin < 1.2 && amin > 1e-4 {
                // Source strength after Stevens (1971, 1998): p_noise ∝ Δp^1.5 √A,
                // gated on by the Reynolds number. (A pure Re² law overshoots
                // badly while a constriction opens, producing a click at the
                // end of every fricative.)
                let re_c = uc / (NU * (amin * 1e-4).sqrt());
                let onset = smoothstep(0.8, 1.6, re_c / RE_CRIT);
                let turb = 12.4 * (self.po.max(0.0) / 700.0).powf(1.5) * (amin / 0.1).sqrt() * onset * smoothstep(1.2, 0.4, amin);
                if turb > 0.0 {
                    let noise = self.fric_lp.process(self.fric_hp.process(self.rng.gauss()));
                    let s = noise * turb * FRIC_GAIN * v.frication_gain;
                    let obst = art.obstacle.clamp(0.0, 1.0);
                    self.tract.add_noise(ijet as f64, s * (1.0 - obst));
                    if obst > 0.0 {
                        // Sibilants: the jet strikes the incisors.
                        let pos = teeth.max(ijet as f64);
                        self.tract.add_noise(pos, s * obst * OBSTACLE_GAIN);
                    }
                    fric_sum += turb;
                }
            }
            // Release transient: the abrupt onset of flow through an opening
            // constriction acts as an acoustic volume-velocity source.
            let uc_cc = uc * 1e6;
            let duc = uc_cc - self.uc_prev;
            self.uc_prev = uc_cc;
            // Only a genuine release from closure produces a burst; the
            // gradual widening at the end of a fricative must not click.
            self.closure = if amin < 0.02 { 1.0 } else { self.closure * self.closure_decay };
            if duc > 0.0 && amin < 0.5 && self.closure > 0.01 {
                self.tract.add_noise(imin as f64 + 0.5, duc * BURST_GAIN * self.closure);
            }

            let y = self.tract.step(g.dflow + asp);
            out.push(y);
            trace_ug += ug;
        }
        if self.keep_trace {
            self.trace.push(AeroTrace {
                oral_pressure: self.po,
                glottal_flow: trace_ug / BLOCK as f64,
                voicing: self.vib,
                frication: fric_sum / BLOCK as f64,
            });
        }
    }
}

/// Render frames to OUTPUT_RATE audio, DC-blocked, peak-normalized to `peak`.
pub fn render(voice: &Voice, frames: &[ControlFrame], seed: u64, peak: f64) -> Vec<f32> {
    let mut engine = Engine::new(voice.clone(), seed);
    let raw = engine.render_internal(frames);
    finish(&raw, peak)
}

pub fn finish(raw: &[f64], peak: f64) -> Vec<f32> {
    let factor = (INTERNAL_RATE / OUTPUT_RATE as f64).round() as usize;
    let mut y = crate::dsp::decimate(raw, factor);
    // DC / subsonic removal.
    let mut hp = Biquad::highpass(40.0, 0.707, OUTPUT_RATE as f64);
    y.iter_mut().for_each(|s| *s = hp.process(*s));
    let max = y.iter().fold(0.0f64, |m, s| m.max(s.abs()));
    let g = if max > 0.0 { peak / max } else { 1.0 };
    y.iter().map(|s| (s * g) as f32).collect()
}
