//! Linguistic structure handed from the front end (markup + G2P) to prosody,
//! plus speaking style and emotion presets.

use crate::ipa::{Lang, Seg};

/// Speaking style: continuous controls that emotions and markup modulate.
#[derive(Clone, Debug, PartialEq)]
pub struct Style {
    /// Speaking-rate multiplier (2 = twice as fast).
    pub rate: f64,
    /// Pitch shift in semitones.
    pub pitch: f64,
    /// Pitch-range (accent excursion) multiplier.
    pub range: f64,
    /// Loudness → subglottal pressure multiplier.
    pub volume: f64,
    /// Laryngeal tension −1 (lax, breathy) … +1 (pressed).
    pub tension: f64,
    /// Extra breathiness 0..1.
    pub breath: f64,
    /// Creaky voice 0..1.
    pub creak: f64,
    /// Jitter/shimmer multiplier.
    pub perturbation: f64,
    /// Articulatory precision: 1 normal, <1 slurred/reduced, >1 hyperarticulated.
    pub precision: f64,
    /// Lip spreading from smiling 0..1 (raises formants).
    pub smile: f64,
    /// Vocal tremor 0..1 (4–6 Hz F0/amplitude modulation).
    pub tremor: f64,
    pub whisper: bool,
}

impl Default for Style {
    fn default() -> Self {
        Style {
            rate: 1.0,
            pitch: 0.0,
            range: 1.0,
            volume: 1.0,
            tension: 0.0,
            breath: 0.0,
            creak: 0.0,
            perturbation: 1.0,
            precision: 1.0,
            smile: 0.0,
            tremor: 0.0,
            whisper: false,
        }
    }
}

/// Emotion presets as offsets from neutral, from the acoustic-correlates
/// literature (Murray & Arnott 1993; Burkhardt & Sendlmeier 2000; Scherer 2003).
struct EmotionPreset {
    names: &'static [&'static str],
    pitch: f64,
    range: f64,
    rate: f64,
    volume: f64,
    tension: f64,
    breath: f64,
    creak: f64,
    perturbation: f64,
    precision: f64,
    smile: f64,
    tremor: f64,
}

const EMOTIONS: &[EmotionPreset] = &[
    EmotionPreset { names: &["neutral", "normal"], pitch: 0.0, range: 1.0, rate: 1.0, volume: 1.0, tension: 0.0, breath: 0.0, creak: 0.0, perturbation: 1.0, precision: 1.0, smile: 0.0, tremor: 0.0 },
    EmotionPreset { names: &["happy", "joy", "joyful", "cheerful", "glad"], pitch: 3.0, range: 1.45, rate: 1.08, volume: 1.15, tension: 0.15, breath: 0.0, creak: 0.0, perturbation: 1.0, precision: 1.0, smile: 0.8, tremor: 0.0 },
    EmotionPreset { names: &["excited", "enthusiastic", "elated"], pitch: 5.0, range: 1.7, rate: 1.18, volume: 1.35, tension: 0.3, breath: 0.05, creak: 0.0, perturbation: 1.1, precision: 1.05, smile: 0.6, tremor: 0.0 },
    EmotionPreset { names: &["sad", "sadness", "sorrow", "melancholy", "depressed"], pitch: -2.0, range: 0.55, rate: 0.82, volume: 0.72, tension: -0.5, breath: 0.35, creak: 0.12, perturbation: 1.3, precision: 0.8, smile: 0.0, tremor: 0.25 },
    EmotionPreset { names: &["angry", "anger", "furious", "mad"], pitch: 2.0, range: 1.35, rate: 1.08, volume: 1.7, tension: 0.85, breath: 0.0, creak: 0.05, perturbation: 1.4, precision: 1.25, smile: 0.0, tremor: 0.0 },
    EmotionPreset { names: &["fear", "afraid", "scared", "anxious", "nervous"], pitch: 5.0, range: 1.1, rate: 1.2, volume: 0.95, tension: 0.25, breath: 0.25, creak: 0.0, perturbation: 2.2, precision: 0.95, smile: 0.0, tremor: 0.55 },
    EmotionPreset { names: &["surprise", "surprised", "amazed", "astonished"], pitch: 4.5, range: 1.85, rate: 1.0, volume: 1.2, tension: 0.1, breath: 0.1, creak: 0.0, perturbation: 1.0, precision: 1.05, smile: 0.3, tremor: 0.0 },
    EmotionPreset { names: &["calm", "gentle", "tender", "soft", "warm", "kind"], pitch: -1.0, range: 0.8, rate: 0.9, volume: 0.8, tension: -0.3, breath: 0.2, creak: 0.0, perturbation: 0.9, precision: 0.95, smile: 0.35, tremor: 0.0 },
    EmotionPreset { names: &["bored", "tired", "sleepy", "boredom"], pitch: -2.5, range: 0.4, rate: 0.85, volume: 0.8, tension: -0.25, breath: 0.15, creak: 0.35, perturbation: 1.1, precision: 0.75, smile: 0.0, tremor: 0.0 },
    EmotionPreset { names: &["disgust", "disgusted", "contempt"], pitch: -1.5, range: 0.9, rate: 0.86, volume: 1.0, tension: 0.35, breath: 0.05, creak: 0.25, perturbation: 1.2, precision: 1.05, smile: 0.0, tremor: 0.0 },
    EmotionPreset { names: &["serious", "firm", "confident", "assertive"], pitch: -1.0, range: 0.9, rate: 0.97, volume: 1.15, tension: 0.3, breath: 0.0, creak: 0.05, perturbation: 0.9, precision: 1.15, smile: 0.0, tremor: 0.0 },
    EmotionPreset { names: &["playful", "teasing", "cute"], pitch: 3.5, range: 1.6, rate: 1.05, volume: 1.05, tension: 0.0, breath: 0.1, creak: 0.0, perturbation: 1.0, precision: 1.0, smile: 0.9, tremor: 0.0 },
];

pub fn emotion_names() -> Vec<&'static str> {
    EMOTIONS.iter().map(|e| e.names[0]).collect()
}

impl Style {
    /// Blend an emotion into this style by `amount` (0..1, >1 exaggerates).
    /// Returns false if the emotion is unknown.
    pub fn apply_emotion(&mut self, name: &str, amount: f64) -> bool {
        let name = name.trim().to_ascii_lowercase();
        let Some(e) = EMOTIONS.iter().find(|e| e.names.contains(&name.as_str())) else {
            return false;
        };
        let k = amount.max(0.0);
        let mul = |base: f64, f: f64| base * (1.0 + (f - 1.0) * k);
        self.pitch += e.pitch * k;
        self.range = mul(self.range, e.range).max(0.1);
        self.rate = mul(self.rate, e.rate).max(0.2);
        self.volume = mul(self.volume, e.volume).max(0.1);
        self.tension = (self.tension + e.tension * k).clamp(-1.0, 1.0);
        self.breath = (self.breath + e.breath * k).clamp(0.0, 1.0);
        self.creak = (self.creak + e.creak * k).clamp(0.0, 1.0);
        self.perturbation = mul(self.perturbation, e.perturbation).max(0.0);
        self.precision = mul(self.precision, e.precision).max(0.3);
        self.smile = (self.smile + e.smile * k).clamp(0.0, 1.0);
        self.tremor = (self.tremor + e.tremor * k).clamp(0.0, 1.0);
        true
    }
}

#[derive(Clone, Debug)]
pub struct Word {
    pub text: String,
    pub segs: Vec<Seg>,
    /// Function words don't receive pitch accents.
    pub function: bool,
    /// Emphasis 0..1 (contrastive focus).
    pub emphasis: f64,
    /// Style in effect for this word (markup can change it mid-phrase).
    pub style: Style,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Boundary {
    /// No boundary (phrase continues into following markup).
    None,
    Comma,
    Period,
    Question,
    Exclaim,
}

#[derive(Clone, Debug)]
pub struct Phrase {
    pub words: Vec<Word>,
    pub boundary: Boundary,
    pub lang: Lang,
}

/// One sung syllable: its phonemes and the note(s) it is sung on
/// (several pitches = melisma on the vowel).
#[derive(Clone, Debug)]
pub struct Note {
    pub text: String,
    pub segs: Vec<Seg>,
    /// (MIDI note number, duration in beats); MIDI may be fractional.
    pub pitches: Vec<(f64, f64)>,
    /// Slur into the next note (no re-articulation gap, glide pitch).
    pub legato: bool,
}

#[derive(Clone, Debug)]
pub enum SungItem {
    Note(Note),
    Rest(f64),
}

/// Singing technique parameters.
#[derive(Clone, Debug, PartialEq)]
pub struct SingStyle {
    pub tempo: f64,
    pub transpose: f64,
    /// Vibrato extent (± semitones) and rate (Hz).
    pub vibrato_depth: f64,
    pub vibrato_rate: f64,
    /// Delay before vibrato sets in (s).
    pub vibrato_delay: f64,
    /// Portamento time constant (s) for pitch glides between notes.
    pub portamento: f64,
    /// "classical" narrows the epilarynx (singer's formant), "pop" is speech-like.
    pub classical: bool,
}

impl Default for SingStyle {
    fn default() -> Self {
        SingStyle {
            tempo: 100.0,
            transpose: 0.0,
            vibrato_depth: 0.35,
            vibrato_rate: 5.6,
            vibrato_delay: 0.28,
            portamento: 0.035,
            classical: false,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Song {
    pub items: Vec<SungItem>,
    pub sing: SingStyle,
    pub style: Style,
    pub lang: Lang,
}

#[derive(Clone, Debug)]
pub enum Block {
    Speech(Phrase),
    Pause(f64),
    Song(Song),
}
