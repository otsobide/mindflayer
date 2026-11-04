//! What the mothers draw from: numbers nobody chose, and the text built of
//! them.
//!
//! The numbers are the ones a stage's name is drawn by (`copy::random`):
//! `std` draws its hashers' keys from the operating system, so no generator
//! is kept as a dependency for tests alone. Not seeded, so no run of the
//! suite is the one before it; a test says what it was handed when it fails.

/// Lowercase ASCII letters.
pub const LOWERCASE: &str = "abcdefghijklmnopqrstuvwxyz";

/// Decimal digits.
pub const DIGITS: &str = "0123456789";

/// Lowercase hexadecimal digits, the one spelling a hash and an id are
/// written in.
pub const HEX: &str = "0123456789abcdef";

/// Characters from several scripts, spaces and punctuation among them: what
/// a person may type into a name, a summary or a message, for a value that
/// holds any text.
pub const ANY: &str =
    "abcXYZ019 -_./:@#?!'\"\\\t\u{e9}\u{df}\u{f1}\u{3b1}\u{416}\u{5d0}\u{4e2d}\u{1f980}";

/// A number nobody chose.
pub fn number() -> u64 {
    crate::copy::random()
}

/// A number from `low` to `high`, both included.
pub fn between(low: usize, high: usize) -> usize {
    assert!(low <= high, "{low} is above {high}");
    let span = (high - low) as u64 + 1;
    low + (number() % span) as usize
}

/// Heads or tails.
pub fn chance() -> bool {
    number() & 1 == 1
}

/// One of `items`.
pub fn pick<T: Clone>(items: &[T]) -> T {
    assert!(!items.is_empty(), "nothing to pick from");
    items[between(0, items.len() - 1)].clone()
}

/// Some of `items`, in the order they are given, possibly none.
pub fn some_of<T: Clone>(items: &[T]) -> Vec<T> {
    items.iter().filter(|_| chance()).cloned().collect()
}

/// `length` characters of `alphabet`.
pub fn drawn_from(alphabet: &str, length: usize) -> String {
    let characters: Vec<char> = alphabet.chars().collect();
    (0..length).map(|_| pick(&characters)).collect()
}

/// `length` lowercase hexadecimal digits.
pub fn hex(length: usize) -> String {
    drawn_from(HEX, length)
}

/// A lowercase word of 3 to 10 letters.
pub fn word() -> String {
    drawn_from(LOWERCASE, between(3, 10))
}

/// Lowercase words of letters and digits joined by single hyphens, as the
/// Agent Skills rule writes a name: never a hyphen at either end or two in a
/// row, and never longer than a segment may be.
pub fn kebab() -> String {
    let alphabet = format!("{LOWERCASE}{DIGITS}");
    (0..between(1, 3))
        .map(|_| drawn_from(&alphabet, between(1, 8)))
        .collect::<Vec<_>>()
        .join("-")
}

/// `length` bytes, any of them.
pub fn bytes(length: usize) -> Vec<u8> {
    (0..length).map(|_| number() as u8).collect()
}

/// Up to `most` characters of any script ([`ANY`]).
pub fn text(most: usize) -> String {
    drawn_from(ANY, between(0, most))
}

#[cfg(test)]
#[path = "random_test.rs"]
mod tests;
