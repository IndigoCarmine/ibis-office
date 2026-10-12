// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! A small JSON reader for the grid manifest (no dependency needed for it).

use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq)]
pub enum Json {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Array(Vec<Json>),
    Object(BTreeMap<String, Json>),
}

impl Json {
    pub fn get(&self, key: &str) -> Option<&Json> {
        match self {
            Json::Object(map) => map.get(key),
            _ => None,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Json::Number(n) => Some(*n),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Json::String(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_array(&self) -> Option<&[Json]> {
        match self {
            Json::Array(a) => Some(a),
            _ => None,
        }
    }
}

/// Parse a JSON text.
pub fn parse(text: &str) -> Result<Json, String> {
    let mut p = Parser { s: text.as_bytes(), i: 0 };
    let value = p.value()?;
    p.ws();
    if p.i != p.s.len() {
        return Err(p.err("extra characters"));
    }
    Ok(value)
}

struct Parser<'a> {
    s: &'a [u8],
    i: usize,
}

impl Parser<'_> {
    fn err(&self, message: &str) -> String {
        format!("JSON, byte {}: {message}", self.i)
    }

    fn ws(&mut self) {
        while self.i < self.s.len() && matches!(self.s[self.i], b' ' | b'\t' | b'\n' | b'\r') {
            self.i += 1;
        }
    }

    fn eat(&mut self, word: &str) -> bool {
        if self.s[self.i..].starts_with(word.as_bytes()) {
            self.i += word.len();
            true
        } else {
            false
        }
    }

    fn value(&mut self) -> Result<Json, String> {
        self.ws();
        match self.s.get(self.i) {
            None => Err(self.err("a value is missing")),
            Some(b'{') => {
                self.i += 1;
                let mut map = BTreeMap::new();
                self.ws();
                if self.eat("}") {
                    return Ok(Json::Object(map));
                }
                loop {
                    self.ws();
                    let key = match self.value()? {
                        Json::String(k) => k,
                        _ => return Err(self.err("a key must be a string")),
                    };
                    self.ws();
                    if !self.eat(":") {
                        return Err(self.err("expected ':'"));
                    }
                    let v = self.value()?;
                    map.insert(key, v);
                    self.ws();
                    if self.eat(",") {
                        continue;
                    }
                    if self.eat("}") {
                        return Ok(Json::Object(map));
                    }
                    return Err(self.err("expected ',' or '}'"));
                }
            }
            Some(b'[') => {
                self.i += 1;
                let mut items = Vec::new();
                self.ws();
                if self.eat("]") {
                    return Ok(Json::Array(items));
                }
                loop {
                    items.push(self.value()?);
                    self.ws();
                    if self.eat(",") {
                        continue;
                    }
                    if self.eat("]") {
                        return Ok(Json::Array(items));
                    }
                    return Err(self.err("expected ',' or ']'"));
                }
            }
            Some(b'"') => {
                self.i += 1;
                let mut out = String::new();
                loop {
                    let Some(&c) = self.s.get(self.i) else { return Err(self.err("unterminated string")) };
                    self.i += 1;
                    match c {
                        b'"' => return Ok(Json::String(out)),
                        b'\\' => {
                            let Some(&e) = self.s.get(self.i) else { return Err(self.err("incomplete escape")) };
                            self.i += 1;
                            match e {
                                b'"' => out.push('"'),
                                b'\\' => out.push('\\'),
                                b'/' => out.push('/'),
                                b'b' => out.push('\u{8}'),
                                b'f' => out.push('\u{c}'),
                                b'n' => out.push('\n'),
                                b'r' => out.push('\r'),
                                b't' => out.push('\t'),
                                b'u' => {
                                    let hex = |p: &mut Self| -> Result<u32, String> {
                                        let h = p.s.get(p.i..p.i + 4).ok_or_else(|| p.err("incomplete \\u escape"))?;
                                        p.i += 4;
                                        u32::from_str_radix(std::str::from_utf8(h).unwrap_or(""), 16).map_err(|_| p.err("invalid \\u escape"))
                                    };
                                    let mut code = hex(self)?;
                                    if (0xd800..0xdc00).contains(&code) && self.eat("\\u") {
                                        let low = hex(self)?;
                                        code = 0x10000 + ((code - 0xd800) << 10) + (low - 0xdc00);
                                    }
                                    out.push(char::from_u32(code).unwrap_or('\u{fffd}'));
                                }
                                _ => return Err(self.err("unknown escape")),
                            }
                        }
                        _ => {
                            // Copy the UTF-8 sequence as it is.
                            let start = self.i - 1;
                            let len = match c {
                                0x00..=0x7f => 1,
                                0xc0..=0xdf => 2,
                                0xe0..=0xef => 3,
                                _ => 4,
                            };
                            self.i = start + len;
                            out.push_str(std::str::from_utf8(&self.s[start..self.i.min(self.s.len())]).map_err(|_| self.err("not UTF-8"))?);
                        }
                    }
                }
            }
            Some(b't') if self.eat("true") => Ok(Json::Bool(true)),
            Some(b'f') if self.eat("false") => Ok(Json::Bool(false)),
            Some(b'n') if self.eat("null") => Ok(Json::Null),
            Some(_) => {
                let start = self.i;
                while self.i < self.s.len() && matches!(self.s[self.i], b'-' | b'+' | b'.' | b'e' | b'E' | b'0'..=b'9') {
                    self.i += 1;
                }
                let text = std::str::from_utf8(&self.s[start..self.i]).unwrap_or("");
                text.parse().map(Json::Number).map_err(|_| self.err("invalid value"))
            }
        }
    }
}

/// A string as a JSON string literal.
pub fn quote(s: &str) -> String {
    let mut out = String::from("\"");
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
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_nested_values() {
        let v = parse(r#"{"a": [1, 2.5, -3e2], "b": {"c": "xあ\n"}, "d": true, "e": null}"#).unwrap();
        assert_eq!(v.get("a").unwrap().as_array().unwrap()[2].as_f64(), Some(-300.0));
        assert_eq!(v.get("b").unwrap().get("c").unwrap().as_str(), Some("xあ\n"));
        assert_eq!(v.get("d"), Some(&Json::Bool(true)));
        assert!(parse("{\"a\": }").is_err());
        assert_eq!(parse("\"日本\"").unwrap().as_str(), Some("日本"));
    }
}
