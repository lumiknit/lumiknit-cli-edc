mod context;
use context::{ChatConfig, Message};
use lumiknit_cli_edc::version::version_description;

use std::fs;
use std::io::{self, BufRead, Read, Write};
use std::process::{Command, Stdio};

const DEFAULT_FILE: &str = "llm.context";

const HELP: &str = "\
Usage: llm [options] [file]

  file      context file (default: llm.context)

Options:
  -h        show this help
  -v        show version
  -D        dry-run: print the curl command instead of executing it
  -init     initialize context file
            -system <msg>  system message
                           (default: OPENAI_DEFAULT_SYSTEM_MESSAGE or
                            'You are a helpful assistant')
  -model    list available models

Providers (api_url field or OPENAI_BASE_URL):
  openai      https://api.openai.com/v1
  google      https://generativelanguage.googleapis.com/v1beta/openai
  openrouter  https://openrouter.ai/api/v1
  (or any full base URL)

Env vars:
  OPENAI_API_KEY
  OPENAI_BASE_URL
  OPENAI_DEFAULT_MODEL
  OPENAI_DEFAULT_SYSTEM_MESSAGE
";

fn load_or_default(path: &str) -> ChatConfig {
    fs::read(path)
        .map(|d| ChatConfig::parse(&d))
        .unwrap_or_else(|_| ChatConfig::new())
}

fn save(path: &str, config: &ChatConfig) {
    fs::write(path, config.serialize()).expect("failed to save file");
}

fn cmd_init(path: &str, system: Option<String>) {
    if std::path::Path::new(path).exists() {
        eprintln!("already exists: {path}");
        std::process::exit(1);
    }
    let system = system.unwrap_or_else(|| {
        std::env::var("OPENAI_DEFAULT_SYSTEM_MESSAGE")
            .unwrap_or_else(|_| "You are a helpful assistant".to_string())
    });
    let mut config = ChatConfig::new();
    config.messages.push(Message::new("system", system));
    save(path, &config);
    println!("initialized: {path}");
}

fn print_curl(args: &[&str]) {
    let parts: Vec<String> = std::iter::once("curl".to_string())
        .chain(args.iter().map(|a| shell_quote(a)))
        .collect();
    println!("{}", parts.join(" "));
}

fn shell_quote(s: &str) -> String {
    if s.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '/' | ':' | ',' | '=')) {
        s.to_string()
    } else {
        format!("'{}'", s.replace('\'', "'\\''"))
    }
}

fn cmd_models(path: &str, dry_run: bool) {
    let config = load_or_default(path);
    let base_url = config.resolve_base_url();
    let api_key = config.resolve_api_key();
    let url = format!("{base_url}/models");
    let auth = format!("Authorization: Bearer {api_key}");
    let curl_args = ["-s", &url, "-H", &auth];

    if dry_run {
        print_curl(&curl_args);
        return;
    }

    let out = Command::new("curl")
        .args(curl_args)
        .output()
        .expect("curl failed");
    let body = String::from_utf8_lossy(&out.stdout);
    let mut s = body.as_ref();
    while let Some(p) = s.find("\"id\":\"") {
        s = &s[p + 6..];
        if let Some(end) = s.find('"') {
            println!("{}", &s[..end]);
            s = &s[end + 1..];
        }
    }
}

fn json_str(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", c as u32))
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn build_json_body(config: &ChatConfig) -> String {
    let msgs = config
        .messages
        .iter()
        .map(|m| {
            format!(
                "{{\"role\":{},\"content\":{}}}",
                json_str(&m.role),
                json_str(&m.content)
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "{{\"model\":{},\"messages\":[{msgs}],\"stream\":true}}",
        json_str(&config.model)
    )
}

fn extract_delta_content(line: &str) -> Option<String> {
    let s = &line[line.find("\"content\":\"")? + 11..];
    let mut result = String::new();
    let mut chars = s.chars();
    loop {
        match chars.next()? {
            '"' => break,
            '\\' => match chars.next()? {
                '"' => result.push('"'),
                '\\' => result.push('\\'),
                '/' => result.push('/'),
                'n' => result.push('\n'),
                'r' => result.push('\r'),
                't' => result.push('\t'),
                'u' => {
                    let hex: String =
                        (0..4).filter_map(|_| chars.next()).collect();
                    if let Ok(n) = u32::from_str_radix(&hex, 16) {
                        result.push(char::from_u32(n).unwrap_or('?'));
                    }
                }
                c => result.push(c),
            },
            c => result.push(c),
        }
    }
    if result.is_empty() {
        None
    } else {
        Some(result)
    }
}

fn cmd_chat(path: &str, dry_run: bool) {
    let mut config = load_or_default(path);

    use std::io::IsTerminal;
    let is_tty = io::stdin().is_terminal();
    if is_tty {
        eprintln!("enter message, then EOF (^D) to send:");
    }
    let mut input = String::new();
    io::stdin()
        .read_to_string(&mut input)
        .expect("failed to read stdin");
    let input = input.trim();
    if input.is_empty() {
        eprintln!("empty message");
        std::process::exit(1);
    }

    config.messages.push(Message::new("user", input));

    let base_url = config.resolve_base_url();
    let api_key = config.resolve_api_key();
    let body = build_json_body(&config);
    let url = format!("{base_url}/chat/completions");
    let auth = format!("Authorization: Bearer {api_key}");
    let curl_args = [
        "-sN", "-X", "POST", &url,
        "-H", "Content-Type: application/json",
        "-H", &auth,
        "-d", &body,
    ];

    if dry_run {
        print_curl(&curl_args);
        return;
    }

    if is_tty {
        eprintln!("\x1b[1;31m[Assistant]\x1b[0m");
    }

    let mut child = Command::new("curl")
        .args(curl_args)
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("curl failed");

    let reader = io::BufReader::new(child.stdout.take().unwrap());
    let mut reply = String::new();
    let out = io::stdout();
    let mut out_lock = out.lock();

    for line in reader.lines().map_while(Result::ok) {
        let Some(data) = line.strip_prefix("data: ") else {
            continue;
        };
        if data == "[DONE]" {
            break;
        }
        if let Some(delta) = extract_delta_content(data) {
            out_lock.write_all(delta.as_bytes()).ok();
            out_lock.flush().ok();
            reply.push_str(&delta);
        }
    }
    drop(out_lock);
    println!();
    child.wait().ok();

    config.messages.push(Message::new("assistant", reply));
    save(path, &config);
}

#[derive(Default)]
struct Args {
    help: bool,
    version: bool,
    dry_run: bool,
    init: bool,
    model: bool,
    system: Option<String>,
    file: Option<String>,
}

fn parse_args() -> Args {
    let mut a = Args::default();
    let mut iter = std::env::args().skip(1);
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "-h" | "--help" => a.help = true,
            "-v" => a.version = true,
            "-D" => a.dry_run = true,
            "-init" => a.init = true,
            "-model" => a.model = true,
            "-system" => a.system = iter.next(),
            _ => a.file = Some(arg),
        }
    }
    a
}

fn main() {
    let a = parse_args();
    let ver = version_description("llm", "minimal LLM chat CLI");
    let file = a.file.as_deref().unwrap_or(DEFAULT_FILE);

    if a.version {
        println!("{ver}");
    } else if a.help {
        println!("{ver}\n");
        print!("{HELP}");
    } else if a.init {
        cmd_init(file, a.system);
    } else if a.model {
        cmd_models(file, a.dry_run);
    } else {
        cmd_chat(file, a.dry_run);
    }
}
