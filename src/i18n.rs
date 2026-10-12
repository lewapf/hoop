//! Translations. The English text is the key, gettext style: code writes
//! `t!("Save as JEF…")` and gets the English back until a catalog for the
//! chosen language has that line. Catalogs are gettext `.po` files, so
//! translators can use Poedit or any PO editor; `locales/hoop.pot` lists
//! every line (refreshed by `cargo test --release update_pot -- --ignored`).
//!
//! Placeholders are named, `t!("Fits the {hoop}", hoop = name)`, so a
//! translation can move them. `{}` takes the next argument and `{0}`, `{1}`
//! pick one by position. Precision, width and sign work as in `format!`:
//! `{w:.1}`, `{n:>5}`, `{dx:+.1}`.
//!
//! Catalogs come bundled from `locales/` and can be added or overridden in
//! ~/.local/share/hoop/locale/<language>.po. To bundle a finished one,
//! add `("pl", include_str!("../locales/pl.po"))` to `BUNDLED`.

use std::collections::HashMap;
use std::fmt::Display;
use std::path::PathBuf;
use std::sync::{LazyLock, RwLock};

/// Catalogs built into the program: (language code, PO text).
const BUNDLED: &[(&str, &str)] = &[("de", include_str!("../locales/de.po")), ("pl", include_str!("../locales/pl.po"))];

static CATALOG: LazyLock<RwLock<HashMap<&'static str, &'static str>>> = LazyLock::new(|| RwLock::new(HashMap::new()));
/// Plural lines: the English singular → each plural form of the language.
static PLURALS: LazyLock<RwLock<HashMap<&'static str, Vec<&'static str>>>> = LazyLock::new(|| RwLock::new(HashMap::new()));
static LANGUAGE: RwLock<String> = RwLock::new(String::new());

/// Translates a line, or returns it unchanged.
pub fn tr(english: &'static str) -> &'static str {
    CATALOG.read().ok().and_then(|c| c.get(english).copied()).unwrap_or(english)
}

/// Which plural form `n` takes in `lang`, following gettext's Plural-Forms
/// for the common languages.
pub fn plural_index(lang: &str, n: u64) -> usize {
    let base = lang.split('_').next().unwrap_or("");
    let (m10, m100) = (n % 10, n % 100);
    match base {
        "ja" | "zh" | "ko" | "vi" | "th" | "id" | "ms" => 0,
        "fr" | "pt_BR" => (n > 1) as usize,
        "pl" => {
            if n == 1 {
                0
            } else if (2..=4).contains(&m10) && !(12..=14).contains(&m100) {
                1
            } else {
                2
            }
        }
        "ru" | "uk" | "be" | "sr" | "hr" | "bs" => {
            if m10 == 1 && m100 != 11 {
                0
            } else if (2..=4).contains(&m10) && !(12..=14).contains(&m100) {
                1
            } else {
                2
            }
        }
        "cs" | "sk" => match n {
            1 => 0,
            2..=4 => 1,
            _ => 2,
        },
        _ => (n != 1) as usize,
    }
}

/// The translated form of a line with a count in it.
pub fn plural(n: u64, one: &'static str, other: &'static str) -> &'static str {
    let lang = language();
    if let Some(forms) = PLURALS.read().ok().and_then(|p| p.get(one).cloned()) {
        if let Some(f) = forms.get(plural_index(&lang, n)).or(forms.last()) {
            if !f.is_empty() {
                return f;
            }
        }
    }
    if n == 1 { one } else { other }
}

/// Translates a line that isn't `'static`, such as one from a table
/// passed around by reference.
pub fn tr_any(english: &str) -> String {
    CATALOG.read().ok().and_then(|c| c.get(english).map(|s| s.to_string())).unwrap_or_else(|| english.to_string())
}

/// The language in use, like "pl", or "" for English.
pub fn language() -> String {
    LANGUAGE.read().map(|l| l.clone()).unwrap_or_default()
}

/// The language the desktop asks for: LANGUAGE, LC_ALL, LC_MESSAGES, LANG.
pub fn system_language() -> String {
    for var in ["LANGUAGE", "LC_ALL", "LC_MESSAGES", "LANG"] {
        if let Ok(v) = std::env::var(var) {
            let first = v.split(':').next().unwrap_or("");
            let code: String = first.split(['.', '@']).next().unwrap_or("").to_string();
            if !code.is_empty() && code != "C" && code != "POSIX" {
                return code;
            }
        }
    }
    String::new()
}

fn user_dir() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_DATA_HOME").map(PathBuf::from).or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))?;
    Some(base.join("hoop/locale"))
}

/// Languages with a catalog, as (code, name in that language).
pub fn available() -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = BUNDLED.iter().map(|(c, po)| (c.to_string(), language_name(c, po))).collect();
    if let Some(dir) = user_dir() {
        if let Ok(rd) = std::fs::read_dir(dir) {
            for e in rd.flatten() {
                let p = e.path();
                if p.extension().is_some_and(|x| x == "po") {
                    let code = p.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
                    if !out.iter().any(|(c, _)| *c == code) {
                        let text = std::fs::read_to_string(&p).unwrap_or_default();
                        out.push((code.clone(), language_name(&code, &text)));
                    }
                }
            }
        }
    }
    out.sort();
    out
}

/// The "Language-Name" header of a catalog, else its code.
fn language_name(code: &str, po: &str) -> String {
    po.lines()
        .find_map(|l| l.trim().trim_matches('"').strip_prefix("Language-Name:").map(|v| v.trim().trim_end_matches("\\n").trim().to_string()))
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| code.to_string())
}

/// Switches to `code` ("" or "en" for English, "system" for the desktop's
/// language). "pl_PL" falls back to "pl". Returns the language loaded.
pub fn set_language(code: &str) -> String {
    let code = if code == "system" { system_language() } else { code.to_string() };
    let mut candidates = vec![code.clone()];
    if let Some((base, _)) = code.split_once('_') {
        candidates.push(base.to_string());
    }
    let mut map: HashMap<&'static str, &'static str> = HashMap::new();
    let mut plurals: HashMap<&'static str, Vec<&'static str>> = HashMap::new();
    let mut loaded = String::new();
    for c in candidates {
        if c.is_empty() || c == "en" || c.starts_with("en_") {
            break;
        }
        let mut text = BUNDLED.iter().find(|(b, _)| *b == c).map(|(_, po)| po.to_string());
        if let Some(t) = user_dir().and_then(|d| std::fs::read_to_string(d.join(format!("{c}.po"))).ok()) {
            text = Some(match text {
                // The user's catalog wins line by line over the bundled one.
                Some(b) => format!("{b}\n{t}"),
                None => t,
            });
        }
        if let Some(t) = text {
            for e in parse_po_entries(&t) {
                let key: &'static str = Box::leak(e.id.into_boxed_str());
                if e.plural {
                    if e.forms.iter().any(|f| !f.is_empty()) {
                        plurals.insert(key, e.forms.into_iter().map(|f| &*Box::leak(f.into_boxed_str())).collect());
                    }
                } else if let Some(v) = e.forms.into_iter().next().filter(|v| !v.is_empty()) {
                    map.insert(key, Box::leak(v.into_boxed_str()));
                }
            }
            loaded = c;
            break;
        }
    }
    if let Ok(mut cat) = CATALOG.write() {
        *cat = map;
    }
    if let Ok(mut p) = PLURALS.write() {
        *p = plurals;
    }
    if let Ok(mut l) = LANGUAGE.write() {
        *l = loaded.clone();
    }
    loaded
}

fn unquote(s: &str) -> String {
    let s = s.trim();
    let inner = s.strip_prefix('"').and_then(|s| s.strip_suffix('"')).unwrap_or("");
    let mut out = String::new();
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('"') => out.push('"'),
            Some('\\') => out.push('\\'),
            Some(o) => {
                out.push('\\');
                out.push(o);
            }
            None => {}
        }
    }
    out
}

#[cfg(test)]
fn quote(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// One catalog entry: the English line, whether it has plural forms, and
/// the translations (one, or one per plural form).
#[derive(Debug, PartialEq)]
pub struct Entry {
    pub id: String,
    pub plural: bool,
    pub forms: Vec<String>,
}

/// Entries of a PO file. Fuzzy entries are skipped, as gettext does, and
/// so is the header (empty msgid).
pub fn parse_po_entries(text: &str) -> Vec<Entry> {
    let mut out = Vec::new();
    let mut cur: Option<Entry> = None;
    let mut fuzzy = false;
    // What continuation lines add to: 0 msgid, 1 msgid_plural, 2 + k msgstr[k].
    let mut field = 0usize;
    let finish = |cur: &mut Option<Entry>, fuzzy: bool, out: &mut Vec<Entry>| {
        if let Some(e) = cur.take() {
            if !e.id.is_empty() && !fuzzy {
                out.push(e);
            }
        }
    };
    for line in text.lines() {
        let l = line.trim();
        if l.starts_with('#') {
            if l.starts_with("#,") && l.contains("fuzzy") {
                finish(&mut cur, fuzzy, &mut out);
                fuzzy = true;
            }
            continue;
        }
        if let Some(rest) = l.strip_prefix("msgid_plural ") {
            if let Some(e) = &mut cur {
                e.plural = true;
            }
            let _ = unquote(rest);
            field = 1;
        } else if let Some(rest) = l.strip_prefix("msgid ") {
            if cur.as_ref().is_some_and(|e| !e.forms.is_empty()) {
                finish(&mut cur, fuzzy, &mut out);
                fuzzy = false;
            }
            cur = Some(Entry { id: unquote(rest), plural: false, forms: Vec::new() });
            field = 0;
        } else if let Some(rest) = l.strip_prefix("msgstr") {
            let (k, value) = match rest.strip_prefix('[') {
                Some(r) => {
                    let (num, v) = r.split_once(']').unwrap_or(("0", ""));
                    (num.trim().parse::<usize>().unwrap_or(0), v)
                }
                None => (0, rest),
            };
            if let Some(e) = &mut cur {
                while e.forms.len() <= k {
                    e.forms.push(String::new());
                }
                e.forms[k] = unquote(value);
            }
            field = 2 + k;
        } else if l.starts_with('"') {
            let more = unquote(l);
            if let Some(e) = &mut cur {
                match field {
                    0 => e.id.push_str(&more),
                    1 => {}
                    k => {
                        if let Some(f) = e.forms.get_mut(k - 2) {
                            f.push_str(&more);
                        }
                    }
                }
            }
        } else if l.is_empty() && cur.as_ref().is_some_and(|e| !e.forms.is_empty()) {
            finish(&mut cur, fuzzy, &mut out);
            fuzzy = false;
        }
    }
    finish(&mut cur, fuzzy, &mut out);
    out
}

#[cfg(test)]
/// msgid → msgstr pairs of the single-form entries.
pub fn parse_po(text: &str) -> Vec<(String, String)> {
    parse_po_entries(text).into_iter().filter(|e| !e.plural).map(|e| (e.id, e.forms.into_iter().next().unwrap_or_default())).collect()
}

/// Fills placeholders in a translated template. `args` are (name, value);
/// positional arguments are named "0", "1", ….
pub fn fill(template: &str, args: &[(&str, &dyn Display)]) -> String {
    let mut out = String::with_capacity(template.len() + 16);
    let mut next = 0usize;
    let b = template.as_bytes();
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if c == b'{' && b.get(i + 1) == Some(&b'{') {
            out.push('{');
            i += 2;
            continue;
        }
        if c == b'}' && b.get(i + 1) == Some(&b'}') {
            out.push('}');
            i += 2;
            continue;
        }
        if c == b'{' {
            if let Some(end) = template[i..].find('}') {
                let inner = &template[i + 1..i + end];
                let (name, spec) = inner.split_once(':').unwrap_or((inner, ""));
                let key = if name.is_empty() {
                    next += 1;
                    (next - 1).to_string()
                } else {
                    name.to_string()
                };
                match args.iter().find(|(n, _)| *n == key) {
                    Some((_, v)) => out.push_str(&apply(*v, spec)),
                    None => out.push_str(&template[i..=i + end]),
                }
                i += end + 1;
                continue;
            }
        }
        let ch = template[i..].chars().next().unwrap();
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

/// Formats one value with a `format!` spec limited to sign, alignment,
/// width and precision.
fn apply(v: &dyn Display, spec: &str) -> String {
    let mut s = spec;
    let mut align = ' ';
    if s.starts_with(['<', '>', '^']) {
        align = s.chars().next().unwrap();
        s = &s[1..];
    }
    let plus = s.starts_with('+');
    if plus {
        s = &s[1..];
    }
    let (w, p) = s.split_once('.').map_or((s, None), |(w, p)| (w, p.parse::<usize>().ok()));
    let width = w.parse::<usize>().unwrap_or(0);
    let body = match (plus, p) {
        (false, None) => format!("{v}"),
        (false, Some(p)) => format!("{v:.p$}"),
        (true, None) => format!("{v:+}"),
        (true, Some(p)) => format!("{v:+.p$}"),
    };
    let n = body.chars().count();
    if n >= width {
        return body;
    }
    let pad = " ".repeat(width - n);
    match align {
        '<' => body + &pad,
        '^' => {
            let l = (width - n) / 2;
            format!("{}{body}{}", " ".repeat(l), " ".repeat(width - n - l))
        }
        _ if align == '>' || !body.starts_with(|c: char| c.is_alphabetic()) => pad + &body,
        _ => body + &pad,
    }
}

/// Translated text: `t!("Open")` gives a `&'static str`; with arguments,
/// `t!("{n} stitches", n = count)` or `t!("{} stitches", count)`, a String.
#[macro_export]
macro_rules! t {
    ($s:literal) => {
        $crate::i18n::tr($s)
    };
    ($s:literal, $($name:ident = $val:expr),+ $(,)?) => {
        $crate::i18n::fill($crate::i18n::tr($s), &[$((stringify!($name), &$val as &dyn std::fmt::Display)),+])
    };
    ($s:literal, $($val:expr),+ $(,)?) => {{
        let vals: &[&dyn std::fmt::Display] = &[$(&$val),+];
        let names: Vec<String> = (0..vals.len()).map(|i| i.to_string()).collect();
        let args: Vec<(&str, &dyn std::fmt::Display)> = names.iter().map(|n| n.as_str()).zip(vals.iter().copied()).collect();
        $crate::i18n::fill($crate::i18n::tr($s), &args)
    }};
}

/// Translated text with a count: `tn!(n, "{n} problem", "{n} problems")`
/// picks the plural form of the language for `n`; `{n}` is the count, and
/// more named arguments may follow.
#[macro_export]
macro_rules! tn {
    ($n:expr, $one:literal, $other:literal $(, $name:ident = $val:expr)* $(,)?) => {{
        let n = $n;
        $crate::i18n::fill($crate::i18n::plural(n as u64, $one, $other), &[("n", &n as &dyn std::fmt::Display) $(, (stringify!($name), &$val as &dyn std::fmt::Display))*])
    }};
}

/// Marks a line for translation without translating it here, for tables
/// that are translated where they are shown (with `tr`).
#[macro_export]
macro_rules! n {
    ($s:literal) => {
        $s
    };
}

#[cfg(test)]
/// Reads the string literal starting just after an opening quote; returns
/// its text and the rest after the closing quote.
fn literal(rest: &str) -> (String, &str) {
    let mut s = String::new();
    let mut chars = rest.char_indices();
    while let Some((k, c)) = chars.next() {
        match c {
            '\\' => {
                if let Some((_, e)) = chars.next() {
                    match e {
                        'n' => s.push('\n'),
                        't' => s.push('\t'),
                        '\n' => {
                            while chars.clone().next().is_some_and(|(_, c)| c == ' ') {
                                chars.next();
                            }
                        }
                        o => s.push(o),
                    }
                }
            }
            '"' => return (s, &rest[k + 1..]),
            c => s.push(c),
        }
    }
    (s, "")
}

#[cfg(test)]
/// Every `t!`, `n!` and `tn!` line in the sources, for the .pot template,
/// as (line, plural line).
pub fn extract(src: &str) -> Vec<(String, Option<String>)> {
    let mut out: Vec<(String, Option<String>)> = Vec::new();
    let mut rest = src;
    while let Some(at) = rest.find("!(") {
        let head = &rest[..at];
        let name_start = head.rfind(|c: char| !(c.is_alphanumeric() || c == '_')).map_or(0, |i| i + 1);
        let name = &head[name_start..];
        rest = &rest[at + 2..];
        match name {
            "t" | "n" => {
                if let Some(r) = rest.strip_prefix('"') {
                    let (s, after) = literal(r);
                    rest = after;
                    if !out.iter().any(|e| e.0 == s) {
                        out.push((s, None));
                    }
                }
            }
            "tn" => {
                // tn!(count, "one", "other", …): the first two literals.
                let Some(q) = rest.find('"') else { continue };
                let (one, after) = literal(&rest[q + 1..]);
                let Some(q2) = after.find('"') else { continue };
                let (other, after2) = literal(&after[q2 + 1..]);
                rest = after2;
                if !out.iter().any(|e| e.0 == one) {
                    out.push((one, Some(other)));
                }
            }
            _ => {}
        }
    }
    out
}

#[cfg(test)]
/// A .pot template for `lines`.
pub fn pot(lines: &[(String, Option<String>)]) -> String {
    let mut out = String::from(
        "# Hoop translation template.\n# Copy to <language>.po (for example pl.po), fill in each msgstr and set\n# Language and Language-Name. Lines left empty stay in English.\nmsgid \"\"\nmsgstr \"\"\n\"Content-Type: text/plain; charset=UTF-8\\n\"\n\"Language: \\n\"\n\"Language-Name: \\n\"\n\n",
    );
    for (l, plural) in lines {
        match plural {
            None => out.push_str(&format!("msgid {}\nmsgstr \"\"\n\n", quote(l))),
            Some(p) => out.push_str(&format!("msgid {}\nmsgid_plural {}\nmsgstr[0] \"\"\nmsgstr[1] \"\"\n\n", quote(l), quote(p))),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fills_named_positional_and_formatted_placeholders() {
        let w = 12.345f32;
        assert_eq!(fill("About {w:.1} mm, {n} rows", &[("w", &w), ("n", &3)]), "About 12.3 mm, 3 rows");
        assert_eq!(fill("{1} before {0}", &[("0", &"a"), ("1", &"b")]), "b before a");
        assert_eq!(fill("{} and {}", &[("0", &1), ("1", &2)]), "1 and 2");
        assert_eq!(fill("{dx:+.1} mm", &[("dx", &2.0f32)]), "+2.0 mm");
        assert_eq!(fill("ST:{n:>7}|{{x}}", &[("n", &42)]), "ST:     42|{x}");
        // A placeholder without an argument is left for the reader to see.
        assert_eq!(fill("{missing} ok", &[]), "{missing} ok");
        assert_eq!(fill("Größe {w:.0}", &[("w", &5.0f32)]), "Größe 5");
    }

    #[test]
    fn macro_returns_static_text_and_formats_arguments() {
        let s: &'static str = t!("Open");
        assert_eq!(s, "Open");
        let n = 5;
        assert_eq!(t!("{n} stitches", n = n), "5 stitches");
        assert_eq!(t!("{} of {}", 1, 2), "1 of 2");
    }

    #[test]
    fn reads_po_files() {
        let po = "msgid \"\"\nmsgstr \"\"\n\"Language: pl\\n\"\n\"Language-Name: Polski\\n\"\n\n#: src/app.rs\nmsgid \"Open\"\nmsgstr \"Otwórz\"\n\nmsgid \"\"\n\"Two \"\n\"lines\"\nmsgstr \"Dwie \"\n\"linie\"\n\n#, fuzzy\nmsgid \"Save\"\nmsgstr \"Zapisz?\"\n\nmsgid \"Quote \\\"x\\\"\"\nmsgstr \"\"\n";
        let p = parse_po(po);
        assert_eq!(p, vec![("Open".into(), "Otwórz".into()), ("Two lines".into(), "Dwie linie".into()), ("Quote \"x\"".into(), String::new())]);
        assert_eq!(language_name("pl", po), "Polski");
    }

    #[test]
    fn bundled_catalogs_load_and_keep_placeholders() {
        for (code, po) in BUNDLED {
            let entries = parse_po_entries(po);
            assert!(entries.len() > 700, "{code}: {}", entries.len());
            let ph = |s: &str| {
                let mut v: Vec<String> = s.split('{').skip(1).filter_map(|p| p.split_once('}').map(|x| x.0.to_string())).collect();
                v.sort();
                v
            };
            for e in entries.iter().filter(|e| !e.plural && !e.forms[0].is_empty()) {
                assert_eq!(ph(&e.id), ph(&e.forms[0]), "{code}: {}", e.id);
            }
        }
        let pl = parse_po_entries(BUNDLED.iter().find(|b| b.0 == "pl").unwrap().1);
        assert!(pl.iter().any(|e| e.id == "Open…" && e.forms[0] == "Otwórz…"));
        let st = pl.iter().find(|e| e.id == "{n} stitch").unwrap();
        assert_eq!(st.forms[plural_index("pl", 5)], "{n} ściegów");
        let de = parse_po_entries(BUNDLED.iter().find(|b| b.0 == "de").unwrap().1);
        let st = de.iter().find(|e| e.id == "{n} stitch").unwrap();
        assert_eq!(st.forms[plural_index("de", 1)], "{n} Stich");
    }

    #[test]
    fn plural_forms_follow_the_language() {
        assert_eq!([1, 2, 5, 22, 25].map(|n| plural_index("pl", n)), [0, 1, 2, 1, 2]);
        assert_eq!([1, 2, 5, 21].map(|n| plural_index("ru", n)), [0, 1, 2, 0]);
        assert_eq!([0, 1, 2].map(|n| plural_index("en", n)), [1, 0, 1]);
        let po = "msgid \"{n} problem\"\nmsgid_plural \"{n} problems\"\nmsgstr[0] \"{n} problem\"\nmsgstr[1] \"{n} problemy\"\nmsgstr[2] \"{n} problemów\"\n";
        let e = parse_po_entries(po);
        assert_eq!(e.len(), 1);
        assert!(e[0].plural);
        assert_eq!(e[0].forms[2], "{n} problemów");
        assert_eq!(tn!(3, "{n} problem", "{n} problems"), "3 problems");
    }

    #[test]
    fn extracts_lines_from_sources() {
        let src = r#"let a = t!("Open"); let b = t!("{n} rows", n = 3); print!("no"); n!("Table line"); t!("Open");"#;
        let names: Vec<String> = extract(src).into_iter().map(|e| e.0).collect();
        assert_eq!(names, vec!["Open", "{n} rows", "Table line"]);
        let p = extract(r#"tn!(count, "{n} problem", "{n} problems", x = 1)"#);
        assert_eq!(p, vec![("{n} problem".to_string(), Some("{n} problems".to_string()))]);
    }

    /// Rewrites locales/hoop.pot from every t! and n! line in src/.
    #[test]
    #[ignore]
    fn update_pot() {
        let mut lines = Vec::new();
        let mut files: Vec<PathBuf> = Vec::new();
        let mut stack = vec![PathBuf::from("src")];
        while let Some(d) = stack.pop() {
            for e in std::fs::read_dir(&d).unwrap().flatten() {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                } else if p.extension().is_some_and(|x| x == "rs") && !p.ends_with("i18n.rs") {
                    files.push(p);
                }
            }
        }
        files.sort();
        for f in files {
            let src = std::fs::read_to_string(&f).unwrap();
            // Tests aren't shown to anyone.
            let src = src.split("#[cfg(test)]").next().unwrap_or("");
            for l in extract(src) {
                if !lines.iter().any(|e: &(String, Option<String>)| e.0 == l.0) {
                    lines.push(l);
                }
            }
        }
        std::fs::create_dir_all("locales").unwrap();
        std::fs::write("locales/hoop.pot", pot(&lines)).unwrap();
        eprintln!("{} lines", lines.len());
    }
}
