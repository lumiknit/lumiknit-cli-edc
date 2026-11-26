use std::io::Write;
use std::process::{Command, Stdio};

fn run(args: &[&str], html: &[u8]) -> std::process::Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_html-skim"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(html).unwrap();
    child.wait_with_output().unwrap()
}
fn skim(args: &[&str], html: &str) -> String {
    let o = run(args, html.as_bytes());
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    String::from_utf8(o.stdout).unwrap()
}

#[test]
fn filters_html_and_keeps_document_metadata() {
    let html = r#"<!doctype html><title>A &amp; B</title><meta name="description" content="hello">
        <meta name="viewport" content="width=device-width"><meta http-equiv="refresh" content="0; url=/bad">
        <link rel="canonical" href="https://example.org"><link rel="preload" href="/x">
        <style>.bad{}</style><script>evil()</script><h1 onclick="evil()" style="color:red" data-x="x">Hi</h1>
        <svg><text>vector</text></svg><!--secret--><iframe src="/x"></iframe>
        <p hidden>hidden</p><p aria-hidden="true">hidden2</p>
        <a href="java&#x09;script:evil()" onfocus="evil()">click</a><p>Text &amp; text</p>"#;
    let out = skim(&["--html"], html);
    for unwanted in [
        "<script",
        "<style",
        "viewport",
        "refresh",
        "preload",
        "onclick",
        "data-x",
        "svg",
        "secret",
        "iframe",
        "hidden",
        "javascript",
        "onfocus",
    ] {
        assert!(!out.contains(unwanted), "{unwanted}: {out}");
    }
    assert!(out.contains("<h1>Hi</h1>"));
    assert!(out.contains("canonical"));
    let md = skim(&[], html);
    let yaml = md
        .strip_prefix("---\n")
        .unwrap()
        .split("\n---")
        .next()
        .unwrap();
    let meta: serde_json::Value = serde_yml::from_str(yaml).unwrap();
    assert_eq!(meta["title"], "A & B");
    assert_eq!(meta["description"], "hello");
    assert!(md.contains("# Hi"));
    assert!(!skim(&["--no-metadata"], html).contains("description"));
}

#[test]
fn main_selection_and_fallback_exclude_hidden_candidates() {
    let html = "<main hidden>secret</main><nav>menu</nav><article>article</article><main><p>actual</p></main>";
    assert_eq!(skim(&["--main"], html), "actual\n");
    assert_eq!(
        skim(
            &["--main"],
            "<nav>menu</nav><div class='entry-content'>post</div>"
        ),
        "post\n"
    );
    assert_eq!(skim(&["--main"], "<p>fallback"), "fallback\n");
    assert_eq!(
        skim(
            &["--main"],
            "<div hidden><main>secret</main></div><article>visible</article>"
        ),
        "visible\n"
    );
}

#[test]
fn assets_are_opt_in_and_can_be_extracted() {
    let html = "<title>Title</title><script src='/app.js'>const x = 1;</script><style>p{color:red}</style><link rel='stylesheet' href='/app.css'><p>body</p>";
    let scripts = skim(&["--only", "scripts"], html);
    assert!(scripts.contains("const x = 1;"));
    assert!(scripts.contains("/app.js"));
    assert!(!scripts.contains("body"));
    assert!(!scripts.contains("color:red"));
    let assets = skim(&["--only", "assets", "--html"], html);
    assert!(assets.contains("<script src=\"/app.js\">const x = 1;</script>"));
    assert!(assets.contains("<style>p{color:red}</style>"));
    assert!(assets.contains("href=\"/app.css\""));
    assert!(!assets.contains("<p>"));
    assert!(skim(&["--include", "scripts"], html).contains("body"));
    assert!(skim(&["--include", "scripts"], html).contains("const x = 1;"));
    assert!(!skim(&["--include", "scripts"], html).contains("color:red"));
}

#[test]
fn markdown_preserves_structure_and_literal_content() {
    let out = skim(
        &[],
        "<h2>Title</h2><p>Hello <strong>world</strong> &amp; <a href='/a?q=1&amp;b=2'>link</a>.</p><ul><li>one</li><li>two</li></ul><blockquote>quote</blockquote><pre>a```b\n&lt;x&gt;</pre><p>literal *x* &lt;script&gt;</p>",
    );
    for expected in [
        "## Title",
        "Hello **world** & [link](/a?q=1&b=2)",
        "- one",
        "- two",
        "> quote",
        "````\na```b\n<x>\n````",
        "literal \\*x\\* \\<script\\>",
    ] {
        assert!(out.contains(expected), "{expected}: {out}");
    }
    assert_eq!(skim(&["--no-links"], "<a href='/x'>label</a>"), "label\n");
}

#[test]
fn byte_limit_is_hard_and_utf8_safe_including_frontmatter() {
    let html = "<title>한글</title><p>안녕하세요🙂</p>";
    let full = skim(&[], html);
    for limit in 0..full.len() + 2 {
        let out = skim(&["--limit", &limit.to_string()], html);
        assert!(out.len() <= limit);
        assert!(full.starts_with(&out));
    }
    let o = run(&[], b"<p>bad \xff text");
    assert!(o.status.success());
    assert!(String::from_utf8(o.stdout).unwrap().contains('\u{fffd}'));
}

#[test]
fn rejects_invalid_options_and_never_accepts_paths() {
    for args in [
        vec!["page.html"],
        vec!["--limit", "-1"],
        vec!["--include", "bad"],
        vec!["--only", "scripts", "--main"],
    ] {
        let o = run(&args, b"");
        assert!(!o.status.success());
        assert!(o.stdout.is_empty());
        assert!(!o.stderr.is_empty());
    }
}

#[test]
fn metadata_duplicates_and_structured_content() {
    let out = skim(
        &[],
        "<meta name='description' content='first'><meta name='description' content='second'><ol start='3'><li>third</li><li>fourth</li></ol><table><tr><td>A</td><td>B</td></tr></table><p>A<strong> bold </strong>word.</p>",
    );
    let yaml = out
        .strip_prefix("---\n")
        .unwrap()
        .split("\n---")
        .next()
        .unwrap();
    let meta: serde_json::Value = serde_yml::from_str(yaml).unwrap();
    assert_eq!(meta["description"], "first");
    assert!(out.contains("3. third\n4. fourth"), "{out}");
    assert!(out.contains("| A | B |"), "{out}");
    assert!(out.contains("A **bold** word."), "{out}");
}
