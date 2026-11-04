//! The server's own mothers: what a request says of where it is going and
//! who sent it, for the guards to be handed both sides of every rule.
//!
//! Of its own rather than core's (`mindflayer_core::mother`), since the
//! server depends on no mindflayer crate, its dev-dependencies included
//! (`crate_test.rs`). Random, as core's are, and drawn the way core draws:
//! from `std`'s hasher keys, with no generator kept for tests alone. A test
//! drawing from one names the value in what it asserts, so a failure says
//! what it was handed.

/// What the mothers draw from.
pub mod random {
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hasher};
    use std::sync::atomic::{AtomicU64, Ordering};

    /// A number nobody chose.
    pub fn number() -> u64 {
        static CALLS: AtomicU64 = AtomicU64::new(0);
        let mut hasher = RandomState::new().build_hasher();
        hasher.write_u64(CALLS.fetch_add(1, Ordering::Relaxed));
        hasher.finish()
    }

    /// A number from `low` to `high`, both included.
    pub fn between(low: usize, high: usize) -> usize {
        assert!(low <= high, "{low} is above {high}");
        low + (number() % ((high - low) as u64 + 1)) as usize
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

    /// `items` in an order nobody chose.
    pub fn shuffled<T>(mut items: Vec<T>) -> Vec<T> {
        for at in (1..items.len()).rev() {
            items.swap(at, between(0, at));
        }
        items
    }

    /// A lowercase word of 3 to 10 letters.
    pub fn word() -> String {
        (0..between(3, 10))
            .map(|_| char::from(b'a' + between(0, 25) as u8))
            .collect()
    }

    /// `text`, each ASCII letter in either case: the same name to a host, a
    /// scheme, or anything else that does not depend on case.
    pub fn cased(text: &str) -> String {
        text.chars()
            .map(|c| {
                if chance() {
                    c.to_ascii_uppercase()
                } else {
                    c.to_ascii_lowercase()
                }
            })
            .collect()
    }

    /// `length` lowercase hexadecimal digits.
    pub fn hex(length: usize) -> String {
        (0..length)
            .map(|_| char::from(b"0123456789abcdef"[between(0, 15)]))
            .collect()
    }
}

/// Makes ports.
pub struct PortMother;

impl PortMother {
    /// A port other than `port`: beside it, its digits with one more or one
    /// fewer, or anywhere in the range.
    pub fn other(port: u16) -> u16 {
        loop {
            let other = match random::between(0, 4) {
                0 => port.wrapping_add(1),
                1 => port.wrapping_sub(1),
                2 => port / 10,
                3 => port.saturating_mul(10),
                _ => random::between(1, usize::from(u16::MAX)) as u16,
            };
            if other != port {
                return other;
            }
        }
    }
}

/// The three names the server answers to on `port`, as it spells them.
pub fn names(port: u16) -> [String; 3] {
    [
        format!("127.0.0.1:{port}"),
        format!("localhost:{port}"),
        format!("[::1]:{port}"),
    ]
}

/// Makes Host headers.
pub struct HostMother;

impl HostMother {
    /// Every name the server answers to on `port`, each spelled in some
    /// case, as a DNS name is the same in any.
    pub fn answered(port: u16) -> Vec<String> {
        names(port).iter().map(|name| random::cased(name)).collect()
    }

    /// Hosts that are not the server's on `port`, each beside why, some
    /// spelled in another case, which makes none of them its.
    pub fn refused(port: u16) -> Vec<(&'static str, String)> {
        let (word, other) = (random::word(), PortMother::other(port));
        let mut refused = vec![
            (
                "a name rebound to loopback",
                format!("{word}.example:{port}"),
            ),
            (
                "a name of loopback's under another",
                format!("localhost.{word}.example:{port}"),
            ),
            ("a name under localhost", format!("{word}.localhost:{port}")),
            ("another port", format!("127.0.0.1:{other}")),
            ("another port of localhost", format!("localhost:{other}")),
            ("another port of [::1]", format!("[::1]:{other}")),
            ("no port", String::from("127.0.0.1")),
            ("no port of localhost", String::from("localhost")),
            ("no port of [::1]", String::from("[::1]")),
            ("an empty port", String::from("127.0.0.1:")),
            ("a port with a leading zero", format!("127.0.0.1:0{port}")),
            ("a port with a sign", format!("127.0.0.1:+{port}")),
            ("a user before the name", format!("{word}@127.0.0.1:{port}")),
            ("a trailing dot", format!("localhost.:{port}")),
            (
                "a trailing dot on the address",
                format!("127.0.0.1.:{port}"),
            ),
            ("another loopback address", format!("127.0.0.2:{port}")),
            ("the address spelled short", format!("127.1:{port}")),
            ("the address as one number", format!("2130706433:{port}")),
            ("the address in hexadecimal", format!("0x7f.0.0.1:{port}")),
            ("every address", format!("0.0.0.0:{port}")),
            ("[::1] written out", format!("[0:0:0:0:0:0:0:1]:{port}")),
            ("[::1] without its brackets", format!("::1:{port}")),
            (
                "loopback mapped into IPv6",
                format!("[::ffff:127.0.0.1]:{port}"),
            ),
            ("a scheme", format!("http://127.0.0.1:{port}")),
            ("a path", format!("127.0.0.1:{port}/")),
            (
                "the server's name before another",
                format!("127.0.0.1:{port}.{word}.example"),
            ),
            ("a space inside", format!("127.0.0.1 :{port}")),
            ("empty", String::new()),
        ];
        for (_, host) in refused.iter_mut() {
            if random::chance() {
                *host = random::cased(host);
            }
        }
        refused
    }
}

/// Makes Origin headers.
pub struct OriginMother;

impl OriginMother {
    /// The origin of a page the server served as `name`, spelled in some
    /// case, as neither a scheme nor a name depends on it.
    pub fn of(name: &str) -> String {
        random::cased(&format!("http://{name}"))
    }

    /// Origins other than that of the page the server served as `name`, one
    /// of its names on `port`, each beside why.
    pub fn foreign(name: &str, port: u16) -> Vec<(&'static str, String)> {
        let word = random::word();
        let another = names(port)
            .into_iter()
            .find(|other| other != name)
            .expect("three names");
        vec![
            ("another site", format!("https://{word}.example")),
            (
                "a page on another port",
                format!("http://127.0.0.1:{}", PortMother::other(port)),
            ),
            (
                "a page under another of its names",
                format!("http://{another}"),
            ),
            ("over https", format!("https://{name}")),
            ("over another scheme", format!("ws://{name}")),
            ("with a path", format!("http://{name}/")),
            ("with a user", format!("http://{word}@{name}")),
            ("with no scheme", name.to_owned()),
            ("a sandboxed frame or a file", String::from("null")),
            ("empty", String::new()),
        ]
    }
}

/// Makes tokens.
pub struct TokenMother;

impl TokenMother {
    /// One as a server issues one: 256 bits in hex. Never a running
    /// server's, but for a chance there is no point counting.
    pub fn random() -> String {
        random::hex(64)
    }

    /// Texts that are not `token`, each beside why: what a check that
    /// compared a prefix, a length or a case would let through.
    pub fn not(token: &str) -> Vec<(&'static str, String)> {
        let at = random::between(0, token.len() - 1);
        let flipped: String = token
            .char_indices()
            .map(|(index, c)| match (index == at, c) {
                (false, c) => c,
                (true, '0') => '1',
                (true, _) => '0',
            })
            .collect();
        let encoded: String = token
            .char_indices()
            .map(|(index, c)| {
                if index == at {
                    format!("%{:02X}", c as u32)
                } else {
                    c.to_string()
                }
            })
            .collect();
        let mut not = vec![
            ("one digit changed", flipped),
            ("one digit short", token[1..].to_owned()),
            ("one digit over", format!("{token}0")),
            ("twice", token.repeat(2)),
            ("in capitals", token.to_ascii_uppercase()),
            ("quoted", format!("\"{token}\"")),
            ("one digit percent-encoded", encoded),
            ("another server's", Self::random()),
            ("empty", String::new()),
        ];
        not.retain(|(_, text)| text != token);
        not
    }
}

/// Makes Cookie headers.
pub struct CookieMother;

impl CookieMother {
    /// The cookies a browser sends from other pages on loopback: any port's,
    /// none of them the token.
    pub fn others() -> Vec<String> {
        (0..random::between(0, 4))
            .map(|_| format!("{}={}", random::word(), random::hex(random::between(0, 12))))
            .collect()
    }

    /// One Cookie header holding `pair` among [`others`](Self::others), at
    /// any place, each pair after a `;` with spaces around it or none, as
    /// browsers and scripts write them.
    pub fn among(pair: &str) -> String {
        let mut pairs = Self::others();
        pairs.insert(random::between(0, pairs.len()), pair.to_owned());
        let mut header = String::new();
        for (at, pair) in pairs.iter().enumerate() {
            if at > 0 {
                header.push_str(random::pick(&[";", "; ", " ;", " ; "]));
            }
            header.push_str(pair);
        }
        header
    }
}

/// Makes Authorization headers.
pub struct BearerMother;

impl BearerMother {
    /// Those that bear `token` as the server takes it, each beside why.
    pub fn taken(token: &str) -> Vec<(&'static str, String)> {
        vec![
            ("as the scheme is spelled", format!("Bearer {token}")),
            (
                "its scheme in any case",
                format!("{} {token}", random::cased("bearer")),
            ),
            (
                "spaces after the scheme",
                format!("Bearer{}{token}", " ".repeat(random::between(2, 4))),
            ),
            ("a space after the token", format!("Bearer {token} ")),
        ]
    }

    /// Those that do not, each beside why.
    pub fn refused(token: &str) -> Vec<(&'static str, String)> {
        let mut refused = vec![
            ("no scheme", token.to_owned()),
            ("another scheme", format!("Basic {token}")),
            ("a scheme of its own", format!("Token {token}")),
            ("the scheme run into it", format!("Bearer{token}")),
            ("a tab after the scheme", format!("Bearer\t{token}")),
            ("a space before the scheme", format!(" Bearer {token}")),
            ("the scheme alone", String::from("Bearer")),
        ];
        for (_, not) in TokenMother::not(token) {
            refused.push(("not the token", format!("Bearer {not}")));
        }
        refused
    }
}
