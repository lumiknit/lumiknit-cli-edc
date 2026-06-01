use lumiknit_cli_edc::version::version_description;
use std::io::{self, Read, Write};
use std::process::Command;

const HELP: &str = "\
Usage: ww [ext]

  ext     file extension for the temp file (e.g. md, py)

Reads stdin into a temp file, opens $EDITOR, then writes
the edited result to stdout.

Options:
  -h      show this help
  -v      show version
";

struct Flags {
    help: bool,
    version: bool,
    ext: Option<String>,
}

fn parse_args() -> Flags {
    let mut flags = Flags { help: false, version: false, ext: None };
    for arg in std::env::args().skip(1) {
        if arg == "-h" || arg == "--help" {
            flags.help = true;
        } else if arg == "-v" {
            flags.version = true;
        } else if !arg.starts_with('-') && flags.ext.is_none() {
            flags.ext = Some(arg);
        }
    }
    flags
}

fn main() {
    let ver = version_description("ww", "Wait, what? - stdin to editor to stdout");
    let flags = parse_args();

    if flags.help {
        println!("{ver}\n");
        print!("{HELP}");
        return;
    }
    if flags.version {
        println!("{ver}");
        return;
    }

    let suffix = flags.ext.as_deref().map(|e| format!(".{e}")).unwrap_or_default();

    // persistent temp file: editor가 atomic save(rename)를 해도 path로 재오픈 가능
    let path = {
        let tmp = tempfile::Builder::new()
            .prefix("ww-")
            .suffix(&suffix)
            .tempfile()
            .expect("failed to create temp file");
        let (_, path) = tmp.keep().expect("failed to keep temp file");
        path
    };

    // stdin → temp file
    {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .open(&path)
            .expect("failed to open temp file for writing");

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
    }

    let editor = std::env::var("VISUAL")
        .or_else(|_| std::env::var("EDITOR"))
        .unwrap_or_else(|_| "vi".to_string());
    let status = Command::new(&editor)
        .arg(&path)
        .status()
        .unwrap_or_else(|_| panic!("failed to launch editor: {editor}"));

    if !status.success() {
        eprintln!("editor exited with: {status}");
        let _ = std::fs::remove_file(&path);
        std::process::exit(1);
    }

    // temp file → stdout (path로 다시 열어서 읽음)
    {
        let mut file = std::fs::File::open(&path).expect("failed to open temp file for reading");
        let mut buf = [0u8; 8192];
        let stdout = io::stdout();
        let mut stdout = stdout.lock();
        loop {
            let n = file.read(&mut buf).expect("failed to read temp file");
            if n == 0 { break; }
            stdout.write_all(&buf[..n]).expect("failed to write stdout");
        }
    }

    std::fs::remove_file(&path).expect("failed to remove temp file");
}
