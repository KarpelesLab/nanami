//! NANAMI Markup (NML): a small, forgiving, SSML-flavoured language designed
//! to be easy for language models to write. See `docs/NML.md`.
//!
//! ```text
//! <voice name="nanami">
//! Hello! <happy>I'm so glad you came.</happy> /ˈnɑnəmi/ is my name.
//! <prosody rate="0.9" pitch="-2">Let me think<break time="400ms"/> yes.</prosody>
//! <sing tempo="96">twin@C4 kle@C4 lit@G4 tle@G4 star@A4:2 _</sing>
//! ```

use crate::g2p::{self, TextToken};
use crate::ipa::{self, Lang, Seg};
use crate::utterance::*;
use crate::voice::Voice;

#[derive(Debug, Clone)]
enum Tok {
    Text(String),
    Open { name: String, attrs: Vec<(String, String)>, selfclose: bool },
    Close(String),
}

fn tokenize(src: &str) -> Vec<Tok> {
    let mut out = Vec::new();
    let mut text = String::new();
    let chars: Vec<char> = src.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '<'
            && let Some(len) = chars[i..].iter().position(|&c| c == '>') {
                let inner: String = chars[i + 1..i + len].iter().collect();
                if let Some(tok) = parse_tag(&inner) {
                    if !text.is_empty() {
                        out.push(Tok::Text(std::mem::take(&mut text)));
                    }
                    out.push(tok);
                    i += len + 1;
                    continue;
                }
            }
        text.push(chars[i]);
        i += 1;
    }
    if !text.is_empty() {
        out.push(Tok::Text(text));
    }
    out
}

fn parse_tag(inner: &str) -> Option<Tok> {
    let inner = inner.trim();
    if inner.is_empty() || inner.starts_with('!') || inner.starts_with('?') {
        return None;
    }
    if let Some(name) = inner.strip_prefix('/') {
        return Some(Tok::Close(name.trim().to_ascii_lowercase()));
    }
    let (inner, selfclose) = match inner.strip_suffix('/') {
        Some(s) => (s.trim(), true),
        None => (inner, false),
    };
    let mut parts = split_attrs(inner);
    if parts.is_empty() {
        return None;
    }
    let name = parts.remove(0).0.to_ascii_lowercase();
    if !name.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_' || c == ':') {
        return None;
    }
    Some(Tok::Open { name, attrs: parts, selfclose })
}

/// Split `name a=1 b="x y" c` into (key, value) pairs; bare words get "".
fn split_attrs(s: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        while i < chars.len() && chars[i].is_whitespace() {
            i += 1;
        }
        if i >= chars.len() {
            break;
        }
        let start = i;
        while i < chars.len() && !chars[i].is_whitespace() && chars[i] != '=' {
            i += 1;
        }
        let key: String = chars[start..i].iter().collect();
        let mut val = String::new();
        if i < chars.len() && chars[i] == '=' {
            i += 1;
            if i < chars.len() && (chars[i] == '"' || chars[i] == '\'') {
                let q = chars[i];
                i += 1;
                while i < chars.len() && chars[i] != q {
                    val.push(chars[i]);
                    i += 1;
                }
                i += 1;
            } else {
                while i < chars.len() && !chars[i].is_whitespace() {
                    val.push(chars[i]);
                    i += 1;
                }
            }
        }
        out.push((key, val));
    }
    out
}

fn attr<'a>(attrs: &'a [(String, String)], names: &[&str]) -> Option<&'a str> {
    attrs.iter().find(|(k, _)| names.contains(&k.to_ascii_lowercase().as_str())).map(|(_, v)| v.as_str())
}

/// First bare (value-less) attribute, e.g. `<emotion happy>` or `<sad 0.5>`.
fn bare(attrs: &[(String, String)], nth: usize) -> Option<&str> {
    attrs.iter().filter(|(_, v)| v.is_empty()).nth(nth).map(|(k, _)| k.as_str())
}

fn num(s: &str) -> Option<f64> {
    let s = s.trim().trim_end_matches("st").trim_end_matches("x").trim();
    if let Some(p) = s.strip_suffix('%') {
        return p.trim().parse::<f64>().ok().map(|v| v / 100.0);
    }
    s.trim_start_matches('+').parse::<f64>().ok()
}

fn seconds(s: &str) -> Option<f64> {
    let s = s.trim().to_ascii_lowercase();
    if let Some(v) = s.strip_suffix("ms") {
        return v.trim().parse::<f64>().ok().map(|v| v / 1000.0);
    }
    if let Some(v) = s.strip_suffix('s') {
        return v.trim().parse::<f64>().ok();
    }
    match s.as_str() {
        "none" => Some(0.0),
        "x-weak" => Some(0.1),
        "weak" | "short" => Some(0.25),
        "medium" => Some(0.45),
        "strong" | "long" => Some(0.8),
        "x-strong" => Some(1.3),
        _ => s.parse::<f64>().ok().map(|v| if v > 20.0 { v / 1000.0 } else { v }),
    }
}

fn rate_value(s: &str) -> Option<f64> {
    match s.trim().to_ascii_lowercase().as_str() {
        "x-slow" => Some(0.6),
        "slow" => Some(0.8),
        "medium" | "normal" => Some(1.0),
        "fast" => Some(1.25),
        "x-fast" => Some(1.55),
        other => num(other),
    }
}

/// A rendered document: runs of blocks, each with its own voice.
pub struct Document {
    pub runs: Vec<(Voice, Vec<Block>)>,
    pub warnings: Vec<String>,
}

struct Ctx {
    style: Style,
    lang: Lang,
    emphasis: f64,
}

struct Parser {
    stack: Vec<(String, Ctx)>,
    cur: Ctx,
    words: Vec<Word>,
    blocks: Vec<Block>,
    runs: Vec<(Voice, Vec<Block>)>,
    voice: Voice,
    warnings: Vec<String>,
    /// Language of the most recent word (phrases take the language of their words).
    last_lang: Lang,
}

pub fn parse(src: &str, default_voice: &Voice, default_lang: Lang) -> Document {
    let mut p = Parser {
        stack: Vec::new(),
        cur: Ctx { style: Style::default(), lang: default_lang, emphasis: 0.0 },
        words: Vec::new(),
        blocks: Vec::new(),
        runs: Vec::new(),
        voice: default_voice.clone(),
        warnings: Vec::new(),
        last_lang: default_lang,
    };
    let toks = tokenize(src);
    let mut i = 0;
    while i < toks.len() {
        match &toks[i] {
            Tok::Text(t) => p.text(t),
            Tok::Open { name, attrs, selfclose } => {
                if name == "sing" || name == "song" {
                    // Collect raw text until </sing>.
                    let mut body = String::new();
                    let mut j = i + 1;
                    while j < toks.len() {
                        match &toks[j] {
                            Tok::Close(n) if n == name => break,
                            Tok::Text(t) => body.push_str(t),
                            _ => p.warnings.push("tags inside <sing> are ignored".into()),
                        }
                        j += 1;
                    }
                    p.flush(Boundary::None);
                    let song = p.song(attrs, &body);
                    p.blocks.push(Block::Song(song));
                    i = j + 1;
                    continue;
                }
                p.open(name, attrs, *selfclose);
            }
            Tok::Close(name) => p.close(name),
        }
        i += 1;
    }
    p.flush(Boundary::Period);
    let voice = p.voice.clone();
    let blocks = std::mem::take(&mut p.blocks);
    if !blocks.is_empty() {
        p.runs.push((voice, blocks));
    }
    Document { runs: p.runs, warnings: p.warnings }
}

impl Parser {
    fn ctx_clone(&self) -> Ctx {
        Ctx { style: self.cur.style.clone(), lang: self.cur.lang, emphasis: self.cur.emphasis }
    }

    fn open(&mut self, name: &str, attrs: &[(String, String)], selfclose: bool) {
        let saved = self.ctx_clone();
        let amount = |nth: usize| -> f64 {
            attr(attrs, &["amount", "intensity", "level", "strength"]).and_then(num).or_else(|| bare(attrs, nth).and_then(num)).unwrap_or(1.0)
        };
        match name {
            "break" | "pause" => {
                let d = attr(attrs, &["time", "duration", "strength"]).and_then(seconds).or_else(|| bare(attrs, 0).and_then(seconds)).unwrap_or(0.45);
                self.flush(Boundary::None);
                self.blocks.push(Block::Pause(d));
                return;
            }
            "voice" => {
                let vname = attr(attrs, &["name", "voice"]).or_else(|| bare(attrs, 0)).unwrap_or("nanami");
                match Voice::by_name(vname) {
                    Some(v) => {
                        self.flush(Boundary::None);
                        let blocks = std::mem::take(&mut self.blocks);
                        if !blocks.is_empty() {
                            self.runs.push((self.voice.clone(), blocks));
                        }
                        self.voice = v;
                    }
                    None => self.warnings.push(format!("unknown voice '{vname}' (known: {})", Voice::names().join(", "))),
                }
            }
            "emotion" | "emo" | "mood" => {
                let ename = attr(attrs, &["name", "type", "value"]).or_else(|| bare(attrs, 0)).unwrap_or("neutral");
                let amt = attr(attrs, &["amount", "intensity", "level", "strength"]).and_then(num).or_else(|| bare(attrs, 1).and_then(num)).unwrap_or(1.0);
                if !self.cur.style.apply_emotion(ename, amt) {
                    self.warnings.push(format!("unknown emotion '{ename}' (known: {})", emotion_names().join(", ")));
                }
            }
            "prosody" => {
                if let Some(r) = attr(attrs, &["rate", "speed"]).and_then(rate_value) {
                    self.cur.style.rate *= r;
                }
                if let Some(v) = attr(attrs, &["pitch"]).and_then(num) {
                    self.cur.style.pitch += v;
                }
                if let Some(v) = attr(attrs, &["range"]).and_then(num) {
                    self.cur.style.range *= v;
                }
                if let Some(v) = attr(attrs, &["volume", "loudness"]) {
                    let f = match v {
                        "x-soft" | "silent" => Some(0.45),
                        "soft" => Some(0.7),
                        "medium" => Some(1.0),
                        "loud" => Some(1.4),
                        "x-loud" => Some(1.9),
                        other => num(other),
                    };
                    if let Some(f) = f {
                        self.cur.style.volume *= f;
                    }
                }
            }
            "emph" | "emphasis" | "em" | "strong" | "b" => self.cur.emphasis = (self.cur.emphasis + 0.8 * amount(0)).min(1.5),
            "whisper" => self.cur.style.whisper = true,
            "quality" | "voice-quality" | "vq" => {
                if let Some(v) = attr(attrs, &["breath", "breathy", "breathiness"]).and_then(num) {
                    self.cur.style.breath = v.clamp(0.0, 1.0);
                }
                if let Some(v) = attr(attrs, &["creak", "creaky", "fry"]).and_then(num) {
                    self.cur.style.creak = v.clamp(0.0, 1.0);
                }
                if let Some(v) = attr(attrs, &["tension", "tense", "pressed"]).and_then(num) {
                    self.cur.style.tension = v.clamp(-1.0, 1.0);
                }
                if let Some(v) = attr(attrs, &["tremor", "shaky"]).and_then(num) {
                    self.cur.style.tremor = v.clamp(0.0, 1.0);
                }
                if let Some(v) = attr(attrs, &["smile"]).and_then(num) {
                    self.cur.style.smile = v.clamp(0.0, 1.0);
                }
                if let Some(v) = attr(attrs, &["precision", "articulation"]).and_then(num) {
                    self.cur.style.precision = v.clamp(0.3, 2.0);
                }
            }
            "lang" | "language" => {
                let l = attr(attrs, &["xml:lang", "lang", "name"]).or_else(|| bare(attrs, 0)).unwrap_or("en");
                self.cur.lang = lang_of(l);
            }
            "ja" | "jp" | "japanese" => self.cur.lang = Lang::Japanese,
            "en" | "english" => self.cur.lang = Lang::English,
            "ipa" | "phoneme" => {
                // <phoneme ph="..."/> (SSML style) or <ipa>...</ipa>.
                if let Some(ph) = attr(attrs, &["ph", "ipa"]) {
                    let ph = ph.to_string();
                    self.ipa_words(&ph);
                }
                if !selfclose {
                    self.stack.push(("ipa".into(), saved));
                    self.stack.push(("#ipa-mode".into(), self.ctx_clone()));
                }
                return;
            }
            "speak" | "nml" | "s" | "p" | "sentence" | "paragraph" => {}
            other => {
                // Shorthand emotion tags: <happy>, <sad 0.5>, <angry amount=0.3>.
                let mut st = self.cur.style.clone();
                if st.apply_emotion(other, amount(0)) {
                    self.cur.style = st;
                } else {
                    self.warnings.push(format!("unknown tag <{other}> ignored"));
                }
            }
        }
        if !selfclose {
            self.stack.push((name.to_string(), saved));
        } else {
            self.cur = saved;
        }
    }

    fn in_ipa_mode(&self) -> bool {
        self.stack.iter().any(|(n, _)| n == "#ipa-mode")
    }

    fn close(&mut self, name: &str) {
        let name = match name {
            "phoneme" => "ipa",
            n => n,
        };
        if name == "ipa" {
            // Pop the ipa-mode marker as well.
            if let Some(pos) = self.stack.iter().rposition(|(n, _)| n == "#ipa-mode") {
                self.stack.remove(pos);
            }
        }
        if let Some(pos) = self.stack.iter().rposition(|(n, _)| n == name) {
            let (_, ctx) = self.stack.remove(pos);
            // Discard anything opened after it (mis-nested markup).
            self.stack.truncate(pos);
            self.cur = ctx;
        } else if name != "voice" {
            self.warnings.push(format!("unmatched </{name}>"));
        }
    }

    fn push_word(&mut self, text: &str, segs: Vec<Seg>, function: bool) {
        if segs.is_empty() {
            return;
        }
        self.words.push(Word { text: text.to_string(), segs, function, emphasis: self.cur.emphasis, style: self.cur.style.clone() });
    }

    fn ipa_words(&mut self, s: &str) {
        let lang = self.cur.lang;
        for w in s.split_whitespace() {
            // Punctuation inside IPA still marks boundaries.
            let (core, punct) = split_trailing_punct(w);
            let segs = ipa::parse(core, lang, &mut self.warnings);
            let function = !core.contains('ˈ') && !core.contains('\'') && segs.iter().filter(|s| s.is_nucleus()).count() <= 1 && lang == Lang::English && segs.len() <= 3;
            self.push_word(core, segs, function);
            self.last_lang = lang;
            if let Some(c) = punct {
                self.punct(c);
            }
        }
    }

    fn text(&mut self, t: &str) {
        if self.in_ipa_mode() {
            self.ipa_words(t);
            return;
        }
        // Inline /ipa/ spans.
        let mut rest = t;
        while let Some(a) = rest.find('/') {
            let after = &rest[a + 1..];
            let Some(b) = after.find('/') else { break };
            self.plain(&rest[..a]);
            let ipa_span = after[..b].to_string();
            self.ipa_words(&ipa_span);
            rest = &after[b + 1..];
        }
        self.plain(rest);
    }

    fn plain(&mut self, t: &str) {
        if t.trim().is_empty() {
            return;
        }
        let lang = match self.cur.lang {
            Lang::Generic => Lang::English,
            l => l,
        };
        // Auto-detect kana in English context.
        let lang = if t.chars().any(is_kana) { Lang::Japanese } else { lang };
        let toks = match lang {
            Lang::Japanese => g2p::japanese_to_tokens(t),
            _ => g2p::english_to_tokens(t),
        };
        for tok in toks {
            match tok {
                TextToken::Word { text, ipa } => {
                    if ipa.is_empty() {
                        self.warnings.push(format!("cannot pronounce '{text}' (kanji need kana or IPA)"));
                        continue;
                    }
                    let segs = ipa::parse(&ipa, lang, &mut self.warnings);
                    let function = lang == Lang::English && !ipa.contains('ˈ') && segs.iter().filter(|s| s.is_nucleus()).count() <= 1;
                    let saved = self.cur.lang;
                    self.cur.lang = lang;
                    self.push_word(&text, segs, function);
                    self.cur.lang = saved;
                    self.last_lang = lang;
                }
                TextToken::Punct(c) => self.punct(c),
            }
        }
    }

    fn punct(&mut self, c: char) {
        let b = match c {
            '.' | '。' => Boundary::Period,
            '!' | '！' => Boundary::Exclaim,
            '?' | '？' => Boundary::Question,
            ',' | '、' | ';' | ':' | '—' | '–' => Boundary::Comma,
            '…' => {
                self.flush(Boundary::Comma);
                self.blocks.push(Block::Pause(0.35));
                return;
            }
            _ => return,
        };
        self.flush(b);
    }

    fn flush(&mut self, boundary: Boundary) {
        if self.words.is_empty() {
            return;
        }
        let words = std::mem::take(&mut self.words);
        let lang = self.last_lang;
        self.blocks.push(Block::Speech(Phrase { words, boundary, lang }));
    }

    // ------------------------------------------------------------------
    // <sing>
    // ------------------------------------------------------------------

    fn song(&mut self, attrs: &[(String, String)], body: &str) -> Song {
        let mut sing = SingStyle::default();
        if let Some(v) = attr(attrs, &["tempo", "bpm"]).and_then(num) {
            sing.tempo = v.clamp(20.0, 400.0);
        }
        if let Some(v) = attr(attrs, &["transpose", "key-shift"]).and_then(num) {
            sing.transpose = v;
        }
        if let Some(v) = attr(attrs, &["vibrato", "vibrato-depth"]).and_then(num) {
            sing.vibrato_depth = v.clamp(0.0, 2.0);
        }
        if let Some(v) = attr(attrs, &["vibrato-rate"]).and_then(num) {
            sing.vibrato_rate = v.clamp(2.0, 9.0);
        }
        if let Some(v) = attr(attrs, &["portamento", "glide"]).and_then(num) {
            sing.portamento = v.clamp(0.005, 0.3);
        }
        if let Some(s) = attr(attrs, &["style", "technique"]) {
            sing.classical = matches!(s.to_ascii_lowercase().as_str(), "classical" | "opera" | "operatic" | "bel-canto");
        }
        let lang = match attr(attrs, &["lang", "xml:lang"]) {
            Some(l) => lang_of(l),
            None => self.cur.lang,
        };
        let mut items: Vec<SungItem> = Vec::new();
        // Durations default to one beat (deliberately not "sticky").
        let last_dur = 1.0;
        for tok in body.split_whitespace() {
            let (lyric, spec) = match tok.rfind('@') {
                Some(at) => (&tok[..at], &tok[at + 1..]),
                None => {
                    // Rest: `_`, `_:2`, `r:1`.
                    let (l, d) = tok.split_once(':').unwrap_or((tok, ""));
                    if l == "_" || l == "r" || l == "rest" || l == "-" && d.is_empty() {
                        let beats = parse_beats(d).unwrap_or(last_dur);
                        items.push(SungItem::Rest(beats));
                        continue;
                    }
                    (tok, "")
                }
            };
            let mut notes: Vec<(f64, f64)> = Vec::new();
            for n in spec.split(',').filter(|s| !s.is_empty()) {
                let (pitch, dur) = n.split_once(':').unwrap_or((n, ""));
                let Some(midi) = parse_pitch(pitch) else {
                    self.warnings.push(format!("bad pitch '{pitch}' in '{tok}'"));
                    continue;
                };
                let beats = parse_beats(dur).unwrap_or(last_dur);
                notes.push((midi, beats));
            }
            if notes.is_empty() {
                self.warnings.push(format!("note '{tok}' has no pitch (use lyric@C4:1)"));
                continue;
            }
            let (legato, lyric) = match lyric.strip_suffix('~') {
                Some(l) => (true, l),
                None => (false, lyric),
            };
            if lyric == "-" || lyric.is_empty() {
                // Melisma continuation of the previous syllable.
                if let Some(SungItem::Note(prev)) = items.last_mut() {
                    prev.pitches.extend(notes);
                    continue;
                }
            }
            let segs = self.lyric_segs(lyric, lang);
            for (k, syl) in split_syllables(&segs, notes.len()).into_iter().enumerate() {
                let pitches = if k + 1 == notes.len().min(syllable_count(&segs).max(1)) { notes[k..].to_vec() } else { vec![notes[k.min(notes.len() - 1)]] };
                items.push(SungItem::Note(Note { text: lyric.to_string(), segs: syl, pitches, legato }));
            }
        }
        Song { items, sing, style: self.cur.style.clone(), lang }
    }

    fn lyric_segs(&mut self, lyric: &str, lang: Lang) -> Vec<Seg> {
        if let Some(inner) = lyric.strip_prefix('/').and_then(|l| l.strip_suffix('/')) {
            return ipa::parse(inner, lang, &mut self.warnings);
        }
        let ipa_str = if lyric.chars().any(is_kana) || lang == Lang::Japanese {
            let s = g2p::kana_to_ipa(lyric);
            if s.is_empty() { g2p::romaji_to_ipa(lyric) } else { s }
        } else {
            g2p::english_word_to_ipa(&lyric.replace('-', ""))
        };
        let l = if lyric.chars().any(is_kana) { Lang::Japanese } else { lang };
        ipa::parse(&ipa_str, l, &mut self.warnings)
    }
}

/// Number of syllable nuclei.
fn syllable_count(segs: &[Seg]) -> usize {
    segs.iter().filter(|s| s.is_nucleus()).count()
}

/// Split a word into `max` syllable groups (extra syllables merge into the last).
fn split_syllables(segs: &[Seg], max: usize) -> Vec<Vec<Seg>> {
    let nuclei: Vec<usize> = (0..segs.len()).filter(|&i| segs[i].is_nucleus()).collect();
    if nuclei.len() <= 1 || max <= 1 {
        return vec![segs.to_vec()];
    }
    let groups = nuclei.len().min(max);
    // Boundaries: before the onset of nuclei[1..groups].
    let mut cuts = Vec::new();
    for g in 1..groups {
        let nuc = nuclei[g];
        let prev = nuclei[g - 1];
        let mut lo = prev + 1;
        while lo < nuc && segs[lo].offglide {
            lo += 1;
        }
        let cons = nuc - lo;
        cuts.push(if cons <= 1 { nuc - cons } else { lo + 1 });
    }
    let mut out = Vec::new();
    let mut start = 0;
    for c in cuts {
        out.push(segs[start..c].to_vec());
        start = c;
    }
    out.push(segs[start..].to_vec());
    out
}

fn split_trailing_punct(w: &str) -> (&str, Option<char>) {
    match w.chars().last() {
        Some(c) if matches!(c, '.' | ',' | '?' | '!' | ';' | '…') && w.chars().count() > 1 => (&w[..w.len() - c.len_utf8()], Some(c)),
        Some(c) if matches!(c, '?' | '!' | ',' | ';') => ("", Some(c)),
        _ => (w, None),
    }
}

fn is_kana(c: char) -> bool {
    matches!(c as u32, 0x3040..=0x30FF | 0x31F0..=0x31FF | 0xFF66..=0xFF9F)
}

fn lang_of(l: &str) -> Lang {
    let l = l.to_ascii_lowercase();
    if l.starts_with("ja") || l.starts_with("jp") {
        Lang::Japanese
    } else if l == "ipa" || l == "generic" {
        Lang::Generic
    } else {
        Lang::English
    }
}

/// Note name (C4, F#3, Bb5, c'), MIDI number (60), or frequency (440hz).
pub fn parse_pitch(s: &str) -> Option<f64> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    let lower = s.to_ascii_lowercase();
    if let Some(f) = lower.strip_suffix("hz") {
        let f: f64 = f.parse().ok()?;
        return Some(69.0 + 12.0 * (f / 440.0).log2());
    }
    if let Ok(m) = s.parse::<f64>() {
        return Some(m);
    }
    let mut chars = s.chars();
    let letter = chars.next()?.to_ascii_uppercase();
    let base = match letter {
        'C' => 0,
        'D' => 2,
        'E' => 4,
        'F' => 5,
        'G' => 7,
        'A' => 9,
        'B' => 11,
        _ => return None,
    } as f64;
    let rest: String = chars.collect();
    let mut acc = 0.0;
    let mut octave_str = rest.as_str();
    loop {
        if let Some(r) = octave_str.strip_prefix('#').or_else(|| octave_str.strip_prefix('♯')) {
            acc += 1.0;
            octave_str = r;
        } else if let Some(r) = octave_str.strip_prefix('b').or_else(|| octave_str.strip_prefix('♭')) {
            acc -= 1.0;
            octave_str = r;
        } else {
            break;
        }
    }
    let octave: f64 = if octave_str.is_empty() { 4.0 } else { octave_str.parse().ok()? };
    Some(12.0 * (octave + 1.0) + base + acc)
}

/// Beats: "1", "0.5", "1/2", "3/4", "1.5"; also w h q e s (+ "." dotted).
pub fn parse_beats(s: &str) -> Option<f64> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    let (core, dotted) = match s.strip_suffix('.') {
        Some(c) if !c.is_empty() && c.chars().all(|c| c.is_ascii_alphabetic()) => (c, true),
        _ => (s, false),
    };
    let v = match core {
        "w" => 4.0,
        "h" => 2.0,
        "q" => 1.0,
        "e" => 0.5,
        "s" => 0.25,
        _ => {
            if let Some((a, b)) = core.split_once('/') {
                a.trim().parse::<f64>().ok()? / b.trim().parse::<f64>().ok()?
            } else {
                core.parse::<f64>().ok()?
            }
        }
    };
    Some(if dotted { v * 1.5 } else { v })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pitches() {
        assert_eq!(parse_pitch("C4"), Some(60.0));
        assert_eq!(parse_pitch("A4"), Some(69.0));
        assert_eq!(parse_pitch("F#3"), Some(54.0));
        assert_eq!(parse_pitch("Bb5"), Some(82.0));
        assert!((parse_pitch("440hz").unwrap() - 69.0).abs() < 1e-9);
    }

    #[test]
    fn beats() {
        assert_eq!(parse_beats("1/2"), Some(0.5));
        assert_eq!(parse_beats("q."), Some(1.5));
        assert_eq!(parse_beats("2"), Some(2.0));
    }

    #[test]
    fn parses_mixed_document() {
        let doc = parse("Hello, <happy>world</happy>! /ˈnɑnəmi/ <break time=\"300ms\"/> <sing tempo=\"120\">la@C4:1 la@D4 _:1</sing>", &Voice::nanami(), Lang::English);
        let blocks = &doc.runs[0].1;
        assert!(matches!(blocks[0], Block::Speech(_)));
        assert!(blocks.iter().any(|b| matches!(b, Block::Pause(_))));
        let song = blocks.iter().find_map(|b| if let Block::Song(s) = b { Some(s) } else { None }).unwrap();
        assert_eq!(song.items.len(), 3);
        assert!(doc.warnings.is_empty(), "{:?}", doc.warnings);
    }
}
