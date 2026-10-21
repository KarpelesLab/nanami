//! Formant frequencies and −3 dB bandwidths of static vowel shapes, from the
//! waveguide impulse response (glottis closed). `bandwidth [voice]`.
use nanami::{analysis, articulation::Articulators, phoneme::VowelShape, voice::Voice};
fn main() {
    let v = Voice::by_name(&std::env::args().nth(1).unwrap_or("nanami".into())).unwrap();
    for ipa in ["i", "ɛ", "æ", "ɑ", "ɔ", "u", "ə"] {
        let mut art = Articulators::default();
        VowelShape::lookup_in(v.vowels, ipa).unwrap().apply(&mut art);
        let nfft = 1 << 16;
        let spec = analysis::tract_response_db(&v.anatomy, v.nose_cm, &art, nfft);
        let bin = 96000.0 / nfft as f64;
        let peaks = analysis::pick_peaks(&spec, 96000.0, nfft, 150.0, 8500.0, 8);
        let bws: Vec<String> = peaks
            .iter()
            .map(|&f| {
                let k = (f / bin).round() as usize;
                let top = spec[k];
                let (mut lo, mut hi) = (k, k);
                while lo > 0 && spec[lo] > top - 3.0 { lo -= 1; }
                while hi + 1 < spec.len() && spec[hi] > top - 3.0 { hi += 1; }
                format!("{:.0}({:.0})", f, (hi - lo) as f64 * bin)
            })
            .collect();
        println!("{ipa}  {}", bws.join("  "));
    }
}
