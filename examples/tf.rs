//! Tract transfer function peak levels (dB) for a vowel: `tf ə [voice]`.
use nanami::{analysis, articulation::Articulators, phoneme::VowelShape, voice::Voice};
fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let v = Voice::by_name(a.get(1).map(|s| s.as_str()).unwrap_or("nanami")).unwrap();
    let mut art = Articulators::default();
    VowelShape::lookup_in(v.vowels, &a[0]).unwrap().apply(&mut art);
    let nfft = 1 << 15;
    let spec = analysis::tract_response_db(&v.anatomy, v.nose_cm, &art, nfft);
    let bin = 96000.0 / nfft as f64;
    for f in [100.0, 300.0, 500.0, 1000.0, 1500.0, 2000.0, 3000.0, 4000.0, 6000.0, 8000.0, 12000.0] {
        print!("{f}:{:.1} ", spec[(f / bin) as usize]);
    }
    println!();
    let peaks = analysis::pick_peaks(&spec, 96000.0, nfft, 150.0, 9000.0, 8);
    for p in peaks {
        print!("F{:.0}:{:.1} ", p, spec[(p / bin).round() as usize]);
    }
    println!();
}
