//! Unit tests for `sources.rs`, kept beside it rather than inside it.

use super::*;

use crate::gather::GatherError;
use crate::ledger::SourceKind;
use crate::mother::{UrlMother, WorkspaceMother};

/// A workspace whose ledger has registered a source for each of `urls`, and
/// the sources, as the ledger lists them.
fn registered(urls: &[String]) -> (tempfile::TempDir, WeaverWorkspace, Ledger, Vec<Source>) {
    let (dir, workspace) = WorkspaceMother::empty();
    let ledger = workspace.ledger().unwrap();
    for url in urls {
        ledger
            .source_for(SourceKind::Git, url, None, "skills", Kind::Skill)
            .unwrap();
    }
    let sources = ledger.sources().unwrap();
    (dir, workspace, ledger, sources)
}

#[test]
fn a_source_is_found_by_the_name_it_is_called_and_nothing_else() {
    let (_dir, _workspace, ledger, sources) = registered(&[UrlMother::https(), UrlMother::https()]);

    for source in &sources {
        assert_eq!(named(&ledger, source.name.as_str()).unwrap(), *source);
    }
    let by_url = named(&ledger, sources[0].url.as_str()).unwrap_err();
    assert!(
        matches!(&by_url, SourceError::NotFound { name } if name == sources[0].url.as_str()),
        "{by_url:?}"
    );
    assert_eq!(by_url.code(), "source_not_found");
}

#[test]
fn a_rename_calls_the_source_its_new_name_and_keeps_the_rest_of_it() {
    let (_dir, workspace, ledger, sources) = registered(&[UrlMother::https()]);

    let renamed = rename(&workspace, &ledger, sources[0].name.as_str(), "team").unwrap();

    assert_eq!(renamed.was, sources[0].name);
    assert_eq!(renamed.source.name.as_str(), "team");
    assert!(!renamed.unchanged());
    assert_eq!(
        ledger.sources().unwrap(),
        [Source {
            name: SourceName::parse("team").unwrap(),
            ..sources[0].clone()
        }]
    );
    assert_eq!(renamed.source.origin(), sources[0].origin());
}

#[test]
fn a_rename_to_the_name_it_has_changes_nothing() {
    let (_dir, workspace, ledger, sources) = registered(&[UrlMother::https()]);
    let name = sources[0].name.to_string();

    let renamed = rename(&workspace, &ledger, &name, &name).unwrap();

    assert!(renamed.unchanged());
    assert_eq!(ledger.sources().unwrap(), sources);
}

/// Refused as `gather git --name` refuses the same names, by its codes.
#[test]
fn a_rename_to_what_no_source_can_be_called_or_another_is_refused_as_gather_refuses_it() {
    let (_dir, workspace, ledger, sources) = registered(&[UrlMother::https(), UrlMother::https()]);
    let (first, second) = (sources[0].name.as_str(), sources[1].name.as_str());

    let invalid = rename(&workspace, &ledger, first, "team/v3").unwrap_err();
    let taken = rename(&workspace, &ledger, first, second).unwrap_err();

    assert_eq!(
        invalid.code(),
        GatherError::NameInvalid {
            name: String::new()
        }
        .code()
    );
    assert_eq!(invalid.code(), "source_name_invalid");
    assert_eq!(
        invalid.to_string(),
        "`team/v3` cannot name a source: it has to be letters, digits, `.`, `_` and `-`, not \
         starting with `-`, and neither `.`, `..` nor `workspace`"
    );
    assert_eq!(taken.code(), "source_name_taken");
    assert_eq!(
        taken.to_string(),
        format!(
            "`{second}` is the name of the source at {} already: rename this one to another",
            sources[1].origin()
        )
    );
    assert_eq!(ledger.sources().unwrap(), sources, "nothing renamed");
}

#[test]
fn renaming_what_no_source_is_called_is_refused() {
    let (_dir, workspace, ledger, sources) = registered(&[UrlMother::https()]);

    let error = rename(&workspace, &ledger, "nobody", "team").unwrap_err();

    assert!(matches!(&error, SourceError::NotFound { name } if name == "nobody"));
    assert_eq!(
        error.to_string(),
        "no source is called `nobody`: `mindweaver gather sources` lists them by name"
    );
    assert_eq!(ledger.sources().unwrap(), sources);
}

#[test]
fn a_source_error_echoes_the_codes_gather_gives_a_name() {
    let gather: Vec<&str> = GatherError::CODES.to_vec();

    assert_eq!(
        SourceError::ECHOED,
        ["source_name_invalid", "source_name_taken"]
    );
    assert!(SourceError::ECHOED.iter().all(|code| gather.contains(code)));
}
