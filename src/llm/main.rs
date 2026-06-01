mod context;
use context::{ChatConfig, Message};
use lumiknit_cli_edc::jsonlite;
use lumiknit_cli_edc::version::version_description;

use std::fs;
use std::io::{self, BufRead, Read, Write};
use std::process::{Command, Stdio};

const DEFAULT_PREFIX: &str = ".llm";
const HELP: &str = include_str!("help.txt");

fn context_path(prefix: &str) -> String {
    format!("{prefix}.context")
}

fn load_or_default(prefix: &str) -> ChatConfig {
    let path = context_path(prefix);
    fs::read(&path)
        .map(|d| ChatConfig::parse(&d))
        .unwrap_or_else(|_| ChatConfig::new())
}

fn save(prefix: &str, config: &ChatConfig) {
    let path = context_path(prefix);
    fs::write(&path, config.serialize()).expect("failed to save context file");
}

fn cmd_init(prefix: &str, system: Option<String>) {
    let path = context_path(prefix);
    if std::path::Path::new(&path).exists() {
        eprintln!("already exists: {path}");
        std::process::exit(1);
    }
    let mut config = ChatConfig::new();
    if let Some(system) = system {
        config.messages[0] = Message::new("system", system);
    }
    save(prefix, &config);
    println!("initialized: {path}");
}

fn print_curl(args: &[&str]) {
    let parts: Vec<String> = std::iter::once("curl".to_string())
        .chain(args.iter().map(|a| shell_quote(a)))
        .collect();
    println!("{}", parts.join(" "));
}

fn shell_quote(s: &str) -> String {
    if s.chars().all(|c| {
        c.is_ascii_alphanumeric()
            || matches!(c, '-' | '_' | '.' | '/' | ':' | ',' | '=')
    }) {
        s.to_string()
    } else {
        format!("'{}'", s.replace('\'', "'\\''"))
    }
}

fn cmd_models(prefix: &str, dry_run: bool) {
    let config = load_or_default(prefix);
    let base_url = config.resolve_base_url();
    let api_key = config.resolve_api_key();
    let url = format!("{base_url}/models");
    let auth = format!("Authorization: Bearer {api_key}");
    let curl_args = ["-s", "--fail-with-body", &url, "-H", &auth];

    if dry_run {
        print_curl(&curl_args);
        return;
    }

    let out = Command::new("curl")
        .args(curl_args)
        .output()
        .expect("curl failed");
    if !out.status.success() {
        let body = String::from_utf8_lossy(&out.stdout);
        eprintln!("request failed ({}): {}", out.status, body.trim());
        std::process::exit(1);
    }
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

fn build_json_body(config: &ChatConfig) -> String {
    let msgs = config
        .messages
        .iter()
        .map(|m| {
            format!(
                "{{\"role\":{},\"content\":{}}}",
                jsonlite::quote(&m.role),
                jsonlite::quote_readable(&m.content)
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "{{\"model\":{},\"messages\":[{msgs}],\"stream\":true}}",
        jsonlite::quote(&config.model)
    )
}

fn extract_delta_content(line: &str) -> Option<String> {
    // find the opening quote of the content value, then take the JSON string
    // (including its closing quote) and delegate decoding to jsonlite::unquote
    let start = line.find("\"content\":\"")? + 10; // points at the opening "
    let rest = &line[start..];
    // find the closing unescaped quote to bound the JSON string literal
    let mut chars = rest.char_indices().peekable();
    chars.next(); // skip opening "
    let mut end = rest.len();
    while let Some((i, c)) = chars.next() {
        if c == '\\' {
            chars.next();
        } else if c == '"' {
            end = i + 1;
            break;
        }
    }
    let decoded = jsonlite::unquote(&rest[..end]);
    if decoded.is_empty() {
        None
    } else {
        Some(decoded)
    }
}

fn cmd_chat(prefix: &str, dry_run: bool, extra: Option<&str>) {
    let mut config = load_or_default(prefix);

    use std::io::IsTerminal;
    let is_tty = io::stdin().is_terminal();
    if is_tty {
        eprintln!("enter message, then EOF (^D) to send:");
    }
    let mut input = String::new();
    io::stdin()
        .read_to_string(&mut input)
        .expect("failed to read stdin");
    let input = input.trim().to_string();
    let input = match extra {
        Some(e) => format!("{input}\n\n{e}"),
        None => input,
    };
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
        "-sNi",
        "-X",
        "POST",
        &url,
        "-H",
        "Content-Type: application/json",
        "-H",
        &auth,
        "-d",
        &body,
    ];

    if dry_run {
        print_curl(&curl_args);
        return;
    }

    let mut child = Command::new("curl")
        .args(curl_args)
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("curl failed");

    let mut reader = io::BufReader::new(child.stdout.take().unwrap());

    // Read headers and check HTTP status code
    let mut status_code = 0u16;
    let mut header_buf = String::new();
    loop {
        header_buf.clear();
        if reader.read_line(&mut header_buf).unwrap_or(0) == 0 {
            break;
        }
        let trimmed = header_buf.trim_end();
        if trimmed.is_empty() {
            break;
        }
        if trimmed.starts_with("HTTP/") {
            if let Some(code_str) = trimmed.split_whitespace().nth(1) {
                status_code = code_str.parse().unwrap_or(0);
            }
        }
    }

    if !(200..300).contains(&status_code) {
        let mut err_body = String::new();
        reader.read_to_string(&mut err_body).ok();
        eprintln!("request failed (HTTP {status_code}): {}", err_body.trim());
        child.wait().ok();
        std::process::exit(1);
    }

    if is_tty {
        eprintln!("\x1b[1;31m[Assistant]\x1b[0m");
    }

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

    config.messages.push(Message::new("assistant", &reply));
    save(prefix, &config);
}

#[derive(Default)]
struct Args {
    help: bool,
    version: bool,
    dry_run: bool,
    init: bool,
    model: bool,
    system: Option<String>,
    extra: Option<String>,
    prefix: Option<String>,
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
            "-e" => a.extra = iter.next(),
            _ => a.prefix = Some(arg),
        }
    }
    a
}

fn main() {
    let a = parse_args();
    let ver = version_description("llm", "minimal LLM chat CLI");
    let prefix = a.prefix.as_deref().unwrap_or(DEFAULT_PREFIX);

    if a.version {
        println!("{ver}");
    } else if a.help {
        println!("{ver}\n");
        print!("{HELP}");
    } else if a.init {
        cmd_init(prefix, a.system);
    } else if a.model {
        cmd_models(prefix, a.dry_run);
    } else {
        cmd_chat(prefix, a.dry_run, a.extra.as_deref());
    }
}
