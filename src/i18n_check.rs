// The catalogues' guard, run by cargo test: every key the UI names exists
// in the English source, every catalogue is well formed and follows the
// source's placeholders, and no user-visible literal slips past i18n.tr.
// See docs/i18n.md.

use crate::backend::proto::ERROR_CODES;
use crate::json::Json;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

fn ui() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("ui")
}

fn ui_src() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("ui-gpui/src")
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {}", path.display(), e))
}

fn catalogue(path: &Path) -> Vec<(String, Json)> {
    match Json::parse(&read(path)) {
        Ok(Json::Obj(fields)) => fields,
        Ok(_) => panic!("{} is not an object", path.display()),
        Err(e) => panic!("{} is not JSON: {}", path.display(), e),
    }
}

fn rust_sources() -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = std::fs::read_dir(ui_src())
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "rs"))
        .collect();
    out.sort();
    out
}

// The {names} in a message.
fn placeholders(text: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let mut rest = text;
    while let Some(open) = rest.find('{') {
        let after = &rest[open + 1..];
        match after.find('}') {
            Some(close) => {
                let name = &after[..close];
                if !name.is_empty() && name.chars().all(|c| c.is_alphanumeric() || c == '_') {
                    out.insert(name.to_string());
                }
                rest = &after[close + 1..];
            }
            None => break,
        }
    }
    out
}

// Every string a message can render, and the placeholders across them.
fn forms(value: &Json) -> Result<Vec<(String, String)>, String> {
    match value {
        Json::Str(s) => Ok(vec![(String::new(), s.clone())]),
        Json::Obj(fields) => {
            let mut out = Vec::new();
            for (form, text) in fields {
                let known = ["zero", "one", "two", "few", "many", "other", "all"].contains(&form.as_str())
                    || (form.starts_with('=') && form[1..].parse::<u32>().is_ok());
                if !known {
                    return Err(format!("unknown plural form {}", form));
                }
                match text.as_str() {
                    Some(s) => out.push((form.clone(), s.to_string())),
                    None => return Err(format!("form {} is not a string", form)),
                }
            }
            if !fields.iter().any(|(f, _)| f == "other") {
                return Err("a plural message needs an other form".into());
            }
            Ok(out)
        }
        _ => Err("a message is a string or an object of plural forms".into()),
    }
}

// A translation against the source: no key the source lacks, the same kind
// of message, and no placeholder the source does not fill.
fn check_translation(source: &[(String, Json)], target: &[(String, Json)]) -> Vec<String> {
    let mut problems = Vec::new();
    for (key, value) in target {
        let Some((_, original)) = source.iter().find(|(k, _)| k == key) else {
            problems.push(format!("{}: not in the source catalogue", key));
            continue;
        };
        if matches!(original, Json::Obj(_)) != matches!(value, Json::Obj(_)) {
            problems.push(format!("{}: plural in one catalogue and not the other", key));
            continue;
        }
        let allowed: BTreeSet<String> = match forms(original) {
            Ok(f) => {
                let mut set: BTreeSet<String> = f.iter().flat_map(|(_, s)| placeholders(s)).collect();
                if matches!(original, Json::Obj(_)) {
                    set.insert("count".into());
                }
                set
            }
            Err(e) => {
                problems.push(format!("{}: source {}", key, e));
                continue;
            }
        };
        match forms(value) {
            Ok(f) => {
                for (form, text) in f {
                    for name in placeholders(&text) {
                        if !allowed.contains(&name) {
                            problems.push(format!("{} {}: {{{}}} is not filled by the UI", key, form, name));
                        }
                    }
                }
            }
            Err(e) => problems.push(format!("{}: {}", key, e)),
        }
    }
    problems
}

// Double-quoted literals on one line, escapes honoured.
fn literals(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut chars = line.chars();
    while let Some(c) = chars.next() {
        if c == '"' {
            let mut lit = String::new();
            let mut closed = false;
            while let Some(d) = chars.next() {
                match d {
                    '\\' => {
                        if let Some(e) = chars.next() {
                            lit.push(e);
                        }
                    }
                    '"' => {
                        closed = true;
                        break;
                    }
                    _ => lit.push(d),
                }
            }
            if closed {
                out.push(lit);
            }
        }
    }
    out
}

const NAMESPACES: [&str; 14] = [
    "app.", "tempo.", "main.", "timeSignature.", "subdivision.", "dialog.", "keys.", "keycap.", "a11y.",
    "voice.", "error.", "cell.", "list.", "rest.",
];

#[test]
fn every_key_the_ui_names_is_in_the_source_catalogue() {
    let source = catalogue(&ui().join("i18n/en.json"));
    let keys: BTreeSet<&str> = source.iter().map(|(k, _)| k.as_str()).collect();
    let mut missing = Vec::new();

    for file in rust_sources() {
        for (n, line) in read(&file).lines().enumerate() {
            if line.trim_start().starts_with("//") {
                continue;
            }
            for lit in literals(line) {
                // format!("error.wire.{}", code) is a key template, not a key.
                if lit.contains('{') {
                    continue;
                }
                let named = NAMESPACES.iter().any(|ns| lit.starts_with(ns)) || lit.starts_with("note.");
                let whole = !lit.ends_with('.') && !lit.contains(' ') && lit.contains('.');
                if named && whole && !keys.contains(lit.as_str()) {
                    missing.push(format!("{}:{} {}", file.display(), n + 1, lit));
                }
            }
        }
    }

    // Keys the UI builds rather than writes: one per wire error code, and
    // one per figure rhythm.rs can name.
    for code in ERROR_CODES {
        let key = format!("error.wire.{}", code);
        if !keys.contains(key.as_str()) {
            missing.push(key);
        }
    }
    const VALUE_KEYS: [&str; 6] = [
        "whole", "half", "quarter", "eighth", "sixteenth", "thirtySecond",
    ];
    for kind in ["note", "rest"] {
        for value in VALUE_KEYS {
            for dotted in ["", ".dotted"] {
                let key = format!("{}.{}{}", kind, value, dotted);
                if !keys.contains(key.as_str()) {
                    missing.push(key);
                }
            }
        }
    }

    assert!(missing.is_empty(), "keys missing from ui/i18n/en.json:\n{}", missing.join("\n"));
}

#[test]
fn every_catalogue_is_well_formed_and_follows_the_source() {
    let source = catalogue(&ui().join("i18n/en.json"));
    let mut problems = Vec::new();
    let mut seen = BTreeSet::new();
    for (key, value) in &source {
        if !seen.insert(key.clone()) {
            problems.push(format!("en {}: duplicate key", key));
        }
        if let Err(e) = forms(value) {
            problems.push(format!("en {}: {}", key, e));
        }
    }
    for entry in std::fs::read_dir(ui().join("i18n")).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_none_or(|x| x != "json") || path.file_name().unwrap() == "en.json" {
            continue;
        }
        let name = path.file_stem().unwrap().to_string_lossy().to_string();
        let valid = name.len() >= 2
            && name.split('_').next().unwrap().chars().all(|c| c.is_ascii_lowercase())
            && name.split('_').nth(1).is_none_or(|r| r.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit()));
        if !valid {
            problems.push(format!("{}: catalogues are named language or language_REGION", path.display()));
        }
        for p in check_translation(&source, &catalogue(&path)) {
            problems.push(format!("{} {}", name, p));
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

#[test]
fn the_translation_check_catches_what_it_should() {
    let source = match Json::parse(
        r#"{"a":"Hello {name}","b":{"one":"{count} beat","other":"{count} beats"}}"#,
    )
    .unwrap()
    {
        Json::Obj(f) => f,
        _ => unreachable!(),
    };
    let good = match Json::parse(r#"{"a":"Olá {name}","b":{"one":"{count} tempo","other":"{count} tempos","many":"{count}"}}"#).unwrap() {
        Json::Obj(f) => f,
        _ => unreachable!(),
    };
    assert!(check_translation(&source, &good).is_empty());
    let bad = match Json::parse(r#"{"a":"Olá {nome}","b":"tempos","c":"extra","d":{"one":"x"}}"#).unwrap() {
        Json::Obj(f) => f,
        _ => unreachable!(),
    };
    let problems = check_translation(&source, &bad);
    assert!(problems.iter().any(|p| p.contains("{nome}")), "{:?}", problems);
    assert!(problems.iter().any(|p| p.starts_with("b:")), "{:?}", problems);
    assert!(problems.iter().any(|p| p.starts_with("c:")), "{:?}", problems);
    assert!(problems.iter().any(|p| p.starts_with("d:")), "{:?}", problems);
}

#[test]
fn no_user_visible_literal_bypasses_the_catalogue() {
    // Words the UI shows on purpose without a catalogue: the digits a font
    // is measured with, and failure strings that are already catalogue keys
    // or protocol detail.
    let allowed = ["0123456789"];
    let source = catalogue(&ui().join("i18n/en.json"));
    let keys: BTreeSet<&str> = source.iter().map(|(k, _)| k.as_str()).collect();
    // Call sites that put a string on screen: tr/tr1 args are keys; bare
    // assignments to failure, or format strings with alphabetic content
    // that are not keys, are the slips.
    let mut found = Vec::new();
    for file in rust_sources() {
        for (n, line) in read(&file).lines().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("//") {
                continue;
            }
            // A bare string assigned to failure (not through i18n.tr / tr1).
            let assigns_literal = (trimmed.contains("failure = \"")
                || trimmed.contains(".failure = \""))
                && !trimmed.contains(".tr(")
                && !trimmed.contains(".tr1(");
            if !assigns_literal {
                continue;
            }
            for lit in literals(trimmed) {
                if lit.chars().any(|c| c.is_alphabetic())
                    && !allowed.contains(&lit.as_str())
                    && !keys.contains(lit.as_str())
                    && !lit.contains('{')
                {
                    found.push(format!("{}:{} \"{}\"", file.display(), n + 1, lit));
                }
            }
        }
    }
    assert!(found.is_empty(), "user-visible text outside the catalogue:\n{}", found.join("\n"));
}
