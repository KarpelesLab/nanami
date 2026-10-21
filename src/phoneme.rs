//! Phoneme inventory (IPA) with phonetic features, intrinsic durations and
//! the articulatory gestures that realize each sound.
//!
//! Intrinsic durations for English follow Klatt (1979, 1987); the gestures
//! are specified per articulator tier and only for the tiers a sound actually
//! controls — unspecified tiers are free to coarticulate.

use crate::approx_table::*;
use crate::articulation::ADJ_N;
use crate::vowel_table::VOWEL_SHAPES;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Manner {
    Vowel,
    Stop,
    Nasal,
    Fricative,
    Affricate,
    Approximant,
    Lateral,
    Tap,
    Trill,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Place {
    None,
    Bilabial,
    Labiodental,
    Dental,
    Alveolar,
    Postalveolar,
    AlveoloPalatal,
    Retroflex,
    Palatal,
    Velar,
    Uvular,
    Pharyngeal,
    Glottal,
    LabialVelar,
}

/// Vocal-tract carrier shape for vowels (see `articulation::Articulators`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VowelShape {
    pub tongue_pos: f64,
    pub tongue_radius: f64,
    pub lip_area: f64,
    pub lip_length: f64,
    pub tongue_length: f64,
    pub pharynx: f64,
    /// Area correction curve (see `Articulators::adj`).
    pub adj: [f64; ADJ_N],
}

impl VowelShape {
    pub const SCHWA: VowelShape = VowelShape { tongue_pos: 0.5, tongue_radius: 1.18, lip_area: 2.8, lip_length: 1.0, tongue_length: 1.0, pharynx: 1.0, adj: [0.0; ADJ_N] };

    pub fn lookup(ipa: &str) -> Option<VowelShape> {
        VOWEL_SHAPES.iter().chain(APPROX_SHAPES).find(|v| v.0 == ipa).map(|(_, p, adj)| VowelShape {
            tongue_pos: p[0],
            tongue_radius: p[1],
            lip_area: p[2],
            lip_length: p[3],
            tongue_length: p[4],
            pharynx: p[5],
            adj: *adj,
        })
    }

    /// Set the carrier articulators (tongue body, pharynx, lips) to this shape.
    pub fn apply(&self, art: &mut crate::articulation::Articulators) {
        art.tongue_pos = self.tongue_pos;
        art.tongue_radius = self.tongue_radius;
        art.tongue_length = self.tongue_length;
        art.pharynx = self.pharynx;
        art.lip_area = self.lip_area;
        art.lip_length = self.lip_length;
        art.adj = self.adj;
    }

    /// Interpolate toward `other` (t = 0 → self, 1 → other).
    pub fn lerp(&self, other: &VowelShape, t: f64) -> VowelShape {
        let l = |a: f64, b: f64| a + (b - a) * t;
        let mut adj = self.adj;
        for (a, b) in adj.iter_mut().zip(&other.adj) {
            *a = l(*a, *b);
        }
        VowelShape {
            tongue_pos: l(self.tongue_pos, other.tongue_pos),
            tongue_radius: l(self.tongue_radius, other.tongue_radius),
            lip_area: l(self.lip_area, other.lip_area),
            lip_length: l(self.lip_length, other.lip_length),
            tongue_length: l(self.tongue_length, other.tongue_length),
            pharynx: l(self.pharynx, other.pharynx),
            adj,
        }
    }

    /// Move toward schwa by `amount` (vowel reduction / hypoarticulation).
    pub fn reduce(&self, amount: f64) -> VowelShape {
        let s = Self::SCHWA;
        let l = |a: f64, b: f64| a + (b - a) * amount;
        VowelShape {
            tongue_pos: l(self.tongue_pos, s.tongue_pos),
            tongue_radius: l(self.tongue_radius, s.tongue_radius),
            lip_area: l(self.lip_area, s.lip_area),
            lip_length: l(self.lip_length, s.lip_length),
            tongue_length: l(self.tongue_length, s.tongue_length),
            pharynx: l(self.pharynx, s.pharynx),
            adj: self.adj.map(|a| a * (1.0 - amount)),
        }
    }
}

/// One constriction gesture on a consonantal tier.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Gesture {
    /// Lip constriction area (cm²).
    Lips(f64),
    /// Tongue tip/blade: (position fraction, area cm²).
    Tip(f64, f64),
    /// Tongue dorsum: (position fraction, area). Position < 0 = "velar,
    /// coarticulated with the neighbouring vowel".
    Dorsum(f64, f64),
    /// Tongue root / pharyngeal constriction area.
    Root(f64),
    /// Lip rounding/protrusion carried by a consonant: (area, length).
    Round(f64, f64),
}

pub const VELAR_AUTO: f64 = -1.0;

#[derive(Clone, Debug)]
pub struct Phone {
    pub ipa: &'static str,
    pub manner: Manner,
    pub place: Place,
    pub voiced: bool,
    pub sibilant: bool,
    /// Intrinsic duration (ms), stressed citation form.
    pub dur: f64,
    /// Minimum duration under compression (ms).
    pub min_dur: f64,
    pub gestures: &'static [Gesture],
    /// Tongue-body/lip carrier shape: vowels, and approximants (which have a
    /// vowel-like posture of their own on top of their constrictions).
    pub vowel: Option<&'static str>,
}

macro_rules! ph {
    ($ipa:expr, $m:ident, $p:ident, $v:expr, $dur:expr, $min:expr, [$($g:expr),*]) => {
        Phone { ipa: $ipa, manner: Manner::$m, place: Place::$p, voiced: $v, sibilant: false, dur: $dur, min_dur: $min, gestures: &[$($g),*], vowel: None }
    };
}
macro_rules! sib {
    ($ipa:expr, $m:ident, $p:ident, $v:expr, $dur:expr, $min:expr, [$($g:expr),*]) => {
        Phone { ipa: $ipa, manner: Manner::$m, place: Place::$p, voiced: $v, sibilant: true, dur: $dur, min_dur: $min, gestures: &[$($g),*], vowel: None }
    };
}
macro_rules! vw {
    ($ipa:expr, $dur:expr, $min:expr) => {
        Phone { ipa: $ipa, manner: Manner::Vowel, place: Place::None, voiced: true, sibilant: false, dur: $dur, min_dur: $min, gestures: &[], vowel: Some($ipa) }
    };
    ($ipa:expr, $carrier:expr, $dur:expr, $min:expr, [$($g:expr),*]) => {
        Phone { ipa: $ipa, manner: Manner::Vowel, place: Place::None, voiced: true, sibilant: false, dur: $dur, min_dur: $min, gestures: &[$($g),*], vowel: Some($carrier) }
    };
    ($ipa:expr, $carrier:expr, $dur:expr, $min:expr, $g:expr) => {
        Phone { ipa: $ipa, manner: Manner::Vowel, place: Place::None, voiced: true, sibilant: false, dur: $dur, min_dur: $min, gestures: $g, vowel: Some($carrier) }
    };
}
/// Approximant with a calibrated carrier shape and gestures (approx_table).
macro_rules! ap {
    ($ipa:expr, $m:ident, $p:ident, $v:expr, $dur:expr, $min:expr, $carrier:expr, $g:expr) => {
        Phone { ipa: $ipa, manner: Manner::$m, place: Place::$p, voiced: $v, sibilant: false, dur: $dur, min_dur: $min, gestures: $g, vowel: Some($carrier) }
    };
}

use Gesture::*;

// Constriction positions (fraction of tract length, glottis = 0, lips = 1).
const DENTAL: f64 = 0.95;
const ALVEOLAR: f64 = 0.89;
const POSTALV: f64 = 0.835;
const ALV_PAL: f64 = 0.83;
const RETRO: f64 = 0.80;
const PALATAL: f64 = 0.76;
const UVULAR: f64 = 0.56;
const CLOSED: f64 = 0.0;

pub static PHONES: &[Phone] = &[
    // --- Vowels (carrier shapes come from the calibrated vowel table) ---
    vw!("i", 160.0, 55.0),
    vw!("ɪ", 135.0, 40.0),
    vw!("e", 160.0, 50.0),
    vw!("ɛ", 150.0, 50.0),
    vw!("æ", 230.0, 80.0),
    vw!("a", 200.0, 70.0),
    vw!("ɑ", 240.0, 80.0),
    vw!("ɒ", 220.0, 80.0),
    vw!("ɔ", 240.0, 80.0),
    vw!("o", 180.0, 60.0),
    vw!("ʊ", 160.0, 50.0),
    vw!("u", 210.0, 60.0),
    vw!("ʌ", 140.0, 50.0),
    vw!("ə", 110.0, 40.0),
    vw!("ɜ", 180.0, 60.0),
    vw!("ɐ", 140.0, 50.0),
    vw!("ɯ", 150.0, 50.0),
    vw!("ɤ", 150.0, 50.0),
    vw!("ɨ", 140.0, 45.0),
    vw!("ʉ", 150.0, 50.0),
    vw!("y", 160.0, 55.0),
    vw!("ø", 160.0, 55.0),
    vw!("œ", 170.0, 60.0),
    // Rhotic vowels: bunched-r constrictions near the F3 velocity maxima
    // (palate, pharynx, lips) over their own calibrated carrier.
    vw!("ɚ", "ɚ", 150.0, 60.0, G_SCHWAR),
    vw!("ɝ", "ɝ", 180.0, 70.0, G_ER),
    // --- Stops ---
    ph!("p", Stop, Bilabial, false, 90.0, 50.0, [Lips(CLOSED)]),
    ph!("b", Stop, Bilabial, true, 85.0, 50.0, [Lips(CLOSED)]),
    ph!("t", Stop, Alveolar, false, 75.0, 40.0, [Tip(ALVEOLAR, CLOSED)]),
    ph!("d", Stop, Alveolar, true, 75.0, 40.0, [Tip(ALVEOLAR, CLOSED)]),
    ph!("ʈ", Stop, Retroflex, false, 75.0, 40.0, [Tip(RETRO, CLOSED)]),
    ph!("ɖ", Stop, Retroflex, true, 75.0, 40.0, [Tip(RETRO, CLOSED)]),
    ph!("c", Stop, Palatal, false, 80.0, 45.0, [Dorsum(PALATAL, CLOSED)]),
    ph!("ɟ", Stop, Palatal, true, 80.0, 45.0, [Dorsum(PALATAL, CLOSED)]),
    ph!("k", Stop, Velar, false, 80.0, 50.0, [Dorsum(VELAR_AUTO, CLOSED)]),
    ph!("g", Stop, Velar, true, 80.0, 50.0, [Dorsum(VELAR_AUTO, CLOSED)]),
    ph!("q", Stop, Uvular, false, 85.0, 50.0, [Dorsum(UVULAR, CLOSED)]),
    ph!("ʔ", Stop, Glottal, false, 60.0, 30.0, []),
    // --- Nasals ---
    ph!("m", Nasal, Bilabial, true, 70.0, 50.0, [Lips(CLOSED)]),
    ph!("n", Nasal, Alveolar, true, 60.0, 35.0, [Tip(ALVEOLAR, CLOSED)]),
    ph!("ɳ", Nasal, Retroflex, true, 60.0, 35.0, [Tip(RETRO, CLOSED)]),
    ph!("ɲ", Nasal, Palatal, true, 70.0, 40.0, [Dorsum(PALATAL, CLOSED)]),
    ph!("ŋ", Nasal, Velar, true, 95.0, 60.0, [Dorsum(VELAR_AUTO, CLOSED)]),
    ph!("ɴ", Nasal, Uvular, true, 90.0, 60.0, [Dorsum(UVULAR, 0.05)]),
    // --- Fricatives ---
    ph!("ɸ", Fricative, Bilabial, false, 90.0, 50.0, [Lips(0.12)]),
    ph!("β", Fricative, Bilabial, true, 60.0, 35.0, [Lips(0.18)]),
    ph!("f", Fricative, Labiodental, false, 100.0, 80.0, [Lips(0.09)]),
    ph!("v", Fricative, Labiodental, true, 60.0, 40.0, [Lips(0.12)]),
    ph!("θ", Fricative, Dental, false, 90.0, 60.0, [Tip(DENTAL, 0.10)]),
    ph!("ð", Fricative, Dental, true, 50.0, 30.0, [Tip(DENTAL, 0.14)]),
    sib!("s", Fricative, Alveolar, false, 105.0, 60.0, [Tip(ALVEOLAR, 0.08)]),
    sib!("z", Fricative, Alveolar, true, 75.0, 40.0, [Tip(ALVEOLAR, 0.1)]),
    sib!("ʃ", Fricative, Postalveolar, false, 105.0, 80.0, [Tip(POSTALV, 0.11), Round(1.2, 1.4)]),
    sib!("ʒ", Fricative, Postalveolar, true, 70.0, 40.0, [Tip(POSTALV, 0.13), Round(1.2, 1.4)]),
    sib!("ɕ", Fricative, AlveoloPalatal, false, 105.0, 70.0, [Tip(ALV_PAL, 0.10)]),
    sib!("ʑ", Fricative, AlveoloPalatal, true, 70.0, 40.0, [Tip(ALV_PAL, 0.12)]),
    sib!("ʂ", Fricative, Retroflex, false, 105.0, 70.0, [Tip(RETRO, 0.10)]),
    ph!("ç", Fricative, Palatal, false, 100.0, 60.0, [Dorsum(PALATAL, 0.10)]),
    ph!("x", Fricative, Velar, false, 100.0, 60.0, [Dorsum(VELAR_AUTO, 0.10)]),
    ph!("ɣ", Fricative, Velar, true, 70.0, 40.0, [Dorsum(VELAR_AUTO, 0.16)]),
    ph!("χ", Fricative, Uvular, false, 100.0, 60.0, [Dorsum(UVULAR, 0.12)]),
    ph!("ʁ", Fricative, Uvular, true, 70.0, 40.0, [Dorsum(UVULAR, 0.2)]),
    ph!("h", Fricative, Glottal, false, 80.0, 40.0, []),
    ph!("ɦ", Fricative, Glottal, true, 70.0, 40.0, []),
    // --- Affricates ---
    sib!("ts", Affricate, Alveolar, false, 110.0, 60.0, [Tip(ALVEOLAR, CLOSED)]),
    sib!("dz", Affricate, Alveolar, true, 90.0, 50.0, [Tip(ALVEOLAR, CLOSED)]),
    sib!("tʃ", Affricate, Postalveolar, false, 115.0, 60.0, [Tip(POSTALV, CLOSED), Round(1.2, 1.4)]),
    sib!("dʒ", Affricate, Postalveolar, true, 95.0, 50.0, [Tip(POSTALV, CLOSED), Round(1.2, 1.4)]),
    sib!("tɕ", Affricate, AlveoloPalatal, false, 110.0, 60.0, [Tip(ALV_PAL, CLOSED)]),
    sib!("dʑ", Affricate, AlveoloPalatal, true, 90.0, 50.0, [Tip(ALV_PAL, CLOSED)]),
    // --- Approximants, laterals, rhotics ---
    ap!("j", Approximant, Palatal, true, 80.0, 40.0, "j", G_J),
    ap!("w", Approximant, LabialVelar, true, 80.0, 40.0, "w", G_W),
    ap!("ʍ", Approximant, LabialVelar, false, 90.0, 50.0, "w", G_W),
    ph!("ɰ", Approximant, Velar, true, 70.0, 40.0, [Dorsum(VELAR_AUTO, 0.5)]),
    ph!("ɥ", Approximant, Palatal, true, 80.0, 40.0, [Dorsum(PALATAL, 0.32), Lips(0.3)]),
    ap!("ɹ", Approximant, Postalveolar, true, 80.0, 40.0, "ɹ", G_R),
    ph!("ɻ", Approximant, Retroflex, true, 80.0, 40.0, [Tip(RETRO, 0.3)]),
    ap!("l", Lateral, Alveolar, true, 80.0, 40.0, "l", G_L),
    ap!("ɫ", Lateral, Alveolar, true, 90.0, 50.0, "ɫ", G_DARK_L),
    ph!("ʎ", Lateral, Palatal, true, 80.0, 40.0, [Dorsum(PALATAL, 0.2)]),
    ph!("ɾ", Tap, Alveolar, true, 30.0, 18.0, [Tip(ALVEOLAR, CLOSED)]),
    ph!("r", Trill, Alveolar, true, 85.0, 60.0, [Tip(ALVEOLAR, CLOSED)]),
];

/// Look up a phone by its IPA symbol.
pub fn find(ipa: &str) -> Option<&'static Phone> {
    PHONES.iter().find(|p| p.ipa == ipa)
}

/// Longest IPA symbol in the inventory (in chars), for greedy parsing.
pub fn max_symbol_chars() -> usize {
    PHONES.iter().map(|p| p.ipa.chars().count()).max().unwrap_or(1)
}

impl Phone {
    pub fn is_vowel(&self) -> bool {
        self.manner == Manner::Vowel
    }
    pub fn is_sonorant(&self) -> bool {
        matches!(self.manner, Manner::Vowel | Manner::Nasal | Manner::Approximant | Manner::Lateral | Manner::Tap | Manner::Trill)
    }
    pub fn is_obstruent(&self) -> bool {
        !self.is_sonorant()
    }
    /// Tongue-body posture of a lingual obstruent/nasal, as (locus shape,
    /// strength). The body is pulled from the neighbouring vowel toward the
    /// locus by `strength` — the degree of articulatory constraint (DAC) of
    /// the consonant (Recasens 1999: high for alveolars and (post)palatals,
    /// low for dentals and velars, zero for labials and glottals). This yields
    /// the formant transitions that cue place: alveolar F2 locus ≈ 1800 Hz,
    /// velar F2–F3 convergence, labial F2 falling (Delattre et al. 1955).
    pub fn body_locus(&self) -> Option<(VowelShape, f64)> {
        if self.vowel.is_some() || matches!(self.manner, Manner::Approximant | Manner::Lateral | Manner::Vowel) {
            return None;
        }
        let (v, w) = match self.place {
            Place::Dental => ("ɪ", 0.5),
            Place::Alveolar => ("ɪ", 0.75),
            Place::Postalveolar | Place::AlveoloPalatal => ("i", 0.7),
            Place::Palatal => ("i", 0.8),
            Place::Retroflex => ("ɜ", 0.4),
            Place::Velar => ("ɯ", 0.45),
            Place::Uvular => ("ɔ", 0.4),
            _ => return None,
        };
        VowelShape::lookup(v).map(|s| (s, w))
    }

    pub fn shape(&self) -> Option<VowelShape> {
        self.vowel.and_then(VowelShape::lookup)
    }
}
