//! What a type core hands a front end says of itself, where it is written
//! by hand: what both descriptions of core's types read, the panel's
//! TypeScript (`typescript`, under the `ts` feature) and the API's JSON
//! Schema (`schema`, under the `schema` feature), so the two say the
//! very same words and the very same fields.
//!
//! - **A word**, a kind, an agent or a state, from the function its
//!   serializer calls on every variant ([`words!`]), in both.
//! - **A field written through a function of its own** (`serialize_with`),
//!   which no derive can read, found in the source for a test to refuse one
//!   it does not hold ([`field_serializers`]).

use std::collections::BTreeSet;
use std::fs;
use std::io;
use std::path::Path;

/// Give a type serde writes as one of its words its TypeScript, under `ts`,
/// and its JSON Schema, under `schema`: the union of those words, each from
/// the function its serializer calls, on every variant (`"skill" | "rule"`
/// in TypeScript, `{"type": "string", "enum": ["skill", "rule"]}` as a
/// schema), with the documentation given.
///
/// Every variant is named, and matched with no wildcard, so one added does
/// not compile until it is in the union: a list kept beside the enum, such
/// as `Kind::ALL`, is no promise that it holds them all. From the function
/// the serializer calls rather than derived, with each variant renamed to
/// its word, for the reason the serializer is written by hand: a derive
/// spells the variant, which a rename would change. One macro for both, so
/// a word is listed once and the two descriptions cannot differ on it.
///
/// `as "Name",` before `by` declares it under a name of its own, for a type
/// whose own name another module's type already has: the panel's types are
/// written into one folder, a file to a name, so the second would be
/// written over the first, and a schema names each type it holds once, so
/// the second would be given a number. Its schema's id is its Rust path
/// all the same, so two types of one name are never taken for one.
macro_rules! words {
    ($(#[doc = $doc:literal])* $type:ident by $word:ident { $($variant:ident),+ $(,)? }) => {
        $crate::described::words! {
            $(#[doc = $doc])* $type as stringify!($type), by $word { $($variant),+ }
        }
    };
    ($(#[doc = $doc:literal])* $type:ident as $name:expr, by $word:ident { $($variant:ident),+ $(,)? }) => {
        #[cfg(feature = "ts")]
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

        #[cfg(feature = "schema")]
        impl ::schemars::JsonSchema for $type {
            fn schema_name() -> ::std::borrow::Cow<'static, str> {
                $name.into()
            }

            fn schema_id() -> ::std::borrow::Cow<'static, str> {
                concat!(module_path!(), "::", stringify!($type)).into()
            }

            fn json_schema(_: &mut ::schemars::SchemaGenerator) -> ::schemars::Schema {
                fn _every(value: $type) {
                    match value {
                        $($type::$variant)|+ => {}
                    }
                }
                $crate::schema::words(&[$($doc),*], [$($type::$variant.$word()),+])
            }
        }
    };
}

pub(crate) use words;

/// Every function a field in the `.rs` files under `folder` is written
/// through, as its attribute names it, `crate::` left off: `serialize_with`,
/// or `with`, whose module's `serialize` writes it. Unit tests are left out,
/// whose types no root names.
///
/// For a test to refuse one it does not hold to its TypeScript and its
/// schema: neither ts-rs nor schemars can read either attribute, and each
/// says of such a field what it says of its type, so a field written as
/// something its type is not would pass both unseen. Read off the source,
/// since no trait says which fields serde writes some other way;
/// `deserialize_with` is passed over, as what a field reads says nothing of
/// what it writes, and so is a `with` of schemars', which names the type a
/// field's schema is read off rather than a function.
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
                if attribute(&text[..at]) != Some("serde") {
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

/// The attribute `before`, the source up to a key, ends inside: the name
/// before the last opening parenthesis, `serde` in `#[serde(default, ` and
/// `schemars` in `#[cfg_attr(feature = "schema", schemars(`.
fn attribute(before: &str) -> Option<&str> {
    let opening = before.rfind('(')?;
    let name = before[..opening]
        .rsplit(|c: char| !(c.is_alphanumeric() || c == '_'))
        .next()?;
    (!name.is_empty()).then_some(name)
}
