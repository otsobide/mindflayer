//! Unit tests for `claude_md.rs`, kept beside it rather than inside it.

use super::*;

#[test]
fn the_walk_counts_the_folders_claudes_own_check_counted() {
    // The starts of the probe's runs, and how many folders Claude's debug
    // log said it searched from each (docs/agents.md).
    let from = |start: &str| searched(Path::new(start)).count();
    assert_eq!(from("/mnt/agent-probe/stage/claude-excluded"), 4);
    assert_eq!(from("/mnt/agent-probe/home/work/claude-user-memory"), 5);
}

#[test]
fn the_walk_goes_from_the_start_up_and_stops_below_the_root() {
    let folders: Vec<&Path> = searched(Path::new("/srv/work/app")).collect();
    assert_eq!(
        folders,
        [
            Path::new("/srv/work/app"),
            Path::new("/srv/work"),
            Path::new("/srv")
        ]
    );
    assert_eq!(searched(Path::new("/")).count(), 0);
}

/// What a team's `CLAUDE.md` might hold before Mindflayer writes in it.
const TEAM: &str = "\
# Working here

Run `make test` before every push.
";

const IMPORT: &str = "@AGENTS.md";

fn text(file: &ClaudeMd) -> String {
    String::from_utf8(file.to_bytes()).unwrap()
}

/// `file` given the import, written and read back, as two renders a run
/// apart would see it.
fn imported(file: &[u8]) -> Vec<u8> {
    let mut parsed = ClaudeMd::parse(file).unwrap();
    parsed.set_import(IMPORT);
    parsed.to_bytes()
}

/// `file` with the import taken out, written and read back.
fn dropped(file: &[u8]) -> Vec<u8> {
    let mut parsed = ClaudeMd::parse(file).unwrap();
    parsed.drop_import();
    parsed.to_bytes()
}

#[test]
fn a_file_gains_the_import_at_the_top_an_empty_line_above_its_text() {
    let mut file = ClaudeMd::parse(TEAM.as_bytes()).unwrap();
    assert!(!file.has_block());
    assert!(file.set_import(IMPORT));
    assert_eq!(
        text(&file),
        "<!-- mindflayer:begin -->\n\
         @AGENTS.md\n\
         <!-- mindflayer:end -->\n\
         \n\
         # Working here\n\
         \n\
         Run `make test` before every push.\n"
    );
    assert!(file.imports(IMPORT));
}

#[test]
fn an_empty_file_gains_the_import_alone() {
    assert_eq!(
        imported(b""),
        b"<!-- mindflayer:begin -->\n@AGENTS.md\n<!-- mindflayer:end -->\n"
    );
    assert_eq!(dropped(&imported(b"")), b"");
}

#[test]
fn the_import_set_twice_changes_nothing_the_second_time() {
    let once = imported(TEAM.as_bytes());
    let mut file = ClaudeMd::parse(&once).unwrap();
    assert!(!file.set_import(IMPORT));
    assert_eq!(file.to_bytes(), once);
}

#[test]
fn dropping_the_import_gives_back_every_file_byte_for_byte() {
    let files: [&[u8]; 8] = [
        TEAM.as_bytes(),
        b"No newline at the end",
        b"\nOpens with an empty line.\n",
        b"Ends with empty lines.\n\n\n",
        b"# Team\r\n\r\nWindows line endings.\r\n",
        b"\xEF\xBB\xBF# Team\r\n\r\nA mark Windows writes first.\r\n",
        b"Not UTF-8: \xff\xfe.\n",
        b"\n",
    ];
    for file in files {
        let with = imported(file);
        assert!(ClaudeMd::parse(&with).unwrap().imports(IMPORT));
        assert_eq!(dropped(&with), file, "{}", String::from_utf8_lossy(file));
    }
}

#[test]
fn a_crlf_file_gets_a_crlf_block_and_a_marked_one_keeps_its_mark_first() {
    let with = imported(b"# Team\r\n");
    assert_eq!(
        with,
        b"<!-- mindflayer:begin -->\r\n@AGENTS.md\r\n<!-- mindflayer:end -->\r\n\r\n# Team\r\n"
    );
    let with = imported(b"\xEF\xBB\xBF# Team\n");
    assert!(with.starts_with(b"\xEF\xBB\xBF<!-- mindflayer:begin -->\n"));
}

#[test]
fn a_block_somebody_moved_is_changed_and_dropped_where_it_is() {
    let moved = "# Team\n\n<!-- mindflayer:begin -->\n@OLD.md\n<!-- mindflayer:end -->";
    let mut file = ClaudeMd::parse(moved.as_bytes()).unwrap();
    assert!(file.has_block());
    assert!(!file.imports(IMPORT));
    assert!(file.set_import(IMPORT));
    assert_eq!(
        text(&file),
        "# Team\n\n<!-- mindflayer:begin -->\n@AGENTS.md\n<!-- mindflayer:end -->"
    );
    // The file ended at the end marker, so it ends where the block began.
    assert!(file.drop_import());
    assert_eq!(text(&file), "# Team\n");
}

#[test]
fn the_import_is_only_the_one_line_alone_between_the_markers() {
    let holds = |inside: &str| {
        let file = format!("<!-- mindflayer:begin -->\n{inside}<!-- mindflayer:end -->\n");
        ClaudeMd::parse(file.as_bytes()).unwrap().imports(IMPORT)
    };
    assert!(holds("@AGENTS.md\n"));
    assert!(holds("@AGENTS.md  \n"));
    assert!(!holds(""));
    assert!(!holds("@AGENTS.md\n@AGENTS.md\n"));
    assert!(!holds("@../AGENTS.md\n"));
    assert!(!holds(" @AGENTS.md\n"));
    assert!(!ClaudeMd::parse(TEAM.as_bytes()).unwrap().imports(IMPORT));
}

#[test]
fn a_malformed_block_is_refused_as_in_agents_md() {
    let refused = |file: &str| ClaudeMd::parse(file.as_bytes()).unwrap_err();
    assert_eq!(
        refused("# Team\n<!-- mindflayer:begin -->\n@AGENTS.md\n"),
        Malformed::Unclosed { line: 2 }
    );
    assert_eq!(
        refused("@AGENTS.md\n<!-- mindflayer:end -->\n"),
        Malformed::Unopened { line: 2 }
    );
    assert_eq!(
        refused(
            "<!-- mindflayer:begin -->\n<!-- mindflayer:end -->\n\
             <!-- mindflayer:begin -->\n<!-- mindflayer:end -->\n"
        ),
        Malformed::Reopened { line: 3 }
    );
    assert_eq!(
        refused("  <!-- mindflayer:begin -->\n@AGENTS.md\n<!-- mindflayer:end -->\n"),
        Malformed::Miswritten { line: 1 }
    );
    assert_eq!(
        refused("<!-- mindflayer:rule style -->\nBe brief.\n"),
        Malformed::Stray { line: 1 }
    );
}

#[test]
fn a_rules_marker_in_the_block_is_refused_as_a_copy_of_agents_md() {
    let copy = "# Team\n\n<!-- mindflayer:begin -->\n\
                <!-- mindflayer:rule style -->\nBe brief.\n<!-- mindflayer:end -->\n";
    assert_eq!(
        ClaudeMd::parse(copy.as_bytes()).unwrap_err(),
        Malformed::Ruled { line: 4 }
    );
}

/// Whether `file`, read as a person's own file is, imports `AGENTS.md`.
fn follows(file: &str) -> bool {
    imported_by(&lines::split(file.as_bytes()), IMPORT)
}

#[test]
fn an_import_somebody_wrote_is_a_line_of_its_own() {
    assert!(follows("@AGENTS.md\n"));
    assert!(follows("@AGENTS.md"));
    assert!(follows("@AGENTS.md  \r\n"));
    assert!(follows(&format!("@AGENTS.md\n\n{TEAM}")));
    assert!(follows(&format!("{TEAM}\n@AGENTS.md\n")));
    assert!(!follows(TEAM));
    assert!(!follows(" @AGENTS.md\n"));
    assert!(!follows("    @AGENTS.md\n"));
    assert!(!follows("`@AGENTS.md`\n"));
    assert!(!follows("@AGENTS.md.\n"));
    assert!(!follows("@../AGENTS.md\n"));
    // Claude follows one in a sentence too, but a word is not looked for.
    assert!(!follows("Read @AGENTS.md first.\n"));
}

#[test]
fn an_import_in_a_code_fence_is_not_one() {
    assert!(!follows("```\n@AGENTS.md\n```\n"));
    assert!(!follows("~~~markdown\n@AGENTS.md\n~~~\n"));
    assert!(!follows("   ```\n@AGENTS.md\n"));
    // Closed only by a run as long, of the same character, alone.
    assert!(!follows("````\n```\n@AGENTS.md\n````\n"));
    assert!(!follows("```\n~~~\n@AGENTS.md\n```\n"));
    assert!(!follows("```\n``` text\n@AGENTS.md\n```\n"));
    assert!(follows("```\nmake test\n```\n\n@AGENTS.md\n"));
    assert!(follows("~~~\n~~~~ \n@AGENTS.md\n"));
}

#[test]
fn an_import_in_an_html_comment_is_not_one() {
    assert!(!follows("<!--\n@AGENTS.md\n-->\n"));
    assert!(!follows("<!-- notes\n@AGENTS.md\n"));
    assert!(!follows("<!-- one -->\n<!-- two\n@AGENTS.md\n-->\n"));
    assert!(!follows("<!--\n--> <!--\n@AGENTS.md\n-->\n"));
    assert!(follows("<!--\nnotes\n-->\n@AGENTS.md\n"));
    // Mindflayer's markers are comments of one line each.
    assert!(follows(
        "<!-- mindflayer:begin -->\n@AGENTS.md\n<!-- mindflayer:end -->\n"
    ));
}

#[test]
fn an_import_outside_the_block_is_somebodys_and_the_one_inside_is_not() {
    let theirs = format!("@AGENTS.md\n\n{TEAM}");
    let file = ClaudeMd::parse(theirs.as_bytes()).unwrap();
    assert!(file.imports_outside(IMPORT));
    assert!(!file.imports(IMPORT));

    let ours = imported(TEAM.as_bytes());
    let file = ClaudeMd::parse(&ours).unwrap();
    assert!(!file.imports_outside(IMPORT));
    assert!(file.imports(IMPORT));

    let both = imported(theirs.as_bytes());
    let file = ClaudeMd::parse(&both).unwrap();
    assert!(file.imports_outside(IMPORT));
    assert!(file.imports(IMPORT));
}
