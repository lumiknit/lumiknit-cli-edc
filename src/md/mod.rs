use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

pub const RESET: &str = "\x1b[0m";
pub const BOLD: &str = "\x1b[1m";
pub const ITALIC: &str = "\x1b[3m";
pub const STRIKE: &str = "\x1b[9m";
pub const RED: &str = "\x1b[31m";
pub const GREEN: &str = "\x1b[32m";
pub const BLUE: &str = "\x1b[34m";
pub const YELLOW: &str = "\x1b[33m";
pub const CYAN: &str = "\x1b[36m";
pub const BOLD_RED_UNDERLINE: &str = "\x1b[1;31;4m";

// ---- CodePrinter -----------------------------------------------------------

/// How to render the body of a fenced code block.
pub enum CodePrinter {
    /// Built-in: each line wrapped in green ANSI escape codes.
    Ansi,
    /// External command (program + pre-set args). The code content is piped
    /// to stdin. If the fenced language is non-empty, `--language=<lang>` is
    /// appended before `-` (stdin marker) automatically.
    External(Vec<String>),
}

impl CodePrinter {
    /// True if code lines can be printed as they arrive (no external command).
    pub fn streams(&self) -> bool {
        matches!(self, CodePrinter::Ansi)
    }

    /// Detect `bat` via PATH; fall back to built-in ANSI if not found.
    pub fn auto_detect() -> Self {
        if which::which("bat").is_ok() {
            Self::External(vec![
                "bat".into(),
                "--pager=never".into(),
                "--color=always".into(),
                "--style=plain".into(),
                "--theme=ansi".into(),
            ])
        } else {
            Self::Ansi
        }
    }

    /// Build from a command string (split on whitespace).
    /// E.g. `"bat --theme=Nord"` → `External(["bat", "--theme=Nord"])`.
    pub fn from_cmd(cmd: &str) -> Self {
        Self::External(cmd.split_whitespace().map(String::from).collect())
    }

    /// Print `content` (the full code block body, trailing newline included)
    /// with language hint `lang` (may be empty). On external-command failure,
    /// falls back to built-in ANSI rendering.
    pub fn print(&self, lang: &str, content: &str, out: &mut impl Write) {
        match self {
            CodePrinter::Ansi => print_ansi(content, out),
            CodePrinter::External(cmd) => {
                if let Some((prog, base_args)) = cmd.split_first() {
                    let mut args: Vec<&str> =
                        base_args.iter().map(String::as_str).collect();
                    let lang_flag;
                    if !lang.is_empty() {
                        lang_flag = format!("--language={lang}");
                        args.push(&lang_flag);
                    }
                    args.push("-");

                    let result = Command::new(prog)
                        .args(&args)
                        .stdin(Stdio::piped())
                        .stdout(Stdio::piped())
                        .stderr(Stdio::null())
                        .spawn()
                        .and_then(|mut child| {
                            child
                                .stdin
                                .take()
                                .ok_or_else(|| {
                                    std::io::Error::other(
                                        "failed to open child stdin",
                                    )
                                })?
                                .write_all(content.as_bytes())?;
                            let output = child.wait_with_output()?;
                            if output.status.success() {
                                Ok(output.stdout)
                            } else {
                                Err(std::io::Error::other("non-zero exit"))
                            }
                        });

                    match result {
                        Ok(bytes) => {
                            out.write_all(&bytes).ok();
                            return;
                        }
                        Err(_) => {} // fall through to ANSI fallback
                    }
                }
                print_ansi(content, out);
            }
        }
    }
}

fn print_ansi(content: &str, out: &mut impl Write) {
    for line in content.lines() {
        writeln!(out, "{GREEN}{line}{RESET}").ok();
    }
}

// ---- Inline rendering ------------------------------------------------------

pub fn lang_ext(lang: &str) -> &str {
    match lang.to_ascii_lowercase().as_str() {
        "c" => "c",
        "cpp" | "c++" => "cpp",
        "csharp" | "cs" | "c#" => "cs",
        "css" => "css",
        "dockerfile" => "dockerfile",
        "elixir" | "ex" => "ex",
        "fish" => "fish",
        "fsharp" | "fs" | "f#" => "fs",
        "go" => "go",
        "haskell" | "hs" => "hs",
        "html" => "html",
        "ini" => "ini",
        "java" => "java",
        "javascript" | "js" => "js",
        "json" => "json",
        "jsx" => "jsx",
        "kotlin" | "kt" => "kt",
        "lua" => "lua",
        "makefile" => "makefile",
        "nim" => "nim",
        "powershell" | "pwsh" | "ps1" => "ps1",
        "python" | "py" => "py",
        "r" => "r",
        "ruby" | "rb" => "rb",
        "rust" | "rs" => "rs",
        "scala" => "scala",
        "scss" => "scss",
        "shell" | "sh" | "bash" => "sh",
        "sql" => "sql",
        "swift" => "swift",
        "toml" => "toml",
        "tsx" => "tsx",
        "typescript" | "ts" => "ts",
        "yaml" | "yml" => "yaml",
        "zsh" => "zsh",
        _ => "txt",
    }
}

pub fn alloc_filename(prefix: &str, ext: &str) -> String {
    for i in 1.. {
        let name = format!("{prefix}{i:03}.{ext}");
        if !Path::new(&name).exists() {
            return name;
        }
    }
    unreachable!()
}

// ---- ANSI stripping (for -M / NO_COLOR) ------------------------------------

/// Writer that drops CSI (`ESC [ ... final`) and OSC (`ESC ] ... BEL|ESC \`)
/// sequences. Stateful, so sequences split across writes are handled.
pub struct StripAnsi<W: Write> {
    inner: W,
    state: u8,
}

impl<W: Write> StripAnsi<W> {
    pub fn new(inner: W) -> Self {
        StripAnsi { inner, state: 0 }
    }
}

impl<W: Write> Write for StripAnsi<W> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let mut v = Vec::with_capacity(buf.len());
        for &b in buf {
            match self.state {
                0 => {
                    if b == 0x1b {
                        self.state = 1;
                    } else {
                        v.push(b);
                    }
                }
                1 => {
                    self.state = match b {
                        b'[' => 2,
                        b']' => 3,
                        _ => 0,
                    }
                }
                2 => {
                    if (0x40..=0x7e).contains(&b) {
                        self.state = 0;
                    }
                }
                3 => {
                    if b == 0x07 {
                        self.state = 0;
                    } else if b == 0x1b {
                        self.state = 4;
                    }
                }
                _ => self.state = 0,
            }
        }
        self.inner.write_all(&v)?;
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}

// ---- Inline rendering ------------------------------------------------------

/// Emphasis state. Persisted across the lines of a block so that
/// `**bold\ncontinued**` works while still emitting each line immediately.
#[derive(Clone, Copy, Default, PartialEq, Debug)]
pub struct InlineState {
    pub bold: bool,
    pub italic: bool,
    pub strike: bool,
}

impl InlineState {
    pub fn active(&self) -> bool {
        self.bold || self.italic || self.strike
    }
}

pub fn apply(out: &mut impl Write, st: &InlineState) {
    apply_base(out, st, "");
}

/// Reset, then restore the block's base style followed by the inline state.
fn apply_base(out: &mut impl Write, st: &InlineState, base: &str) {
    write!(out, "{RESET}{base}").ok();
    if st.bold {
        write!(out, "{BOLD}").ok();
    }
    if st.italic {
        write!(out, "{ITALIC}").ok();
    }
    if st.strike {
        write!(out, "{STRIKE}{RED}").ok();
    }
}

/// Visible text stays `[label](url)`; OSC 8 hyperlink is added around it.
fn osc8_link(url: &str, label: &str, out: &mut impl Write) {
    let linkable = !url.chars().any(|c| c.is_control());
    if linkable {
        write!(out, "\x1b]8;;{url}\x1b\\").ok();
    }
    write!(out, "{CYAN}\x1b[4m[{label}]({url}){RESET}").ok();
    if linkable {
        write!(out, "\x1b]8;;\x1b\\").ok();
    }
}

/// After a span that ended with RESET: restore base style and emphasis.
fn restyle(out: &mut impl Write, st: &InlineState, base: &str) {
    if st.active() || !base.is_empty() {
        apply_base(out, st, base);
    }
}

fn is_ws(c: Option<char>) -> bool {
    c.is_none_or(|c| c.is_whitespace())
}

fn is_alnum(c: Option<char>) -> bool {
    c.is_some_and(|c| c.is_alphanumeric())
}

fn run_len(chars: &[char], i: usize) -> usize {
    chars[i..].iter().take_while(|&&c| c == chars[i]).count()
}

/// Start index of the next backtick run of exactly `n` at or after `from`.
fn find_closing_ticks(chars: &[char], from: usize, n: usize) -> Option<usize> {
    let mut j = from;
    while j < chars.len() {
        if chars[j] == '`' {
            let m = run_len(chars, j);
            if m == n {
                return Some(j);
            }
            j += m;
        } else {
            j += 1;
        }
    }
    None
}

/// Parse `[label](url)` starting at `i` (on `[`). Parentheses in the url may
/// nest. Returns (label, url, index after the closing paren).
fn parse_link(chars: &[char], i: usize) -> Option<(String, String, usize)> {
    let mut close = i + 1;
    loop {
        match chars.get(close)? {
            '\\' => close += 2,
            ']' => break,
            _ => close += 1,
        }
    }
    if chars.get(close + 1) != Some(&'(') {
        return None;
    }
    let mut depth = 1;
    let mut j = close + 2;
    while j < chars.len() {
        match chars[j] {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    break;
                }
            }
            _ => {}
        }
        j += 1;
    }
    if j >= chars.len() {
        return None;
    }
    Some((
        chars[i + 1..close].iter().collect(),
        chars[close + 2..j].iter().collect(),
        j + 1,
    ))
}

pub fn render_inline(text: &str, out: &mut impl Write) {
    render_inline_with(text, &mut InlineState::default(), out);
}

/// Render one line of inline markup, continuing from (and updating) `st`.
/// Output is emitted immediately; styles are reset at the end of the line and
/// re-applied at the start of the next one.
pub fn render_inline_with(
    text: &str,
    st: &mut InlineState,
    out: &mut impl Write,
) {
    render_inline_base(text, st, "", out);
}

/// Like `render_inline_with`, but `base` (an escape sequence, e.g. a heading
/// style) is restored after every internal RESET.
pub fn render_inline_base(
    text: &str,
    st: &mut InlineState,
    base: &str,
    out: &mut impl Write,
) {
    let chars: Vec<char> = text.chars().collect();
    if st.active() {
        apply_base(out, st, base);
    }
    let mut i = 0;
    let mut prev: Option<char> = Some(' ');

    while i < chars.len() {
        let c = chars[i];
        let next1 = chars.get(i + 1).copied();

        // backslash escape
        if c == '\\' && next1.is_some_and(|d| d.is_ascii_punctuation()) {
            let d = next1.unwrap();
            write!(out, "{d}").ok();
            prev = Some(d);
            i += 2;
            continue;
        }

        // bare URL
        if c == 'h' && !is_alnum(prev) {
            let head: String = chars[i..].iter().take(8).collect();
            if head.starts_with("http://") || head.starts_with("https://") {
                let mut end = chars[i..]
                    .iter()
                    .position(|c| c.is_whitespace())
                    .map_or(chars.len(), |p| i + p);
                while end > i {
                    let l = chars[end - 1];
                    let open =
                        chars[i..end].iter().filter(|&&c| c == '(').count();
                    let close =
                        chars[i..end].iter().filter(|&&c| c == ')').count();
                    if ".,;:!?'\"".contains(l) || (l == ')' && close > open) {
                        end -= 1;
                    } else {
                        break;
                    }
                }
                let url: String = chars[i..end].iter().collect();
                osc8_link_bare(&url, out);
                restyle(out, st, base);
                prev = chars.get(end - 1).copied();
                i = end;
                continue;
            }
        }

        match c {
            '`' => {
                let n = run_len(&chars, i);
                if let Some(j) = find_closing_ticks(&chars, i + n, n) {
                    let mut body: String = chars[i + n..j].iter().collect();
                    if body.len() > 2
                        && body.starts_with(' ')
                        && body.ends_with(' ')
                        && !body.trim().is_empty()
                    {
                        body = body[1..body.len() - 1].to_string();
                    }
                    write!(out, "{RESET}{GREEN}{body}").ok();
                    // Always reset the green, even when no emphasis is active,
                    // so the color does not bleed into the rest of the line.
                    apply_base(out, st, base);
                    prev = Some('`');
                    i = j + n;
                } else {
                    write!(out, "{}", "`".repeat(n)).ok();
                    prev = Some('`');
                    i += n;
                }
            }
            '*' | '_' => {
                let n = run_len(&chars, i);
                let next = chars.get(i + n).copied();
                let (can_open, can_close) = if c == '*' {
                    (!is_ws(next), !is_ws(prev))
                } else {
                    (
                        !is_ws(next) && !is_alnum(prev),
                        !is_ws(prev) && !is_alnum(next),
                    )
                };
                let mut left = n;
                let mut changed = false;
                if can_close {
                    if left >= 3 && st.bold && st.italic {
                        st.bold = false;
                        st.italic = false;
                        left -= 3;
                        changed = true;
                    }
                    if left >= 2 && st.bold {
                        st.bold = false;
                        left -= 2;
                        changed = true;
                    }
                    if left >= 1 && st.italic {
                        st.italic = false;
                        left -= 1;
                        changed = true;
                    }
                }
                if !changed && can_open {
                    if left >= 3 {
                        st.bold = true;
                        st.italic = true;
                        left -= 3;
                        changed = true;
                    } else if left == 2 && !st.bold {
                        st.bold = true;
                        left -= 2;
                        changed = true;
                    } else if left == 1 && !st.italic {
                        st.italic = true;
                        left -= 1;
                        changed = true;
                    }
                }
                if changed {
                    apply_base(out, st, base);
                }
                if left > 0 {
                    write!(out, "{}", c.to_string().repeat(left)).ok();
                }
                prev = Some(c);
                i += n;
            }
            '~' if next1 == Some('~') => {
                let next = chars.get(i + 2).copied();
                if st.strike && !is_ws(prev) {
                    st.strike = false;
                    apply_base(out, st, base);
                } else if !st.strike && !is_ws(next) {
                    st.strike = true;
                    apply_base(out, st, base);
                } else {
                    write!(out, "~~").ok();
                }
                prev = Some('~');
                i += 2;
            }
            '[' => {
                if let Some((label, url, end)) = parse_link(&chars, i) {
                    osc8_link(&url, &label, out);
                    restyle(out, st, base);
                    prev = Some(')');
                    i = end;
                } else {
                    write!(out, "[").ok();
                    prev = Some('[');
                    i += 1;
                }
            }
            '!' if next1 == Some('[') => {
                if let Some((alt, url, end)) = parse_link(&chars, i + 1) {
                    let label = if alt.is_empty() { "image" } else { &alt };
                    write!(out, "🖼 ").ok();
                    osc8_link(&url, label, out);
                    restyle(out, st, base);
                    prev = Some(')');
                    i = end;
                } else {
                    write!(out, "!").ok();
                    prev = Some('!');
                    i += 1;
                }
            }
            c => {
                write!(out, "{c}").ok();
                prev = Some(c);
                i += 1;
            }
        }
    }
    if st.active() {
        write!(out, "{RESET}").ok();
    }
}

fn osc8_link_bare(url: &str, out: &mut impl Write) {
    let linkable = !url.chars().any(|c| c.is_control());
    if linkable {
        write!(out, "\x1b]8;;{url}\x1b\\").ok();
    }
    write!(out, "{CYAN}\x1b[4m{url}{RESET}").ok();
    if linkable {
        write!(out, "\x1b]8;;\x1b\\").ok();
    }
}

pub fn is_hr(line: &str) -> bool {
    let t = line.trim();
    if t.len() < 3 {
        return false;
    }
    let c = match t.chars().next() {
        Some(c @ ('-' | '*' | '_')) => c,
        _ => return false,
    };
    t.chars().all(|x| x == c || x == ' ')
        && t.chars().filter(|&x| x == c).count() >= 3
}

// ---- Block helpers ---------------------------------------------------------

/// Opening fence: (indent, fence char, fence length, info string).
fn parse_fence(line: &str) -> Option<(usize, char, usize, &str)> {
    let t = line.trim_start();
    let indent = line.len() - t.len();
    let c = t.chars().next()?;
    if c != '`' && c != '~' {
        return None;
    }
    let len = t.chars().take_while(|&x| x == c).count();
    if len < 3 {
        return None;
    }
    let info = t[len..].trim();
    if c == '`' && info.contains('`') {
        return None;
    }
    Some((indent, c, len, info))
}

fn is_closing_fence(line: &str, c: char, len: usize) -> bool {
    let t = line.trim_start();
    let n = t.chars().take_while(|&x| x == c).count();
    n >= len && t[n..].trim().is_empty()
}

/// List item marker: (indent, marker byte length, ordered number).
fn parse_list(line: &str) -> Option<(usize, usize, Option<u64>)> {
    let t = line.trim_start_matches(' ');
    let indent = line.len() - t.len();
    let b = t.as_bytes();
    if matches!(b.first(), Some(b'-' | b'*' | b'+')) && b.get(1) == Some(&b' ')
    {
        return Some((indent, 1, None));
    }
    let digits = b.iter().take_while(|b| b.is_ascii_digit()).count();
    if (1..=9).contains(&digits)
        && matches!(b.get(digits), Some(b'.' | b')'))
        && b.get(digits + 1) == Some(&b' ')
    {
        let n = t[..digits].parse().ok()?;
        return Some((indent, digits + 1, Some(n)));
    }
    None
}

// ---- Block renderer --------------------------------------------------------

pub struct CodeBlock {
    pub filename: String,
    pub lang: String,
    pub lines: Vec<String>,
    pub fence_char: char,
    pub fence_len: usize,
    pub indent: usize,
}

#[derive(Clone, Copy, PartialEq)]
pub enum BlockKind {
    None,
    Para,
    List,
    Heading,
    Code,
    Blockquote,
    Hr,
}

pub struct Renderer {
    pub dry: bool,
    pub prefix: String,
    pub printer: CodePrinter,
    pub in_code_block: Option<CodeBlock>,
    pub needs_newline: bool,
    pub last_kind: BlockKind,
    pub had_blank: bool,
    pub inline: InlineState,
    pub quote_depth: usize,
}

impl Renderer {
    pub fn new(dry: bool, prefix: String, printer: CodePrinter) -> Self {
        Renderer {
            dry,
            prefix,
            printer,
            in_code_block: None,
            needs_newline: false,
            last_kind: BlockKind::None,
            had_blank: false,
            inline: InlineState::default(),
            quote_depth: 0,
        }
    }

    pub fn flush_para(&mut self, out: &mut impl Write) {
        if self.needs_newline {
            writeln!(out).ok();
            self.needs_newline = false;
        }
    }

    pub fn begin_block(&mut self, kind: BlockKind, out: &mut impl Write) {
        self.flush_para(out);
        let need_blank = self.last_kind != BlockKind::None
            && (self.last_kind != kind
                || (kind == BlockKind::Para && self.had_blank));
        if need_blank {
            writeln!(out).ok();
        }
        self.last_kind = kind;
        self.had_blank = false;
        self.inline = InlineState::default();
    }

    /// Finish the open code block: print (if not already streamed), the
    /// closing fence line (if any) and save the file.
    fn close_code(&mut self, closing: Option<&str>, out: &mut impl Write) {
        let Some(blk) = self.in_code_block.take() else {
            return;
        };
        let content = if blk.lines.is_empty() {
            String::new()
        } else {
            blk.lines.join("\n") + "\n"
        };
        if !self.printer.streams() {
            self.printer.print(&blk.lang, &content, out);
        }
        if let Some(line) = closing {
            writeln!(out, "{BOLD}{BLUE}{line}{RESET}").ok();
        }
        out.flush().ok();
        if !self.dry {
            std::fs::write(&blk.filename, content).ok();
        }
    }

    pub fn render_line(&mut self, line: &str, out: &mut impl Write) {
        if let Some(blk) = self.in_code_block.as_mut() {
            if is_closing_fence(line, blk.fence_char, blk.fence_len) {
                self.close_code(Some(line), out);
            } else {
                let skip = line
                    .bytes()
                    .take(blk.indent)
                    .take_while(|&b| b == b' ')
                    .count();
                let l = &line[skip..];
                blk.lines.push(l.to_string());
                if self.printer.streams() {
                    writeln!(out, "{GREEN}{l}{RESET}").ok();
                    out.flush().ok();
                }
            }
            return;
        }

        if let Some((indent, fence_char, fence_len, info)) = parse_fence(line) {
            self.begin_block(BlockKind::Code, out);
            let fence = fence_char.to_string().repeat(fence_len);
            let lang = info.split_whitespace().next().unwrap_or("").to_string();
            let ext = lang_ext(&lang);
            let filename = if self.dry {
                format!("{}???.{ext}", self.prefix)
            } else {
                alloc_filename(&self.prefix, ext)
            };
            writeln!(
                out,
                "{:indent$}{BOLD}{BLUE}{fence}{lang}  \x1b[4m{filename}\x1b[24m{RESET}",
                ""
            )
            .ok();
            out.flush().ok();
            self.in_code_block = Some(CodeBlock {
                filename,
                lang,
                lines: vec![],
                fence_char,
                fence_len,
                indent,
            });
            return;
        }

        if line.trim().is_empty() {
            self.flush_para(out);
            self.had_blank = true;
            return;
        }

        if is_hr(line) {
            self.begin_block(BlockKind::Hr, out);
            writeln!(out, "{BLUE}{}{RESET}", line.trim()).ok();
            out.flush().ok();
            return;
        }

        let t = line.trim_start();
        let indent = line.len() - t.len();

        let hashes = t.bytes().take_while(|&b| b == b'#').count();
        if indent <= 3
            && (1..=6).contains(&hashes)
            && t.as_bytes().get(hashes) == Some(&b' ')
        {
            self.begin_block(BlockKind::Heading, out);
            write!(out, "{BOLD_RED_UNDERLINE}").ok();
            let mut st = InlineState::default();
            render_inline_base(t, &mut st, BOLD_RED_UNDERLINE, out);
            writeln!(out, "{RESET}").ok();
            out.flush().ok();
            return;
        }

        if indent <= 3 && t.starts_with('>') {
            let mut depth = 0;
            let mut rest = t;
            while let Some(r) = rest.strip_prefix('>') {
                depth += 1;
                rest = r.strip_prefix(' ').unwrap_or(r);
            }
            let marker = ">".repeat(depth);
            if rest.trim().is_empty() {
                self.begin_block(BlockKind::Blockquote, out);
                writeln!(out, "{YELLOW}{marker}{RESET}").ok();
                self.quote_depth = depth;
            } else if self.needs_newline
                && self.last_kind == BlockKind::Blockquote
                && depth == self.quote_depth
            {
                write!(out, " ").ok();
                let mut st = self.inline;
                render_inline_with(rest.trim(), &mut st, out);
                self.inline = st;
            } else {
                self.begin_block(BlockKind::Blockquote, out);
                self.quote_depth = depth;
                write!(out, "{YELLOW}{marker}{RESET} ").ok();
                let mut st = self.inline;
                render_inline_with(rest.trim_end(), &mut st, out);
                self.inline = st;
                self.needs_newline = true;
            }
            out.flush().ok();
            return;
        }

        if let Some((indent, mlen, num)) = parse_list(line) {
            // only "1." may interrupt a paragraph
            let in_para =
                self.needs_newline && self.last_kind == BlockKind::Para;
            if !(in_para && num.is_some_and(|n| n != 1)) {
                self.begin_block(BlockKind::List, out);
                let marker = &line[indent..indent + mlen];
                write!(out, "{:indent$}{YELLOW}{marker}{RESET}", "").ok();
                let mut st = self.inline;
                render_inline_with(&line[indent + mlen..], &mut st, out);
                self.inline = st;
                self.needs_newline = true;
                out.flush().ok();
                return;
            }
        }

        // paragraph / continuation: a soft line break (newline + indentation)
        // is rendered as a single space
        let continues = self.needs_newline
            && matches!(
                self.last_kind,
                BlockKind::Para | BlockKind::List | BlockKind::Blockquote
            );
        let mut st = self.inline;
        if continues {
            write!(out, " ").ok();
            render_inline_with(line.trim(), &mut st, out);
        } else {
            self.begin_block(BlockKind::Para, out);
            st = self.inline;
            render_inline_with(line.trim(), &mut st, out);
        }
        self.inline = st;
        self.needs_newline = true;
        out.flush().ok();
    }

    pub fn finish(&mut self, out: &mut impl Write) {
        self.close_code(None, out);
        self.flush_para(out);
        self.last_kind = BlockKind::None;
        self.had_blank = false;
        self.inline = InlineState::default();
        self.quote_depth = 0;
        out.flush().ok();
    }
}
