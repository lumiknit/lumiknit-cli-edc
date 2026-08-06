/// Output format for a run result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutFmt {
    Md,
    Hash,
}

impl OutFmt {
    fn parse(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "md" => Some(Self::Md),
            "hash" => Some(Self::Hash),
            _ => None,
        }
    }
}

/// What this invocation of `po` should do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Run,
    Kill,
    Start,
    Help,
    Version,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Args {
    pub context: String,
    pub out_fmt: OutFmt,
    pub print_stdin: bool,
    pub print_exit: bool,
    pub mode: Mode,
}

fn env_flag(env_lookup: &impl Fn(&str) -> Option<String>, key: &str) -> bool {
    matches!(
        env_lookup(key).as_deref().map(str::to_lowercase).as_deref(),
        Some("1") | Some("true") | Some("yes")
    )
}

/// Parse CLI args (excluding argv[0]) plus environment defaults into `Args`.
/// `env_lookup` is injected so tests can mock environment variables.
pub fn parse(
    argv: &[String],
    env_lookup: impl Fn(&str) -> Option<String>,
) -> Result<Args, String> {
    let mut context = env_lookup("EDC_PO_CONTEXT").unwrap_or_default();
    let mut out_fmt = env_lookup("EDC_PO_OUT_FMT")
        .as_deref()
        .and_then(OutFmt::parse)
        .unwrap_or(OutFmt::Md);
    let mut print_stdin = env_flag(&env_lookup, "EDC_PO_PRINT_STDIN");
    let print_exit = env_flag(&env_lookup, "EDC_PO_PRINT_EXIT");
    let mut mode = Mode::Run;

    let mut iter = argv.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "-h" | "--help" => mode = Mode::Help,
            "-v" => mode = Mode::Version,
            "-kill" => mode = Mode::Kill,
            "-start" => mode = Mode::Start,
            "-i" => print_stdin = true,
            "-ni" => print_stdin = false,
            "-x" => {
                context = iter
                    .next()
                    .cloned()
                    .ok_or_else(|| "-x requires a context name".to_string())?;
            }
            "-o" => {
                let val = iter
                    .next()
                    .ok_or_else(|| "-o requires a format".to_string())?;
                out_fmt = OutFmt::parse(val)
                    .ok_or_else(|| format!("unknown output format: {val}"))?;
            }
            other => return Err(format!("unknown argument: {other}")),
        }
    }

    Ok(Args {
        context: context.to_lowercase(),
        out_fmt,
        print_stdin,
        print_exit,
        mode,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn no_env(_: &str) -> Option<String> {
        None
    }

    fn env_map(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let map: HashMap<String, String> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        move |k: &str| map.get(k).cloned()
    }

    fn args(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn defaults() {
        let a = parse(&args(&[]), no_env).unwrap();
        assert_eq!(a.context, "");
        assert_eq!(a.out_fmt, OutFmt::Md);
        assert!(!a.print_stdin);
        assert!(!a.print_exit);
        assert_eq!(a.mode, Mode::Run);
    }

    #[test]
    fn context_flag_lowercased() {
        let a = parse(&args(&["-x", "MyCtx"]), no_env).unwrap();
        assert_eq!(a.context, "myctx");
    }

    #[test]
    fn context_env() {
        let a =
            parse(&args(&[]), env_map(&[("EDC_PO_CONTEXT", "Proj")])).unwrap();
        assert_eq!(a.context, "proj");
    }

    #[test]
    fn out_fmt_flag() {
        let a = parse(&args(&["-o", "hash"]), no_env).unwrap();
        assert_eq!(a.out_fmt, OutFmt::Hash);
    }

    #[test]
    fn out_fmt_unknown_errors() {
        assert!(parse(&args(&["-o", "yaml"]), no_env).is_err());
    }

    #[test]
    fn print_stdin_i_overrides_env() {
        let a =
            parse(&args(&["-i"]), env_map(&[("EDC_PO_PRINT_STDIN", "false")]))
                .unwrap();
        assert!(a.print_stdin);
    }

    #[test]
    fn print_stdin_ni_overrides_env() {
        let a =
            parse(&args(&["-ni"]), env_map(&[("EDC_PO_PRINT_STDIN", "true")]))
                .unwrap();
        assert!(!a.print_stdin);
    }

    #[test]
    fn print_stdin_env_default() {
        let a = parse(&args(&[]), env_map(&[("EDC_PO_PRINT_STDIN", "true")]))
            .unwrap();
        assert!(a.print_stdin);
    }

    #[test]
    fn modes() {
        assert_eq!(parse(&args(&["-kill"]), no_env).unwrap().mode, Mode::Kill);
        assert_eq!(
            parse(&args(&["-start"]), no_env).unwrap().mode,
            Mode::Start
        );
        assert_eq!(parse(&args(&["-h"]), no_env).unwrap().mode, Mode::Help);
        assert_eq!(parse(&args(&["-v"]), no_env).unwrap().mode, Mode::Version);
    }
}
