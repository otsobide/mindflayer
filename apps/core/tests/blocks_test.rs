//! The blocks Mindweaver keeps in files somebody else owns, git's
//! `info/exclude` ([`ExcludeFile`]) and a project's `AGENTS.md`
//! ([`AgentsMd`]), held as properties over files nobody wrote by hand.
//!
//! "Never overwrite what somebody wrote" is the invariant both are built
//! for: every byte outside the block is kept, in the file's own line
//! endings, a missing final newline and bytes that are no UTF-8 included,
//! and taking the block out gives the file back as it was. The suites beside
//! each module name the cases; these draw the files, so a change to how lines
//! are split, joined or spaced that breaks a file no test spelled fails here.

use mindweaver_core::agents_md::{AgentsMd, Section};
use mindweaver_core::mother::random;
use mindweaver_core::mother::ArtifactNameMother;
use mindweaver_core::ExcludeFile;

const DRAWS: usize = 300;

/// A line somebody wrote: any text, a comment or a pattern, but no marker,
/// which neither block's markers can be spelled without (`<` and `#` at its
/// start are left out of what is drawn).
fn line() -> Vec<u8> {
    let text = random::text(30).replace(['<', '#'], "");
    let mut bytes = text.into_bytes();
    if random::between(0, 9) == 0 {
        // Now and then a byte that is no UTF-8, which a file can hold.
        bytes.push(0xff);
    }
    bytes
}

/// A file somebody wrote: some lines, each ended by LF or by CRLF as the
/// file's own convention or mixed, the last with or without an ending.
fn file() -> Vec<u8> {
    let crlf = random::chance();
    let mixed = random::between(0, 4) == 0;
    let count = random::between(0, 8);
    let mut bytes = Vec::new();
    for n in 0..count {
        bytes.extend(line());
        let last = n + 1 == count;
        if last && random::chance() {
            break;
        }
        let this_crlf = if mixed { random::chance() } else { crlf };
        bytes.extend_from_slice(if this_crlf { b"\r\n" } else { b"\n" });
    }
    bytes
}

/// Some patterns an exclude group may hold: none blank, none a comment and
/// none holding a control character, as the block refuses.
fn patterns() -> Vec<String> {
    (0..random::between(1, 4))
        .map(|_| format!("/{}/{}", random::word(), random::kebab()))
        .collect()
}

/// A project's key, as an exclude group names it.
fn key() -> String {
    random::hex(32)
}

/// Sections of rules with routes of their own and bodies with no marker.
fn sections() -> Vec<Section> {
    let mut routes: Vec<String> = (0..random::between(1, 4))
        .map(|_| ArtifactNameMother::random().into_string())
        .collect();
    routes.sort();
    routes.dedup();
    routes
        .into_iter()
        .map(|route| {
            let body: Vec<String> = (0..random::between(1, 5))
                .map(|_| random::text(30).replace(['<', '#', '\r', '\n'], ""))
                .collect();
            let body = format!("# {route}\n\n{}", body.join("\n"));
            Section::new(route.as_str(), &body).unwrap_or_else(|error| panic!("{body:?}: {error}"))
        })
        .collect()
}

/// The ending a file uses, for a line it gains: its first.
fn ending_of(bytes: &[u8]) -> &'static [u8] {
    match bytes.iter().position(|&b| b == b'\n') {
        Some(at) if at > 0 && bytes[at - 1] == b'\r' => b"\r\n",
        _ => b"\n",
    }
}

// ---------------------------------------------------------------------------
// git's info/exclude
// ---------------------------------------------------------------------------

#[test]
fn an_exclude_file_with_no_block_is_written_back_byte_for_byte() {
    for _ in 0..DRAWS {
        let bytes = file();

        let parsed = ExcludeFile::parse(&bytes).unwrap_or_else(|e| panic!("{bytes:?}: {e}"));

        assert_eq!(parsed.to_bytes(), bytes, "{bytes:?}");
    }
}

#[test]
fn adding_a_group_and_forgetting_it_gives_the_exclude_file_back_as_it_was() {
    for _ in 0..DRAWS {
        let bytes = file();
        let (key, patterns) = (key(), patterns());
        let mut parsed = ExcludeFile::parse(&bytes).unwrap();

        assert!(parsed.add(&key, &patterns).unwrap(), "{bytes:?}");
        let written = parsed.to_bytes();
        let mut again = ExcludeFile::parse(&written).unwrap_or_else(|e| panic!("{written:?}: {e}"));
        assert_eq!(again.lines_of(&key), patterns, "{written:?}");
        assert!(again.forget(&key).unwrap());

        assert_eq!(again.to_bytes(), bytes, "{bytes:?}");
    }
}

#[test]
fn what_an_exclude_file_held_before_its_block_is_kept_ahead_of_it() {
    for _ in 0..DRAWS {
        let bytes = file();
        let mut parsed = ExcludeFile::parse(&bytes).unwrap();
        parsed.add(&key(), &patterns()).unwrap();

        let written = parsed.to_bytes();

        // The last line gains the ending it lacked, so the block starts on a
        // line of its own; everything before that is the file's.
        let kept = match bytes.last() {
            Some(b'\n') | None => bytes.clone(),
            Some(_) => [bytes.as_slice(), ending_of(&bytes)].concat(),
        };
        assert!(written.starts_with(&kept), "{bytes:?} became {written:?}");
    }
}

#[test]
fn a_group_added_twice_changes_nothing_the_second_time() {
    for _ in 0..DRAWS {
        let (key, patterns) = (key(), patterns());
        let mut parsed = ExcludeFile::parse(&file()).unwrap();
        parsed.add(&key, &patterns).unwrap();
        let once = parsed.to_bytes();

        assert!(!parsed.add(&key, &patterns).unwrap());
        assert_eq!(parsed.to_bytes(), once);
    }
}

#[test]
fn removing_what_was_added_leaves_only_what_was_not() {
    for _ in 0..DRAWS {
        let key = key();
        let (kept, taken) = (patterns(), patterns());
        let mut parsed = ExcludeFile::parse(&file()).unwrap();
        parsed.add(&key, &kept).unwrap();
        parsed.add(&key, &taken).unwrap();

        parsed.remove(&key, &taken).unwrap();

        let left: Vec<String> = kept
            .iter()
            .filter(|pattern| !taken.contains(pattern))
            .cloned()
            .collect();
        assert_eq!(parsed.lines_of(&key), left, "{kept:?} less {taken:?}");
    }
}

#[test]
fn one_projects_group_is_never_touched_by_anothers_change() {
    for _ in 0..DRAWS {
        let (mine, theirs) = (key(), key());
        let (my_patterns, their_patterns) = (patterns(), patterns());
        let mut parsed = ExcludeFile::parse(&file()).unwrap();
        parsed.add(&theirs, &their_patterns).unwrap();
        parsed.add(&mine, &my_patterns).unwrap();

        parsed.forget(&mine).unwrap();

        assert_eq!(parsed.lines_of(&theirs), their_patterns);
        assert!(parsed.lines_of(&mine).is_empty());
    }
}

#[test]
fn a_crlf_exclude_file_gains_its_block_in_crlf() {
    for _ in 0..DRAWS {
        let bytes = b"# mine\r\n/target\r\n".to_vec();
        let mut parsed = ExcludeFile::parse(&bytes).unwrap();
        parsed.add(&key(), &patterns()).unwrap();

        let written = parsed.to_bytes();

        let lone_lf = written
            .iter()
            .enumerate()
            .any(|(at, &b)| b == b'\n' && (at == 0 || written[at - 1] != b'\r'));
        assert!(!lone_lf, "{}", String::from_utf8_lossy(&written));
    }
}

#[test]
fn a_key_or_a_pattern_the_block_cannot_hold_is_refused_and_nothing_changes() {
    for _ in 0..DRAWS {
        let bytes = file();
        let mut parsed = ExcludeFile::parse(&bytes).unwrap();
        let bad_key = random::pick(&["", " padded", "line\nbreak", "tab\there"]);
        let bad_pattern = random::pick(&["", "   ", "# a comment", "a\nb", "bell\u{7}"]);

        assert!(parsed.add(bad_key, &patterns()).is_err(), "{bad_key:?}");
        assert!(
            parsed.add(&key(), &[bad_pattern.to_owned()]).is_err(),
            "{bad_pattern:?}"
        );
        assert_eq!(parsed.to_bytes(), bytes);
    }
}

// ---------------------------------------------------------------------------
// A project's AGENTS.md
// ---------------------------------------------------------------------------

#[test]
fn an_agents_md_with_no_block_is_written_back_byte_for_byte() {
    for _ in 0..DRAWS {
        let bytes = file();

        let parsed = AgentsMd::parse(&bytes).unwrap_or_else(|e| panic!("{bytes:?}: {e}"));

        assert!(!parsed.has_block());
        assert_eq!(parsed.to_bytes(), bytes, "{bytes:?}");
    }
}

#[test]
fn rendering_a_block_and_dropping_it_gives_the_agents_md_back_as_it_was() {
    for _ in 0..DRAWS {
        let bytes = file();
        let sections = sections();
        let mut parsed = AgentsMd::parse(&bytes).unwrap();

        assert!(parsed.set_block(&sections).unwrap());
        let written = parsed.to_bytes();
        let mut again = AgentsMd::parse(&written).unwrap_or_else(|e| panic!("{written:?}: {e}"));
        assert!(again.has_block());
        assert!(again.set_block(&[]).unwrap());

        assert_eq!(again.to_bytes(), bytes, "{bytes:?}");
    }
}

#[test]
fn a_block_read_back_holds_each_section_in_the_order_it_was_rendered() {
    for _ in 0..DRAWS {
        let sections = sections();
        let mut parsed = AgentsMd::parse(&file()).unwrap();
        parsed.set_block(&sections).unwrap();

        let again = AgentsMd::parse(&parsed.to_bytes()).unwrap();

        let routes: Vec<&str> = sections.iter().map(Section::route).collect();
        assert_eq!(again.routes(), routes);
        for section in &sections {
            let read = again.section(section.route()).unwrap();
            assert_eq!(read, *section, "{}", section.route());
            assert_eq!(read.hash(), section.hash(), "{}", section.route());
        }
    }
}

#[test]
fn rendering_the_same_sections_again_changes_nothing() {
    for _ in 0..DRAWS {
        let sections = sections();
        let mut parsed = AgentsMd::parse(&file()).unwrap();
        parsed.set_block(&sections).unwrap();
        let once = parsed.to_bytes();

        let mut again = AgentsMd::parse(&once).unwrap();
        assert!(!again.set_block(&sections).unwrap());
        assert_eq!(again.to_bytes(), once);
    }
}

#[test]
fn what_an_agents_md_held_is_kept_ahead_of_its_block() {
    for _ in 0..DRAWS {
        let bytes = file();
        let mut parsed = AgentsMd::parse(&bytes).unwrap();
        parsed.set_block(&sections()).unwrap();

        let written = parsed.to_bytes();

        let kept = match bytes.last() {
            Some(b'\n') | None => bytes.clone(),
            Some(_) => [bytes.as_slice(), ending_of(&bytes)].concat(),
        };
        assert!(written.starts_with(&kept), "{bytes:?} became {written:?}");
    }
}

#[test]
fn a_file_that_held_nothing_holds_nothing_but_the_block_outside_it() {
    for _ in 0..DRAWS {
        let mut parsed = AgentsMd::parse(b"").unwrap();
        parsed.set_block(&sections()).unwrap();

        let again = AgentsMd::parse(&parsed.to_bytes()).unwrap();

        assert!(again.is_blank_outside());
    }
}

#[test]
fn a_section_twice_in_one_block_is_refused_and_nothing_changes() {
    for _ in 0..DRAWS {
        let bytes = file();
        let mut parsed = AgentsMd::parse(&bytes).unwrap();
        let section = sections().remove(0);

        assert!(parsed.set_block(&[section.clone(), section]).is_err());
        assert_eq!(parsed.to_bytes(), bytes);
    }
}

#[test]
fn a_rule_whose_body_holds_a_marker_is_no_section() {
    let route = ArtifactNameMother::random();
    for marker in [
        String::from("<!-- mindweaver:begin -->"),
        String::from("<!-- mindweaver:end -->"),
        format!("<!-- mindweaver:rule {route} -->"),
        String::from("<!-- MINDFLAYER:whatever -->"),
    ] {
        let body = format!("# {route}\n\n{marker}\n\nMore.");

        assert!(Section::new(route.as_str(), &body).is_err(), "{marker}");
    }
}

#[test]
fn a_section_is_named_only_by_a_route_the_name_rules_take() {
    for (why, route) in ArtifactNameMother::unwritable() {
        assert!(
            Section::new(route.as_str(), "Body.").is_err(),
            "{why}: {route:?}"
        );
    }
    for _ in 0..DRAWS {
        let route = ArtifactNameMother::route();
        assert!(Section::new(route.as_str(), "Body.").is_ok(), "{route}");
    }
}

/// A block moved to Mindflayer's markers and back is the block it was, and
/// one rendered again under the old markers keeps them and every byte
/// outside: what `migrate --names` and a render of a project an older build
/// wrote into rely on.
#[test]
fn a_block_spelled_under_either_names_keeps_every_byte_outside_it() {
    use mindweaver_core::Names;

    for _ in 0..DRAWS {
        let sections = sections();
        let mut file = AgentsMd::parse(&file()).unwrap();
        file.set_block(&sections).unwrap();
        let new = file.to_bytes();

        assert!(file.renamed(Names::Mindflayer));
        let old = file.to_bytes();
        let mut reread = AgentsMd::parse(&old).unwrap();
        assert_eq!(reread.names(), Some(Names::Mindflayer));
        assert!(!reread.set_block(&sections).unwrap(), "rendered again");
        assert_eq!(reread.to_bytes(), old);
        assert!(reread.renamed(Names::Mindweaver));
        assert_eq!(reread.to_bytes(), new);
    }
}
