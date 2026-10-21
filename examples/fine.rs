//! Sample-accurate (0.5 ms) aero trace in a window: `fine "<text>" t0 t1`.
use nanami::{articulation, ipa::Lang, markup, prosody, score, synth, voice::Voice};
fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let (t0, t1): (f64, f64) = (a[1].parse().unwrap(), a[2].parse().unwrap());
    let voice = Voice::by_name("nanami").unwrap();
    let doc = markup::parse(&a[0], &voice, Lang::English);
    let (v, blocks) = &doc.runs[0];
    let tl = prosody::Planner::new(v).plan(blocks);
    let frames = score::render_frames(&tl, v);
    let mut eng = synth::Engine::new(v.clone(), 1);
    eng.keep_trace = true;
    let raw = eng.render_internal(&frames);
    let mut areas = Vec::new();
    for fi in ((t0 * synth::CONTROL_RATE) as usize)..((t1 * synth::CONTROL_RATE) as usize) {
        let fr = &frames[fi];
        let tr = &eng.trace[fi];
        articulation::area_function(&fr.art, &v.anatomy, eng.tract.n, &mut areas);
        let (imin, amin) = articulation::min_constriction(&areas, &v.anatomy);
        let blk = &raw[fi * synth::BLOCK..(fi + 1) * synth::BLOCK];
        let rms = (blk.iter().map(|x| x * x).sum::<f64>() / blk.len() as f64).sqrt();
        println!("{:.4} rms {:9.4} vib {:4.2} Po {:5.0} Ug {:8.2e} glot {:5.3} amin {:5.3}@{:2} tip {:5.3} obst {:4.2} fric {:5.1}",
            fi as f64 / synth::CONTROL_RATE, rms, tr.voicing, tr.oral_pressure, tr.glottal_flow, fr.art.glottis, amin, imin, fr.art.tip_area, fr.art.obstacle, tr.frication);
    }
}
