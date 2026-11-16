use lumiknit_cli_edc::md::*;

fn strip_ansi(s: &str) -> String {
    let mut out = String::new();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' && chars.peek() == Some(&']') {
            // OSC: until BEL or ESC \
            chars.next();
            while let Some(d) = chars.next() {
                if d == '\x07' {
                    break;
                }
                if d == '\x1b' {
                    chars.next();
                    break;
                }
            }
        } else if c == '\x1b' && chars.peek() == Some(&'[') {
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

// --- inline fixes ---

#[test]
fn snake_case_not_italic() {
    assert_eq!(inline("a snake_case_name here"), "a snake_case_name here");
    let mut out = Vec::new();
    render_inline("snake_case_name", &mut out);
    assert!(!String::from_utf8(out).unwrap().contains("\x1b[3m"));
}

#[test]
fn spaced_stars_literal() {
    let mut out = Vec::new();
    render_inline("2 * 3 * 4", &mut out);
    let s = String::from_utf8(out).unwrap();
    assert_eq!(s, "2 * 3 * 4");
}

#[test]
fn escapes() {
    assert_eq!(inline(r"\*not\* \_x\_ \[a\]"), "*not* _x_ [a]");
}

#[test]
fn multi_backtick_code() {
    assert_eq!(inline("`` a`b ``"), "a`b");
    assert_eq!(inline("a ` b"), "a ` b");
}

#[test]
fn link_with_parens() {
    let s = inline("[t](https://x.org/a_(b)) after");
    assert_eq!(s, "[t](https://x.org/a_(b)) after");
}

#[test]
fn bare_url_trailing_punct() {
    let mut out = Vec::new();
    render_inline("see https://x.org/a.", &mut out);
    let s = String::from_utf8(out).unwrap();
    assert!(s.ends_with("\x1b]8;;\x1b\\."), "got: {s:?}");
}

#[test]
fn emphasis_across_lines() {
    let mut out = Vec::new();
    let mut r = Renderer::new(true, ".md-".into(), CodePrinter::Ansi);
    r.render_line("**bold", &mut out);
    r.render_line("more** plain", &mut out);
    r.finish(&mut out);
    let raw = String::from_utf8(out).unwrap();
    // bold re-applied on the second line
    assert_eq!(raw.matches("\x1b[1m").count(), 2, "got: {raw:?}");
    assert_eq!(strip_ansi(&raw), "bold more plain\n");
}

// --- blocks ---

#[test]
fn nested_list_keeps_indent() {
    let s = render_str("- a\n  - b\n    - c");
    assert_eq!(s, "- a\n  - b\n    - c\n");
}

#[test]
fn list_continuation_joined() {
    assert_eq!(render_str("- a\n  b\n- c"), "- a b\n- c\n");
}

#[test]
fn number_does_not_interrupt_paragraph() {
    assert_eq!(render_str("in\n2019. then"), "in 2019. then\n");
}

#[test]
fn blockquote_joined() {
    assert_eq!(render_str("> a\n> b"), "> a b\n");
}

#[test]
fn fence_with_info_not_closing() {
    let s = render_str("```rust\n```rust\nx\n```");
    assert!(s.contains("x"), "got: {s}");
}

#[test]
fn long_fence_contains_short() {
    let s = render_str("````md\n```\ninner\n```\n````\nafter");
    assert!(s.contains("inner") && s.contains("after"), "got: {s}");
    assert!(s.matches("```").count() >= 2, "got: {s}");
}

#[test]
fn indented_fence_in_list() {
    let s = render_str("- item\n  ```sh\n  echo hi\n  ```\n- next");
    assert!(s.contains("\necho hi\n"), "got: {s}");
}

#[test]
fn unterminated_code_block_is_flushed() {
    let s = render_str("```py\nprint(1)");
    assert!(s.contains("print(1)"), "got: {s}");
}

#[test]
fn strip_ansi_writer() {
    use std::io::Write;
    let mut buf = Vec::new();
    {
        let mut w = StripAnsi::new(&mut buf);
        w.write_all(b"a\x1b[1").unwrap();
        w.write_all(b"mb\x1b]8;;http://x\x1b\\c\x1b]8;;\x07d")
            .unwrap();
    }
    assert_eq!(String::from_utf8(buf).unwrap(), "abcd");
}

#[test]
fn heading_style_restored_after_inline() {
    let mut out = Vec::new();
    let mut r = Renderer::new(true, ".md-".into(), CodePrinter::Ansi);
    r.render_line("# a `c` **b** d", &mut out);
    let raw = String::from_utf8(out).unwrap();
    let after_code = raw.split("c").nth(1).unwrap();
    assert!(
        after_code.starts_with("\x1b[0m\x1b[1;31;4m"),
        "got: {raw:?}"
    );
    assert_eq!(strip_ansi(&raw), "# a c b d\n");
}
