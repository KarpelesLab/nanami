//! Speaker definitions: anatomy plus habitual source settings.

use crate::articulation::Anatomy;

#[derive(Clone, Debug)]
pub struct Voice {
    pub name: String,
    pub anatomy: Anatomy,
    /// Nasal cavity length, velum → nostrils (cm).
    pub nose_cm: f64,
    /// Habitual speaking F0 (Hz) and pitch range for accents (semitones).
    pub f0: f64,
    pub range_st: f64,
    /// Habitual voice quality (LF Rd): ~0.8 tense, 1.0 modal, 1.5+ breathy.
    pub rd: f64,
    /// Constant posterior-glottal leak → breathiness, 0..1.
    pub breathiness: f64,
    pub jitter: f64,
    pub shimmer: f64,
    pub flutter: f64,
    /// Typical subglottal pressure for comfortable speech (Pa).
    pub lung: f64,
    /// Speaking-rate multiplier (1 = normal).
    pub rate: f64,
    /// Turbulence noise gains.
    pub aspiration_gain: f64,
    pub frication_gain: f64,
}

impl Voice {
    /// NANAMI's own voice: a young, light, slightly breathy female voice.
    pub fn nanami() -> Voice {
        Voice {
            name: "nanami".into(),
            anatomy: Anatomy { length_cm: 14.2, larynx_area: 1.0, larynx_cm: 1.8, max_area: 9.0 },
            nose_cm: 10.0,
            f0: 215.0,
            range_st: 5.0,
            rd: 1.15,
            breathiness: 0.25,
            jitter: 0.004,
            shimmer: 0.03,
            flutter: 0.4,
            lung: 750.0,
            rate: 1.0,
            aspiration_gain: 1.0,
            frication_gain: 1.0,
        }
    }

    pub fn male() -> Voice {
        Voice {
            name: "male".into(),
            anatomy: Anatomy::adult_male(),
            nose_cm: 11.5,
            f0: 112.0,
            range_st: 4.5,
            rd: 1.0,
            breathiness: 0.08,
            jitter: 0.005,
            shimmer: 0.035,
            flutter: 0.4,
            lung: 800.0,
            rate: 1.0,
            aspiration_gain: 1.0,
            frication_gain: 1.0,
        }
    }

    pub fn female() -> Voice {
        Voice {
            name: "female".into(),
            anatomy: Anatomy { length_cm: 14.6, larynx_area: 1.1, larynx_cm: 1.9, max_area: 9.5 },
            nose_cm: 10.3,
            f0: 200.0,
            range_st: 5.0,
            rd: 1.1,
            breathiness: 0.18,
            ..Voice::male()
        }
    }

    pub fn child() -> Voice {
        Voice {
            name: "child".into(),
            anatomy: Anatomy { length_cm: 12.0, larynx_area: 0.8, larynx_cm: 1.4, max_area: 7.0 },
            nose_cm: 8.2,
            f0: 290.0,
            range_st: 6.0,
            rd: 1.2,
            breathiness: 0.2,
            ..Voice::male()
        }
    }

    pub fn by_name(name: &str) -> Option<Voice> {
        match name.to_ascii_lowercase().as_str() {
            "nanami" | "default" => Some(Self::nanami()),
            "male" | "man" | "bass" | "baritone" | "tenor" => Some(Self::male()),
            "female" | "woman" | "alto" | "soprano" => Some(Self::female()),
            "child" | "kid" => Some(Self::child()),
            _ => None,
        }
    }

    pub fn names() -> &'static [&'static str] {
        &["nanami", "female", "male", "child"]
    }
}
