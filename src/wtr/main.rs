use std::io::{self, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const HELP: &str = include_str!("help.txt");

struct CreateArgs {
    branch: String,
    base: Option<String>,
    yes: bool,
    no_copy: bool,
}

enum Args {
    Create(CreateArgs),
    Clear { yes: bool },
    Main,
}

fn parse_args() -> Result<Option<Args>, String> {
    let mut branch = None;
    let mut base = None;
    let mut clear = false;
    let mut main = false;
    let mut yes = false;
    let mut no_copy = false;
    let mut iter = std::env::args().skip(1);

    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "-h" | "--help" => {
                print!("{HELP}");
                return Ok(None);
            }
            "--clear" => {
                if clear {
                    return Err(
                        "--clear can only be specified once".to_string()
                    );
                }
                clear = true;
            }
            "--main" => main = true,
            "-y" => yes = true,
            "--no-copy" => no_copy = true,
            "--base" => {
                if base.is_some() {
                    return Err("--base can only be specified once".to_string());
                }
                let value = iter.next().ok_or_else(|| {
                    "--base requires a branch name".to_string()
                })?;
                if value.starts_with('-') {
                    return Err(
                        "--base value must not start with '-'".to_string()
                    );
                }
                base = Some(value);
            }
            arg if arg.starts_with("--base=") => {
                if base.is_some() {
                    return Err("--base can only be specified once".to_string());
                }
                let value = &arg["--base=".len()..];
                if value.is_empty() || value.starts_with('-') {
                    return Err(
                        "--base requires a branch name not starting with '-'"
                            .to_string(),
                    );
                }
                base = Some(value.to_string());
            }
            arg if arg.starts_with('-') => {
                return Err(format!("unknown option: {arg}"));
            }
            _ if branch.is_some() => {
                return Err("only one new branch name may be given".to_string());
            }
            _ => branch = Some(arg),
        }
    }

    if main {
        if clear || branch.is_some() || base.is_some() || yes || no_copy {
            return Err(
                "--main cannot be combined with other options".to_string()
            );
        }
        return Ok(Some(Args::Main));
    }

    if clear {
        if branch.is_some() || base.is_some() || no_copy {
            return Err(
                "--clear cannot be combined with a branch, --base, or --no-copy"
                    .to_string(),
            );
        }
        return Ok(Some(Args::Clear { yes }));
    }

    let Some(branch) = branch else {
        return Err("a new branch name is required".to_string());
    };
    if branch.is_empty() {
        return Err("a new branch name cannot be empty".to_string());
    }
    if base.as_deref() == Some("") {
        return Err("a base branch name cannot be empty".to_string());
    }

    Ok(Some(Args::Create(CreateArgs {
        branch,
        base,
        yes,
        no_copy,
    })))
}

fn git_output(root: Option<&Path>, args: &[&str]) -> Result<Output, String> {
    let mut command = Command::new("git");
    command.args(args);
    if let Some(root) = root {
        command.current_dir(root);
    }
    command
        .output()
        .map_err(|error| format!("failed to run git: {error}"))
}

fn find_root() -> Result<PathBuf, String> {
    let output = git_output(None, &["rev-parse", "--show-toplevel"])?;
    if !output.status.success() {
        return Err(
            "the current directory is not inside a git repository".to_string()
        );
    }
    let root = String::from_utf8(output.stdout)
        .map_err(|_| "git returned a non-UTF-8 repository path".to_string())?;
    let root = root.trim_end_matches(['\r', '\n']);
    if root.is_empty() {
        return Err("git returned an empty repository path".to_string());
    }
    Ok(PathBuf::from(root))
}

fn abbreviation(name: &str) -> String {
    let mut result = String::new();
    let mut at_word_start = true;
    for ch in name.chars() {
        if ch.is_ascii() && !ch.is_ascii_alphanumeric() {
            at_word_start = true;
        } else {
            if at_word_start {
                result.push(ch);
            }
            at_word_start = false;
        }
    }
    result
}

fn worktree_path(root: &Path, branch: &str) -> Result<PathBuf, String> {
    let repo_name = root
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| {
            "could not read the repository folder name".to_string()
        })?;
    let prefix = abbreviation(repo_name);
    if prefix.is_empty() {
        return Err(
            "could not derive a prefix from the repository folder name"
                .to_string(),
        );
    }
    let branch_leaf = branch.rsplit('/').next().unwrap_or_default();
    if branch_leaf.is_empty() {
        return Err("the branch name must not end with '/'".to_string());
    }
    let parent = root.parent().ok_or_else(|| {
        "could not find a parent folder for the repository".to_string()
    })?;
    Ok(parent.join(format!("-{prefix}--{branch_leaf}")))
}

struct Worktree {
    path: PathBuf,
    branch: Option<String>,
}

fn worktrees(root: &Path) -> Result<Vec<Worktree>, String> {
    let output = git_output(Some(root), &["worktree", "list", "--porcelain"])?;
    if !output.status.success() {
        return Err("failed to list git worktrees".to_string());
    }
    let listing = String::from_utf8(output.stdout)
        .map_err(|_| "git returned a non-UTF-8 worktree list".to_string())?;

    let mut result = Vec::new();
    let mut path = None;
    let mut branch = None;
    for line in listing.lines().chain(std::iter::once("")) {
        if let Some(value) = line.strip_prefix("worktree ") {
            path = Some(PathBuf::from(value));
        } else if let Some(value) = line.strip_prefix("branch refs/heads/") {
            branch = Some(value.to_string());
        } else if line.is_empty() {
            if let Some(path) = path.take() {
                result.push(Worktree {
                    path,
                    branch: branch.take(),
                });
            }
        }
    }
    Ok(result)
}

fn branch_exists(root: &Path, branch: &str) -> Result<bool, String> {
    let reference = format!("refs/heads/{branch}");
    let output = Command::new("git")
        .args(["show-ref", "--verify", "--quiet", &reference])
        .current_dir(root)
        .status()
        .map_err(|error| format!("failed to run git: {error}"))?;
    match output.code() {
        Some(0) => Ok(true),
        Some(1) => Ok(false),
        _ => {
            Err("failed to check whether the branch already exists".to_string())
        }
    }
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn same_path(left: &Path, right: &Path) -> bool {
    if left == right {
        return true;
    }
    matches!((left.canonicalize(), right.canonicalize()), (Ok(a), Ok(b)) if a == b)
}

fn main_worktree(worktrees: &[Worktree]) -> Result<&Worktree, String> {
    worktrees
        .first()
        .ok_or_else(|| "git returned no worktrees".to_string())
}

fn remote_for_branch(
    root: &Path,
    branch: &str,
) -> Result<Option<String>, String> {
    let output = git_output(Some(root), &["remote"])?;
    if !output.status.success() {
        return Err("failed to list git remotes".to_string());
    }
    let remotes = String::from_utf8_lossy(&output.stdout);
    let reference = format!("refs/heads/{branch}");
    let mut matches = Vec::new();
    for remote in remotes.lines() {
        let output = git_output(
            Some(root),
            &["ls-remote", "--heads", remote, &reference],
        )?;
        if !output.status.success() {
            return Err(format!("failed to query remote '{remote}'"));
        }
        let found = String::from_utf8_lossy(&output.stdout)
            .lines()
            .any(|line| line.split_whitespace().nth(1) == Some(&reference));
        if found {
            matches.push(remote.to_string());
        }
    }

    if matches.contains(&"origin".to_string()) {
        return Ok(Some("origin".to_string()));
    }
    match matches.len() {
        0 => Ok(None),
        1 => Ok(matches.pop()),
        _ => Err(format!(
            "branch '{branch}' exists on multiple remotes: {}",
            matches.join(", ")
        )),
    }
}

fn confirm(prompt: &str, yes: bool) -> Result<bool, String> {
    if yes {
        return Ok(true);
    }
    if !io::stdin().is_terminal() {
        return Err(
            "confirmation requires a TTY; pass -y to confirm non-interactively"
                .to_string(),
        );
    }

    eprint!("{prompt} [y/N] ");
    io::stderr()
        .flush()
        .map_err(|error| format!("failed to write prompt: {error}"))?;
    let mut answer = String::new();
    let read = io::stdin()
        .read_line(&mut answer)
        .map_err(|error| format!("failed to read confirmation: {error}"))?;
    Ok(read > 0 && answer.trim().eq_ignore_ascii_case("y"))
}

fn report_git_output(output: &Output) {
    if !output.stdout.is_empty() {
        eprint!("{}", String::from_utf8_lossy(&output.stdout));
    }
    if !output.stderr.is_empty() {
        eprint!("{}", String::from_utf8_lossy(&output.stderr));
    }
}

fn run_git(command: &mut Command) -> Result<(), String> {
    let output = command
        .output()
        .map_err(|error| format!("failed to run git: {error}"))?;
    report_git_output(&output);
    if output.status.success() {
        Ok(())
    } else {
        Err(format!("git command failed with {}", output.status))
    }
}

fn print_worktree_path(path: &Path, copy_to_clipboard: bool) {
    let path = path.to_string_lossy().into_owned();
    println!("{path}");

    if copy_to_clipboard && io::stdout().is_terminal() {
        match arboard::Clipboard::new()
            .and_then(|mut clipboard| clipboard.set_text(path.clone()))
        {
            Ok(()) => eprintln!("copied worktree path to clipboard"),
            Err(error) => {
                eprintln!("could not copy worktree path to clipboard: {error}");
            }
        }
    }
}

fn run_create(args: CreateArgs) -> Result<(), String> {
    let root = find_root()?;
    let all_worktrees = worktrees(&root)?;
    let main_root = &main_worktree(&all_worktrees)?.path;
    let path = worktree_path(main_root, &args.branch)?;

    match path.symlink_metadata() {
        Ok(_) => {
            let is_branch_worktree = all_worktrees.iter().any(|worktree| {
                same_path(&worktree.path, &path)
                    && worktree.branch.as_deref() == Some(args.branch.as_str())
                    && path.is_dir()
            });
            if is_branch_worktree {
                print_worktree_path(&path, !args.no_copy);
                return Ok(());
            }
            return Err(format!(
                "destination already exists but is not a worktree for '{}': {}",
                args.branch,
                path.display()
            ));
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(format!(
                "failed to inspect destination {}: {error}",
                path.display()
            ));
        }
    }

    let existing_branch = branch_exists(&root, &args.branch)?;
    if let Some(worktree) = all_worktrees.iter().find(|worktree| {
        worktree.branch.as_deref() == Some(args.branch.as_str())
    }) {
        return Err(format!(
            "branch '{}' is already checked out at {}",
            args.branch,
            worktree.path.display()
        ));
    }
    if existing_branch && args.base.is_some() {
        return Err(
            "--base only applies when creating a new branch; omit it to use the existing branch"
                .to_string(),
        );
    }

    enum Source {
        ExistingLocal,
        New(Option<String>),
        Remote(String),
    }
    let source = if existing_branch {
        Source::ExistingLocal
    } else if let Some(base) = args.base.clone() {
        Source::New(Some(base))
    } else if let Some(remote) = remote_for_branch(&root, &args.branch)? {
        Source::Remote(remote)
    } else {
        Source::New(None)
    };

    match &source {
        Source::ExistingLocal => eprintln!(
            "git worktree add {} {}",
            shell_quote(&path.to_string_lossy()),
            shell_quote(&args.branch)
        ),
        Source::New(base) => {
            let mut command = format!(
                "git worktree add -b {} {}",
                shell_quote(&args.branch),
                shell_quote(&path.to_string_lossy())
            );
            if let Some(base) = base {
                command.push(' ');
                command.push_str(&shell_quote(base));
            }
            eprintln!("{command}");
        }
        Source::Remote(remote) => {
            let refspec = format!(
                "+refs/heads/{}:refs/remotes/{remote}/{}",
                args.branch, args.branch
            );
            eprintln!(
                "git fetch {} {}",
                shell_quote(remote),
                shell_quote(&refspec)
            );
            eprintln!(
                "git worktree add --track -b {} {} {}/{}",
                shell_quote(&args.branch),
                shell_quote(&path.to_string_lossy()),
                shell_quote(remote),
                shell_quote(&args.branch)
            );
        }
    }

    if !confirm("Create this worktree?", args.yes)? {
        eprintln!("cancelled");
        return Ok(());
    }

    if let Source::Remote(remote) = &source {
        let refspec = format!(
            "+refs/heads/{}:refs/remotes/{remote}/{}",
            args.branch, args.branch
        );
        let mut fetch = Command::new("git");
        fetch.args(["fetch", remote, &refspec]).current_dir(&root);
        run_git(&mut fetch)?;
    }

    let mut git = Command::new("git");
    git.args(["worktree", "add"]).current_dir(&root);
    match &source {
        Source::ExistingLocal => {
            git.arg(&path).arg(&args.branch);
        }
        Source::New(base) => {
            git.arg("-b").arg(&args.branch).arg(&path);
            if let Some(base) = base {
                git.arg(base);
            }
        }
        Source::Remote(remote) => {
            git.args(["--track", "-b"])
                .arg(&args.branch)
                .arg(&path)
                .arg(format!("{remote}/{}", args.branch));
        }
    }
    run_git(&mut git)?;
    print_worktree_path(&path, !args.no_copy);
    Ok(())
}

fn run_main_path() -> Result<(), String> {
    let root = find_root()?;
    let all_worktrees = worktrees(&root)?;
    println!("{}", main_worktree(&all_worktrees)?.path.display());
    Ok(())
}

fn run_clear(yes: bool) -> Result<(), String> {
    let root = find_root()?;
    let all_worktrees = worktrees(&root)?;
    let main = main_worktree(&all_worktrees)?;
    if !same_path(&root, &main.path) {
        return Err(format!(
            "--clear can only be run from the main worktree: {}",
            main.path.display()
        ));
    }

    let targets: Vec<_> = all_worktrees
        .iter()
        .filter(|worktree| !same_path(&worktree.path, &main.path))
        .collect();
    if targets.is_empty() {
        eprintln!("no other worktrees to clear");
    } else {
        eprintln!("Worktrees to remove:");
        for worktree in &targets {
            let branch = worktree.branch.as_deref().unwrap_or("detached");
            eprintln!("  {} ({branch})", worktree.path.display());
        }
        if !confirm("Remove these worktrees?", yes)? {
            eprintln!("cancelled");
            return Ok(());
        }
    }

    let mut failed_removals = Vec::new();
    let mut stale_paths = Vec::new();
    for worktree in targets {
        match worktree.path.symlink_metadata() {
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                stale_paths.push(worktree.path.clone());
                continue;
            }
            Err(error) => {
                eprintln!("failed: {} ({error})", worktree.path.display());
                failed_removals.push(worktree.path.clone());
                continue;
            }
            Ok(_) => {}
        }
        let mut remove = Command::new("git");
        remove
            .args(["worktree", "remove"])
            .arg(&worktree.path)
            .current_dir(&root);
        match remove.output() {
            Ok(output) if output.status.success() => {
                eprintln!("removed: {}", worktree.path.display());
            }
            Ok(output) => {
                let reason = String::from_utf8_lossy(&output.stderr);
                eprintln!(
                    "failed: {} ({})",
                    worktree.path.display(),
                    reason.trim()
                );
                failed_removals.push(worktree.path.clone());
            }
            Err(error) => {
                eprintln!("failed: {} ({error})", worktree.path.display());
                failed_removals.push(worktree.path.clone());
            }
        }
    }

    let mut prune = Command::new("git");
    prune
        .args(["worktree", "prune", "--expire", "now"])
        .current_dir(&root);
    let prune_result = prune.output();
    let prune_succeeded = match prune_result {
        Ok(output) if output.status.success() => true,
        Ok(output) => {
            let reason = String::from_utf8_lossy(&output.stderr);
            eprintln!("failed to prune stale worktrees ({})", reason.trim());
            false
        }
        Err(error) => {
            eprintln!("failed to run git worktree prune: {error}");
            false
        }
    };

    let mut failed_prunes = Vec::new();
    if !stale_paths.is_empty() {
        if prune_succeeded {
            let remaining = worktrees(&root)?;
            for path in stale_paths {
                if remaining
                    .iter()
                    .any(|worktree| same_path(&worktree.path, &path))
                {
                    eprintln!(
                        "failed to prune stale worktree: {}",
                        path.display()
                    );
                    failed_prunes.push(path);
                } else {
                    eprintln!("pruned stale worktree: {}", path.display());
                }
            }
        } else {
            for path in stale_paths {
                eprintln!("failed to prune stale worktree: {}", path.display());
                failed_prunes.push(path);
            }
        }
    }

    if !failed_removals.is_empty() {
        eprintln!("To force-remove a failed worktree, run:");
        for path in &failed_removals {
            eprintln!(
                "  git worktree remove --force {}",
                shell_quote(&path.to_string_lossy())
            );
        }
    }
    if !failed_prunes.is_empty() {
        eprintln!(
            "To remove stale worktree records, run: git worktree prune --expire now"
        );
    }
    if !prune_succeeded && failed_prunes.is_empty() {
        eprintln!(
            "To retry stale worktree cleanup, run: git worktree prune --expire now"
        );
    }
    let failed = failed_removals.len()
        + failed_prunes.len()
        + usize::from(!prune_succeeded && failed_prunes.is_empty());
    if failed > 0 {
        return Err(format!("{failed} worktree(s) could not be cleared"));
    }
    Ok(())
}

fn main() {
    let args = match parse_args() {
        Ok(Some(args)) => args,
        Ok(None) => return,
        Err(error) => {
            eprintln!("error: {error}\n\n{HELP}");
            std::process::exit(2);
        }
    };

    let result = match args {
        Args::Create(args) => run_create(args),
        Args::Clear { yes } => run_clear(yes),
        Args::Main => run_main_path(),
    };
    if let Err(error) = result {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}
