use lumiknit_cli_edc::po::{daemon::exec, detect, fmt, option};
use std::collections::HashMap;

#[test]
fn end_to_end_fenced_shell_block() {
    let input = "```sh\necho hello\n```";
    let info = detect::detect_code(input).unwrap();
    let runtime =
        detect::resolve_runtime(info.lang.as_deref().unwrap()).unwrap();

    let result =
        exec::run_shell(&info.body, Some(runtime), &HashMap::new()).unwrap();
    assert_eq!(result.stdout.trim(), "hello");
    assert_eq!(result.exit_code, 0);

    let outcome = fmt::RunOutcome {
        stdout: &result.stdout,
        stderr: &result.stderr,
        exit_code: result.exit_code,
        elapsed_ms: result.elapsed_ms,
    };
    let out = fmt::format_md(None, &outcome, false);
    assert_eq!(out, "```\nhello\n```\n");
}

#[test]
fn end_to_end_shebang_setenv_context() {
    let input = "#!/bin/sh\necho 'FOO=\"carried\"' > \"$SETENV\"\necho done\n";
    let info = detect::detect_code(input).unwrap();
    assert!(info.is_shebang);

    let mut ctx_env = HashMap::new();
    let result = exec::run_shell(&info.body, None, &ctx_env).unwrap();
    assert_eq!(result.stdout.trim(), "done");
    ctx_env.extend(result.setenv.clone());
    assert_eq!(ctx_env.get("FOO"), Some(&"carried".to_string()));

    // Next invocation in the same context should see the carried value.
    let result2 =
        exec::run_shell("echo \"$FOO\"", Some("sh"), &ctx_env).unwrap();
    assert_eq!(result2.stdout.trim(), "carried");
}

#[test]
fn unrecognized_input_is_rejected_before_running_anything() {
    let input = "just plain text, no fence or shebang";
    assert!(detect::detect_code(input).is_err());
}

#[test]
fn option_parse_picks_up_context_and_format_for_a_real_invocation() {
    let argv: Vec<String> = vec![
        "-x".into(),
        "Proj".into(),
        "-o".into(),
        "hash".into(),
        "-i".into(),
    ];
    let args = option::parse(&argv, |_| None).unwrap();
    assert_eq!(args.context, "proj");
    assert_eq!(args.out_fmt, option::OutFmt::Hash);
    assert!(args.print_stdin);
    assert_eq!(args.mode, option::Mode::Run);
}
