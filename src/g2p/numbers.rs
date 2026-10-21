//! Number → English words expansion (cardinals, ordinals, years).

const ONES: [&str; 20] = [
    "zero",
    "one",
    "two",
    "three",
    "four",
    "five",
    "six",
    "seven",
    "eight",
    "nine",
    "ten",
    "eleven",
    "twelve",
    "thirteen",
    "fourteen",
    "fifteen",
    "sixteen",
    "seventeen",
    "eighteen",
    "nineteen",
];

const TENS: [&str; 10] = [
    "", "", "twenty", "thirty", "forty", "fifty", "sixty", "seventy", "eighty", "ninety",
];

/// Scale words for groups of three digits (index 1 = thousand, ...).
const SCALES: [&str; 7] = [
    "",
    "thousand",
    "million",
    "billion",
    "trillion",
    "quadrillion",
    "quintillion",
];

/// Words for 0..=99 ("twenty-one" style hyphenation).
fn under_hundred(n: u64) -> String {
    debug_assert!(n < 100);
    if n < 20 {
        ONES[n as usize].to_string()
    } else {
        let t = TENS[(n / 10) as usize];
        match n % 10 {
            0 => t.to_string(),
            o => format!("{t}-{}", ONES[o as usize]),
        }
    }
}

/// Words for 1..=999 (no "and": American style).
fn under_thousand(n: u64) -> String {
    debug_assert!(n > 0 && n < 1000);
    let h = n / 100;
    let r = n % 100;
    match (h, r) {
        (0, r) => under_hundred(r),
        (h, 0) => format!("{} hundred", ONES[h as usize]),
        (h, r) => format!("{} hundred {}", ONES[h as usize], under_hundred(r)),
    }
}

/// Cardinal number in words, e.g. `1234` → "one thousand two hundred thirty-four".
/// Supports the whole `u64` range.
pub fn number_to_words(n: u64) -> String {
    if n == 0 {
        return "zero".to_string();
    }
    let mut groups = Vec::new();
    let mut rest = n;
    while rest > 0 {
        groups.push(rest % 1000);
        rest /= 1000;
    }
    let mut parts: Vec<String> = Vec::new();
    for (i, &g) in groups.iter().enumerate().rev() {
        if g == 0 {
            continue;
        }
        let w = under_thousand(g);
        if i == 0 {
            parts.push(w);
        } else {
            parts.push(format!("{w} {}", SCALES[i]));
        }
    }
    parts.join(" ")
}

/// Turns the last word of a cardinal phrase into its ordinal form.
fn ordinalize_word(w: &str) -> String {
    match w {
        "one" => "first".into(),
        "two" => "second".into(),
        "three" => "third".into(),
        "five" => "fifth".into(),
        "eight" => "eighth".into(),
        "nine" => "ninth".into(),
        "twelve" => "twelfth".into(),
        _ => {
            if let Some(stem) = w.strip_suffix('y') {
                format!("{stem}ieth")
            } else {
                format!("{w}th")
            }
        }
    }
}

/// Ordinal number in words, e.g. `21` → "twenty-first".
pub fn ordinal_to_words(n: u64) -> String {
    let card = number_to_words(n);
    // The last word may be hyphenated ("twenty-one"): ordinalize only its last part.
    let split = card.rfind([' ', '-']).map(|i| i + 1).unwrap_or(0);
    let (head, last) = card.split_at(split);
    format!("{head}{}", ordinalize_word(last))
}

/// Year reading: 1999 → "nineteen ninety-nine", 2005 → "two thousand five",
/// 2024 → "twenty twenty-four", 1900 → "nineteen hundred", 1905 → "nineteen oh five".
pub fn year_to_words(y: u64) -> String {
    if !(1000..10000).contains(&y) {
        return number_to_words(y);
    }
    let hi = y / 100;
    let lo = y % 100;
    // 2000..=2009, 1000..=1009 etc.: "two thousand (five)".
    if hi.is_multiple_of(10) && lo < 10 {
        return number_to_words(y);
    }
    let hi_w = under_hundred(hi);
    match lo {
        0 => format!("{hi_w} hundred"),
        1..=9 => format!("{hi_w} oh {}", ONES[lo as usize]),
        _ => format!("{hi_w} {}", under_hundred(lo)),
    }
}

/// Pluralizes the final word of a phrase ("nineteen ninety" → "nineteen nineties").
pub fn pluralize_last(phrase: &str) -> String {
    let split = phrase.rfind([' ', '-']).map(|i| i + 1).unwrap_or(0);
    let (head, last) = phrase.split_at(split);
    let plural = if let Some(stem) = last.strip_suffix('y') {
        format!("{stem}ies")
    } else if last.ends_with('x') || last.ends_with('s') {
        format!("{last}es")
    } else {
        format!("{last}s")
    };
    format!("{head}{plural}")
}

/// Reads a string of digits one by one ("042" → "zero four two").
pub fn digits_to_words(digits: &str) -> String {
    digits
        .chars()
        .filter_map(|c| c.to_digit(10))
        .map(|d| ONES[d as usize])
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cardinals() {
        assert_eq!(number_to_words(0), "zero");
        assert_eq!(number_to_words(7), "seven");
        assert_eq!(number_to_words(21), "twenty-one");
        assert_eq!(number_to_words(100), "one hundred");
        assert_eq!(
            number_to_words(1234),
            "one thousand two hundred thirty-four"
        );
        assert_eq!(number_to_words(1_000_001), "one million one");
        assert_eq!(number_to_words(3_000_000_000_000), "three trillion");
        // Must not panic on the max value.
        assert!(number_to_words(u64::MAX).starts_with("eighteen quintillion"));
    }

    #[test]
    fn ordinals() {
        assert_eq!(ordinal_to_words(1), "first");
        assert_eq!(ordinal_to_words(2), "second");
        assert_eq!(ordinal_to_words(3), "third");
        assert_eq!(ordinal_to_words(12), "twelfth");
        assert_eq!(ordinal_to_words(20), "twentieth");
        assert_eq!(ordinal_to_words(21), "twenty-first");
        assert_eq!(ordinal_to_words(100), "one hundredth");
        assert_eq!(ordinal_to_words(105), "one hundred fifth");
    }

    #[test]
    fn years() {
        assert_eq!(year_to_words(1999), "nineteen ninety-nine");
        assert_eq!(year_to_words(2005), "two thousand five");
        assert_eq!(year_to_words(2000), "two thousand");
        assert_eq!(year_to_words(2024), "twenty twenty-four");
        assert_eq!(year_to_words(2010), "twenty ten");
        assert_eq!(year_to_words(1900), "nineteen hundred");
        assert_eq!(year_to_words(1905), "nineteen oh five");
        assert_eq!(pluralize_last(&year_to_words(1990)), "nineteen nineties");
    }
}
