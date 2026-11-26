use html5ever::{parse_document, serialize, tendril::TendrilSink};
use markup5ever_rcdom::{Handle, NodeData, RcDom, SerializableHandle};
use std::io::{self, Read, Write};

const HELP: &str = include_str!("help.txt");

#[derive(Default)]
struct Options {
    html: bool,
    main: bool,
    scripts: bool,
    styles: bool,
    only: bool,
    no_metadata: bool,
    no_links: bool,
    limit: Option<usize>,
}

fn main() {
    if let Err(e) = run()
        && e.kind() != io::ErrorKind::BrokenPipe
    {
        eprintln!("html-skim: {e}");
        std::process::exit(1);
    }
}

fn run() -> io::Result<()> {
    let mut o = Options::default();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => {
                print!("{HELP}");
                return Ok(());
            }
            "-v" | "--version" => {
                println!(
                    "{}",
                    lumiknit_cli_edc::version::version_description(
                        "html-skim",
                        "HTML document filter"
                    )
                );
                return Ok(());
            }
            "--html" => o.html = true,
            "--main" => o.main = true,
            "--no-metadata" => o.no_metadata = true,
            "--no-links" => o.no_links = true,
            "--include" | "--only" => {
                o.only |= arg == "--only";
                match args.next().as_deref() {
                    Some("scripts") => o.scripts = true,
                    Some("styles") => o.styles = true,
                    Some("assets") => {
                        o.scripts = true;
                        o.styles = true;
                    }
                    _ => {
                        return Err(io::Error::other(format!(
                            "{arg} requires scripts, styles or assets"
                        )));
                    }
                }
            }
            "--limit" => {
                o.limit =
                    Some(args.next().and_then(|s| s.parse().ok()).ok_or_else(
                        || {
                            io::Error::other(
                                "--limit requires a nonnegative byte count",
                            )
                        },
                    )?);
            }
            _ => {
                return Err(io::Error::other(format!(
                    "unknown option: {arg} (see --help)"
                )));
            }
        }
    }
    if o.only && o.main {
        return Err(io::Error::other("--only and --main cannot be combined"));
    }
    let mut input = Vec::new();
    io::stdin().lock().read_to_end(&mut input)?;
    let output = skim(&input, &o)?;
    let mut end = o.limit.unwrap_or(output.len()).min(output.len());
    while !output.is_char_boundary(end) {
        end -= 1;
    }
    io::stdout().lock().write_all(&output.as_bytes()[..end])
}

fn tag(n: &Handle) -> &str {
    if let NodeData::Element { name, .. } = &n.data {
        name.local.as_ref()
    } else {
        ""
    }
}
fn attr(n: &Handle, key: &str) -> Option<String> {
    if let NodeData::Element { attrs, .. } = &n.data {
        attrs
            .borrow()
            .iter()
            .find(|a| &*a.name.local == key)
            .map(|a| a.value.to_string())
    } else {
        None
    }
}
fn text(n: &Handle) -> String {
    if let NodeData::Text { contents } = &n.data {
        contents.borrow().to_string()
    } else {
        n.children.borrow().iter().map(text).collect()
    }
}
fn find(n: &Handle, predicate: &impl Fn(&Handle) -> bool) -> Option<Handle> {
    if predicate(n) {
        return Some(n.clone());
    }
    n.children.borrow().iter().find_map(|c| find(c, predicate))
}
fn asset(n: &Handle, o: &Options) -> bool {
    (tag(n) == "script" && o.scripts)
        || (o.styles
            && (tag(n) == "style"
                || (tag(n) == "link"
                    && attr(n, "rel").is_some_and(|s| {
                        s.eq_ignore_ascii_case("stylesheet")
                    }))))
}
fn useful_meta(n: &Handle) -> bool {
    match tag(n) {
        "title" => true,
        "meta" => {
            attr(n, "name")
                .or_else(|| attr(n, "property"))
                .is_some_and(|s| {
                    matches!(
                        s.to_ascii_lowercase().as_str(),
                        "description"
                            | "author"
                            | "keywords"
                            | "og:title"
                            | "og:description"
                            | "og:url"
                            | "og:site_name"
                            | "article:published_time"
                            | "article:modified_time"
                    )
                })
        }
        "link" => {
            attr(n, "rel").is_some_and(|s| s.eq_ignore_ascii_case("canonical"))
        }
        _ => false,
    }
}
fn safe_url(value: &str) -> bool {
    let compact: String = value
        .chars()
        .filter(|c| !c.is_ascii_control() && !c.is_whitespace())
        .collect();
    match compact.split_once(':') {
        Some((scheme, _)) if !scheme.contains(['/', '?', '#']) => matches!(
            scheme.to_ascii_lowercase().as_str(),
            "http" | "https" | "mailto" | "tel"
        ),
        _ => true,
    }
}
fn clean(n: &Handle, o: &Options) {
    n.children.borrow_mut().retain(|c| {
        if matches!(
            c.data,
            NodeData::Comment { .. } | NodeData::ProcessingInstruction { .. }
        ) {
            return false;
        }
        let t = tag(c);
        if asset(c, o) {
            return true;
        }
        if matches!(
            t,
            "script"
                | "style"
                | "svg"
                | "math"
                | "template"
                | "noscript"
                | "iframe"
                | "object"
                | "embed"
                | "base"
        ) {
            return false;
        }
        if matches!(t, "meta" | "link" | "title") {
            return !o.no_metadata && useful_meta(c);
        }
        if attr(c, "hidden").is_some()
            || attr(c, "aria-hidden")
                .is_some_and(|s| s.eq_ignore_ascii_case("true"))
        {
            return false;
        }
        true
    });
    for c in n.children.borrow().iter() {
        clean(c, o);
    }
}
fn clean_attrs(n: &Handle) {
    if let NodeData::Element { attrs, .. } = &n.data {
        attrs.borrow_mut().retain(|a| {
            let key: &str = a.name.local.as_ref();
            if !a.name.ns.is_empty() || key.starts_with("on") {
                return false;
            }
            let allowed = matches!(
                key,
                "href"
                    | "src"
                    | "alt"
                    | "title"
                    | "lang"
                    | "name"
                    | "property"
                    | "content"
                    | "rel"
                    | "datetime"
                    | "colspan"
                    | "rowspan"
                    | "start"
                    | "type"
            );
            allowed && (!matches!(key, "href" | "src") || safe_url(&a.value))
        });
    }
    for c in n.children.borrow().iter() {
        clean_attrs(c);
    }
}
fn metadata(n: &Handle, out: &mut std::collections::BTreeMap<String, String>) {
    if useful_meta(n) {
        let pair = match tag(n) {
            "title" => Some(("title".into(), text(n))),
            "meta" => attr(n, "name")
                .or_else(|| attr(n, "property"))
                .zip(attr(n, "content")),
            "link" => attr(n, "href").map(|v| ("canonical".into(), v)),
            _ => None,
        };
        if let Some(p) = pair {
            out.entry(p.0).or_insert(p.1);
        }
    }
    for c in n.children.borrow().iter() {
        metadata(c, out);
    }
}
fn skim(input: &[u8], o: &Options) -> io::Result<String> {
    let dom = parse_document(RcDom::default(), Default::default())
        .from_utf8()
        .read_from(&mut &input[..])?;
    clean(&dom.document, o);
    // Select before removing class/id attributes. Prefer semantic containers, then common IDs/classes.
    let root = if o.main {
        find(&dom.document, &|n| {
            tag(n) == "main" || attr(n, "role").as_deref() == Some("main")
        })
        .or_else(|| find(&dom.document, &|n| tag(n) == "article"))
        .or_else(|| {
            find(&dom.document, &|n| {
                ["id", "class"].iter().any(|k| {
                    attr(n, k).is_some_and(|s| {
                        s.split_ascii_whitespace().any(|v| {
                            matches!(
                                v,
                                "content"
                                    | "main-content"
                                    | "post-content"
                                    | "entry-content"
                                    | "article-body"
                            )
                        })
                    })
                })
            })
        })
        .or_else(|| find(&dom.document, &|n| tag(n) == "body"))
        .unwrap_or_else(|| dom.document.clone())
    } else {
        dom.document.clone()
    };
    clean_attrs(&dom.document);
    let mut out = String::new();
    if !o.html && !o.only && !o.no_metadata {
        let mut meta = std::collections::BTreeMap::new();
        if let Some(head) = find(&dom.document, &|n| tag(n) == "head") {
            metadata(&head, &mut meta);
        }
        if !meta.is_empty() {
            out.push_str("---\n");
            // JSON strings are valid YAML scalars, including embedded newlines and delimiters.
            for (k, v) in meta {
                out.push_str(&format!(
                    "{}: {}\n",
                    serde_json::to_string(&k)?,
                    serde_json::to_string(&v)?
                ));
            }
            out.push_str("---\n\n");
        }
    }
    if o.only {
        render_assets(&dom.document, o, &mut out)?;
    } else if o.html {
        let mut bytes = Vec::new();
        serialize(
            &mut bytes,
            &SerializableHandle::from(root),
            Default::default(),
        )?;
        out = String::from_utf8(bytes).map_err(io::Error::other)?;
    } else {
        markdown(&root, o, &mut out, 0);
        out = out.trim().to_string();
        if !out.is_empty() {
            out.push('\n');
        }
    }
    Ok(out)
}
fn render_assets(n: &Handle, o: &Options, out: &mut String) -> io::Result<()> {
    if asset(n, o) {
        if o.html {
            let mut bytes = Vec::new();
            let opts = serialize::SerializeOpts {
                traversal_scope: serialize::TraversalScope::IncludeNode,
                ..Default::default()
            };
            serialize(&mut bytes, &SerializableHandle::from(n.clone()), opts)?;
            out.push_str(&String::from_utf8(bytes).map_err(io::Error::other)?);
            out.push('\n');
        } else {
            markdown(n, o, out, 0);
        }
    } else {
        for c in n.children.borrow().iter() {
            render_assets(c, o, out)?;
        }
    }
    Ok(())
}
fn gap(out: &mut String) {
    if !out.is_empty() && !out.ends_with("\n\n") {
        if !out.ends_with('\n') {
            out.push('\n');
        }
        out.push('\n');
    }
}
fn escaped(s: &str) -> String {
    let mut out = String::new();
    for (i, c) in s.char_indices() {
        if matches!(
            c,
            '\\' | '`' | '*' | '_' | '[' | ']' | '<' | '>' | '#' | '|' | '~'
        ) || (matches!(c, '-' | '+') && s.len() == 1)
            || (c == '.' && i > 0 && s[..i].bytes().all(|b| b.is_ascii_digit()))
        {
            out.push('\\');
        }
        out.push(c);
    }
    out
}
fn destination(s: &str) -> String {
    s.replace('\\', "%5C")
        .replace(' ', "%20")
        .replace('(', "%28")
        .replace(')', "%29")
        .replace('<', "%3C")
        .replace('>', "%3E")
        .replace(['\n', '\r', '\t'], "")
}
fn fence(s: &str) -> String {
    "`".repeat(
        s.split(|c| c != '`')
            .map(str::len)
            .max()
            .unwrap_or(0)
            .max(2)
            + 1,
    )
}
fn markdown(n: &Handle, o: &Options, out: &mut String, depth: usize) {
    if let NodeData::Text { contents } = &n.data {
        let s = contents.borrow();
        // Collapse HTML whitespace while preserving spaces across inline element boundaries.
        for part in s.split_inclusive(|c: char| c.is_ascii_whitespace()) {
            let word = part.trim_end_matches(|c: char| c.is_ascii_whitespace());
            out.push_str(&escaped(word));
            if part.len() != word.len() && !out.ends_with([' ', '\n']) {
                out.push(' ');
            }
        }
        return;
    }
    let t = tag(n);
    if t == "head" {
        // Head assets are opt-in too; metadata was emitted separately.
        for c in n.children.borrow().iter() {
            if asset(c, o) {
                markdown(c, o, out, depth);
            }
        }
        return;
    }
    if matches!(t, "title" | "meta" | "link") && !asset(n, o) {
        return;
    }
    if asset(n, o) {
        gap(out);
        if let Some(src) = attr(n, if t == "link" { "href" } else { "src" }) {
            out.push_str(&format!("{}: {}\n", t, escaped(&src)));
        }
        let raw = text(n);
        if !raw.is_empty() {
            let f = fence(&raw);
            out.push_str(&format!(
                "{f}{}\n{raw}\n{f}\n",
                if t == "script" { "javascript" } else { "css" }
            ));
        }
        gap(out);
        return;
    }
    if t == "pre" {
        gap(out);
        let raw = text(n);
        let f = fence(&raw);
        out.push_str(&format!("{f}\n{}\n{f}\n", raw.trim_end_matches('\n')));
        gap(out);
        return;
    }
    if t == "code" {
        let raw = text(n).replace('\n', " ");
        let f = "`".repeat(
            raw.split(|c| c != '`').map(str::len).max().unwrap_or(0) + 1,
        );
        out.push_str(&format!("{f} {raw} {f}"));
        return;
    }
    if t == "ol" {
        gap(out);
        let mut index = attr(n, "start")
            .and_then(|s| s.parse::<i64>().ok())
            .unwrap_or(1);
        for li in n.children.borrow().iter().filter(|c| tag(c) == "li") {
            if !out.ends_with('\n') && !out.is_empty() {
                out.push('\n');
            }
            out.push_str(&format!("{}{}. ", "  ".repeat(depth), index));
            for c in li.children.borrow().iter() {
                markdown(c, o, out, depth + 1);
            }
            out.push('\n');
            index = index.saturating_add(1);
        }
        gap(out);
        return;
    }
    let block = matches!(
        t,
        "p" | "div"
            | "section"
            | "article"
            | "main"
            | "header"
            | "footer"
            | "nav"
            | "aside"
            | "ul"
            | "ol"
            | "dl"
            | "table"
            | "h1"
            | "h2"
            | "h3"
            | "h4"
            | "h5"
            | "h6"
            | "blockquote"
    );
    if block {
        gap(out);
    }
    if matches!(t, "h1" | "h2" | "h3" | "h4" | "h5" | "h6") {
        out.push_str(&format!(
            "{} ",
            "#".repeat(t.as_bytes()[1] as usize - b'0' as usize)
        ));
    }
    if t == "br" {
        out.push_str("  \n");
        return;
    }
    if t == "hr" {
        gap(out);
        out.push_str("---\n\n");
        return;
    }
    if t == "li" {
        if !out.ends_with('\n') {
            out.push('\n');
        }
        out.push_str(&"  ".repeat(depth.saturating_sub(1)));
        out.push_str("- ");
    }
    if t == "tr" && !out.ends_with('\n') {
        out.push('\n');
    }
    if matches!(t, "td" | "th") {
        out.push_str("| ");
    }
    if t == "img" {
        if let Some(alt) = attr(n, "alt") {
            out.push_str(&escaped(&alt));
        }
        return;
    }
    let marker = match t {
        "strong" | "b" => "**",
        "em" | "i" => "*",
        "del" | "s" => "~~",
        _ => "",
    };
    if !marker.is_empty() {
        let mut inner = String::new();
        for c in n.children.borrow().iter() {
            markdown(c, o, &mut inner, depth);
        }
        let trimmed = inner.trim();
        if inner.starts_with(char::is_whitespace) && !out.ends_with([' ', '\n'])
        {
            out.push(' ');
        }
        if !trimmed.is_empty() {
            out.push_str(&format!("{marker}{trimmed}{marker}"));
        }
        if inner.ends_with(char::is_whitespace) {
            out.push(' ');
        }
        return;
    }
    let start = out.len();
    if t == "a" && !o.no_links && attr(n, "href").is_some() {
        out.push('[');
    }
    for c in n.children.borrow().iter() {
        markdown(c, o, out, depth + usize::from(matches!(t, "ul" | "ol")));
    }
    if t == "a"
        && !o.no_links
        && let Some(href) = attr(n, "href")
    {
        out.push_str(&format!("]({})", destination(&href)));
    }
    if t == "blockquote" {
        let inner = out.split_off(start);
        for line in inner.trim().lines() {
            out.push_str(&format!("> {line}\n"));
        }
    }
    if matches!(t, "td" | "th") {
        out.push(' ');
    }
    if t == "tr" {
        out.push_str("|\n");
    }
    if t == "li" {
        out.push('\n');
    }
    if block {
        gap(out);
    }
}
