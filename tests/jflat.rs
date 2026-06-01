use lumiknit_cli_edc::jflat::*;

fn flatten(input: &str) -> Vec<String> {
    let entries = do_flatten(input, None, true).expect("flatten failed");
    entries
        .iter()
        .map(|e| {
            format!("{}\t{}\t{}", fmt_path(&e.path, "\t"), e.typ, e.val)
        })
        .collect()
}

fn roundtrip_json(input: &str) -> serde_json::Value {
    // flatten then unflatten back to JSON
    let entries = do_flatten(input, None, true).expect("flatten failed");
    let flat: String = entries
        .iter()
        .map(|e| {
            format!("{}\t{}\t{}\n", fmt_path(&e.path, "\t"), e.typ, e.val)
        })
        .collect();
    let json_str =
        do_unflatten(&flat, Fmt::Json, "\t").expect("unflatten failed");
    serde_json::from_str(&json_str).expect("invalid json from unflatten")
}

// --- flatten JSON ---

#[test]
fn flat_simple_object() {
    let lines = flatten(r#"{"a":1,"b":"hello"}"#);
    assert!(lines.iter().any(|l| l.contains(".a")
        && l.contains("number")
        && l.contains("1")));
    assert!(lines.iter().any(|l| l.contains(".b")
        && l.contains("string")
        && l.contains("hello")));
}

#[test]
fn flat_nested_object() {
    let lines = flatten(r#"{"x":{"y":42}}"#);
    assert_eq!(lines.len(), 1);
    assert!(
        lines[0].contains(".x.y") && lines[0].contains("42"),
        "got: {:?}",
        lines
    );
}

#[test]
fn flat_array() {
    let lines = flatten(r#"[1,2,3]"#);
    assert_eq!(lines.len(), 3);
    assert!(lines[0].contains("[0]"));
    assert!(lines[2].contains("[2]"));
}

#[test]
fn flat_null_and_bool() {
    let lines = flatten(r#"{"n":null,"b":true,"f":false}"#);
    assert!(lines.iter().any(|l| l.contains(".n") && l.contains("null")));
    assert!(lines.iter().any(|l| l.contains(".b")
        && l.contains("boolean")
        && l.contains("true")));
    assert!(
        lines
            .iter()
            .any(|l| l.contains(".f") && l.contains("false"))
    );
}

#[test]
fn flat_empty_object() {
    let lines = flatten(r#"{"empty":{}}"#);
    assert_eq!(lines.len(), 1);
    assert!(
        lines[0].contains(".empty")
            && lines[0].contains("object")
            && lines[0].contains("{}")
    );
}

#[test]
fn flat_empty_array() {
    let lines = flatten(r#"{"arr":[]}"#);
    assert_eq!(lines.len(), 1);
    assert!(
        lines[0].contains(".arr")
            && lines[0].contains("array")
            && lines[0].contains("[]")
    );
}

#[test]
fn flat_key_with_special_chars() {
    // Keys with dots must be quoted
    let lines = flatten(r#"{"a.b":1}"#);
    assert_eq!(lines.len(), 1);
    assert!(
        lines[0].contains("\"a.b\""),
        "key should be quoted, got: {:?}",
        lines
    );
}

#[test]
fn flat_sorted_keys() {
    // do_flatten with sort=true must output paths in lexicographic order
    let entries = do_flatten(r#"{"z":1,"a":2,"m":3}"#, None, true).unwrap();
    let keys: Vec<_> =
        entries.iter().map(|e| fmt_path(&e.path, "\t")).collect();
    let mut sorted = keys.clone();
    sorted.sort();
    assert_eq!(keys, sorted, "output should be sorted");
}

// --- unflatten ---

#[test]
fn unflatten_simple() {
    let flat = ".a\tnumber\t1\n.b\tstring\t\"hello\"\n";
    let json_str = do_unflatten(flat, Fmt::Json, "\t").unwrap();
    let v: serde_json::Value = serde_json::from_str(&json_str).unwrap();
    assert_eq!(v["a"], serde_json::json!(1));
    assert_eq!(v["b"], serde_json::json!("hello"));
}

#[test]
fn unflatten_array() {
    let flat = "[0]\tnumber\t10\n[1]\tnumber\t20\n";
    let json_str = do_unflatten(flat, Fmt::Json, "\t").unwrap();
    let v: serde_json::Value = serde_json::from_str(&json_str).unwrap();
    assert_eq!(v, serde_json::json!([10, 20]));
}

#[test]
fn unflatten_empty_object() {
    let flat = ".x\tobject\t{}\n";
    let json_str = do_unflatten(flat, Fmt::Json, "\t").unwrap();
    let v: serde_json::Value = serde_json::from_str(&json_str).unwrap();
    assert_eq!(v["x"], serde_json::json!({}));
}

#[test]
fn unflatten_skips_blank_lines() {
    let flat = ".a\tnumber\t1\n\n.b\tnumber\t2\n";
    let json_str = do_unflatten(flat, Fmt::Json, "\t").unwrap();
    let v: serde_json::Value = serde_json::from_str(&json_str).unwrap();
    assert_eq!(v["a"], 1);
    assert_eq!(v["b"], 2);
}

// --- roundtrip (the core property: flatten | unflatten ≈ identity for sorted JSON) ---

#[test]
fn roundtrip_flat_object() {
    let orig: serde_json::Value =
        serde_json::from_str(r#"{"a":1,"b":"hello","c":true}"#).unwrap();
    let rt = roundtrip_json(r#"{"a":1,"b":"hello","c":true}"#);
    assert_eq!(orig, rt);
}

#[test]
fn roundtrip_nested() {
    let orig: serde_json::Value =
        serde_json::from_str(r#"{"x":{"y":{"z":99}}}"#).unwrap();
    let rt = roundtrip_json(r#"{"x":{"y":{"z":99}}}"#);
    assert_eq!(orig, rt);
}

#[test]
fn roundtrip_array_of_objects() {
    let json = r#"[{"a":1},{"a":2}]"#;
    let orig: serde_json::Value = serde_json::from_str(json).unwrap();
    let rt = roundtrip_json(json);
    assert_eq!(orig, rt);
}

#[test]
fn roundtrip_mixed_types() {
    let json = r#"{"n":null,"b":false,"i":42,"s":"hi","arr":[1,2]}"#;
    let orig: serde_json::Value = serde_json::from_str(json).unwrap();
    let rt = roundtrip_json(json);
    assert_eq!(orig, rt);
}

#[test]
fn roundtrip_empty_containers() {
    let json = r#"{"e":{},"a":[]}"#;
    let orig: serde_json::Value = serde_json::from_str(json).unwrap();
    let rt = roundtrip_json(json);
    assert_eq!(orig, rt);
}

// --- env unflatten ---

fn env(json: &str) -> Vec<(String, String)> {
    let entries = do_flatten(json, None, true).expect("flatten failed");
    let flat: String = entries.iter().map(|e| {
        format!("{}\t{}\t{}\n", fmt_path(&e.path, "\t"), e.typ, e.val)
    }).collect();
    let out = do_unflatten_env(&flat, "\t").expect("unflatten_env failed");
    out.lines().map(|l| {
        let (k, v) = l.split_once('=').unwrap();
        (k.to_string(), v.to_string())
    }).collect()
}

#[test]
fn env_simple_keys() {
    let pairs = env(r#"{"host":"localhost","port":8080}"#);
    assert!(pairs.iter().any(|(k, v)| k == "HOST" && v == "'localhost'"));
    assert!(pairs.iter().any(|(k, v)| k == "PORT" && v == "8080"));
}

#[test]
fn env_nested_key() {
    let pairs = env(r#"{"db":{"host":"srv"}}"#);
    assert!(pairs.iter().any(|(k, v)| k == "DB_HOST" && v == "'srv'"));
}

#[test]
fn env_array_index() {
    let pairs = env(r#"{"hosts":["a","b"]}"#);
    assert!(pairs.iter().any(|(k, v)| k == "HOSTS_0" && v == "'a'"));
    assert!(pairs.iter().any(|(k, v)| k == "HOSTS_1" && v == "'b'"));
}

#[test]
fn env_null_is_empty_value() {
    let flat = ".x\tnull\tnull\n";
    let out = do_unflatten_env(flat, "\t").unwrap();
    assert_eq!(out, "X=\n");
}

#[test]
fn env_boolean() {
    let pairs = env(r#"{"enabled":true}"#);
    assert!(pairs.iter().any(|(k, v)| k == "ENABLED" && v == "true"));
}

#[test]
fn env_special_chars_in_key_collapsed() {
    // "my-key" → MY_KEY (hyphen → single underscore)
    let flat = ".my-key\tstring\t\"val\"\n";
    let out = do_unflatten_env(flat, "\t").unwrap();
    assert_eq!(out, "MY_KEY='val'\n");
}

#[test]
fn env_key_leading_trailing_underscore_stripped() {
    // "-leading" → LEADING (leading _ stripped after sanitize)
    let flat = ".\"-leading\"\tstring\t\"v\"\n";
    let out = do_unflatten_env(flat, "\t").unwrap();
    assert_eq!(out, "LEADING='v'\n");
}

#[test]
fn env_string_single_quote_escaped() {
    // value with ' must be escaped as '\''
    let flat = ".key\tstring\t\"it's\"\n";
    let out = do_unflatten_env(flat, "\t").unwrap();
    assert_eq!(out, "KEY='it'\\''s'\n");
}

#[test]
fn env_empty_containers_skipped() {
    let pairs = env(r#"{"empty":{},"arr":[],"x":1}"#);
    assert!(!pairs.iter().any(|(k, _)| k == "EMPTY" || k == "ARR"));
    assert!(pairs.iter().any(|(k, _)| k == "X"));
}

#[test]
fn env_duplicate_keys_both_output() {
    // .foo.bar and .foo_bar both map to FOO_BAR — both lines must appear
    let flat = ".foo.bar\tstring\t\"a\"\n.foo_bar\tstring\t\"b\"\n";
    let out = do_unflatten_env(flat, "\t").unwrap();
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines.len(), 2);
    assert!(lines.iter().all(|l| l.starts_with("FOO_BAR=")));
}

// --- path encoding ---

#[test]
fn parse_fmt_path_roundtrip() {
    let segs =
        vec![Seg::Key("a".into()), Seg::Idx(0), Seg::Key("b".into())];
    let s = fmt_path(&segs, "\t");
    let parsed = parse_path(&s);
    assert_eq!(parsed, segs);
}

#[test]
fn parse_fmt_path_quoted_key() {
    let segs = vec![Seg::Key("a.b".into())];
    let s = fmt_path(&segs, "\t");
    assert!(s.contains('"'), "dotted key must be quoted: {s}");
    let parsed = parse_path(&s);
    assert_eq!(parsed, segs);
}

#[test]
fn root_path() {
    assert_eq!(fmt_path(&[], "\t"), ".");
}
