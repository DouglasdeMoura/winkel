// The wire format is one JSON object per line, both ways. serde would buy a
// parse for types this protocol never carries, at the cost of a dependency
// tree the backend does not want; this module is the whole of what Winkel needs.

#[derive(Debug, Clone, PartialEq)]
pub enum Json {
    Null,
    Bool(bool),
    Num(f64),
    Str(String),
    Arr(Vec<Json>),
    Obj(Vec<(String, Json)>),
}

impl Json {
    pub fn get(&self, key: &str) -> Option<&Json> {
        match self {
            Json::Obj(pairs) => pairs.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Json::Num(n) if n.is_finite() => Some(*n),
            _ => None,
        }
    }

    pub fn as_u32(&self) -> Option<u32> {
        match self {
            Json::Num(n) if *n >= 0.0 && *n <= u32::MAX as f64 && n.fract() == 0.0 => {
                Some(*n as u32)
            }
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Json::Str(s) => Some(s),
            _ => None,
        }
    }

    pub fn str(s: &str) -> Json {
        Json::Str(s.to_string())
    }

    pub fn int(n: i64) -> Json {
        Json::Num(n as f64)
    }

    pub fn obj(pairs: Vec<(&str, Json)>) -> Json {
        Json::Obj(pairs.into_iter().map(|(k, v)| (k.to_string(), v)).collect())
    }

    // One line, no trailing newline. Integral floats render bare ("120", not
    // "120.0"), so a line round-trips through jq and the eye alike.
    pub fn render(&self) -> String {
        let mut out = String::new();
        self.write(&mut out);
        out
    }

    fn write(&self, out: &mut String) {
        match self {
            Json::Null => out.push_str("null"),
            Json::Bool(true) => out.push_str("true"),
            Json::Bool(false) => out.push_str("false"),
            Json::Num(n) => {
                if n.fract() == 0.0 && n.abs() < 9_007_199_254_740_992.0 {
                    out.push_str(&format!("{}", *n as i64));
                } else if n.is_finite() {
                    out.push_str(&format!("{}", n));
                } else {
                    out.push_str("null");
                }
            }
            Json::Str(s) => write_string(s, out),
            Json::Arr(items) => {
                out.push('[');
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    item.write(out);
                }
                out.push(']');
            }
            Json::Obj(pairs) => {
                out.push('{');
                for (i, (k, v)) in pairs.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    write_string(k, out);
                    out.push(':');
                    v.write(out);
                }
                out.push('}');
            }
        }
    }

    pub fn parse(input: &str) -> Result<Json, String> {
        let bytes = input.as_bytes();
        let mut p = Parser { bytes, pos: 0 };
        p.skip_ws();
        let value = p.value(0)?;
        p.skip_ws();
        if p.pos != bytes.len() {
            return Err(format!("trailing bytes at {}", p.pos));
        }
        Ok(value)
    }
}

fn write_string(s: &str, out: &mut String) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
}

struct Parser<'a> {
    bytes: &'a [u8],
    pos: usize,
}

const MAX_DEPTH: usize = 64;

impl<'a> Parser<'a> {
    fn skip_ws(&mut self) {
        while let Some(b) = self.bytes.get(self.pos) {
            match b {
                b' ' | b'\t' | b'\n' | b'\r' => self.pos += 1,
                _ => break,
            }
        }
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    fn expect(&mut self, b: u8) -> Result<(), String> {
        if self.peek() == Some(b) {
            self.pos += 1;
            Ok(())
        } else {
            Err(format!("expected '{}' at {}", b as char, self.pos))
        }
    }

    fn value(&mut self, depth: usize) -> Result<Json, String> {
        if depth > MAX_DEPTH {
            return Err("nesting too deep".to_string());
        }
        match self.peek() {
            Some(b'{') => self.object(depth),
            Some(b'[') => self.array(depth),
            Some(b'"') => Ok(Json::Str(self.string()?)),
            Some(b't') => self.literal("true", Json::Bool(true)),
            Some(b'f') => self.literal("false", Json::Bool(false)),
            Some(b'n') => self.literal("null", Json::Null),
            Some(b) if b == b'-' || b.is_ascii_digit() => self.number(),
            Some(b) => Err(format!("unexpected '{}' at {}", b as char, self.pos)),
            None => Err("unexpected end of input".to_string()),
        }
    }

    fn literal(&mut self, word: &str, value: Json) -> Result<Json, String> {
        if self.bytes[self.pos..].starts_with(word.as_bytes()) {
            self.pos += word.len();
            Ok(value)
        } else {
            Err(format!("bad literal at {}", self.pos))
        }
    }

    fn object(&mut self, depth: usize) -> Result<Json, String> {
        self.expect(b'{')?;
        let mut pairs = Vec::new();
        self.skip_ws();
        if self.peek() == Some(b'}') {
            self.pos += 1;
            return Ok(Json::Obj(pairs));
        }
        loop {
            self.skip_ws();
            let key = self.string()?;
            self.skip_ws();
            self.expect(b':')?;
            self.skip_ws();
            let value = self.value(depth + 1)?;
            pairs.push((key, value));
            self.skip_ws();
            match self.peek() {
                Some(b',') => self.pos += 1,
                Some(b'}') => {
                    self.pos += 1;
                    return Ok(Json::Obj(pairs));
                }
                _ => return Err(format!("expected ',' or '}}' at {}", self.pos)),
            }
        }
    }

    fn array(&mut self, depth: usize) -> Result<Json, String> {
        self.expect(b'[')?;
        let mut items = Vec::new();
        self.skip_ws();
        if self.peek() == Some(b']') {
            self.pos += 1;
            return Ok(Json::Arr(items));
        }
        loop {
            self.skip_ws();
            items.push(self.value(depth + 1)?);
            self.skip_ws();
            match self.peek() {
                Some(b',') => self.pos += 1,
                Some(b']') => {
                    self.pos += 1;
                    return Ok(Json::Arr(items));
                }
                _ => return Err(format!("expected ',' or ']' at {}", self.pos)),
            }
        }
    }

    fn string(&mut self) -> Result<String, String> {
        self.expect(b'"')?;
        let mut out = String::new();
        loop {
            match self.peek() {
                None => return Err("unterminated string".to_string()),
                Some(b'"') => {
                    self.pos += 1;
                    return Ok(out);
                }
                Some(b'\\') => {
                    self.pos += 1;
                    // Every arm consumes the one character it names; the \u
                    // arm consumes its four hex digits through hex4.
                    match self.peek() {
                        Some(b'"') => {
                            out.push('"');
                            self.pos += 1;
                        }
                        Some(b'\\') => {
                            out.push('\\');
                            self.pos += 1;
                        }
                        Some(b'/') => {
                            out.push('/');
                            self.pos += 1;
                        }
                        Some(b'b') => {
                            out.push('\u{0008}');
                            self.pos += 1;
                        }
                        Some(b'f') => {
                            out.push('\u{000C}');
                            self.pos += 1;
                        }
                        Some(b'n') => {
                            out.push('\n');
                            self.pos += 1;
                        }
                        Some(b'r') => {
                            out.push('\r');
                            self.pos += 1;
                        }
                        Some(b't') => {
                            out.push('\t');
                            self.pos += 1;
                        }
                        Some(b'u') => {
                            self.pos += 1;
                            let hi = self.hex4()?;
                            let cp = if (0xD800..0xDC00).contains(&hi) {
                                // A high surrogate must be followed by \uDC00..\uDFFF to name
                                // one code point; anything else stays a replacement character.
                                if self.bytes[self.pos..].starts_with(b"\\u") {
                                    self.pos += 2;
                                    let lo = self.hex4()?;
                                    if (0xDC00..0xE000).contains(&lo) {
                                        0x10000 + ((hi - 0xD800) << 10) + (lo - 0xDC00)
                                    } else {
                                        self.pos -= 6;
                                        0xFFFD
                                    }
                                } else {
                                    0xFFFD
                                }
                            } else if (0xDC00..0xE000).contains(&hi) {
                                0xFFFD
                            } else {
                                hi
                            };
                            out.push(char::from_u32(cp).unwrap_or('\u{FFFD}'));
                        }
                        _ => return Err(format!("bad escape at {}", self.pos)),
                    }
                }
                Some(b) if b < 0x20 => return Err(format!("unescaped control at {}", self.pos)),
                Some(_) => {
                    // Copy a complete unescaped span once, avoiding repeated
                    // UTF-8 validation of the remaining string for every char.
                    let start = self.pos;
                    while self
                        .peek()
                        .is_some_and(|b| b >= 0x20 && b != b'"' && b != b'\\')
                    {
                        self.pos += 1;
                    }
                    let text = std::str::from_utf8(&self.bytes[start..self.pos])
                        .map_err(|_| "bad utf-8".to_string())?;
                    out.push_str(text);
                }
            }
        }
    }

    fn hex4(&mut self) -> Result<u32, String> {
        if self.pos + 4 > self.bytes.len() {
            return Err("truncated \\u escape".to_string());
        }
        let s = std::str::from_utf8(&self.bytes[self.pos..self.pos + 4])
            .map_err(|_| "bad \\u escape".to_string())?;
        if !s.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err("bad unicode escape".into());
        }
        let v = u32::from_str_radix(s, 16).map_err(|_| "bad \\u escape".to_string())?;
        self.pos += 4;
        Ok(v)
    }

    fn number(&mut self) -> Result<Json, String> {
        let start = self.pos;
        if self.peek() == Some(b'-') {
            self.pos += 1;
        }
        match self.peek() {
            Some(b'0') => self.pos += 1,
            Some(b'1'..=b'9') => {
                while self.peek().is_some_and(|b| b.is_ascii_digit()) {
                    self.pos += 1;
                }
            }
            _ => return Err(format!("bad number at {}", start)),
        }
        if self.peek() == Some(b'.') {
            self.pos += 1;
            let digits = self.pos;
            while self.peek().is_some_and(|b| b.is_ascii_digit()) {
                self.pos += 1;
            }
            if self.pos == digits {
                return Err(format!("bad number at {}", start));
            }
        }
        if matches!(self.peek(), Some(b'e') | Some(b'E')) {
            self.pos += 1;
            if matches!(self.peek(), Some(b'+') | Some(b'-')) {
                self.pos += 1;
            }
            let digits = self.pos;
            while self.peek().is_some_and(|b| b.is_ascii_digit()) {
                self.pos += 1;
            }
            if self.pos == digits {
                return Err(format!("bad number at {}", start));
            }
        }
        let text = std::str::from_utf8(&self.bytes[start..self.pos]).unwrap();
        let number = text
            .parse::<f64>()
            .map_err(|_| format!("bad number at {}", start))?;
        if !number.is_finite() {
            return Err(format!("number out of range at {}", start));
        }
        Ok(Json::Num(number))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unicode_escape_requires_four_hex_digits() {
        assert!(Json::parse(r#""\u+001""#).is_err());
    }

    #[test]
    fn rejects_invalid_numbers_and_unescaped_controls() {
        for input in ["01", "-01", "1.", "-.5", "1.e2", "1e999", "\"a\n b\""] {
            assert!(Json::parse(input).is_err(), "accepted {input:?}");
        }
        for input in ["0", "-0", "0.5", "-0.5", "1e+2"] {
            assert!(Json::parse(input).is_ok(), "rejected {input}");
        }
    }

    #[test]
    fn malformed_surrogate_does_not_swallow_the_next_escape() {
        assert_eq!(
            Json::parse(r#""\ud800\u0041""#).unwrap().as_str(),
            Some("\u{FFFD}A")
        );
    }

    #[test]
    fn round_trips_a_protocol_shaped_object() {
        let line = r#"{"t":"beat","beat":0,"sub":2,"kind":"sub","rate":48.5}"#;
        let parsed = Json::parse(line).unwrap();
        assert_eq!(parsed.get("kind").unwrap().as_str(), Some("sub"));
        assert_eq!(parsed.get("beat").unwrap().as_u32(), Some(0));
        assert_eq!(parsed.get("rate").unwrap().as_f64(), Some(48.5));
        let rendered = parsed.render();
        let again = Json::parse(&rendered).unwrap();
        assert_eq!(parsed, again);
    }

    #[test]
    fn integral_floats_render_bare() {
        assert_eq!(Json::Num(120.0).render(), "120");
        assert_eq!(Json::Num(0.8).render(), "0.8");
        assert_eq!(Json::Num(-3.0).render(), "-3");
    }

    #[test]
    fn unicode_escapes_and_multibyte_survive() {
        let parsed = Json::parse(r#""é😀""#).unwrap();
        assert_eq!(parsed.as_str(), Some("é😀"));
        let escaped = Json::parse(r#""\u00e9""#).unwrap();
        assert_eq!(escaped.as_str(), Some("é"));
        assert_eq!(Json::Str("é".into()).render(), "\"é\"");
    }

    #[test]
    fn rejects_garbage() {
        assert!(Json::parse("").is_err());
        assert!(Json::parse("{").is_err());
        assert!(Json::parse("{\"a\":}").is_err());
        assert!(Json::parse("[1,]").is_err());
        assert!(Json::parse("nul").is_err());
        assert!(Json::parse("{} {}").is_err());
        assert!(Json::parse("\"\\q\"").is_err());
    }

    #[test]
    fn nested_arrays_and_whitespace() {
        let parsed = Json::parse("  { \"a\" : [ 1 , 2 , { \"b\" : null } ] }  ").unwrap();
        assert_eq!(
            parsed.get("a").unwrap(),
            &Json::Arr(vec![
                Json::Num(1.0),
                Json::Num(2.0),
                Json::Obj(vec![("b".into(), Json::Null)]),
            ])
        );
    }

    #[test]
    fn lone_surrogate_becomes_replacement() {
        let parsed = Json::parse(r#""\ud83d""#).unwrap();
        assert_eq!(parsed.as_str(), Some("\u{FFFD}"));
    }
}
