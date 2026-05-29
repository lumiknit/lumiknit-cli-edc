use lumiknit_cli_edc::version::version_description;
use std::io::{self, Read, Write};
use std::process::Command;
use tempfile::Builder;

const HELP: &str = "\
Usage: ww [ext]

  ext     file extension for the temp file (e.g. md, py)

Reads stdin into a temp file, opens $EDITOR, then writes
the edited result to stdout.

Options:
  -h      show this help
  -v      show version
";

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let ver = version_description("ww", "Wait, what? - stdin to editor to stdout");

    if args.iter().any(|a| a == "-h" || a == "--help") {
        println!("{ver}\n");
        print!("{HELP}");
        return;
    }
    if args.iter().any(|a| a == "-v") {
        println!("{ver}");
        return;
    }

    let ext = args.get(1).filter(|a| !a.starts_with('-'));

    let suffix = ext.map(|e| format!(".{e}")).unwrap_or_default();
    let mut file = Builder::new()
        .prefix("ww-")
        .suffix(&suffix)
        .tempfile()
        .expect("failed to create temp file");

    use std::io::IsTerminal;
    if !io::stdin().is_terminal() {
        let mut buf = [0u8; 8192];
        let stdin = io::stdin();
        let mut stdin = stdin.lock();
        loop {
            let n = stdin.read(&mut buf).expect("failed to read stdin");
            if n == 0 { break; }
            file.write_all(&buf[..n]).expect("failed to write temp file");
        }
        file.flush().expect("failed to flush temp file");
    }

    let editor = std::env::var("VISUAL")
        .or_else(|_| std::env::var("EDITOR"))
        .unwrap_or_else(|_| "vi".to_string());
    let status = Command::new(&editor)
        .arg(file.path())
        .status()
        .unwrap_or_else(|_| panic!("failed to launch editor: {editor}"));

    if !status.success() {
        eprintln!("editor exited with: {status}");
        std::process::exit(1);
    }

    // temp file → stdout
    let mut file = file.reopen().expect("failed to reopen temp file");
    let mut buf = [0u8; 8192];
    let stdout = io::stdout();
    let mut stdout = stdout.lock();
    loop {
        let n = file.read(&mut buf).expect("failed to read temp file");
        if n == 0 { break; }
        stdout.write_all(&buf[..n]).expect("failed to write stdout");
    }
}
