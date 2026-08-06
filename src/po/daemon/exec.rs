use std::collections::HashMap;
use std::io;
use std::process::Command;
use std::time::Instant;

use tempfile::TempDir;

use crate::jsonlite;

/// The outcome of running a single script.
pub struct RunResult {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
    pub elapsed_ms: u64,
    pub setenv: HashMap<String, String>,
}

/// Parse the contents of a SETENV file: one `key="value"` assignment per
/// line. The value is decoded with [`jsonlite::unquote`] (handles the
/// surrounding quotes and backslash escapes), blank lines and lines with
/// an invalid key are ignored.
pub fn parse_setenv(contents: &str) -> HashMap<String, String> {
    let mut result = HashMap::new();
    for line in contents.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Some(eq) = line.find('=') else {
            continue;
        };
        let key = line[..eq].trim();
        let is_valid_key = !key.is_empty()
            && key
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
            && key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
        if !is_valid_key {
            continue;
        }
        let value = jsonlite::unquote(line[eq + 1..].trim());
        result.insert(key.to_string(), value);
    }
    result
}

/// Run `code`, merging `base_env` into the child's environment and
/// injecting `SETENV` so the script can hand new context values back to
/// the daemon. Runs inside a fresh temp directory used as the child's cwd.
///
/// `runtime` picks the execution mode:
/// - `Some(cmd)`: invoke `cmd` with the script file as its argument (used
///   for fenced code blocks, e.g. `sh`/`bash`/`python3`).
/// - `None`: the code came with its own shebang line, so it's written to
///   an executable file and run directly, letting the OS resolve the
///   shebang's interpreter (any language, not just the ones this build
///   knows the name of).
pub fn run_shell(
    code: &str,
    runtime: Option<&str>,
    base_env: &HashMap<String, String>,
) -> io::Result<RunResult> {
    let workdir = TempDir::with_prefix("po-run-")?;
    let script_path = workdir.path().join("script");
    let setenv_path = workdir.path().join("setenv");
    std::fs::write(&script_path, code)?;
    std::fs::write(&setenv_path, "")?;

    let mut command = match runtime {
        Some(cmd) => {
            let mut c = Command::new(cmd);
            c.arg(&script_path);
            c
        }
        None => {
            make_executable(&script_path)?;
            Command::new(&script_path)
        }
    };
    command
        .current_dir(workdir.path())
        .envs(base_env)
        .env("SETENV", &setenv_path);

    let start = Instant::now();
    let output = command.output()?;
    let elapsed_ms = start.elapsed().as_millis() as u64;

    let setenv_contents =
        std::fs::read_to_string(&setenv_path).unwrap_or_default();
    let setenv = parse_setenv(&setenv_contents);

    Ok(RunResult {
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        exit_code: output.status.code().unwrap_or(-1),
        elapsed_ms,
        setenv,
    })
}

#[cfg(unix)]
fn make_executable(path: &std::path::Path) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut perms = std::fs::metadata(path)?.permissions();
    perms.set_mode(perms.mode() | 0o700);
    std::fs::set_permissions(path, perms)
}

#[cfg(windows)]
fn make_executable(_path: &std::path::Path) -> io::Result<()> {
    // Windows doesn't honor shebangs for arbitrary extensionless files;
    // direct-exec of a shebang script isn't expected to work here yet.
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_setenv_basic() {
        let m = parse_setenv(r#"FOO="bar""#);
        assert_eq!(m.get("FOO"), Some(&"bar".to_string()));
    }

    #[test]
    fn parse_setenv_multi_line() {
        let m = parse_setenv("A=\"1\"\nB=\"2\"\n");
        assert_eq!(m.get("A"), Some(&"1".to_string()));
        assert_eq!(m.get("B"), Some(&"2".to_string()));
    }

    #[test]
    fn parse_setenv_escaped_quotes() {
        let m = parse_setenv(r#"MSG="say \"hi\"""#);
        assert_eq!(m.get("MSG"), Some(&"say \"hi\"".to_string()));
    }

    #[test]
    fn parse_setenv_empty() {
        assert!(parse_setenv("").is_empty());
        assert!(parse_setenv("\n\n  \n").is_empty());
    }

    #[test]
    fn run_shell_basic() {
        let env = HashMap::new();
        let result = run_shell("echo hi", Some("sh"), &env).unwrap();
        assert_eq!(result.stdout.trim(), "hi");
        assert_eq!(result.exit_code, 0);
    }

    #[test]
    fn run_shell_setenv_roundtrip() {
        let env = HashMap::new();
        let result =
            run_shell("echo 'FOO=\"bar\"' > \"$SETENV\"", Some("sh"), &env)
                .unwrap();
        assert_eq!(result.setenv.get("FOO"), Some(&"bar".to_string()));
    }

    #[test]
    fn run_shell_base_env_passed_through() {
        let mut env = HashMap::new();
        env.insert("MY_VAR".to_string(), "hello".to_string());
        let result = run_shell("echo \"$MY_VAR\"", Some("sh"), &env).unwrap();
        assert_eq!(result.stdout.trim(), "hello");
    }

    #[cfg(unix)]
    #[test]
    fn run_shell_via_shebang_direct_exec() {
        let env = HashMap::new();
        let script = "#!/bin/sh\necho ran via shebang\n";
        let result = run_shell(script, None, &env).unwrap();
        assert_eq!(result.stdout.trim(), "ran via shebang");
        assert_eq!(result.exit_code, 0);
    }

    #[test]
    fn run_shell_cwd_is_a_fresh_tempdir() {
        let env = HashMap::new();
        let result = run_shell("pwd", Some("sh"), &env).unwrap();
        let cwd = result.stdout.trim();
        assert!(cwd.contains("po-run-"));
    }
}
