//! Unit tests for `url.rs`, kept beside it rather than inside it.

use super::*;

#[test]
fn redacting_drops_the_userinfo_before_the_host() {
    assert_eq!(
        redacted("https://x-access-token:ghp_secret@github.com/acme/private"),
        "https://github.com/acme/private"
    );
    // A password that kept a raw `@` still ends at the last one before the
    // host.
    assert_eq!(
        redacted("https://me:p@ss@example.com/x"),
        "https://example.com/x"
    );
    // A host with no path after it.
    assert_eq!(
        redacted("https://me:hunter2@example.com"),
        "https://example.com"
    );
}

#[test]
fn a_password_with_a_slash_in_it_goes_too_however_soon_the_slash_comes() {
    for (url, clean) in [
        (
            "https://me:p/ss@example.invalid/acme/skills",
            "https://example.invalid/acme/skills",
        ),
        ("https://me:/pass@example.com/x", "https://example.com/x"),
        (
            "ssh://git:p/ss@example.com/acme/private.git",
            "ssh://git@example.com/acme/private.git",
        ),
    ] {
        assert_eq!(redacted(url), clean, "{url}");
        assert!(carries_credentials(url), "{url}");
    }
    // As `gix` quotes it when it cannot parse it: whitespace still ends it.
    assert_eq!(
        redacted("URL \"https://me:p/ss@example.invalid/x\" can not be parsed as valid URL"),
        "URL \"https://example.invalid/x\" can not be parsed as valid URL"
    );
}

#[test]
fn a_user_name_alone_goes_too_where_a_token_can_stand_in_for_one() {
    // GitHub takes a token as the user name of an https URL.
    assert_eq!(
        redacted("https://ghp_secret@github.com/acme/private"),
        "https://github.com/acme/private"
    );
    assert_eq!(
        redacted("git://me@example.com/acme/skills"),
        "git://example.com/acme/skills"
    );
}

#[test]
fn the_user_ssh_logs_in_as_is_kept_and_only_its_password_goes() {
    for url in [
        "ssh://git@example.com:2222/acme/private.git",
        "git+ssh://git@example.com/acme/private.git",
        "SSH://git@example.com/acme/private.git",
    ] {
        assert_eq!(redacted(url), url);
    }
    assert_eq!(
        redacted("ssh://git:hunter2@example.com/acme/private.git"),
        "ssh://git@example.com/acme/private.git"
    );
    // A password with no user before it leaves nothing to log in as.
    assert_eq!(
        redacted("ssh://:hunter2@example.com/acme/private.git"),
        "ssh://example.com/acme/private.git"
    );
}

#[test]
fn redacting_leaves_a_url_without_userinfo_as_typed() {
    for url in [
        "https://github.com/acme/skills",
        "HTTPS://github.com:443/acme/skills",
        // An `@` after the host is part of the path, not a user.
        "file:///srv/git/team@work/skills",
        "https://example.com/acme/skills?ref=a@b",
        // A port, an IPv6 host and a drive letter are not the start of a
        // password, so an `@` after them is still in the path.
        "https://example.com:8080/team@work/skills",
        "https://[::1]/team@work/skills",
        "https://[::1]:8080/team@work/skills",
        "file://C:/Users/me@corp/skills",
        // Nor is a `:` with no `@` anywhere after it.
        "https://example.com:x/acme/skills",
        // The scp-like form has no room for a password.
        "git@github.com:acme/skills.git",
        "/srv/git/skills",
    ] {
        assert!(matches!(redacted(url), Cow::Borrowed(_)), "{url}");
    }
}

#[test]
fn every_url_in_a_message_is_redacted_and_the_words_around_them_kept() {
    assert_eq!(
        redacted(
            "URL \"https://me:hunter2@example.com\" can not be parsed, \
             and ssh://git:hunter2@example.com/x refused it"
        ),
        "URL \"https://example.com\" can not be parsed, and ssh://git@example.com/x refused it"
    );
    // Whitespace ends a URL, so an `@` in the words after one is left alone.
    assert_eq!(
        redacted("https://example.com failed for me@example.com"),
        "https://example.com failed for me@example.com"
    );
    // Text before the scheme that is not ASCII is stepped over, not cut.
    assert_eq!(
        redacted("échec: https://me:hunter2@example.com/x"),
        "échec: https://example.com/x"
    );
}

#[test]
fn a_url_carries_credentials_when_redacting_would_change_it() {
    assert!(carries_credentials("https://u:t@example.com/x"));
    assert!(carries_credentials("https://t@example.com/x"));
    assert!(carries_credentials("ssh://git:t@example.com/x"));
    assert!(!carries_credentials("ssh://git@example.com/x"));
    assert!(!carries_credentials("git@example.com:acme/x.git"));
    assert!(!carries_credentials("https://example.com/x"));
}

/// A token as GitHub hands one out, made here rather than written down, so
/// nothing in the repository looks like a secret to a scanner.
fn token() -> String {
    format!("ghp_{}", "T0k3n".repeat(7) + "x")
}

#[test]
fn every_spelling_of_a_forge_repository_is_one_identity() {
    let token = token();
    for (forge, one) in [
        ("github.com", "https://github.com/acme/skills"),
        ("gitlab.com", "https://gitlab.com/acme/skills"),
    ] {
        for spelling in [
            format!("https://{forge}/acme/skills"),
            format!("https://{forge}/acme/skills.git"),
            format!("https://{forge}/acme/skills/"),
            format!("HTTPS://{}/Acme/Skills.git", forge.to_uppercase()),
            format!("http://{forge}/acme/skills"),
            format!("git@{forge}:acme/skills.git"),
            format!("git@{forge}:acme/skills"),
            format!("ssh://git@{forge}/acme/skills.git"),
            format!("ssh://git@{forge}:22/acme/skills.git"),
            format!("git+ssh://git@{forge}/acme/skills"),
            format!("git://{forge}/acme/skills.git"),
            format!("https://x-access-token:{token}@{forge}/acme/skills.git"),
            format!("https://{token}@{forge}/acme/skills"),
            format!("ssh://git:{token}@{forge}/acme/skills.git"),
        ] {
            assert_eq!(canonical(&spelling), one, "{spelling}");
        }
    }
    // `owner/repo` is a GitHub repository's.
    assert_eq!(canonical("acme/skills"), "https://github.com/acme/skills");
    assert_eq!(
        canonical("Acme/skills.git"),
        "https://github.com/acme/skills"
    );
    // A GitLab group nests, and keeps every level.
    assert_eq!(
        canonical("git@gitlab.com:acme/platform/skills.git"),
        "https://gitlab.com/acme/platform/skills"
    );
}

#[test]
fn two_repositories_stay_two_identities() {
    for (one, other) in [
        (
            "https://github.com/acme/skills",
            "https://github.com/acme/skills-2",
        ),
        (
            "https://github.com/acme/skills",
            "https://gitlab.com/acme/skills",
        ),
        (
            "https://github.com/acme/skills",
            "https://github.com/other/skills",
        ),
        // Elsewhere `host:path` starts at the login's home and
        // `ssh://host/path` at the root, and `x.git` may be another `x`.
        (
            "git@example.com:acme/skills",
            "ssh://git@example.com/acme/skills",
        ),
        (
            "https://example.com/acme/skills",
            "https://example.com/acme/skills.git",
        ),
        ("/srv/git/skills", "/srv/git/skills-2"),
        // Off a forge the login is part of where the repository is:
        // `skills` in alice's home and in bob's.
        ("alice@example.com:skills", "bob@example.com:skills"),
        (
            "ssh://alice@example.com/~/skills",
            "ssh://bob@example.com/~/skills",
        ),
    ] {
        assert_ne!(canonical(one), canonical(other), "{one} and {other}");
    }
}

#[test]
fn elsewhere_only_what_names_no_other_repository_goes() {
    let token = token();
    for (spelling, one) in [
        (
            format!("https://me:{token}@Example.COM/Acme/Skills.git/"),
            "https://example.com/Acme/Skills.git",
        ),
        (
            String::from("SSH://alice@Example.com:2222/acme/skills.git"),
            "ssh://alice@example.com:2222/acme/skills.git",
        ),
        (
            format!("ssh://alice:{token}@example.com/acme/skills.git"),
            "ssh://alice@example.com/acme/skills.git",
        ),
        (
            String::from("git@Example.com:acme/skills.git"),
            "git@example.com:acme/skills.git",
        ),
        (
            format!("https://me:p/{token}@example.invalid/acme/skills"),
            "https://example.invalid/acme/skills",
        ),
        (String::from("/srv/git/skills/"), "/srv/git/skills"),
        (String::from("file:///srv/git/skills"), "/srv/git/skills"),
        (
            String::from("file://localhost/srv/git/skills"),
            "/srv/git/skills",
        ),
        (String::from("./skills/repo"), "./skills/repo"),
        (String::from("../team"), "../team"),
        (String::from("C:\\git\\skills"), "C:\\git\\skills"),
        (String::from("/"), "/"),
    ] {
        assert_eq!(canonical(&spelling), one, "{spelling}");
    }
}

#[test]
fn no_identity_carries_a_credential() {
    let token = token();
    for url in [
        format!("https://x-access-token:{token}@github.com/acme/private"),
        format!("https://{token}@github.com/acme/private"),
        format!("https://me:{token}@example.com/acme/private"),
        format!("ssh://git:{token}@example.com/acme/private.git"),
        format!("https://me:p/{token}@example.invalid/acme/private"),
        format!("git+ssh://me:{token}@gitlab.com/acme/private.git"),
    ] {
        let identity = canonical(&url);
        assert!(!identity.contains(&token), "{url} gave {identity}");
        // What userinfo is left is the user ssh logs in as off a forge.
        assert!(
            !identity.contains('@') || identity.starts_with("ssh://git@example.com/"),
            "{url} gave {identity}"
        );
        assert!(!carries_credentials(&identity), "{url} gave {identity}");
    }
}

#[test]
fn owner_and_repo_is_fetched_from_github_and_a_path_says_it_is_one() {
    assert_eq!(fetched("acme/skills"), "https://github.com/acme/skills");
    assert_eq!(
        fetched("acme/skills.git"),
        "https://github.com/acme/skills.git"
    );
    for typed in [
        "./skills/repo",
        "../team",
        ".hidden/repo",
        "acme/..",
        "/srv/git/skills",
        "acme/skills/more",
        "https://github.com/acme/skills",
        "git@github.com:acme/skills.git",
        "skills",
        "C:\\git\\skills",
    ] {
        assert_eq!(fetched(typed), typed, "{typed}");
    }
}

#[test]
fn a_repository_is_named_by_the_last_folder_of_its_identity() {
    for (url, name) in [
        ("acme/pdf-skill", "pdf-skill"),
        ("git@github.com:Acme/PDF-Skill.git", "pdf-skill"),
        ("https://github.com/acme/pdf-skill/", "pdf-skill"),
        ("ssh://git@gitlab.com/group/sub/notes.git", "notes"),
        ("https://git.example.com/team/Notes.git", "Notes"),
        ("alice@host:skills/notes", "notes"),
        ("file:///srv/git/notes.git", "notes"),
        ("../src/notes", "notes"),
        ("./notes/", "notes"),
        ("https://me:t0ken@example.com/team/notes", "notes"),
    ] {
        assert_eq!(repository(url).as_deref(), Some(name), "{url}");
    }
}

#[test]
fn a_url_whose_path_ends_in_no_folder_names_no_repository() {
    for url in [
        ".",
        "./",
        "/",
        "..",
        "../",
        "https://example.com",
        "https://example.com/",
    ] {
        assert_eq!(repository(url), None, "{url}");
    }
}

/// An address holding `url`, at `reference`, in `path`.
fn addressed(url: &str, reference: Option<&str>, path: Option<&str>) -> Address {
    Address {
        url: url.to_owned(),
        reference: reference.map(str::to_owned),
        path: path.map(str::to_owned),
    }
}

#[test]
fn owner_and_repo_at_a_ref_is_the_repository_at_that_ref() {
    for (typed, url, reference) in [
        ("acme/skills@v2", "acme/skills", "v2"),
        ("acme/skills@main", "acme/skills", "main"),
        (
            "Acme/Skills.git@release/2.0",
            "Acme/Skills.git",
            "release/2.0",
        ),
        ("acme/skills@4f9c2a7", "acme/skills", "4f9c2a7"),
    ] {
        assert_eq!(
            address(typed),
            addressed(url, Some(reference), None),
            "{typed}"
        );
    }
    assert_eq!(address("acme/skills"), addressed("acme/skills", None, None));
}

#[test]
fn a_github_tree_url_is_its_repository_its_branch_and_its_folder() {
    for (typed, url, reference, path) in [
        (
            "https://github.com/acme/skills/tree/main/skills/pdf",
            "https://github.com/acme/skills",
            "main",
            Some("skills/pdf"),
        ),
        (
            "https://github.com/acme/skills/tree/v2/plugins/ops/skills/",
            "https://github.com/acme/skills",
            "v2",
            Some("plugins/ops/skills"),
        ),
        (
            "https://github.com/acme/skills/tree/main",
            "https://github.com/acme/skills",
            "main",
            None,
        ),
        (
            "https://github.com/acme/skills/tree/main/",
            "https://github.com/acme/skills",
            "main",
            None,
        ),
        (
            "HTTPS://GitHub.com/Acme/Skills/tree/dev/x",
            "HTTPS://GitHub.com/Acme/Skills",
            "dev",
            Some("x"),
        ),
    ] {
        assert_eq!(
            address(typed),
            addressed(url, Some(reference), path),
            "{typed}"
        );
    }
}

#[test]
fn a_gitlab_tree_url_is_its_repository_however_deep_its_groups_go() {
    for (typed, url, reference, path) in [
        (
            "https://gitlab.com/acme/skills/-/tree/main/skills/pdf",
            "https://gitlab.com/acme/skills",
            "main",
            Some("skills/pdf"),
        ),
        (
            "https://gitlab.com/acme/team/skills/-/tree/v2",
            "https://gitlab.com/acme/team/skills",
            "v2",
            None,
        ),
    ] {
        assert_eq!(
            address(typed),
            addressed(url, Some(reference), path),
            "{typed}"
        );
    }
}

#[test]
fn a_tree_url_keeps_the_credential_it_carries_for_a_gather_to_refuse() {
    let typed = format!(
        "https://{}@github.com/acme/private/tree/main/skills",
        token()
    );

    let read = address(&typed);

    assert_eq!(
        read.url,
        format!("https://{}@github.com/acme/private", token())
    );
    assert!(carries_credentials(&read.url));
}

#[test]
fn anything_else_is_the_url_it_is_with_nothing_beside_it() {
    for typed in [
        "https://github.com/acme/skills",
        "https://github.com/acme/skills.git",
        "git@github.com:acme/skills.git",
        "ssh://git@github.com/acme/skills",
        "https://github.com/acme/skills/blob/main/README.md",
        "https://github.com/acme/skills/tree",
        "https://github.com/acme/skills/tree/",
        "https://github.com/acme/tree/main",
        "https://gitlab.com/acme/skills/tree/main",
        "https://git.example.com/acme/skills/tree/main/x",
        "ssh://git@github.com/acme/skills/tree/main/x",
        "./acme/skills@v2",
        "acme/skills@",
        "me@host:skills",
        "/srv/git/skills",
    ] {
        assert_eq!(address(typed), addressed(typed, None, None), "{typed}");
    }
}
