//! Japanese kana / romaji → IPA.
//!
//! Kana are parsed into morae (CV syllables, moraic nasal ん, sokuon っ and the
//! long mark ー), then rendered with context rules for ん assimilation,
//! gemination and vowel length. Romaji is first transliterated to hiragana so
//! both inputs share the same phonology.

use super::TextToken;

/// A parsed mora.
#[derive(Clone, Debug, PartialEq)]
enum Mora {
    /// Consonant part (may be empty, may be a cluster like "kj") and vowel.
    Syl { cons: String, vowel: char },
    /// Moraic nasal ん.
    N,
    /// Sokuon っ (gemination of the next consonant).
    Q,
    /// Long vowel mark ー.
    Long,
}

/// Katakana → hiragana (other characters unchanged).
fn to_hiragana(c: char) -> char {
    match c {
        'ァ'..='ヶ' | 'ヽ' | 'ヾ' => char::from_u32(c as u32 - 0x60).unwrap_or(c),
        _ => c,
    }
}

/// Consonant and vowel of a plain (full-size) hiragana.
fn kana_base(c: char) -> Option<(&'static str, char)> {
    Some(match c {
        'あ' => ("", 'a'),
        'い' => ("", 'i'),
        'う' => ("", 'ɯ'),
        'え' => ("", 'e'),
        'お' => ("", 'o'),
        'か' | 'ゕ' => ("k", 'a'),
        'き' => ("k", 'i'),
        'く' => ("k", 'ɯ'),
        'け' | 'ゖ' => ("k", 'e'),
        'こ' => ("k", 'o'),
        'が' => ("g", 'a'),
        'ぎ' => ("g", 'i'),
        'ぐ' => ("g", 'ɯ'),
        'げ' => ("g", 'e'),
        'ご' => ("g", 'o'),
        'さ' => ("s", 'a'),
        'し' => ("ɕ", 'i'),
        'す' => ("s", 'ɯ'),
        'せ' => ("s", 'e'),
        'そ' => ("s", 'o'),
        'ざ' => ("z", 'a'),
        'じ' => ("dʑ", 'i'),
        'ず' => ("z", 'ɯ'),
        'ぜ' => ("z", 'e'),
        'ぞ' => ("z", 'o'),
        'た' => ("t", 'a'),
        'ち' => ("tɕ", 'i'),
        'つ' => ("ts", 'ɯ'),
        'て' => ("t", 'e'),
        'と' => ("t", 'o'),
        'だ' => ("d", 'a'),
        'ぢ' => ("dʑ", 'i'),
        'づ' => ("z", 'ɯ'),
        'で' => ("d", 'e'),
        'ど' => ("d", 'o'),
        'な' => ("n", 'a'),
        'に' => ("ɲ", 'i'),
        'ぬ' => ("n", 'ɯ'),
        'ね' => ("n", 'e'),
        'の' => ("n", 'o'),
        'は' => ("h", 'a'),
        'ひ' => ("ç", 'i'),
        'ふ' => ("ɸ", 'ɯ'),
        'へ' => ("h", 'e'),
        'ほ' => ("h", 'o'),
        'ば' => ("b", 'a'),
        'び' => ("b", 'i'),
        'ぶ' => ("b", 'ɯ'),
        'べ' => ("b", 'e'),
        'ぼ' => ("b", 'o'),
        'ぱ' => ("p", 'a'),
        'ぴ' => ("p", 'i'),
        'ぷ' => ("p", 'ɯ'),
        'ぺ' => ("p", 'e'),
        'ぽ' => ("p", 'o'),
        'ま' => ("m", 'a'),
        'み' => ("m", 'i'),
        'む' => ("m", 'ɯ'),
        'め' => ("m", 'e'),
        'も' => ("m", 'o'),
        'や' => ("j", 'a'),
        'ゆ' => ("j", 'ɯ'),
        'よ' => ("j", 'o'),
        'ら' => ("ɾ", 'a'),
        'り' => ("ɾ", 'i'),
        'る' => ("ɾ", 'ɯ'),
        'れ' => ("ɾ", 'e'),
        'ろ' => ("ɾ", 'o'),
        'わ' => ("w", 'a'),
        'ゐ' => ("", 'i'),
        'ゑ' => ("", 'e'),
        'を' => ("", 'o'),
        'ゔ' => ("v", 'ɯ'),
        // Rare katakana-only va/vi/ve/vo letters.
        'ヷ' => ("v", 'a'),
        'ヸ' => ("v", 'i'),
        'ヹ' => ("v", 'e'),
        'ヺ' => ("v", 'o'),
        _ => return None,
    })
}

/// Small vowel kana ぁぃぅぇぉ.
fn small_vowel(c: char) -> Option<char> {
    Some(match c {
        'ぁ' => 'a',
        'ぃ' => 'i',
        'ぅ' => 'ɯ',
        'ぇ' => 'e',
        'ぉ' => 'o',
        _ => return None,
    })
}

/// Small ゃゅょ.
fn small_y(c: char) -> Option<char> {
    Some(match c {
        'ゃ' => 'a',
        'ゅ' => 'ɯ',
        'ょ' => 'o',
        _ => return None,
    })
}

/// Consonants that already include palatalization (yōon adds no extra j).
fn is_palatal(cons: &str) -> bool {
    matches!(cons, "ɕ" | "tɕ" | "dʑ" | "ɲ" | "ç" | "j")
}

/// True for characters handled by the kana parser.
pub fn is_kana(c: char) -> bool {
    matches!(c, '\u{3041}'..='\u{3096}' | 'ゝ' | 'ゞ' | '\u{30A1}'..='\u{30FA}' | 'ー' | 'ヽ' | 'ヾ')
}

fn parse_morae(kana: &str) -> Vec<Mora> {
    let mut morae: Vec<Mora> = Vec::new();
    for c in kana.chars().map(to_hiragana) {
        if let Some((cons, vowel)) = kana_base(c) {
            morae.push(Mora::Syl {
                cons: cons.to_string(),
                vowel,
            });
            continue;
        }
        match c {
            'ん' => morae.push(Mora::N),
            'っ' => morae.push(Mora::Q),
            'ー' => morae.push(Mora::Long),
            'ゝ' | 'ゞ' => {
                // Iteration mark: repeat the previous syllable.
                if let Some(m @ Mora::Syl { .. }) = morae.last().cloned() {
                    morae.push(m);
                }
            }
            'ゎ' => match morae.last_mut() {
                Some(Mora::Syl { cons, vowel }) if !cons.is_empty() => {
                    cons.push('w');
                    *vowel = 'a';
                }
                _ => morae.push(Mora::Syl {
                    cons: "w".into(),
                    vowel: 'a',
                }),
            },
            _ => {
                if let Some(v) = small_y(c) {
                    match morae.last_mut() {
                        // きゃ, しゃ, にゃ, てゅ, でゅ ...
                        Some(Mora::Syl { cons, vowel }) if matches!(*vowel, 'i' | 'e') => {
                            if cons.is_empty() {
                                *cons = "j".into();
                            } else if !is_palatal(cons) || *vowel == 'e' {
                                cons.push('j');
                            }
                            *vowel = v;
                        }
                        _ => morae.push(Mora::Syl {
                            cons: "j".into(),
                            vowel: v,
                        }),
                    }
                } else if let Some(v) = small_vowel(c) {
                    match morae.last_mut() {
                        Some(Mora::Syl { cons, vowel }) => {
                            match (cons.as_str(), *vowel) {
                                // ウィ ウェ ウォ
                                ("", 'ɯ') => *cons = "w".into(),
                                // イェ
                                ("", 'i') => *cons = "j".into(),
                                // クァ グァ
                                ("k" | "g", 'ɯ') => cons.push('w'),
                                // キェ ギェ ... (palatal ones like シェ keep their consonant)
                                (c, 'i') if !c.is_empty() && !is_palatal(c) => cons.push('j'),
                                // ファ ツァ ヴァ ティ ディ トゥ ドゥ シェ チェ ジェ: keep consonant.
                                _ => {}
                            }
                            *vowel = v;
                        }
                        _ => morae.push(Mora::Syl {
                            cons: String::new(),
                            vowel: v,
                        }),
                    }
                }
                // Anything else is ignored.
            }
        }
    }
    morae
}

/// Whether vowel `cur` after `prev` forms a long vowel (aa, ii, uu, ee, oo, ou, ei).
fn lengthens(prev: char, cur: char) -> bool {
    prev == cur || (prev == 'o' && cur == 'ɯ') || (prev == 'e' && cur == 'i')
}

/// Realization of ん given the following consonant.
fn moraic_nasal(next_cons: Option<&str>) -> &'static str {
    let Some(c) = next_cons else { return "ɴ" };
    if c.starts_with("tɕ") || c.starts_with("dʑ") || c.starts_with('ɲ') {
        "ɲ"
    } else if c.starts_with(['p', 'b', 'm']) {
        "m"
    } else if c.starts_with(['t', 'd', 'n', 'ɾ']) {
        "n"
    } else if c.starts_with(['k', 'g']) {
        "ŋ"
    } else {
        "ɴ"
    }
}

fn render(morae: &[Mora]) -> String {
    let mut out = String::new();
    let mut last_vowel: Option<char> = None;
    let mut last_long = false;
    let mut pending_gem = false;
    for (idx, m) in morae.iter().enumerate() {
        match m {
            Mora::Long => {
                if last_vowel.is_some() && !last_long {
                    out.push('ː');
                    last_long = true;
                }
            }
            Mora::N => {
                if pending_gem {
                    out.push('ʔ');
                    pending_gem = false;
                }
                let next_cons = match morae.get(idx + 1) {
                    // ん + z is realized [ndz].
                    Some(Mora::Syl { cons, .. }) if cons == "z" => Some("dz"),
                    Some(Mora::Syl { cons, .. }) if !cons.is_empty() => Some(cons.as_str()),
                    _ => None,
                };
                out.push_str(moraic_nasal(next_cons));
                last_vowel = None;
                last_long = false;
            }
            Mora::Q => {
                if pending_gem {
                    out.push('ʔ');
                }
                pending_gem = true;
            }
            Mora::Syl { cons, vowel } => {
                let prev = if idx == 0 { None } else { morae.get(idx - 1) };
                // z is an affricate word-initially and after ん / っ.
                let cons: &str =
                    if cons == "z" && matches!(prev, None | Some(Mora::N) | Some(Mora::Q)) {
                        "dz"
                    } else {
                        cons
                    };
                if pending_gem {
                    let mut chars = cons.chars();
                    match chars.next() {
                        // っ before a vowel / at a word edge: glottal stop.
                        None => out.push('ʔ'),
                        // Geminate the first segment (tːɕ, tːs, kːj, sː ...).
                        Some(first) => {
                            out.push(first);
                            out.push('ː');
                            out.push_str(chars.as_str());
                        }
                    }
                } else {
                    out.push_str(cons);
                }
                let long = cons.is_empty()
                    && !pending_gem
                    && !last_long
                    && last_vowel.is_some_and(|p| lengthens(p, *vowel));
                if long {
                    out.push('ː');
                    last_long = true;
                } else {
                    out.push(*vowel);
                    last_vowel = Some(*vowel);
                    last_long = false;
                }
                pending_gem = false;
            }
        }
    }
    if pending_gem {
        out.push('ʔ');
    }
    out
}

/// Kana (hiragana/katakana) → IPA. Non-kana characters are ignored.
pub fn kana_to_ipa(kana: &str) -> String {
    let mut hira: String = kana.chars().map(to_hiragana).collect();
    // Fixed greetings where the final は is the topic particle (wa).
    for greeting in ["こんにちは", "こんばんは"] {
        if hira.ends_with(greeting) {
            hira.truncate(hira.len() - 'は'.len_utf8());
            hira.push('わ');
        }
    }
    render(&parse_morae(&hira))
}

// ---------------------------------------------------------------------------
// Romaji
// ---------------------------------------------------------------------------

#[rustfmt::skip]
const ROMAJI: &[(&str, &str)] = &[
    ("a", "あ"), ("i", "い"), ("u", "う"), ("e", "え"), ("o", "お"),
    ("ka", "か"), ("ki", "き"), ("ku", "く"), ("ke", "け"), ("ko", "こ"),
    ("kya", "きゃ"), ("kyu", "きゅ"), ("kyo", "きょ"),
    ("ga", "が"), ("gi", "ぎ"), ("gu", "ぐ"), ("ge", "げ"), ("go", "ご"),
    ("gya", "ぎゃ"), ("gyu", "ぎゅ"), ("gyo", "ぎょ"),
    ("sa", "さ"), ("shi", "し"), ("si", "し"), ("su", "す"), ("se", "せ"), ("so", "そ"),
    ("sha", "しゃ"), ("shu", "しゅ"), ("sho", "しょ"), ("she", "しぇ"),
    ("sya", "しゃ"), ("syu", "しゅ"), ("syo", "しょ"),
    ("za", "ざ"), ("ji", "じ"), ("zi", "じ"), ("zu", "ず"), ("ze", "ぜ"), ("zo", "ぞ"),
    ("ja", "じゃ"), ("ju", "じゅ"), ("jo", "じょ"), ("je", "じぇ"),
    ("jya", "じゃ"), ("jyu", "じゅ"), ("jyo", "じょ"),
    ("zya", "じゃ"), ("zyu", "じゅ"), ("zyo", "じょ"),
    ("ta", "た"), ("chi", "ち"), ("ti", "ち"), ("tsu", "つ"), ("tu", "つ"), ("te", "て"), ("to", "と"),
    ("cha", "ちゃ"), ("chu", "ちゅ"), ("cho", "ちょ"), ("che", "ちぇ"),
    ("tya", "ちゃ"), ("tyu", "ちゅ"), ("tyo", "ちょ"),
    ("tsa", "つぁ"), ("tsi", "つぃ"), ("tse", "つぇ"), ("tso", "つぉ"),
    ("da", "だ"), ("di", "ぢ"), ("du", "づ"), ("dzu", "づ"), ("de", "で"), ("do", "ど"),
    ("dya", "ぢゃ"), ("dyu", "ぢゅ"), ("dyo", "ぢょ"),
    ("na", "な"), ("ni", "に"), ("nu", "ぬ"), ("ne", "ね"), ("no", "の"),
    ("nya", "にゃ"), ("nyu", "にゅ"), ("nyo", "にょ"),
    ("ha", "は"), ("hi", "ひ"), ("fu", "ふ"), ("hu", "ふ"), ("he", "へ"), ("ho", "ほ"),
    ("hya", "ひゃ"), ("hyu", "ひゅ"), ("hyo", "ひょ"),
    ("fa", "ふぁ"), ("fi", "ふぃ"), ("fe", "ふぇ"), ("fo", "ふぉ"), ("fyu", "ふゅ"),
    ("ba", "ば"), ("bi", "び"), ("bu", "ぶ"), ("be", "べ"), ("bo", "ぼ"),
    ("bya", "びゃ"), ("byu", "びゅ"), ("byo", "びょ"),
    ("pa", "ぱ"), ("pi", "ぴ"), ("pu", "ぷ"), ("pe", "ぺ"), ("po", "ぽ"),
    ("pya", "ぴゃ"), ("pyu", "ぴゅ"), ("pyo", "ぴょ"),
    ("ma", "ま"), ("mi", "み"), ("mu", "む"), ("me", "め"), ("mo", "も"),
    ("mya", "みゃ"), ("myu", "みゅ"), ("myo", "みょ"),
    ("ya", "や"), ("yu", "ゆ"), ("yo", "よ"), ("ye", "いぇ"),
    ("ra", "ら"), ("ri", "り"), ("ru", "る"), ("re", "れ"), ("ro", "ろ"),
    ("rya", "りゃ"), ("ryu", "りゅ"), ("ryo", "りょ"),
    ("la", "ら"), ("li", "り"), ("lu", "る"), ("le", "れ"), ("lo", "ろ"),
    ("lya", "りゃ"), ("lyu", "りゅ"), ("lyo", "りょ"),
    ("wa", "わ"), ("wi", "うぃ"), ("we", "うぇ"), ("wo", "を"),
    ("va", "ゔぁ"), ("vi", "ゔぃ"), ("vu", "ゔ"), ("ve", "ゔぇ"), ("vo", "ゔぉ"),
];

fn is_romaji_vowel(c: char) -> bool {
    matches!(c, 'a' | 'i' | 'u' | 'e' | 'o')
}

/// Expands macron/circumflex vowels to vowel + ー.
fn expand_long_vowels(s: &str) -> Vec<char> {
    let mut out = Vec::with_capacity(s.len());
    for c in s.chars().flat_map(char::to_lowercase) {
        let base = match c {
            'ā' | 'â' => 'a',
            'ī' | 'î' => 'i',
            'ū' | 'û' => 'u',
            'ē' | 'ê' => 'e',
            'ō' | 'ô' => 'o',
            _ => {
                out.push(c);
                continue;
            }
        };
        out.push(base);
        out.push('ー');
    }
    out
}

/// Hepburn / Kunrei romaji → hiragana.
fn romaji_to_kana(romaji: &str) -> String {
    let s = expand_long_vowels(romaji);
    let mut out = String::new();
    let mut i = 0;
    while i < s.len() {
        let c = s[i];
        let next = s.get(i + 1).copied();
        if c == 'ー' {
            out.push('ー');
            i += 1;
            continue;
        }
        if !c.is_ascii_lowercase() {
            i += 1; // apostrophes, hyphens, unknown symbols
            continue;
        }
        if c == 'n' {
            match next {
                Some('\'') => {
                    out.push('ん');
                    i += 2;
                    continue;
                }
                Some(v) if is_romaji_vowel(v) || v == 'y' => {} // na / nya: table below
                Some('n') => {
                    let after = s.get(i + 2).copied();
                    out.push('ん');
                    // "nn" + vowel = ん + n-syllable (konnichi); otherwise "nn" = ん.
                    i += if after.is_some_and(|a| is_romaji_vowel(a) || a == 'y') {
                        1
                    } else {
                        2
                    };
                    continue;
                }
                _ => {
                    out.push('ん');
                    i += 1;
                    continue;
                }
            }
        }
        // Hepburn "m" before labials (shimbun, sammai).
        if c == 'm' && matches!(next, Some('b' | 'm' | 'p')) {
            out.push('ん');
            i += 1;
            continue;
        }
        // Double consonant → っ ("kk", "tt", "ss"; also "tch").
        if !is_romaji_vowel(c)
            && (next == Some(c) || (c == 't' && next == Some('c') && s.get(i + 2) == Some(&'h')))
        {
            out.push('っ');
            i += 1;
            continue;
        }
        // Greedy longest match in the syllable table.
        let mut matched = false;
        for len in (1..=3).rev() {
            if i + len > s.len() {
                continue;
            }
            let chunk: String = s[i..i + len].iter().collect();
            if let Some((_, kana)) = ROMAJI.iter().find(|(r, _)| *r == chunk) {
                out.push_str(kana);
                i += len;
                matched = true;
                break;
            }
        }
        if !matched {
            i += 1; // stray consonant: skip
        }
    }
    out
}

/// Hepburn (or Kunrei) romaji → IPA.
pub fn romaji_to_ipa(romaji: &str) -> String {
    kana_to_ipa(&romaji_to_kana(romaji))
}

// ---------------------------------------------------------------------------
// Numbers (read in Japanese)
// ---------------------------------------------------------------------------

const JP_DIGITS: [&str; 10] = [
    "ぜろ",
    "いち",
    "に",
    "さん",
    "よん",
    "ご",
    "ろく",
    "なな",
    "はち",
    "きゅう",
];

/// Reads 1..=9999 in kana (with the usual sound changes).
fn jp_under_10000(n: u64) -> String {
    let mut s = String::new();
    let (th, h, t, o) = (n / 1000, n / 100 % 10, n / 10 % 10, n % 10);
    match th {
        0 => {}
        1 => s.push_str("せん"),
        3 => s.push_str("さんぜん"),
        8 => s.push_str("はっせん"),
        d => {
            s.push_str(JP_DIGITS[d as usize]);
            s.push_str("せん");
        }
    }
    match h {
        0 => {}
        1 => s.push_str("ひゃく"),
        3 => s.push_str("さんびゃく"),
        6 => s.push_str("ろっぴゃく"),
        8 => s.push_str("はっぴゃく"),
        d => {
            s.push_str(JP_DIGITS[d as usize]);
            s.push_str("ひゃく");
        }
    }
    match t {
        0 => {}
        1 => s.push_str("じゅう"),
        d => {
            s.push_str(JP_DIGITS[d as usize]);
            s.push_str("じゅう");
        }
    }
    if o > 0 {
        s.push_str(JP_DIGITS[o as usize]);
    }
    s
}

/// Integer → kana reading (up to 9999兆; larger numbers are read digit by digit).
fn jp_number_kana(digits: &str) -> String {
    let Ok(n) = digits.parse::<u64>() else {
        return jp_digits_kana(digits);
    };
    if n == 0 {
        return "ぜろ".into();
    }
    if n >= 10_000_000_000_000_000 {
        return jp_digits_kana(digits);
    }
    let groups = [
        (n / 1_000_000_000_000, "ちょう"),
        (n / 100_000_000 % 10_000, "おく"),
        (n / 10_000 % 10_000, "まん"),
        (n % 10_000, ""),
    ];
    let mut s = String::new();
    for (g, unit) in groups {
        if g == 0 {
            continue;
        }
        let mut w = jp_under_10000(g);
        if unit == "ちょう" {
            // いっちょう, はっちょう, じゅっちょう
            for (from, to) in [("いち", "いっ"), ("はち", "はっ"), ("じゅう", "じゅっ")]
            {
                if let Some(stem) = w.strip_suffix(from) {
                    w = format!("{stem}{to}");
                    break;
                }
            }
        }
        s.push_str(&w);
        s.push_str(unit);
    }
    s
}

fn jp_digits_kana(digits: &str) -> String {
    digits
        .chars()
        .filter_map(|c| c.to_digit(10))
        .map(|d| JP_DIGITS[d as usize])
        .collect()
}

// ---------------------------------------------------------------------------
// Tokenization
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Debug)]
enum Script {
    Kana,
    Kanji,
    Latin,
    Digit,
}

fn is_kanji(c: char) -> bool {
    matches!(c,
        '\u{4E00}'..='\u{9FFF}' | '\u{3400}'..='\u{4DBF}' | '\u{F900}'..='\u{FAFF}'
        | '\u{20000}'..='\u{2FFFF}' | '々' | '〆')
}

fn is_latin(c: char) -> bool {
    c.is_ascii_alphabetic()
        || matches!(
            c,
            'ā' | 'ī'
                | 'ū'
                | 'ē'
                | 'ō'
                | 'â'
                | 'î'
                | 'û'
                | 'ê'
                | 'ô'
                | 'Ā'
                | 'Ī'
                | 'Ū'
                | 'Ē'
                | 'Ō'
                | 'Â'
                | 'Î'
                | 'Û'
                | 'Ê'
                | 'Ô'
        )
}

/// Full-width ASCII forms (！ Ａ １ ...) and ideographic space → ASCII.
fn normalize_width(c: char) -> char {
    match c {
        '\u{FF01}'..='\u{FF5E}' => char::from_u32(c as u32 - 0xFEE0).unwrap_or(c),
        '\u{3000}' => ' ',
        _ => c,
    }
}

fn punct_of(c: char) -> Option<char> {
    Some(match c {
        '。' | '.' | '｡' => '.',
        '、' | ',' | '､' => ',',
        '?' => '?',
        '!' => '!',
        ';' => ';',
        ':' => ':',
        '…' | '‥' => '…',
        '—' | '―' => '—',
        _ => return None,
    })
}

fn flush(out: &mut Vec<TextToken>, run: &mut String, script: Option<Script>) {
    if run.is_empty() {
        return;
    }
    let text = std::mem::take(run);
    let ipa = match script {
        Some(Script::Kana) => kana_to_ipa(&text),
        Some(Script::Latin) => romaji_to_ipa(&text),
        Some(Script::Digit) => {
            // "3.14" → さんてんいちよん
            match text.split_once('.') {
                Some((int, frac)) => kana_to_ipa(&format!(
                    "{}てん{}",
                    jp_number_kana(int),
                    jp_digits_kana(frac)
                )),
                None => kana_to_ipa(&jp_number_kana(&text)),
            }
        }
        Some(Script::Kanji) | None => String::new(),
    };
    out.push(TextToken::Word { text, ipa });
}

/// Japanese text (kana, romaji, digits; kanji passed through with empty IPA) → tokens.
pub fn japanese_to_tokens(text: &str) -> Vec<TextToken> {
    let chars: Vec<char> = text.chars().map(normalize_width).collect();
    let mut out = Vec::new();
    let mut run = String::new();
    let mut script: Option<Script> = None;
    for (i, &c) in chars.iter().enumerate() {
        let kind = if is_kana(c) {
            Some(Script::Kana)
        } else if is_kanji(c) {
            Some(Script::Kanji)
        } else if is_latin(c) {
            Some(Script::Latin)
        } else if c.is_ascii_digit() {
            Some(Script::Digit)
        } else if c == '\'' && script == Some(Script::Latin) {
            // n' inside romaji
            Some(Script::Latin)
        } else if c == '.'
            && script == Some(Script::Digit)
            && chars.get(i + 1).is_some_and(char::is_ascii_digit)
            && !run.contains('.')
        {
            Some(Script::Digit)
        } else {
            None
        };
        match kind {
            Some(k) => {
                if script != Some(k) {
                    flush(&mut out, &mut run, script);
                    script = Some(k);
                }
                run.push(c);
            }
            None => {
                flush(&mut out, &mut run, script);
                script = None;
                if let Some(p) = punct_of(c)
                    && out.last() != Some(&TextToken::Punct(p))
                {
                    out.push(TextToken::Punct(p));
                }
                // Quotes, brackets, spaces, ・ etc. just separate words.
            }
        }
    }
    flush(&mut out, &mut run, script);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_kana() {
        assert_eq!(kana_to_ipa("こんにちは"), "koɲɲitɕiwa");
        assert_eq!(kana_to_ipa("こんにちわ"), "koɲɲitɕiwa");
        assert_eq!(kana_to_ipa("はな"), "hana");
        assert_eq!(kana_to_ipa("がっこう"), "gakːoː");
        assert_eq!(kana_to_ipa("トーキョー"), "toːkjoː");
        assert_eq!(kana_to_ipa("とうきょう"), "toːkjoː");
        assert_eq!(kana_to_ipa("しんぶん"), "ɕimbɯɴ");
        assert_eq!(kana_to_ipa("さんま"), "samma");
        assert_eq!(kana_to_ipa("かんじ"), "kaɲdʑi");
        assert_eq!(kana_to_ipa("ほんや"), "hoɴja");
        assert_eq!(kana_to_ipa("りんご"), "ɾiŋgo");
        assert_eq!(kana_to_ipa("ふじさん"), "ɸɯdʑisaɴ");
        assert_eq!(kana_to_ipa("せんせい"), "seɴseː");
        assert_eq!(kana_to_ipa("おかあさん"), "okaːsaɴ");
    }

    #[test]
    fn yoon_and_gemination() {
        assert_eq!(kana_to_ipa("しゃしん"), "ɕaɕiɴ");
        assert_eq!(kana_to_ipa("ちゃ"), "tɕa");
        assert_eq!(kana_to_ipa("じゃ"), "dʑa");
        assert_eq!(kana_to_ipa("りゃく"), "ɾjakɯ");
        assert_eq!(kana_to_ipa("にゃ"), "ɲa");
        assert_eq!(kana_to_ipa("ひゃく"), "çakɯ");
        assert_eq!(kana_to_ipa("みゃ"), "mja");
        assert_eq!(kana_to_ipa("まっちゃ"), "matːɕa");
        assert_eq!(kana_to_ipa("みっつ"), "mitːsɯ");
        assert_eq!(kana_to_ipa("きっぷ"), "kipːɯ");
        assert_eq!(kana_to_ipa("ざっし"), "dzaɕːi");
        assert_eq!(kana_to_ipa("あっ"), "aʔ");
    }

    #[test]
    fn katakana_extensions() {
        assert_eq!(kana_to_ipa("ファン"), "ɸaɴ");
        assert_eq!(kana_to_ipa("ティー"), "tiː");
        assert_eq!(kana_to_ipa("ディスク"), "disɯkɯ");
        assert_eq!(kana_to_ipa("ヴァイオリン"), "vaioɾiɴ");
        assert_eq!(kana_to_ipa("ウィキ"), "wiki");
        assert_eq!(kana_to_ipa("ウェブ"), "webɯ");
        assert_eq!(kana_to_ipa("ウォ"), "wo");
        assert_eq!(kana_to_ipa("シェフ"), "ɕeɸɯ");
        assert_eq!(kana_to_ipa("チェック"), "tɕekːɯ");
        assert_eq!(kana_to_ipa("ジェット"), "dʑetːo");
        assert_eq!(kana_to_ipa("ツァ"), "tsa");
        assert_eq!(kana_to_ipa("を"), "o");
        assert_eq!(kana_to_ipa("ゐゑ"), "ie");
    }

    #[test]
    fn z_affrication() {
        assert_eq!(kana_to_ipa("ぞう"), "dzoː");
        assert_eq!(kana_to_ipa("かぜ"), "kaze");
        assert_eq!(kana_to_ipa("かんざし"), "kandzaɕi");
    }

    #[test]
    fn romaji() {
        assert_eq!(romaji_to_ipa("arigatou"), "aɾigatoː");
        assert_eq!(romaji_to_ipa("arigatō"), "aɾigatoː");
        assert_eq!(romaji_to_ipa("konnichiwa"), "koɲɲitɕiwa");
        assert_eq!(romaji_to_ipa("gakkou"), "gakːoː");
        assert_eq!(romaji_to_ipa("Tōkyō"), "toːkjoː");
        assert_eq!(romaji_to_ipa("toukyou"), "toːkjoː");
        assert_eq!(romaji_to_ipa("shinbun"), "ɕimbɯɴ");
        assert_eq!(romaji_to_ipa("shimbun"), "ɕimbɯɴ");
        assert_eq!(romaji_to_ipa("sinbun"), "ɕimbɯɴ");
        assert_eq!(romaji_to_ipa("kan'i"), "kaɴi");
        assert_eq!(romaji_to_ipa("kani"), "kaɲi");
        assert_eq!(romaji_to_ipa("matcha"), "matːɕa");
        assert_eq!(romaji_to_ipa("tsunami"), "tsɯnami");
        assert_eq!(romaji_to_ipa("tunami"), "tsɯnami");
        assert_eq!(romaji_to_ipa("fuji"), "ɸɯdʑi");
        assert_eq!(romaji_to_ipa("huzi"), "ɸɯdʑi");
        assert_eq!(romaji_to_ipa("chi ti"), "tɕitɕi");
        assert_eq!(romaji_to_ipa("onna"), "onna");
        assert_eq!(romaji_to_ipa("sensei"), "seɴseː");
        assert_eq!(romaji_to_ipa("nanami"), "nanami");
    }

    #[test]
    fn numbers() {
        assert_eq!(jp_number_kana("3"), "さん");
        assert_eq!(jp_number_kana("10"), "じゅう");
        assert_eq!(jp_number_kana("300"), "さんびゃく");
        assert_eq!(jp_number_kana("1000"), "せん");
        assert_eq!(jp_number_kana("10000"), "いちまん");
        assert_eq!(jp_number_kana("2024"), "にせんにじゅうよん");
        assert_eq!(jp_number_kana("100000000"), "いちおく");
        assert_eq!(jp_number_kana("1000000000000"), "いっちょう");
    }

    #[test]
    fn tokens() {
        let t = japanese_to_tokens("こんにちは、世界！「すごい」ですね。");
        assert_eq!(
            t,
            vec![
                TextToken::Word {
                    text: "こんにちは".into(),
                    ipa: "koɲɲitɕiwa".into()
                },
                TextToken::Punct(','),
                TextToken::Word {
                    text: "世界".into(),
                    ipa: String::new()
                },
                TextToken::Punct('!'),
                TextToken::Word {
                    text: "すごい".into(),
                    ipa: "sɯgoi".into()
                },
                TextToken::Word {
                    text: "ですね".into(),
                    ipa: "desɯne".into()
                },
                TextToken::Punct('.'),
            ]
        );
        let t = japanese_to_tokens("watashi wa Nanami desu.");
        assert_eq!(t.len(), 5);
        assert_eq!(
            t[2],
            TextToken::Word {
                text: "Nanami".into(),
                ipa: "nanami".into()
            }
        );
        let t = japanese_to_tokens("食べる？");
        assert_eq!(
            t,
            vec![
                TextToken::Word {
                    text: "食".into(),
                    ipa: String::new()
                },
                TextToken::Word {
                    text: "べる".into(),
                    ipa: "beɾɯ".into()
                },
                TextToken::Punct('?'),
            ]
        );
        let t = japanese_to_tokens("３匹");
        assert_eq!(
            t[0],
            TextToken::Word {
                text: "3".into(),
                ipa: "saɴ".into()
            }
        );
    }

    #[test]
    fn no_panic() {
        for s in [
            "",
            "ー",
            "っ",
            "ん",
            "ゃ",
            "ぁ",
            "ゎ",
            "ゝ",
            "ーーっっんん",
            "n'",
            "nn",
            "'",
            "xyz",
            "ā",
            "ッ!",
            "𠀋",
            "99999999999999999999999",
            "1.2.3",
            "．．．",
            "\u{0}",
            "🙂",
        ] {
            let _ = japanese_to_tokens(s);
            let _ = kana_to_ipa(s);
            let _ = romaji_to_ipa(s);
        }
    }
}
