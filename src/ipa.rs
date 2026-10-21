//! IPA string → phonetic segments.
//!
//! Accepts standard IPA plus the common "LLM-isms": ASCII `'` and `,` for
//! stress, `:` for length, `g` for `ɡ`, ligatures (ʧ ʤ ʦ ʨ), tie bars, and
//! `r` for English /ɹ/. Pitch marks ꜛ ꜜ (step up/down, e.g. Japanese
//! accent) and ↗ ↘ (rise/fall) attach to the preceding vowel.

use crate::phoneme::{self, Manner, Phone};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lang {
    English,
    Japanese,
    /// Plain IPA without language-specific allophony.
    Generic,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PitchMark {
    /// ꜛ upstep
    Up,
    /// ꜜ downstep (accent fall after this vowel)
    Down,
    /// ↗ rising contour on this syllable
    Rise,
    /// ↘ falling contour on this syllable
    Fall,
}

#[derive(Clone, Debug)]
pub struct Seg {
    pub phone: &'static Phone,
    /// 0 = unstressed, 1 = primary, 2 = secondary (vowels only).
    pub stress: u8,
    pub long: bool,
    pub half_long: bool,
    pub nasalized: bool,
    pub aspirated: bool,
    pub voiceless: bool,
    /// Second element of a diphthong (non-syllabic).
    pub offglide: bool,
    /// Syllabic consonant (e.g. "button" /ˈbʌtn̩/).
    pub syllabic: bool,
    pub pitch: Option<PitchMark>,
}

impl Seg {
    pub fn new(phone: &'static Phone) -> Seg {
        Seg {
            phone,
            stress: 0,
            long: false,
            half_long: false,
            nasalized: false,
            aspirated: false,
            voiceless: false,
            offglide: false,
            syllabic: false,
            pitch: None,
        }
    }
    pub fn is_vowel(&self) -> bool {
        self.phone.is_vowel()
    }
    /// Nucleus of a syllable (vowel that is not an offglide, or syllabic consonant).
    pub fn is_nucleus(&self) -> bool {
        (self.is_vowel() && !self.offglide) || self.syllabic
    }
}

fn normalize(input: &str, lang: Lang) -> String {
    let mut s = String::with_capacity(input.len());
    for c in input.chars() {
        match c {
            'ɡ' => s.push('g'),
            'ʧ' => s.push_str("tʃ"),
            'ʤ' => s.push_str("dʒ"),
            'ʦ' => s.push_str("ts"),
            'ʣ' => s.push_str("dz"),
            'ʨ' => s.push_str("tɕ"),
            'ʥ' => s.push_str("dʑ"),
            'ᵻ' => s.push('ɨ'),
            'ᵿ' => s.push('ʊ'),
            '\u{0361}' | '\u{035C}' | '‿' => {}
            '\'' | '’' | 'ˈ' => s.push('ˈ'),
            'ˌ' => s.push('ˌ'),
            ':' | 'ː' => s.push('ː'),
            '/' | '[' | ']' => {}
            'r' if lang == Lang::English => s.push('ɹ'),
            'ɐ' if lang == Lang::English => s.push('ʌ'),
            _ => s.push(c),
        }
    }
    s
}

/// Parse one IPA word/phrase. Unknown symbols are reported in `warnings`.
pub fn parse(input: &str, lang: Lang, warnings: &mut Vec<String>) -> Vec<Seg> {
    let text = normalize(input, lang);
    let chars: Vec<char> = text.chars().collect();
    let maxlen = phoneme::max_symbol_chars();
    let mut segs: Vec<Seg> = Vec::new();
    let mut pending_stress = 0u8;
    // True when a syllable boundary/stress mark intervened since the last vowel.
    let mut boundary = true;
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        match c {
            'ˈ' => {
                pending_stress = 1;
                boundary = true;
                i += 1;
                continue;
            }
            'ˌ' => {
                pending_stress = 2;
                boundary = true;
                i += 1;
                continue;
            }
            '.' | ' ' | '-' => {
                boundary = true;
                i += 1;
                continue;
            }
            _ => {}
        }
        // Modifiers apply to the previous segment.
        if let Some(last) = segs.last_mut() {
            let handled = match c {
                'ː' => {
                    last.long = true;
                    true
                }
                'ˑ' => {
                    last.half_long = true;
                    true
                }
                '\u{0303}' => {
                    last.nasalized = true;
                    true
                }
                'ʰ' => {
                    last.aspirated = true;
                    true
                }
                '\u{0325}' | '\u{030A}' => {
                    last.voiceless = true;
                    true
                }
                '\u{0329}' | '\u{030D}' => {
                    last.syllabic = true;
                    true
                }
                '\u{032F}' | '\u{0311}' => {
                    if last.is_vowel() {
                        last.offglide = true;
                    }
                    true
                }
                'ꜛ' => {
                    last.pitch = Some(PitchMark::Up);
                    true
                }
                'ꜜ' => {
                    last.pitch = Some(PitchMark::Down);
                    true
                }
                '↗' => {
                    last.pitch = Some(PitchMark::Rise);
                    true
                }
                '↘' => {
                    last.pitch = Some(PitchMark::Fall);
                    true
                }
                // Diacritics we accept but don't model.
                '\u{031A}' | 'ʲ' | 'ʷ' | 'ˠ' | '\u{0320}' | '\u{031F}' | '\u{031D}' | '\u{031E}' | '\u{0318}' | '\u{0319}'
                | '\u{032A}' | '\u{0348}' | '\u{0339}' | '\u{031C}' | '\u{0308}' | '\u{033D}' | '\u{0324}' | '\u{0330}' => true,
                _ => false,
            };
            if handled {
                i += 1;
                continue;
            }
        }
        // Greedy longest-match against the inventory.
        let mut matched = None;
        for len in (1..=maxlen.min(chars.len() - i)).rev() {
            let cand: String = chars[i..i + len].iter().collect();
            if let Some(p) = phoneme::find(&cand) {
                matched = Some((p, len));
                break;
            }
        }
        match matched {
            Some((p, len)) => {
                let mut seg = Seg::new(p);
                if p.is_vowel() {
                    let prev_vowel = segs.last().map(|s| s.is_vowel()).unwrap_or(false);
                    if prev_vowel && !boundary && lang != Lang::Japanese && matches!(p.ipa, "ɪ" | "ʊ" | "ə" | "ɐ") {
                        seg.offglide = true;
                    } else {
                        seg.stress = pending_stress;
                        pending_stress = 0;
                    }
                    boundary = false;
                }
                segs.push(seg);
                i += len;
            }
            None => {
                if !c.is_whitespace() && !c.is_ascii_punctuation() {
                    warnings.push(format!("unknown IPA symbol '{c}' (U+{:04X}) ignored", c as u32));
                }
                i += 1;
            }
        }
    }
    // Japanese moraic nasal and syllabic marks: nothing else to do here.
    let _ = Manner::Vowel;
    segs
}

pub fn to_string(segs: &[Seg]) -> String {
    let mut s = String::new();
    for seg in segs {
        match seg.stress {
            1 => s.push('ˈ'),
            2 => s.push('ˌ'),
            _ => {}
        }
        s.push_str(seg.phone.ipa);
        if seg.nasalized {
            s.push('\u{0303}');
        }
        if seg.long {
            s.push('ː');
        }
        if seg.offglide {
            s.push('\u{032F}');
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_diphthongs_and_stress() {
        let mut w = Vec::new();
        let s = parse("həˈloʊ", Lang::English, &mut w);
        let ipa: Vec<_> = s.iter().map(|s| s.phone.ipa).collect();
        assert_eq!(ipa, ["h", "ə", "l", "o", "ʊ"]);
        assert_eq!(s[3].stress, 1);
        assert!(s[4].offglide);
        assert!(w.is_empty());
    }

    #[test]
    fn affricates_and_ligatures() {
        let mut w = Vec::new();
        let s = parse("ʧɝʧ t͡ʃ", Lang::English, &mut w);
        let ipa: Vec<_> = s.iter().map(|s| s.phone.ipa).collect();
        assert_eq!(ipa, ["tʃ", "ɝ", "tʃ", "tʃ"]);
    }

    #[test]
    fn japanese_vowel_sequences_stay_syllabic() {
        let mut w = Vec::new();
        let s = parse("kaɪ", Lang::Japanese, &mut w);
        assert!(!s[2].offglide);
    }
}
