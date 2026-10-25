//! Article sanitization and quality control (pure functions, no I/O).

use crate::util::text::normalize;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FaqItem {
    pub q: String,
    pub a: String,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct QcReport {
    pub word_count: usize,
    pub max_similarity: f64,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

pub struct QcInput<'a> {
    pub content_html: &'a str,
    pub keyword: &'a str,
    /// Host of the project's own site (e.g. "example.com"); links to it are not counted as external.
    pub own_host: &'a str,
    pub min_words: usize,
    pub max_words: usize,
    pub faq_enabled: bool,
    pub faq: &'a [FaqItem],
    /// Keyword-stripped normalized texts of the other articles in the same campaign (see `strip_phrase`).
    pub others: &'a [String],
}

/// Cleans AI HTML down to a safe, WordPress-friendly subset. H1 is demoted to H2 because the
/// post title already is the H1.
pub fn sanitize_html(raw: &str) -> String {
    let demoted = raw
        .replace("<h1", "<h2")
        .replace("</h1", "</h2")
        .replace("<H1", "<h2")
        .replace("</H1", "</h2");
    let tags: HashSet<&str> = [
        "h2", "h3", "h4", "p", "ul", "ol", "li", "strong", "em", "b", "i", "a", "table", "thead", "tbody", "tr", "th",
        "td", "blockquote", "br", "code", "pre", "hr", "div",
    ]
    .into_iter()
    .collect();
    let mut attrs: HashMap<&str, HashSet<&str>> = HashMap::new();
    attrs.insert("a", ["href", "title"].into_iter().collect());
    attrs.insert("div", ["class"].into_iter().collect());
    let schemes: HashSet<&str> = ["http", "https", "mailto", "tel"].into_iter().collect();
    let mut b = ammonia::Builder::default();
    b.tags(tags)
        .tag_attributes(attrs)
        .url_schemes(schemes)
        .link_rel(Some("noopener noreferrer"))
        .strip_comments(true);
    b.clean(&demoted).to_string()
}

fn decode_entities(s: &str) -> String {
    s.replace("&nbsp;", " ")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&amp;", "&")
}

/// Turns HTML into plain text (tags become spaces).
pub fn plain_text(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut in_tag = false;
    for c in html.chars() {
        match c {
            '<' => {
                in_tag = true;
                out.push(' ');
            }
            '>' if in_tag => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    decode_entities(&out)
}

pub fn word_count(text: &str) -> usize {
    text.split_whitespace().filter(|w| w.chars().any(|c| c.is_alphanumeric())).count()
}

fn shingles(text: &str) -> HashSet<String> {
    let norm = normalize(text);
    let words: Vec<&str> = norm.split(' ').filter(|w| !w.is_empty()).collect();
    if words.len() < 5 {
        return words.iter().map(|w| w.to_string()).collect();
    }
    words.windows(5).map(|w| w.join(" ")).collect()
}

/// Share of 5-word phrases two texts have in common (Jaccard index).
pub fn shingle_similarity(a: &str, b: &str) -> f64 {
    let (sa, sb) = (shingles(a), shingles(b));
    if sa.is_empty() || sb.is_empty() {
        return 0.0;
    }
    let inter = sa.intersection(&sb).count() as f64;
    inter / ((sa.len() + sb.len()) as f64 - inter)
}

// Range instead of an exact count: models rarely hit an exact number, and exact-count failures wasted whole articles.
pub const KEYWORD_MIN_USES: usize = 10;
pub const KEYWORD_MAX_USES: usize = 40;
const LEAD_WORDS: usize = 150;
const PARAGRAPH_WARN_WORDS: usize = 100;
const PARAGRAPH_MAX_WORDS: usize = 150;
const MIN_EXTERNAL_LINKS: usize = 2;

/// Counts whole-phrase occurrences of `phrase` in `text`. Both sides are normalized, so Persian
/// letter variants and digits match, and "کلمه 1" does not match "کلمه 12".
pub fn count_phrase(text: &str, phrase: &str) -> usize {
    let norm_text = normalize(text);
    let norm_phrase = normalize(phrase);
    let t: Vec<&str> = norm_text.split(' ').filter(|w| !w.is_empty()).collect();
    let p: Vec<&str> = norm_phrase.split(' ').filter(|w| !w.is_empty()).collect();
    if p.is_empty() || t.len() < p.len() {
        return 0;
    }
    t.windows(p.len()).filter(|w| w.iter().eq(p.iter())).count()
}

/// Normalized text with every whole occurrence of `phrase` removed. Used before comparing two articles,
/// so that the focus keyword (repeated on purpose) does not hide near-duplicate content.
pub fn strip_phrase(text: &str, phrase: &str) -> String {
    let norm_text = normalize(text);
    let norm_phrase = normalize(phrase);
    let t: Vec<&str> = norm_text.split(' ').filter(|w| !w.is_empty()).collect();
    let p: Vec<&str> = norm_phrase.split(' ').filter(|w| !w.is_empty()).collect();
    if p.is_empty() {
        return t.join(" ");
    }
    let mut out = Vec::with_capacity(t.len());
    let mut i = 0;
    while i < t.len() {
        if i + p.len() <= t.len() && t[i..i + p.len()] == p[..] {
            i += p.len();
        } else {
            out.push(t[i]);
            i += 1;
        }
    }
    out.join(" ")
}

fn first_words(text: &str, n: usize) -> String {
    text.split_whitespace().take(n).collect::<Vec<_>>().join(" ")
}

/// Host of a site setting such as "example.com" or "https://www.example.com/", without "www.".
pub fn host_of(site: &str) -> String {
    let s = site.trim();
    if s.is_empty() {
        return String::new();
    }
    let full = if s.contains("://") { s.to_string() } else { format!("https://{s}") };
    url::Url::parse(&full)
        .ok()
        .and_then(|u| u.host_str().map(|h| h.trim_start_matches("www.").to_lowercase()))
        .unwrap_or_default()
}

fn hrefs(html: &str) -> Vec<String> {
    html.split("href=\"").skip(1).filter_map(|s| s.split('"').next()).map(str::to_string).collect()
}

/// An http(s) link to a host other than the project's own site (and its subdomains).
fn is_external(href: &str, own: &str) -> bool {
    let Ok(u) = url::Url::parse(href) else { return false };
    if !matches!(u.scheme(), "http" | "https") {
        return false;
    }
    let Some(h) = u.host_str() else { return false };
    let h = h.trim_start_matches("www.").to_lowercase();
    own.is_empty() || !(h == own || h.ends_with(&format!(".{own}")))
}

/// Word count of every <p> paragraph in the sanitized HTML.
fn paragraph_word_counts(html: &str) -> Vec<usize> {
    html.split("<p")
        .skip(1)
        .filter(|s| s.starts_with('>') || s.starts_with(' '))
        .map(|s| {
            let inner = s.split_once('>').map(|x| x.1).unwrap_or("");
            word_count(&plain_text(inner.split("</p").next().unwrap_or("")))
        })
        .collect()
}

pub fn validate(input: &QcInput) -> QcReport {
    let mut errors = Vec::new();
    let mut warnings = Vec::new();
    let html = input.content_html;
    let text = plain_text(html);
    let wc = word_count(&text);

    if html.trim().is_empty() || wc == 0 {
        errors.push("محتوای مقاله خالی است.".to_string());
    }

    // Length: LLMs routinely undershoot; tolerate 20% below the minimum.
    let floor = (input.min_words as f64 * 0.8) as usize;
    if wc > 0 && wc < floor {
        errors.push(format!("مقاله کوتاه است: {wc} کلمه (حداقل ≈ {floor} کلمه لازم است)."));
    } else if wc > 0 && wc < input.min_words {
        warnings.push(format!("طول مقاله ({wc}) کمی کمتر از حداقل قالب ({}) است.", input.min_words));
    }
    if wc > (input.max_words as f64 * 1.3) as usize {
        warnings.push(format!("مقاله بلندتر از حداکثر قالب است ({wc} کلمه)."));
    }

    // Headings.
    let h2 = html.matches("<h2").count();
    if wc > 0 && h2 < 2 {
        errors.push("ساختار عنوان‌ها نامناسب است: حداقل دو بخش H2 لازم است.".to_string());
    }
    if let (Some(i3), Some(i2)) = (html.find("<h3"), html.find("<h2")) {
        if i3 < i2 {
            errors.push("ساختار عنوان‌ها نامناسب است: H3 قبل از اولین H2 آمده است.".to_string());
        }
    } else if html.contains("<h3") && !html.contains("<h2") {
        errors.push("ساختار عنوان‌ها نامناسب است: H3 بدون H2.".to_string());
    }

    // Keyword presence.
    let kw = normalize(input.keyword);
    let norm_text = normalize(&text);
    if !kw.is_empty() && wc > 0 && !norm_text.contains(&kw) {
        let tokens: Vec<&str> = kw.split(' ').filter(|t| !t.is_empty()).collect();
        let hit = tokens.iter().filter(|t| norm_text.contains(**t)).count();
        if tokens.is_empty() || (hit as f64) < tokens.len() as f64 * 0.6 {
            errors.push(format!("کلمه‌ی کلیدی «{}» در متن استفاده نشده است.", input.keyword));
        } else {
            warnings.push(format!("عبارت دقیق «{}» در متن نیامده (فقط اجزای آن).", input.keyword));
        }
    }

    // FAQ.
    if input.faq_enabled {
        let usable = input.faq.iter().filter(|f| !f.q.trim().is_empty() && !f.a.trim().is_empty()).count();
        if usable == 0 {
            errors.push("بخش پرسش‌های متداول (FAQ) تولید نشده است.".to_string());
        }
    }

    // Focus keyword: the exact phrase must appear 27–32 times, and once in the first 150 words.
    let kw_norm = normalize(input.keyword);
    if wc > 0 && !kw_norm.is_empty() {
        let uses = count_phrase(&text, input.keyword);
        if uses < KEYWORD_MIN_USES || uses > KEYWORD_MAX_USES {
            errors.push(format!(
                "عبارت «{}» باید به‌صورت کامل بین {} تا {} بار در متن بیاید؛ الان {} بار است.",
                input.keyword, KEYWORD_MIN_USES, KEYWORD_MAX_USES, uses
            ));
        }
        if count_phrase(&first_words(&text, LEAD_WORDS), input.keyword) == 0 {
            errors.push(format!("عبارت «{}» باید حداقل یک بار در {} کلمه‌ی اول مقاله بیاید.", input.keyword, LEAD_WORDS));
        }
    }

    // Paragraphs: short paragraphs keep the article readable.
    if let Some(longest) = paragraph_word_counts(html).into_iter().max() {
        if longest > PARAGRAPH_MAX_WORDS {
            errors.push(format!("یک پاراگراف {longest} کلمه دارد؛ هر پاراگراف باید حداکثر {PARAGRAPH_MAX_WORDS} کلمه باشد."));
        } else if longest > PARAGRAPH_WARN_WORDS {
            warnings.push(format!("یک پاراگراف {longest} کلمه دارد؛ بهتر است کوتاه‌تر شود."));
        }
    }

    // External links to relevant sources (links to the project's own site do not count).
    if wc > 0 {
        let external = hrefs(html).iter().filter(|h| is_external(h, input.own_host)).count();
        if external < MIN_EXTERNAL_LINKS {
            errors.push(format!(
                "حداقل {MIN_EXTERNAL_LINKS} لینک خارجی به منابع معتبر و مرتبط لازم است؛ الان {external} لینک خارجی وجود دارد."
            ));
        }
    }

    // Duplicate content against the other articles of the campaign.
    let mut max_sim: f64 = 0.0;
    // `others` must be keyword-stripped (see `strip_phrase`); this article is stripped the same way.
    let own = strip_phrase(&text, input.keyword);
    for o in input.others {
        max_sim = max_sim.max(shingle_similarity(&own, o));
    }
    if max_sim >= 0.8 {
        errors.push(format!("محتوا تقریباً تکراری است ({}٪ شباهت با مقاله‌ی دیگر).", (max_sim * 100.0).round()));
    } else if max_sim >= 0.4 {
        warnings.push(format!("شباهت محتوایی بالا با مقاله‌ی دیگر ({}٪).", (max_sim * 100.0).round()));
    }

    QcReport { word_count: wc, max_similarity: (max_sim * 100.0).round() / 100.0, errors, warnings }
}

/// Persian-friendly slug: normalized letters/digits joined by hyphens, max 80 chars.
pub fn slugify(s: &str) -> String {
    let n = normalize(s).replace(' ', "-");
    let mut out: String = n.chars().take(80).collect();
    while out.ends_with('-') {
        out.pop();
    }
    out
}

pub fn truncate_chars(s: &str, max: usize) -> String {
    let s = s.trim();
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut t: String = s.chars().take(max.saturating_sub(1)).collect();
    t = t.trim_end().to_string();
    t.push('…');
    t
}

#[cfg(test)]
mod tests {
    use super::*;

    const KW: &str = "لپ تاپ دانشجویی";

    fn words(n: usize, seed: &str) -> String {
        (0..n).map(|i| format!("{seed}{i}")).collect::<Vec<_>>().join(" ")
    }

    fn input<'a>(html: &'a str, faq: &'a [FaqItem], others: &'a [String]) -> QcInput<'a> {
        QcInput { content_html: html, keyword: KW, own_host: "example.com", min_words: 100, max_words: 2000, faq_enabled: true, faq, others }
    }

    /// Body that uses the keyword `uses` times in short paragraphs, plus two external links.
    fn good_html(uses: usize) -> String {
        let mut h = String::from("<h2>الف</h2>");
        for i in 0..uses {
            if i == uses / 2 {
                h.push_str("<h2>ب</h2>");
            }
            h.push_str(&format!("<p>{KW} {}</p>", words(20, &format!("w{i}_"))));
        }
        h.push_str(r#"<p><a href="https://www.who.int/">منبع یک</a> و <a href="https://en.wikipedia.org/wiki/Laptop">منبع دو</a></p>"#);
        h
    }

    fn faq() -> Vec<FaqItem> {
        vec![FaqItem { q: "چرا؟".into(), a: "چون.".into() }]
    }

    #[test]
    fn sanitize_strips_dangerous_markup_and_demotes_h1() {
        let out = sanitize_html("<h1 onclick=\"x()\">T</h1><script>alert(1)</script><p style=\"c\">hi <img src=x onerror=y> <a href=\"javascript:bad()\">l</a><a href=\"https://a.com\">ok</a></p>");
        assert!(!out.contains("script") && !out.contains("onclick") && !out.contains("onerror") && !out.contains("<img"));
        assert!(!out.contains("javascript:") && !out.contains("style="));
        assert!(out.contains("<h2>T</h2>") && out.contains("https://a.com") && out.contains("noopener"));
    }

    #[test]
    fn accepts_a_good_article() {
        let f = faq();
        let r = validate(&input(&good_html(KEYWORD_MIN_USES), &f, &[]));
        assert!(r.errors.is_empty(), "{:?}", r.errors);
    }

    #[test]
    fn rejects_short_missing_keyword_bad_headings_no_faq() {
        let f = faq();
        let short = validate(&input("<h2>a</h2><h2>b</h2><p>لپ تاپ دانشجویی کوتاه</p>", &f, &[]));
        assert!(short.errors.iter().any(|e| e.contains("کوتاه")));

        let nokw = format!("<h2>الف</h2><p>{}</p><h2>ب</h2><p>{}</p>", words(60, "x"), words(60, "y"));
        assert!(validate(&input(&nokw, &f, &[])).errors.iter().any(|e| e.contains("در متن استفاده نشده")));

        let h3first = format!("<h3>z</h3><p>{KW} {}</p><h2>الف</h2><h2>ب</h2>", words(120, "q"));
        assert!(validate(&input(&h3first, &f, &[])).errors.iter().any(|e| e.contains("H3")));

        let noh2 = format!("<p>{KW} {}</p>", words(120, "w"));
        assert!(validate(&input(&noh2, &f, &[])).errors.iter().any(|e| e.contains("H2")));

        assert!(validate(&input(&good_html(KEYWORD_MIN_USES), &[], &[])).errors.iter().any(|e| e.contains("FAQ")));
        assert!(validate(&input("", &f, &[])).errors.iter().any(|e| e.contains("خالی")));
    }

    #[test]
    fn keyword_must_be_used_within_bounds_as_a_whole_phrase() {
        let f = faq();
        let few = validate(&input(&good_html(KEYWORD_MIN_USES - 1), &f, &[]));
        assert!(few.errors.iter().any(|e| e.contains(&format!("{KEYWORD_MIN_USES} تا {KEYWORD_MAX_USES}"))), "{:?}", few.errors);
        let many = validate(&input(&good_html(KEYWORD_MAX_USES + 1), &f, &[]));
        assert!(many.errors.iter().any(|e| e.contains(&format!("{KEYWORD_MIN_USES} تا {KEYWORD_MAX_USES}"))), "{:?}", many.errors);
        assert!(validate(&input(&good_html(KEYWORD_MAX_USES), &f, &[])).errors.is_empty());

        // Only the partial phrase is present: it does not count.
        let partial = good_html(KEYWORD_MIN_USES).replace(KW, "دانشجویی لپ");
        assert_eq!(count_phrase(&partial, KW), 0);
        assert_eq!(count_phrase("کلمه 1 کلمه 1 کلمه 12", "کلمه 1"), 2);
    }

    #[test]
    fn keyword_must_appear_in_the_first_150_words() {
        let f = faq();
        let late = format!("<p>{}</p>{}", words(200, "p"), good_html(KEYWORD_MIN_USES));
        let r = validate(&input(&late, &f, &[]));
        assert!(r.errors.iter().any(|e| e.contains("150 کلمه")), "{:?}", r.errors);
    }

    #[test]
    fn long_paragraphs_are_rejected() {
        let f = faq();
        let long = good_html(KEYWORD_MIN_USES).replacen("<p>", &format!("<p>{} ", words(160, "L")), 1);
        assert!(validate(&input(&long, &f, &[])).errors.iter().any(|e| e.contains("پاراگراف")));
    }

    #[test]
    fn needs_two_external_links_and_ignores_own_site() {
        let f = faq();
        let no_links = good_html(KEYWORD_MIN_USES).replace(r#"<a href="https://www.who.int/">منبع یک</a> و <a href="https://en.wikipedia.org/wiki/Laptop">منبع دو</a>"#, "");
        assert!(validate(&input(&no_links, &f, &[])).errors.iter().any(|e| e.contains("لینک خارجی")));

        let own = good_html(KEYWORD_MIN_USES).replace("https://www.who.int/", "https://www.example.com/a").replace("https://en.wikipedia.org/wiki/Laptop", "https://shop.example.com/b");
        assert!(validate(&input(&own, &f, &[])).errors.iter().any(|e| e.contains("لینک خارجی")));
    }

    #[test]
    fn duplicates_fail_and_different_text_passes() {
        let f = faq();
        let html = good_html(KEYWORD_MIN_USES);
        let others = vec![strip_phrase(&plain_text(&html), KW)];
        let dup = validate(&input(&html, &f, &others));
        assert!(dup.errors.iter().any(|e| e.contains("تکراری")), "{:?}", dup);
        let diff = vec![words(200, "zzz")];
        assert!(validate(&input(&html, &f, &diff)).errors.is_empty());
    }

    #[test]
    fn slug_truncate_and_host() {
        assert_eq!(slugify("بهترین لپ‌تاپ دانشجویی؟"), "بهترین-لپ-تاپ-دانشجویی");
        assert_eq!(slugify("Best Laptop!!"), "best-laptop");
        assert_eq!(truncate_chars("abcdef", 4), "abc…");
        assert_eq!(truncate_chars("abc", 4), "abc");
        assert_eq!(plain_text("<p>a</p><p>b &amp; c</p>").split_whitespace().collect::<Vec<_>>(), vec!["a", "b", "&", "c"]);
        assert_eq!(host_of("https://www.Example.com/"), "example.com");
        assert_eq!(host_of("example.com"), "example.com");
    }
}
