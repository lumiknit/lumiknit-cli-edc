use lumiknit_cli_edc::jex::*;
use lumiknit_cli_edc::version::version_description;
use std::io::{self, BufRead, Read, Write};

const HELP: &str = include_str!("help.txt");

fn load_template(src: &str) -> serde_json::Value {
    serde_json::from_str(src).unwrap_or_else(|e| {
        eprintln!("invalid JSON: {e}\n  {src}");
        std::process::exit(1);
    })
}

fn load_template_file(path: &str) -> serde_json::Value {
    let src = std::fs::read_to_string(path).unwrap_or_else(|e| {
        eprintln!("failed to read {path}: {e}");
        std::process::exit(1);
    });
    serde_json::from_str(&src).unwrap_or_else(|e| {
        eprintln!("invalid JSON in {path}: {e}");
        std::process::exit(1);
    })
}

struct Args {
    global: bool,
    rules: Vec<Rule>,
}

fn parse_args() -> Option<Args> {
    let mut global = false;
    let mut rules: Vec<Rule> = vec![];
    let mut pending_mode = Mode::AllMatch;
    let mut iter = std::env::args().skip(1).peekable();

    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "-h" | "--help" => {
                let ver = version_description(
                    "jex",
                    "fill a JSON template from stdin via regex",
                );
                println!("{ver}\n");
                print!("{HELP}");
                return None;
            }
            "-v" => {
                println!(
                    "{}",
                    version_description(
                        "jex",
                        "fill a JSON template from stdin via regex"
                    )
                );
                return None;
            }
            "-g" => global = true,
            "-a" => pending_mode = Mode::Always,
            "-1" => pending_mode = Mode::AtLeastOne,
            "-e" => {
                let src = iter.next().unwrap_or_else(|| {
                    eprintln!("-e requires a JSON string argument");
                    std::process::exit(1);
                });
                let mode = pending_mode;
                pending_mode = Mode::AllMatch;
                let compiled = compile(&load_template(&src)).unwrap_or_else(|e| {
                    eprintln!("error in template: {e}");
                    std::process::exit(1);
                });
                rules.push(Rule { mode, compiled });
            }
            path => {
                let mode = pending_mode;
                pending_mode = Mode::AllMatch;
                let compiled = compile(&load_template_file(path)).unwrap_or_else(|e| {
                    eprintln!("error in {path}: {e}");
                    std::process::exit(1);
                });
                rules.push(Rule { mode, compiled });
            }
        }
    }

    if rules.is_empty() {
        eprintln!("error: at least one template file or -e <json> required");
        eprintln!(
            "usage: jex [options] ( [-a|-1] <file.json> | [-a|-1] -e '<json>' )+"
        );
        std::process::exit(1);
    }

    Some(Args { global, rules })
}

fn main() {
    let Some(args) = parse_args() else { return };
    let stdout = io::stdout();
    let mut out = stdout.lock();

    if args.global {
        let mut input = String::new();
        io::stdin()
            .read_to_string(&mut input)
            .expect("failed to read stdin");
        process(&input, &args.rules, &mut out);
    } else {
        for line in io::stdin().lock().lines().map_while(Result::ok) {
            process(&line, &args.rules, &mut out);
        }
    }

    out.flush().ok();
}
