use lumiknit_cli_edc::md::*;

fn strip_ansi(s: &str) -> String {
    let mut out = String::new();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' && chars.peek() == Some(&'[') {
            chars.next();
            while let Some(&d) = chars.peek() {
                chars.next();
                if d.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

fn render_str(input: &str) -> String {
    let mut out = Vec::<u8>::new();
    let mut r = Renderer::new(true, ".md-".to_string(), CodePrinter::Ansi);
    for line in input.lines() {
        r.render_line(line, &mut out);
    }
    r.finish(&mut out);
    strip_ansi(&String::from_utf8(out).unwrap())
}

fn inline(input: &str) -> String {
    let mut out = Vec::<u8>::new();
    render_inline(input, &mut out);
    strip_ansi(&String::from_utf8(out).unwrap())
}

// --- render_inline ---

#[test]
fn inline_plain() {
    assert_eq!(inline("hello world"), "hello world");
}

#[test]
fn inline_bold() {
    let s = inline("**bold**");
    assert!(s.contains("bold"), "got: {s}");
}

#[test]
fn inline_italic() {
    let s = inline("_italic_");
    assert!(s.contains("italic"), "got: {s}");
}

#[test]
fn inline_code() {
    let s = inline("`code`");
    assert!(s.contains("code"), "got: {s}");
}

#[test]
fn inline_strikethrough() {
    let s = inline("~~strike~~");
    assert!(s.contains("strike"), "got: {s}");
}

#[test]
fn inline_link() {
    let s = inline("[text](https://example.com)");
    assert!(s.contains("[text](https://example.com)"), "got: {s}");
}

#[test]
fn inline_not_a_link_missing_url() {
    // "[foo]" without "(url)" is rendered verbatim
    let s = inline("[foo]");
    assert!(s.contains("[foo]"), "got: {s}");
}

// --- Renderer block types ---

#[test]
fn heading_h1() {
    let s = render_str("# Title");
    assert!(s.contains("# Title"), "got: {s}");
}

#[test]
fn heading_h3() {
    let s = render_str("### Sub");
    assert!(s.contains("### Sub"), "got: {s}");
}

#[test]
fn unordered_list() {
    let s = render_str("- item one\n- item two");
    assert!(s.contains("item one"), "got: {s}");
    assert!(s.contains("item two"), "got: {s}");
}

#[test]
fn ordered_list() {
    let s = render_str("1. first\n2. second");
    assert!(s.contains("first"), "got: {s}");
    assert!(s.contains("second"), "got: {s}");
}

#[test]
fn blockquote() {
    let s = render_str("> quoted");
    assert!(s.contains(">"), "got: {s}");
    assert!(s.contains("quoted"), "got: {s}");
}

#[test]
fn horizontal_rule() {
    let s = render_str("---");
    assert!(s.contains("---"), "got: {s}");
}

#[test]
fn code_block_dry() {
    // dry=true so no file is written; filename ends with ???.rs
    let input = "```rust\nfn main() {}\n```";
    let s = render_str(input);
    assert!(s.contains("rust"), "got: {s}");
    assert!(s.contains("fn main() {}"), "got: {s}");
}

#[test]
fn paragraph_wrapping() {
    // Two consecutive lines without blank line are joined with a space
    let s = render_str("line one\nline two");
    assert!(s.contains("line one"), "got: {s}");
    assert!(s.contains("line two"), "got: {s}");
}

#[test]
fn blank_line_separates_paragraphs() {
    let s = render_str("para one\n\npara two");
    assert!(s.contains("para one"), "got: {s}");
    assert!(s.contains("para two"), "got: {s}");
}

// --- is_hr ---

#[test]
fn hr_dashes() {
    assert!(is_hr("---"));
}
#[test]
fn hr_stars() {
    assert!(is_hr("***"));
}
#[test]
fn hr_underscores() {
    assert!(is_hr("___"));
}
#[test]
fn hr_with_spaces() {
    assert!(is_hr("- - -"));
}
#[test]
fn hr_too_short() {
    assert!(!is_hr("--"));
}
#[test]
fn hr_mixed_chars() {
    assert!(!is_hr("-_-"));
}
