//! Acoustic-to-articulatory calibration of the vowel inventory.
//!
//! For each vowel, searches the articulatory parameter space — Stevens & House
//! tongue (position, radius, front length, pharynx), lips (area, length), and
//! an 8-point smooth area-correction curve — with Nelder–Mead so the
//! waveguide's formants match published adult-male formant means
//! (Hillenbrand et al. 1995 for American English — except /ɛ æ/, from
//! Peterson & Barney 1952, whose æ is not raised toward ɛ; P&B and
//! typical cardinal-vowel values elsewhere), regularized toward a
//! phonetically plausible prior.
//!
//! Usage: cargo run --release --example calibrate > src/vowel_table.rs
//!        cargo run --release --example calibrate female > src/vowel_table_female.rs
//! The female set is fitted on NANAMI's anatomy to Hillenbrand et al.'s
//! women's means (P&B women for /ɛ æ/; other vowels scaled from the male set).

use nanami::analysis::tract_formants;
use nanami::articulation::{ADJ_N, Anatomy, Articulators};
use nanami::voice::Voice;

/// Female F1–F3 targets for vowels with published women's means.
const FEMALE: &[(&str, [f64; 3])] = &[
    ("i", [437.0, 2761.0, 3372.0]),
    ("ɪ", [483.0, 2365.0, 3053.0]),
    ("e", [536.0, 2530.0, 3047.0]),
    ("ɛ", [610.0, 2330.0, 2990.0]),
    ("æ", [860.0, 2050.0, 2850.0]),
    ("ɑ", [936.0, 1551.0, 2815.0]),
    ("ɔ", [781.0, 1136.0, 2824.0]),
    ("o", [555.0, 1035.0, 2828.0]),
    ("ʊ", [519.0, 1225.0, 2827.0]),
    ("u", [459.0, 1105.0, 2735.0]),
    ("ʌ", [753.0, 1426.0, 2933.0]),
    ("ə", [580.0, 1720.0, 2900.0]),
];

fn female_target(t: &Target) -> [f64; 3] {
    FEMALE.iter().find(|f| f.0 == t.ipa).map(|f| f.1).unwrap_or([t.f[0] * 1.15, t.f[1] * 1.15, t.f[2] * 1.12])
}

struct Target {
    ipa: &'static str,
    f: [f64; 3],
    prior: [f64; 6],
}

const TARGETS: &[Target] = &[
    Target { ipa: "i", f: [342.0, 2322.0, 3000.0], prior: [0.75, 0.3, 2.5, 1.0, 1.5, 1.6] },
    Target { ipa: "ɪ", f: [427.0, 2034.0, 2684.0], prior: [0.72, 0.45, 2.5, 1.0, 1.5, 1.6] },
    Target { ipa: "e", f: [476.0, 2089.0, 2691.0], prior: [0.7, 0.5, 2.5, 1.0, 1.5, 1.6] },
    Target { ipa: "ɛ", f: [530.0, 1840.0, 2480.0], prior: [0.66, 0.65, 3.0, 1.0, 1.5, 1.6] },
    Target { ipa: "æ", f: [660.0, 1720.0, 2410.0], prior: [0.6, 0.75, 4.0, 1.0, 1.5, 1.6] },
    Target { ipa: "a", f: [750.0, 1250.0, 2550.0], prior: [0.4, 0.6, 4.0, 1.0, 1.0, 1.0] },
    Target { ipa: "ɑ", f: [768.0, 1333.0, 2522.0], prior: [0.3, 0.55, 4.0, 1.0, 1.0, 1.0] },
    Target { ipa: "ɒ", f: [650.0, 950.0, 2500.0], prior: [0.28, 0.5, 2.0, 1.3, 1.0, 1.0] },
    Target { ipa: "ɔ", f: [652.0, 997.0, 2538.0], prior: [0.33, 0.5, 1.5, 1.3, 1.0, 1.0] },
    Target { ipa: "o", f: [497.0, 910.0, 2459.0], prior: [0.45, 0.45, 1.0, 1.4, 1.0, 1.0] },
    Target { ipa: "ʊ", f: [469.0, 1122.0, 2434.0], prior: [0.55, 0.5, 1.5, 1.2, 1.0, 1.0] },
    Target { ipa: "u", f: [378.0, 997.0, 2343.0], prior: [0.62, 0.35, 0.5, 1.6, 1.0, 1.0] },
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
const NP: usize = 6 + ADJ_N;
/// Bound on the log-area correction (factor 0.3 … 3.3).
const ADJ_MAX: f64 = 1.2;

fn clampv(p: &[f64]) -> Vec<f64> {
    let mut q = p.to_vec();
    for i in 0..6 {
        q[i] = q[i].clamp(LO[i], HI[i]);
    }
    for v in q.iter_mut().skip(6) {
        *v = v.clamp(-ADJ_MAX, ADJ_MAX);
    }
    q
}

fn to_art(p: &[f64]) -> Articulators {
    let p = clampv(p);
    let mut a = Articulators::default();
    a.tongue_pos = p[0];
    a.tongue_radius = p[1];
    a.lip_area = p[2];
    a.lip_length = p[3];
    a.tongue_length = p[4];
    a.pharynx = p[5];
    for k in 0..ADJ_N {
        a.adj[k] = p[6 + k];
    }
    a
}

fn cost(anat: &Anatomy, nose: f64, t: &Target, p: &[f64]) -> f64 {
    let f = tract_formants(anat, nose, &to_art(p), 3);
    if f.len() < 3 {
        return 1e3;
    }
    let w = [1.0, 1.0, 0.4];
    let mut c = 0.0;
    for i in 0..3 {
        c += w[i] * (f[i] / t.f[i]).ln().powi(2);
    }
    // Regularize toward the prior (in normalized units) …
    let scale = [0.2, 0.5, 3.0, 0.8, 1.0, 0.8];
    let mut r = 0.0;
    for i in 0..6 {
        r += ((p[i] - t.prior[i]) / scale[i]).powi(2);
    }
    // … and keep the correction curve small and smooth (no fitted ripples).
    let adj = &p[6..];
    let size: f64 = adj.iter().map(|v| v * v).sum();
    let rough: f64 = adj.windows(2).map(|w| (w[1] - w[0]).powi(2)).sum();
    c + 0.0015 * r + 0.004 * size + 0.01 * rough
}

fn nelder_mead(f: impl Fn(&[f64]) -> f64, start: Vec<f64>, step: &[f64], iters: usize) -> (Vec<f64>, f64) {
    let n = start.len();
    let mut simplex: Vec<(Vec<f64>, f64)> = vec![(start.clone(), f(&start))];
    for i in 0..n {
        let mut p = start.clone();
        p[i] += step[i];
        let v = f(&p);
        simplex.push((p, v));
    }
    for _ in 0..iters {
        simplex.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
        let mut c = vec![0.0; n];
        for s in &simplex[..n] {
            for i in 0..n {
                c[i] += s.0[i] / n as f64;
            }
        }
        let worst = simplex[n].clone();
        let pt = |t: f64| (0..n).map(|i| c[i] + t * (worst.0[i] - c[i])).collect::<Vec<_>>();
        let r = pt(-1.0);
        let fr = f(&r);
        if fr < simplex[0].1 {
            let e = pt(-2.0);
            let fe = f(&e);
            simplex[n] = if fe < fr { (e, fe) } else { (r, fr) };
        } else if fr < simplex[n - 1].1 {
            simplex[n] = (r, fr);
        } else {
            let k = pt(0.5);
            let fk = f(&k);
            if fk < worst.1 {
                simplex[n] = (k, fk);
            } else {
                let best = simplex[0].0.clone();
                for s in simplex.iter_mut().skip(1) {
                    for i in 0..n {
                        s.0[i] = best[i] + 0.5 * (s.0[i] - best[i]);
                    }
                    s.1 = f(&s.0);
                }
            }
        }
    }
    simplex.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
    let (p, c) = simplex.swap_remove(0);
    (clampv(&p), c)
}

fn main() {
    let female = std::env::args().nth(1).as_deref() == Some("female");
    let (anat, nose) = if female {
        let v = Voice::nanami();
        (v.anatomy, v.nose_cm)
    } else {
        (Anatomy::adult_male(), 11.5)
    };
    let targets: Vec<Target> = TARGETS.iter().map(|t| Target { f: if female { female_target(t) } else { t.f }, ..*t }).collect();
    let results: Vec<_> = std::thread::scope(|s| {
        let handles: Vec<_> = targets
            .iter()
            .map(|t| {
                let anat = &anat;
                s.spawn(move || {
                    let mut step = vec![0.05, 0.15, 0.8, 0.3, 0.3, 0.3];
                    step.extend([0.3; ADJ_N]);
                    let mut best = (vec![], f64::INFINITY);
                    // Multi-start from jittered priors; restart from the best
                    // point to escape simplex collapse in 14 dimensions.
                    for k in 0..8 {
                        let mut start: Vec<f64> = t.prior.to_vec();
                        start.extend([0.0; ADJ_N]);
                        if k > 0 {
                            for (i, v) in start.iter_mut().enumerate() {
                                let u = (((k * 7 + i * 13) % 11) as f64 / 10.0 - 0.5) * 2.0;
                                *v += u * step[i] * if i < 6 { 1.0 } else { 0.5 };
                            }
                        }
                        let mut r = nelder_mead(|p| cost(anat, nose, t, p), clampv(&start), &step, 1500);
                        for _ in 0..3 {
                            let small: Vec<f64> = step.iter().map(|s| s * 0.3).collect();
                            r = nelder_mead(|p| cost(anat, nose, t, p), r.0.clone(), &small, 1000);
                        }
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
    println!("// @generated by `cargo run --release --example calibrate` — do not edit by hand.");
    println!("#![allow(clippy::approx_constant)]");
    println!("// Vowel carrier shapes: (ipa, [tongue_pos, tongue_radius, lip_area, lip_length,");
    println!("// tongue_length, pharynx], [area correction at ADJ_POS]).");
    println!("// Fitted to adult {} formant targets; comments show target → achieved F1–F3.", if female { "female" } else { "male" });
    println!("pub const VOWEL_SHAPES{}: &[(&str, [f64; 6], [f64; 8])] = &[", if female { "_FEMALE" } else { "" });
    for (t, (p, c)) in targets.iter().zip(results) {
        let f = tract_formants(&anat, nose, &to_art(&p), 3);
        let fmt = |v: &[f64]| v.iter().map(|x| format!("{x:.4}")).collect::<Vec<_>>().join(", ");
        println!(
            "    (\"{}\", [{}], [{}]), // {:?} → {:?} cost {:.4}",
            t.ipa,
            fmt(&p[..6]),
            fmt(&p[6..NP]),
            t.f.map(|x| x as i64),
            f.iter().map(|x| x.round() as i64).collect::<Vec<_>>(),
            c
        );
    }
    println!("];");
}
