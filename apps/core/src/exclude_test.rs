//! Unit tests for `exclude.rs`, kept beside it rather than inside it.

use super::*;

/// What `git init` puts in `info/exclude`: the lines a block is written after.
const TEMPLATE: &str = "\
# git ls-files --others --exclude-from=.git/info/exclude
# Lines that start with '#' are comments.
# For a project mostly in C, the following would be a good set of
# exclude patterns (uncomment them if you want to use them):
# *.[oa]
# *~
";

fn patterns(lines: &[&str]) -> Vec<String> {
    lines.iter().map(|line| (*line).to_owned()).collect()
}

fn text(file: &ExcludeFile) -> String {
    String::from_utf8(file.to_bytes()).unwrap()
}

#[test]
fn an_absent_file_gains_a_block_holding_the_group() {
    let mut file = ExcludeFile::parse(b"").unwrap();
    let changed = file
        .add("collapse", &patterns(&["/.agents/skills/commit-style/"]))
        .unwrap();
    assert!(changed);
    assert_eq!(
        text(&file),
        "# mindflayer:begin\n# project collapse\n/.agents/skills/commit-style/\n# mindflayer:end\n"
    );
}

#[test]
fn the_block_goes_after_what_was_there_and_leaves_it_byte_for_byte() {
    let mut file = ExcludeFile::parse(TEMPLATE.as_bytes()).unwrap();
    file.add("collapse", &patterns(&["/a/"])).unwrap();
    let written = text(&file);
    assert_eq!(
        written,
        format!("{TEMPLATE}# mindflayer:begin\n# project collapse\n/a/\n# mindflayer:end\n")
    );

    let mut file = ExcludeFile::parse(written.as_bytes()).unwrap();
    assert!(file.remove("collapse", &patterns(&["/a/"])).unwrap());
    assert_eq!(text(&file), TEMPLATE);
}

#[test]
fn lines_after_the_block_stay_where_they_are() {
    let before = "*.log\n# mindflayer:begin\n# project a\n/a/\n# mindflayer:end\ntarget/\n";
    let mut file = ExcludeFile::parse(before.as_bytes()).unwrap();
    file.add("a", &patterns(&["/b/"])).unwrap();
    assert_eq!(
        text(&file),
        "*.log\n# mindflayer:begin\n# project a\n/a/\n/b/\n# mindflayer:end\ntarget/\n"
    );

    file.remove("a", &patterns(&["/a/", "/b/"])).unwrap();
    assert_eq!(text(&file), "*.log\ntarget/\n");
}

#[test]
fn a_file_without_a_final_newline_keeps_ending_without_one() {
    let mut file = ExcludeFile::parse(b"*.log").unwrap();
    file.add("a", &patterns(&["/a/"])).unwrap();
    let written = text(&file);
    assert_eq!(
        written,
        "*.log\n# mindflayer:begin\n# project a\n/a/\n# mindflayer:end"
    );

    let mut file = ExcludeFile::parse(written.as_bytes()).unwrap();
    file.add("a", &patterns(&["/b/"])).unwrap();
    assert_eq!(
        text(&file),
        "*.log\n# mindflayer:begin\n# project a\n/a/\n/b/\n# mindflayer:end"
    );
    file.remove("a", &patterns(&["/a/", "/b/"])).unwrap();
    assert_eq!(file.to_bytes(), b"*.log");
}

#[test]
fn an_empty_file_gets_a_block_ending_in_a_newline_and_is_empty_again() {
    let mut file = ExcludeFile::parse(b"").unwrap();
    file.add("a", &patterns(&["/a/"])).unwrap();
    assert!(text(&file).ends_with("# mindflayer:end\n"));
    file.remove("a", &patterns(&["/a/"])).unwrap();
    assert_eq!(file.to_bytes(), b"");
}

#[test]
fn a_crlf_file_gets_crlf_lines_and_keeps_its_own() {
    let before = "*.log\r\ntarget/\r\n";
    let mut file = ExcludeFile::parse(before.as_bytes()).unwrap();
    file.add("a", &patterns(&["/a/"])).unwrap();
    let written = text(&file);
    assert_eq!(
        written,
        "*.log\r\ntarget/\r\n# mindflayer:begin\r\n# project a\r\n/a/\r\n# mindflayer:end\r\n"
    );

    let mut file = ExcludeFile::parse(written.as_bytes()).unwrap();
    file.add("a", &patterns(&["/b/"])).unwrap();
    file.remove("a", &patterns(&["/a/"])).unwrap();
    assert_eq!(
        text(&file),
        "*.log\r\ntarget/\r\n# mindflayer:begin\r\n# project a\r\n/b/\r\n# mindflayer:end\r\n"
    );
}

#[test]
fn bytes_that_are_not_utf8_survive_an_edit() {
    let before = b"caf\xe9/\n\xff\xfe\n";
    let mut file = ExcludeFile::parse(before).unwrap();
    file.add("a", &patterns(&["/a/"])).unwrap();
    file.remove("a", &patterns(&["/a/"])).unwrap();
    assert_eq!(file.to_bytes(), before);
}

#[test]
fn adding_what_is_there_changes_nothing() {
    let mut file = ExcludeFile::parse(b"").unwrap();
    file.add("a", &patterns(&["/a/", "/b/"])).unwrap();
    let once = file.to_bytes();
    assert!(!file.add("a", &patterns(&["/b/", "/a/"])).unwrap());
    assert_eq!(file.to_bytes(), once);
}

#[test]
fn a_pattern_named_twice_is_written_once() {
    let mut file = ExcludeFile::parse(b"").unwrap();
    file.add("a", &patterns(&["/a/", "/a/"])).unwrap();
    assert_eq!(
        text(&file),
        "# mindflayer:begin\n# project a\n/a/\n# mindflayer:end\n"
    );
}

#[test]
fn removing_what_is_not_there_changes_nothing() {
    let mut file = ExcludeFile::parse(TEMPLATE.as_bytes()).unwrap();
    assert!(!file.remove("a", &patterns(&["/a/"])).unwrap());
    file.add("a", &patterns(&["/a/"])).unwrap();
    let once = file.to_bytes();
    assert!(!file.remove("a", &patterns(&["/b/"])).unwrap());
    assert!(!file.remove("b", &patterns(&["/a/"])).unwrap());
    assert_eq!(file.to_bytes(), once);
}

#[test]
fn one_projects_edits_never_touch_anothers_lines() {
    let mut file = ExcludeFile::parse(b"").unwrap();
    file.add("a", &patterns(&["/a/one/"])).unwrap();
    file.add("b", &patterns(&["/b/one/"])).unwrap();
    assert_eq!(
        text(&file),
        "# mindflayer:begin\n# project a\n/a/one/\n# project b\n/b/one/\n# mindflayer:end\n"
    );

    file.add("a", &patterns(&["/a/two/"])).unwrap();
    assert_eq!(
        text(&file),
        "# mindflayer:begin\n# project a\n/a/one/\n/a/two/\n# project b\n/b/one/\n# mindflayer:end\n"
    );

    file.remove("a", &patterns(&["/a/one/", "/a/two/"]))
        .unwrap();
    assert_eq!(
        text(&file),
        "# mindflayer:begin\n# project b\n/b/one/\n# mindflayer:end\n"
    );
}

#[test]
fn forgetting_a_group_takes_every_line_of_it_and_no_other_groups() {
    let mut file = ExcludeFile::parse(TEMPLATE.as_bytes()).unwrap();
    file.add("a", &patterns(&["/a/one/", "/a/two/"])).unwrap();
    file.add("b", &patterns(&["/b/one/"])).unwrap();

    assert!(file.forget("a").unwrap());
    assert_eq!(
        text(&file),
        format!("{TEMPLATE}# mindflayer:begin\n# project b\n/b/one/\n# mindflayer:end\n")
    );

    // The last group takes the block with it, giving back the file as it was.
    assert!(file.forget("b").unwrap());
    assert_eq!(text(&file), TEMPLATE);
    assert!(!file.forget("b").unwrap());
}

#[test]
fn forgetting_the_last_group_keeps_a_block_somebody_wrote_in() {
    let before = "# mindflayer:begin\n/kept/\n# project a\n/a/\n# mindflayer:end\n";
    let mut file = ExcludeFile::parse(before.as_bytes()).unwrap();
    assert!(file.forget("a").unwrap());
    assert_eq!(
        text(&file),
        "# mindflayer:begin\n/kept/\n# mindflayer:end\n"
    );
}

#[test]
fn a_groups_lines_are_read_as_written_without_its_blanks() {
    let before =
        "# mindflayer:begin\n# project a\n/a/\n\n/b/\n# project b\n/c/\n# mindflayer:end\n";
    let file = ExcludeFile::parse(before.as_bytes()).unwrap();
    assert_eq!(file.lines_of("a"), patterns(&["/a/", "/b/"]));
    assert_eq!(file.lines_of("b"), patterns(&["/c/"]));
    assert!(file.lines_of("c").is_empty());
    assert!(ExcludeFile::parse(b"").unwrap().lines_of("a").is_empty());
}

#[test]
fn the_same_pattern_in_two_groups_is_each_groups_own() {
    // Two projects can want one path kept out: removing it from one must
    // leave it excluded for the other.
    let mut file = ExcludeFile::parse(b"").unwrap();
    file.add("a", &patterns(&["/shared/"])).unwrap();
    file.add("b", &patterns(&["/shared/"])).unwrap();
    file.remove("a", &patterns(&["/shared/"])).unwrap();
    assert_eq!(
        text(&file),
        "# mindflayer:begin\n# project b\n/shared/\n# mindflayer:end\n"
    );
}

#[test]
fn lines_in_the_block_before_any_group_keep_it() {
    // Mindflayer never writes them, so they are somebody's.
    let before = "# mindflayer:begin\n/kept/\n# project a\n/a/\n# mindflayer:end\n";
    let mut file = ExcludeFile::parse(before.as_bytes()).unwrap();
    file.remove("a", &patterns(&["/a/"])).unwrap();
    assert_eq!(
        text(&file),
        "# mindflayer:begin\n/kept/\n# mindflayer:end\n"
    );
}

#[test]
fn markers_may_trail_whitespace_but_not_lead_it() {
    let trailing = "# mindflayer:begin  \n# project a \n/a/\n# mindflayer:end\t\n";
    let mut file = ExcludeFile::parse(trailing.as_bytes()).unwrap();
    file.remove("a", &patterns(&["/a/"])).unwrap();
    assert_eq!(text(&file), "");

    // Git reads an indented `#` as a pattern, not a comment, so this is no
    // marker and there is no block to close.
    let indented = " # mindflayer:begin\n";
    let mut file = ExcludeFile::parse(indented.as_bytes()).unwrap();
    file.add("a", &patterns(&["/a/"])).unwrap();
    assert!(text(&file).starts_with(indented));
}

#[test]
fn a_begin_without_an_end_is_malformed() {
    let error = ExcludeFile::parse(b"*.log\n# mindflayer:begin\n# project a\n/a/\n").unwrap_err();
    assert_eq!(error, Malformed::Unclosed { line: 2 });
    assert_eq!(
        error.to_string(),
        "line 2: `# mindflayer:begin` is never closed by `# mindflayer:end`"
    );
}

#[test]
fn an_end_without_a_begin_is_malformed() {
    assert_eq!(
        ExcludeFile::parse(b"/a/\n# mindflayer:end\n").unwrap_err(),
        Malformed::Unopened { line: 2 }
    );
    assert_eq!(
        ExcludeFile::parse(b"# mindflayer:begin\n# mindflayer:end\n# mindflayer:end\n")
            .unwrap_err(),
        Malformed::Unopened { line: 3 }
    );
}

#[test]
fn a_second_block_is_malformed() {
    assert_eq!(
        ExcludeFile::parse(b"# mindflayer:begin\n# mindflayer:begin\n# mindflayer:end\n")
            .unwrap_err(),
        Malformed::Reopened { line: 2 }
    );
    assert_eq!(
        ExcludeFile::parse(
            b"# mindflayer:begin\n# mindflayer:end\n# mindflayer:begin\n# mindflayer:end\n"
        )
        .unwrap_err(),
        Malformed::Reopened { line: 3 }
    );
}

#[test]
fn a_project_with_two_groups_is_malformed() {
    let before = "# mindflayer:begin\n# project a\n/a/\n# project a\n/b/\n# mindflayer:end\n";
    assert_eq!(
        ExcludeFile::parse(before.as_bytes()).unwrap_err(),
        Malformed::Regrouped {
            line: 4,
            key: "a".to_owned()
        }
    );
}

#[test]
fn a_key_that_is_not_one_clean_line_is_refused() {
    let mut file = ExcludeFile::parse(b"").unwrap();
    for key in ["", "a\nb", "a\r", " a", "a "] {
        assert!(
            matches!(
                file.add(key, &patterns(&["/a/"])),
                Err(ExcludeError::Key { .. })
            ),
            "{key:?} was accepted"
        );
    }
    assert_eq!(file.to_bytes(), b"");
}

#[test]
fn a_pattern_that_is_not_one_line_is_refused() {
    let mut file = ExcludeFile::parse(b"").unwrap();
    let error = file.add("a", &patterns(&["/a/", "/b/\n/c/"])).unwrap_err();
    assert!(matches!(error, ExcludeError::Pattern { .. }));
    assert_eq!(file.to_bytes(), b"");
}

#[test]
fn a_pattern_that_would_read_back_as_a_comment_or_a_blank_is_refused() {
    // An end marker would close the block early and a header would start
    // another project's group, so the file would no longer read back as what
    // was meant to be written.
    let mut file = ExcludeFile::parse(b"").unwrap();
    for pattern in [
        "# mindflayer:end",
        "# mindflayer:begin",
        "# project b",
        "#note",
        "   ",
    ] {
        assert!(
            matches!(
                file.add("a", &patterns(&["/a/", pattern])),
                Err(ExcludeError::Pattern { .. })
            ),
            "{pattern:?} was accepted"
        );
    }
    assert_eq!(file.to_bytes(), b"");

    // Escaped, a leading `#` is a pattern like any other.
    assert!(file.add("a", &patterns(&[r"\#notes"])).unwrap());
}

#[test]
fn a_project_at_the_root_anchors_its_entries_there() {
    let entry = Entry::directory(".agents/skills/commit-style");
    assert_eq!(
        entry.pattern(Path::new("")).unwrap(),
        "/.agents/skills/commit-style/"
    );
}

#[test]
fn a_nested_project_anchors_its_entries_at_the_work_tree_root() {
    let route = Path::new("packages/app");
    assert_eq!(
        Entry::directory(".agents/skills/commit-style")
            .pattern(route)
            .unwrap(),
        "/packages/app/.agents/skills/commit-style/"
    );
    assert_eq!(
        Entry::file("AGENTS.md").pattern(route).unwrap(),
        "/packages/app/AGENTS.md"
    );
}

#[test]
fn single_dots_say_nothing() {
    assert_eq!(
        Entry::directory("./.agents/./skills/x")
            .pattern(Path::new("./packages/app"))
            .unwrap(),
        "/packages/app/.agents/skills/x/"
    );
}

#[test]
fn glob_characters_are_written_literally() {
    assert_eq!(
        Entry::directory("a*b?")
            .pattern(Path::new("apps/[legacy]"))
            .unwrap(),
        r"/apps/\[legacy]/a\*b\?/"
    );
}

#[cfg(unix)]
#[test]
fn a_backslash_in_a_name_is_escaped() {
    assert_eq!(
        Entry::file(r"a\b").pattern(Path::new("")).unwrap(),
        r"/a\\b"
    );
}

#[test]
fn trailing_spaces_on_a_file_are_kept() {
    assert_eq!(
        Entry::file("notes  ").pattern(Path::new("my app")).unwrap(),
        r"/my app/notes\ \ "
    );
}

#[test]
fn an_entry_that_leaves_the_project_is_refused() {
    for path in ["../elsewhere", "skills/../../elsewhere", "", "."] {
        assert!(
            matches!(
                Entry::directory(path).pattern(Path::new("app")),
                Err(ExcludeError::Path { .. })
            ),
            "{path:?} was accepted"
        );
    }
    let absolute = std::env::temp_dir().join("elsewhere");
    assert!(matches!(
        Entry::file(absolute).pattern(Path::new("")),
        Err(ExcludeError::Path { .. })
    ));
}

#[test]
fn a_route_that_climbs_is_refused() {
    assert!(matches!(
        Entry::file("AGENTS.md").pattern(Path::new("../app")),
        Err(ExcludeError::Path { .. })
    ));
}

#[test]
fn a_name_with_a_line_break_is_refused() {
    assert!(matches!(
        Entry::directory("a\nb").pattern(Path::new("")),
        Err(ExcludeError::Path { .. })
    ));
}
