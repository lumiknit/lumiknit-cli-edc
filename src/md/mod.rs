use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

pub const RESET: &str = "\x1b[0m";
pub const BOLD: &str = "\x1b[1m";
pub const ITALIC: &str = "\x1b[3m";
pub const STRIKE: &str = "\x1b[9m";
pub const RED: &str = "\x1b[31m";
pub const GREEN: &str = "\x1b[32m";
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
                        .spawn()
                        .and_then(|mut child| {
                            child
                                .stdin
                                .take()
                                .ok_or_else(|| {
                                    std::io::Error::other("failed to open child stdin")
                                })?
                                .write_all(content.as_bytes())?;
                            Ok(child.wait_with_output()?.stdout)
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

pub fn apply(out: &mut impl Write, bold: bool, italic: bool, code: bool, strike: bool) {
    write!(out, "{RESET}").ok();
    if code {
        write!(out, "{GREEN}").ok();
        return;
    }
    if bold {
        write!(out, "{BOLD}").ok();
    }
    if italic {
        write!(out, "{ITALIC}").ok();
    }
    if strike {
        write!(out, "{STRIKE}{RED}").ok();
    }
}

pub fn render_inline(text: &str, out: &mut impl Write) {
    let mut chars = text.chars().peekable();
    let (mut bold, mut italic, mut code, mut strike) = (false, false, false, false);

    while let Some(c) = chars.next() {
        match c {
            '`' => {
                code = !code;
                apply(out, bold, italic, code, strike);
            }
            '*' | '_' if !code => {
                if chars.peek() == Some(&c) {
                    chars.next();
                    bold = !bold;
                } else {
                    italic = !italic;
                }
                apply(out, bold, italic, code, strike);
            }
            '~' if !code && chars.peek() == Some(&'~') => {
                chars.next();
                strike = !strike;
                apply(out, bold, italic, code, strike);
            }
            '[' if !code => {
                let mut link_text = String::new();
                let mut consumed = vec!['['];
                let mut is_link = false;
                let mut url = String::new();
                loop {
                    match chars.next() {
                        None => break,
                        Some(']') => {
                            consumed.push(']');
                            if chars.peek() == Some(&'(') {
                                chars.next();
                                loop {
                                    match chars.next() {
                                        None => break,
                                        Some(')') => {
                                            is_link = true;
                                            break;
                                        }
                                        Some(c) => url.push(c),
                                    }
                                }
                            }
                            break;
                        }
                        Some(c) => {
                            link_text.push(c);
                            consumed.push(c);
                        }
                    }
                }
                if is_link {
                    write!(out, "{CYAN}[{link_text}]({url}){RESET}").ok();
                    apply(out, bold, italic, code, strike);
                } else {
                    for c in consumed {
                        write!(out, "{c}").ok();
                    }
                }
            }
            c => {
                write!(out, "{c}").ok();
            }
        }
    }
    if bold || italic || code || strike {
        write!(out, "{RESET}").ok();
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

// ---- Block renderer --------------------------------------------------------

pub struct CodeBlock {
    pub filename: String,
    pub lang: String,
    pub lines: Vec<String>,
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
    }

    pub fn render_line(&mut self, line: &str, out: &mut impl Write) {
        if let Some(ref mut blk) = self.in_code_block {
            if line.starts_with("```") || line.starts_with("~~~") {
                let filename = blk.filename.clone();
                let lang = blk.lang.clone();
                let content = blk.lines.join("\n") + "\n";
                self.in_code_block = None;

                self.printer.print(&lang, &content, out);
                writeln!(out, "{GREEN}{line}{RESET}").ok();
                out.flush().ok();

                if !self.dry {
                    std::fs::write(&filename, content).ok();
                }
            } else {
                blk.lines.push(line.to_string());
            }
            return;
        }

        if line.starts_with("```") || line.starts_with("~~~") {
            self.begin_block(BlockKind::Code, out);
            let fence_char = &line[..3];
            let lang = line[3..].trim().to_string();
            let ext = lang_ext(&lang);
            let filename = if self.dry {
                format!("{}???.{ext}", self.prefix)
            } else {
                alloc_filename(&self.prefix, ext)
            };
            let display = if lang.is_empty() {
                format!("{fence_char}  \x1b[4m{filename}\x1b[24m")
            } else {
                format!("{fence_char}{lang}  \x1b[4m{filename}\x1b[24m")
            };
            writeln!(out, "{GREEN}{display}{RESET}").ok();
            out.flush().ok();
            self.in_code_block = Some(CodeBlock { filename, lang, lines: vec![] });
            return;
        }

        if line.trim().is_empty() {
            self.flush_para(out);
            self.had_blank = true;
            return;
        }

        if is_hr(line) {
            self.begin_block(BlockKind::Hr, out);
            writeln!(out, "{CYAN}{line}{RESET}").ok();
            out.flush().ok();
            return;
        }

        let hashes = line.bytes().take_while(|&b| b == b'#').count();
        if hashes > 0 && line.as_bytes().get(hashes) == Some(&b' ') {
            self.begin_block(BlockKind::Heading, out);
            write!(out, "{BOLD_RED_UNDERLINE}").ok();
            render_inline(line, out);
            writeln!(out, "{RESET}").ok();
            out.flush().ok();
            return;
        }

        if let Some(rest) = line.strip_prefix('>') {
            self.begin_block(BlockKind::Blockquote, out);
            write!(out, "{YELLOW}>{RESET}").ok();
            render_inline(rest, out);
            writeln!(out).ok();
            out.flush().ok();
            return;
        }

        if line.starts_with("- ") || line.starts_with("* ") || line.starts_with("+ ") {
            self.begin_block(BlockKind::List, out);
            write!(out, "{YELLOW}{}{RESET}", &line[..1]).ok();
            render_inline(&line[1..], out);
            writeln!(out).ok();
            out.flush().ok();
            return;
        }

        let digits = line.bytes().take_while(|b| b.is_ascii_digit()).count();
        if digits > 0
            && line.as_bytes().get(digits) == Some(&b'.')
            && line.as_bytes().get(digits + 1) == Some(&b' ')
        {
            self.begin_block(BlockKind::List, out);
            write!(out, "{YELLOW}{}.{RESET}", &line[..digits]).ok();
            render_inline(&line[digits + 1..], out);
            writeln!(out).ok();
            out.flush().ok();
            return;
        }

        // paragraph
        self.begin_block(BlockKind::Para, out);
        if self.needs_newline {
            write!(out, " ").ok();
        }
        render_inline(line, out);
        self.needs_newline = true;
        out.flush().ok();
    }

    pub fn finish(&mut self, out: &mut impl Write) {
        self.flush_para(out);
        out.flush().ok();
    }
}
