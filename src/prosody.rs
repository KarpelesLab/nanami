//! Prosody planning: segment durations, F0 contour, respiration and voice
//! quality over time, for speech (English stress-timed, Japanese mora-timed)
//! and singing (note-aligned).
//!
//! Speech durations follow Klatt's (1979) multiplicative rules on intrinsic
//! durations. Intonation uses a sparse target model in the autosegmental-
//! metrical spirit (Pierrehumbert 1980): H* pitch accents on stressed syllables
//! of content words with downstep, plus boundary tones (L-L% statements,
//! H-H% yes/no questions, L-H% continuation), smoothed later by the
//! articulator-like F0 dynamics. Japanese uses lexical pitch accent with a
//! default heiban (LHH…) pattern and ꜜ for the accent kernel.

use crate::ipa::{Lang, PitchMark, Seg};
use crate::phoneme::{self, Manner, Place};
use crate::utterance::*;
use crate::voice::Voice;

/// A target value for a control trajectory, taking effect at `t` and
/// approached with time constant `tau` (critically damped).
#[derive(Clone, Copy, Debug)]
pub struct Key {
    pub t: f64,
    pub v: f64,
    pub tau: f64,
}

#[derive(Clone, Debug)]
pub struct TimedSeg {
    pub seg: Seg,
    pub t0: f64,
    pub dur: f64,
    pub style: Style,
    /// Voiceless stop released with aspiration (long VOT).
    pub aspirated: bool,
    pub word_initial: bool,
    pub sung: bool,
}

impl TimedSeg {
    pub fn end(&self) -> f64 {
        self.t0 + self.dur
    }
}

#[derive(Default, Debug)]
pub struct Timeline {
    pub segs: Vec<TimedSeg>,
    /// F0 in Hz.
    pub f0: Vec<Key>,
    /// Subglottal pressure in Pa.
    pub lung: Vec<Key>,
    pub rd: Vec<Key>,
    pub breath: Vec<Key>,
    pub creak: Vec<Key>,
    pub perturbation: Vec<Key>,
    pub tremor: Vec<Key>,
    /// Vibrato extent (± semitones) and rate (Hz).
    pub vibrato: Vec<Key>,
    pub vibrato_rate: Vec<Key>,
    /// Extra glottal abduction targets (breathing in pauses).
    pub glottis: Vec<Key>,
    /// Epilarynx area (cm²) for singing technique.
    pub larynx: Vec<Key>,
    pub end: f64,
    pub warnings: Vec<String>,
}

fn key(t: f64, v: f64, tau: f64) -> Key {
    Key { t, v, tau }
}

fn st(hz: f64) -> f64 {
    12.0 * hz.log2()
}
fn hz(st: f64) -> f64 {
    (st / 12.0).exp2()
}

const WH_WORDS: &[&str] = &["what", "where", "when", "why", "who", "whom", "whose", "which", "how"];

pub struct Planner<'a> {
    pub voice: &'a Voice,
    pub tl: Timeline,
    t: f64,
}

impl<'a> Planner<'a> {
    pub fn new(voice: &'a Voice) -> Self {
        let mut tl = Timeline::default();
        tl.lung.push(key(0.0, 0.0, 0.03));
        tl.glottis.push(key(0.0, 0.45, 0.05));
        tl.larynx.push(key(0.0, voice.anatomy.larynx_area, 0.1));
        tl.vibrato.push(key(0.0, 0.0, 0.1));
        tl.vibrato_rate.push(key(0.0, 5.5, 0.1));
        tl.f0.push(key(0.0, voice.f0, 0.05));
        Planner { voice, tl, t: 0.12 }
    }

    pub fn plan(mut self, blocks: &[Block]) -> Timeline {
        for (i, b) in blocks.iter().enumerate() {
            match b {
                Block::Speech(p) => {
                    let joined = matches!(blocks.get(i + 1), Some(Block::Speech(_))) && p.boundary == Boundary::None;
                    self.speak(p, joined);
                }
                Block::Pause(d) => self.pause(*d),
                Block::Song(s) => self.sing(s),
            }
        }
        self.breathe_out();
        self.tl.end = self.t + 0.25;
        self.tl
    }

    fn pause(&mut self, d: f64) {
        self.breathe_out();
        self.t += d;
    }

    /// End of a breath group: release lung pressure, open the glottis.
    fn breathe_out(&mut self) {
        let t = self.t;
        self.tl.lung.push(key(t, 0.0, 0.035));
        // Open for breathing once the pressure has mostly decayed.
        self.tl.glottis.push(key(t + 0.12, 0.45, 0.05));
        self.tl.vibrato.push(key(t, 0.0, 0.05));
    }

    // ------------------------------------------------------------------
    // Speech
    // ------------------------------------------------------------------

    fn speak(&mut self, phrase: &Phrase, joined: bool) {
        if phrase.words.is_empty() {
            return;
        }
        // Flatten words into segments with bookkeeping.
        struct S {
            seg: Seg,
            word: usize,
            word_initial: bool,
        }
        let mut flat: Vec<S> = Vec::new();
        for (wi, w) in phrase.words.iter().enumerate() {
            let segs = match phrase.lang {
                Lang::English => english_allophones(&w.segs),
                _ => w.segs.clone(),
            };
            for (k, seg) in segs.into_iter().enumerate() {
                flat.push(S { seg, word: wi, word_initial: k == 0 });
            }
        }
        if flat.is_empty() {
            return;
        }
        if phrase.lang == Lang::Japanese {
            japanese_devoicing(&mut flat.iter_mut().map(|s| &mut s.seg).collect::<Vec<_>>());
        }
        // Syllable index per segment: a syllable = onset consonants + nucleus + following
        // consonants up to the next onset (maximal onset approximated: one consonant
        // before a nucleus goes to the onset, clusters split before their last two).
        let n = flat.len();
        let nuclei: Vec<usize> = (0..n).filter(|&i| flat[i].seg.is_nucleus()).collect();
        let seg_refs: Vec<&Seg> = flat.iter().map(|s| &s.seg).collect();
        let mut syl = vec![0usize; n];
        for (k, &nuc) in nuclei.iter().enumerate() {
            let from = onset_start(&seg_refs, nuc, k.checked_sub(1).map(|j| nuclei[j]));
            syl.iter_mut().skip(from).for_each(|s| *s = k);
        }
        let last_syl = syl[n - 1];
        let words_syllables: Vec<usize> = {
            let mut v = vec![0; phrase.words.len()];
            for &ni in &nuclei {
                v[flat[ni].word] += 1;
            }
            v
        };
        let syl_stress: Vec<u8> = nuclei.iter().map(|&i| flat[i].seg.stress).collect();

        // --- Durations ---
        let mut durs = vec![0.0f64; n];
        let mora = 0.125;
        for i in 0..n {
            let s = &flat[i];
            let w = &phrase.words[s.word];
            let p = s.seg.phone;
            // Klatt's inherent durations are for careful citation speech;
            // conversational tempo is ~15% faster.
            let rate = (w.style.rate * self.voice.rate * 1.15).max(0.2);
            let final_syl = syl[i] == last_syl;
            let d = if phrase.lang == Lang::Japanese {
                japanese_duration(&s.seg, final_syl, mora)
            } else {
                let stressed = syl_stress.get(syl[i]).copied().unwrap_or(0) > 0;
                let mut pr = 1.0;
                if p.is_vowel() || s.seg.syllabic {
                    if s.seg.offglide {
                        pr *= 0.45;
                    }
                    if !stressed {
                        pr *= 0.55;
                    }
                    pr *= if final_syl { 1.35 } else { 0.62 };
                    if words_syllables[s.word] > 1 {
                        pr *= 0.85;
                    }
                    // Voicing of the following obstruent in the same word.
                    if let Some(nx) = flat.get(i + 1).filter(|nx| nx.word == s.word)
                        && nx.seg.phone.is_obstruent() && !nx.seg.offglide {
                            pr *= if nx.seg.phone.voiced { 1.15 } else { 0.82 };
                        }
                    if is_diphthong_onset(&seg_refs, i) {
                        pr *= 0.8;
                    }
                } else {
                    if !s.word_initial {
                        pr *= 0.85;
                    }
                    let prev_c = i > 0 && !flat[i - 1].seg.phone.is_vowel();
                    let next_c = i + 1 < n && !flat[i + 1].seg.phone.is_vowel();
                    if prev_c || next_c {
                        pr *= 0.8;
                    }
                    if !stressed {
                        pr *= 0.8;
                    }
                    if final_syl {
                        pr *= 1.2;
                    }
                }
                pr *= 1.0 + 0.4 * w.emphasis;
                if s.seg.long {
                    pr *= if p.is_vowel() { 1.6 } else { 1.8 };
                }
                (p.min_dur + (p.dur - p.min_dur) * pr) / 1000.0
            };
            durs[i] = d / rate;
        }

        // --- Timing ---
        let start = self.t;
        let mut t = start + 0.05;
        let mut times = vec![0.0; n];
        for i in 0..n {
            times[i] = t;
            t += durs[i];
        }
        let end = t;

        // Aspiration (English): voiceless stops in stressed-syllable or word-initial
        // onsets, not after /s/.
        for i in 0..n {
            let s = &flat[i];
            let p = s.seg.phone;
            let mut asp = s.seg.aspirated;
            if p.manner == Manner::Stop && !p.voiced && p.place != Place::Glottal {
                let after_s = i > 0 && flat[i - 1].seg.phone.sibilant && flat[i - 1].word == s.word;
                let before_vowel = flat.get(i + 1).map(|x| x.seg.phone.is_sonorant()).unwrap_or(false);
                let stressed_onset = syl_stress.get(syl[i]).copied().unwrap_or(0) > 0;
                asp |= phrase.lang == Lang::English && !after_s && before_vowel && (stressed_onset || s.word_initial);
            }
            self.tl.segs.push(TimedSeg {
                seg: s.seg.clone(),
                t0: times[i],
                dur: durs[i],
                style: phrase.words[s.word].style.clone(),
                aspirated: asp,
                word_initial: s.word_initial,
                sung: false,
            });
        }

        // --- Intonation ---
        let style0 = &phrase.words[0].style;
        let base_of = |w: &Word| st(self.voice.f0) + w.style.pitch;
        let range_of = |w: &Word| self.voice.range_st * w.style.range;
        let decl = |tt: f64| -0.9 * (tt - start).min(3.0);
        let wh = phrase.words.first().map(|w| WH_WORDS.contains(&w.text.to_ascii_lowercase().as_str())).unwrap_or(false);
        let question = phrase.boundary == Boundary::Question && !wh;
        let f0_tau = 0.045;
        {
            let w0 = &phrase.words[0];
            self.tl.f0.push(key(start, hz(base_of(w0) + 0.3 * range_of(w0)), 0.02));
        }
        if phrase.lang == Lang::Japanese {
            self.japanese_pitch(phrase, &flat.iter().map(|s| (s.word, &s.seg)).collect::<Vec<_>>(), &times, &durs, start, end, question);
        } else {
            // Accented nuclei: primary stress of content words (or emphasized words).
            let mut accents: Vec<(usize, f64)> = Vec::new(); // (seg index, emphasis)
            for (wi, w) in phrase.words.iter().enumerate() {
                if w.function && w.emphasis <= 0.0 {
                    continue;
                }
                let idx: Vec<usize> = nuclei.iter().copied().filter(|&i| flat[i].word == wi).collect();
                let pick = idx.iter().copied().find(|&i| flat[i].seg.stress == 1).or_else(|| if idx.len() == 1 { Some(idx[0]) } else { idx.first().copied() });
                if let Some(i) = pick {
                    accents.push((i, w.emphasis));
                }
            }
            let nacc = accents.len();
            let mut prev_peak: Option<(f64, f64)> = None; // (time, semitones above base)
            for (k, &(i, emph)) in accents.iter().enumerate() {
                let w = &phrase.words[flat[i].word];
                let (b, r) = (base_of(w), range_of(w));
                let nuclear = k + 1 == nacc;
                let down = 0.82f64.powi(k as i32);
                let t_on = times[i];
                let t_pk = times[i] + 0.6 * durs[i];
                // Between accents F0 sags only partway toward the baseline
                // ("sagging transition", Pierrehumbert 1980), and only when the
                // accents are far enough apart; close accents form a plateau.
                if let Some((tp, hp)) = prev_peak {
                    let gap = t_on - tp;
                    if gap > 0.22 {
                        let sag = (0.25 + 0.5 * ((gap - 0.22) / 0.6).min(1.0)).min(0.75);
                        let tv = tp + 0.55 * gap;
                        self.tl.f0.push(key(tv, hz(b + decl(tv) + hp * (1.0 - sag) + 0.1 * r * sag), 0.07));
                    }
                }
                if nuclear && question {
                    // L* H-H%: low accent, high rise to the end.
                    self.tl.f0.push(key(t_on, hz(b + decl(t_on) - 0.2 * r), f0_tau));
                } else {
                    let h = r * down * (1.0 + 0.6 * emph) * if nuclear && phrase.boundary == Boundary::Exclaim { 1.3 } else { 1.0 };
                    self.tl.f0.push(key(t_pk - 0.05, hz(b + decl(t_pk) + h), f0_tau));
                    prev_peak = Some((t_pk, h));
                }
                // Explicit pitch marks on the nucleus.
            }
            // IPA pitch marks.
            for i in 0..n {
                if let Some(pm) = flat[i].seg.pitch {
                    let w = &phrase.words[flat[i].word];
                    let (b, r) = (base_of(w) + decl(times[i]), range_of(w));
                    let (t0, t1) = (times[i], times[i] + durs[i]);
                    match pm {
                        PitchMark::Rise => {
                            self.tl.f0.push(key(t0, hz(b), f0_tau));
                            self.tl.f0.push(key(t0 + 0.5 * (t1 - t0), hz(b + r), f0_tau));
                        }
                        PitchMark::Fall => {
                            self.tl.f0.push(key(t0, hz(b + r), f0_tau));
                            self.tl.f0.push(key(t0 + 0.5 * (t1 - t0), hz(b - 1.0), f0_tau));
                        }
                        PitchMark::Up => self.tl.f0.push(key(t0, hz(b + r), f0_tau)),
                        PitchMark::Down => self.tl.f0.push(key(t1 - 0.02, hz(b), f0_tau)),
                    }
                }
            }
            // Boundary tones.
            let wl = &phrase.words[phrase.words.len() - 1];
            let (b, r) = (base_of(wl) + decl(end), range_of(wl));
            let last_nuc = nuclei.last().copied().unwrap_or(n - 1);
            let t_ln = times[last_nuc] + 0.5 * durs[last_nuc];
            match phrase.boundary {
                Boundary::Period | Boundary::Exclaim => {
                    self.tl.f0.push(key(t_ln.max(end - 0.18), hz(b - 1.0), 0.06));
                    self.tl.f0.push(key(end - 0.06, hz(b - 2.8), 0.07));
                }
                Boundary::Question if question => {
                    self.tl.f0.push(key(t_ln, hz(b + 0.3 * r), 0.06));
                    self.tl.f0.push(key(end - 0.08, hz(b + 1.7 * r), 0.06));
                }
                Boundary::Question => {
                    self.tl.f0.push(key(t_ln.max(end - 0.18), hz(b - 0.8), 0.06));
                    self.tl.f0.push(key(end - 0.06, hz(b - 2.2), 0.07));
                }
                Boundary::Comma => {
                    self.tl.f0.push(key(t_ln, hz(b), 0.05));
                    self.tl.f0.push(key(end - 0.08, hz(b + 0.55 * r), 0.06));
                }
                Boundary::None => {
                    self.tl.f0.push(key(end - 0.05, hz(b + 0.25 * r), 0.06));
                }
            }
        }

        // --- Respiration & voice quality ---
        let vol0 = style0.volume;
        // Adduct first, then build pressure (a soft, non-breathy onset). A new
        // breath group cancels breathing gestures still pending from the pause.
        let onset = start - 0.09;
        self.tl.glottis.retain(|k| k.t < onset);
        // A voiceless first segment starts from an open glottis, so no voicing
        // (an audible schwa) precedes it while lung pressure builds.
        let first_voiceless = !flat[0].seg.phone.voiced || flat[0].seg.voiceless;
        let first_obstruent = flat[0].seg.phone.is_obstruent();
        let g_on = if first_voiceless {
            0.25
        } else if style0.whisper {
            0.2
        } else if first_obstruent {
            0.12
        } else {
            0.0
        };
        self.tl.glottis.push(key(onset, g_on, 0.02));
        self.tl.lung.push(key(start - 0.05, self.voice.lung * vol0, 0.03));
        for (wi, w) in phrase.words.iter().enumerate() {
            let Some(first) = flat.iter().position(|s| s.word == wi) else { continue };
            let t_w = times[first];
            let mut lung = self.voice.lung * w.style.volume * (1.0 + 0.3 * w.emphasis);
            if w.style.whisper {
                lung *= 0.7;
            }
            self.tl.lung.push(key(t_w - 0.02, lung, 0.04));
            let rd = (self.voice.rd - 0.45 * w.style.tension + 0.6 * w.style.breath).clamp(0.3, 2.7);
            self.tl.rd.push(key(t_w - 0.02, rd, 0.05));
            self.tl.breath.push(key(t_w - 0.02, w.style.breath, 0.05));
            self.tl.creak.push(key(t_w - 0.02, w.style.creak, 0.05));
            self.tl.perturbation.push(key(t_w - 0.02, w.style.perturbation, 0.05));
            self.tl.tremor.push(key(t_w - 0.02, w.style.tremor, 0.1));
        }
        // Stressed syllables get a little more respiratory effort.
        for &i in &nuclei {
            let w = &phrase.words[flat[i].word];
            if flat[i].seg.stress == 1 && !w.function {
                let lung = self.voice.lung * w.style.volume * (1.08 + 0.3 * w.emphasis);
                self.tl.lung.push(key(times[i] - 0.03, lung, 0.04));
                self.tl.lung.push(key(times[i] + durs[i], self.voice.lung * w.style.volume, 0.05));
            }
        }
        // Utterance-final creak and slight laxing for falling boundaries.
        if matches!(phrase.boundary, Boundary::Period) {
            let wl = &phrase.words[phrase.words.len() - 1];
            let fry = (wl.style.creak + if self.voice.f0 < 140.0 { 0.15 } else { 0.0 }).min(1.0);
            if fry > 0.0 {
                self.tl.creak.push(key(end - 0.12, fry, 0.04));
                self.tl.creak.push(key(end + 0.05, wl.style.creak, 0.05));
            }
            self.tl.lung.push(key(end - 0.1, self.voice.lung * wl.style.volume * 0.8, 0.06));
        }

        self.t = end;
        if !joined {
            self.breathe_out();
            let rate = style0.rate.max(0.2);
            self.t += match phrase.boundary {
                Boundary::Comma => 0.2,
                Boundary::Period | Boundary::Exclaim => 0.45,
                Boundary::Question => 0.5,
                Boundary::None => 0.06,
            } / rate;
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn japanese_pitch(&mut self, phrase: &Phrase, flat: &[(usize, &Seg)], times: &[f64], durs: &[f64], start: f64, end: f64, question: bool) {
        let tau = 0.04;
        let decl = |tt: f64| -(tt - start).min(3.0);
        for (wi, w) in phrase.words.iter().enumerate() {
            let b = st(self.voice.f0) + w.style.pitch;
            let r = self.voice.range_st * w.style.range * (1.0 + 0.5 * w.emphasis);
            // Morae = vowels (incl. long), moraic nasals, geminates; we key on vowels/ɴ.
            let idx: Vec<usize> = (0..flat.len()).filter(|&i| flat[i].0 == wi && (flat[i].1.is_vowel() || flat[i].1.phone.ipa == "ɴ")).collect();
            if idx.is_empty() {
                continue;
            }
            let kernel = idx.iter().position(|&i| flat[i].1.pitch == Some(PitchMark::Down));
            for (m, &i) in idx.iter().enumerate() {
                let high = match kernel {
                    Some(0) => m == 0,
                    Some(k) => m >= 1 && m <= k,
                    None => m >= 1 || idx.len() == 1,
                };
                let v = b + decl(times[i]) + if high { 0.85 * r } else { 0.1 * r };
                self.tl.f0.push(key(times[i] + 0.3 * durs[i], hz(v), tau));
            }
        }
        let wl = &phrase.words[phrase.words.len() - 1];
        let (b, r) = (st(self.voice.f0) + wl.style.pitch + decl(end), self.voice.range_st * wl.style.range);
        if question {
            self.tl.f0.push(key(end - 0.12, hz(b + 0.2 * r), 0.05));
            self.tl.f0.push(key(end - 0.04, hz(b + 1.6 * r), 0.05));
        } else if matches!(phrase.boundary, Boundary::Period | Boundary::Exclaim) {
            self.tl.f0.push(key(end - 0.1, hz(b - 2.0), 0.06));
        }
    }

    // ------------------------------------------------------------------
    // Singing
    // ------------------------------------------------------------------

    fn sing(&mut self, song: &Song) {
        let beat = 60.0 / song.sing.tempo.max(10.0);
        let v = self.voice;
        // Absolute note start times.
        struct Ev<'n> {
            note: Option<&'n Note>,
            t0: f64,
            dur: f64,
        }
        let mut evs: Vec<Ev> = Vec::new();
        let lead = 0.25; // room for the first onset consonants and a breath
        let mut t = self.t + lead;
        for item in &song.items {
            match item {
                SungItem::Note(n) => {
                    let d: f64 = n.pitches.iter().map(|p| p.1).sum::<f64>() * beat;
                    evs.push(Ev { note: Some(n), t0: t, dur: d });
                    t += d;
                }
                SungItem::Rest(b) => {
                    evs.push(Ev { note: None, t0: t, dur: b * beat });
                    t += b * beat;
                }
            }
        }
        let song_end = t;
        let style = &song.style;
        let rd = (v.rd - 0.2 - 0.45 * style.tension + 0.6 * style.breath).clamp(0.3, 2.7);
        self.tl.rd.push(key(self.t, rd, 0.05));
        self.tl.breath.push(key(self.t, style.breath, 0.05));
        self.tl.creak.push(key(self.t, style.creak * 0.5, 0.05));
        self.tl.perturbation.push(key(self.t, style.perturbation * 0.7, 0.05));
        self.tl.tremor.push(key(self.t, style.tremor, 0.1));
        self.tl.vibrato_rate.push(key(self.t, song.sing.vibrato_rate, 0.1));
        let larynx = if song.sing.classical { 0.45 } else { v.anatomy.larynx_area };
        self.tl.larynx.push(key(self.t, larynx, 0.1));
        let lung_sing = v.lung * 1.2 * style.volume;
        let rate = 1.0 / style.rate.max(0.2);

        // Split each syllable into onset / nucleus / coda.
        let split = |segs: &[Seg]| -> (usize, usize) {
            let first = segs.iter().position(|s| s.is_nucleus()).unwrap_or(segs.len());
            let last = segs.iter().rposition(|s| s.is_vowel() || s.syllabic).map(|i| i + 1).unwrap_or(first);
            (first, last.max(first))
        };
        let cons_dur = |s: &Seg| -> f64 {
            let p = s.phone;
            let base = if p.manner == Manner::Tap { 0.03 } else { (p.dur * 0.8 / 1000.0).clamp(0.04, 0.11) };
            base * rate * if s.long { 1.6 } else { 1.0 }
        };
        let mut prev_breathed = true;
        for k in 0..evs.len() {
            let ev = &evs[k];
            let Some(note) = ev.note else {
                if ev.dur >= 0.2 {
                    // Breathe during rests.
                    self.tl.lung.push(key(ev.t0 + 0.02, 0.0, 0.04));
                    self.tl.glottis.push(key(ev.t0 + 0.12, 0.45, 0.05));
                    self.tl.vibrato.push(key(ev.t0, 0.0, 0.05));
                    prev_breathed = true;
                }
                continue;
            };
            let segs = &note.segs;
            let (on, nuc_end) = split(segs);
            let onset: f64 = segs[..on].iter().map(cons_dur).sum();
            let coda: f64 = segs[nuc_end..].iter().map(cons_dur).sum();
            // Next note's onset eats into the end of this one.
            let next_onset = match evs.get(k + 1).and_then(|e| e.note) {
                Some(nn) => {
                    let (non, _) = split(&nn.segs);
                    nn.segs[..non].iter().map(cons_dur).sum::<f64>()
                }
                None => 0.0,
            };
            let t_on = ev.t0 - onset;
            let vowel_end = (ev.t0 + ev.dur - next_onset - coda).max(ev.t0 + 0.05);
            if prev_breathed {
                self.tl.glottis.retain(|k| k.t < t_on - 0.1);
                self.tl.glottis.push(key(t_on - 0.1, 0.0, 0.02));
                self.tl.lung.push(key(t_on - 0.07, lung_sing, 0.03));
                prev_breathed = false;
            }
            // Place segments.
            let mut tt = t_on;
            for s in &segs[..on] {
                let d = cons_dur(s);
                self.push_sung(s, tt, d, style);
                tt += d;
            }
            let nucleus = &segs[on..nuc_end];
            let vowel_total = vowel_end - ev.t0;
            if !nucleus.is_empty() {
                // Diphthong offglides get a short slot at the end of the note.
                let glides = nucleus.iter().filter(|s| s.offglide).count();
                let glide_d = (0.09 * rate).min(vowel_total * 0.3);
                let main_d = (vowel_total - glide_d * glides as f64) / (nucleus.len() - glides).max(1) as f64;
                let mut tv = ev.t0;
                for s in nucleus {
                    let d = if s.offglide { glide_d } else { main_d };
                    self.push_sung(s, tv, d, style);
                    tv += d;
                }
            }
            let mut tc = vowel_end;
            for s in &segs[nuc_end..] {
                let d = cons_dur(s);
                self.push_sung(s, tc, d, style);
                tc += d;
            }
            // Pitch: one key per sub-note (melisma), approached with portamento.
            let mut tp = ev.t0;
            let port = if note.legato { song.sing.portamento * 1.8 } else { song.sing.portamento };
            for (j, &(midi, beats)) in note.pitches.iter().enumerate() {
                let f = 440.0 * ((midi + song.sing.transpose + style.pitch - 69.0) / 12.0).exp2();
                let lead_t = if j == 0 { 0.03 } else { 0.015 };
                self.tl.f0.push(key(tp - lead_t, f, port));
                // Vibrato onset after a delay on long-enough notes.
                let d = beats * beat;
                self.tl.vibrato.push(key(tp, 0.0, 0.04));
                if d > song.sing.vibrato_delay + 0.12 {
                    self.tl.vibrato.push(key(tp + song.sing.vibrato_delay, song.sing.vibrato_depth, 0.12));
                }
                tp += d;
            }
            // Gentle dynamic swell over the note.
            self.tl.lung.push(key(ev.t0, lung_sing, 0.05));
            self.tl.lung.push(key(ev.t0 + 0.4 * ev.dur, lung_sing * 1.08, 0.12));
            self.tl.lung.push(key(ev.t0 + 0.85 * ev.dur, lung_sing, 0.1));
        }
        self.t = song_end;
        // Release the last note.
        self.tl.vibrato.push(key(song_end, 0.0, 0.05));
    }

    fn push_sung(&mut self, s: &Seg, t0: f64, dur: f64, style: &Style) {
        let p = s.phone;
        let aspirated = p.manner == Manner::Stop && !p.voiced && p.place != Place::Glottal;
        self.tl.segs.push(TimedSeg { seg: s.clone(), t0, dur, style: style.clone(), aspirated, word_initial: false, sung: true });
    }
}

/// First index of the syllable whose nucleus is at `nuc`.
fn onset_start(segs: &[&Seg], nuc: usize, prev_nuc: Option<usize>) -> usize {
    let lo = prev_nuc.map(|p| p + 1).unwrap_or(0);
    // Skip offglides belonging to the previous nucleus.
    let mut lo = lo;
    while lo < nuc && segs[lo].offglide {
        lo += 1;
    }
    let cons = nuc - lo;
    match (prev_nuc, cons) {
        (None, _) => 0,
        (_, 0) => nuc,
        (_, 1) => nuc - 1,
        // Keep legal-ish two-consonant onsets (obstruent + liquid/glide, s + stop).
        _ => {
            let a = segs[nuc - 2].phone;
            let b = segs[nuc - 1].phone;
            let liquid = matches!(b.manner, Manner::Approximant | Manner::Lateral);
            let s_cluster = a.ipa == "s" && matches!(b.manner, Manner::Stop | Manner::Nasal);
            if (a.is_obstruent() && liquid) || s_cluster {
                nuc - 2
            } else {
                nuc - 1
            }
        }
    }
}

fn is_diphthong_onset(segs: &[&Seg], i: usize) -> bool {
    segs.get(i + 1).map(|s| s.offglide).unwrap_or(false)
}

/// American English allophony on a word: flapping, dark /l/.
fn english_allophones(segs: &[Seg]) -> Vec<Seg> {
    let mut out = segs.to_vec();
    for i in 0..out.len() {
        let p = out[i].phone.ipa;
        // Flapping: /t d/ between a vowel (or ɹ) and an unstressed vowel.
        if (p == "t" || p == "d") && i > 0 && i + 1 < out.len() {
            let prev = out[i - 1].phone;
            let next = &out[i + 1];
            if (prev.is_vowel() || prev.ipa == "ɹ") && next.is_vowel() && next.stress == 0
                && let Some(flap) = phoneme::find("ɾ") {
                    out[i].phone = flap;
                }
        }
        // Dark /l/ in syllable codas (no vowel follows in the word).
        if p == "l" && i > 0 && !out.get(i + 1).map(|s| s.is_vowel()).unwrap_or(false)
            && let Some(dark) = phoneme::find("ɫ") {
                out[i].phone = dark;
            }
    }
    out
}

/// High vowels /i ɯ/ devoice between voiceless consonants or phrase-finally
/// after a voiceless consonant (Tokyo Japanese).
fn japanese_devoicing(segs: &mut [&mut Seg]) {
    let n = segs.len();
    for i in 1..n {
        let ipa = segs[i].phone.ipa;
        if (ipa == "i" || ipa == "ɯ") && !segs[i].long && segs[i].pitch.is_none() {
            let prev_vl = !segs[i - 1].phone.voiced;
            let next_vl = if i + 1 < n { !segs[i + 1].phone.voiced && !segs[i + 1].phone.is_vowel() } else { true };
            if prev_vl && next_vl {
                segs[i].voiceless = true;
            }
        }
    }
}

fn japanese_duration(seg: &Seg, final_mora: bool, mora: f64) -> f64 {
    let p = seg.phone;
    let fin = if final_mora { 1.25 } else { 1.0 };
    if p.is_vowel() {
        let mut d = mora * 0.62 * fin;
        if seg.long {
            d += mora;
        }
        if seg.voiceless {
            d *= 0.6;
        }
        d
    } else if p.ipa == "ɴ" {
        mora * 0.9 * fin
    } else {
        let mut d = (p.dur / 1000.0 * 0.62).clamp(0.02, 0.085);
        if p.manner == Manner::Tap {
            d = 0.025;
        }
        if seg.long {
            d += mora * 0.9;
        }
        d
    }
}

