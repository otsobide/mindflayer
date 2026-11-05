//! `mindweaver activate`: the shell code that activates a workspace, printed for
//! `eval`, and nothing at all when there is nothing to activate.
//!
//! The code itself is core's, asserted whole for each shell there; what is
//! tested here is which workspace and which shell it is written for. How the
//! shells run it is `shells_test.rs`'s.

mod common;

use common::*;
use mindweaver_core::activate::{self, Shell};
use mindweaver_core::ActivateError;

/// What core writes to activate the workspace at `root` in `shell`.
fn script(root: &Path, shell: Shell) -> String {
    activate::script(&WeaverWorkspace::open(root).unwrap(), shell).unwrap()
}

/// Every path under `dir`, in order: what a command that writes nothing
/// leaves as it found.
fn tree(dir: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut pending = vec![dir.to_path_buf()];
    while let Some(next) = pending.pop() {
        for entry in fs::read_dir(&next).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                pending.push(path.clone());
            }
            found.push(path);
        }
    }
    found.sort();
    found
}

/// A directory holding `writing`, a workspace, and `plain`, a directory
/// that is not one.
fn weavers() -> TempDir {
    let dir = TempDir::new().unwrap();
    weaver(dir.path(), &["init", "writing"]).unwrap();
    fs::create_dir(dir.path().join("plain")).unwrap();
    dir
}

#[test]
fn activate_prints_the_code_for_the_workspace_it_is_given() {
    let dir = weavers();

    let outcome = weaver(dir.path(), &["activate", "writing", "--shell", "bash"]).unwrap();

    assert_eq!(
        outcome,
        Outcome {
            stdout: script(&dir.path().join("writing"), Shell::Bash),
            stderr: Vec::new(),
            ok: true,
        }
    );
}

#[test]
fn a_workspace_is_activated_by_its_mindweaver_folder_too() {
    let dir = weavers();
    let folder = format!("writing/{WEAVER_DIR}");

    let outcome = weaver(dir.path(), &["activate", &folder, "--shell", "zsh"]).unwrap();

    assert_eq!(
        outcome.stdout,
        script(&dir.path().join("writing"), Shell::Zsh)
    );
}

#[test]
fn activate_on_its_own_activates_the_workspace_a_command_here_would_use() {
    let dir = workspace_with_two();

    let outcome = weaver(&dir.path().join("alpha"), &["activate", "--shell", "fish"]).unwrap();

    assert_eq!(outcome.stdout, script(dir.path(), Shell::Fish));
}

#[test]
fn activate_on_its_own_takes_the_workspace_w_names() {
    let dir = workspace_with_two();
    let other = weavers();
    let writing = other.path().join("writing");

    let outcome = weaver(
        &dir.path().join("alpha"),
        &[
            "activate",
            "-W",
            &writing.to_string_lossy(),
            "--shell",
            "pwsh",
        ],
    )
    .unwrap();

    assert_eq!(outcome.stdout, script(&writing, Shell::Pwsh));
}

#[test]
fn activating_a_directory_that_is_no_workspace_is_refused_and_writes_nothing() {
    let dir = weavers();
    let before = tree(dir.path());

    let failure = weaver(dir.path(), &["activate", "plain", "--shell", "bash"]).unwrap_err();

    assert_eq!(
        failure.error.to_string(),
        format!(
            "{}: not a weaver workspace (no .mindweaver/weaver.toml or .mindflayer/flayer.toml)",
            dir.path().join("plain").display()
        )
    );
    assert_eq!(tree(dir.path()), before);
}

#[test]
fn activating_the_default_workspace_before_it_is_made_makes_nothing() {
    let home = TempDir::new().unwrap();
    let elsewhere = TempDir::new().unwrap();

    let failure = weaver_at_home(
        elsewhere.path(),
        home.path(),
        &["activate", "--shell", "bash"],
    )
    .unwrap_err();

    assert!(
        matches!(
            *failure.error,
            CliError::Activate(ActivateError::Unmade { .. })
        ),
        "{}",
        failure.error
    );
    assert!(tree(home.path()).is_empty());
}

#[test]
fn activate_is_mind_weaver_activate_too() {
    let dir = weavers();
    let args = ["activate", "writing", "--shell", "bash"];

    let nested = mind(dir.path(), &[&["weaver"][..], &args].concat()).unwrap();

    assert_eq!(nested, weaver(dir.path(), &args).unwrap());
}

// ---------------------------------------------------------------------------
// As the binary runs: what $SHELL says, and what reaches `eval`
// ---------------------------------------------------------------------------

#[test]
fn the_shell_is_guessed_from_shell() {
    let dir = weavers();
    let writing = dir.path().join("writing");

    for (program, shell) in [
        ("/bin/bash", Shell::Bash),
        ("/usr/bin/zsh", Shell::Zsh),
        ("/usr/bin/fish", Shell::Fish),
        ("/usr/bin/pwsh", Shell::Pwsh),
    ] {
        let printed = spawn(
            "mindweaver",
            dir.path(),
            &["activate", "writing"],
            &[("SHELL", program)],
        );

        assert!(printed.ok, "{program}: {}", printed.stderr);
        assert_eq!(printed.stdout, script(&writing, shell), "{program}");
    }
}

#[test]
fn the_shell_named_wins_over_the_one_shell_names() {
    let dir = weavers();

    let printed = spawn(
        "mindweaver",
        dir.path(),
        &["activate", "writing", "--shell", "fish"],
        &[("SHELL", "/bin/bash")],
    );

    assert!(printed.ok, "{}", printed.stderr);
    assert_eq!(
        printed.stdout,
        script(&dir.path().join("writing"), Shell::Fish)
    );
}

#[test]
fn with_no_shell_to_guess_activate_prints_no_code() {
    let dir = weavers();

    for (program, said) in [
        ("", "error: $SHELL is not set; say which shell to write for with --shell (bash, zsh, fish or pwsh)\n"),
        ("/bin/tcsh", "error: cannot tell which shell `/bin/tcsh` is; say which to write for with --shell (bash, zsh, fish or pwsh)\n"),
    ] {
        let printed = spawn(
            "mindweaver",
            dir.path(),
            &["activate", "writing"],
            &[("SHELL", program)],
        );

        assert!(!printed.ok);
        assert_eq!(printed.stdout, "", "{program:?}");
        assert_eq!(printed.stderr, said, "{program:?}");
    }
}

#[test]
fn a_failed_activation_gives_eval_nothing_to_run() {
    let dir = weavers();

    let printed = spawn(
        "mindweaver",
        dir.path(),
        &["activate", "plain", "--shell", "bash"],
        &[],
    );

    assert!(!printed.ok);
    assert_eq!(printed.stdout, "");
    assert!(
        printed.stderr.starts_with("error: ") && printed.stderr.contains("not a weaver workspace"),
        "{}",
        printed.stderr
    );
}
