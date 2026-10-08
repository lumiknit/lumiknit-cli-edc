use lumiknit_cli_edc::jflat::*;
use lumiknit_cli_edc::version::version_description;
use std::io::{self, Read, Write};

const HELP: &str = include_str!("help.txt");

struct Args {
    force_fmt: Option<Fmt>,
    sort: bool,
    sep: String,
    unflatten: Option<Fmt>,
    sv_sep: Option<char>,
    sv_quote: Option<bool>,
    sv_header: Option<bool>,
}

impl Args {
    fn sv_opts(&self, fmt: Fmt) -> Option<SvOpts> {
        let mut opts = fmt.default_sv_opts()?;
        if let Some(sep) = self.sv_sep {
            opts.sep = sep;
        }
        if let Some(quote) = self.sv_quote {
            opts.quote = quote;
        }
        if let Some(header) = self.sv_header {
            opts.header = header;
        }
        Some(opts)
    }
}

fn parse_args() -> Option<Args> {
    let mut force_fmt = None;
    let mut sort = true;
    let mut sep = "\t".to_string();
    let mut unflatten = None;
    let mut sv_sep = None;
    let mut sv_quote = None;
    let mut sv_header = None;
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
            "-svsep" => {
                let s = iter.next().unwrap_or_else(|| {
                    eprintln!("-svsep requires a single-character value");
                    std::process::exit(1);
                });
                let mut chars = s.chars();
                let c = chars.next().unwrap_or_else(|| {
                    eprintln!("-svsep requires a single-character value");
                    std::process::exit(1);
                });
                if chars.next().is_some() {
                    eprintln!("-svsep requires a single-character value");
                    std::process::exit(1);
                }
                sv_sep = Some(c);
            }
            "-svquote" => sv_quote = Some(true),
            "-svnoquote" => sv_quote = Some(false),
            "-noheader" => sv_header = Some(false),
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
        sv_sep,
        sv_quote,
        sv_header,
    })
}

fn main() {
    let Some(args) = parse_args() else { return };
    let mut input = String::new();
    io::stdin().read_to_string(&mut input).expect("read failed");

    if let Some(out_fmt) = args.unflatten {
        let result = match out_fmt {
            Fmt::Env => do_unflatten_env(&input, &args.sep),
            fmt => do_unflatten(&input, fmt, &args.sep, args.sv_opts(fmt)),
        };
        match result {
            Ok(s) => print!("{s}"),
            Err(e) => {
                eprintln!("error: {e}");
                std::process::exit(1);
            }
        }
    } else {
        let sv_opts = args.force_fmt.and_then(|f| args.sv_opts(f));
        match do_flatten(&input, args.force_fmt, args.sort, sv_opts) {
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
