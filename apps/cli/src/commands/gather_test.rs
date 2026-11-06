//! Unit tests for `gather.rs`: what a marketplace's report says, on values
//! no repository on the disk can make, since a plugin in a repository of
//! its own is fetched from another machine.

use mindweaver_core::marketplace::{Child, Dialect, Entry};
use mindweaver_core::value::{Redacted, SourceName};

use super::*;

#[test]
fn a_plugins_url_its_catalog_gives_is_said_in_printable_text() {
    let entry = Entry {
        name: String::from("tools"),
        description: None,
        plugin: Plugin::Url {
            url: String::from("https://git.example.com/acme/\u{1b}[2Jtools.git"),
            reference: None,
            sha: None,
        },
        skills: None,
        license: None,
        declares: Vec::new(),
    };
    let report = marketplace::Report {
        catalog: Listed {
            name: SourceName::parse("m").unwrap(),
            url: Redacted::new("https://github.com/acme/market"),
            reference: None,
            revision: None,
            listed: 1,
            entries: vec![entry],
            failures: Vec::new(),
            dialect: Dialect::Claude,
        },
        children: vec![Child {
            entry: String::from("tools"),
            outcome: Ok(Report {
                revision: None,
                directory: String::from("github.com-acme-market-tools"),
                added: Vec::new(),
                updated: Vec::new(),
                unchanged: Vec::new(),
                failures: Vec::new(),
                warnings: Vec::new(),
                name: SourceName::parse("m.tools").unwrap(),
                removed: Vec::new(),
            }),
            holds: Vec::new(),
            mcp_files: Vec::new(),
        }],
    };

    let said = marketplace_gathered(&report, &Pick::Plugins(vec![String::from("tools")]));

    assert!(
        said.stdout
            .contains("m.tools: https://git.example.com/acme/ [2Jtools.git\n"),
        "{}",
        said.stdout
    );
    assert!(
        !said
            .stdout
            .chars()
            .any(|character| character.is_control() && character != '\n'),
        "{:?}",
        said.stdout
    );
}
