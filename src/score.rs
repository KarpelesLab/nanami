//! Gestural score: timed segments → articulator target keys → smooth
//! control trajectories → engine control frames.
//!
//! Each articulator tier is a critically damped second-order system tracking
//! piecewise-constant targets, as in Task Dynamics (Saltzman & Munhall 1989).
//! Vowels drive the tongue-body/lip carrier tiers; consonants drive separate
//! constriction tiers that combine with the carrier by taking the narrower
//! area. Vowel targets are issued during the preceding consonants
//! (anticipatory coarticulation), and constriction areas move in log-area
//! space so constrictions form and release with realistic acceleration.

use crate::articulation::{ADJ_N, Articulators, OPEN};
use crate::ipa::Seg;
use crate::phoneme::{Gesture, Manner, Place, VELAR_AUTO, VowelShape};
use crate::prosody::{Key, TimedSeg, Timeline};
use crate::synth::{CONTROL_RATE, ControlFrame};
use crate::voice::Voice;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(usize)]
enum T {
    VPos,
    VRad,
    VLen,
    VPhar,
    VLipA,
    VLipL,
    CLip,
    CTipPos,
    CTipA,
    CDorPos,
    CDorA,
    CRoot,
    CRoundA,
    CRoundL,
    Obst,
    Velum,
    Glottis,
    Count,
}
const NT: usize = T::Count as usize;

/// log-area transform so constrictions move smoothly toward closure.
fn la(a: f64) -> f64 {
    (a.max(0.0) + 0.01).ln()
}
fn ila(x: f64) -> f64 {
    (x.exp() - 0.01).max(0.0)
}
const CLOSURE: f64 = 0.004;

struct Tier {
    keys: Vec<Key>,
    idx: usize,
    target: f64,
    tau: f64,
    y1: f64,
    y2: f64,
}

impl Tier {
    fn new(init: f64, mut keys: Vec<Key>) -> Tier {
        keys.sort_by(|a, b| a.t.partial_cmp(&b.t).unwrap_or(std::cmp::Ordering::Equal));
        Tier { keys, idx: 0, target: init, tau: 0.02, y1: init, y2: init }
    }
    fn advance(&mut self, t: f64, dt: f64) -> f64 {
        while self.idx < self.keys.len() && self.keys[self.idx].t <= t {
            self.target = self.keys[self.idx].v;
            self.tau = self.keys[self.idx].tau.max(1e-4);
            self.idx += 1;
        }
        // Two cascaded one-poles = critically damped second-order response.
        let a = 1.0 - (-dt / (0.5 * self.tau)).exp();
        self.y1 += a * (self.target - self.y1);
        self.y2 += a * (self.y1 - self.y2);
        self.y2
    }
}

fn k(t: f64, v: f64, tau: f64) -> Key {
    Key { t, v, tau }
}

fn tiers_of(g: &Gesture) -> &'static [T] {
    match g {
        Gesture::Lips(_) => &[T::CLip],
        Gesture::Tip(..) => &[T::CTipA],
        Gesture::Dorsum(..) => &[T::CDorA],
        Gesture::Root(_) => &[T::CRoot],
        Gesture::Round(..) => &[T::CRoundA],
    }
}

fn seg_uses(seg: &Seg, tier: T) -> bool {
    seg.phone.gestures.iter().any(|g| tiers_of(g).contains(&tier))
}

/// Vowel carrier shape for a segment, with reduction and smile applied.
fn vowel_target(ts: &TimedSeg) -> VowelShape {
    let shape = ts.seg.phone.shape().unwrap_or(VowelShape::SCHWA);
    let mut red = (1.0 - ts.style.precision).max(0.0) * 0.6;
    if ts.seg.stress == 0 && !ts.sung && !ts.seg.offglide {
        red += 0.12;
    }
    let mut v = shape.reduce(red.min(0.8));
    let smile = ts.style.smile;
    if smile > 0.0 && v.lip_area > 1.2 {
        v.lip_area *= 1.0 + 0.35 * smile;
        v.lip_length *= 1.0 - 0.3 * smile;
    }
    v
}

pub struct Score {
    keys: Vec<Vec<Key>>,
    /// Keys for the carrier's area-correction curve, one tier per point.
    adj: Vec<Vec<Key>>,
}

impl Score {
    pub fn build(tl: &Timeline) -> Score {
        let mut keys: Vec<Vec<Key>> = (0..NT).map(|_| Vec::new()).collect();
        let mut adj: Vec<Vec<Key>> = (0..ADJ_N).map(|_| Vec::new()).collect();
        let segs = &tl.segs;
        let mut tip_free_at = -1.0f64; // time the tip tier became inactive
        for (i, ts) in segs.iter().enumerate() {
            let seg = &ts.seg;
            let p = seg.phone;
            let prec = ts.style.precision.clamp(0.4, 2.0);
            let (t0, end) = (ts.t0, ts.end());
            let next = segs.get(i + 1).filter(|n| n.t0 - end < 0.05);
            let prev = if i > 0 { segs.get(i - 1).filter(|pv| t0 - pv.end() < 0.05) } else { None };
            let whisper = ts.style.whisper;
            let voiced_g = if whisper { 0.2 } else { 0.0 };

            if p.is_vowel() {
                let v = vowel_target(ts);
                // Anticipation: start moving during the preceding consonant cluster.
                let mut j = i;
                // (Approximants carry their own tongue-body target, so the vowel
                // only starts moving at their release.)
                while j > 0 && segs[j - 1].seg.phone.vowel.is_none() && t0 - segs[j - 1].t0 < 0.25 {
                    j -= 1;
                }
                let cluster = t0 - if j < i { segs[j].t0 } else { t0 };
                // After a consonant that holds the tongue body itself, the body
                // only heads for the vowel at the release: CV formant transitions.
                let held = prev.map(|pv| pv.seg.phone.body_locus().is_some()).unwrap_or(false);
                let (tv, tau) = if held {
                    (t0 - 0.008, 0.03 / prec)
                } else if prev.is_none() && j == i {
                    // Phrase-initial vowel: shape the tract before voicing starts.
                    (t0 - 0.1, 0.024 / prec)
                } else if seg.offglide { (t0 - 0.02, 0.035 / prec) } else { (t0 - (0.55 * cluster).min(0.09) - 0.01, 0.024 / prec) };
                for (tier, val) in [(T::VPos, v.tongue_pos), (T::VRad, v.tongue_radius), (T::VLen, v.tongue_length), (T::VPhar, v.pharynx)] {
                    keys[tier as usize].push(k(tv, val, tau));
                }
                keys[T::VLipA as usize].push(k(tv, la(v.lip_area), tau * 0.8));
                keys[T::VLipL as usize].push(k(tv, v.lip_length, tau * 0.8));
                for (a, &val) in adj.iter_mut().zip(&v.adj) {
                    a.push(k(tv, val, tau));
                }
                if seg.nasalized {
                    keys[T::Velum as usize].push(k(t0 - 0.03, 0.7, 0.03));
                    if !next.map(|n| n.seg.nasalized || n.seg.phone.manner == Manner::Nasal).unwrap_or(false) {
                        keys[T::Velum as usize].push(k(end - 0.02, 0.0, 0.03));
                    }
                }
                let g = if seg.voiceless { 0.25 } else { voiced_g };
                // A preceding voiceless consonant schedules its own glottal return
                // (e.g. after the aspiration of /pʰ/); don't pre-empt it.
                let after_voiceless = prev.map(|pv| !pv.seg.phone.voiced || pv.seg.voiceless).unwrap_or(false);
                if seg.voiceless || !after_voiceless {
                    keys[T::Glottis as usize].push(k(t0 - 0.02, g, 0.015));
                }
                if seg.voiceless {
                    keys[T::Glottis as usize].push(k(end - 0.01, voiced_g, 0.015));
                }
                // Rhotic vowels: r-colouring gestures across the vowel.
                if !p.gestures.is_empty() {
                    emit_constrictions(&mut keys, ts, next, segs, i, prec, &mut tip_free_at, t0 + 0.15 * ts.dur, end);
                }
                continue;
            }

            // --- Consonants ---
            emit_constrictions(&mut keys, ts, next, segs, i, prec, &mut tip_free_at, t0, end);
            // Lingual consonants pull the tongue body toward their locus.
            if let Some((locus, w)) = p.body_locus() {
                let near = segs[i + 1..]
                    .iter()
                    .take(3)
                    .find(|s| s.seg.phone.is_vowel())
                    .or_else(|| segs[..i].iter().rev().take(2).find(|s| s.seg.phone.is_vowel()));
                let base = near.map(vowel_target).unwrap_or(VowelShape::SCHWA);
                let body = base.lerp(&locus, w * prec.min(1.2));
                let tv = t0 - if prev.is_none() { 0.1 } else { 0.055 };
                let tau = 0.025 / prec;
                for (tier, val) in [(T::VPos, body.tongue_pos), (T::VRad, body.tongue_radius), (T::VLen, body.tongue_length), (T::VPhar, body.pharynx)] {
                    keys[tier as usize].push(k(tv, val, tau));
                }
                for (a, &val) in adj.iter_mut().zip(&body.adj) {
                    a.push(k(tv, val, tau));
                }
            }
            // Approximants: vowel-like tongue-body and lip posture.
            if let Some(c) = p.shape() {
                let (tv, tau) = (t0 - if prev.is_none() { 0.1 } else { 0.03 }, 0.022 / prec);
                for (tier, val) in [(T::VPos, c.tongue_pos), (T::VRad, c.tongue_radius), (T::VLen, c.tongue_length), (T::VPhar, c.pharynx)] {
                    keys[tier as usize].push(k(tv, val, tau));
                }
                keys[T::VLipA as usize].push(k(tv, la(c.lip_area), tau * 0.8));
                keys[T::VLipL as usize].push(k(tv, c.lip_length, tau * 0.8));
                for (a, &val) in adj.iter_mut().zip(&c.adj) {
                    a.push(k(tv, val, tau));
                }
            }
            let last = next.is_none();

            // Velum for nasals (anticipatory lowering, carry-over raising).
            if p.manner == Manner::Nasal {
                let open = match p.place {
                    Place::Velar => 1.0,
                    Place::Uvular => 0.75,
                    _ => 0.9,
                };
                if !prev.map(|pv| pv.seg.phone.manner == Manner::Nasal).unwrap_or(false) {
                    keys[T::Velum as usize].push(k(t0 - 0.05, open, 0.025));
                }
                if !next.map(|n| n.seg.phone.manner == Manner::Nasal || n.seg.nasalized).unwrap_or(false) {
                    // Oral obstruents need the velopharyngeal port sealed by their
                    // onset (pressure build-up); vowels tolerate carry-over nasality.
                    if next.map(|n| n.seg.phone.is_obstruent()).unwrap_or(false) {
                        keys[T::Velum as usize].push(k(end - 0.02, 0.0, 0.01));
                    } else {
                        keys[T::Velum as usize].push(k(end - 0.005, 0.0, 0.03));
                    }
                }
            }

            // Laryngeal gestures.
            let voiceless = !p.voiced || seg.voiceless;
            let g = &mut keys[T::Glottis as usize];
            let before = g.len();
            let after_voiceless = prev.map(|pv| !pv.seg.phone.voiced || pv.seg.voiceless).unwrap_or(false);
            match (p.manner, p.place) {
                (Manner::Stop, Place::Glottal) => {
                    g.push(k(t0 - 0.01, -1.0, 0.006));
                    g.push(k(end - 0.005, voiced_g, 0.01));
                }
                (_, Place::Glottal) => {
                    let a = if p.voiced { 0.07 } else { 0.2 };
                    g.push(k(t0 - 0.015, a, 0.015));
                    g.push(k(end - 0.01, voiced_g, 0.015));
                }
                (Manner::Stop, _) if voiceless => {
                    g.push(k(t0 + 0.005, 0.3, 0.012));
                    let back = if ts.aspirated { end + 0.028 } else { end - 0.02 };
                    g.push(k(back, voiced_g, 0.012));
                }
                (Manner::Fricative | Manner::Affricate, _) if voiceless => {
                    g.push(k(t0 - 0.015, 0.24, 0.01));
                    g.push(k(end - 0.012, voiced_g, 0.012));
                }
                _ if voiceless => {
                    // Voiceless sonorants (ʍ etc.).
                    g.push(k(t0 - 0.01, 0.15, 0.012));
                    g.push(k(end - 0.01, voiced_g, 0.012));
                }
                // Phrase-initial voiced obstruents are not prevoiced in English
                // (short-lag VOT): voicing starts around the release.
                (Manner::Stop | Manner::Fricative | Manner::Affricate, _) if prev.is_none() && !ts.sung => {
                    g.push(k(t0 - 0.01, 0.12, 0.012));
                    let on = if p.manner == Manner::Stop { end - 0.025 } else { t0 + 0.4 * ts.dur };
                    g.push(k(on, voiced_g.max(if p.manner == Manner::Stop { 0.0 } else { 0.045 }), 0.012));
                    if p.manner != Manner::Stop {
                        g.push(k(end - 0.01, voiced_g, 0.012));
                    }
                }
                // Voiced fricatives: slight abduction keeps enough airflow for frication.
                (Manner::Fricative | Manner::Affricate, _) => {
                    g.push(k(t0 - 0.01, voiced_g.max(0.045), 0.012));
                    g.push(k(end - 0.01, voiced_g, 0.012));
                }
                // Voiced sonorants after a voiceless segment let its glottal
                // return (e.g. aspiration) run its course.
                _ if after_voiceless => {}
                _ => g.push(k(t0 - 0.01, voiced_g, 0.012)),
            }
            // This segment's laryngeal gesture supersedes keys that the previous
            // segment scheduled for later (e.g. its return to voicing).
            if let Some(first) = g[before..].iter().map(|key| key.t).reduce(f64::min) {
                let mut idx = 0;
                g.retain(|key| {
                    idx += 1;
                    idx > before || key.t < first
                });
            }
            if last {
                // Phrase-final consonant: the glottis opens as the breath group
                // ends, so the release is breathy instead of a voiced schwa.
                g.retain(|key| key.t < end - 0.03 || key.t < t0);
                g.push(k(end - 0.02, 0.3, 0.02));
            }
        }

        Score { keys, adj }
    }

    /// Integrate all tiers into control frames.
    pub fn frames(&self, tl: &Timeline, voice: &Voice) -> Vec<ControlFrame> {
        let schwa = VowelShape::SCHWA;
        let init = |t: T| -> f64 {
            match t {
                T::VPos => schwa.tongue_pos,
                T::VRad => schwa.tongue_radius,
                T::VLen => schwa.tongue_length,
                T::VPhar => schwa.pharynx,
                T::VLipA => la(schwa.lip_area),
                T::VLipL => schwa.lip_length,
                T::CLip | T::CTipA | T::CDorA | T::CRoot | T::CRoundA => la(OPEN),
                T::CTipPos => 0.9,
                T::CDorPos => 0.68,
                T::CRoundL => 0.0,
                T::Obst | T::Velum => 0.0,
                T::Glottis => 0.45,
                T::Count => 0.0,
            }
        };
        let all = [
            T::VPos,
            T::VRad,
            T::VLen,
            T::VPhar,
            T::VLipA,
            T::VLipL,
            T::CLip,
            T::CTipPos,
            T::CTipA,
            T::CDorPos,
            T::CDorA,
            T::CRoot,
            T::CRoundA,
            T::CRoundL,
            T::Obst,
            T::Velum,
            T::Glottis,
        ];
        let mut tiers: Vec<Tier> = all
            .iter()
            .map(|&t| {
                let mut ks = self.keys[t as usize].clone();
                if t == T::Glottis {
                    ks.extend(tl.glottis.iter().copied());
                }
                Tier::new(init(t), ks)
            })
            .collect();
        let mut adj_tiers: Vec<Tier> = self.adj.iter().map(|ks| Tier::new(0.0, ks.clone())).collect();
        let log_keys = |v: &[Key]| -> Vec<Key> { v.iter().map(|x| Key { v: x.v.max(1.0).ln(), ..*x }).collect() };
        let mut f0 = Tier::new(voice.f0.ln(), log_keys(&tl.f0));
        let mut lung = Tier::new(0.0, tl.lung.clone());
        let mut rd = Tier::new(voice.rd, tl.rd.clone());
        let mut breath = Tier::new(0.0, tl.breath.clone());
        let mut creak = Tier::new(0.0, tl.creak.clone());
        let mut perturb = Tier::new(1.0, tl.perturbation.clone());
        let mut tremor = Tier::new(0.0, tl.tremor.clone());
        let mut vib = Tier::new(0.0, tl.vibrato.clone());
        let mut vib_rate = Tier::new(5.5, tl.vibrato_rate.clone());
        let mut larynx = Tier::new(voice.anatomy.larynx_area, tl.larynx.clone());

        let dt = 1.0 / CONTROL_RATE;
        let n = (tl.end * CONTROL_RATE).ceil() as usize;
        let mut out = Vec::with_capacity(n);
        let mut vib_phase = 0.0f64;
        let mut v = [0.0f64; NT];
        for fi in 0..n {
            let t = fi as f64 * dt;
            for (ti, tier) in tiers.iter_mut().enumerate() {
                v[all[ti] as usize] = tier.advance(t, dt);
            }
            let rate = vib_rate.advance(t, dt);
            let depth = vib.advance(t, dt);
            vib_phase += 2.0 * std::f64::consts::PI * rate * dt;
            let trem = tremor.advance(t, dt);
            let trem_mod = (2.0 * std::f64::consts::PI * 5.3 * t).sin() + 0.4 * (2.0 * std::f64::consts::PI * 3.1 * t).sin();
            let semis = depth * vib_phase.sin() + trem * 0.45 * trem_mod;
            let f0_hz = f0.advance(t, dt).exp() * (semis / 12.0).exp2();
            let lung_pa = lung.advance(t, dt).max(0.0) * (1.0 + 0.1 * trem * trem_mod) * (1.0 + 0.04 * depth * vib_phase.sin());

            let lip_c = ila(v[T::CLip as usize]).min(ila(v[T::CRoundA as usize]));
            let mut adj = [0.0; ADJ_N];
            for (a, tier) in adj.iter_mut().zip(adj_tiers.iter_mut()) {
                *a = tier.advance(t, dt);
            }
            let art = Articulators {
                tongue_pos: v[T::VPos as usize],
                tongue_radius: v[T::VRad as usize],
                tongue_length: v[T::VLen as usize],
                pharynx: v[T::VPhar as usize],
                lip_area: ila(v[T::VLipA as usize]).min(lip_c),
                lip_length: v[T::VLipL as usize].max(v[T::CRoundL as usize]),
                tip_pos: v[T::CTipPos as usize],
                tip_area: ila(v[T::CTipA as usize]),
                dorsum_pos: v[T::CDorPos as usize],
                dorsum_area: ila(v[T::CDorA as usize]),
                root_area: ila(v[T::CRoot as usize]),
                velum: v[T::Velum as usize].max(0.0),
                glottis: v[T::Glottis as usize],
                lung: lung_pa,
                obstacle: v[T::Obst as usize].clamp(0.0, 1.0),
                larynx: larynx.advance(t, dt),
                adj,
            };
            out.push(ControlFrame {
                art,
                f0: f0_hz,
                rd: rd.advance(t, dt),
                breath: breath.advance(t, dt).clamp(0.0, 1.0),
                creak: creak.advance(t, dt).clamp(0.0, 1.0),
                perturbation: perturb.advance(t, dt).max(0.0),
            });
        }
        out
    }
}

/// Emit the consonantal constriction gestures of `ts` between `on` and `off`.
#[allow(clippy::too_many_arguments)]
fn emit_constrictions(keys: &mut [Vec<Key>], ts: &TimedSeg, next: Option<&TimedSeg>, segs: &[TimedSeg], i: usize, prec: f64, tip_free_at: &mut f64, on: f64, off: f64) {
    let p = ts.seg.phone;
    let dur = off - on;
    for g in p.gestures {
        let (area, tier, tau) = match *g {
            Gesture::Lips(a) => (a, T::CLip, 0.011),
            Gesture::Tip(_, a) => (a, T::CTipA, 0.009),
            Gesture::Dorsum(_, a) => (a, T::CDorA, 0.013),
            Gesture::Root(a) => (a, T::CRoot, 0.02),
            Gesture::Round(a, _) => (a, T::CRoundA, 0.02),
        };
        let approx = matches!(p.manner, Manner::Approximant | Manner::Lateral | Manner::Vowel);
        let tau = tau / prec * if approx { 1.1 } else { 1.0 };
        let target = if area <= 0.0 { CLOSURE } else { area };
        let lead = if area <= 0.0 { 2.0 * tau } else { 1.5 * tau };
        let same_next = next.map(|n| seg_uses(&n.seg, tier)).unwrap_or(false);
        let same_prev = i > 0 && seg_uses(&segs[i - 1].seg, tier) && on - segs[i - 1].end() < 0.05;
        let initial = i == 0 || segs[i].t0 - segs[i - 1].end() >= 0.05;
        let t_on = if same_prev {
            on - 0.3 * lead
        } else if initial && !ts.seg.phone.is_vowel() {
            // Phrase-initial: the constriction is in place before the breath starts.
            (on - lead).min(on - 0.1)
        } else {
            on - lead
        };
        let release = off - tau;
        let tk = &mut keys[tier as usize];
        match p.manner {
            Manner::Tap => {
                tk.push(k(on - 0.006, la(CLOSURE), 0.004));
                tk.push(k(on + 0.5 * dur, la(OPEN), 0.005));
                continue;
            }
            Manner::Trill => {
                let taps = ((dur / 0.028).round() as usize).clamp(2, 4);
                for n in 0..taps {
                    let tt = on + n as f64 * dur / taps as f64;
                    tk.push(k(tt - 0.004, la(CLOSURE), 0.004));
                    tk.push(k(tt + 0.4 * dur / taps as f64, la(0.3), 0.004));
                }
                tk.push(k(off, la(OPEN), 0.006));
                continue;
            }
            _ => {}
        }
        tk.push(k(t_on, la(target), tau));
        if p.manner == Manner::Affricate {
            let fric = match p.place {
                Place::Alveolar => 0.06,
                Place::Postalveolar => 0.1,
                _ => 0.09,
            };
            tk.push(k(on + 0.5 * dur, la(fric), 0.008));
        }
        if !same_next {
            tk.push(k(release, la(OPEN), tau * if p.manner == Manner::Stop { 0.8 } else { 1.0 }));
        }
        // Places / auxiliary parameters.
        match *g {
            Gesture::Tip(pos, _) => {
                let snap = on - *tip_free_at > 0.03 || *tip_free_at < 0.0;
                keys[T::CTipPos as usize].push(k(t_on - 0.002, pos, if snap { 0.002 } else { 0.015 }));
                if !same_next {
                    *tip_free_at = release;
                }
            }
            Gesture::Dorsum(pos, _) => {
                let pos = if pos == VELAR_AUTO { velar_place(segs, i) } else { pos };
                keys[T::CDorPos as usize].push(k(t_on - 0.002, pos, 0.01));
            }
            Gesture::Round(_, len) => {
                keys[T::CRoundL as usize].push(k(t_on, len, tau));
                if !same_next {
                    keys[T::CRoundL as usize].push(k(release, 0.0, tau));
                }
            }
            _ => {}
        }
    }
    if p.sibilant {
        let t_obs = if p.manner == Manner::Affricate { on + 0.45 * dur } else { on - 0.01 };
        keys[T::Obst as usize].push(k(t_obs, 1.0, 0.004));
        keys[T::Obst as usize].push(k(off, 0.0, 0.01));
    }
}

/// Velar place coarticulates with the adjacent vowel (fronted before /i/,
/// backed before /a/).
fn velar_place(segs: &[TimedSeg], i: usize) -> f64 {
    let near = segs[i + 1..]
        .iter()
        .take(3)
        .find(|s| s.seg.is_vowel())
        .or_else(|| segs[..i].iter().rev().take(3).find(|s| s.seg.is_vowel()));
    let tp = near.and_then(|s| s.seg.phone.shape()).map(|v| v.tongue_pos).unwrap_or(0.5);
    (0.68 + 0.3 * (tp - 0.5)).clamp(0.6, 0.78)
}

/// Convenience: timeline → control frames.
pub fn render_frames(tl: &Timeline, voice: &Voice) -> Vec<ControlFrame> {
    Score::build(tl).frames(tl, voice)
}
