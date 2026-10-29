//! Unit tests for `error.rs`, kept beside it rather than inside it.

use super::*;

/// A code reaches the panel without the type it came from, so one the CLI
/// gives that core gives too, or that two of the CLI's types give, would be
/// two conditions a front end cannot tell apart.
#[test]
fn no_code_the_cli_gives_is_given_twice_here_or_in_core() {
    let mut seen: Vec<(&str, &str)> = mindflayer_core::code::ALL
        .iter()
        .flat_map(|(owner, codes)| codes.iter().map(move |code| (*owner, *code)))
        .collect();
    let mut twice: Vec<String> = Vec::new();
    for (owner, codes) in CODES {
        for code in *codes {
            if let Some((first, _)) = seen.iter().find(|(_, seen)| seen == code) {
                twice.push(format!("`{code}` in {first} and {owner}"));
            }
            seen.push((owner, code));
        }
    }

    assert!(twice.is_empty(), "codes given twice: {twice:?}");
}

#[test]
fn every_code_the_cli_gives_is_snake_case() {
    let unfit: Vec<&str> = CODES
        .iter()
        .flat_map(|(_, codes)| codes.iter().copied())
        .filter(|code| {
            !code.split('_').all(|word| {
                !word.is_empty()
                    && word
                        .chars()
                        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
            })
        })
        .collect();

    assert!(unfit.is_empty(), "codes not in snake_case: {unfit:?}");
}

/// What the CLI says of a condition core names, in words of its own, goes to
/// the panel by core's code for it: one condition, one code.
#[test]
fn every_code_the_cli_echoes_is_one_core_gives() {
    let core: Vec<&str> = mindflayer_core::code::ALL
        .iter()
        .flat_map(|(_, codes)| codes.iter().copied())
        .collect();
    let strange: Vec<&str> = CliError::ECHOED
        .iter()
        .copied()
        .filter(|code| !core.contains(code))
        .collect();

    assert!(strange.is_empty(), "echoed but not core's: {strange:?}");
}

#[test]
fn an_error_core_answered_keeps_its_code_in_the_cli_s_words() {
    let managed = CliError::from(ResolveError::NotManaged {
        project: PathBuf::from("/work/alpha"),
        workspace: PathBuf::from("/work"),
    });
    let carried = CliError::from(WorkspaceError::NotAProject {
        path: PathBuf::from("/work/alpha"),
    });

    assert_eq!(managed.code(), "not_managed");
    assert!(
        managed.to_string().ends_with("`mind link` adds it"),
        "{managed}"
    );
    assert_eq!(carried.code(), "not_a_project");
    assert_eq!(
        CliError::MigrateUnrecorded(Box::new(carried)).code(),
        "not_a_project",
        "what the ledger could not read is what went wrong"
    );
    assert_eq!(CliError::InstallWhere.code(), "install_no_projects");
}

/// A held port is said with what to do about it, which `--dev` changes: the
/// dev server's proxy reaches one port only, so another, or 0, is no way out
/// unless the proxy is told it too.
#[test]
fn a_port_the_panel_cannot_take_is_said_with_what_to_do_about_it() {
    let unbound = |port, dev| {
        CliError::Unbound {
            port,
            dev,
            source: io::Error::new(io::ErrorKind::AddrInUse, "address in use"),
        }
        .to_string()
    };

    assert_eq!(
        unbound(9000, false),
        "the panel cannot listen on 127.0.0.1:9000: address in use; name another with \
         --port, or 0 for one the system picks"
    );
    assert_eq!(
        unbound(7878, true),
        "the panel cannot listen on 127.0.0.1:7878: address in use; the panel's dev \
         server reaches only the port `SERVER` names in apps/web/vite.config.ts, so \
         stop what holds it, or name another there and with --port"
    );
    for dev in [false, true] {
        assert_eq!(
            unbound(0, dev),
            "the panel cannot listen on 127.0.0.1:0: address in use"
        );
    }
}
