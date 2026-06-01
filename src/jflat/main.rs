use lumiknit_cli_edc::jflat::*;
use lumiknit_cli_edc::version::version_description;
use std::io::{self, Read, Write};

const HELP: &str = include_str!("help.txt");

struct Args {
    force_fmt: Option<Fmt>,
    sort: bool,
    sep: String,
    unflatten: Option<Fmt>,
}

fn parse_args() -> Option<Args> {
    let mut force_fmt = None;
    let mut sort = true;
    let mut sep = "\t".to_string();
    let mut unflatten = None;
    let mut iter = std::env::args().skip(1).peekable();

    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "-h" | "--help" => {
                let ver = version_description(
                    "jflat",
                    "multi-format structure flattener / unflattener",
                );
                println!("{ver}\n");
                print!("{HELP}");
                return None;
            }
            "-v" => {
                println!(
                    "{}",
                    version_description(
                        "jflat",
                        "multi-format structure flattener / unflattener"
                    )
                );
                return None;
            }
            "-nosort" => sort = false,
            "-sep" => {
                sep = iter.next().unwrap_or_else(|| {
                    eprintln!("-sep requires a value");
                    std::process::exit(1);
                });
            }
            "-f" => {
                let s = iter.next().unwrap_or_else(|| {
                    eprintln!("-f requires a format");
                    std::process::exit(1);
                });
                force_fmt = Some(Fmt::parse(&s).unwrap_or_else(|| {
                    eprintln!("unknown format: {s}");
                    std::process::exit(1);
                }));
            }
            "-u" => {
                let s = iter.next().unwrap_or_else(|| {
                    eprintln!("-u requires a format");
                    std::process::exit(1);
                });
                unflatten = Some(Fmt::parse(&s).unwrap_or_else(|| {
                    eprintln!("unknown format: {s}");
                    std::process::exit(1);
                }));
            }
            other => {
                eprintln!("unknown argument: {other}");
                std::process::exit(1);
            }
        }
    }
    Some(Args {
        force_fmt,
        sort,
        sep,
        unflatten,
    })
}

fn main() {
    let Some(args) = parse_args() else { return };
    let mut input = String::new();
    io::stdin().read_to_string(&mut input).expect("read failed");

    if let Some(out_fmt) = args.unflatten {
        let result = match out_fmt {
            Fmt::Env => do_unflatten_env(&input, &args.sep),
            fmt => do_unflatten(&input, fmt, &args.sep),
        };
        match result {
            Ok(s) => print!("{s}"),
            Err(e) => {
                eprintln!("error: {e}");
                std::process::exit(1);
            }
        }
    } else {
        match do_flatten(&input, args.force_fmt, args.sort) {
            Ok(entries) => {
                let stdout = io::stdout();
                let mut out = stdout.lock();
                for e in &entries {
                    writeln!(
                        out,
                        "{}{}{}{}{}",
                        fmt_path(&e.path, &args.sep),
                        args.sep,
                        e.typ,
                        args.sep,
                        e.val
                    )
                    .ok();
                }
            }
            Err(e) => {
                eprintln!("error: {e}");
                std::process::exit(1);
            }
        }
    }
}
