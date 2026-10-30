//! Unit tests for `typescript.rs`: the export the panel's types come from,
//! and every type serde writes by hand held to what its TypeScript says.

use std::collections::BTreeSet;
use std::env;
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::Value;

use super::*;
use crate::agent::Agent;
use crate::artifact::ValidationIssue;
use crate::contents::Withheld;
use crate::install::Place;
use crate::kind::Kind;
use crate::ledger::SourceKind;
use crate::resolve::Reason;
use crate::scan;
use crate::settings::{ProjectId, Share};
use crate::status::State;
use crate::template::{Origin, Template};
use crate::url::redacted;
use crate::view::Health;
use crate::workspace::MarkerFile;

/// The words a union of them says, as [`union`] or [`lines`] wrote it.
fn literals(union: &str) -> BTreeSet<String> {
    union
        .split('|')
        .map(str::trim)
        .filter(|word| !word.is_empty())
        .map(|word| {
            word.strip_prefix('"')
                .and_then(|word| word.strip_suffix('"'))
                .unwrap_or_else(|| panic!("{word} is no string literal"))
                .to_owned()
        })
        .collect()
}

/// What serde writes of each of `values`, each a string.
fn written<T: Serialize>(values: &[T]) -> BTreeSet<String> {
    values
        .iter()
        .map(|value| match serde_json::to_value(value).unwrap() {
            Value::String(word) => word,
            other => panic!("{other} is no word"),
        })
        .collect()
}

/// That what serde writes of `values`, each variant of `T`, is every word
/// its TypeScript says and nothing else: a variant left out of `values`
/// shows as a word only the TypeScript says.
fn says_what_serde_writes<T: Serialize + TS>(values: &[T]) {
    assert_eq!(
        literals(&T::inline(&Config::new())),
        written(values),
        "{}",
        T::name(&Config::new())
    );
}

/// The export the panel's types come from, which `make web/types` runs with
/// the feature: every root and what it names, written where
/// `.cargo/config.toml` says.
///
/// Refused, rather than written into `./bindings`, ts-rs's default, when
/// nothing says where: a run of cargo from outside the repository reads no
/// `.cargo/config.toml`, and a folder left beside the crate is a stray one.
#[test]
fn export_bindings() {
    assert!(
        env::var_os("TS_RS_EXPORT_DIR").is_some(),
        "TS_RS_EXPORT_DIR is not set: run cargo from inside the repository, \
         whose .cargo/config.toml points it at the panel"
    );
    let mut export = Export::from_env();
    roots(&mut export);
    export.finish().unwrap();
}

#[test]
fn no_two_types_are_written_to_one_file() {
    let mut files = Files::default();
    roots(&mut files);

    assert_eq!(files.shared(), Vec::<String>::new());
}

#[test]
fn every_word_type_says_in_typescript_the_words_serde_writes() {
    says_what_serde_writes(&Kind::ALL);
    says_what_serde_writes(&Agent::ALL);
    says_what_serde_writes(&State::ALL);
    says_what_serde_writes(&Health::ALL);
    says_what_serde_writes(&Share::ALL);
    says_what_serde_writes(&[
        Reason::Explicit,
        Reason::Activated,
        Reason::Above,
        Reason::Home,
    ]);
    says_what_serde_writes(&[
        MarkerFile::MindToml,
        MarkerFile::SettingsJson,
        MarkerFile::SettingsToml,
    ]);
    says_what_serde_writes(&[SourceKind::Git]);
    says_what_serde_writes(&[
        scan::Reason::Id,
        scan::Reason::Mind,
        scan::Reason::Agents,
        scan::Reason::Git,
    ]);
    says_what_serde_writes(&[Withheld::Binary, Withheld::TooLarge]);
    says_what_serde_writes(&[Place::Workspace, Place::Loaded, Place::Shelf]);
}

#[test]
fn a_word_type_is_declared_as_the_union_of_its_words() {
    assert_eq!(
        Kind::decl(&Config::new()),
        r#"type Kind = "skill" | "rule";"#
    );
    assert_eq!(
        Agent::decl(&Config::new()),
        r#"type Agent = "claude" | "codex" | "opencode";"#
    );
}

/// A scan's reason under a name of its own, in a file of its own, apart
/// from the rule that chose a workspace, which is `Reason` in both.
#[test]
fn a_word_type_named_apart_is_declared_and_written_under_that_name() {
    assert_eq!(
        scan::Reason::decl(&Config::new()),
        r#"type ScanReason = "id" | "mind" | "agents" | "git";"#
    );
    assert_eq!(
        scan::Reason::output_path(),
        Some(PathBuf::from("ScanReason.ts"))
    );
    assert_eq!(Reason::output_path(), Some(PathBuf::from("Reason.ts")));
}

/// What the TypeScript is read off holds one issue of each code, so no
/// code is missing from the union and none is there twice.
#[test]
fn an_issue_of_every_code_is_what_its_typescript_is_read_off() {
    let codes: Vec<&str> = ValidationIssue::every()
        .iter()
        .map(ValidationIssue::code)
        .collect();

    assert_eq!(codes, ValidationIssue::CODES);
}

/// Each issue's JSON holds the keys its member of the union names, and no
/// other, its code as the literal that member is told apart by.
#[test]
fn every_issue_holds_what_its_member_of_the_union_names() {
    let union = ValidationIssue::inline(&Config::new());
    let members: Vec<&str> = union
        .split('\n')
        .filter_map(|line| line.trim().strip_prefix("| "))
        .collect();
    let issues = ValidationIssue::every();
    assert_eq!(members.len(), issues.len());

    for (issue, member) in issues.iter().zip(members) {
        let named: BTreeSet<&str> = member
            .trim_start_matches("{ ")
            .trim_end_matches(" }")
            .split(", ")
            .map(|field| field.split_once(": ").unwrap().0)
            .collect();
        let Value::Object(json) = serde_json::to_value(issue).unwrap() else {
            panic!("{issue:?} is no object");
        };
        let keys: BTreeSet<&str> = json.keys().map(String::as_str).collect();

        assert_eq!(keys, named, "{member}");
        assert!(
            member.starts_with(&format!("{{ code: {:?},", issue.code())),
            "{member}"
        );
    }
}

#[test]
fn an_issue_says_what_it_carries_with_its_type() {
    let union = ValidationIssue::inline(&Config::new());

    assert!(union.contains(
        r#"| { code: "name_segment_too_long", segment: string, length: number, warning: boolean, message: string }"#
    ));
    assert!(union.contains(r#"| { code: "name_empty", warning: boolean, message: string }"#));
}

/// A template's origin is the folder or file it is, or none for one built
/// in, written by hand, which its field says as `string | null`.
#[test]
fn a_template_s_origin_is_a_string_or_null_in_both() {
    let template = |origin: Origin| Template {
        name: "full".to_owned(),
        kind: Kind::Skill,
        origin,
    };

    assert_eq!(
        serde_json::to_value(template(Origin::BuiltIn)).unwrap()["origin"],
        Value::Null
    );
    assert_eq!(
        serde_json::to_value(template(Origin::Path(PathBuf::from("/work/full")))).unwrap()
            ["origin"],
        Value::String("/work/full".to_owned())
    );
    assert!(Template::decl(&Config::new()).contains("origin: string | null"));
}

/// A skill's metadata is a list of pairs in Rust, kept in the order
/// written, and an object in JSON, which its field says.
#[test]
fn a_skill_s_metadata_is_an_object_in_both() {
    let shown = Shown {
        kind: Kind::Skill,
        name: "deploy".to_owned(),
        path: PathBuf::from("/work/skills/deploy"),
        project: PathBuf::from("/work"),
        file: PathBuf::from("/work/skills/deploy/SKILL.md"),
        summary: None,
        description: None,
        allowed_tools: None,
        license: None,
        compatibility: None,
        metadata: Some(vec![("team".to_owned(), "infra".to_owned())]),
        paths: None,
        body: String::new(),
    };

    assert_eq!(
        serde_json::to_value(&shown).unwrap()["metadata"],
        serde_json::json!({"team": "infra"})
    );
    assert!(Shown::decl(&Config::new()).contains("metadata: { [key in string]: string } | null"));
}

/// Each function a field is written through, on a field of the type it
/// takes, as every field it writes holds: ts-rs reads that type in place of
/// the attribute, so what it says of the type is what the panel is told.
/// `in_order`, which writes a skill's metadata, is `reading`'s own, and its
/// field says what it writes with `ts(as)`, held by the test of the metadata.
#[derive(Serialize, TS)]
struct Written {
    #[serde(serialize_with = "crate::paths::lossy")]
    path: PathBuf,
    #[serde(serialize_with = "crate::paths::lossy_all")]
    paths: Vec<PathBuf>,
    #[serde(serialize_with = "crate::url::serialize_redacted")]
    url: String,
    #[serde(serialize_with = "crate::url::serialize_redacted_option")]
    detail: Option<String>,
    #[serde(serialize_with = "crate::url::serialize_redacted_option")]
    none: Option<String>,
}

/// Every function a field of core's names, each held to its TypeScript by
/// the test of [`Written`]: one new joins it there first, or a field written
/// as something its type is not would leave the panel told the type.
const FIELD_SERIALIZERS: [&str; 5] = [
    "in_order",
    "paths::lossy",
    "paths::lossy_all",
    "url::serialize_redacted",
    "url::serialize_redacted_option",
];

/// A path whose bytes are no UTF-8, where a path can be one: `lossy` writes
/// it as text all the same, which is why its TypeScript is a `string`.
fn unprintable_path() -> PathBuf {
    #[cfg(unix)]
    {
        use std::ffi::OsStr;
        use std::os::unix::ffi::OsStrExt;

        PathBuf::from(OsStr::from_bytes(b"/work/skills/\xffdeploy"))
    }
    #[cfg(not(unix))]
    {
        PathBuf::from("/work/skills/deploy")
    }
}

#[test]
fn every_field_serializer_writes_what_typescript_says_of_its_field() {
    let url = "https://user:secret@example.com/skills.git";
    let detail = format!("cloned {url}");
    let path = unprintable_path();
    let written = Written {
        path: path.clone(),
        paths: vec![path.clone(), PathBuf::from("/work/rules")],
        url: url.to_owned(),
        detail: Some(detail.clone()),
        none: None,
    };

    assert_eq!(
        serde_json::to_value(&written).unwrap(),
        serde_json::json!({
            "path": path.to_string_lossy(),
            "paths": [path.to_string_lossy(), "/work/rules"],
            "url": redacted(url),
            "detail": redacted(&detail),
            "none": null,
        })
    );
    assert_eq!(
        Written::inline(&Config::new()),
        "{ path: string, paths: Array<string>, url: string, detail: string | null, none: string | null, }"
    );
}

#[test]
fn every_field_serializer_in_core_is_one_held_to_its_typescript() {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let named = field_serializers(&source).unwrap();

    assert_eq!(
        named,
        FIELD_SERIALIZERS.map(str::to_owned).into(),
        "a field written through a function the test of `Written` does not hold"
    );
}

/// What finds them, which a test that found none would pass unseen: a
/// `deserialize_with` names no serializer, and `with` names a module.
#[test]
fn field_serializers_are_read_off_every_attribute_that_names_one() {
    let folder = tempfile::tempdir().unwrap();
    std::fs::create_dir(folder.path().join("nested")).unwrap();
    std::fs::write(
        folder.path().join("nested/one.rs"),
        "#[serde(default, deserialize_with = \"read\")]\n\
         #[serde(serialize_with = \"crate::paths::lossy\")]\n\
         #[serde(\n    with = \"humantime\"\n)]\n",
    )
    .unwrap();
    std::fs::write(
        folder.path().join("one_test.rs"),
        "#[serde(serialize_with = \"only_a_test\")]\n",
    )
    .unwrap();

    assert_eq!(
        field_serializers(folder.path()).unwrap(),
        BTreeSet::from(["humantime".to_owned(), "paths::lossy".to_owned()])
    );
}

#[test]
fn a_project_id_is_the_string_it_holds_in_both() {
    let id = ProjectId::parse("0123456789abcdef0123456789abcdef").unwrap();

    assert_eq!(
        serde_json::to_value(&id).unwrap(),
        Value::String(id.as_str().to_owned())
    );
    assert_eq!(ProjectId::decl(&Config::new()), "type ProjectId = string;");
}

#[test]
fn every_code_core_gives_is_in_its_union_once() {
    let given: Vec<&str> = code::ALL
        .iter()
        .flat_map(|(_, codes)| codes.iter().copied())
        .collect();
    let said = literals(&CoreCode::inline(&Config::new()));

    assert_eq!(said.len(), given.len());
    assert!(given.iter().all(|code| said.contains(*code)));
}
