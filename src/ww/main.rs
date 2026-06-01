use lumiknit_cli_edc::{editor, version::version_description};
use std::io::{self, Read, Write};

const HELP: &str = include_str!("help.txt");

struct Flags {
    help: bool,
    version: bool,
    ext: Option<String>,
}

fn parse_args() -> Flags {
    let mut flags = Flags {
        help: false,
        version: false,
        ext: None,
    };
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
    let ver = version_description("ww", "stdin to editor to stdout");
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

    let suffix = flags
        .ext
        .as_deref()
        .map(|e| format!(".{e}"))
        .unwrap_or_default();

    let path = {
        let tmp = tempfile::Builder::new()
            .prefix("ww-")
            .suffix(&suffix)
            .tempfile()
            .expect("failed to create temp file");
        let (_, path) = tmp.keep().expect("failed to keep temp file");
        path
    };

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
                if n == 0 {
                    break;
                }
                file.write_all(&buf[..n])
                    .expect("failed to write temp file");
            }
            file.flush().expect("failed to flush temp file");
        }
    }

    let status = editor::open(&path).expect("failed to launch editor");

    if !status.success() {
        eprintln!("editor exited with: {status}");
        let _ = std::fs::remove_file(&path);
        std::process::exit(1);
    }

    {
        let mut file = std::fs::File::open(&path)
            .expect("failed to open temp file for reading");
        let mut buf = [0u8; 8192];
        let stdout = io::stdout();
        let mut stdout = stdout.lock();
        loop {
            let n = file.read(&mut buf).expect("failed to read temp file");
            if n == 0 {
                break;
            }
            stdout.write_all(&buf[..n]).expect("failed to write stdout");
        }
    }

    std::fs::remove_file(&path).expect("failed to remove temp file");
}
