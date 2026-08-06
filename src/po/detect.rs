/// The result of recognizing a piped-in blob as executable code.
pub struct FenceInfo {
    pub lang: Option<String>,
    pub body: String,
    /// `true` if this was recognized via a shebang line, meaning `body`
    /// should be executed directly (as a `chmod +x`'d file) rather than
    /// passed as an argument to an interpreter picked by [`resolve_runtime`].
    pub is_shebang: bool,
}

/// Try a triple-fence (```` ``` ```` or `~~~`) block: first line is the
/// opening fence with an optional language hint, last non-empty line is a
/// matching closing fence of the same kind.
fn fence_triple(input: &str) -> Option<FenceInfo> {
    let mut lines = input.lines();
    let first = lines.next()?.trim_end();
    let fence = if first.starts_with("```") {
        "```"
    } else if first.starts_with("~~~") {
        "~~~"
    } else {
        return None;
    };
    let lang = first[fence.len()..].trim();
    let lang = if lang.is_empty() {
        None
    } else {
        Some(lang.to_string())
    };

    let rest: Vec<&str> = lines.collect();
    let close_idx = rest.iter().rposition(|l| l.trim_end() == fence)?;
    let body = rest[..close_idx].join("\n");
    Some(FenceInfo {
        lang,
        body,
        is_shebang: false,
    })
}

/// Try a single-backtick inline block: first char and last non-whitespace
/// char of the trimmed input are both a single backtick, with no language
/// hint (bare code only).
fn fence_single(input: &str) -> Option<FenceInfo> {
    let trimmed = input.trim();
    if trimmed.len() < 2 || !trimmed.starts_with('`') || !trimmed.ends_with('`')
    {
        return None;
    }
    if trimmed.starts_with("```") {
        return None;
    }
    let body = &trimmed[1..trimmed.len() - 1];
    Some(FenceInfo {
        lang: None,
        body: body.to_string(),
        is_shebang: false,
    })
}

/// Extract the interpreter name from a shebang line, e.g.
/// `#!/usr/bin/env bash` -> "bash", `#!/bin/sh` -> "sh".
fn shebang_lang(line: &str) -> Option<String> {
    let rest = line.strip_prefix("#!")?.trim();
    let mut parts = rest.split_whitespace();
    let first = parts.next()?;
    let name = first.rsplit('/').next().unwrap_or(first);
    if name == "env" {
        parts.next().map(str::to_string)
    } else {
        Some(name.to_string())
    }
}

/// Try to recognize `input` as a shebang script: the whole input is kept
/// as the body (including the shebang line itself), the language is derived
/// from the shebang.
fn fence_shebang(input: &str) -> Option<FenceInfo> {
    let first = input.lines().next()?;
    let lang = shebang_lang(first)?;
    Some(FenceInfo {
        lang: Some(lang),
        body: input.to_string(),
        is_shebang: true,
    })
}

/// Detect the executable content and its language hint from a piece of
/// piped-in text. Tries, in order: a triple-fence block, a single-backtick
/// inline block, a shebang line. Errors if none match.
pub fn detect_code(input: &str) -> Result<FenceInfo, String> {
    fence_triple(input)
        .or_else(|| fence_single(input))
        .or_else(|| fence_shebang(input))
        .ok_or_else(|| {
            "unrecognized input format (expected a code fence or a shebang)"
                .to_string()
        })
}

/// Map a detected language hint to an interpreter command this build knows
/// how to invoke (used for fenced code blocks, which have no shebang of
/// their own). Shebang scripts bypass this entirely and are executed
/// directly instead. `ruby`/`go`/`lua` are planned but not implemented yet.
pub fn resolve_runtime(lang: &str) -> Option<&'static str> {
    match lang {
        "sh" => Some("sh"),
        "bash" => Some("bash"),
        "python" | "python3" => Some("python3"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn triple_backtick_with_lang() {
        let input = "```bash\necho hi\necho bye\n```";
        let info = detect_code(input).unwrap();
        assert_eq!(info.lang.as_deref(), Some("bash"));
        assert_eq!(info.body, "echo hi\necho bye");
    }

    #[test]
    fn triple_tilde_with_lang() {
        let input = "~~~sh\necho hi\n~~~";
        let info = detect_code(input).unwrap();
        assert_eq!(info.lang.as_deref(), Some("sh"));
        assert_eq!(info.body, "echo hi");
    }

    #[test]
    fn triple_backtick_no_lang() {
        let input = "```\necho hi\n```";
        let info = detect_code(input).unwrap();
        assert_eq!(info.lang, None);
        assert_eq!(info.body, "echo hi");
    }

    #[test]
    fn single_backtick() {
        let input = "`echo hi`";
        let info = detect_code(input).unwrap();
        assert_eq!(info.lang, None);
        assert_eq!(info.body, "echo hi");
    }

    #[test]
    fn shebang_sh() {
        let input = "#!/bin/sh\necho hi\n";
        let info = detect_code(input).unwrap();
        assert_eq!(info.lang.as_deref(), Some("sh"));
        assert_eq!(info.body, input);
        assert!(info.is_shebang);
    }

    #[test]
    fn shebang_env_bash() {
        let input = "#!/usr/bin/env bash\necho hi\n";
        let info = detect_code(input).unwrap();
        assert_eq!(info.lang.as_deref(), Some("bash"));
        assert!(info.is_shebang);
    }

    #[test]
    fn fence_is_not_shebang() {
        let info = detect_code("```sh\necho hi\n```").unwrap();
        assert!(!info.is_shebang);
    }

    #[test]
    fn unrecognized() {
        assert!(detect_code("just some text\nno fence here").is_err());
    }

    #[test]
    fn resolve_runtime_known() {
        assert_eq!(resolve_runtime("sh"), Some("sh"));
        assert_eq!(resolve_runtime("bash"), Some("bash"));
        assert_eq!(resolve_runtime("python"), Some("python3"));
        assert_eq!(resolve_runtime("python3"), Some("python3"));
    }

    #[test]
    fn resolve_runtime_unknown() {
        assert_eq!(resolve_runtime("ruby"), None);
        assert_eq!(resolve_runtime("go"), None);
    }
}
