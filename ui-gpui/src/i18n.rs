// Every word the window shows comes through here, the way I18n.qml does it:
// catalogues are JSON files, English the source every other one overlays;
// {placeholders} fill, plurals pick a form, and a missing key shows as
// itself, loudly.

use crate::json::Json;
use std::collections::HashMap;

#[derive(Clone, Debug, PartialEq)]
pub enum Msg {
    Text(String),
    Forms(Vec<(String, String)>),
}

#[derive(Clone, Debug)]
pub struct I18n {
    pub locale: String,
    pub language: String,
    pub rtl: bool,
    pub pseudo: bool,
    messages: HashMap<String, Msg>,
    dir: Option<std::path::PathBuf>,
}

impl Default for I18n {
    fn default() -> Self {
        I18n::load()
    }
}

// Where the catalogues live: WINKEL_UI first, then beside the binary, a
// capped walk up the tree to a checkout, a prefixed install, then the
// system path — the same walk the backend's paths.rs uses. The marker is
// en.json (the QML shell is gone).
pub fn find_ui_dir() -> Option<std::path::PathBuf> {
    if let Some(v) = std::env::var_os("WINKEL_UI") {
        if !v.is_empty() {
            return Some(std::path::PathBuf::from(v));
        }
    }
    let exe = std::env::current_exe().ok()?;
    let mut dir = exe.parent().map(std::path::PathBuf::from)?;
    if dir.join("ui").join("i18n").join("en.json").is_file() {
        return Some(dir.join("ui"));
    }
    for _ in 0..4 {
        let repo = dir.join("ui");
        if repo.join("i18n").join("en.json").is_file() {
            return Some(repo);
        }
        let installed = dir.join("share/winkel/ui");
        if installed.join("i18n").join("en.json").is_file() {
            return Some(installed);
        }
        if !dir.pop() {
            break;
        }
    }
    let system = std::path::PathBuf::from("/usr/share/winkel/ui");
    if system.join("i18n").join("en.json").is_file() {
        return Some(system);
    }
    None
}

impl I18n {
    // A catalogue read from an explicit directory: how the tests find the
    // checkout's ui/i18n.
    pub fn with_dir(dir: std::path::PathBuf) -> I18n {
        let locale = detect();
        let pseudo = locale == "pseudo" || locale == "pseudo-rtl";
        let language = if pseudo { "en".into() } else { locale.split('_').next().unwrap_or("en").to_string() };
        let mut i18n = I18n {
            locale,
            language,
            rtl: false,
            pseudo,
            messages: HashMap::new(),
            dir: Some(dir),
        };
        i18n.rebuild();
        i18n
    }

    pub fn load() -> Self {
        let locale = detect();
        let pseudo = locale == "pseudo" || locale == "pseudo-rtl";
        let language = if pseudo { "en".into() } else { locale.split('_').next().unwrap_or("en").to_string() };
        let rtl = locale == "pseudo-rtl" || (!pseudo && RTL_LANGUAGES.contains(&language.as_str()));

        let dir = find_ui_dir().map(|d| d.join("i18n"));

        let mut i18n = I18n {
            locale,
            language,
            rtl,
            pseudo,
            messages: HashMap::new(),
            dir,
        };
        i18n.rebuild();
        i18n
    }

    // The merged catalogue: English, overlaid by the language, overlaid by
    // the region. The pseudo locales read the English words through
    // pseudoize(), so only en.json loads for them.
    fn rebuild(&mut self) {
        self.messages.clear();
        let Some(dir) = self.dir.clone() else { return };
        let language_file = if !self.pseudo && self.language != "en" {
            Some(dir.join(format!("{}.json", self.language)))
        } else {
            None
        };
        let region_file = if !self.pseudo && self.locale.contains('_') {
            Some(dir.join(format!("{}.json", self.locale)))
        } else {
            None
        };

        for path in [Some(dir.join("en.json")), language_file, region_file].into_iter().flatten() {
            if let Ok(body) = std::fs::read_to_string(&path) {
                if let Ok(doc) = Json::parse(&body) {
                    if let Json::Obj(pairs) = doc {
                        for (k, v) in pairs {
                            self.messages.insert(k, to_msg(&v));
                        }
                    }
                }
            }
        }
    }

    pub fn has(&self, key: &str) -> bool {
        self.messages.contains_key(key)
    }

    // A message by key. args fill {placeholders}; args.count picks a plural
    // form, "=N" before the language's category, and args.form picks a named
    // form such as "all".
    pub fn tr(&self, key: &str, args: &[(&str, Arg)]) -> String {
        let entry = match self.messages.get(key) {
            Some(entry) => entry,
            None => return key.to_string(),
        };
        let text = match entry {
            Msg::Text(t) => t.clone(),
            Msg::Forms(forms) => pick_form(forms, args, self),
        };
        let text = if self.pseudo { pseudoize(&text, self.locale == "pseudo-rtl") } else { text };
        substitute(&text, args)
    }

    pub fn tr1(&self, key: &str) -> String {
        self.tr(key, &[])
    }

    // A key cap or shortcut: left to right in every language, so it sits in
    // a left-to-right isolate and "1 2 4 8" never turns around inside a
    // right-to-left sentence.
    pub fn shortcut(&self, key: &str) -> String {
        let text = self.tr1(key).trim_start_matches('\u{200f}').to_string();
        format!("\u{2066}{}\u{2069}", text)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Arg {
    Str(String),
    Num(f64),
    Count(f64),
    Form(String),
}

impl From<&str> for Arg {
    fn from(s: &str) -> Self {
        Arg::Str(s.into())
    }
}

impl From<String> for Arg {
    fn from(s: String) -> Self {
        Arg::Str(s)
    }
}

fn to_msg(v: &Json) -> Msg {
    match v {
        Json::Str(s) => Msg::Text(s.clone()),
        Json::Obj(pairs) => Msg::Forms(
            pairs
                .iter()
                .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                .collect(),
        ),
        _ => Msg::Text(String::new()),
    }
}

fn pick_form(forms: &[(String, String)], args: &[(&str, Arg)], i18n: &I18n) -> String {
    let mut form: Option<&str> = None;
    let mut count: Option<f64> = None;
    for (_, arg) in args {
        match arg {
            Arg::Form(f) => form = Some(f.as_str()),
            Arg::Count(c) => count = Some(*c),
            _ => {}
        }
    }
    if let Some(f) = form {
        if let Some((_, t)) = forms.iter().find(|(k, _)| k == f) {
            return t.clone();
        }
    }
    if let Some(n) = count {
        let exact = format!("={}", format_count(n));
        if let Some((_, t)) = forms.iter().find(|(k, _)| *k == exact) {
            return t.clone();
        }
        let category = plural_category(i18n, n);
        if let Some((_, t)) = forms.iter().find(|(k, _)| k == category) {
            return t.clone();
        }
    }
    forms
        .iter()
        .find(|(k, _)| k == "other")
        .map(|(_, t)| t.clone())
        .unwrap_or_default()
}

// The keys the QML builds for exact matches are written like "=2"; a count
// of 2.0 is "2", integral floats render bare.
fn format_count(n: f64) -> String {
    if n.fract() == 0.0 && n.abs() < 9_007_199_254_740_992.0 {
        format!("{}", n as i64)
    } else {
        format!("{}", n)
    }
}

fn substitute(text: &str, args: &[(&str, Arg)]) -> String {
    // The QML's one regex: {name} becomes the arg's word; an unknown or
    // missing name shows as itself. A walk over the chars keeps every
    // non-ASCII byte the message carries.
    let mut out = String::with_capacity(text.len());
    let mut chars = text.char_indices().peekable();
    while let Some((i, ch)) = chars.next() {
        if ch != '{' {
            out.push(ch);
            continue;
        }
        // Find the matching '}' on the same scan; { has no nesting here.
        let mut name = String::new();
        let mut closed = false;
        for (_, inner) in chars.by_ref() {
            if inner == '}' {
                closed = true;
                break;
            }
            name.push(inner);
        }
        if !closed {
            out.push('{');
            out.push_str(&name);
            continue;
        }
        let value = args.iter().find(|(k, _)| *k == name.as_str()).map(|(_, v)| match v {
            Arg::Str(s) => s.clone(),
            Arg::Num(n) | Arg::Count(n) => format_count(*n),
            Arg::Form(f) => f.clone(),
        });
        match value {
            Some(v) => out.push_str(&v),
            None => out.push_str(&format!("{{{}}}", name)),
        }
        let _ = i;
    }
    out
}

// CLDR's plural categories for whole numbers, by language, the same table
// the QML walks. A language not listed takes one and other.
fn plural_category(i18n: &I18n, n: f64) -> &'static str {
    let n = n as i64; // Winkel only ever counts whole things
    let mod10 = n % 10;
    let mod100 = n % 100;
    match i18n.language.as_str() {
        "ja" | "zh" | "ko" | "th" | "vi" | "id" | "ms" | "lo" | "my" | "km" => "other",
        "fr" | "hy" | "kab" => {
            if n == 0 || n == 1 { "one" } else { "other" }
        }
        "pt" => {
            if i18n.locale == "pt_PT" {
                if n == 1 { "one" } else { "other" }
            } else if n == 0 || n == 1 {
                "one"
            } else {
                "other"
            }
        }
        "ru" | "uk" | "be" => {
            if mod10 == 1 && mod100 != 11 {
                "one"
            } else if (2..=4).contains(&mod10) && !(12..=14).contains(&mod100) {
                "few"
            } else {
                "many"
            }
        }
        "hr" | "sr" | "bs" => {
            if mod10 == 1 && mod100 != 11 {
                "one"
            } else if (2..=4).contains(&mod10) && !(12..=14).contains(&mod100) {
                "few"
            } else {
                "other"
            }
        }
        "pl" => {
            if n == 1 {
                "one"
            } else if (2..=4).contains(&mod10) && !(12..=14).contains(&mod100) {
                "few"
            } else {
                "many"
            }
        }
        "cs" | "sk" => {
            if n == 1 {
                "one"
            } else if (2..=4).contains(&n) {
                "few"
            } else {
                "other"
            }
        }
        "sl" => match mod100 {
            1 => "one",
            2 => "two",
            3 | 4 => "few",
            _ => "other",
        },
        "lt" => {
            if mod10 == 1 && !(11..=19).contains(&mod100) {
                "one"
            } else if (2..=9).contains(&mod10) && !(11..=19).contains(&mod100) {
                "few"
            } else {
                "other"
            }
        }
        "lv" => {
            if mod10 == 0 || (11..=19).contains(&mod100) {
                "zero"
            } else if mod10 == 1 && mod100 != 11 {
                "one"
            } else {
                "other"
            }
        }
        "ro" => {
            if n == 1 {
                "one"
            } else if n == 0 || (2..=19).contains(&mod100) {
                "few"
            } else {
                "other"
            }
        }
        "ar" => {
            if n == 0 {
                "zero"
            } else if n == 1 {
                "one"
            } else if n == 2 {
                "two"
            } else if (3..=10).contains(&mod100) {
                "few"
            } else if mod100 >= 11 {
                "many"
            } else {
                "other"
            }
        }
        "he" => match n {
            1 => "one",
            2 => "two",
            _ => "other",
        },
        "ga" => match n {
            1 => "one",
            2 => "two",
            3..=6 => "few",
            7..=10 => "many",
            _ => "other",
        },
        "cy" => match n {
            0 => "zero",
            1 => "one",
            2 => "two",
            3 => "few",
            6 => "many",
            _ => "other",
        },
        _ => {
            if n == 1 {
                "one"
            } else {
                "other"
            }
        }
    }
}

// WINKEL_LANG wins, then the POSIX order: LANGUAGE's first entry when a
// real locale is set, LC_ALL, LC_MESSAGES, LANG.
fn detect() -> String {
    if let Ok(forced) = std::env::var("WINKEL_LANG") {
        if !forced.is_empty() {
            return normalize(&forced);
        }
    }
    let set = ["LC_ALL", "LC_MESSAGES", "LANG"]
        .iter()
        .find_map(|v| std::env::var(v).ok().filter(|s| !s.is_empty()))
        .unwrap_or_default();
    if let Ok(priority) = std::env::var("LANGUAGE") {
        if !priority.is_empty() && normalize(&set) != "en" {
            return normalize(priority.split(':').next().unwrap_or(""));
        }
    }
    normalize(&set)
}

// "pt_BR.UTF-8@euro" and "pt-br" are both pt_BR; C and POSIX are English.
fn normalize(name: &str) -> String {
    let name = name.split('.').next().unwrap_or("").split('@').next().unwrap_or("").replace('-', "_");
    if name == "pseudo" || name == "pseudo_rtl" {
        return name.replace('_', "-");
    }
    if name.is_empty() || name == "C" || name == "POSIX" {
        return "en".into();
    }
    let parts: Vec<&str> = name.split('_').collect();
    if parts.len() > 1 {
        format!("{}_{}", parts[0].to_lowercase(), parts[1].to_uppercase())
    } else {
        parts[0].to_lowercase()
    }
}

const RTL_LANGUAGES: &[&str] = &["ar", "he", "fa", "ur", "ps", "sd", "ug", "yi", "ckb", "dv"];

// Accents and lengthens the message's own text, never its placeholders,
// and brackets it so a clipped end is visible.
fn pseudoize(text: &str, rtl: bool) -> String {
    let map: &[(char, char)] = &[
        ('a', 'á'), ('e', 'é'), ('i', 'î'), ('o', 'ö'), ('u', 'ü'), ('c', 'ç'),
        ('n', 'ñ'), ('s', 'š'), ('y', 'ý'), ('A', 'Å'), ('E', 'É'), ('I', 'Î'),
        ('O', 'Ö'), ('U', 'Û'), ('C', 'Ç'), ('N', 'Ñ'), ('S', 'Š'), ('Y', 'Ý'),
    ];
    let mut out = String::new();
    let mut depth = 0usize;
    for ch in text.chars() {
        if ch == '{' {
            depth += 1;
        }
        if depth > 0 {
            out.push(ch);
        } else {
            out.push(map.iter().find(|(from, _)| *from == ch).map(|(_, to)| *to).unwrap_or(ch));
        }
        if ch == '}' && depth > 0 {
            depth -= 1;
        }
    }
    let pad = "·".repeat((text.len() as f64 * 0.3).ceil() as usize);
    let rtl_mark = if rtl { "\u{200f}" } else { "" };
    format!("{}⟦{}{}⟧", rtl_mark, out, pad)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_i18n() -> I18n {
        // Find the checkout's ui/i18n the way paths.rs would.
        let mut dir = std::env::current_exe().unwrap();
        for _ in 0..6 {
            dir.pop();
            let candidate = dir.join("ui/i18n");
            if candidate.join("en.json").is_file() {
                return I18n::with_dir(candidate);
            }
        }
        panic!("ui/i18n not found from the test binary");
    }

    #[test]
    fn substitutes_placeholders() {
        let i18n = test_i18n();
        assert_eq!(i18n.tr("a11y.timeSignature", &[("beats", Arg::Num(4.0)), ("unit", Arg::Num(4.0))]), "Time signature 4/4");
        assert_eq!(i18n.tr("error.wire.device_failed", &[("detail", Arg::Str("boom".into()))]), "The audio device failed (boom)");
    }

    #[test]
    fn plurals_pick_the_exact_form_first() {
        let i18n = test_i18n();
        assert_eq!(i18n.tr("note.sixteenth", &[("count", Arg::Count(2.0))]), "two sixteenths");
        assert_eq!(i18n.tr("note.sixteenth", &[("count", Arg::Count(1.0))]), "sixteenth");
        assert_eq!(i18n.tr("note.sixteenth", &[("count", Arg::Count(6.0)), ("form", Arg::Form("all".into()))]), "sixteenths");
        assert_eq!(i18n.tr("note.sixteenth", &[("count", Arg::Count(5.0))]), "five sixteenths");
    }

    #[test]
    fn a_missing_key_shows_itself() {
        let i18n = test_i18n();
        assert_eq!(i18n.tr1("no.such.key"), "no.such.key");
    }
}
