//! English text normalization and grapheme-to-phoneme conversion.
//!
//! Pipeline for a single word:
//! 1. CMU dictionary lookup (first variant only),
//! 2. morphological fallback (strip a common suffix, look the stem up, re-attach),
//! 3. compound split into two dictionary words,
//! 4. a "foreign CV word" heuristic (e.g. Japanese/Italian-looking names),
//! 5. NRL-style context letter-to-sound rules.
//!
//! Everything is produced as ARPAbet internally and converted to IPA at the end,
//! so that stress placement / syllabification is shared by all paths.

use std::collections::HashMap;
use std::sync::OnceLock;

use super::TextToken;
use super::numbers;

static DICT_SRC: &str = include_str!("../../data/cmudict.dict");

/// word → ARPAbet string (first variant only).
fn dict() -> &'static HashMap<&'static str, &'static str> {
    static DICT: OnceLock<HashMap<&'static str, &'static str>> = OnceLock::new();
    DICT.get_or_init(|| {
        let mut map = HashMap::with_capacity(140_000);
        for line in DICT_SRC.lines() {
            let line = match line.find('#') {
                Some(i) => &line[..i],
                None => line,
            };
            let Some((word, phones)) = line.trim().split_once(' ') else {
                continue;
            };
            // Alternate pronunciations look like "word(2)": keep the first one only.
            if word.ends_with(')') {
                continue;
            }
            map.entry(word).or_insert(phones.trim());
        }
        map
    })
}

// ---------------------------------------------------------------------------
// Phone representation
// ---------------------------------------------------------------------------

/// One ARPAbet phone: base symbol (no digit) and, for vowels, its stress (0/1/2).
#[derive(Clone, Copy, Debug, PartialEq)]
struct Ph {
    base: &'static str,
    stress: Option<u8>,
}

impl Ph {
    fn new(base: &'static str) -> Ph {
        let stress = if is_vowel_base(base) { Some(0) } else { None };
        Ph { base, stress }
    }
    fn is_vowel(&self) -> bool {
        self.stress.is_some()
    }
    /// Pseudo-phone marking a forced syllable boundary (between spelled
    /// letters or compound members). It produces no output.
    const BARRIER: Ph = Ph {
        base: "",
        stress: None,
    };
    fn is_barrier(&self) -> bool {
        self.base.is_empty()
    }
}

fn is_vowel_base(b: &str) -> bool {
    matches!(
        b,
        "AA" | "AE"
            | "AH"
            | "AO"
            | "AW"
            | "AY"
            | "EH"
            | "ER"
            | "EY"
            | "IH"
            | "IY"
            | "OW"
            | "OY"
            | "UH"
            | "UW"
    )
}

/// Parses a CMU-style phone string ("HH AH0 L OW1").
fn parse_arpabet(s: &'static str) -> Vec<Ph> {
    s.split_whitespace()
        .map(|tok| {
            let last = tok.as_bytes()[tok.len() - 1];
            if last.is_ascii_digit() {
                Ph {
                    base: &tok[..tok.len() - 1],
                    stress: Some(last - b'0'),
                }
            } else {
                Ph::new(tok)
            }
        })
        .collect()
}

/// Parses an unstressed rule output ("K W") into phones.
fn parse_plain(s: &'static str) -> impl Iterator<Item = Ph> {
    s.split_whitespace().map(Ph::new)
}

fn arpa_to_ipa(p: &Ph) -> &'static str {
    let unstressed = p.stress == Some(0);
    match p.base {
        "AA" => "ɑ",
        "AE" => "æ",
        "AH" if unstressed => "ə",
        "AH" => "ʌ",
        "AO" => "ɔ",
        "AW" => "aʊ",
        "AY" => "aɪ",
        "EH" => "ɛ",
        "ER" if unstressed => "ɚ",
        "ER" => "ɝ",
        "EY" => "eɪ",
        "IH" => "ɪ",
        "IY" => "i",
        "OW" => "oʊ",
        "OY" => "ɔɪ",
        "UH" => "ʊ",
        "UW" => "u",
        "B" => "b",
        "CH" => "tʃ",
        "D" => "d",
        "DH" => "ð",
        "F" => "f",
        "G" => "g",
        "HH" => "h",
        "JH" => "dʒ",
        "K" => "k",
        "L" => "l",
        "M" => "m",
        "N" => "n",
        "NG" => "ŋ",
        "P" => "p",
        "R" => "ɹ",
        "S" => "s",
        "SH" => "ʃ",
        "T" => "t",
        "TH" => "θ",
        "V" => "v",
        "W" => "w",
        "Y" => "j",
        "Z" => "z",
        "ZH" => "ʒ",
        _ => "",
    }
}

/// Legal English onset clusters (beyond single consonants), used for
/// maximal-onset syllabification when placing stress marks.
const ONSETS: &[&str] = &[
    "P L", "P R", "P Y", "B L", "B R", "B Y", "T R", "T W", "D R", "D W", "K L", "K R", "K W",
    "K Y", "G L", "G R", "G W", "F L", "F R", "F Y", "TH R", "TH W", "SH R", "S P", "S T", "S K",
    "S M", "S N", "S L", "S W", "S F", "V Y", "M Y", "N Y", "HH Y", "HH W", "S P L", "S P R",
    "S T R", "S K R", "S K W", "S K Y", "S P Y",
];

fn legal_onset(ph: &[Ph]) -> bool {
    // A leading syllable barrier is a forced boundary; a barrier inside the
    // candidate onset makes it illegal.
    let ph = match ph.first() {
        Some(p) if p.is_barrier() => &ph[1..],
        _ => ph,
    };
    if ph.iter().any(Ph::is_barrier) {
        return false;
    }
    match ph.len() {
        0 => true,
        1 => !ph[0].is_vowel() && ph[0].base != "NG",
        2 | 3 => {
            let joined = ph.iter().map(|p| p.base).collect::<Vec<_>>().join(" ");
            ONSETS.contains(&joined.as_str())
        }
        _ => false,
    }
}

/// Converts phones to an IPA string, placing ˈ/ˌ at the start of stressed syllables.
fn phones_to_ipa(ph: &[Ph], strip_stress: bool) -> String {
    let mut marks: Vec<Option<char>> = vec![None; ph.len()];
    let mut prev_vowel: Option<usize> = None;
    for (i, p) in ph.iter().enumerate() {
        let Some(s) = p.stress else { continue };
        if !strip_stress && s > 0 {
            let start = match prev_vowel {
                None => 0,
                Some(pv) => (pv + 1..i).find(|&k| legal_onset(&ph[k..i])).unwrap_or(i),
            };
            marks[start] = Some(if s == 1 { 'ˈ' } else { 'ˌ' });
        }
        prev_vowel = Some(i);
    }
    let mut out = String::new();
    for (p, m) in ph.iter().zip(marks) {
        if let Some(m) = m {
            out.push(m);
        }
        out.push_str(arpa_to_ipa(p));
    }
    out
}

// ---------------------------------------------------------------------------
// Function words, letters, abbreviations
// ---------------------------------------------------------------------------

/// Monosyllabic function words emitted without stress.
const FUNCTION_WORDS: &[&str] = &[
    "a", "an", "the", "of", "to", "and", "in", "is", "it", "for", "on", "with", "as", "at", "by",
    "be", "or", "are", "was", "from", "his", "her", "him", "we", "you", "he", "she", "they", "i",
    "me", "my", "your", "that", "this", "but", "not", "nor", "so", "if", "do", "does", "did",
    "has", "had", "have", "than", "then", "them", "us", "our", "its", "who", "whom", "whose",
    "when", "which", "were", "will", "would", "can", "could", "shall", "should", "may", "might",
    "must", "am", "been", "up", "out", "off", "their", "there", "these", "those", "some", "such",
    "what", "where", "while", "yet", "into", "onto", "per", "via", "'s", "it's", "i'm", "i'll",
    "i'd", "i've", "you're", "we're", "they're", "he's", "she's", "that's", "there's", "let's",
    "can't", "don't", "won't", "isn't", "aren't", "wasn't",
];

/// Spoken names of the letters, used to spell acronyms.
fn letter_name(c: char) -> &'static str {
    match c.to_ascii_lowercase() {
        'a' => "EY1",
        'b' => "B IY1",
        'c' => "S IY1",
        'd' => "D IY1",
        'e' => "IY1",
        'f' => "EH1 F",
        'g' => "JH IY1",
        'h' => "EY1 CH",
        'i' => "AY1",
        'j' => "JH EY1",
        'k' => "K EY1",
        'l' => "EH1 L",
        'm' => "EH1 M",
        'n' => "EH1 N",
        'o' => "OW1",
        'p' => "P IY1",
        'q' => "K Y UW1",
        'r' => "AA1 R",
        's' => "EH1 S",
        't' => "T IY1",
        'u' => "Y UW1",
        'v' => "V IY1",
        'w' => "D AH1 B AH0 L Y UW0",
        'x' => "EH1 K S",
        'y' => "W AY1",
        'z' => "Z IY1",
        _ => "",
    }
}

/// Spells a word letter by letter: secondary stress on every letter, primary on the last.
fn spell_phones(word: &str) -> Vec<Ph> {
    let letters: Vec<char> = word.chars().filter(|c| c.is_ascii_alphabetic()).collect();
    let mut out = Vec::new();
    for (i, &c) in letters.iter().enumerate() {
        let last = i + 1 == letters.len();
        if i > 0 {
            out.push(Ph::BARRIER);
        }
        for mut p in parse_arpabet(letter_name(c)) {
            if p.stress == Some(1) && !last {
                p.stress = Some(2);
            }
            out.push(p);
        }
    }
    out
}

/// All-caps acronyms that collide with real dictionary words but are normally spelled.
const ALWAYS_SPELL: &[&str] = &[
    "US", "AI", "UN", "EU", "UK", "TV", "PC", "ID", "IT", "OS", "CPU",
];

/// True when the (original-case) word should be spelled out letter by letter.
fn is_spelled_acronym(word: &str) -> bool {
    let n = word.chars().count();
    if !(2..=5).contains(&n) || !word.chars().all(|c| c.is_ascii_uppercase()) {
        return false;
    }
    if word == "IT" {
        // Too ambiguous with shouted "it"; read as the word.
        return false;
    }
    ALWAYS_SPELL.contains(&word) || !dict().contains_key(word.to_ascii_lowercase().as_str())
}

/// Folds common Latin accented letters to ASCII (é → e, ß → ss, ...).
fn fold_char(c: char, out: &mut String) {
    let s = match c {
        'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'ā' => "a",
        'æ' => "ae",
        'ç' | 'č' => "c",
        'è' | 'é' | 'ê' | 'ë' | 'ē' => "e",
        'ì' | 'í' | 'î' | 'ï' | 'ī' => "i",
        'ñ' => "n",
        'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø' | 'ō' => "o",
        'œ' => "oe",
        'ù' | 'ú' | 'û' | 'ü' | 'ū' => "u",
        'ý' | 'ÿ' => "y",
        'ß' => "ss",
        'š' => "s",
        'ž' => "z",
        '\'' | '’' | 'ʼ' | '‘' => "'",
        c if c.is_ascii_alphabetic() => {
            out.push(c);
            return;
        }
        _ => "",
    };
    out.push_str(s);
}

/// Lowercases, folds accents and keeps only `[a-z']`.
fn normalize_word(w: &str) -> String {
    let mut out = String::with_capacity(w.len());
    for c in w.chars().flat_map(char::to_lowercase) {
        fold_char(c, &mut out);
    }
    out
}

// ---------------------------------------------------------------------------
// Word → phones
// ---------------------------------------------------------------------------

fn dict_lookup(w: &str) -> Option<Vec<Ph>> {
    dict().get(w).map(|s| parse_arpabet(s))
}

#[derive(Clone, Copy)]
enum Suffix {
    Plural,
    Past,
    Ing,
    Er,
    Est,
    Ly,
    Ness,
    Less,
    Ment,
    Ful,
    Able,
}

const SUFFIXES: &[(&str, Suffix)] = &[
    ("'s", Suffix::Plural),
    ("ness", Suffix::Ness),
    ("less", Suffix::Less),
    ("ment", Suffix::Ment),
    ("ful", Suffix::Ful),
    ("able", Suffix::Able),
    ("ing", Suffix::Ing),
    ("est", Suffix::Est),
    ("es", Suffix::Plural),
    ("ed", Suffix::Past),
    ("er", Suffix::Er),
    ("ly", Suffix::Ly),
    ("s", Suffix::Plural),
];

fn apply_suffix(mut stem: Vec<Ph>, kind: Suffix) -> Vec<Ph> {
    let last = stem.last().map(|p| p.base).unwrap_or("");
    let sibilant = matches!(last, "S" | "Z" | "SH" | "ZH" | "CH" | "JH");
    let voiceless = matches!(last, "P" | "T" | "K" | "F" | "TH" | "S" | "SH" | "CH");
    let add: &'static str = match kind {
        Suffix::Plural if sibilant => "IH0 Z",
        Suffix::Plural if voiceless => "S",
        Suffix::Plural => "Z",
        Suffix::Past if matches!(last, "T" | "D") => "IH0 D",
        Suffix::Past if voiceless => "T",
        Suffix::Past => "D",
        Suffix::Ing => "IH0 NG",
        Suffix::Er => "ER0",
        Suffix::Est => "IH0 S T",
        Suffix::Ly => "L IY0",
        Suffix::Ness => "N AH0 S",
        Suffix::Less => "L AH0 S",
        Suffix::Ment => "M AH0 N T",
        Suffix::Ful => "F AH0 L",
        Suffix::Able => "AH0 B AH0 L",
    };
    stem.extend(parse_arpabet(add));
    stem
}

/// Candidate stems for a word with `suffix` removed (handles e-deletion,
/// consonant doubling and y → i).
fn stem_candidates(stem: &str, suffix: &str) -> Vec<String> {
    let mut c = vec![stem.to_string()];
    let b = stem.as_bytes();
    let vowel_suffix = suffix.starts_with(['e', 'i', 'a']);
    if vowel_suffix {
        c.push(format!("{stem}e"));
        if b.len() >= 3 && b[b.len() - 1] == b[b.len() - 2] && !b"aeiouls".contains(&b[b.len() - 1])
        {
            c.push(stem[..stem.len() - 1].to_string());
        }
    }
    if let Some(s) = stem.strip_suffix('i') {
        c.push(format!("{s}y"));
    }
    c
}

/// Dictionary lookup with suffix stripping (recursive up to `depth`).
fn morph(word: &str, depth: u8) -> Option<Vec<Ph>> {
    if let Some(p) = dict_lookup(word) {
        return Some(p);
    }
    if depth == 0 || word.len() < 4 {
        return None;
    }
    for &(suf, kind) in SUFFIXES {
        let Some(stem) = word.strip_suffix(suf) else {
            continue;
        };
        if suf == "s" && (stem.ends_with('s') || stem.ends_with('\'')) {
            continue;
        }
        for cand in stem_candidates(stem, suf) {
            if cand.len() < 2 {
                continue;
            }
            if let Some(sp) = morph(&cand, depth - 1) {
                return Some(apply_suffix(sp, kind));
            }
        }
    }
    None
}

/// Splits an OOV word into two known words ("voicegenerator").
fn compound(word: &str) -> Option<Vec<Ph>> {
    let n = word.len();
    if n < 8 || !word.is_ascii() {
        return None;
    }
    for split in (4..=n - 4).rev() {
        let (l, r) = word.split_at(split);
        let Some(mut lp) = morph(l, 1) else { continue };
        let Some(rp) = morph(r, 1) else { continue };
        // Compound stress: first element keeps primary, second gets secondary.
        lp.push(Ph::BARRIER);
        lp.extend(rp.into_iter().map(|mut p| {
            if p.stress == Some(1) {
                p.stress = Some(2);
            }
            p
        }));
        return Some(lp);
    }
    None
}

/// Heuristic for "foreign" open-syllable words such as Japanese/Italian names
/// ("nanami", "takeshi"): continental vowels and penultimate stress.
fn foreign_cv(word: &str) -> Option<Vec<Ph>> {
    let b = word.as_bytes();
    if b.len() < 4 || !matches!(b.last(), Some(b'a' | b'i' | b'o' | b'u')) {
        return None;
    }
    let vowel = |c: u8| -> Option<&'static str> {
        Some(match c {
            b'a' => "AA",
            b'e' => "EH",
            b'i' => "IY",
            b'o' => "OW",
            b'u' => "UW",
            _ => return None,
        })
    };
    let mut out: Vec<Ph> = Vec::new();
    let mut i = 0;
    while i < b.len() {
        // Optional consonant (or digraph) followed by exactly one vowel.
        let two = if i + 1 < b.len() {
            &b[i..i + 2]
        } else {
            &b[i..]
        };
        let cons: Option<(&'static str, usize)> = match two {
            b"sh" => Some(("SH", 2)),
            b"ch" => Some(("CH", 2)),
            b"ts" => Some(("T S", 2)),
            _ => match b[i] {
                b'b' => Some(("B", 1)),
                b'd' => Some(("D", 1)),
                b'f' => Some(("F", 1)),
                b'g' => Some(("G", 1)),
                b'h' => Some(("HH", 1)),
                b'j' => Some(("JH", 1)),
                b'k' => Some(("K", 1)),
                b'm' => Some(("M", 1)),
                b'n' => Some(("N", 1)),
                b'p' => Some(("P", 1)),
                b'r' => Some(("R", 1)),
                b's' => Some(("S", 1)),
                b't' => Some(("T", 1)),
                b'v' => Some(("V", 1)),
                b'w' => Some(("W", 1)),
                b'y' => Some(("Y", 1)),
                b'z' => Some(("Z", 1)),
                _ => None,
            },
        };
        if let Some((c, len)) = cons {
            out.extend(parse_plain(c));
            i += len;
            // Syllable-final n before another consonant (ke-n-ji).
            if c == "N" && i < b.len() && vowel(b[i]).is_none() {
                if b[i] == b'y' {
                    return None;
                }
                continue;
            }
        } else if vowel(b[i]).is_none() {
            return None;
        }
        let v = vowel(*b.get(i)?)?;
        out.push(Ph::new(v));
        i += 1;
    }
    let vowels: Vec<usize> = (0..out.len()).filter(|&k| out[k].is_vowel()).collect();
    if vowels.len() < 2 {
        return None;
    }
    out[vowels[vowels.len() - 2]].stress = Some(1);
    Some(out)
}

/// Assigns lexical stress to rule-generated phones (all vowels start at 0).
fn assign_default_stress(word: &str, ph: &mut [Ph]) {
    let vowels: Vec<usize> = (0..ph.len()).filter(|&k| ph[k].is_vowel()).collect();
    if vowels.is_empty() {
        return;
    }
    let n = vowels.len();
    // Stress-attracting suffixes: stress falls on the syllable just before them.
    let from_end = if word.ends_with("tion")
        || word.ends_with("sion")
        || word.ends_with("cian")
        || word.ends_with("ic")
    {
        2
    } else if word.ends_with("ity") || word.ends_with("ical") || word.ends_with("ious") {
        3
    } else {
        n
    };
    let idx = n.saturating_sub(from_end.min(n));
    ph[vowels[idx]].stress = Some(1);
    // Unstressed short vowels reduce to schwa (ə).
    for (j, &k) in vowels.iter().enumerate() {
        if j != idx && matches!(ph[k].base, "AE" | "AA" | "EH") {
            ph[k].base = "AH";
        }
    }
}

/// Full lookup chain for a normalized (lowercase ASCII) word.
fn word_phones(word: &str) -> Vec<Ph> {
    if let Some(p) = dict_lookup(word) {
        return p;
    }
    let bare: String = word.chars().filter(|&c| c != '\'').collect();
    if bare.is_empty() {
        return Vec::new();
    }
    if let Some(p) = morph(word, 2).or_else(|| dict_lookup(&bare)) {
        return p;
    }
    // Words without any vowel letter ("llm", "nth") are spelled.
    if bare.len() <= 5 && !bare.contains(['a', 'e', 'i', 'o', 'u', 'y']) {
        return spell_phones(&bare);
    }
    if let Some(p) = compound(&bare).or_else(|| foreign_cv(&bare)) {
        return p;
    }
    // Possessive of any word, or plural of a compound / foreign word ("nanamis").
    if let Some(stem) = word.strip_suffix("'s")
        && !stem.is_empty()
    {
        return apply_suffix(word_phones(stem), Suffix::Plural);
    }
    if let Some(stem) = bare.strip_suffix('s')
        && !stem.ends_with('s')
        && let Some(p) = compound(stem).or_else(|| foreign_cv(stem))
    {
        return apply_suffix(p, Suffix::Plural);
    }
    let mut p = lts(&bare);
    if !p.iter().any(Ph::is_vowel) {
        return spell_phones(&bare);
    }
    assign_default_stress(&bare, &mut p);
    p
}

/// IPA for a single word made of letters/apostrophes (no further tokenization).
fn simple_word_ipa(w: &str) -> String {
    if is_spelled_acronym(w) {
        return phones_to_ipa(&spell_phones(w), false);
    }
    let norm = normalize_word(w);
    // Keep dictionary entries like "'em"; otherwise drop stray quote-apostrophes.
    let norm = if dict().contains_key(norm.as_str()) {
        norm
    } else {
        norm.trim_matches('\'').to_string()
    };
    if norm.is_empty() {
        return String::new();
    }
    let mut ph = word_phones(&norm);
    // Some entries carry several primary stresses ("nineteen" N AY1 N T IY1 N):
    // keep the last one as primary, demote the others to secondary.
    if let Some(last) = ph.iter().rposition(|p| p.stress == Some(1)) {
        for p in &mut ph[..last] {
            if p.stress == Some(1) {
                p.stress = Some(2);
            }
        }
    }
    let syllables = ph.iter().filter(|p| p.is_vowel()).count();
    let strip = syllables <= 1 && FUNCTION_WORDS.contains(&norm.as_str());
    phones_to_ipa(&ph, strip)
}

/// Single English word → IPA (dictionary lookup, else letter-to-sound rules).
/// Inputs containing digits, hyphens or spaces are tokenized first and the
/// resulting words joined with single spaces.
pub fn english_word_to_ipa(word: &str) -> String {
    let w = word.trim();
    if w.is_empty() {
        return String::new();
    }
    if w.chars()
        .any(|c| !(c.is_alphabetic() || matches!(c, '\'' | '’' | 'ʼ')))
    {
        return english_to_tokens(w)
            .into_iter()
            .filter_map(|t| match t {
                TextToken::Word { ipa, .. } if !ipa.is_empty() => Some(ipa),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join(" ");
    }
    simple_word_ipa(w)
}

// ---------------------------------------------------------------------------
// Letter-to-sound rules (NRL / Elovitz-style context rules)
// ---------------------------------------------------------------------------
//
// Each rule is (left context, match, right context, ARPAbet output).
// Context symbols:
//   ' '  word boundary          '#'  one or more vowels (a e i o u)
//   ':'  zero or more consonants '^'  exactly one consonant
//   '.'  a voiced consonant      '!'  a voiceless consonant
//   '+'  a front vowel (e i y)   '&'  a sibilant (s c g z x j ch sh)
//   '@'  t s r d l z n j th ch sh (after which long u is /u/)
//   '%'  a suffix: e er es ed ing ely (optionally + s) at end of word
// Rules for a letter are tried in order; the first match wins.

type Rule = (&'static str, &'static str, &'static str, &'static str);

#[rustfmt::skip]
const RULES: &[Rule] = &[
    // ---- A ----
    ("", "a", " ", "AH"),
    (" ", "are", " ", "AA R"),
    (" ", "ar", "o", "AH R"),
    ("", "ar", "#", "EH R"),
    ("^", "as", "#", "EY S"),
    ("", "a", "wa", "AH"),
    ("", "aw", "", "AO"),
    (" :", "any", "", "EH N IY"),
    ("", "a", "^+#", "EY"),
    ("#:", "ally", "", "AH L IY"),
    (" ", "al", "#", "AH L"),
    ("", "again", "", "AH G EH N"),
    ("#:", "age", " ", "IH JH"),
    ("", "a", "^+:#", "AE"),
    (" :", "a", "^+ ", "EY"),
    ("", "a", "^%", "EY"),
    (" ", "arr", "", "AH R"),
    ("", "arr", "", "AE R"),
    (" :", "ar", " ", "AA R"),
    ("", "ar", " ", "ER"),
    ("", "ar", "", "AA R"),
    ("", "air", "", "EH R"),
    ("", "ai", "", "EY"),
    ("", "ay", "", "EY"),
    ("", "au", "", "AO"),
    ("#:", "al", " ", "AH L"),
    ("#:", "als", " ", "AH L Z"),
    ("", "alk", "", "AO K"),
    ("", "al", "^", "AO L"),
    (" :", "able", "", "EY B AH L"),
    ("", "able", "", "AH B AH L"),
    ("", "ang", "+", "EY N JH"),
    ("", "a", "", "AE"),
    // ---- B ----
    (" ", "be", "^#", "B IH"),
    ("m", "b", " ", ""),
    ("", "b", "", "B"),
    // ---- C ----
    (" ", "ch", "^", "K"),
    ("^e", "ch", "", "K"),
    ("", "ch", "", "CH"),
    (" s", "ci", "#", "S AY"),
    ("", "ci", "a", "SH"),
    ("", "ci", "o", "SH"),
    ("", "ci", "en", "SH"),
    ("", "cc", "+", "K S"),
    ("", "cc", "", "K"),
    ("", "c", "+", "S"),
    ("", "ck", "", "K"),
    ("", "com", "%", "K AH M"),
    ("", "c", "", "K"),
    // ---- D ----
    ("#:", "ded", " ", "D IH D"),
    ("#:!e", "d", " ", "T"),
    (" ", "de", "^#", "D IH"),
    (" ", "do", " ", "D UW"),
    (" ", "does", "", "D AH Z"),
    ("", "dge", "", "JH"),
    ("", "du", "a", "JH UW"),
    ("", "d", "", "D"),
    // ---- E ----
    ("#:", "e", " ", ""),
    (" :", "e", " ", "IY"),
    ("#", "ed", " ", "D"),
    ("#:", "e", "d ", ""),
    ("", "ev", "er", "EH V"),
    ("", "e", "^%", "IY"),
    ("", "eri", "#", "IY R IY"),
    ("", "eri", "", "EH R IH"),
    ("#:", "er", "#", "ER"),
    ("", "er", "#", "EH R"),
    ("", "er", "", "ER"),
    (" ", "even", "", "IY V EH N"),
    ("#:", "e", "w", ""),
    ("@", "ew", "", "UW"),
    ("", "ew", "", "Y UW"),
    ("", "e", "o", "IY"),
    ("#:&", "es", " ", "IH Z"),
    ("#:", "e", "s ", ""),
    ("#:", "ely", " ", "L IY"),
    ("#:", "ement", "", "M EH N T"),
    ("", "eful", "", "F UH L"),
    ("", "ee", "", "IY"),
    ("", "earn", "", "ER N"),
    (" ", "ear", "^", "ER"),
    ("", "ead", "", "EH D"),
    ("#:", "ea", " ", "IY AH"),
    ("", "ea", "su", "EH"),
    ("", "ea", "", "IY"),
    ("", "eigh", "", "EY"),
    ("c", "ei", "", "IY"),
    ("", "ei", "", "EY"),
    (" ", "eye", "", "AY"),
    ("", "ey", "", "IY"),
    ("", "eu", "", "Y UW"),
    ("", "e", "", "EH"),
    // ---- F ----
    ("", "ful", "", "F UH L"),
    ("", "f", "", "F"),
    // ---- G ----
    ("", "giv", "", "G IH V"),
    (" ", "g", "i^", "G"),
    ("", "ge", "t", "G EH"),
    ("su", "gges", "", "G JH EH S"),
    (" b#", "g", "", "G"),
    ("", "g", "+", "JH"),
    ("", "great", "", "G R EY T"),
    ("#", "gh", "", ""),
    (" ", "gh", "", "G"),
    (" ", "gn", "", "N"),
    ("", "gn", " ", "N"),
    ("", "g", "", "G"),
    // ---- H ----
    (" ", "hav", "", "HH AE V"),
    (" ", "here", "", "HH IY R"),
    (" ", "hour", "", "AW ER"),
    ("", "how", "", "HH AW"),
    ("", "h", "#", "HH"),
    ("", "h", "", ""),
    // ---- I ----
    ("", "in", "d", "AY N"),
    (" ", "i", " ", "AY"),
    ("", "ie", "t", "AY EH"),
    ("#:r", "ied", "", "IY D"),
    ("", "ied", " ", "AY D"),
    ("", "ien", "", "IY EH N"),
    (" :", "ie", " ", "AY"),
    ("", "ie", " ", "IY"),
    ("", "ier", "", "IY ER"),
    ("", "ie", "", "IY"),
    ("", "igh", "", "AY"),
    ("", "ild", "", "AY L D"),
    ("", "ign", " ", "AY N"),
    ("", "ign", "^", "AY N"),
    ("", "ign", "%", "AY N"),
    ("", "ique", "", "IY K"),
    ("", "ir", "#", "AY R"),
    ("", "iz", "%", "AY Z"),
    ("", "is", "%", "AY Z"),
    ("", "i", "d%", "AY"),
    ("+^", "i", "^+", "IH"),
    ("", "i", "t%", "AY"),
    ("#^:", "i", "^+", "IH"),
    ("", "i", "^+", "AY"),
    ("", "ir", "", "ER"),
    ("^", "i", " ", "IY"),
    ("", "i", "", "IH"),
    // ---- J ----
    ("", "j", "", "JH"),
    // ---- K ----
    (" ", "kn", "", "N"),
    ("", "k", "", "K"),
    // ---- L ----
    ("", "lo", "c#", "L OW"),
    ("#:^", "l", "%", "AH L"),
    ("", "lead", "", "L IY D"),
    ("", "l", "", "L"),
    // ---- M ----
    ("", "mov", "", "M UW V"),
    ("", "m", "", "M"),
    // ---- N ----
    ("e", "ng", "+", "N JH"),
    ("", "ng", "r", "NG G"),
    ("", "ng", "#", "NG G"),
    ("", "ngl", "%", "NG G AH L"),
    ("", "ng", "", "NG"),
    ("", "nk", "", "NG K"),
    (" ", "now", " ", "N AW"),
    ("", "n", "", "N"),
    // ---- O ----
    ("", "of", " ", "AH V"),
    ("", "orough", "", "ER OW"),
    ("#:", "or", " ", "ER"),
    ("#:", "ors", " ", "ER Z"),
    ("", "or", "", "AO R"),
    (" ", "one", "", "W AH N"),
    ("", "ous", "", "AH S"),
    ("", "ough", "t", "AO"),
    ("", "ough", "", "AH F"),
    ("", "oul", "d", "UH"),
    ("", "ou", "", "AW"),
    ("", "ow", "n", "AW"),
    ("", "ow", "d", "AW"),
    ("", "ow", "l", "AW"),
    ("", "ow", "", "OW"),
    ("", "oy", "", "OY"),
    ("", "oi", "", "OY"),
    ("", "oor", "", "AO R"),
    ("", "ook", "", "UH K"),
    ("", "ood", "", "UH D"),
    ("", "oo", "", "UW"),
    ("", "oa", "", "OW"),
    ("", "o", "e", "OW"),
    ("", "o", " ", "OW"),
    ("", "o", "^%", "OW"),
    ("", "o", "^en", "OW"),
    ("", "o", "^i#", "OW"),
    ("", "ol", "d", "OW L"),
    ("", "on", "", "AA N"),
    ("", "o", "", "AA"),
    // ---- P ----
    ("", "ph", "", "F"),
    ("", "peop", "", "P IY P"),
    ("", "pow", "", "P AW"),
    ("", "put", " ", "P UH T"),
    (" ", "ps", "", "S"),
    ("", "p", "", "P"),
    // ---- Q ----
    ("", "quar", "", "K W AO R"),
    ("", "que", " ", "K"),
    ("", "qu", "", "K W"),
    ("", "q", "", "K"),
    // ---- R ----
    (" ", "re", "^#", "R IY"),
    ("", "r", "", "R"),
    // ---- S ----
    ("", "sh", "", "SH"),
    ("#", "sion", "", "ZH AH N"),
    ("", "sion", "", "SH AH N"),
    ("", "some", "", "S AH M"),
    ("#", "sur", "#", "ZH ER"),
    ("", "sur", "#", "SH ER"),
    ("#", "su", "#", "ZH UW"),
    ("#", "ssu", "#", "SH UW"),
    ("#", "sed", " ", "Z D"),
    ("#", "s", "#", "Z"),
    ("", "said", "", "S EH D"),
    (".", "s", " ", "Z"),
    ("!e", "s", " ", "S"),
    ("e", "s", " ", "Z"),
    ("#:.e", "s", " ", "Z"),
    (" ", "sch", "", "S K"),
    ("", "sch", "", "SH"),
    ("", "s", "c+", ""),
    ("#", "sm", "", "Z M"),
    ("", "s", "", "S"),
    // ---- T ----
    (" ", "the", " ", "DH AH"),
    ("", "to", " ", "T UW"),
    (" ", "that", " ", "DH AE T"),
    (" ", "this", " ", "DH IH S"),
    (" ", "they", "", "DH EY"),
    (" ", "there", "", "DH EH R"),
    ("", "ther", "", "DH ER"),
    ("", "their", "", "DH EH R"),
    (" ", "than", " ", "DH AE N"),
    (" ", "them", " ", "DH EH M"),
    ("", "these", " ", "DH IY Z"),
    (" ", "then", "", "DH EH N"),
    ("", "through", "", "TH R UW"),
    ("", "those", "", "DH OW Z"),
    ("", "though", " ", "DH OW"),
    (" ", "thus", "", "DH AH S"),
    ("", "th", "", "TH"),
    ("#:", "ted", " ", "T IH D"),
    ("", "tion", "", "SH AH N"),
    ("s", "ti", "#n", "CH"),
    ("", "ti", "o", "SH"),
    ("", "ti", "a", "SH"),
    ("", "tien", "", "SH AH N"),
    ("", "ture", "", "CH ER"),
    ("", "tur", "#", "CH ER"),
    ("", "tu", "a", "CH UW"),
    (" ", "two", "", "T UW"),
    ("", "tsch", "", "CH"),
    ("", "tch", "", "CH"),
    ("", "t", "", "T"),
    // ---- U ----
    (" ", "un", "i", "Y UW N"),
    (" ", "un", "", "AH N"),
    (" ", "upon", "", "AH P AO N"),
    ("@", "ur", "#", "UH R"),
    ("", "ur", "#", "Y UH R"),
    ("", "ur", "", "ER"),
    ("", "u", "^ ", "AH"),
    ("", "u", "^^", "AH"),
    ("", "uy", "", "AY"),
    (" g", "u", "#", ""),
    ("g", "u", "%", ""),
    ("g", "u", "#", "W"),
    ("#n", "u", "", "Y UW"),
    ("@", "u", "", "UW"),
    ("", "u", "", "Y UW"),
    // ---- V ----
    ("", "v", "", "V"),
    // ---- W ----
    (" ", "were", "", "W ER"),
    ("", "wa", "s", "W AA"),
    ("", "wa", "t", "W AA"),
    ("", "where", "", "W EH R"),
    ("", "what", "", "W AH T"),
    ("", "whol", "", "HH OW L"),
    ("", "who", "", "HH UW"),
    ("", "wh", "", "W"),
    ("", "war", "", "W AO R"),
    ("", "wor", "^", "W ER"),
    ("", "wr", "", "R"),
    ("", "w", "", "W"),
    // ---- X ----
    (" ", "x", "", "Z"),
    ("", "x", "", "K S"),
    // ---- Y ----
    ("", "young", "", "Y AH NG"),
    (" ", "you", "", "Y UW"),
    (" ", "yes", "", "Y EH S"),
    (" ", "y", "", "Y"),
    ("#:^", "y", " ", "IY"),
    ("#:^", "y", "i", "IY"),
    (" :", "y", " ", "AY"),
    (" :", "y", "#", "AY"),
    (" :", "y", "^+:#", "IH"),
    (" :", "y", "^#", "AY"),
    ("", "y", "", "IH"),
    // ---- Z ----
    ("t", "z", "", "S"),
    ("", "z", "", "Z"),
];

fn is_v(c: u8) -> bool {
    matches!(c, b'a' | b'e' | b'i' | b'o' | b'u')
}
fn is_c(c: u8) -> bool {
    c.is_ascii_lowercase() && !is_v(c)
}
fn is_voiced_c(c: u8) -> bool {
    matches!(
        c,
        b'b' | b'd' | b'g' | b'j' | b'l' | b'm' | b'n' | b'r' | b'v' | b'w' | b'z'
    )
}
fn is_voiceless_c(c: u8) -> bool {
    matches!(c, b'p' | b't' | b'k' | b'f' | b's' | b'c' | b'x')
}

/// Byte at signed position, word boundary (space) outside the word.
fn at(w: &[u8], pos: isize) -> u8 {
    if pos < 0 {
        b' '
    } else {
        w.get(pos as usize).copied().unwrap_or(b' ')
    }
}

fn is_boundary(c: u8) -> bool {
    !c.is_ascii_lowercase()
}

fn match_left(w: &[u8], mid_start: usize, pat: &str) -> bool {
    let mut pos = mid_start as isize - 1;
    for p in pat.bytes().rev() {
        let c = at(w, pos);
        match p {
            b' ' => {
                if !is_boundary(c) {
                    return false;
                }
                pos -= 1;
            }
            b'#' => {
                if !is_v(c) {
                    return false;
                }
                while is_v(at(w, pos)) {
                    pos -= 1;
                }
            }
            b':' => {
                while is_c(at(w, pos)) {
                    pos -= 1;
                }
            }
            b'^' | b'.' | b'!' | b'+' => {
                let ok = match p {
                    b'^' => is_c(c),
                    b'.' => is_voiced_c(c),
                    b'!' => is_voiceless_c(c),
                    _ => matches!(c, b'e' | b'i' | b'y'),
                };
                if !ok {
                    return false;
                }
                pos -= 1;
            }
            b'&' | b'@' => {
                let prev = at(w, pos - 1);
                if c == b'h' && matches!(prev, b'c' | b's')
                    || (p == b'@' && c == b'h' && prev == b't')
                {
                    pos -= 2;
                } else if (p == b'&' && matches!(c, b's' | b'c' | b'g' | b'z' | b'x' | b'j'))
                    || (p == b'@'
                        && matches!(c, b't' | b's' | b'r' | b'd' | b'l' | b'z' | b'n' | b'j'))
                {
                    pos -= 1;
                } else {
                    return false;
                }
            }
            lit => {
                if c != lit {
                    return false;
                }
                pos -= 1;
            }
        }
    }
    true
}

fn match_right(w: &[u8], after: usize, pat: &str) -> bool {
    let mut pos = after as isize;
    for p in pat.bytes() {
        let c = at(w, pos);
        match p {
            b' ' => {
                if !is_boundary(c) {
                    return false;
                }
                pos += 1;
            }
            b'#' => {
                if !is_v(c) {
                    return false;
                }
                while is_v(at(w, pos)) {
                    pos += 1;
                }
            }
            b':' => {
                while is_c(at(w, pos)) {
                    pos += 1;
                }
            }
            b'^' | b'.' | b'!' | b'+' => {
                let ok = match p {
                    b'^' => is_c(c),
                    b'.' => is_voiced_c(c),
                    b'!' => is_voiceless_c(c),
                    _ => matches!(c, b'e' | b'i' | b'y'),
                };
                if !ok {
                    return false;
                }
                pos += 1;
            }
            b'%' => {
                let rest = &w[(pos.max(0) as usize).min(w.len())..];
                let found = ["ing", "ely", "er", "es", "ed", "e"].iter().find(|suf| {
                    rest.starts_with(suf.as_bytes()) && {
                        let n = suf.len() as isize;
                        is_boundary(at(w, pos + n))
                            || (at(w, pos + n) == b's' && is_boundary(at(w, pos + n + 1)))
                    }
                });
                match found {
                    Some(suf) => pos += suf.len() as isize,
                    None => return false,
                }
            }
            b'&' | b'@' => {
                let next = at(w, pos + 1);
                if next == b'h' && (matches!(c, b'c' | b's') || (p == b'@' && c == b't')) {
                    pos += 2;
                } else if (p == b'&' && matches!(c, b's' | b'c' | b'g' | b'z' | b'x' | b'j'))
                    || (p == b'@'
                        && matches!(c, b't' | b's' | b'r' | b'd' | b'l' | b'z' | b'n' | b'j'))
                {
                    pos += 1;
                } else {
                    return false;
                }
            }
            lit => {
                if c != lit {
                    return false;
                }
                pos += 1;
            }
        }
    }
    true
}

/// Applies the letter-to-sound rules to a lowercase word. Output vowels are unstressed.
fn lts(word: &str) -> Vec<Ph> {
    let w: Vec<u8> = std::iter::once(b' ')
        .chain(word.bytes().filter(u8::is_ascii_lowercase))
        .chain(std::iter::once(b' '))
        .collect();
    let mut out = Vec::new();
    let mut i = 1;
    while i + 1 < w.len() {
        let c = w[i];
        // Doubled consonants are pronounced once ("ll", "ss", "tt"); "cc" has rules.
        if is_c(c) && c == w[i - 1] && c != b'c' {
            i += 1;
            continue;
        }
        let rule = RULES.iter().find(|(l, m, r, _)| {
            m.as_bytes()[0] == c
                && w[i..].starts_with(m.as_bytes())
                && match_left(&w, i, l)
                && match_right(&w, i + m.len(), r)
        });
        match rule {
            Some((_, m, _, o)) => {
                out.extend(parse_plain(o));
                i += m.len();
            }
            None => i += 1,
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Text normalization / tokenization
// ---------------------------------------------------------------------------

/// Pushes the spoken form of `phrase` (possibly several words, hyphenated or not).
fn push_words(out: &mut Vec<TextToken>, phrase: &str) {
    for part in phrase.split([' ', '-']).filter(|p| !p.is_empty()) {
        let text = if is_spelled_acronym(part) {
            part.to_string()
        } else {
            part.to_lowercase()
        };
        out.push(TextToken::Word {
            text,
            ipa: simple_word_ipa(part),
        });
    }
}

fn push_punct(out: &mut Vec<TextToken>, c: char) {
    // Collapse repeated identical punctuation ("!!!", ",,").
    if let Some(TextToken::Punct(p)) = out.last()
        && *p == c
    {
        return;
    }
    out.push(TextToken::Punct(c));
}

/// Abbreviations that are expanded when followed by a period.
/// The bool says whether the abbreviation is a title (never sentence-final).
fn abbreviation(lower: &str) -> Option<(&'static str, bool)> {
    Some(match lower {
        "mr" => ("mister", true),
        "mrs" => ("missus", true),
        "ms" => ("miz", true),
        "dr" => ("doctor", true),
        "st" => ("saint", true),
        "prof" => ("professor", true),
        "sgt" => ("sergeant", true),
        "capt" => ("captain", true),
        "lt" => ("lieutenant", true),
        "col" => ("colonel", true),
        "gen" => ("general", true),
        "gov" => ("governor", true),
        "sen" => ("senator", true),
        "rep" => ("representative", true),
        "rev" => ("reverend", true),
        "mt" => ("mount", true),
        "ft" => ("fort", true),
        "jr" => ("junior", false),
        "sr" => ("senior", false),
        "vs" => ("versus", true),
        "etc" => ("et cetera", false),
        "approx" => ("approximately", true),
        "dept" => ("department", false),
        "inc" => ("incorporated", false),
        "ltd" => ("limited", false),
        "corp" => ("corporation", false),
        "co" => ("company", false),
        "ave" => ("avenue", false),
        "blvd" => ("boulevard", false),
        "fig" => ("figure", true),
        "vol" => ("volume", true),
        "no" => ("number", true),
        "jan" => ("january", false),
        "feb" => ("february", false),
        "mar" => ("march", false),
        "apr" => ("april", false),
        "aug" => ("august", false),
        "sept" => ("september", false),
        "sep" => ("september", false),
        "oct" => ("october", false),
        "nov" => ("november", false),
        "dec" => ("december", false),
        _ => return None,
    })
}

/// Index of the next non-whitespace char at or after `i`.
fn next_non_space(chars: &[char], mut i: usize) -> Option<usize> {
    while i < chars.len() && chars[i].is_whitespace() {
        i += 1;
    }
    (i < chars.len()).then_some(i)
}

/// Reads a run of digits with optional thousands commas. Returns (digits, end).
fn read_digits(chars: &[char], mut i: usize) -> (String, usize, bool) {
    let mut s = String::new();
    let mut had_comma = false;
    while i < chars.len() {
        let c = chars[i];
        if c.is_ascii_digit() {
            s.push(c);
            i += 1;
        } else if c == ','
            && !s.is_empty()
            && chars.len() >= i + 4
            && chars[i + 1..i + 4].iter().all(char::is_ascii_digit)
            && !chars.get(i + 4).is_some_and(char::is_ascii_digit)
        {
            had_comma = true;
            i += 1;
        } else {
            break;
        }
    }
    (s, i, had_comma)
}

/// Cardinal reading of a digit string (falls back to digit-by-digit if huge
/// or if it has leading zeros).
fn cardinal(digits: &str) -> String {
    if digits.len() > 1 && digits.starts_with('0') {
        return numbers::digits_to_words(digits);
    }
    match digits.parse::<u64>() {
        Ok(n) => numbers::number_to_words(n),
        Err(_) => numbers::digits_to_words(digits),
    }
}

/// Reads an optional ".digits" fraction at `i`.
fn read_fraction(chars: &[char], i: usize) -> (Option<String>, usize) {
    if chars.get(i) == Some(&'.') && chars.get(i + 1).is_some_and(char::is_ascii_digit) {
        let mut j = i + 1;
        let mut f = String::new();
        while j < chars.len() && chars[j].is_ascii_digit() {
            f.push(chars[j]);
            j += 1;
        }
        (Some(f), j)
    } else {
        (None, i)
    }
}

const SCALE_WORDS: &[&str] = &["thousand", "million", "billion", "trillion"];

/// Reads a lowercase alphabetic word at `i` (after optional spaces).
fn peek_word(chars: &[char], i: usize) -> Option<(String, usize)> {
    let start = next_non_space(chars, i)?;
    let mut j = start;
    while j < chars.len() && chars[j].is_alphabetic() {
        j += 1;
    }
    (j > start).then(|| (chars[start..j].iter().collect::<String>().to_lowercase(), j))
}

/// Currency amounts: "$5", "$3.50", "$2 million", "£1.20", "€10".
fn read_currency(chars: &[char], i: usize, out: &mut Vec<TextToken>) -> usize {
    let (unit_sg, unit_pl, sub_sg, sub_pl) = match chars[i] {
        '£' => ("pound", "pounds", "penny", "pence"),
        '€' => ("euro", "euros", "cent", "cents"),
        '¥' => ("yen", "yen", "sen", "sen"),
        _ => ("dollar", "dollars", "cent", "cents"),
    };
    let (int, j, _) = read_digits(chars, i + 1);
    let (frac, j) = read_fraction(chars, j);
    let int_words = cardinal(&int);
    // "$2 million" / "$1.5 billion" → "two million dollars".
    if let Some((w, end)) = peek_word(chars, j)
        && SCALE_WORDS.contains(&w.as_str())
    {
        let num = match &frac {
            Some(f) => format!("{int_words} point {}", numbers::digits_to_words(f)),
            None => int_words,
        };
        push_words(out, &format!("{num} {w} {unit_pl}"));
        return end;
    }
    let int_val = int.parse::<u64>().unwrap_or(u64::MAX);
    let unit = if int_val == 1 { unit_sg } else { unit_pl };
    let mut phrase = String::new();
    match frac {
        Some(f) if f.len() <= 2 => {
            let cents: u64 = format!("{f:0<2}").parse().unwrap_or(0);
            if int_val != 0 || cents == 0 {
                phrase.push_str(&format!("{int_words} {unit}"));
            }
            if cents > 0 {
                if !phrase.is_empty() {
                    phrase.push_str(" and ");
                }
                let sub = if cents == 1 { sub_sg } else { sub_pl };
                phrase.push_str(&format!("{} {sub}", numbers::number_to_words(cents)));
            }
        }
        Some(f) => {
            phrase = format!(
                "{int_words} point {} {unit_pl}",
                numbers::digits_to_words(&f)
            );
        }
        None => phrase = format!("{int_words} {unit}"),
    }
    push_words(out, &phrase);
    j
}

/// Numbers: integers, decimals, ordinals, years, percentages, decades, times.
fn read_number(chars: &[char], i: usize, out: &mut Vec<TextToken>) -> usize {
    let (int, mut j, had_comma) = read_digits(chars, i);

    // Clock time "3:30" / "10:05".
    if !had_comma
        && int.len() <= 2
        && chars.get(j) == Some(&':')
        && chars.get(j + 1).is_some_and(char::is_ascii_digit)
        && chars.get(j + 2).is_some_and(char::is_ascii_digit)
        && !chars.get(j + 3).is_some_and(char::is_ascii_digit)
    {
        let h = cardinal(&int);
        let m: u64 = chars[j + 1..j + 3]
            .iter()
            .collect::<String>()
            .parse()
            .unwrap_or(0);
        let phrase = match m {
            0 => format!("{h} o'clock"),
            1..=9 => format!("{h} oh {}", numbers::number_to_words(m)),
            _ => format!("{h} {}", numbers::number_to_words(m)),
        };
        push_words(out, &phrase);
        return j + 3;
    }

    let (frac, k) = read_fraction(chars, j);
    j = k;

    // Alphabetic suffix directly attached ("1st", "1990s", "5km").
    let mut s_end = j;
    while s_end < chars.len() && chars[s_end].is_alphabetic() {
        s_end += 1;
    }
    let suffix: String = chars[j..s_end].iter().collect::<String>().to_lowercase();
    let value = int.parse::<u64>().ok();
    let is_yearish = !had_comma
        && frac.is_none()
        && int.len() == 4
        && value.is_some_and(|v| (1100..=2099).contains(&v));

    let mut phrase;
    if frac.is_none() && matches!(suffix.as_str(), "st" | "nd" | "rd" | "th") {
        phrase = match value {
            Some(v) => numbers::ordinal_to_words(v),
            None => cardinal(&int),
        };
        j = s_end;
    } else if frac.is_none()
        && (suffix == "s" || suffix == "'s")
        && value.is_some_and(|v| v % 10 == 0)
        && (is_yearish || int.len() == 2)
    {
        // Decades: "1990s", "90s".
        let base = if is_yearish {
            numbers::year_to_words(value.unwrap_or(0))
        } else {
            cardinal(&int)
        };
        phrase = numbers::pluralize_last(&base);
        j = s_end;
    } else {
        phrase = if is_yearish && chars.get(j) != Some(&'%') {
            numbers::year_to_words(value.unwrap_or(0))
        } else {
            cardinal(&int)
        };
        if let Some(f) = frac {
            phrase.push_str(" point ");
            phrase.push_str(&numbers::digits_to_words(&f));
        }
    }
    if chars.get(j) == Some(&'%') {
        phrase.push_str(" percent");
        j += 1;
    }
    push_words(out, &phrase);
    j
}

/// Reads a word starting at `i` (letters, internal apostrophes, internal
/// abbreviation dots, hyphens) and pushes its tokens.
fn read_word(chars: &[char], i: usize, out: &mut Vec<TextToken>) -> usize {
    let is_apos = |c: char| matches!(c, '\'' | '’' | 'ʼ');
    let mut j = i;
    while j < chars.len() {
        let c = chars[j];
        let next_alpha = chars.get(j + 1).is_some_and(|c| c.is_alphabetic());
        // Internal apostrophes/hyphens ("don't", "well-known").
        let joiner = (is_apos(c) || c == '-') && next_alpha && j > i;
        // Dotted abbreviations like "e.g", "U.S.A": single letters between dots.
        let abbrev_dot = c == '.'
            && next_alpha
            && (j == i + 1 || chars.get(j.wrapping_sub(2)) == Some(&'.'))
            && !chars.get(j + 2).is_some_and(|c| c.is_alphabetic());
        if c.is_alphabetic() || joiner || abbrev_dot {
            j += 1;
        } else {
            break;
        }
    }
    let raw: String = chars[i..j].iter().collect();
    let followed_by_period = chars.get(j) == Some(&'.');
    // After the abbreviation's own period: is this the end of a sentence?
    let sentence_end_after = |pos: usize| -> bool {
        match next_non_space(chars, pos) {
            None => true,
            Some(k) => chars[k].is_uppercase(),
        }
    };

    // Dotted abbreviations: "e.g.", "i.e.", "U.S.", "a.m.".
    if raw.contains('.') {
        let compact: String = raw.chars().filter(|&c| c != '.').collect();
        let lower = compact.to_lowercase();
        let end = if followed_by_period { j + 1 } else { j };
        match lower.as_str() {
            "eg" => push_words(out, "for example"),
            "ie" => push_words(out, "that is"),
            _ => out.push(TextToken::Word {
                text: compact.to_uppercase(),
                ipa: phones_to_ipa(&spell_phones(&normalize_word(&compact)), false),
            }),
        }
        if followed_by_period && chars.get(end).is_none() {
            push_punct(out, '.');
        }
        return end;
    }

    let lower = raw.to_lowercase();
    if followed_by_period && let Some((expansion, is_title)) = abbreviation(&lower) {
        // "No." only means "number" before a digit.
        let next = next_non_space(chars, j + 1);
        let next_is_digit = next.is_some_and(|k| chars[k].is_ascii_digit());
        let next_is_upper = next.is_some_and(|k| chars[k].is_uppercase());
        if lower != "no" || next_is_digit {
            let (expansion, is_title) = match lower.as_str() {
                "st" if !next_is_upper => ("street", false),
                "dr" if !next_is_upper => ("drive", false),
                _ => (expansion, is_title),
            };
            push_words(out, expansion);
            if !is_title && sentence_end_after(j + 1) {
                push_punct(out, '.');
            }
            return j + 1;
        }
    }
    match lower.as_str() {
        "vs" => push_words(out, "versus"),
        "etc" => push_words(out, "et cetera"),
        _ => {
            for part in raw.split('-').filter(|p| !p.is_empty()) {
                // Strip trailing possessive apostrophe ("dogs'").
                let part = part.trim_end_matches(is_apos);
                if !part.is_empty() {
                    push_words(out, part);
                }
            }
        }
    }
    j
}

/// English text → tokens.
pub fn english_to_tokens(text: &str) -> Vec<TextToken> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let next = chars.get(i + 1).copied();
        let next_digit = next.is_some_and(|c| c.is_ascii_digit());
        let prev_is_alnum = i > 0 && chars[i - 1].is_alphanumeric();
        if c.is_whitespace() {
            i += 1;
        } else if matches!(c, '$' | '£' | '€' | '¥') && next_digit {
            i = read_currency(&chars, i, &mut out);
        } else if matches!(c, '-' | '−') && next_digit && !prev_is_alnum {
            push_words(&mut out, "minus");
            i += 1;
        } else if c.is_ascii_digit() {
            i = read_number(&chars, i, &mut out);
        } else if c.is_alphabetic() {
            i = read_word(&chars, i, &mut out);
        } else {
            match c {
                '.' => {
                    if next == Some('.') && chars.get(i + 2) == Some(&'.') {
                        push_punct(&mut out, '…');
                        i += 3;
                        continue;
                    }
                    push_punct(&mut out, '.');
                }
                ',' | '?' | '!' | ';' | ':' | '…' => push_punct(&mut out, c),
                '。' => push_punct(&mut out, '.'),
                '、' => push_punct(&mut out, ','),
                '？' => push_punct(&mut out, '?'),
                '！' => push_punct(&mut out, '!'),
                '—' | '–' | '―' => push_punct(&mut out, '—'),
                '-' => {
                    if next == Some('-') {
                        push_punct(&mut out, '—');
                        i += 2;
                        continue;
                    }
                    let spaced_before = i == 0 || chars[i - 1].is_whitespace();
                    let spaced_after = next.is_none_or(|c| c.is_whitespace());
                    if spaced_before && spaced_after {
                        push_punct(&mut out, '—');
                    }
                }
                '&' => push_words(&mut out, "and"),
                '+' => push_words(&mut out, "plus"),
                '@' => push_words(&mut out, "at"),
                '%' => push_words(&mut out, "percent"),
                '=' => push_words(&mut out, "equals"),
                _ => {} // quotes, brackets, other symbols: dropped
            }
            i += 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(text: &str) -> Vec<String> {
        english_to_tokens(text)
            .into_iter()
            .filter_map(|t| match t {
                TextToken::Word { text, .. } => Some(text),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn dict_parsed() {
        assert!(dict().len() > 100_000);
        assert_eq!(dict().get("hello"), Some(&"HH AH0 L OW1"));
    }

    #[test]
    fn hello_world() {
        let t = english_to_tokens("Hello, world!");
        assert_eq!(
            t,
            vec![
                TextToken::Word {
                    text: "hello".into(),
                    ipa: "həˈloʊ".into()
                },
                TextToken::Punct(','),
                TextToken::Word {
                    text: "world".into(),
                    ipa: "ˈwɝld".into()
                },
                TextToken::Punct('!'),
            ]
        );
    }

    #[test]
    fn stress_syllabification() {
        // Maximal onset: "extra" → ˈɛk.stɹə; "about" → əˈbaʊt; "construct" kənˈstɹʌkt.
        assert_eq!(english_word_to_ipa("about"), "əˈbaʊt");
        assert_eq!(english_word_to_ipa("extra"), "ˈɛkstɹə");
        assert_eq!(english_word_to_ipa("construct"), "kənˈstɹʌkt");
        assert_eq!(english_word_to_ipa("going"), "ˈgoʊɪŋ");
        assert_eq!(english_word_to_ipa("nineteen"), "ˌnaɪnˈtin");
    }

    #[test]
    fn function_words_unstressed() {
        assert_eq!(english_word_to_ipa("the"), "ðə");
        assert_eq!(english_word_to_ipa("and"), "ənd");
        assert_eq!(english_word_to_ipa("to"), "tu");
        assert!(!english_word_to_ipa("cat").is_empty());
        assert!(english_word_to_ipa("cat").contains('ˈ'));
    }

    #[test]
    fn numbers_in_text() {
        assert_eq!(words("I have 3 cats."), ["i", "have", "three", "cats"]);
        let t = english_to_tokens("I have 3 cats.");
        assert_eq!(t.last(), Some(&TextToken::Punct('.')));
        assert_eq!(words("3.14"), ["three", "point", "one", "four"]);
        assert_eq!(words("the 21st time"), ["the", "twenty", "first", "time"]);
        assert_eq!(words("in 1999"), ["in", "nineteen", "ninety", "nine"]);
        assert_eq!(words("2024"), ["twenty", "twenty", "four"]);
        assert_eq!(words("$5"), ["five", "dollars"]);
        assert_eq!(words("$1"), ["one", "dollar"]);
        assert_eq!(
            words("$3.50"),
            ["three", "dollars", "and", "fifty", "cents"]
        );
        assert_eq!(words("50%"), ["fifty", "percent"]);
        assert_eq!(words("1,000,000"), ["one", "million"]);
        assert_eq!(words("$2 million"), ["two", "million", "dollars"]);
        assert_eq!(words("the 1990s"), ["the", "nineteen", "nineties"]);
        assert_eq!(words("3:30"), ["three", "thirty"]);
        assert_eq!(words("-4"), ["minus", "four"]);
    }

    #[test]
    fn abbreviations() {
        assert_eq!(words("Mr. Smith"), ["mister", "smith"]);
        assert_eq!(words("Dr. Who"), ["doctor", "who"]);
        assert_eq!(words("St. Louis"), ["saint", "louis"]);
        assert_eq!(words("Main St."), ["main", "street"]);
        assert_eq!(words("e.g. this"), ["for", "example", "this"]);
        assert_eq!(words("i.e. that"), ["that", "is", "that"]);
        assert_eq!(words("cats vs. dogs"), ["cats", "versus", "dogs"]);
        // No spurious period after a title.
        let t = english_to_tokens("Mr. Smith");
        assert!(!t.contains(&TextToken::Punct('.')));
        // Sentence-final abbreviation keeps its period.
        let t = english_to_tokens("apples, pears, etc.");
        assert_eq!(t.last(), Some(&TextToken::Punct('.')));
    }

    #[test]
    fn acronyms() {
        assert_eq!(words("NASA"), ["nasa"]);
        assert_eq!(words("LLM"), ["LLM"]);
        assert_eq!(english_word_to_ipa("LLM"), "ˌɛlˌɛlˈɛm");
        assert_eq!(words("the U.S.A."), ["the", "USA"]);
    }

    #[test]
    fn contractions_and_hyphens() {
        assert_eq!(english_word_to_ipa("don't"), "doʊnt");
        assert_eq!(english_word_to_ipa("it's"), "ɪts");
        assert_eq!(words("don't"), ["don't"]);
        assert_eq!(words("it’s"), ["it’s"]);
        assert_eq!(words("well-known"), ["well", "known"]);
        assert_eq!(english_word_to_ipa("well-known"), "ˈwɛl ˈnoʊn");
    }

    #[test]
    fn punctuation() {
        let t = english_to_tokens("Wait... what?! \"Yes\" — (maybe)");
        let p: Vec<char> = t
            .iter()
            .filter_map(|t| match t {
                TextToken::Punct(c) => Some(*c),
                _ => None,
            })
            .collect();
        assert_eq!(p, ['…', '?', '!', '—']);
    }

    #[test]
    fn oov_words() {
        // Foreign CV heuristic.
        assert_eq!(english_word_to_ipa("nanami"), "nɑˈnɑmi");
        // Suffix stripping on dictionary stems.
        let ipa = english_word_to_ipa("nanamis");
        assert!(ipa.ends_with('z'), "{ipa}");
        assert_eq!(english_word_to_ipa("voicegenerator"), "ˈvɔɪsˌdʒɛnɚˌeɪtɚ");
        // Letter-to-sound rules.
        let blick = english_word_to_ipa("blick");
        assert_eq!(blick, "ˈblɪk");
        assert_eq!(english_word_to_ipa("zate"), "ˈzeɪt");
        assert_eq!(english_word_to_ipa("phlorsh"), "ˈflɔɹʃ");
        assert_eq!(english_word_to_ipa("blorping"), "ˈblɔɹpɪŋ");
        assert_eq!(english_word_to_ipa("fration"), "ˈfɹeɪʃən");
    }

    #[test]
    fn no_panic_on_weird_input() {
        for s in [
            "",
            " ",
            "'",
            "''",
            "-",
            "$",
            "$.",
            "1.",
            ".5",
            "1,",
            ",000",
            "999999999999999999999999",
            "$99999999999999999999999.999",
            "日本語",
            "🙂🙂",
            "a'b'c",
            "x-",
            "-x",
            "...",
            "U.S",
            "e.g",
            "St.",
            "Dr.",
            "No. 5",
            "12:3",
            "1st2nd",
            "Ⅻ",
            "ǅ",
            "İstanbul",
            "ß",
            "naïve café",
            "1990's",
            "%%%",
            "\u{0}",
            "a\u{301}",
            "ÅÄÖ",
            "ＡＢＣ",
        ] {
            let _ = english_to_tokens(s);
            let _ = english_word_to_ipa(s);
        }
    }

    #[test]
    #[ignore]
    fn lts_accuracy_probe() {
        let mut keys: Vec<_> = dict()
            .iter()
            .filter(|(k, _)| k.len() >= 4 && k.bytes().all(|b| b.is_ascii_lowercase()))
            .collect();
        keys.sort();
        let (mut ok, mut n) = (0, 0);
        for (i, (k, v)) in keys.iter().enumerate() {
            if i % 97 != 0 {
                continue;
            }
            n += 1;
            let mut g = lts(k);
            assign_default_stress(k, &mut g);
            let got: Vec<_> = g.iter().map(|p| p.base).collect();
            let want: Vec<_> = parse_arpabet(v).iter().map(|p| p.base).collect();
            if got == want {
                ok += 1;
            } else if n % 7 == 0 {
                println!("{k}: got {} want {}", got.join(" "), want.join(" "));
            }
        }
        println!("LTS exact: {ok}/{n}");
    }
}
