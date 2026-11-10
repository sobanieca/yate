//! A small JSON parser for `yate.json`. It also accepts `//` and `/* */`
//! comments and trailing commas, and reports errors with line and column.

#[derive(Clone, Debug, PartialEq)]
pub enum Json {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Array(Vec<Json>),
    /// Keys in file order; a later duplicate key wins.
    Object(Vec<(String, Json)>),
}

impl Json {
    pub fn get(&self, key: &str) -> Option<&Json> {
        match self {
            Json::Object(entries) => entries.iter().rev().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Json::String(text) => Some(text),
            _ => None,
        }
    }

    pub fn entries(&self) -> &[(String, Json)] {
        match self {
            Json::Object(entries) => entries,
            _ => &[],
        }
    }

    pub fn type_name(&self) -> &'static str {
        match self {
            Json::Null => "null",
            Json::Bool(_) => "a boolean",
            Json::Number(_) => "a number",
            Json::String(_) => "a string",
            Json::Array(_) => "an array",
            Json::Object(_) => "an object",
        }
    }

    /// Merges `other` into `self`: objects merge key by key, recursively;
    /// any other value is replaced.
    pub fn merge(&mut self, other: Json) {
        match (self, other) {
            (Json::Object(base), Json::Object(overrides)) => {
                for (key, value) in overrides {
                    match base.iter_mut().find(|(k, _)| *k == key) {
                        Some((_, existing)) => existing.merge(value),
                        None => base.push((key, value)),
                    }
                }
            }
            (base, other) => *base = other,
        }
    }
}

pub fn parse(text: &str) -> Result<Json, String> {
    let mut parser = Parser {
        chars: text.chars().collect(),
        pos: 0,
    };
    let value = parser.value()?;
    parser.skip_whitespace()?;
    if parser.pos < parser.chars.len() {
        return Err(parser.error("unexpected text after the value"));
    }
    Ok(value)
}

struct Parser {
    chars: Vec<char>,
    pos: usize,
}

impl Parser {
    fn error(&self, message: &str) -> String {
        let before = &self.chars[..self.pos.min(self.chars.len())];
        let line = before.iter().filter(|c| **c == '\n').count() + 1;
        let column = before.iter().rev().take_while(|c| **c != '\n').count() + 1;
        format!("line {line}, column {column}: {message}")
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn skip_whitespace(&mut self) -> Result<(), String> {
        loop {
            match (self.peek(), self.chars.get(self.pos + 1)) {
                (Some(c), _) if c.is_whitespace() => self.pos += 1,
                (Some('/'), Some('/')) => {
                    while self.peek().is_some_and(|c| c != '\n') {
                        self.pos += 1;
                    }
                }
                (Some('/'), Some('*')) => {
                    let start = self.pos;
                    self.pos += 2;
                    loop {
                        match (self.peek(), self.chars.get(self.pos + 1)) {
                            (Some('*'), Some('/')) => {
                                self.pos += 2;
                                break;
                            }
                            (Some(_), _) => self.pos += 1,
                            (None, _) => {
                                self.pos = start;
                                return Err(self.error("unclosed /* comment"));
                            }
                        }
                    }
                }
                _ => return Ok(()),
            }
        }
    }

    fn expect(&mut self, c: char) -> Result<(), String> {
        self.skip_whitespace()?;
        if self.peek() == Some(c) {
            self.pos += 1;
            Ok(())
        } else {
            Err(self.error(&format!("expected '{c}'")))
        }
    }

    fn value(&mut self) -> Result<Json, String> {
        self.skip_whitespace()?;
        match self.peek() {
            Some('{') => self.object(),
            Some('[') => self.array(),
            Some('"') => self.string().map(Json::String),
            Some(c) if c == '-' || c.is_ascii_digit() => self.number(),
            Some(_) => self.literal(),
            None => Err(self.error("expected a value")),
        }
    }

    fn literal(&mut self) -> Result<Json, String> {
        for (word, value) in [
            ("true", Json::Bool(true)),
            ("false", Json::Bool(false)),
            ("null", Json::Null),
        ] {
            let end = self.pos + word.len();
            if end <= self.chars.len() && self.chars[self.pos..end].iter().copied().eq(word.chars())
            {
                self.pos = end;
                return Ok(value);
            }
        }
        Err(self.error("expected a value (string, number, true, false, null, object or array)"))
    }

    fn number(&mut self) -> Result<Json, String> {
        let start = self.pos;
        while self
            .peek()
            .is_some_and(|c| c.is_ascii_digit() || matches!(c, '-' | '+' | '.' | 'e' | 'E'))
        {
            self.pos += 1;
        }
        let text: String = self.chars[start..self.pos].iter().collect();
        text.parse().map(Json::Number).map_err(|_| {
            self.pos = start;
            self.error(&format!("invalid number {text}"))
        })
    }

    fn string(&mut self) -> Result<String, String> {
        self.pos += 1;
        let mut out = String::new();
        loop {
            match self.peek() {
                None | Some('\n') => return Err(self.error("unclosed string")),
                Some('"') => {
                    self.pos += 1;
                    return Ok(out);
                }
                Some('\\') => {
                    self.pos += 1;
                    let escaped = match self.peek() {
                        Some('n') => '\n',
                        Some('t') => '\t',
                        Some('r') => '\r',
                        Some('b') => '\u{8}',
                        Some('f') => '\u{c}',
                        Some('u') => {
                            let hex: String = self
                                .chars
                                .get(self.pos + 1..self.pos + 5)
                                .unwrap_or(&[])
                                .iter()
                                .collect();
                            let code = u32::from_str_radix(&hex, 16)
                                .map_err(|_| self.error("invalid \\u escape"))?;
                            self.pos += 4;
                            char::from_u32(code).unwrap_or('\u{fffd}')
                        }
                        Some(c @ ('"' | '\\' | '/')) => c,
                        _ => return Err(self.error("invalid escape")),
                    };
                    out.push(escaped);
                    self.pos += 1;
                }
                Some(c) => {
                    out.push(c);
                    self.pos += 1;
                }
            }
        }
    }

    fn array(&mut self) -> Result<Json, String> {
        self.pos += 1;
        let mut items = Vec::new();
        loop {
            self.skip_whitespace()?;
            if self.peek() == Some(']') {
                self.pos += 1;
                return Ok(Json::Array(items));
            }
            items.push(self.value()?);
            self.skip_whitespace()?;
            match self.peek() {
                Some(',') => self.pos += 1,
                Some(']') => {}
                _ => return Err(self.error("expected ',' or ']'")),
            }
        }
    }

    fn object(&mut self) -> Result<Json, String> {
        self.pos += 1;
        let mut entries = Vec::new();
        loop {
            self.skip_whitespace()?;
            match self.peek() {
                Some('}') => {
                    self.pos += 1;
                    return Ok(Json::Object(entries));
                }
                Some('"') => {}
                _ => return Err(self.error("expected a \"key\" or '}'")),
            }
            let key = self.string()?;
            self.expect(':')?;
            let value = self.value()?;
            entries.push((key, value));
            self.skip_whitespace()?;
            match self.peek() {
                Some(',') => self.pos += 1,
                Some('}') => {}
                _ => return Err(self.error("expected ',' or '}'")),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_json_with_comments_and_trailing_commas() {
        let value = parse(
            r##"{
              // the theme
              "theme": { "preset": "tokyonight-night", "colors": { "bg": "#000000", }, },
              /* options */ "options": { "tabSize": 4, "x": [true, null, -1.5e2, "a\"A"] }
            }"##,
        )
        .unwrap();
        assert_eq!(
            value
                .get("theme")
                .and_then(|t| t.get("preset"))
                .and_then(Json::as_str),
            Some("tokyonight-night")
        );
        assert_eq!(
            value.get("options").and_then(|o| o.get("x")),
            Some(&Json::Array(vec![
                Json::Bool(true),
                Json::Null,
                Json::Number(-150.0),
                Json::String("a\"A".to_string())
            ]))
        );
    }

    #[test]
    fn errors_name_the_line_and_column() {
        assert_eq!(
            parse("{\n  \"a\": 1\n  \"b\": 2\n}").unwrap_err(),
            "line 3, column 3: expected ',' or '}'"
        );
        assert!(
            parse("{\"a\": tru}")
                .unwrap_err()
                .starts_with("line 1, column 7")
        );
        assert!(parse("[1] x").is_err());
    }

    #[test]
    fn objects_merge_deeply() {
        let mut base =
            parse(r##"{"theme": {"colors": {"bg": "#111111", "fg": "#eeeeee"}}, "a": 1}"##)
                .unwrap();
        base.merge(parse(r##"{"theme": {"colors": {"bg": "#222222"}}, "a": [2]}"##).unwrap());
        let colors = base.get("theme").and_then(|t| t.get("colors")).unwrap();
        assert_eq!(colors.get("bg").and_then(Json::as_str), Some("#222222"));
        assert_eq!(colors.get("fg").and_then(Json::as_str), Some("#eeeeee"));
        assert_eq!(base.get("a"), Some(&Json::Array(vec![Json::Number(2.0)])));
    }
}
