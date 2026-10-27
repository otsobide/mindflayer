//! Unit tests for `agents_md.rs`, kept beside it rather than inside it.

use super::*;

/// What a team's `AGENTS.md` might hold before Mindflayer writes in it.
const TEAM: &str = "\
# Working here

Run `make test` before every push.
";

fn section(route: &str, body: &str) -> Section {
    Section::new(route, body).unwrap()
}

fn text(file: &AgentsMd) -> String {
    String::from_utf8(file.to_bytes()).unwrap()
}

/// `file` with its block set to `sections`, written and read back, as two
/// renders a run apart would see it.
fn render(file: &[u8], sections: &[Section]) -> Vec<u8> {
    let mut parsed = AgentsMd::parse(file).unwrap();
    parsed.set_block(sections).unwrap();
    parsed.to_bytes()
}

#[test]
fn a_section_hashes_what_it_says_whatever_its_lines_end_with() {
    let said = section("review", "Say what a change does.\n\nThen what is wrong.\n");

    assert_eq!(
        said.hash(),
        section(
            "review",
            "\r\nSay what a change does.\r\n\r\nThen what is wrong.\r\n"
        )
        .hash(),
        "another ending, and blank lines around the body"
    );
    assert_eq!(
        said.hash(),
        section("other", "Say what a change does.\n\nThen what is wrong.").hash(),
        "another route"
    );
    assert_eq!(
        said.hash(),
        crate::hash::bytes(b"Say what a change does.\n\nThen what is wrong.\n")
    );
    assert_ne!(
        said.hash(),
        section("review", "Say what a change does.\nThen what is wrong.\n").hash()
    );
}

#[test]
fn an_absent_file_gains_a_block_holding_one_section() {
    let mut file = AgentsMd::parse(b"").unwrap();
    let changed = file
        .set_block(&[section("no-force-push", "Never force-push.\n")])
        .unwrap();
    assert!(changed);
    assert_eq!(
        text(&file),
        "<!-- mindflayer:begin -->\n\
         <!-- mindflayer:rule no-force-push -->\n\
         Never force-push.\n\
         <!-- mindflayer:end -->\n"
    );
}

#[test]
fn sections_are_one_blank_line_apart_and_bodies_lose_their_outer_blank_lines() {
    let mut file = AgentsMd::parse(b"").unwrap();
    file.set_block(&[
        section("git/no-force-push", "\n\nNever force-push.\n\n\n"),
        section("review/tone", "# Tone\n\nKind first.  \nThen precise."),
    ])
    .unwrap();
    assert_eq!(
        text(&file),
        "<!-- mindflayer:begin -->\n\
         <!-- mindflayer:rule git/no-force-push -->\n\
         Never force-push.\n\
         \n\
         <!-- mindflayer:rule review/tone -->\n\
         # Tone\n\
         \n\
         Kind first.  \n\
         Then precise.\n\
         <!-- mindflayer:end -->\n"
    );
}

#[test]
fn the_block_goes_after_what_was_there_and_leaves_it_byte_for_byte() {
    let rendered = render(TEAM.as_bytes(), &[section("a", "Rule a.")]);
    let written = String::from_utf8(rendered.clone()).unwrap();
    assert_eq!(
        written,
        format!("{TEAM}\n{BEGIN}\n{RULE}a{CLOSE}\nRule a.\n{END}\n")
    );

    let mut again = AgentsMd::parse(&rendered).unwrap();
    assert!(!again.set_block(&[section("a", "Rule a.")]).unwrap());
    assert_eq!(again.to_bytes(), rendered);

    assert_eq!(render(&rendered, &[]), TEAM.as_bytes());
}

#[test]
fn a_file_ending_in_a_blank_line_is_still_set_apart_and_given_back_as_it_was() {
    let before = "Team.\n\n";
    let rendered = render(before.as_bytes(), &[section("a", "Rule a.")]);
    assert_eq!(
        String::from_utf8(rendered.clone()).unwrap(),
        format!("Team.\n\n\n{BEGIN}\n{RULE}a{CLOSE}\nRule a.\n{END}\n")
    );
    assert_eq!(render(&rendered, &[]), before.as_bytes());
}

#[test]
fn an_empty_line_above_the_block_goes_with_it_and_a_line_of_spaces_stays() {
    let block = format!("{BEGIN}\n{RULE}a{CLOSE}\nRule a.\n{END}\n");
    let spaced = format!("Team.\n\n{block}");
    let mut again = AgentsMd::parse(spaced.as_bytes()).unwrap();
    assert!(!again.set_block(&[section("a", "Rule a.")]).unwrap());
    assert_eq!(text(&again), spaced);
    assert_eq!(render(spaced.as_bytes(), &[]), b"Team.\n");

    let kept = format!("Team.\n  \n{block}");
    assert_eq!(render(kept.as_bytes(), &[]), b"Team.\n  \n");
}

#[test]
fn text_after_the_block_stays_where_it_is() {
    let before = format!("{TEAM}{BEGIN}\n{RULE}a{CLOSE}\nRule a.\n{END}\n\n## Later\n\nMore.\n");
    let rendered = render(before.as_bytes(), &[section("b", "Rule b.")]);
    assert_eq!(
        String::from_utf8(rendered.clone()).unwrap(),
        format!("{TEAM}{BEGIN}\n{RULE}b{CLOSE}\nRule b.\n{END}\n\n## Later\n\nMore.\n")
    );
    assert_eq!(
        String::from_utf8(render(&rendered, &[])).unwrap(),
        format!("{TEAM}\n## Later\n\nMore.\n")
    );
}

#[test]
fn a_file_without_a_final_newline_keeps_ending_without_one() {
    let rendered = render(b"Be brief.", &[section("a", "Rule a.")]);
    assert_eq!(
        String::from_utf8(rendered.clone()).unwrap(),
        format!("Be brief.\n\n{BEGIN}\n{RULE}a{CLOSE}\nRule a.\n{END}")
    );
    let again = render(
        &rendered,
        &[section("a", "Rule a."), section("b", "Rule b.")],
    );
    assert!(again.ends_with(END.as_bytes()));
    assert_eq!(render(&again, &[]), b"Be brief.");
}

#[test]
fn an_empty_file_gets_a_block_ending_in_a_newline_and_is_empty_again() {
    let rendered = render(b"", &[section("a", "Rule a.")]);
    assert!(rendered.ends_with(format!("{END}\n").as_bytes()));
    let emptied = AgentsMd::parse(&render(&rendered, &[])).unwrap();
    assert_eq!(emptied.to_bytes(), b"");
    assert!(emptied.is_blank_outside());
}

#[test]
fn a_crlf_file_gets_crlf_lines_and_keeps_its_own() {
    let before = "# Team\r\n\r\nBe brief.\r\n";
    let rendered = render(before.as_bytes(), &[section("a", "One.\nTwo.\n")]);
    assert_eq!(
        String::from_utf8(rendered.clone()).unwrap(),
        format!("{before}\r\n{BEGIN}\r\n{RULE}a{CLOSE}\r\nOne.\r\nTwo.\r\n{END}\r\n")
    );

    let mut again = AgentsMd::parse(&rendered).unwrap();
    assert!(!again.set_block(&[section("a", "One.\nTwo.\n")]).unwrap());
    assert_eq!(render(&rendered, &[]), before.as_bytes());
}

#[test]
fn a_crlf_body_is_written_with_the_files_endings() {
    let rendered = render(b"Team.\n", &[section("a", "One.\r\nTwo.\r\n")]);
    assert_eq!(
        String::from_utf8(rendered).unwrap(),
        format!("Team.\n\n{BEGIN}\n{RULE}a{CLOSE}\nOne.\nTwo.\n{END}\n")
    );
}

#[test]
fn bytes_that_are_not_utf8_survive_a_render_and_a_removal() {
    let before = b"caf\xe9\n\xff\xfe\n";
    let rendered = render(before, &[section("a", "Rule a.")]);
    assert!(rendered.starts_with(before));
    assert_eq!(render(&rendered, &[]), before);
}

#[test]
fn a_section_reads_back_as_the_one_rendered_and_an_edit_with_it() {
    let sections = [
        section("a", "Rule a.\n\nIts second paragraph.\n"),
        section("c/d", "Rule d.\r\n"),
    ];
    let rendered = render(TEAM.as_bytes(), &sections);
    let file = AgentsMd::parse(&rendered).unwrap();

    assert_eq!(file.section("a"), Some(sections[0].clone()));
    assert_eq!(file.section("c/d"), Some(sections[1].clone()));
    assert_eq!(file.section("b"), None);
    assert_eq!(AgentsMd::parse(TEAM.as_bytes()).unwrap().section("a"), None);

    let edited = String::from_utf8(rendered)
        .unwrap()
        .replace("Rule d.", "Rule d, edited by hand.");
    let file = AgentsMd::parse(edited.as_bytes()).unwrap();
    assert_eq!(
        file.section("c/d"),
        Some(section("c/d", "Rule d, edited by hand."))
    );
    let mut unreadable = edited.into_bytes();
    let at = unreadable.windows(4).position(|w| w == b"hand").unwrap();
    unreadable[at] = 0xff;
    assert_eq!(AgentsMd::parse(&unreadable).unwrap().section("c/d"), None);
}

#[test]
fn the_block_is_replaced_whole_and_its_routes_read_back() {
    let before = format!(
        "{TEAM}{BEGIN}\n{RULE}a{CLOSE}\nRule a, edited by hand.\nA line of somebody's.\n{END}\n"
    );
    let file = AgentsMd::parse(before.as_bytes()).unwrap();
    assert_eq!(file.routes(), ["a"]);

    let rendered = render(
        before.as_bytes(),
        &[section("a", "Rule a."), section("c/d", "Rule d.")],
    );
    let file = AgentsMd::parse(&rendered).unwrap();
    assert_eq!(file.routes(), ["a", "c/d"]);
    assert_eq!(
        String::from_utf8(rendered).unwrap(),
        format!("{TEAM}{BEGIN}\n{RULE}a{CLOSE}\nRule a.\n\n{RULE}c/d{CLOSE}\nRule d.\n{END}\n")
    );
}

#[test]
fn markers_forgive_trailing_whitespace_and_are_kept_as_written() {
    let before = format!("{BEGIN}  \n{RULE}a{CLOSE}\t\nRule a.\n{END} \n");
    let mut file = AgentsMd::parse(before.as_bytes()).unwrap();
    assert_eq!(file.routes(), ["a"]);
    file.set_block(&[section("b", "Rule b.")]).unwrap();
    assert_eq!(
        text(&file),
        format!("{BEGIN}  \n{RULE}b{CLOSE}\nRule b.\n{END} \n")
    );
}

#[test]
fn prose_that_names_a_marker_is_text() {
    let before = format!("Mindflayer writes `{BEGIN}` above its rules.\n> {END}\n");
    let file = AgentsMd::parse(before.as_bytes()).unwrap();
    assert!(!file.has_block());
}

#[test]
fn a_file_without_a_block_and_no_sections_is_left_alone() {
    let mut file = AgentsMd::parse(TEAM.as_bytes()).unwrap();
    assert!(!file.set_block(&[]).unwrap());
    assert_eq!(text(&file), TEAM);
    assert!(!file.is_blank_outside());
}

#[test]
fn a_block_left_open_is_refused() {
    let before = format!("{TEAM}{BEGIN}\n{RULE}a{CLOSE}\nRule a.\n");
    assert_eq!(
        AgentsMd::parse(before.as_bytes()),
        Err(Malformed::Unclosed { line: 4 })
    );
}

#[test]
fn an_end_marker_that_closes_nothing_is_refused() {
    let before = format!("{TEAM}{END}\n");
    assert_eq!(
        AgentsMd::parse(before.as_bytes()),
        Err(Malformed::Unopened { line: 4 })
    );
    let twice = format!("{BEGIN}\n{END}\n{END}\n");
    assert_eq!(
        AgentsMd::parse(twice.as_bytes()),
        Err(Malformed::Unopened { line: 3 })
    );
}

#[test]
fn a_second_block_is_refused() {
    let before = format!("{BEGIN}\n{END}\nTeam.\n{BEGIN}\n{END}\n");
    assert_eq!(
        AgentsMd::parse(before.as_bytes()),
        Err(Malformed::Reopened { line: 4 })
    );
    let nested = format!("{BEGIN}\n{BEGIN}\n{END}\n");
    assert_eq!(
        AgentsMd::parse(nested.as_bytes()),
        Err(Malformed::Reopened { line: 2 })
    );
}

#[test]
fn a_rules_marker_outside_the_block_is_refused() {
    let before = format!("{RULE}a{CLOSE}\n{BEGIN}\n{END}\n");
    assert_eq!(
        AgentsMd::parse(before.as_bytes()),
        Err(Malformed::Stray { line: 1 })
    );
    let after = format!("{BEGIN}\n{END}\n{RULE}a{CLOSE}\n");
    assert_eq!(
        AgentsMd::parse(after.as_bytes()),
        Err(Malformed::Stray { line: 3 })
    );
}

#[test]
fn a_second_section_for_one_route_is_refused() {
    let before = format!("{BEGIN}\n{RULE}a{CLOSE}\nOne.\n{RULE}a{CLOSE}\nTwo.\n{END}\n");
    assert_eq!(
        AgentsMd::parse(before.as_bytes()),
        Err(Malformed::Repeated {
            line: 4,
            route: String::from("a"),
        })
    );
}

#[test]
fn a_marker_written_any_other_way_is_refused() {
    for miswritten in [
        "  <!-- mindflayer:begin -->",
        "<!--mindflayer:begin-->",
        "<!-- mindflayer:begin",
        "<!-- Mindflayer:end -->",
        "<!-- mindflayer:rule -->",
        "<!-- mindflayer:rule  a -->",
        "<!-- mindflayer:rule Not A Name -->",
        "<!-- mindflayer:rule a/../b -->",
        "<!-- mindflayer:import -->",
        "Text, then <!-- mindflayer:end --> after it.",
    ] {
        let before = format!("{BEGIN}\n{miswritten}\n{END}\n");
        let parsed = AgentsMd::parse(before.as_bytes());
        // The last one is text: it opens with no comment.
        if miswritten.starts_with("Text") {
            assert!(parsed.is_ok(), "{miswritten:?}");
        } else {
            assert_eq!(
                parsed,
                Err(Malformed::Miswritten { line: 2 }),
                "{miswritten:?}"
            );
        }
    }
}

#[test]
fn a_route_that_is_no_name_is_refused() {
    for route in ["", "Not A Name", "a--b", "a/../b", "-a"] {
        assert!(
            matches!(
                Section::new(route, "Body."),
                Err(AgentsMdError::Route { .. })
            ),
            "{route:?}"
        );
    }
}

#[test]
fn a_body_line_that_reads_as_a_marker_is_refused() {
    for line in [
        END,
        BEGIN,
        "<!-- mindflayer:rule other -->",
        "  <!-- mindflayer:end -->",
    ] {
        let body = format!("Before.\n{line}\nAfter.\n");
        match Section::new("a", &body) {
            Err(AgentsMdError::Marked { route, line: said }) => {
                assert_eq!(route, "a");
                assert_eq!(said, line);
            }
            other => panic!("{line:?}: {other:?}"),
        }
    }
    // Quoted in prose it is text, as the parse reads it.
    assert!(Section::new("a", &format!("Write `{END}` last.")).is_ok());
}

#[test]
fn two_sections_for_one_route_are_refused_and_nothing_changes() {
    let mut file = AgentsMd::parse(TEAM.as_bytes()).unwrap();
    let refused = file.set_block(&[section("a", "One."), section("a", "Two.")]);
    assert!(matches!(refused, Err(AgentsMdError::Twice { route }) if route == "a"));
    assert_eq!(text(&file), TEAM);
}

#[test]
fn what_is_rendered_reads_back_as_itself() {
    let sections = [
        section("a", "---\nnot: front matter here\n---\nBody."),
        section("b/c", "\tIndented.\n\n\n  Spaced.  \n"),
        section("d", "Last line\r\r\n"),
    ];
    for ending in ["", "Team.\n", "Team.\r\n", "Team."] {
        let rendered = render(ending.as_bytes(), &sections);
        let mut again = AgentsMd::parse(&rendered).unwrap();
        assert_eq!(again.routes(), ["a", "b/c", "d"]);
        assert!(!again.set_block(&sections).unwrap(), "{ending:?}");
        assert_eq!(render(&rendered, &[]), ending.as_bytes());
    }
}
