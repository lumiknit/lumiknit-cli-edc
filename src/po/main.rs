use lumiknit_cli_edc::po::{daemon, detect, fmt, ipc, option};
use lumiknit_cli_edc::version::version_description;

use std::io::{self, Read};

const HELP: &str = include_str!("help.txt");

fn main() {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let args = match option::parse(&argv, |k| std::env::var(k).ok()) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
    };

    let ver = version_description("po", "pipe-friendly code runner");

    match args.mode {
        option::Mode::Help => {
            println!("{ver}\n");
            print!("{HELP}");
        }
        option::Mode::Version => println!("{ver}"),
        option::Mode::Start => {
            if let Err(e) = daemon::run(&args.context) {
                eprintln!("error: daemon failed: {e}");
                std::process::exit(1);
            }
        }
        option::Mode::Kill => match daemon::kill(&args.context) {
            Ok(true) => println!("killed context {:?}", args.context),
            Ok(false) => {
                println!("no daemon running for context {:?}", args.context)
            }
            Err(e) => {
                eprintln!("error: {e}");
                std::process::exit(1);
            }
        },
        option::Mode::Run => run_chat(&args),
    }
}

fn run_chat(args: &option::Args) {
    let mut input = String::new();
    io::stdin()
        .read_to_string(&mut input)
        .expect("failed to read stdin");

    if input.trim().is_empty() {
        // Nothing piped in: do nothing (per spec, plain `po` is a no-op).
        return;
    }

    let info = match detect::detect_code(&input) {
        Ok(i) => i,
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
    };
    let runtime = if info.is_shebang {
        None
    } else {
        let lang = info.lang.as_deref().unwrap_or("sh");
        match detect::resolve_runtime(lang) {
            Some(r) => Some(r.to_string()),
            None => {
                eprintln!("error: unsupported runtime: {lang}");
                std::process::exit(1);
            }
        }
    };

    let req = ipc::Request::Run {
        code: info.body,
        runtime,
    };
    let response = match daemon::send_request(&args.context, &req) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: failed to reach context daemon: {e}");
            std::process::exit(1);
        }
    };

    let stdin_shown = args.print_stdin.then_some(input.as_str());
    match response {
        ipc::Response::Ran {
            stdout,
            stderr,
            exit_code,
            elapsed_ms,
            ..
        } => {
            let outcome = fmt::RunOutcome {
                stdout: &stdout,
                stderr: &stderr,
                exit_code,
                elapsed_ms,
            };
            let out = match args.out_fmt {
                option::OutFmt::Md => {
                    fmt::format_md(stdin_shown, &outcome, args.print_exit)
                }
                option::OutFmt::Hash => {
                    fmt::format_hash(stdin_shown, &outcome, args.print_exit)
                }
            };
            print!("{out}");
        }
        ipc::Response::Error { message } => {
            eprintln!("error: {message}");
            std::process::exit(1);
        }
        ipc::Response::Killed | ipc::Response::Pong => {
            eprintln!("error: unexpected response from daemon");
            std::process::exit(1);
        }
    }
}
