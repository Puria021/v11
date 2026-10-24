//! Persian-aware text helpers: normalization, similarity, intent heuristics.

use std::collections::HashSet;

fn map_char(c: char) -> char {
    match c {
        'ي' | 'ى' => 'ی',
        'ك' => 'ک',
        'ۀ' | 'ة' => 'ه',
        'أ' | 'إ' | 'ٱ' => 'ا',
        '۰'..='۹' => char::from_digit(c as u32 - '۰' as u32, 10).unwrap(),
        '٠'..='٩' => char::from_digit(c as u32 - '٠' as u32, 10).unwrap(),
        _ => c,
    }
}

/// Lowercases, unifies Arabic/Persian letter variants and digits, drops diacritics/tatweel,
/// turns ZWNJ and punctuation into single spaces.
pub fn normalize(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut last_space = true;
    for c in s.chars() {
        let c = map_char(c);
        if ('\u{064B}'..='\u{065F}').contains(&c) || c == '\u{0670}' || c == '\u{0640}' {
            continue;
        }
        if c.is_alphanumeric() {
            out.extend(c.to_lowercase());
            last_space = false;
        } else if !last_space {
            out.push(' ');
            last_space = true;
        }
    }
    out.trim().to_string()
}

pub fn trigrams(norm: &str) -> HashSet<String> {
    let chars: Vec<char> = norm.chars().collect();
    let mut set = HashSet::new();
    if chars.len() < 3 {
        if !chars.is_empty() {
            set.insert(norm.to_string());
        }
        return set;
    }
    for w in chars.windows(3) {
        set.insert(w.iter().collect());
    }
    set
}

pub fn jaccard(a: &HashSet<String>, b: &HashSet<String>) -> f64 {
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    let inter = a.intersection(b).count() as f64;
    let union = (a.len() + b.len()) as f64 - inter;
    inter / union
}

/// Similarity of two raw strings in [0, 1] (character-trigram Jaccard on normalized text).
pub fn similarity(a: &str, b: &str) -> f64 {
    jaccard(&trigrams(&normalize(a)), &trigrams(&normalize(b)))
}

pub const INTENTS: [&str; 4] = ["informational", "commercial", "transactional", "navigational"];

pub fn valid_intent(s: &str) -> Option<&'static str> {
    let s = s.trim().to_lowercase();
    INTENTS.iter().copied().find(|i| *i == s)
}

fn has_token(tokens: &[&str], set: &[&str]) -> bool {
    tokens.iter().any(|t| set.contains(t))
}

/// Rule-based search-intent guess. It is a heuristic, not an oracle — the UI lets users see it
/// and the AI may supply its own label.
pub fn guess_intent(keyword: &str) -> &'static str {
    let norm = normalize(keyword);
    let tokens: Vec<&str> = norm.split(' ').collect();
    const INFO: &[&str] = &[
        "چیست", "چگونه", "چطور", "چرا", "آموزش", "راهنما", "راهنمای", "علت", "تفاوت", "آیا", "چه", "what", "how", "why",
        "guide", "tutorial",
    ];
    const TRANS: &[&str] = &[
        "خرید", "قیمت", "فروش", "سفارش", "ارزان", "تخفیف", "خریدار", "buy", "price", "order", "discount", "cheap",
    ];
    const COMM: &[&str] = &[
        "بهترین", "بهتر", "مقایسه", "بررسی", "معرفی", "مدل", "برند", "نقد", "کدام", "best", "top", "vs", "review",
        "compare",
    ];
    const NAV: &[&str] = &["ورود", "لاگین", "login", "signin", "official", "رسمی"];
    if has_token(&tokens, INFO) {
        "informational"
    } else if has_token(&tokens, TRANS) {
        "transactional"
    } else if has_token(&tokens, COMM) {
        "commercial"
    } else if has_token(&tokens, NAV) {
        "navigational"
    } else {
        "informational"
    }
}

pub fn language_name(code: &str) -> String {
    match code.to_lowercase().as_str() {
        "fa" => "Persian (Farsi)".into(),
        "en" => "English".into(),
        "ar" => "Arabic".into(),
        "tr" => "Turkish".into(),
        "de" => "German".into(),
        "fr" => "French".into(),
        "es" => "Spanish".into(),
        other => other.to_string(),
    }
}

/// Converts Persian/Arabic digits to ASCII, removes thousands separators, parses a number.
pub fn parse_number(s: &str) -> Option<f64> {
    let cleaned: String = s
        .trim()
        .chars()
        .map(map_char)
        .filter(|c| !matches!(c, ',' | '٬' | ' ' | '\u{200c}'))
        .map(|c| if c == '٫' { '.' } else { c })
        .collect();
    if cleaned.is_empty() {
        return None;
    }
    cleaned.parse::<f64>().ok().filter(|v| v.is_finite())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_persian_variants() {
        assert_eq!(normalize("لپ‌تاپ  دانشجويي!"), "لپ تاپ دانشجویی");
        assert_eq!(normalize("خرید ۱۲۳"), "خرید 123");
        assert_eq!(normalize(" Best  Laptop, 2026 "), "best laptop 2026");
    }

    #[test]
    fn similarity_behaves() {
        assert!(similarity("خرید لپ تاپ دانشجویی", "خرید لپ‌تاپ دانشجويي") > 0.99);
        assert!(similarity("بهترین لپ تاپ دانشجویی", "بهترین لپ تاپ دانشجویی ارزان") > 0.6);
        assert!(similarity("آموزش پخت کیک", "قیمت گوشی سامسونگ") < 0.15);
    }

    #[test]
    fn intent_guess() {
        assert_eq!(guess_intent("لپ تاپ دانشجویی چیست"), "informational");
        assert_eq!(guess_intent("خرید لپ تاپ دانشجویی"), "transactional");
        assert_eq!(guess_intent("بهترین لپ تاپ دانشجویی"), "commercial");
        assert_eq!(guess_intent("how to choose a laptop"), "informational");
        assert_eq!(guess_intent("ورود به پنل"), "navigational");
    }

    #[test]
    fn numbers() {
        assert_eq!(parse_number("۱٬۲۰۰"), Some(1200.0));
        assert_eq!(parse_number("1,200"), Some(1200.0));
        assert_eq!(parse_number("0.35"), Some(0.35));
        assert_eq!(parse_number("abc"), None);
        assert_eq!(parse_number(""), None);
    }
}
