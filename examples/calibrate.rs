//! Acoustic-to-articulatory calibration of the vowel inventory.
//!
//! For each vowel, searches the Stevens & House parameter space (tongue
//! position, constriction radius, lip area, lip length) with Nelder–Mead so the
//! waveguide's formants match published adult-male formant means
//! (Hillenbrand et al. 1995 for American English; Peterson & Barney 1952 and
//! typical cardinal-vowel values elsewhere), regularized toward a
//! phonetically plausible prior.
//!
//! Usage: cargo run --release --example calibrate > src/vowel_table.rs

use nanami::analysis::tract_formants;
use nanami::articulation::{Anatomy, Articulators};

struct Target {
    ipa: &'static str,
    f: [f64; 3],
    prior: [f64; 6],
}

const TARGETS: &[Target] = &[
    Target { ipa: "i", f: [300.0, 2300.0, 3000.0], prior: [0.75, 0.3, 2.5, 1.0, 1.5, 1.6] },
    Target { ipa: "ɪ", f: [427.0, 2034.0, 2684.0], prior: [0.72, 0.45, 2.5, 1.0, 1.5, 1.6] },
    Target { ipa: "e", f: [476.0, 2089.0, 2691.0], prior: [0.7, 0.5, 2.5, 1.0, 1.5, 1.6] },
    Target { ipa: "ɛ", f: [580.0, 1799.0, 2605.0], prior: [0.66, 0.65, 3.0, 1.0, 1.5, 1.6] },
    Target { ipa: "æ", f: [650.0, 1850.0, 2550.0], prior: [0.6, 0.75, 4.0, 1.0, 1.5, 1.6] },
    Target { ipa: "a", f: [750.0, 1250.0, 2550.0], prior: [0.4, 0.6, 4.0, 1.0, 1.0, 1.0] },
    Target { ipa: "ɑ", f: [750.0, 1150.0, 2500.0], prior: [0.3, 0.55, 4.0, 1.0, 1.0, 1.0] },
    Target { ipa: "ɒ", f: [650.0, 950.0, 2500.0], prior: [0.28, 0.5, 2.0, 1.3, 1.0, 1.0] },
    Target { ipa: "ɔ", f: [600.0, 900.0, 2450.0], prior: [0.33, 0.5, 1.5, 1.3, 1.0, 1.0] },
    Target { ipa: "o", f: [470.0, 850.0, 2400.0], prior: [0.45, 0.45, 1.0, 1.4, 1.0, 1.0] },
    Target { ipa: "ʊ", f: [469.0, 1122.0, 2434.0], prior: [0.55, 0.5, 1.5, 1.2, 1.0, 1.0] },
    Target { ipa: "u", f: [320.0, 850.0, 2300.0], prior: [0.62, 0.35, 0.5, 1.6, 1.0, 1.0] },
    Target { ipa: "ʌ", f: [623.0, 1200.0, 2550.0], prior: [0.45, 0.7, 3.0, 1.0, 1.0, 1.0] },
    Target { ipa: "ə", f: [500.0, 1500.0, 2500.0], prior: [0.5, 1.2, 3.0, 1.0, 1.0, 1.0] },
    Target { ipa: "ɜ", f: [550.0, 1450.0, 2450.0], prior: [0.5, 0.9, 3.0, 1.0, 1.0, 1.0] },
    Target { ipa: "ɐ", f: [650.0, 1350.0, 2500.0], prior: [0.45, 0.75, 3.5, 1.0, 1.0, 1.0] },
    Target { ipa: "ɯ", f: [350.0, 1350.0, 2350.0], prior: [0.6, 0.4, 2.0, 1.0, 1.0, 1.0] },
    Target { ipa: "ɤ", f: [450.0, 1150.0, 2450.0], prior: [0.55, 0.5, 2.0, 1.0, 1.0, 1.0] },
    Target { ipa: "ɨ", f: [350.0, 1600.0, 2400.0], prior: [0.65, 0.35, 2.0, 1.0, 1.5, 1.6] },
    Target { ipa: "ʉ", f: [330.0, 1350.0, 2200.0], prior: [0.65, 0.35, 0.7, 1.4, 1.0, 1.0] },
    Target { ipa: "y", f: [300.0, 1800.0, 2150.0], prior: [0.75, 0.3, 0.5, 1.5, 1.5, 1.6] },
    Target { ipa: "ø", f: [380.0, 1550.0, 2250.0], prior: [0.7, 0.5, 0.8, 1.4, 1.5, 1.6] },
    Target { ipa: "œ", f: [520.0, 1450.0, 2350.0], prior: [0.65, 0.7, 1.2, 1.3, 1.5, 1.6] },
];

const LO: [f64; 6] = [0.2, 0.12, 0.15, 0.5, 0.6, 0.5];
const HI: [f64; 6] = [0.88, 1.8, 4.5, 2.2, 3.0, 2.8];

fn to_art(p: &[f64; 6]) -> Articulators {
    let mut a = Articulators::default();
    a.tongue_pos = p[0];
    a.tongue_radius = p[1];
    a.lip_area = p[2];
    a.lip_length = p[3];
    a.tongue_length = p[4];
    a.pharynx = p[5];
    a
}

fn clampv(p: [f64; 6]) -> [f64; 6] {
    let mut q = p;
    for i in 0..6 {
        q[i] = q[i].clamp(LO[i], HI[i]);
    }
    q
}

fn cost(anat: &Anatomy, t: &Target, p: &[f64; 6]) -> f64 {
    let p = clampv(*p);
    let f = tract_formants(anat, 11.5, &to_art(&p), 3);
    if f.len() < 3 {
        return 1e3;
    }
    let w = [1.0, 1.0, 0.4];
    let mut c = 0.0;
    for i in 0..3 {
        c += w[i] * (f[i] / t.f[i]).ln().powi(2);
    }
    // Regularize toward the prior (in normalized units).
    let scale = [0.2, 0.5, 3.0, 0.8, 1.0, 0.8];
    let mut r = 0.0;
    for i in 0..6 {
        r += ((p[i] - t.prior[i]) / scale[i]).powi(2);
    }
    c + 0.0015 * r
}

fn nelder_mead(f: impl Fn(&[f64; 6]) -> f64, start: [f64; 6], step: [f64; 6], iters: usize) -> ([f64; 6], f64) {
    let mut simplex: Vec<([f64; 6], f64)> = vec![(start, f(&start))];
    for i in 0..6 {
        let mut p = start;
        p[i] += step[i];
        simplex.push((p, f(&p)));
    }
    for _ in 0..iters {
        simplex.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
        let mut c = [0.0; 6];
        for s in &simplex[..6] {
            for i in 0..6 {
                c[i] += s.0[i] / 6.0;
            }
        }
        let worst = simplex[6];
        let pt = |t: f64| {
            let mut p = [0.0; 6];
            for i in 0..6 {
                p[i] = c[i] + t * (worst.0[i] - c[i]);
            }
            p
        };
        let r = pt(-1.0);
        let fr = f(&r);
        if fr < simplex[0].1 {
            let e = pt(-2.0);
            let fe = f(&e);
            simplex[6] = if fe < fr { (e, fe) } else { (r, fr) };
        } else if fr < simplex[5].1 {
            simplex[6] = (r, fr);
        } else {
            let k = pt(0.5);
            let fk = f(&k);
            if fk < worst.1 {
                simplex[6] = (k, fk);
            } else {
                let best = simplex[0].0;
                for s in simplex.iter_mut().skip(1) {
                    for i in 0..6 {
                        s.0[i] = best[i] + 0.5 * (s.0[i] - best[i]);
                    }
                    s.1 = f(&s.0);
                }
            }
        }
    }
    simplex.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
    (clampv(simplex[0].0), simplex[0].1)
}

fn main() {
    let anat = Anatomy::adult_male();
    println!("// @generated by `cargo run --release --example calibrate` — do not edit by hand.");
    println!("#![allow(clippy::approx_constant)]");
    println!("// Vowel carrier shapes: (ipa, tongue_pos, tongue_radius, lip_area, lip_length, tongue_length, pharynx).");
    println!("// Fitted to adult male formant targets; comments show target → achieved F1–F3.");
    println!("pub const VOWEL_SHAPES: &[(&str, f64, f64, f64, f64, f64, f64)] = &[");
    let results: Vec<_> = std::thread::scope(|s| {
        let handles: Vec<_> = TARGETS
            .iter()
            .map(|t| {
                let anat = &anat;
                s.spawn(move || {
                    let mut best = (t.prior, f64::INFINITY);
                    // Multi-start from jittered priors.
                    for k in 0..6 {
                        let mut start = t.prior;
                        let j = [0.08, 0.2, 1.0, 0.3, 0.4, 0.4];
                        for i in 0..6 {
                            let u = (((k * 7 + i * 13) % 11) as f64 / 10.0 - 0.5) * 2.0;
                            if k > 0 {
                                start[i] += u * j[i];
                            }
                        }
                        let r = nelder_mead(|p| cost(anat, t, p), clampv(start), [0.05, 0.15, 0.8, 0.3, 0.3, 0.3], 300);
                        if r.1 < best.1 {
                            best = r;
                        }
                    }
                    best
                })
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });
    for (t, (p, c)) in TARGETS.iter().zip(results) {
        let f = tract_formants(&anat, 11.5, &to_art(&p), 3);
        println!(
            "    (\"{}\", {:.4}, {:.4}, {:.4}, {:.4}, {:.4}, {:.4}), // {:?} → {:?} cost {:.4}",
            t.ipa,
            p[0],
            p[1],
            p[2],
            p[3],
            p[4],
            p[5],
            t.f.map(|x| x as i64),
            f.iter().map(|x| x.round() as i64).collect::<Vec<_>>(),
            c
        );
    }
    println!("];");
}
