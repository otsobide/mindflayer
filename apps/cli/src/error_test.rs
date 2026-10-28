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
