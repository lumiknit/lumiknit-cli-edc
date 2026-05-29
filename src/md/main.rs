use lumiknit_cli_edc::version::version_description;
use std::io::{self, BufRead, Write};
use std::path::Path;

const RESET: &str = "\x1b[0m";
const BOLD: &str = "\x1b[1m";
const ITALIC: &str = "\x1b[3m";
const STRIKE: &str = "\x1b[9m";
const RED: &str = "\x1b[31m";
const GREEN: &str = "\x1b[32m";
const YELLOW: &str = "\x1b[33m";
const CYAN: &str = "\x1b[36m";
const BOLD_RED_UNDERLINE: &str = "\x1b[1;31;4m";

const HELP: &str = "\
Usage: md [-dry]

Reads markdown from stdin and renders with ANSI colors.
Code blocks are saved to files automatically.
  Opening fence is shown as: ```lang  md-code-NNN.ext

Options:
  -h      show this help
  -v      show version
  -dry    render only, do not write code block files
";

fn lang_ext(lang: &str) -> &str {
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

fn alloc_filename(ext: &str) -> String {
    for i in 1.. {
        let name = format!("md-code-{i:03}.{ext}");
        if !Path::new(&name).exists() {
            return name;
        }
    }
    unreachable!()
}

fn apply(
    out: &mut impl Write,
    bold: bool,
    italic: bool,
    code: bool,
    strike: bool,
) {
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

fn render_inline(text: &str, out: &mut impl Write) {
    let mut chars = text.chars().peekable();
    let (mut bold, mut italic, mut code, mut strike) =
        (false, false, false, false);

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

fn is_hr(line: &str) -> bool {
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

struct CodeBlock {
    filename: String,
    lines: Vec<String>,
}

struct Renderer {
    dry: bool,
    in_code_block: Option<CodeBlock>,
    needs_newline: bool,
    pending_blank: bool,
}

impl Renderer {
    fn new(dry: bool) -> Self {
        Renderer {
            dry,
            in_code_block: None,
            needs_newline: false,
            pending_blank: false,
        }
    }

    fn flush_para(&mut self, out: &mut impl Write) {
        if self.needs_newline {
            writeln!(out).ok();
            self.needs_newline = false;
        }
        if self.pending_blank {
            writeln!(out).ok();
            self.pending_blank = false;
        }
    }

    fn render_line(&mut self, line: &str, out: &mut impl Write) {
        // inside code block
        if let Some(ref mut blk) = self.in_code_block {
            if line.starts_with("```") || line.starts_with("~~~") {
                // closing fence
                let filename = blk.filename.clone();
                let content = blk.lines.join("\n") + "\n";
                writeln!(out, "{GREEN}{line}{RESET}").ok();
                out.flush().ok();
                if !self.dry {
                    std::fs::write(&filename, content).ok();
                }
                self.in_code_block = None;
            } else {
                blk.lines.push(line.to_string());
                writeln!(out, "{GREEN}{line}{RESET}").ok();
                out.flush().ok();
            }
            return;
        }

        // opening code fence
        if line.starts_with("```") || line.starts_with("~~~") {
            self.flush_para(out);
            let fence_char = &line[..3];
            let lang = line[3..].trim().to_string();
            let ext = lang_ext(&lang);
            let filename = if self.dry {
                format!("md-code-???.{ext}")
            } else {
                alloc_filename(ext)
            };
            let display = if lang.is_empty() {
                format!("{fence_char}  \x1b[4m{filename}\x1b[24m")
            } else {
                format!("{fence_char}{lang}  \x1b[4m{filename}\x1b[24m")
            };
            writeln!(out, "{GREEN}{display}{RESET}").ok();
            out.flush().ok();
            self.in_code_block = Some(CodeBlock {
                filename,
                lines: vec![],
            });
            return;
        }

        // blank line
        if line.trim().is_empty() {
            if self.needs_newline {
                self.needs_newline = false;
                self.pending_blank = true;
            }
            return;
        }

        // HR
        if is_hr(line) {
            self.flush_para(out);
            writeln!(out, "{CYAN}{line}{RESET}").ok();
            out.flush().ok();
            return;
        }

        // heading
        let hashes = line.bytes().take_while(|&b| b == b'#').count();
        if hashes > 0 && line.as_bytes().get(hashes) == Some(&b' ') {
            self.flush_para(out);
            write!(out, "{BOLD_RED_UNDERLINE}").ok();
            render_inline(line, out);
            writeln!(out, "{RESET}").ok();
            out.flush().ok();
            return;
        }

        // blockquote
        if let Some(rest) = line.strip_prefix('>') {
            self.flush_para(out);
            write!(out, "{YELLOW}>{RESET}").ok();
            render_inline(rest, out);
            writeln!(out).ok();
            out.flush().ok();
            return;
        }

        // unordered list
        if line.starts_with("- ")
            || line.starts_with("* ")
            || line.starts_with("+ ")
        {
            self.flush_para(out);
            write!(out, "{YELLOW}{}{RESET}", &line[..1]).ok();
            render_inline(&line[1..], out);
            writeln!(out).ok();
            out.flush().ok();
            return;
        }

        // ordered list
        let digits = line.bytes().take_while(|b| b.is_ascii_digit()).count();
        if digits > 0
            && line.as_bytes().get(digits) == Some(&b'.')
            && line.as_bytes().get(digits + 1) == Some(&b' ')
        {
            self.flush_para(out);
            write!(out, "{YELLOW}{}.{RESET}", &line[..digits]).ok();
            render_inline(&line[digits + 1..], out);
            writeln!(out).ok();
            out.flush().ok();
            return;
        }

        // normal paragraph
        if self.pending_blank {
            writeln!(out).ok();
            self.pending_blank = false;
            self.needs_newline = false;
        } else if self.needs_newline {
            write!(out, " ").ok();
        }
        render_inline(line, out);
        self.needs_newline = true;
        out.flush().ok();
    }

    fn finish(&mut self, out: &mut impl Write) {
        if self.needs_newline {
            writeln!(out).ok();
            out.flush().ok();
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let ver = version_description("md", "markdown ANSI renderer");

    if args.iter().any(|a| a == "-v") {
        println!("{ver}");
        return;
    }
    if args.iter().any(|a| a == "-h" || a == "--help") {
        println!("{ver}\n");
        print!("{HELP}");
        return;
    }
    let dry = args.iter().any(|a| a == "-dry");

    let stdin = io::stdin();
    let mut out = io::stdout();
    let mut r = Renderer::new(dry);

    for line in stdin.lock().lines().map_while(Result::ok) {
        r.render_line(&line, &mut out);
    }
    r.finish(&mut out);
}
