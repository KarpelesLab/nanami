//! Acoustic-to-articulatory calibration of approximants and rhotic vowels.
//!
//! Approximants are vowel-like: besides their constrictions they have a
//! tongue-body posture of their own (Öhman 1966; Gick 2004 for /l/ and /ɹ/).
//! For each one we fit the carrier shape (same 6 parameters as the vowels)
//! together with the areas of its constriction gestures, so the static tract
//! matches published adult-male formants of American English approximants
//! (Lehiste 1964; Espy-Wilson 1992; Delattre & Freeman 1968 for /ɹ/).
//!
//! Usage: cargo run --release --example calibrate_approx > src/approx_table.rs

use nanami::analysis::tract_formants;
use nanami::articulation::{Anatomy, Articulators};

#[derive(Clone, Copy)]
enum G {
    Lips,
    Tip(f64),
    Dorsum(f64),
    Root,
    Round(f64),
}

struct Target {
    ipa: &'static str,
    name: &'static str,
    f: [f64; 3],
    prior: [f64; 6],
    gestures: &'static [(G, f64)],
}

const TARGETS: &[Target] = &[
    Target { ipa: "ɹ", name: "R", f: [350.0, 1100.0, 1500.0], prior: [0.55, 0.6, 1.5, 1.3, 1.0, 1.0], gestures: &[(G::Tip(0.74), 0.3), (G::Root, 1.1), (G::Round(1.3), 1.4)] },
    Target { ipa: "ɚ", name: "SCHWAR", f: [480.0, 1350.0, 1650.0], prior: [0.5, 0.9, 2.0, 1.2, 1.0, 1.0], gestures: &[(G::Tip(0.74), 0.4), (G::Root, 1.3), (G::Round(1.2), 1.8)] },
    Target { ipa: "ɝ", name: "ER", f: [490.0, 1350.0, 1600.0], prior: [0.5, 0.8, 2.0, 1.2, 1.0, 1.0], gestures: &[(G::Tip(0.74), 0.35), (G::Root, 1.2), (G::Round(1.3), 1.6)] },
    Target { ipa: "l", name: "L", f: [360.0, 1050.0, 2700.0], prior: [0.45, 0.7, 2.5, 1.0, 1.0, 1.0], gestures: &[(G::Tip(0.915), 0.25)] },
    Target { ipa: "ɫ", name: "DARK_L", f: [450.0, 850.0, 2600.0], prior: [0.4, 0.6, 2.0, 1.1, 1.0, 0.9], gestures: &[(G::Tip(0.915), 0.3), (G::Dorsum(0.55), 0.9)] },
    Target { ipa: "w", name: "W", f: [300.0, 650.0, 2200.0], prior: [0.62, 0.4, 0.4, 1.6, 1.0, 1.0], gestures: &[(G::Lips, 0.25), (G::Dorsum(0.62), 0.5)] },
    Target { ipa: "j", name: "J", f: [270.0, 2200.0, 2900.0], prior: [0.8, 0.5, 3.0, 1.5, 1.5, 1.7], gestures: &[(G::Dorsum(0.76), 0.3)] },
];

const LO: [f64; 6] = [0.2, 0.12, 0.15, 0.5, 0.6, 0.5];
const HI: [f64; 6] = [0.88, 1.8, 4.5, 2.2, 3.0, 2.8];
/// Approximant constrictions stay wide enough not to become fricatives.
const AREA_LO: f64 = 0.15;
const AREA_HI: f64 = 4.0;

fn params(t: &Target, p: &[f64]) -> ([f64; 6], Vec<f64>) {
    let mut c = [0.0; 6];
    for i in 0..6 {
        c[i] = p[i].clamp(LO[i], HI[i]);
    }
    let areas = p[6..].iter().map(|a| a.exp().clamp(AREA_LO, AREA_HI)).collect::<Vec<_>>();
    debug_assert_eq!(areas.len(), t.gestures.len());
    (c, areas)
}

fn to_art(t: &Target, p: &[f64]) -> Articulators {
    let (c, areas) = params(t, p);
    let mut a = Articulators::default();
    a.tongue_pos = c[0];
    a.tongue_radius = c[1];
    a.lip_area = c[2];
    a.lip_length = c[3];
    a.tongue_length = c[4];
    a.pharynx = c[5];
    for (&(g, _), &area) in t.gestures.iter().zip(&areas) {
        match g {
            G::Lips => a.lip_area = a.lip_area.min(area),
            G::Tip(pos) => {
                a.tip_pos = pos;
                a.tip_area = area;
            }
            G::Dorsum(pos) => {
                a.dorsum_pos = pos;
                a.dorsum_area = area;
            }
            G::Root => a.root_area = area,
            G::Round(len) => {
                a.lip_area = a.lip_area.min(area);
                a.lip_length = a.lip_length.max(len);
            }
        }
    }
    a
}

fn cost(anat: &Anatomy, t: &Target, p: &[f64]) -> f64 {
    let f = tract_formants(anat, 11.5, &to_art(t, p), 3);
    if f.len() < 3 {
        return 1e3;
    }
    let mut c = 0.0;
    for i in 0..3 {
        c += (f[i] / t.f[i]).ln().powi(2);
    }
    let scale = [0.2, 0.5, 3.0, 0.8, 1.0, 0.8];
    let mut r = 0.0;
    for i in 0..6 {
        r += ((p[i] - t.prior[i]) / scale[i]).powi(2);
    }
    for (k, &(_, a0)) in t.gestures.iter().enumerate() {
        r += ((p[6 + k] - a0.ln()) / 1.0).powi(2);
    }
    c + 0.0015 * r
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
    simplex.swap_remove(0)
}

fn main() {
    let anat = Anatomy::adult_male();
    let results: Vec<_> = std::thread::scope(|s| {
        let handles: Vec<_> = TARGETS
            .iter()
            .map(|t| {
                let anat = &anat;
                s.spawn(move || {
                    let mut start: Vec<f64> = t.prior.to_vec();
                    start.extend(t.gestures.iter().map(|g| g.1.ln()));
                    let mut step = vec![0.05, 0.15, 0.8, 0.3, 0.3, 0.3];
                    step.extend(t.gestures.iter().map(|_| 0.4));
                    let mut best = (start.clone(), f64::INFINITY);
                    for k in 0..8 {
                        let mut s0 = start.clone();
                        if k > 0 {
                            for (i, v) in s0.iter_mut().enumerate() {
                                let u = (((k * 7 + i * 13) % 11) as f64 / 10.0 - 0.5) * 2.0;
                                *v += u * step[i] * 0.8;
                            }
                        }
                        let r = nelder_mead(|p| cost(anat, t, p), s0, &step, 500);
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
    println!("// @generated by `cargo run --release --example calibrate_approx` — do not edit by hand.");
    println!("#![allow(clippy::approx_constant)]");
    println!("// Approximant / rhotic-vowel tongue-body carriers and constriction gestures,");
    println!("// fitted to adult male formant targets; comments show target → achieved F1–F3.");
    println!("use crate::phoneme::Gesture::{{self, *}};");
    println!();
    println!("pub const APPROX_SHAPES: &[(&str, [f64; 6], [f64; 8])] = &[");
    for (t, (p, c)) in TARGETS.iter().zip(&results) {
        let (cv, _) = params(t, p);
        let f = tract_formants(&anat, 11.5, &to_art(t, p), 3);
        println!(
            "    (\"{}\", [{:.4}, {:.4}, {:.4}, {:.4}, {:.4}, {:.4}], [0.0; 8]), // {:?} → {:?} cost {:.4}",
            t.ipa,
            cv[0],
            cv[1],
            cv[2],
            cv[3],
            cv[4],
            cv[5],
            t.f.map(|x| x as i64),
            f.iter().map(|x| x.round() as i64).collect::<Vec<_>>(),
            c
        );
    }
    println!("];");
    for (t, (p, _)) in TARGETS.iter().zip(&results) {
        let (_, areas) = params(t, p);
        let gs: Vec<String> = t
            .gestures
            .iter()
            .zip(&areas)
            .map(|(&(g, _), a)| match g {
                G::Lips => format!("Lips({a:.3})"),
                G::Tip(pos) => format!("Tip({pos}, {a:.3})"),
                G::Dorsum(pos) => format!("Dorsum({pos}, {a:.3})"),
                G::Root => format!("Root({a:.3})"),
                G::Round(len) => format!("Round({a:.3}, {len})"),
            })
            .collect();
        println!("pub const G_{}: &[Gesture] = &[{}];", t.name, gs.join(", "));
    }
}
