use lumiknit_cli_edc::md::*;
use lumiknit_cli_edc::version::version_description;
use std::io::{self, Read};

const HELP: &str = include_str!("help.txt");

fn main() {
    let ver = version_description("md", "Markdown ANSI renderer");

    let mut dry = false;
    let mut prefix = ".md-".to_string();
    let mut code_printer: Option<CodePrinter> = None;
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
            _ => {}
        }
    }

    let printer = code_printer.unwrap_or_else(CodePrinter::auto_detect);

    let mut stdin = io::stdin().lock();
    let mut out = io::stdout();
    let mut r = Renderer::new(dry, prefix, printer);

    let mut raw = [0u8; 4096];
    let mut line_buf: Vec<u8> = Vec::new();

    loop {
        let n = stdin.read(&mut raw).expect("read failed");
        if n == 0 {
            break;
        }
        for &b in &raw[..n] {
            if b == b'\n' {
                let line = String::from_utf8_lossy(&line_buf);
                r.render_line(&line, &mut out);
                line_buf.clear();
            } else {
                line_buf.push(b);
            }
        }
    }
    if !line_buf.is_empty() {
        let line = String::from_utf8_lossy(&line_buf);
        r.render_line(&line, &mut out);
    }
    r.finish(&mut out);
}
