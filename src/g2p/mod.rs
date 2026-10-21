//! Grapheme-to-phoneme conversion: text → words with IPA transcriptions.
//!
//! IPA conventions (shared with the IPA parser): ASCII `g`, `ː` for long
//! vowels and geminates, `ˈ`/`ˌ` at the start of stressed syllables (English
//! only), words separated by single spaces.

mod english;
mod japanese;
pub mod numbers;

/// A unit of text after normalization.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TextToken {
    /// `text` is the (normalized) word, `ipa` its transcription. An empty `ipa`
    /// means the word could not be converted (e.g. kanji).
    Word { text: String, ipa: String },
    /// Prosodically relevant punctuation: `. , ? ! ; : — …`.
    Punct(char),
}

/// English text → tokens. Handles case, apostrophes (don't, it's), hyphenated
/// words (split), numbers (integers, decimals, ordinals, years, currency,
/// percentages), common abbreviations and letter-by-letter acronyms.
pub fn english_to_tokens(text: &str) -> Vec<TextToken> {
    english::english_to_tokens(text)
}

/// Single English word → IPA (dict lookup, else letter-to-sound rules).
pub fn english_word_to_ipa(word: &str) -> String {
    english::english_word_to_ipa(word)
}

/// Japanese kana (hiragana/katakana) or Hepburn/Kunrei romaji → tokens.
/// Kanji cannot be converted and are emitted as `Word { text, ipa: "" }`.
pub fn japanese_to_tokens(text: &str) -> Vec<TextToken> {
    japanese::japanese_to_tokens(text)
}

/// Hiragana/katakana → IPA (non-kana characters are ignored).
pub fn kana_to_ipa(kana: &str) -> String {
    japanese::kana_to_ipa(kana)
}

/// Hepburn/Kunrei romaji → IPA.
pub fn romaji_to_ipa(romaji: &str) -> String {
    japanese::romaji_to_ipa(romaji)
}
