//! Mothers of URLs: those a source is gathered from, and those typed with a
//! credential in them, beside what is left of each once it is redacted.
//!
//! Every way a credential can be typed into a URL is a way it can leak, and
//! the rule that none is stored, logged or shown ([`crate::url::redacted`])
//! is held by every value that holds one ([`crate::value::Redacted`],
//! [`crate::value::Origin`]): so the forms are listed here once, for every
//! test of that rule to try all of them.

use crate::mother::random;

/// A URL as somebody typed it with a credential in it, and as it may be
/// kept: what [`crate::url::redacted`] leaves of it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Credentialed {
    /// What was typed, the credential in it.
    pub typed: String,
    /// What may be stored, logged and shown of it.
    pub clean: String,
    /// The credential itself, which no redacted text holds.
    pub secret: String,
}

/// Makes URLs.
pub struct UrlMother;

impl UrlMother {
    /// A host nobody owns: `.example.com` is reserved for documentation.
    pub fn host() -> String {
        format!("{}.example.com", random::word())
    }

    /// An owner and a repository, as a forge lays out a path.
    pub fn path() -> String {
        format!("{}/{}.git", random::word(), random::word())
    }

    /// A secret as a forge issues one: a prefix and letters and digits, with
    /// no character a URL would read as a separator.
    pub fn secret() -> String {
        let alphabet = format!("{}{}ABCXYZ", random::LOWERCASE, random::DIGITS);
        format!("ghp_{}", random::drawn_from(&alphabet, 20))
    }

    /// An https URL with no credential.
    pub fn https() -> String {
        format!("https://{}/{}", Self::host(), Self::path())
    }

    /// One of the URLs a source can be gathered from that carry no
    /// credential, which redacting leaves as it is.
    pub fn clean() -> String {
        random::pick(&Self::every_clean())
    }

    /// Every form of URL with no credential in it, an ssh login among them,
    /// which is where the repository is rather than a secret.
    pub fn every_clean() -> Vec<String> {
        let (host, path) = (Self::host(), Self::path());
        vec![
            format!("https://{host}/{path}"),
            format!("http://{host}:8080/{path}"),
            format!("https://{host}/{path}?ref=main"),
            format!("ssh://git@{host}/{path}"),
            format!("ssh://git@{host}:2222/{path}"),
            format!("git+ssh://git@{host}/{path}"),
            format!("git@{host}:{path}"),
            format!("git://{host}/{path}"),
            format!("file:///srv/git/{path}"),
            format!("/srv/git/{path}"),
            format!("https://[::1]:8443/{path}"),
        ]
    }

    /// One of the URLs typed with a credential in it.
    pub fn credentialed() -> Credentialed {
        random::pick(&Self::every_credentialed())
    }

    /// Every way a credential is typed into a URL that the redaction knows,
    /// each with a host, a path and a secret of its own.
    pub fn every_credentialed() -> Vec<Credentialed> {
        let user = random::word();
        let forms: Vec<(String, String, String)> = (0..12)
            .map(|_| (Self::host(), Self::path(), Self::secret()))
            .collect();
        let at = |n: usize| forms[n].clone();
        let mut every = Vec::new();
        let mut add = |typed: String, clean: String, secret: String| {
            every.push(Credentialed {
                typed,
                clean,
                secret,
            });
        };

        let (h, p, s) = at(0);
        add(
            format!("https://{user}:{s}@{h}/{p}"),
            format!("https://{h}/{p}"),
            s,
        );
        // A token alone, where GitHub takes one in the user's place.
        let (h, p, s) = at(1);
        add(
            format!("https://{s}@{h}/{p}"),
            format!("https://{h}/{p}"),
            s,
        );
        let (h, p, s) = at(2);
        add(
            format!("https://x-access-token:{s}@{h}/{p}"),
            format!("https://{h}/{p}"),
            s,
        );
        let (h, p, s) = at(3);
        add(
            format!("http://{user}:{s}@{h}:8080/{p}"),
            format!("http://{h}:8080/{p}"),
            s,
        );
        // No path after the host.
        let (h, _, s) = at(4);
        add(format!("https://{user}:{s}@{h}"), format!("https://{h}"), s);
        // A scheme in capitals is the same scheme.
        let (h, p, s) = at(5);
        add(
            format!("HTTPS://{user}:{s}@{h}/{p}"),
            format!("HTTPS://{h}/{p}"),
            s,
        );
        // A password a `/` cuts short, which every parser reads as a port.
        let (h, p, s) = at(6);
        add(
            format!("https://{user}:{s}/x@{h}/{p}"),
            format!("https://{h}/{p}"),
            s,
        );
        // A password that kept a raw `@`.
        let (h, p, s) = at(7);
        add(
            format!("https://{user}:{s}@x@{h}/{p}"),
            format!("https://{h}/{p}"),
            s,
        );
        // Over ssh only the password goes: the user is who ssh logs in as.
        let (h, p, s) = at(8);
        add(
            format!("ssh://git:{s}@{h}/{p}"),
            format!("ssh://git@{h}/{p}"),
            s,
        );
        let (h, p, s) = at(9);
        add(
            format!("git+ssh://git:{s}@{h}/{p}"),
            format!("git+ssh://git@{h}/{p}"),
            s,
        );
        // A password with no user before it leaves nobody to log in as.
        let (h, p, s) = at(10);
        add(format!("ssh://:{s}@{h}/{p}"), format!("ssh://{h}/{p}"), s);
        // The user of a git:// URL is a token's place too.
        let (h, p, s) = at(11);
        add(format!("git://{s}@{h}/{p}"), format!("git://{h}/{p}"), s);
        every
    }

    /// Words quoting `url`, as an error from a clone does, and what is left
    /// of them once redacted.
    pub fn quoted(url: &Credentialed) -> Credentialed {
        let (before, after) = random::pick(&[
            ("failed to clone ", ": connection refused"),
            ("URL \"", "\" can not be parsed as valid URL"),
            (
                "fatal: unable to access '",
                "/': The requested URL returned error: 403",
            ),
            ("", ""),
        ]);
        Credentialed {
            typed: format!("{before}{}{after}", url.typed),
            clean: format!("{before}{}{after}", url.clean),
            secret: url.secret.clone(),
        }
    }
}
