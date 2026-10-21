//! Formant transitions into and out of each consonant closure (static tract
//! formants of the articulation; F2 at the edges ≈ locus):
//! `locus "/ɑ ˈbɑ ɑ ˈdɑ/"` prints F1–F3 at −30, −15, −5 ms before the
//! consonant and +5, +15, +30 ms after its release.
use nanami::{analysis, ipa::Lang, markup, prosody, score, synth, voice::Voice};
fn main() {
    let text = std::env::args().nth(1).unwrap();
    let voice = Voice::by_name(&std::env::var("VOICE").unwrap_or("nanami".into())).unwrap();
    let doc = markup::parse(&text, &voice, Lang::English);
    let (v, blocks) = &doc.runs[0];
    let tl = prosody::Planner::new(v).plan(blocks);
    let frames = score::render_frames(&tl, v);
    let at = |t: f64| {
        let fr = &frames[((t * synth::CONTROL_RATE) as usize).min(frames.len() - 1)];
        let f = analysis::tract_formants(&v.anatomy, v.nose_cm, &fr.art, 3);
        format!("{:>16}", f.iter().map(|x| format!("{:.0}", x)).collect::<Vec<_>>().join("/"))
    };
    for (i, ts) in tl.segs.iter().enumerate() {
        if ts.seg.phone.is_vowel() || i == 0 {
            continue;
        }
        let ctx = format!("{}{}{}", tl.segs[i - 1].seg.phone.ipa, ts.seg.phone.ipa, tl.segs.get(i + 1).map(|s| s.seg.phone.ipa).unwrap_or(""));
        println!("{ctx:6} in {} {} {} | out {} {} {}", at(ts.t0 - 0.03), at(ts.t0 - 0.015), at(ts.t0 - 0.005), at(ts.end() + 0.005), at(ts.end() + 0.015), at(ts.end() + 0.03));
    }
}
