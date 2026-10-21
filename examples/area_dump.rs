//! Print the area function and formants of a vowel shape: `area_dump i`.
use nanami::{analysis, articulation::*, phoneme::VowelShape};
fn main() {
    let ipa = std::env::args().nth(1).unwrap_or("i".into());
    let anat = Anatomy::adult_male();
    let mut art = Articulators::default();
    VowelShape::lookup(&ipa).unwrap().apply(&mut art);
    let n = (anat.length_cm / (350.0 / 96000.0 * 100.0)).round() as usize;
    let mut a = Vec::new();
    area_function(&art, &anat, n, &mut a);
    for (i, v) in a.iter().enumerate().step_by(3) {
        println!("{:5.1} cm {:5.2} {}", (i as f64 + 0.5) * anat.length_cm / n as f64, v, "#".repeat((v * 6.0) as usize));
    }
    println!("{:?}", analysis::tract_formants(&anat, 11.5, &art, 4));
}
