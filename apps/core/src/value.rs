//! The values the domain is made of: value objects, of which every record,
//! report and view core builds is an aggregate.
//!
//! A value object holds what it is and nothing else: two are the same exactly
//! when they hold the same, none changes once made, and each is made only the
//! way its rule allows. The alternative, a `String` for each of them, was what
//! core had, and it said nothing of what it held: a hash could be handed where
//! a name was meant, and a URL still carrying a token stored where only one
//! without may go, with nothing but a reviewer to notice. A type says it once,
//! here, and the compiler holds every caller to it.
//!
//! They come in two sorts:
//!
//! - **Held to a rule**: [`ContentHash`], [`ProjectId`] and [`Redacted`],
//!   made only through the rule (the recipe, the generator or the parser, the
//!   redaction), never from any text at all. A function that is handed one
//!   need not check it again.
//! - **Named apart**: [`ArtifactName`], [`Origin`] and [`Revision`], which
//!   hold any text, since what core reads it reads leniently (a name on the
//!   disk can break the rules a write is held to, and is listed all the same:
//!   "strict when writing, lenient when reading"), but each a type of its own
//!   so that one is never handed where another is meant.
//!
//! Each goes out as the text it holds, and its contract is that text: serde
//! writes a string, ts-rs says `string` (so no file of its own is exported
//! for the panel) and schemars says what it says of a `String`. A field turned from a `String` into one of these changes nothing
//! the JSON, the panel's types or the API's schemas say, which `make
//! web/types/check` and `make api/schemas/check` hold. [`ProjectId`] is the
//! one exception, kept as it was published: a type of its own in TypeScript
//! and a pattern in its schema.
//!
//! None of them knows where it is stored. The ledger's adapter writes and
//! reads each as text, rather than each implementing a SQLite trait, so that
//! the domain depends on no adapter ([`crate::port`]).

use std::borrow::{Borrow, Cow};
use std::ffi::OsStr;
use std::fmt;
use std::ops::Deref;
use std::path::Path;

use serde::{Serialize, Serializer};

use crate::url::redacted;

/// Declares a value object that holds one `String`, with everything that
/// reads it as text: `Deref` to `str`, comparisons with text either way
/// round, `Display` and `Debug` as the text (so a report printed with `{:?}`
/// reads as it did when the field was a `String`), and the TypeScript and
/// JSON Schema of a string. What makes one, and how it is serialized, each
/// type says for itself.
///
/// `Deref` and no `DerefMut`: a value is read as the text it holds, and is
/// never changed in place.
macro_rules! text_value {
    ($(#[$meta:meta])* pub struct $name:ident;) => {
        $(#[$meta])*
        #[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(String);

        /// Said as a `string`, the way ts-rs says a `String`, rather than
        /// derived: a derive, even with `ts(as)`, names the type and exports
        /// a file of its own, which every field holding one would import,
        /// where a value object adds nothing the panel is told.
        #[cfg(feature = "ts")]
        impl ts_rs::TS for $name {
            type WithoutGenerics = Self;
            type OptionInnerType = Self;

            fn name(_: &ts_rs::Config) -> String {
                String::from("string")
            }

            fn inline(config: &ts_rs::Config) -> String {
                <Self as ts_rs::TS>::name(config)
            }
        }

        impl $name {
            /// The text it holds.
            pub fn as_str(&self) -> &str {
                &self.0
            }

            /// The text it holds, given up.
            pub fn into_string(self) -> String {
                self.0
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                fmt::Debug::fmt(&self.0, f)
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl Deref for $name {
            type Target = str;

            fn deref(&self) -> &str {
                &self.0
            }
        }

        impl AsRef<str> for $name {
            fn as_ref(&self) -> &str {
                &self.0
            }
        }

        impl AsRef<OsStr> for $name {
            fn as_ref(&self) -> &OsStr {
                OsStr::new(&self.0)
            }
        }

        impl AsRef<Path> for $name {
            fn as_ref(&self) -> &Path {
                Path::new(&self.0)
            }
        }

        impl Borrow<str> for $name {
            fn borrow(&self) -> &str {
                &self.0
            }
        }

        impl PartialEq<str> for $name {
            fn eq(&self, other: &str) -> bool {
                self.0 == other
            }
        }

        impl PartialEq<&str> for $name {
            fn eq(&self, other: &&str) -> bool {
                self.0 == *other
            }
        }

        impl PartialEq<String> for $name {
            fn eq(&self, other: &String) -> bool {
                &self.0 == other
            }
        }

        impl PartialEq<&$name> for $name {
            fn eq(&self, other: &&$name) -> bool {
                self.0 == other.0
            }
        }

        impl PartialEq<$name> for &$name {
            fn eq(&self, other: &$name) -> bool {
                self.0 == other.0
            }
        }

        impl PartialEq<Cow<'_, str>> for $name {
            fn eq(&self, other: &Cow<'_, str>) -> bool {
                self.0 == **other
            }
        }

        impl PartialEq<$name> for str {
            fn eq(&self, other: &$name) -> bool {
                self == other.0
            }
        }

        impl PartialEq<$name> for &str {
            fn eq(&self, other: &$name) -> bool {
                *self == other.0
            }
        }

        impl PartialEq<$name> for String {
            fn eq(&self, other: &$name) -> bool {
                *self == other.0
            }
        }

        impl From<$name> for String {
            fn from(value: $name) -> String {
                value.0
            }
        }

        /// The schema of the text it holds, inline, as a `String` field
        /// had it: a value object adds no definition to the API's schemas.
        #[cfg(feature = "schema")]
        impl schemars::JsonSchema for $name {
            fn inline_schema() -> bool {
                true
            }

            fn schema_name() -> Cow<'static, str> {
                <String as schemars::JsonSchema>::schema_name()
            }

            fn schema_id() -> Cow<'static, str> {
                <String as schemars::JsonSchema>::schema_id()
            }

            fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
                <String as schemars::JsonSchema>::json_schema(generator)
            }
        }
    };
}

/// Lets any text make a value object of the second sort, named apart and
/// held to no rule.
macro_rules! from_text {
    ($name:ident) => {
        impl From<String> for $name {
            fn from(text: String) -> Self {
                $name(text)
            }
        }

        impl From<&str> for $name {
            fn from(text: &str) -> Self {
                $name(text.to_owned())
            }
        }

        impl From<&String> for $name {
            fn from(text: &String) -> Self {
                $name(text.clone())
            }
        }

        impl From<Cow<'_, str>> for $name {
            fn from(text: Cow<'_, str>) -> Self {
                $name(text.into_owned())
            }
        }

        /// Read as any string, since any text makes one: what a front end
        /// sends, a box's origin among it, is taken as it was sent.
        impl<'de> serde::Deserialize<'de> for $name {
            fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                String::deserialize(deserializer).map($name)
            }
        }
    };
}

/// Serializes a value object as the text it holds.
macro_rules! as_text {
    ($name:ident) => {
        impl Serialize for $name {
            fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                serializer.serialize_str(&self.0)
            }
        }
    };
}

text_value! {
    /// The SHA-256 of what a copy holds, as 64 lowercase hexadecimal digits:
    /// what the ledger keeps of what an install wrote, so a copy nobody
    /// touched can later be told from one somebody edited (D-04).
    ///
    /// Made by the recipe and nowhere else ([`crate::hash::tree`] for a
    /// folder, [`crate::hash::bytes`] for a file written whole), by
    /// [`ContentHash::parse`] from a digest somebody hands over, or as the
    /// ledger holds one. No `From<String>`: a name or a path handed where a
    /// hash is meant does not compile.
    pub struct ContentHash;
}
as_text!(ContentHash);

impl ContentHash {
    /// The number of hexadecimal digits a hash is written with.
    pub const LEN: usize = 64;

    /// What [`crate::hash`]'s recipes computed, already 64 lowercase digits.
    pub(crate) fn computed(digits: String) -> Self {
        debug_assert!(digits.len() == Self::LEN);
        ContentHash(digits)
    }

    /// A hash as the ledger holds it, taken as it is written there rather
    /// than judged again: every hash the ledger holds an install wrote, and
    /// a row somebody edited by hand into something else reads, as it always
    /// has, as a hash no copy holds, so the copy reads as edited.
    pub(crate) fn recorded(text: String) -> Self {
        ContentHash(text)
    }

    /// The hash `text` spells, when it is one: 64 hexadecimal digits and
    /// nothing else, in either case, since a hash copied from elsewhere may
    /// be written in capitals and means the same. Kept in lowercase, the one
    /// spelling the recipes write, so two spellings of one hash are equal.
    ///
    /// For a hash somebody hands over to be compared with one of these, as
    /// `edit --if-unchanged` does: one that is no hash at all, such as the
    /// line `sha256sum` prints with the file's name after its digits, is
    /// told apart from the hash of something else, which no comparison
    /// could do.
    pub fn parse(text: &str) -> Option<Self> {
        let digits = text.len() == Self::LEN && text.bytes().all(|byte| byte.is_ascii_hexdigit());
        digits.then(|| ContentHash(text.to_ascii_lowercase()))
    }
}

text_value! {
    /// A text, a URL or words that may quote one, with every credential a
    /// URL in it carried taken out ([`crate::url::redacted`]).
    ///
    /// Made only through [`Redacted::new`], which redacts whatever it is
    /// handed, so a value of this type cannot hold a credential: what was a
    /// rule every writer of a URL had to remember ("stored, logged and shown
    /// without credentials") is the type of the field. The ledger hands back
    /// its URLs and its log's words as these, and a row an older build wrote
    /// with a token in it is redacted as it is read, rather than only where
    /// it is written out.
    pub struct Redacted;
}
as_text!(Redacted);

impl Redacted {
    /// `text` without the credentials any URL in it carried.
    pub fn new(text: &str) -> Self {
        Redacted(redacted(text).into_owned())
    }
}

text_value! {
    /// Where an installed artifact came from, spelled as `--from` takes it:
    /// `workspace` for the workspace's own, `load:` and the entry of a
    /// workspace it loads, or a source's address with its branch after a `#`
    /// ([`crate::install::Offer::origin`]).
    ///
    /// Named apart, and held to no rule: it is compared with what a front
    /// end was given and with what the ledger recorded, both as written.
    /// Written out only through [`crate::url::redacted`], whatever built
    /// it, since a source's address is part of it: what the ledger hands
    /// back carries no credential already, and one a front end built before
    /// a gather refused it must not print one either.
    pub struct Origin;
}
from_text!(Origin);

impl Serialize for Origin {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&redacted(&self.0))
    }
}

text_value! {
    /// The name an artifact is asked for by: a skill's folder name, or a
    /// rule's route below its kind's folder.
    ///
    /// Named apart and held to no rule, since a name read from the disk can
    /// break the rules a write is held to and is listed all the same. Which
    /// names may be written is each kind's (`artifact::strict_issues`,
    /// `install::is_folder_name`, `install::is_installable_name`).
    pub struct ArtifactName;
}
from_text!(ArtifactName);
as_text!(ArtifactName);

text_value! {
    /// The commit a source was gathered at, as the remote's repository
    /// names it: what a gather resolved, kept as it said it.
    pub struct Revision;
}
from_text!(Revision);
as_text!(Revision);

/// A project's id: 128 random bits, written as 32 lowercase hexadecimal
/// digits.
///
/// What names a project when its name and its path are no use: both change,
/// a name by hand and a path when the repository moves, and several
/// workspaces may know one project by different routes. Random rather than
/// derived from either, so that a copy of a project made with `cp -r` is the
/// only way two projects share one.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
// Derived, though serde's is written by hand: ts-rs says a struct around
// one value as that value, the `string` serde writes.
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ProjectId(String);

impl ProjectId {
    /// The number of hexadecimal digits an id is written with.
    pub const LEN: usize = 32;

    /// A new id, drawn from the operating system through `std`'s hasher keys
    /// as a stage's name is (`copy::random`), rather than from a dependency
    /// kept for it.
    pub fn generate() -> Self {
        ProjectId(format!(
            "{:016x}{:016x}",
            crate::copy::random(),
            crate::copy::random()
        ))
    }

    /// The id as written, when `text` is one: exactly [`LEN`](Self::LEN)
    /// lowercase hexadecimal digits, one spelling for one id.
    pub fn parse(text: &str) -> Option<Self> {
        let digits = text.len() == Self::LEN
            && text
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte));
        digits.then(|| ProjectId(text.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// As its text, the one spelling settings hold it in and an exclude group
/// names it by, rather than as a struct around it.
impl Serialize for ProjectId {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

/// As the text serde writes it, which only an id's own spelling matches:
/// written by hand, as the serializer is, rather than derived, which says
/// any string.
#[cfg(feature = "schema")]
impl schemars::JsonSchema for ProjectId {
    fn schema_name() -> Cow<'static, str> {
        "ProjectId".into()
    }

    fn schema_id() -> Cow<'static, str> {
        // Where it was first declared, which is only ever compared with
        // itself: kept as it was, rather than moved with the type.
        "mindweaver_core::settings::ProjectId".into()
    }

    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        crate::schema::schema(serde_json::json!({
            "description": "A project's id: 128 random bits, written as 32 lowercase \
                            hexadecimal digits.",
            "type": "string",
            "pattern": format!("^[0-9a-f]{{{}}}$", ProjectId::LEN),
        }))
    }
}

impl fmt::Display for ProjectId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
#[path = "value_test.rs"]
mod tests;
