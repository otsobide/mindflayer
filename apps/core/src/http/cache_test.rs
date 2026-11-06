//! Unit tests for `cache.rs`: an entry on the disk, read and written with no
//! transport. What a client serves from it is `tests/http_test.rs`'s.

use super::*;

use std::sync::atomic::{AtomicI64, Ordering};

use tempfile::TempDir;

use crate::http::Served;

const URL: &str = "https://catalog.example/v0/servers?cursor=a/../b&limit=100";

/// A clock that tells what a test sets it to.
#[derive(Debug, Default)]
struct Set(AtomicI64);

impl Clock for Set {
    fn now(&self) -> i64 {
        self.0.load(Ordering::SeqCst)
    }
}

fn entry(body: &[u8]) -> Entry {
    Entry {
        stored: 1_800_000_000,
        headers: Headers::new()
            .with("ETag", "\"v1\"")
            .with("Content-Type", "application/json"),
        body: body.to_vec(),
    }
}

/// One plain name of one length, whatever the URL holds, inside the
/// cache's folder.
#[test]
fn a_url_with_a_query_is_kept_under_one_plain_name() {
    let folder = TempDir::new().unwrap();
    let cache = Cache::at(folder.path());

    let path = cache.path_of(URL);

    assert_eq!(path.parent(), Some(folder.path()));
    let name = path.file_name().unwrap().to_str().unwrap();
    assert_eq!(name.len(), 64, "{name}");
    assert!(name.chars().all(|c| c.is_ascii_hexdigit()), "{name}");
    assert_ne!(
        cache.path_of("https://catalog.example/v0/servers?cursor=b"),
        path
    );
}

#[test]
fn an_entry_is_read_back_as_it_was_written() {
    let folder = TempDir::new().unwrap();
    let cache = Cache::at(folder.path().join("http"));
    // Not text, and holding the line break the entry's first line ends at.
    let kept = entry(b"\x00\xff\nsecond line\n");

    cache.write(URL, &kept).unwrap();

    assert_eq!(cache.read(URL).unwrap(), Some(kept));
}

#[test]
fn no_entry_is_a_miss_and_no_failure() {
    let folder = TempDir::new().unwrap();

    assert_eq!(Cache::at(folder.path()).read(URL).unwrap(), None);
    assert_eq!(
        Cache::at(folder.path().join("never-made"))
            .read(URL)
            .unwrap(),
        None
    );
}

/// Each way an entry can be broken is a miss said, never a fetch failed.
#[test]
fn an_entry_that_cannot_be_read_is_said_so() {
    let folder = TempDir::new().unwrap();
    let cache = Cache::at(folder.path());

    for (written, said) in [
        (&b"no line break at all"[..], "it says nothing of itself"),
        (b"{not json}\nbody", "what it says of itself: "),
        (b"{\"format\":1}\nbody", "what it says of itself: "),
        (
            b"{\"format\":2,\"stored\":1,\"headers\":[]}\nbody",
            "it is in format 2, and this build reads format 1",
        ),
    ] {
        fs::write(cache.path_of(URL), written).unwrap();

        let error = cache.read(URL).unwrap_err();

        assert_eq!(error.code(), "http_cache_unreadable");
        assert!(error.to_string().contains(said), "{error}");
    }
}

#[test]
fn an_entry_keeps_only_the_headers_a_later_read_needs() {
    let folder = TempDir::new().unwrap();
    let cache = Cache::at(folder.path());
    let fetched = Fetched {
        url: URL.to_owned(),
        status: 200,
        headers: Headers::new()
            .with("ETag", "\"v1\"")
            .with("Last-Modified", "Wed, 21 Oct 2026 07:28:00 GMT")
            .with("Link", "<https://catalog.example/2>; rel=\"next\"")
            .with("Content-Type", "application/json")
            .with("Set-Cookie", "session=s3cret")
            .with("X-RateLimit-Remaining", "59"),
        body: b"[]".to_vec(),
        served: Served::Network,
        warnings: Vec::new(),
    };

    cache.write(URL, &Entry::of(&fetched, 7)).unwrap();

    let names: Vec<String> = cache
        .read(URL)
        .unwrap()
        .unwrap()
        .headers
        .iter()
        .map(|(name, _)| name.to_ascii_lowercase())
        .collect();
    assert_eq!(names, ["etag", "last-modified", "link", "content-type"]);
    let written = fs::read_to_string(cache.path_of(URL)).unwrap();
    assert!(!written.contains("s3cret"), "{written}");
}

#[test]
fn an_entry_asks_whether_it_changed_as_its_answer_said_to() {
    let request = Request::get(URL).header("Accept", "application/json");
    let both = Entry {
        headers: Headers::new()
            .with("ETag", "\"v1\"")
            .with("Last-Modified", "Wed, 21 Oct 2026 07:28:00 GMT"),
        ..entry(b"[]")
    };

    let asking = both.conditional(&request).unwrap();

    assert_eq!(asking.header_value("if-none-match"), Some("\"v1\""));
    assert_eq!(
        asking.header_value("if-modified-since"),
        Some("Wed, 21 Oct 2026 07:28:00 GMT")
    );
    assert_eq!(asking.header_value("accept"), Some("application/json"));
    let neither = Entry {
        headers: Headers::new().with("Content-Type", "application/json"),
        ..entry(b"[]")
    };
    assert_eq!(neither.conditional(&request), None);
}

#[test]
fn an_entry_is_fresh_for_as_long_as_the_cache_is_told() {
    let clock = Arc::new(Set::default());
    let cache = Cache::at("unused")
        .fresh_for(Duration::from_secs(60))
        .stamped_by(clock.clone());
    let kept = Entry {
        stored: 1000,
        ..entry(b"[]")
    };

    for (now, fresh) in [(1000, true), (1059, true), (1060, false), (999, false)] {
        clock.0.store(now, Ordering::SeqCst);
        assert_eq!(cache.is_fresh(&kept), fresh, "at {now}");
    }
    clock.0.store(1000, Ordering::SeqCst);
    assert!(!Cache::at("unused").stamped_by(clock).is_fresh(&kept));
}
