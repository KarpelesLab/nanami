//! Static formants of every English vowel for a voice vs. Hillenbrand et al.
//! (1995) means for that voice's sex: `vowel_check [nanami|male|female]`.
use nanami::{analysis, articulation::Articulators, phoneme::VowelShape, voice::Voice};
const MALE: &[(&str, [f64; 3])] = &[("i", [342., 2322., 3000.]), ("ɪ", [427., 2034., 2684.]), ("e", [476., 2089., 2691.]), ("ɛ", [580., 1799., 2605.]), ("æ", [588., 1952., 2601.]), ("ɑ", [768., 1333., 2522.]), ("ɔ", [652., 997., 2538.]), ("o", [497., 910., 2459.]), ("ʊ", [469., 1122., 2434.]), ("u", [378., 997., 2343.]), ("ʌ", [623., 1200., 2550.]), ("ɝ", [474., 1379., 1710.])];
const FEMALE: &[(&str, [f64; 3])] = &[("i", [437., 2761., 3372.]), ("ɪ", [483., 2365., 3053.]), ("e", [536., 2530., 3047.]), ("ɛ", [731., 2058., 2979.]), ("æ", [669., 2349., 2972.]), ("ɑ", [936., 1551., 2815.]), ("ɔ", [781., 1136., 2824.]), ("o", [555., 1035., 2828.]), ("ʊ", [519., 1225., 2827.]), ("u", [459., 1105., 2735.]), ("ʌ", [753., 1426., 2933.]), ("ɝ", [523., 1588., 1929.])];
fn main() {
    let name = std::env::args().nth(1).unwrap_or("nanami".into());
    let v = Voice::by_name(&name).unwrap();
    let refs = if name == "male" { MALE } else { FEMALE };
    let mut err = [0.0; 3];
    for (ipa, t) in refs {
        let mut art = Articulators::default();
        VowelShape::lookup_in(v.vowels, ipa).unwrap().apply(&mut art);
        let f = analysis::tract_formants(&v.anatomy, v.nose_cm, &art, 3);
        let d: Vec<String> = (0..3).map(|i| { let e = 100.0 * (f[i] / t[i] - 1.0); err[i] += e.abs(); format!("{:5.0}/{:5.0} ({:+4.0}%)", f[i], t[i], e) }).collect();
        println!("{ipa}  {}", d.join("  "));
    }
    println!("mean |err| F1 {:.1}%  F2 {:.1}%  F3 {:.1}%", err[0] / refs.len() as f64, err[1] / refs.len() as f64, err[2] / refs.len() as f64);
}
