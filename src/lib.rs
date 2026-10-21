//! NANAMI — a physically based articulatory speech and singing synthesizer.
//!
//! Pipeline: NML markup → G2P → prosody (timing, intonation, singing) →
//! gestural score → articulatory model → aerodynamics + LF glottal source →
//! waveguide vocal tract → audio.

pub mod analysis;
mod approx_table;
pub mod articulation;
pub mod audio_io;
pub mod dsp;
pub mod g2p;
pub mod glottis;
pub mod ipa;
pub mod markup;
pub mod midi;
pub mod phoneme;
pub mod prosody;
pub mod score;
pub mod synth;
pub mod tract;
pub mod utterance;
pub mod voice;
mod vowel_table;

use ipa::Lang;
use voice::Voice;

/// Result of rendering a document.
pub struct Rendered {
    pub audio: Vec<f32>,
    pub sample_rate: u32,
    pub warnings: Vec<String>,
}

/// Render NML markup (plain text works too) to audio.
pub fn render_markup(src: &str, voice: &Voice, lang: Lang) -> Rendered {
    let doc = markup::parse(src, voice, lang);
    let mut warnings = doc.warnings;
    let mut audio: Vec<f32> = Vec::new();
    for (run_voice, blocks) in &doc.runs {
        let tl = prosody::Planner::new(run_voice).plan(blocks);
        warnings.extend(tl.warnings.iter().cloned());
        let frames = score::render_frames(&tl, run_voice);
        let mut engine = synth::Engine::new(run_voice.clone(), 0x5EED);
        let raw = engine.render_internal(&frames);
        audio.extend(synth::finish(&raw, 1.0));
    }
    normalize_loudness(&mut audio);
    Rendered { audio, sample_rate: synth::OUTPUT_RATE, warnings }
}

/// Scale to a consistent loudness with a peak ceiling.
fn normalize_loudness(audio: &mut [f32]) {
    let peak = audio.iter().fold(0.0f32, |m, s| m.max(s.abs()));
    if peak > 0.0 {
        let g = 0.89 / peak;
        audio.iter_mut().for_each(|s| *s *= g);
    }
}
