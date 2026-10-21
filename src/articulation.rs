//! Articulatory state → vocal tract area function.
//!
//! The vowel "carrier" shape follows the Stevens & House (1955) three-parameter
//! model: a parabolic tongue constriction of radius r0 at position d, plus a lip
//! tube of given area and length. Consonantal gestures are overlaid as separate
//! constriction tiers (tongue tip/blade, tongue dorsum, tongue root/pharynx),
//! in the spirit of Articulatory Phonology (Browman & Goldstein 1992): the vowel
//! tier keeps moving underneath consonants, which produces coarticulation.
//!
//! All positions are fractions of the tract length (0 = glottis, 1 = lips) so
//! the same gestures work for any vocal tract length.

use std::f64::consts::PI;

/// Area value meaning "constriction tier inactive".
pub const OPEN: f64 = 16.0;

#[derive(Clone, Debug, PartialEq)]
pub struct Articulators {
    /// Tongue body constriction location (fraction of tract length).
    pub tongue_pos: f64,
    /// Tongue body constriction radius r0 (cm), Stevens & House.
    pub tongue_radius: f64,
    /// Tongue constriction length factor toward the lips (1 = Stevens & House
    /// curvature; >1 = longer, flatter front channel as in /i/).
    pub tongue_length: f64,
    /// Pharynx cross-section scale from tongue-root advancement (1 = neutral;
    /// >1 wide pharynx as in front vowels, <1 retracted root).
    pub pharynx: f64,
    /// Lip opening area (cm²); 0 = closed.
    pub lip_area: f64,
    /// Lip tube length (cm) — protrusion for rounding.
    pub lip_length: f64,
    /// Tongue tip/blade constriction: location and minimum area (cm²).
    pub tip_pos: f64,
    pub tip_area: f64,
    /// Tongue dorsum (palatal/velar/uvular) constriction.
    pub dorsum_pos: f64,
    pub dorsum_area: f64,
    /// Tongue root / pharyngeal constriction (fixed low position).
    pub root_area: f64,
    /// Velopharyngeal port area (cm²); 0 = velum raised (oral).
    pub velum: f64,
    /// Glottal abduction (cm²). 0 = adducted for voicing; >0 open
    /// (voiceless/breath); <0 pressed shut (glottal stop), -1 = full closure.
    pub glottis: f64,
    /// Subglottal (lung) pressure in Pa.
    pub lung: f64,
    /// Sibilance: fraction of frication noise generated at the teeth obstacle.
    pub obstacle: f64,
    /// Epilarynx area override (cm²); 0 = use the anatomy's default.
    pub larynx: f64,
}

impl Default for Articulators {
    fn default() -> Self {
        Articulators {
            tongue_pos: 0.55,
            tongue_radius: 1.0,
            tongue_length: 1.0,
            pharynx: 1.0,
            lip_area: 3.0,
            lip_length: 1.0,
            tip_pos: 0.88,
            tip_area: OPEN,
            dorsum_pos: 0.68,
            dorsum_area: OPEN,
            root_area: OPEN,
            velum: 0.0,
            glottis: 0.4,
            lung: 0.0,
            obstacle: 0.0,
            larynx: 0.0,
        }
    }
}

/// Fixed anatomical parameters of a vocal tract.
#[derive(Clone, Debug)]
pub struct Anatomy {
    /// Total length glottis→lips (cm).
    pub length_cm: f64,
    /// Epilarynx tube area (cm²); narrow epilarynx boosts the "singer's formant".
    pub larynx_area: f64,
    /// Length of the larynx tube (cm).
    pub larynx_cm: f64,
    /// Maximum cross-section area (cm²).
    pub max_area: f64,
}

impl Anatomy {
    pub fn adult_male() -> Self {
        Anatomy { length_cm: 17.5, larynx_area: 1.4, larynx_cm: 2.2, max_area: 11.0 }
    }
}

/// Compute the oral tract area function for `n` sections.
pub fn area_function(art: &Articulators, anat: &Anatomy, n: usize, out: &mut Vec<f64>) {
    out.clear();
    let len = anat.length_cm;
    let seg = len / n as f64;
    // Stevens & House curvature, defined for a ~17.5 cm tract; scale with length.
    let k_back = 0.025 * (17.5 / len).powi(2);
    // Asymmetric tongue: the length factor stretches the constriction toward
    // the lips (palatal channel) while the pharyngeal side keeps S&H curvature.
    let k_front = k_back / art.tongue_length.clamp(0.3, 4.0).powi(2);
    let r0 = art.tongue_radius.max(0.0);
    let d = art.tongue_pos * len;
    let max_r = (anat.max_area / PI).sqrt();
    let lip_len = art.lip_length.clamp(0.3, 3.0);
    for i in 0..n {
        let x = (i as f64 + 0.5) * seg;
        let k = if x > d { k_front } else { k_back };
        let r = (r0 + k * (1.2 - r0).max(0.0) * (x - d).powi(2)).min(max_r);
        let mut a = PI * r * r;
        // Tongue-root advancement widens the pharynx (behind the constriction).
        if x < d {
            let ph = smooth_window(x, anat.larynx_cm, 0.5 * len, 1.5);
            a = (a * (1.0 + (art.pharynx.clamp(0.3, 3.0) - 1.0) * ph)).min(anat.max_area);
        }
        // Larynx tube with a short flare into the pharynx.
        let lx = anat.larynx_cm;
        let lar = if art.larynx > 0.0 { art.larynx } else { anat.larynx_area };
        if x < lx {
            a = lar;
        } else if x < lx + 1.0 {
            a = crate::dsp::lerp(lar, a, x - lx);
        }
        // Lip tube.
        let from_lips = len - x;
        if from_lips < lip_len {
            a = art.lip_area;
        } else if from_lips < lip_len + 0.6 {
            let t = (from_lips - lip_len) / 0.6;
            a = crate::dsp::lerp(art.lip_area, a, t);
        }
        // Consonantal constriction overlays: parabolic widening around the
        // constriction, taking the minimum with the carrier shape.
        a = a.min(constriction(x, art.tip_pos * len, art.tip_area, 0.9 * (17.5 / len).powi(2)));
        a = a.min(constriction(x, art.dorsum_pos * len, art.dorsum_area, 0.10 * (17.5 / len).powi(2)));
        a = a.min(constriction(x, 0.22 * len, art.root_area, 0.12 * (17.5 / len).powi(2)));
        out.push(a.max(0.0));
    }
}

#[inline]
fn constriction(x: f64, xc: f64, area: f64, k: f64) -> f64 {
    if area >= OPEN {
        return f64::INFINITY;
    }
    let rc = (area.max(0.0) / PI).sqrt();
    let r = rc + k * (x - xc).powi(2);
    PI * r * r
}

/// Location (section index) and value of the narrowest supraglottal
/// constriction, excluding the larynx tube.
pub fn min_constriction(areas: &[f64], anat: &Anatomy) -> (usize, f64) {
    let start = ((anat.larynx_cm + 1.0) / anat.length_cm * areas.len() as f64) as usize;
    let mut best = (areas.len() - 1, f64::INFINITY);
    for (i, &a) in areas.iter().enumerate().skip(start) {
        if a < best.1 {
            best = (i, a);
        }
    }
    best
}

/// Smooth plateau of 1 over [a, b] with cosine ramps of width `ramp` (cm).
fn smooth_window(x: f64, a: f64, b: f64, ramp: f64) -> f64 {
    let rise = crate::dsp::smoothstep(a, a + ramp, x);
    let fall = 1.0 - crate::dsp::smoothstep(b - ramp, b, x);
    rise * fall
}
