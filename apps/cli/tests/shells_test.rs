//! Activation as shells run it: `eval "$(flayer activate …)"` in bash, and
//! in zsh where it is installed, on its own and beside an active Python
//! virtualenv, activated in either order and deactivated in either order.
//!
//! Each script runs in a shell of its own that reads no startup file, with
//! an environment holding nothing but a PATH, a temporary HOME and what the
//! test sets: nothing here reads or changes anybody's own shell, and no
//! `MINDFLAYER_*` variable of the shell running the tests gets in. A shell
//! that is not installed is skipped, and so is a virtualenv when there is no
//! python3 to make one; each says so on stderr.
//!
//! Linux only for now, where the shells are the ones the code is written
//! for; fish and PowerShell are asserted as text, in core.

#![cfg(target_os = "linux")]

mod common;

use std::process::Command;

use common::*;

/// What every script starts with: a prompt to change, `state` to say what
/// activating has changed so far, and `activate_flayer` to do it the way
/// the README says to.
const PRELUDE: &str = r#"PS1='$ '
defined() {
    if typeset -f "$1" >/dev/null 2>&1; then printf yes; else printf no; fi
}
state() {
    printf '%s: prompt=[%s] workspace=[%s] virtualenv=[%s] path=[%s] deactivate=%s flayer-deactivate=%s\n' \
        "$1" "${PS1-}" "${MINDFLAYER_WORKSPACE-unset}" "${VIRTUAL_ENV-unset}" \
        "${PATH%%:*}" "$(defined deactivate)" "$(defined flayer-deactivate)"
}
activate_flayer() {
    eval "$("$FLAYER" activate --shell "$SHELL_NAME" "$@")"
}
"#;

/// A shell installed here, and how to start it reading no startup file.
struct Installed {
    name: &'static str,
    program: PathBuf,
    quiet: &'static [&'static str],
}

/// The shells the code is written for that are installed here.
fn shells() -> Vec<Installed> {
    let mut found = Vec::new();
    for (name, quiet) in [
        ("bash", &["--norc", "--noprofile"][..]),
        ("zsh", &["-f"][..]),
    ] {
        match on_path(name) {
            Some(program) => found.push(Installed {
                name,
                program,
                quiet,
            }),
            None => eprintln!("skipped {name}: not installed"),
        }
    }
    found
}

/// Where `name` is on the PATH the tests run with.
fn on_path(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(name))
        .find(|candidate| candidate.is_file())
}

/// What `script` prints, run after the prelude in `shell` from `dir`, with
/// `variables` as the environment's only paths besides the binary's; and
/// nothing on stderr, which is where a line of the code that went wrong
/// would say so while the rest ran on.
fn run(shell: &Installed, dir: &Path, script: &str, variables: &[(&str, &Path)]) -> String {
    let home = TempDir::new().unwrap();
    let output = Command::new(&shell.program)
        .args(shell.quiet)
        .arg("-c")
        .arg(format!("{PRELUDE}{script}"))
        .current_dir(dir)
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", home.path())
        .env("FLAYER", env!("CARGO_BIN_EXE_flayer"))
        .env("SHELL_NAME", shell.name)
        .envs(
            variables
                .iter()
                .map(|(name, path)| (*name, path.as_os_str())),
        )
        .output()
        .expect("run the shell");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{}: {stderr}", shell.name);
    assert_eq!(stderr, "", "{}", shell.name);
    String::from_utf8(output.stdout).unwrap()
}

/// A directory holding a workspace whose path has a space and both quotes
/// in it, with a skill of its own, and `other`, a second workspace.
struct Fixture {
    dir: TempDir,
    workspace: PathBuf,
    other: PathBuf,
}

const NAME: &str = r#"it's a "flayer""#;

fn fixture() -> Fixture {
    let dir = TempDir::new().unwrap();
    flayer(dir.path(), &["init", NAME]).unwrap();
    flayer(dir.path(), &["init", "other"]).unwrap();
    let workspace = dir.path().join(NAME);
    flayer(
        &workspace,
        &[
            "add",
            "skill",
            "prose-style",
            "How every essay here is written",
        ],
    )
    .unwrap();
    Fixture {
        workspace,
        other: dir.path().join("other"),
        dir,
    }
}

/// A Python virtualenv called `venv` under `dir`, made by the python3 on
/// the PATH without pip, which nothing here runs; `None`, said on stderr,
/// when there is no python3 that will make one.
fn virtualenv(dir: &Path) -> Option<PathBuf> {
    let Some(python) = on_path("python3") else {
        eprintln!("skipped: no python3 to make a virtualenv with");
        return None;
    };
    let venv = dir.join("venv");
    let home = TempDir::new().unwrap();
    let made = Command::new(python)
        .args(["-m", "venv", "--without-pip"])
        .arg(&venv)
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", home.path())
        .status();
    match made {
        Ok(status) if status.success() => Some(venv),
        _ => {
            eprintln!("skipped: python3 could not make a virtualenv");
            None
        }
    }
}

/// One line of `state`, for a shell with no virtualenv.
fn alone(label: &str, prompt: &str, workspace: Option<&Path>, flayer_deactivate: bool) -> String {
    line(label, prompt, workspace, None, flayer_deactivate)
}

/// One line of `state`: the prompt, the workspace and the virtualenv active,
/// if any, where the PATH starts, and which deactivate is defined; a
/// virtualenv's is exactly while it is active.
fn line(
    label: &str,
    prompt: &str,
    workspace: Option<&Path>,
    venv: Option<&Path>,
    flayer_deactivate: bool,
) -> String {
    let yes = |defined: bool| if defined { "yes" } else { "no" };
    let shown =
        |path: Option<&Path>| path.map_or(String::from("unset"), |path| path.display().to_string());
    format!(
        "{label}: prompt=[{prompt}] workspace=[{}] virtualenv=[{}] path=[{}] deactivate={} flayer-deactivate={}\n",
        shown(workspace),
        shown(venv),
        venv.map_or(String::from("/usr/bin"), |venv| venv.join("bin").display().to_string()),
        yes(venv.is_some()),
        yes(flayer_deactivate),
    )
}

/// The prompt while the fixture's workspace is active.
const ACTIVE: &str = r#"(flayer:it's a "flayer") $ "#;

#[test]
fn bash_and_zsh_accept_the_code_as_printed() {
    let fixture = fixture();
    for shell in shells() {
        let code = flayer(
            fixture.dir.path(),
            &["activate", NAME, "--shell", shell.name],
        )
        .unwrap()
        .stdout;
        let file = fixture.dir.path().join(format!("activate.{}", shell.name));
        fs::write(&file, code).unwrap();

        // Started as `run` starts a shell: zsh reads `~/.zshenv` even to
        // only check a file, and takes HOME from the account when it is unset.
        let home = TempDir::new().unwrap();
        let checked = Command::new(&shell.program)
            .args(shell.quiet)
            .arg("-n")
            .arg(&file)
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .env("HOME", home.path())
            .output()
            .unwrap();

        assert!(
            checked.status.success() && checked.stderr.is_empty(),
            "{}: {}",
            shell.name,
            String::from_utf8_lossy(&checked.stderr)
        );
    }
}

#[test]
fn activating_sets_the_variable_and_the_prompt_and_flayer_deactivate_restores_both() {
    let fixture = fixture();
    let listed = flayer(
        fixture.dir.path(),
        &["list", "-W", &fixture.workspace.to_string_lossy()],
    )
    .unwrap()
    .stdout;

    for shell in shells() {
        let printed = run(
            &shell,
            fixture.dir.path(),
            r#"state before
activate_flayer "$WORKSPACE"
state active
"$FLAYER" list
flayer-deactivate
state after
"#,
            &[("WORKSPACE", &fixture.workspace)],
        );

        assert_eq!(
            printed,
            [
                alone("before", "$ ", None, false),
                alone("active", ACTIVE, Some(&fixture.workspace), true),
                listed.clone(),
                alone("after", "$ ", None, false),
            ]
            .concat(),
            "{}",
            shell.name
        );
    }
}

#[test]
fn a_workspace_variable_set_before_activating_is_put_back_and_still_exported() {
    let fixture = fixture();
    let before = Path::new("/somewhere/else");

    for shell in shells() {
        let printed = run(
            &shell,
            fixture.dir.path(),
            r#"export MINDFLAYER_WORKSPACE=/somewhere/else
activate_flayer "$WORKSPACE"
state active
flayer-deactivate
state after
/bin/sh -c 'printf "exported: %s\n" "${MINDFLAYER_WORKSPACE-unset}"'
"#,
            &[("WORKSPACE", &fixture.workspace)],
        );

        assert_eq!(
            printed,
            [
                alone("active", ACTIVE, Some(&fixture.workspace), true),
                alone("after", "$ ", Some(before), false),
                String::from("exported: /somewhere/else\n"),
            ]
            .concat(),
            "{}",
            shell.name
        );
    }
}

#[test]
fn activating_another_workspace_takes_the_first_one_out_before_putting_it_in() {
    let fixture = fixture();

    for shell in shells() {
        let printed = run(
            &shell,
            fixture.dir.path(),
            r#"activate_flayer "$WORKSPACE"
activate_flayer "$OTHER"
state swapped
flayer-deactivate
state after
"#,
            &[("WORKSPACE", &fixture.workspace), ("OTHER", &fixture.other)],
        );

        assert_eq!(
            printed,
            [
                alone("swapped", "(flayer:other) $ ", Some(&fixture.other), true),
                alone("after", "$ ", None, false),
            ]
            .concat(),
            "{}",
            shell.name
        );
    }
}

#[test]
fn beside_a_virtualenv_activated_first_each_deactivate_undoes_only_its_own() {
    let fixture = fixture();
    let Some(venv) = virtualenv(fixture.dir.path()) else {
        return;
    };
    let ws = Some(fixture.workspace.as_path());
    let active = Some(venv.as_path());
    let both = r#"(flayer:it's a "flayer") (venv) $ "#;

    for shell in shells() {
        let variables = [
            ("WORKSPACE", fixture.workspace.as_path()),
            ("VENV", venv.as_path()),
        ];
        let ours_first = run(
            &shell,
            fixture.dir.path(),
            r#". "$VENV/bin/activate"
activate_flayer "$WORKSPACE"
state both
flayer-deactivate
state flayer-deactivated
deactivate
state both-deactivated
"#,
            &variables,
        );
        let theirs_first = run(
            &shell,
            fixture.dir.path(),
            r#". "$VENV/bin/activate"
activate_flayer "$WORKSPACE"
deactivate
state virtualenv-deactivated
flayer-deactivate
state both-deactivated
"#,
            &variables,
        );

        assert_eq!(
            ours_first,
            [
                line("both", both, ws, active, true),
                line("flayer-deactivated", "(venv) $ ", None, active, false),
                line("both-deactivated", "$ ", None, None, false),
            ]
            .concat(),
            "{}",
            shell.name
        );
        assert_eq!(
            theirs_first,
            [
                line("virtualenv-deactivated", ACTIVE, ws, None, true),
                line("both-deactivated", "$ ", None, None, false),
            ]
            .concat(),
            "{}",
            shell.name
        );
    }
}

#[test]
fn beside_a_virtualenv_activated_second_each_deactivate_undoes_only_its_own() {
    let fixture = fixture();
    let Some(venv) = virtualenv(fixture.dir.path()) else {
        return;
    };
    let ws = Some(fixture.workspace.as_path());
    let active = Some(venv.as_path());
    let both = r#"(venv) (flayer:it's a "flayer") $ "#;

    for shell in shells() {
        let variables = [
            ("WORKSPACE", fixture.workspace.as_path()),
            ("VENV", venv.as_path()),
        ];
        let ours_first = run(
            &shell,
            fixture.dir.path(),
            r#"activate_flayer "$WORKSPACE"
. "$VENV/bin/activate"
state both
flayer-deactivate
state flayer-deactivated
deactivate
state both-deactivated
"#,
            &variables,
        );
        let theirs_first = run(
            &shell,
            fixture.dir.path(),
            r#"activate_flayer "$WORKSPACE"
. "$VENV/bin/activate"
deactivate
state virtualenv-deactivated
flayer-deactivate
state both-deactivated
"#,
            &variables,
        );

        assert_eq!(
            ours_first,
            [
                line("both", both, ws, active, true),
                line("flayer-deactivated", "(venv) $ ", None, active, false),
                line("both-deactivated", "$ ", None, None, false),
            ]
            .concat(),
            "{}",
            shell.name
        );
        assert_eq!(
            theirs_first,
            [
                line("virtualenv-deactivated", ACTIVE, ws, None, true),
                line("both-deactivated", "$ ", None, None, false),
            ]
            .concat(),
            "{}",
            shell.name
        );
    }
}

#[test]
fn code_in_a_workspaces_path_or_name_is_never_run() {
    let dir = TempDir::new().unwrap();
    let name = "$(touch pwned) `touch pwned-too`";
    flayer(dir.path(), &["init", name]).unwrap();
    let workspace = dir.path().join(name);
    let Some(bash) = shells().into_iter().find(|shell| shell.name == "bash") else {
        return;
    };

    // `${PS1@P}` expands the prompt as bash does each time it draws it.
    let printed = run(
        &bash,
        dir.path(),
        r#"activate_flayer "$WORKSPACE"
printf 'drawn: %s\n' "${PS1@P}"
printf 'workspace: %s\n' "$MINDFLAYER_WORKSPACE"
flayer-deactivate
"#,
        &[("WORKSPACE", &workspace)],
    );

    assert_eq!(
        printed,
        format!(
            "drawn: (flayer:_(touch pwned) _touch pwned-too_) $ \nworkspace: {}\n",
            workspace.display()
        )
    );
    assert!(!dir.path().join("pwned").exists());
    assert!(!dir.path().join("pwned-too").exists());
}
