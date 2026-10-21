//! Frame-level trace (10 ms): current segment, level, voicing, oral pressure,
//! glottal opening, minimum constriction. For spotting gaps/dips between segments.
use nanami::{articulation, ipa::Lang, markup, prosody, score, synth, voice::Voice};

fn main() {
    let text = std::env::args().skip(1).collect::<Vec<_>>().join(" ");
    let text = if text.is_empty() { "Hello, how are you today?".to_string() } else { text };
    let voice = Voice::by_name(&std::env::var("VOICE").unwrap_or("nanami".into())).unwrap();
    let doc = markup::parse(&text, &voice, Lang::English);
    let (v, blocks) = &doc.runs[0];
    let tl = prosody::Planner::new(v).plan(blocks);
    let frames = score::render_frames(&tl, v);
    let mut eng = synth::Engine::new(v.clone(), 1);
    eng.keep_trace = true;
    let raw = eng.render_internal(&frames);
    let audio = synth::finish(&raw, 1.0);
    let fs = synth::OUTPUT_RATE as f64;
    for ts in &tl.segs {
        print!("{}[{:.3}+{:.0}] ", ts.seg.phone.ipa, ts.t0, ts.dur * 1000.0);
    }
    println!();
    let hop = 0.01;
    let peak = audio.iter().fold(0.0f32, |m, s| m.max(s.abs())) as f64;
    let mut areas = Vec::new();
    let mut t = 0.0;
    while t < tl.end {
        let a = (t * fs) as usize;
        let b = (((t + hop) * fs) as usize).min(audio.len());
        if a >= b { break; }
        let rms = (audio[a..b].iter().map(|&s| (s as f64).powi(2)).sum::<f64>() / (b - a) as f64).sqrt();
        let db = 20.0 * (rms / peak).max(1e-6).log10();
        let fi = ((t + hop / 2.0) * synth::CONTROL_RATE) as usize;
        let seg = tl.segs.iter().find(|s| s.t0 <= t + hop / 2.0 && t + hop / 2.0 < s.end()).map(|s| s.seg.phone.ipa).unwrap_or("·");
        if let (Some(tr), Some(fr)) = (eng.trace.get(fi), frames.get(fi)) {
            articulation::area_function(&fr.art, &v.anatomy, eng.tract.n, &mut areas);
            let (_, amin) = articulation::min_constriction(&areas, &v.anatomy);
            let bar = "#".repeat(((db + 60.0).max(0.0) / 1.5) as usize);
            println!("{:5.2} {:3} {:6.1}dB vib {:4.2} Po {:5.0} lung {:4.0} glot {:5.2} amin {:5.2} f0 {:4.0} {}", t, seg, db, tr.voicing, tr.oral_pressure, fr.art.lung, fr.art.glottis, amin, fr.f0, bar);
        }
        t += hop;
    }
    let s = "/tmp/claude-1000/-home-magicaltux-projects-nanami/36d41bca-cc2a-4bf5-9c52-554bce78d3fa/scratchpad";
    nanami::audio_io::write_wav(std::path::Path::new(&format!("{s}/env.wav")), &audio, 48000).unwrap();
    nanami::audio_io::write_spectrogram(std::path::Path::new(&format!("{s}/env.png")), &audio, 48000, 8000.0).unwrap();
}
