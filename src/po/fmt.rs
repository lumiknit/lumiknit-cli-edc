/// The result of running a piece of code, as returned by the daemon.
pub struct RunOutcome<'a> {
    pub stdout: &'a str,
    pub stderr: &'a str,
    pub exit_code: i32,
    pub elapsed_ms: u64,
}

/// `md` format: original stdin (if requested) followed by the run output
/// wrapped in a fenced code block, stdout and stderr interleaved in capture
/// order isn't tracked separately -- stdout first, then stderr.
pub fn format_md(
    stdin: Option<&str>,
    outcome: &RunOutcome,
    print_exit: bool,
) -> String {
    let mut out = String::new();
    if let Some(s) = stdin {
        out.push_str(s);
        if !s.ends_with('\n') {
            out.push('\n');
        }
        out.push_str("---\n");
    }
    out.push_str("```\n");
    if !outcome.stdout.is_empty() {
        out.push_str(outcome.stdout);
        if !outcome.stdout.ends_with('\n') {
            out.push('\n');
        }
    }
    if !outcome.stderr.is_empty() {
        out.push_str(outcome.stderr);
        if !outcome.stderr.ends_with('\n') {
            out.push('\n');
        }
    }
    out.push_str("```\n");
    if print_exit {
        out.push_str(&exit_line(outcome));
    }
    out
}

/// `hash` format: each stdout line prefixed with `#=> `, each stderr line
/// prefixed with `#!> `.
pub fn format_hash(
    stdin: Option<&str>,
    outcome: &RunOutcome,
    print_exit: bool,
) -> String {
    let mut out = String::new();
    if let Some(s) = stdin {
        out.push_str(s);
        if !s.ends_with('\n') {
            out.push('\n');
        }
        out.push_str("---\n");
    }
    for line in outcome.stdout.lines() {
        out.push_str("#=> ");
        out.push_str(line);
        out.push('\n');
    }
    for line in outcome.stderr.lines() {
        out.push_str("#!> ");
        out.push_str(line);
        out.push('\n');
    }
    if print_exit {
        out.push_str(&exit_line(outcome));
    }
    out
}

fn exit_line(outcome: &RunOutcome) -> String {
    format!(
        "exit code: {}  elapsed: {}ms\n",
        outcome.exit_code, outcome.elapsed_ms
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn outcome<'a>(stdout: &'a str, stderr: &'a str) -> RunOutcome<'a> {
        RunOutcome {
            stdout,
            stderr,
            exit_code: 0,
            elapsed_ms: 12,
        }
    }

    #[test]
    fn md_basic() {
        let o = outcome("hi\n", "");
        let out = format_md(None, &o, false);
        assert_eq!(out, "```\nhi\n```\n");
    }

    #[test]
    fn md_with_stdin_and_exit() {
        let o = outcome("hi\n", "warn\n");
        let out = format_md(Some("echo hi"), &o, true);
        assert_eq!(
            out,
            "echo hi\n---\n```\nhi\nwarn\n```\nexit code: 0  elapsed: 12ms\n"
        );
    }

    #[test]
    fn hash_basic() {
        let o = outcome("a\nb\n", "e\n");
        let out = format_hash(None, &o, false);
        assert_eq!(out, "#=> a\n#=> b\n#!> e\n");
    }

    #[test]
    fn hash_with_exit() {
        let mut o = outcome("a\n", "");
        o.exit_code = 1;
        let out = format_hash(None, &o, true);
        assert_eq!(out, "#=> a\nexit code: 1  elapsed: 12ms\n");
    }
}
