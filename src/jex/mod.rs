use regex::Regex;
use serde_json::Value;
use std::io::Write;

// --- regex field parsing ---

pub enum Field {
    Literal(Value),
    Regex { re: Regex, replace: Option<String> },
}

pub fn split_on_slash(s: &str) -> (&str, Option<&str>) {
    let mut chars = s.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        if c == '\\' {
            chars.next();
        } else if c == '/' {
            return (&s[..i], Some(&s[i + 1..]));
        }
    }
    (s, None)
}

pub fn unescape_slash(s: &str) -> String {
    s.replace("\\/", "/")
}

pub fn build_regex(pattern: &str, flags: &str) -> Result<Regex, regex::Error> {
    if flags.is_empty() {
        Regex::new(pattern)
    } else {
        Regex::new(&format!("(?{flags}){pattern}"))
    }
}

pub fn parse_field(s: &str) -> Result<Field, String> {
    if s.starts_with("//") {
        return Ok(Field::Literal(Value::String(s[1..].to_string())));
    }
    if !s.starts_with('/') {
        return Ok(Field::Literal(Value::String(s.to_string())));
    }
    let inner = &s[1..];
    let (pattern_raw, rest) = split_on_slash(inner);
    let (replace_opt, flags) = match rest {
        None => (None, ""),
        Some(rest) => match split_on_slash(rest) {
            (replace_raw, Some(flags)) => (Some(unescape_slash(replace_raw)), flags),
            (flags, None) => (None, flags),
        },
    };
    let pattern = unescape_slash(pattern_raw);
    build_regex(&pattern, flags)
        .map(|re| Field::Regex { re, replace: replace_opt })
        .map_err(|e| format!("invalid regex {pattern:?}: {e}"))
}

// --- compiled value tree ---

pub enum CompiledValue {
    Field(Field),
    Fixed(Value),
    Object(Vec<(String, CompiledValue)>),
    Array(Vec<CompiledValue>),
}

pub fn compile(v: &Value) -> Result<CompiledValue, String> {
    match v {
        Value::Object(map) => {
            let fields = map
                .iter()
                .map(|(k, v)| compile(v).map(|cv| (k.clone(), cv)))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(CompiledValue::Object(fields))
        }
        Value::Array(arr) => {
            let items = arr.iter().map(compile).collect::<Result<Vec<_>, _>>()?;
            Ok(CompiledValue::Array(items))
        }
        Value::String(s) => parse_field(s).map(CompiledValue::Field),
        other => Ok(CompiledValue::Fixed(other.clone())),
    }
}

impl CompiledValue {
    // Returns (value, regex_fields_matched, total_regex_fields).
    pub fn apply(&self, input: &str) -> (Value, usize, usize) {
        match self {
            CompiledValue::Fixed(v) => (v.clone(), 0, 0),
            CompiledValue::Field(Field::Literal(v)) => (v.clone(), 0, 0),
            CompiledValue::Field(Field::Regex { re, replace }) => {
                let val = match replace {
                    None => re
                        .find(input)
                        .map(|m| Value::String(m.as_str().to_string()))
                        .unwrap_or(Value::Null),
                    Some(rep) => re
                        .captures(input)
                        .map(|caps| {
                            let mut out = String::new();
                            caps.expand(rep, &mut out);
                            Value::String(out)
                        })
                        .unwrap_or(Value::Null),
                };
                let matched = usize::from(val != Value::Null);
                (val, matched, 1)
            }
            CompiledValue::Object(fields) => {
                let mut matched = 0;
                let mut total = 0;
                let map = fields
                    .iter()
                    .map(|(k, cv)| {
                        let (v, m, t) = cv.apply(input);
                        matched += m;
                        total += t;
                        (k.clone(), v)
                    })
                    .collect();
                (Value::Object(map), matched, total)
            }
            CompiledValue::Array(items) => {
                let mut matched = 0;
                let mut total = 0;
                let arr = items
                    .iter()
                    .map(|cv| {
                        let (v, m, t) = cv.apply(input);
                        matched += m;
                        total += t;
                        v
                    })
                    .collect();
                (Value::Array(arr), matched, total)
            }
        }
    }
}

// --- rule / mode ---

#[derive(Clone, Copy)]
pub enum Mode {
    AllMatch,   // default: all regex fields must match
    AtLeastOne, // -1: at least one regex field must match
    Always,     // -a: always output
}

pub struct Rule {
    pub mode: Mode,
    pub compiled: CompiledValue,
}

impl Rule {
    pub fn apply_and_check(&self, input: &str) -> Option<Value> {
        let (val, matched, total) = self.compiled.apply(input);
        let pass = match self.mode {
            Mode::Always => true,
            Mode::AtLeastOne => total == 0 || matched > 0,
            Mode::AllMatch => matched == total,
        };
        if pass { Some(val) } else { None }
    }
}

pub fn process(input: &str, rules: &[Rule], out: &mut impl Write) {
    for rule in rules {
        if let Some(val) = rule.apply_and_check(input) {
            serde_json::to_writer(&mut *out, &val).ok();
            writeln!(out).ok();
        }
    }
}
