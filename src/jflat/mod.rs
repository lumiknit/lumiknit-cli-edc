use crate::jsonlite;
use std::collections::HashSet;

// ---- Segment / Path -------------------------------------------------------

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Seg {
    Key(String),
    Idx(usize),
}

impl PartialOrd for Seg {
    fn partial_cmp(&self, o: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(o))
    }
}
impl Ord for Seg {
    fn cmp(&self, o: &Self) -> std::cmp::Ordering {
        use std::cmp::Ordering::*;
        match (self, o) {
            (Seg::Idx(a), Seg::Idx(b)) => a.cmp(b),
            (Seg::Key(a), Seg::Key(b)) => a.cmp(b),
            (Seg::Idx(_), Seg::Key(_)) => Less,
            (Seg::Key(_), Seg::Idx(_)) => Greater,
        }
    }
}

pub fn needs_quoting(k: &str, sep: &str) -> bool {
    k.is_empty()
        || k.contains(sep)
        || k.chars()
            .any(|c| matches!(c, '.' | '[' | ']' | '"' | '\\' | '\n'))
}

pub fn fmt_path(segs: &[Seg], sep: &str) -> String {
    if segs.is_empty() {
        return ".".to_string();
    }
    let mut s = String::new();
    for seg in segs {
        match seg {
            Seg::Key(k) if !needs_quoting(k, sep) => {
                s.push('.');
                s.push_str(k);
            }
            Seg::Key(k) => {
                s.push_str(".\"");
                for c in k.chars() {
                    if c == '"' || c == '\\' {
                        s.push('\\');
                    }
                    s.push(c);
                }
                s.push('"');
            }
            Seg::Idx(i) => {
                s.push('[');
                s.push_str(&i.to_string());
                s.push(']');
            }
        }
    }
    s
}

// Internal canonical path for hashmap keys (sep=\x00 to avoid collisions).
pub fn canon_path(segs: &[Seg]) -> String {
    fmt_path(segs, "\x00")
}

pub fn parse_path(s: &str) -> Vec<Seg> {
    if s == "." {
        return vec![];
    }
    let mut segs = vec![];
    let mut chars = s.chars().peekable();
    while let Some(&c) = chars.peek() {
        match c {
            '.' => {
                chars.next();
                if chars.peek() == Some(&'"') {
                    chars.next();
                    let mut key = String::new();
                    loop {
                        match chars.next() {
                            None | Some('"') => break,
                            Some('\\') => {
                                if let Some(c) = chars.next() {
                                    key.push(c);
                                }
                            }
                            Some(c) => key.push(c),
                        }
                    }
                    segs.push(Seg::Key(key));
                } else {
                    let mut key = String::new();
                    while matches!(chars.peek(), Some(&c) if c != '.' && c != '[')
                    {
                        key.push(chars.next().unwrap());
                    }
                    segs.push(Seg::Key(key));
                }
            }
            '[' => {
                chars.next();
                let mut num = String::new();
                loop {
                    match chars.next() {
                        None | Some(']') => break,
                        Some(c) => num.push(c),
                    }
                }
                segs.push(Seg::Idx(num.parse().unwrap_or(0)));
            }
            _ => break,
        }
    }
    segs
}

// ---- Entry ----------------------------------------------------------------

pub struct Entry {
    pub path: Vec<Seg>,
    pub typ: &'static str,
    pub val: String,
}

pub fn fmt_float(f: f64) -> String {
    if f.is_finite() && f.fract() == 0.0 && f.abs() < 9.007_199_254_740_992e15 {
        (f as i64).to_string()
    } else {
        format!("{f}")
    }
}

// ---- Flatten TOML ---------------------------------------------------------

pub fn flatten_toml(v: &toml::Value, path: &mut Vec<Seg>, out: &mut Vec<Entry>) {
    match v {
        toml::Value::String(s) => out.push(Entry {
            path: path.clone(),
            typ: "string",
            val: jsonlite::quote_readable(s),
        }),
        toml::Value::Integer(i) => out.push(Entry {
            path: path.clone(),
            typ: "number",
            val: i.to_string(),
        }),
        toml::Value::Float(f) => out.push(Entry {
            path: path.clone(),
            typ: "number",
            val: fmt_float(*f),
        }),
        toml::Value::Boolean(b) => out.push(Entry {
            path: path.clone(),
            typ: "boolean",
            val: b.to_string(),
        }),
        toml::Value::Datetime(d) => out.push(Entry {
            path: path.clone(),
            typ: "time",
            val: d.to_string(),
        }),
        toml::Value::Array(arr) if arr.is_empty() => out.push(Entry {
            path: path.clone(),
            typ: "array",
            val: "[]".into(),
        }),
        toml::Value::Table(map) if map.is_empty() => out.push(Entry {
            path: path.clone(),
            typ: "object",
            val: "{}".into(),
        }),
        toml::Value::Array(arr) => {
            for (i, v) in arr.iter().enumerate() {
                path.push(Seg::Idx(i));
                flatten_toml(v, path, out);
                path.pop();
            }
        }
        toml::Value::Table(map) => {
            for (k, v) in map {
                path.push(Seg::Key(k.clone()));
                flatten_toml(v, path, out);
                path.pop();
            }
        }
    }
}

// ---- Flatten YAML ---------------------------------------------------------

pub fn flatten_yaml(
    v: &serde_yml::Value,
    path: &mut Vec<Seg>,
    out: &mut Vec<Entry>,
) {
    match v {
        serde_yml::Value::Null => out.push(Entry {
            path: path.clone(),
            typ: "null",
            val: "null".into(),
        }),
        serde_yml::Value::Bool(b) => out.push(Entry {
            path: path.clone(),
            typ: "boolean",
            val: b.to_string(),
        }),
        serde_yml::Value::String(s) => out.push(Entry {
            path: path.clone(),
            typ: "string",
            val: jsonlite::quote_readable(s),
        }),
        serde_yml::Value::Number(n) => {
            let f = n.as_f64();
            let val = fmt_float(f);
            out.push(Entry {
                path: path.clone(),
                typ: "number",
                val,
            });
        }
        serde_yml::Value::Sequence(arr) if arr.is_empty() => out.push(Entry {
            path: path.clone(),
            typ: "array",
            val: "[]".into(),
        }),
        serde_yml::Value::Mapping(map) if map.is_empty() => out.push(Entry {
            path: path.clone(),
            typ: "object",
            val: "{}".into(),
        }),
        serde_yml::Value::Sequence(arr) => {
            for (i, v) in arr.iter().enumerate() {
                path.push(Seg::Idx(i));
                flatten_yaml(v, path, out);
                path.pop();
            }
        }
        serde_yml::Value::Mapping(map) => {
            for (k, v) in map.iter() {
                path.push(Seg::Key(k.clone()));
                flatten_yaml(v, path, out);
                path.pop();
            }
        }
        serde_yml::Value::Tagged(t) => flatten_yaml(&t.value(), path, out),
    }
}

// ---- Format detection -----------------------------------------------------

#[derive(Clone, Copy)]
pub enum Fmt {
    Json,
    Yaml,
    Toml,
    Env,
}

impl Fmt {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "json" => Some(Fmt::Json),
            "yaml" | "yml" => Some(Fmt::Yaml),
            "toml" => Some(Fmt::Toml),
            "env" => Some(Fmt::Env),
            _ => None,
        }
    }
}

pub fn do_flatten(
    input: &str,
    force: Option<Fmt>,
    sort: bool,
) -> Result<Vec<Entry>, String> {
    let mut path = vec![];
    let mut out = vec![];
    let use_toml = match force {
        Some(Fmt::Toml) => true,
        Some(_) => false,
        None => toml::from_str::<toml::Value>(input).is_ok(),
    };
    if use_toml {
        let v: toml::Value =
            toml::from_str(input).map_err(|e| e.to_string())?;
        flatten_toml(&v, &mut path, &mut out);
    } else {
        let v: serde_yml::Value =
            serde_yml::from_str(input).map_err(|e| e.to_string())?;
        flatten_yaml(&v, &mut path, &mut out);
    }
    if sort {
        out.sort_by(|a, b| a.path.cmp(&b.path));
    }
    Ok(out)
}

// ---- Unflatten ------------------------------------------------------------

pub fn split_line<'a>(
    line: &'a str,
    sep: &str,
) -> Option<(&'a str, &'a str, &'a str)> {
    let b = line.as_bytes();
    let mut i = 0;
    let mut in_q = false;
    while i < b.len() {
        if in_q {
            if b[i] == b'\\' {
                i += 2;
                continue;
            }
            if b[i] == b'"' {
                in_q = false;
            }
        } else if b[i] == b'"' {
            in_q = true;
        } else if line[i..].starts_with(sep) {
            let path_s = &line[..i];
            let rest = &line[i + sep.len()..];
            return rest
                .find(sep)
                .map(|j| (path_s, &rest[..j], &rest[j + sep.len()..]));
        }
        i += 1;
    }
    None
}

pub fn leaf_to_json(typ: &str, val: &str) -> Result<serde_json::Value, String> {
    Ok(match typ {
        "null" => serde_json::Value::Null,
        "boolean" => serde_json::Value::Bool(val == "true"),
        "number" => serde_json::from_str(val).map_err(|e| e.to_string())?,
        "string" => serde_json::Value::String(jsonlite::unquote(val)),
        "time" => serde_json::Value::String(val.to_string()),
        "object" => serde_json::json!({}),
        "array" => serde_json::json!([]),
        other => return Err(format!("unknown type: {other}")),
    })
}

pub fn insert_json(
    root: &mut serde_json::Value,
    path: &[Seg],
    typ: &str,
    val: &str,
) -> Result<(), String> {
    if path.is_empty() {
        *root = leaf_to_json(typ, val)?;
        return Ok(());
    }
    match &path[0] {
        Seg::Key(k) => {
            if root.is_null() {
                *root = serde_json::json!({});
            }
            let obj = root.as_object_mut().ok_or("expected object node")?;
            insert_json(
                obj.entry(k).or_insert(serde_json::Value::Null),
                &path[1..],
                typ,
                val,
            )
        }
        Seg::Idx(i) => {
            if root.is_null() {
                *root = serde_json::json!([]);
            }
            let arr = root.as_array_mut().ok_or("expected array node")?;
            while arr.len() <= *i {
                arr.push(serde_json::Value::Null);
            }
            insert_json(&mut arr[*i], &path[1..], typ, val)
        }
    }
}

pub fn json_to_toml(
    v: &serde_json::Value,
    path: &[Seg],
    times: &HashSet<String>,
) -> Result<toml::Value, String> {
    match v {
        serde_json::Value::Null => {
            Err("TOML does not support null".to_string())
        }
        serde_json::Value::Bool(b) => Ok(toml::Value::Boolean(*b)),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Ok(toml::Value::Integer(i))
            } else if let Some(f) = n.as_f64() {
                Ok(toml::Value::Float(f))
            } else {
                Err(format!("unrepresentable number: {n}"))
            }
        }
        serde_json::Value::String(s) => {
            if times.contains(&canon_path(path)) {
                Ok(toml::Value::Datetime(
                    s.parse().map_err(|_| format!("invalid datetime: {s}"))?,
                ))
            } else {
                Ok(toml::Value::String(s.clone()))
            }
        }
        serde_json::Value::Array(arr) => {
            let items: Result<Vec<_>, _> = arr
                .iter()
                .enumerate()
                .map(|(i, v)| {
                    let mut p = path.to_vec();
                    p.push(Seg::Idx(i));
                    json_to_toml(v, &p, times)
                })
                .collect();
            Ok(toml::Value::Array(items?))
        }
        serde_json::Value::Object(map) => {
            let mut t = toml::map::Map::new();
            for (k, v) in map {
                let mut p = path.to_vec();
                p.push(Seg::Key(k.clone()));
                t.insert(k.clone(), json_to_toml(v, &p, times)?);
            }
            Ok(toml::Value::Table(t))
        }
    }
}

// ---- Unflatten env --------------------------------------------------------

pub fn path_to_env_key(segs: &[Seg]) -> String {
    let mut key = String::new();
    for seg in segs {
        let raw = match seg {
            Seg::Key(k) => k.as_str().to_owned(),
            Seg::Idx(i) => i.to_string(),
        };
        let mut prev_under = false;
        for c in raw.chars() {
            if c.is_ascii_alphanumeric() {
                key.push(c.to_ascii_uppercase());
                prev_under = false;
            } else if !prev_under {
                key.push('_');
                prev_under = true;
            }
        }
        if !prev_under {
            key.push('_');
            prev_under = true;
        }
        let _ = prev_under;
    }
    let key = key.trim_end_matches('_');
    let key = key.trim_start_matches('_');
    key.to_string()
}

pub fn bash_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

pub fn do_unflatten_env(input: &str, sep: &str) -> Result<String, String> {
    let mut out = String::new();
    for line in input.lines() {
        if line.is_empty() { continue; }
        let (path_s, typ, val) = split_line(line, sep)
            .ok_or_else(|| format!("malformed line: {line}"))?;
        let path = parse_path(path_s);
        let key = path_to_env_key(&path);
        if key.is_empty() { continue; }
        let value = match typ {
            "null"    => String::new(),
            "boolean" | "number" | "time" => val.to_string(),
            "string"  => bash_quote(&jsonlite::unquote(val)),
            "object" | "array" => continue,
            other => return Err(format!("unknown type: {other}")),
        };
        out.push_str(&format!("{key}={value}\n"));
    }
    Ok(out)
}

pub fn do_unflatten(
    input: &str,
    out_fmt: Fmt,
    sep: &str,
) -> Result<String, String> {
    let mut root = serde_json::Value::Null;
    let mut times: HashSet<String> = HashSet::new();

    for line in input.lines() {
        if line.is_empty() {
            continue;
        }
        let (path_s, typ, val) = split_line(line, sep)
            .ok_or_else(|| format!("malformed line: {line}"))?;
        let path = parse_path(path_s);
        if typ == "time" {
            times.insert(canon_path(&path));
        }
        insert_json(&mut root, &path, typ, val)?;
    }

    match out_fmt {
        Fmt::Json => {
            serde_json::to_string_pretty(&root).map_err(|e| e.to_string())
        }
        Fmt::Yaml => serde_yml::to_string(&root).map_err(|e| e.to_string()),
        Fmt::Toml => {
            let tv = json_to_toml(&root, &[], &times)?;
            toml::to_string_pretty(&tv).map_err(|e| e.to_string())
        }
        Fmt::Env => unreachable!("env handled before do_unflatten"),
    }
}
