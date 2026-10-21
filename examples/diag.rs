//! Segment-level acoustic diagnostics: renders test syllables and reports
//! per-segment level (dB re loudest vowel) and spectral centroid.
use nanami::{ipa::Lang, markup, prosody, score, synth, voice::Voice};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let text = if args.is_empty() { "/ˈasa ˈaʃa ˈafa ˈaθa ˈaha ˈata ˈaka ˈapa ˈaza ˈama/".to_string() } else { args.join(" ") };
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
    let mut rows = Vec::new();
    let mut vmax = 0.0f64;
    for ts in &tl.segs {
        let a = ((ts.t0 + 0.2 * ts.dur) * fs) as usize;
        let b = ((ts.t0 + 0.8 * ts.dur) * fs) as usize;
        let x: Vec<f64> = audio[a.min(audio.len())..b.min(audio.len())].iter().map(|&s| s as f64).collect();
        if x.is_empty() { continue; }
        let rms = (x.iter().map(|v| v * v).sum::<f64>() / x.len() as f64).sqrt();
        // centroid via FFT of up to 2048 samples
        let nfft = 2048usize;
        let mut re: Vec<f64> = (0..nfft).map(|i| x.get(i).copied().unwrap_or(0.0) * (0.5 - 0.5 * (2.0 * std::f64::consts::PI * i as f64 / x.len().min(nfft) as f64).cos()) * if i < x.len() {1.0} else {0.0}).collect();
        let mut im = vec![0.0; nfft];
        nanami::dsp::fft(&mut re, &mut im);
        let (mut num, mut den) = (0.0, 0.0);
        for k in 1..nfft / 2 { let p = re[k] * re[k] + im[k] * im[k]; let f = k as f64 * fs / nfft as f64; num += f * p; den += p; }
        let fi = (ts.t0 * synth::CONTROL_RATE) as usize;
        let tr = eng.trace.get(fi + (0.5 * ts.dur * synth::CONTROL_RATE) as usize).cloned().unwrap_or_default();
        if ts.seg.is_vowel() { vmax = vmax.max(rms); }
        rows.push((ts.seg.phone.ipa, ts.t0, ts.dur, rms, num / den.max(1e-30), tr));
    }
    println!("seg   t0     dur   level  centroid  Po(Pa)  voicing  fric");
    for (ipa, t0, dur, rms, c, tr) in rows {
        println!("{:4} {:6.3} {:5.0}ms {:6.1}dB {:6.0}Hz {:7.0} {:6.2} {:8.2}", ipa, t0, dur * 1000.0, 20.0 * (rms / vmax).log10(), c, tr.oral_pressure, tr.voicing, tr.frication);
    }
    let s = "/tmp/claude-1000/-home-magicaltux-projects-nanami/36d41bca-cc2a-4bf5-9c52-554bce78d3fa/scratchpad";
    nanami::audio_io::write_wav(std::path::Path::new(&format!("{s}/diag.wav")), &audio, 48000).unwrap();
    nanami::audio_io::write_spectrogram(std::path::Path::new(&format!("{s}/diag.png")), &audio, 48000, 10000.0).unwrap();
}
