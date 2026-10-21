//! Static tract formants of the articulatory state at given times of an
//! utterance: `shape_at "<text>" t1 t2 …` (separates articulation from dynamics).
use nanami::{analysis, ipa::Lang, markup, prosody, score, synth, voice::Voice};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let voice = Voice::by_name(&std::env::var("VOICE").unwrap_or("nanami".into())).unwrap();
    let doc = markup::parse(&args[0], &voice, Lang::English);
    let (v, blocks) = &doc.runs[0];
    let tl = prosody::Planner::new(v).plan(blocks);
    let frames = score::render_frames(&tl, v);
    for t in &args[1..] {
        let t: f64 = t.parse().unwrap();
        let fr = &frames[((t * synth::CONTROL_RATE) as usize).min(frames.len() - 1)];
        let f = analysis::tract_formants(&v.anatomy, v.nose_cm, &fr.art, 4);
        let seg = tl.segs.iter().find(|s| s.t0 <= t && t < s.end()).map(|s| s.seg.phone.ipa).unwrap_or("·");
        let a = &fr.art;
        println!("{t:.3} {seg:3} F {:?}  tongue pos {:.2} r {:.2} len {:.2} phar {:.2} lip {:.2}/{:.2} tip {:.2}@{:.2} dors {:.2}@{:.2} root {:.2}",
            f.iter().map(|x| x.round() as i64).collect::<Vec<_>>(), a.tongue_pos, a.tongue_radius, a.tongue_length, a.pharynx, a.lip_area, a.lip_length, a.tip_area, a.tip_pos, a.dorsum_area, a.dorsum_pos, a.root_area);
    }
}
