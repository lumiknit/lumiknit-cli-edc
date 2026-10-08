use lumiknit_cli_edc::md::*;
use lumiknit_cli_edc::version::version_description;
use std::io::{self, IsTerminal, Read, Write};

const HELP: &str = include_str!("help.txt");

fn main() {
    let ver = version_description("md", "Markdown ANSI renderer");

    let mut dry = false;
    let mut plain = false;
    let mut prefix = ".md-".to_string();
    let mut code_printer: Option<CodePrinter> = None;
    let mut files: Vec<String> = Vec::new();
    let mut iter = std::env::args().skip(1).peekable();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "-v" => {
                println!("{ver}");
                return;
            }
            "-h" | "--help" => {
                println!("{ver}\n");
                print!("{HELP}");
                return;
            }
            "-dry" => dry = true,
            "-M" => plain = true,
            "-p" => {
                prefix = iter.next().unwrap_or_else(|| {
                    eprintln!("-p requires an argument");
                    std::process::exit(1);
                });
            }
            "-C" => {
                let cmd = iter.next().unwrap_or_else(|| {
                    eprintln!("-C requires a command argument");
                    std::process::exit(1);
                });
                code_printer = Some(CodePrinter::from_cmd(&cmd));
            }
            _ => files.push(arg),
        }
    }

    let printer = code_printer.unwrap_or_else(CodePrinter::auto_detect);

    let mut r = Renderer::new(dry, prefix, printer);
    let plain = plain || std::env::var_os("NO_COLOR").is_some_and(|v| !v.is_empty());
    if plain {
        run(&files, &mut r, &mut StripAnsi::new(io::stdout()));
    } else {
        run(&files, &mut r, &mut io::stdout());
    }
}

fn run<W: Write>(files: &[String], r: &mut Renderer, out: &mut W) {

    let mut first = true;
    for path in files {
        let f = std::fs::File::open(path).unwrap_or_else(|e| {
            eprintln!("{path}: {e}");
            std::process::exit(1);
        });
        if !first {
            write!(out, "\n\n").ok();
        }
        first = false;
        render_reader(f, r, out);
    }

    if files.is_empty() || !io::stdin().is_terminal() {
        if !first {
            write!(out, "\n\n").ok();
        }
        render_reader(io::stdin().lock(), r, out);
    }
}

fn render_reader(mut input: impl Read, r: &mut Renderer, out: &mut impl Write) {
    let mut raw = [0u8; 4096];
    let mut line_buf: Vec<u8> = Vec::new();

    loop {
        let n = input.read(&mut raw).expect("read failed");
        if n == 0 {
            break;
        }
        for &b in &raw[..n] {
            if b == b'\n' {
                let line = String::from_utf8_lossy(&line_buf);
                r.render_line(&line, out);
                line_buf.clear();
            } else {
                line_buf.push(b);
            }
        }
    }
    if !line_buf.is_empty() {
        let line = String::from_utf8_lossy(&line_buf);
        r.render_line(&line, out);
    }
    r.finish(out);
}
