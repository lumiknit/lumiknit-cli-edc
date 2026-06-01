use lumiknit_cli_edc::jex::*;

fn load_template(src: &str) -> serde_json::Value {
    serde_json::from_str(src).unwrap_or_else(|e| {
        panic!("invalid JSON: {e}\n  {src}");
    })
}

fn make_rule(json: &str, mode: Mode) -> Rule {
    Rule {
        mode,
        compiled: compile(&load_template(json)).expect("compile failed"),
    }
}

fn apply_rule(json: &str, mode: Mode, input: &str) -> Option<String> {
    let rule = make_rule(json, mode);
    rule.apply_and_check(input).map(|v| v.to_string())
}

fn run_process(json: &str, mode: Mode, input: &str) -> String {
    let rules = vec![make_rule(json, mode)];
    let mut out = Vec::<u8>::new();
    process(input, &rules, &mut out);
    String::from_utf8(out).unwrap()
}

// --- parse_field / literal ---

#[test]
fn literal_plain_string() {
    let f = parse_field("hello").unwrap();
    assert!(
        matches!(f, Field::Literal(serde_json::Value::String(s)) if s == "hello")
    );
}

#[test]
fn literal_escaped_slash_prefix() {
    // "//" prefix means literal string starting with "/"
    let f = parse_field("//path").unwrap();
    assert!(
        matches!(f, Field::Literal(serde_json::Value::String(s)) if s == "/path")
    );
}

// --- regex field matching ---

#[test]
fn regex_find_match() {
    // "/pattern/" extracts matched text
    let out =
        apply_rule(r#"{"x":"/hello/"}"#, Mode::AllMatch, "say hello world");
    assert_eq!(out, Some(r#"{"x":"hello"}"#.to_string()));
}

#[test]
fn regex_no_match_all_mode() {
    // AllMatch: all regex fields must match → None if any fails
    let out = apply_rule(
        r#"{"x":"/nothere/"}"#,
        Mode::AllMatch,
        "something else",
    );
    assert_eq!(out, None);
}

#[test]
fn regex_replace_capture_group() {
    let out = apply_rule(
        r#"{"v":"/user=(\\w+)/$1/"}"#,
        Mode::AllMatch,
        "user=alice",
    );
    assert_eq!(out, Some(r#"{"v":"alice"}"#.to_string()));
}

#[test]
fn mode_always_outputs_even_without_match() {
    let out =
        apply_rule(r#"{"x":"/nothere/"}"#, Mode::Always, "something else");
    assert!(out.is_some());
    // x should be null because regex didn't match
    assert!(out.unwrap().contains("null"));
}

#[test]
fn mode_at_least_one_with_one_match() {
    // -1: at least one regex field must match
    let out = apply_rule(
        r#"{"a":"/found/","b":"/missing/"}"#,
        Mode::AtLeastOne,
        "found it",
    );
    assert!(out.is_some());
}

#[test]
fn mode_at_least_one_none_match() {
    let out = apply_rule(
        r#"{"a":"/no/","b":"/nope/"}"#,
        Mode::AtLeastOne,
        "something else",
    );
    assert_eq!(out, None);
}

// --- nested templates ---

#[test]
fn nested_object_template() {
    let out = apply_rule(
        r#"{"outer":{"inner":"/\\d+/"}}"#,
        Mode::AllMatch,
        "code 42",
    );
    assert_eq!(out, Some(r#"{"outer":{"inner":"42"}}"#.to_string()));
}

#[test]
fn array_template() {
    let out =
        apply_rule(r#"["/\\w+/","/\\d+/"]"#, Mode::AllMatch, "abc 123");
    assert_eq!(out, Some(r#"["abc","123"]"#.to_string()));
}

// --- process function (multiple rules) ---

#[test]
fn process_outputs_one_line_per_match() {
    let rules = vec![
        make_rule(r#"{"n":"/\\d+/"}"#, Mode::AllMatch),
        make_rule(r#"{"w":"/[a-z]+/"}"#, Mode::AllMatch),
    ];
    let mut out = Vec::<u8>::new();
    process("hello 42", &rules, &mut out);
    let s = String::from_utf8(out).unwrap();
    let lines: Vec<&str> = s.lines().collect();
    assert_eq!(lines.len(), 2, "expected 2 output lines, got: {s}");
}

#[test]
fn process_no_match_no_output() {
    let s = run_process(r#"{"x":"/NOTHING/"}"#, Mode::AllMatch, "abc");
    assert!(s.is_empty(), "expected empty output, got: {s}");
}

// --- fixed value passthrough ---

#[test]
fn fixed_bool_value() {
    let out = apply_rule(r#"{"ok":true}"#, Mode::Always, "anything");
    assert_eq!(out, Some(r#"{"ok":true}"#.to_string()));
}
