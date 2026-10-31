//! What core hands a front end, in TypeScript, for the panel to compile
//! against (#70): what the `ts` feature adds, and nothing else builds.
//!
//! ts-rs derives most of it, beside each type's serde derive and from the
//! same attributes, so a derived type says in TypeScript what serde writes.
//! What serde writes by hand ts-rs cannot read, and it would spell the Rust
//! variants that a hand-written serializer exists not to, so each such type
//! is given its TypeScript from what its serializer writes, beside it:
//!
//! - **A word**, a kind, an agent or a state: the union of the words, from
//!   the function its serializer calls ([`words!`]).
//! - **A path or a URL**, written as text whatever it holds: a `string`,
//!   which is what ts-rs says of a `PathBuf` and a `String` already. Each
//!   field written through a function of its own (`serialize_with`) is one
//!   ts-rs reads as its type, so a test holds each such function to what
//!   ts-rs says of the type it takes, and [`field_serializers`] finds every
//!   one a field names, for a test to refuse one it does not hold.
//! - **Anything else**, an issue `validate` finds, a template's origin, a
//!   skill's metadata: from what its serializer writes, beside it, each
//!   held to its serializer by a test.
//!
//! [`roots`] names what is exported, and every type each root names goes
//! with it, into `TS_RS_EXPORT_DIR`, which `.cargo/config.toml` points at
//! the panel. One list rather than `#[ts(export)]` on each root, so the
//! test that finds two types written to one file walks what the export
//! writes, and no other list.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use ts_rs::{Config, ExportError, TypeVisitor, TS};

use crate::code;
use crate::contents::{self, Contents};
use crate::draft::Judged;
use crate::reading::{Shown, Validation};
use crate::update::Updates;
use crate::view::{Listing, Snapshot};

/// Every type core hands a front end whole, each with every type it names:
/// the snapshot, an artifact's files and a file's contents, and what an
/// update of every project would do, which the panel queries; what `list`,
/// `show` and `validate` print, which a script reads; and every code core
/// gives, which a front end compares a code with.
pub fn roots(visitor: &mut impl TypeVisitor) {
    visitor.visit::<Snapshot>();
    visitor.visit::<Updates>();
    visitor.visit::<Listing>();
    visitor.visit::<Shown>();
    visitor.visit::<Validation>();
    visitor.visit::<contents::Files>();
    visitor.visit::<Contents>();
    visitor.visit::<Judged>();
    visitor.visit::<CoreCode>();
}

/// `words` as the union TypeScript reads one of them as: `"skill" |
/// "rule"`.
pub fn union<'a>(words: impl IntoIterator<Item = &'a str>) -> String {
    words
        .into_iter()
        .map(|word| format!("{word:?}"))
        .collect::<Vec<_>>()
        .join(" | ")
}

/// `members` as a union laid out one to a line, each its own line in a diff:
/// for a union too long to read on one.
pub fn lines(members: impl IntoIterator<Item = String>) -> String {
    members
        .into_iter()
        .map(|member| format!("\n  | {member}"))
        .collect()
}

/// `text`, a line at a time as a doc comment holds it, as the comment ts-rs
/// writes a derived type's documentation as: for a type whose TypeScript is
/// written by hand.
pub fn docs(text: &[&str]) -> String {
    let mut docs = String::from("/**\n");
    for line in text {
        docs.push_str(" *");
        docs.push_str(line);
        docs.push('\n');
    }
    docs.push_str(" */\n");
    docs
}

/// Give a type serde writes as one of its words its TypeScript: the union of
/// those words, each from the function its serializer calls, on every
/// variant (`"skill" | "rule"`), with the documentation given.
///
/// Every variant is named, and matched with no wildcard, so one added does
/// not compile until it is in the union: a list kept beside the enum, such
/// as `Kind::ALL`, is no promise that it holds them all. From the function
/// the serializer calls rather than derived, with each variant renamed to
/// its word, for the reason the serializer is written by hand: a derive
/// spells the variant, which a rename would change.
///
/// `as "Name",` before `by` declares it under a name of its own, for a type
/// whose own name another module's type already has in TypeScript: both
/// are written into one folder, a file to a name, so the second would be
/// written over the first.
macro_rules! words {
    ($(#[doc = $doc:literal])* $type:ident by $word:ident { $($variant:ident),+ $(,)? }) => {
        $crate::typescript::words! {
            $(#[doc = $doc])* $type as stringify!($type), by $word { $($variant),+ }
        }
    };
    ($(#[doc = $doc:literal])* $type:ident as $name:expr, by $word:ident { $($variant:ident),+ $(,)? }) => {
        impl ::ts_rs::TS for $type {
            type WithoutGenerics = Self;
            type OptionInnerType = Self;

            fn docs() -> Option<String> {
                Some($crate::typescript::docs(&[$($doc),*]))
            }

            fn name(_: &::ts_rs::Config) -> String {
                $name.to_owned()
            }

            fn decl(config: &::ts_rs::Config) -> String {
                format!("type {} = {};", $name, Self::inline(config))
            }

            fn decl_concrete(config: &::ts_rs::Config) -> String {
                Self::decl(config)
            }

            fn inline(_: &::ts_rs::Config) -> String {
                fn _every(value: $type) {
                    match value {
                        $($type::$variant)|+ => {}
                    }
                }
                $crate::typescript::union([$($type::$variant.$word()),+])
            }

            fn output_path() -> Option<::std::path::PathBuf> {
                Some(format!("{}.ts", $name).into())
            }
        }
    };
}

pub(crate) use words;

/// Every code core gives ([`code::ALL`]), as the union of them: what a
/// front end compares the code of what went wrong with, so that a code the
/// panel names is one core gives.
///
/// Not the type of a `Said`'s `code`, which stays a `string`: what a front
/// end hands on holds its own codes beside core's, the CLI's among them.
pub struct CoreCode;

impl TS for CoreCode {
    type WithoutGenerics = Self;
    type OptionInnerType = Self;

    fn docs() -> Option<String> {
        Some(docs(&[
            " Every code core gives, for what went wrong: unique across core, and",
            " never renamed once given.",
        ]))
    }

    fn name(_: &Config) -> String {
        "CoreCode".to_owned()
    }

    fn decl(config: &Config) -> String {
        format!("type CoreCode ={};", Self::inline(config))
    }

    fn decl_concrete(config: &Config) -> String {
        Self::decl(config)
    }

    fn inline(_: &Config) -> String {
        lines(
            code::ALL
                .iter()
                .flat_map(|(_, codes)| codes.iter())
                .map(|code| format!("{code:?}")),
        )
    }

    fn output_path() -> Option<PathBuf> {
        Some("CoreCode.ts".into())
    }
}

/// What an export test hands [`roots`]: it writes each type it visits, with
/// every type that one names, where and as the environment says
/// (`TS_RS_EXPORT_DIR`, `TS_RS_LARGE_INT`).
pub struct Export {
    config: Config,
    failed: Option<ExportError>,
}

impl Export {
    /// An export as `.cargo/config.toml` sets it up.
    pub fn from_env() -> Self {
        Export {
            config: Config::from_env(),
            failed: None,
        }
    }

    /// What went wrong, if anything did: the first failure, after which it
    /// writes nothing more.
    pub fn finish(self) -> Result<(), ExportError> {
        self.failed.map_or(Ok(()), Err)
    }
}

impl TypeVisitor for Export {
    fn visit<T: TS + 'static + ?Sized>(&mut self) {
        if self.failed.is_none() {
            self.failed = T::export_all(&self.config).err();
        }
    }
}

/// The file each type it visits is written to, with every type it names:
/// for a test to find two types written to one, which the export would
/// either merge into one file, declaring a name twice when the two share
/// it, or write one over the other, a crate's tests after another's.
#[derive(Default)]
pub struct Files {
    /// Each file, and the Rust types written to it, by their paths without
    /// their generics: a generic type is one file whatever it is given.
    written: BTreeMap<PathBuf, Vec<String>>,
}

impl Files {
    /// Each file more than one type is written to, with those types.
    pub fn shared(&self) -> Vec<String> {
        self.written
            .iter()
            .filter(|(_, types)| types.len() > 1)
            .map(|(file, types)| format!("{}: {}", file.display(), types.join(", ")))
            .collect()
    }
}

/// Following what each type names, as the export does.
impl TypeVisitor for Files {
    fn visit<T: TS + 'static + ?Sized>(&mut self) {
        let Some(file) = T::output_path() else {
            return;
        };
        let name = std::any::type_name::<T>();
        let name = name.split_once('<').map_or(name, |(name, _)| name);
        let types = self.written.entry(file).or_default();
        if types.iter().any(|seen| seen == name) {
            return;
        }
        types.push(name.to_owned());
        T::visit_dependencies(self);
    }
}

/// Every function a field in the `.rs` files under `folder` is written
/// through, as its attribute names it, `crate::` left off: `serialize_with`,
/// or `with`, whose module's `serialize` writes it. Unit tests are left out,
/// whose types no root names.
///
/// For a test to refuse one it does not hold to its TypeScript: ts-rs cannot
/// read either attribute, and says of such a field what it says of its type,
/// with no warning (`no-serde-warnings`), so a field written as something
/// its type is not would pass the export unseen. Read off the source, since
/// no trait says which fields serde writes some other way; `deserialize_with`
/// is passed over, as what a field reads says nothing of what it writes.
pub fn field_serializers(folder: &Path) -> io::Result<BTreeSet<String>> {
    let mut named = BTreeSet::new();
    let mut entries = fs::read_dir(folder)?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<io::Result<Vec<_>>>()?;
    entries.sort();
    for path in entries {
        if path.is_dir() {
            named.extend(field_serializers(&path)?);
            continue;
        }
        let name = path.file_name().and_then(|name| name.to_str());
        if !name.is_some_and(|name| name.ends_with(".rs") && !name.ends_with("_test.rs")) {
            continue;
        }
        let text = fs::read_to_string(&path)?;
        for key in ["serialize_with", "with"] {
            let opening = format!("{key} = \"");
            for (at, _) in text.match_indices(&opening) {
                // `deserialize_with` holds `serialize_with`, and both hold
                // `with`: only the attribute's own name counts.
                let before = text[..at].chars().next_back();
                if before.is_some_and(|c| c.is_alphanumeric() || c == '_') {
                    continue;
                }
                let rest = &text[at + opening.len()..];
                let function = rest.split('"').next().unwrap_or_default();
                named.insert(
                    function
                        .strip_prefix("crate::")
                        .unwrap_or(function)
                        .to_owned(),
                );
            }
        }
    }
    Ok(named)
}

#[cfg(test)]
#[path = "typescript_test.rs"]
mod tests;
