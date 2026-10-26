//! The secret a launch is opened with.

use std::fmt;

/// How many random bytes a token holds: 256 bits, past guessing by anything
/// that can reach a loopback port.
const BYTES: usize = 32;

/// A secret issued once per launch. Whoever presents it is whoever the
/// server's link was printed for; nothing else about a request says so,
/// because every other process on the machine can reach 127.0.0.1 too.
#[derive(Clone)]
pub(crate) struct Token(String);

impl Token {
    /// A fresh one, from the thread's generator, which is a CSPRNG seeded
    /// from the operating system. Hex rather than base64 so that it needs no
    /// escaping as a query parameter, a cookie value or a header.
    pub(crate) fn issue() -> Self {
        let mut bytes = [0u8; BYTES];
        rand::fill(&mut bytes[..]);
        Self(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }

    /// Whether `offered` is this token, in time that does not depend on
    /// where the two first differ. An ordinary comparison returns at the
    /// first wrong byte, which lets a patient caller learn the token a byte
    /// at a time from how long each refusal took.
    pub(crate) fn matches(&self, offered: &str) -> bool {
        let (ours, theirs) = (self.0.as_bytes(), offered.as_bytes());
        ours.len() == theirs.len()
            && ours
                .iter()
                .zip(theirs)
                .fold(0u8, |difference, (a, b)| difference | (a ^ b))
                == 0
    }
}

/// Never the secret itself, so a stray `{:?}` cannot put it in a log.
impl fmt::Debug for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Token(<redacted>)")
    }
}

#[cfg(test)]
#[path = "token_test.rs"]
mod tests;
