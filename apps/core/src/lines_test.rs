//! Unit tests for `lines.rs`, kept beside it rather than inside it.
//!
//! What the module promises is that a file split into lines and written back
//! is the file it was, byte for byte: every edit of a block somebody else's
//! file holds is that rejoin, so a split that lost a `\r` or a last line
//! would rewrite every line outside the block. Random bytes try what no test
//! spells.

use super::*;

use crate::mother::random;

const DRAWS: usize = 200;

/// The bytes `lines` are written back as, as each editor of a block writes
/// them: each line's text, then the ending it had.
fn joined(lines: &[Line]) -> Vec<u8> {
    lines
        .iter()
        .flat_map(|line| line.text.iter().chain(line.end).copied())
        .collect()
}

/// Bytes rich in what a split cuts at: `\r`, `\n` and both together, among
/// bytes of any value.
fn ragged(length: usize) -> Vec<u8> {
    (0..length)
        .map(|_| match random::between(0, 5) {
            0 => b'\n',
            1 => b'\r',
            _ => random::number() as u8,
        })
        .collect()
}

#[test]
fn nothing_splits_into_no_lines() {
    assert_eq!(split(b""), Vec::<Line>::new());
}

#[test]
fn a_file_with_lf_endings_splits_into_its_lines_each_ending_in_lf() {
    assert_eq!(
        split(b"one\ntwo\n"),
        vec![Line::new("one", LF), Line::new("two", LF)]
    );
}

#[test]
fn a_file_with_crlf_endings_keeps_crlf_out_of_each_line_s_text() {
    assert_eq!(
        split(b"one\r\ntwo\r\n"),
        vec![Line::new("one", CRLF), Line::new("two", CRLF)]
    );
}

#[test]
fn a_last_line_without_an_ending_is_a_line_ending_in_nothing() {
    assert_eq!(
        split(b"one\ntwo"),
        vec![Line::new("one", LF), Line::new("two", b"")]
    );
}

#[test]
fn a_file_mixing_endings_keeps_each_line_s_own() {
    assert_eq!(
        split(b"a\r\nb\nc\r\nd"),
        vec![
            Line::new("a", CRLF),
            Line::new("b", LF),
            Line::new("c", CRLF),
            Line::new("d", b""),
        ]
    );
}

#[test]
fn empty_lines_are_lines_of_their_own() {
    assert_eq!(
        split(b"\n\r\n\n"),
        vec![Line::new("", LF), Line::new("", CRLF), Line::new("", LF)]
    );
}

/// A `\r` is part of an ending only right before the `\n`: one alone, or
/// one of two before it, is text a file holds and gives back.
#[test]
fn a_carriage_return_not_right_before_a_newline_is_text() {
    assert_eq!(
        split(b"a\rb\n\r\r\nc\r"),
        vec![
            Line {
                text: b"a\rb".to_vec(),
                end: LF,
            },
            Line {
                text: b"\r".to_vec(),
                end: CRLF,
            },
            Line {
                text: b"c\r".to_vec(),
                end: b"",
            },
        ]
    );
}

#[test]
fn bytes_that_are_not_utf_8_are_kept_as_they_are() {
    let bytes = b"\xff\xfe caf\xe9\r\n\x80\n";

    assert_eq!(
        split(bytes),
        vec![
            Line {
                text: b"\xff\xfe caf\xe9".to_vec(),
                end: CRLF,
            },
            Line {
                text: b"\x80".to_vec(),
                end: LF,
            },
        ]
    );
}

#[test]
fn any_bytes_split_and_joined_again_are_the_bytes_they_were() {
    for _ in 0..DRAWS {
        let bytes = random::pick(&[ragged(random::between(0, 80)), random::bytes(80)]);

        assert_eq!(joined(&split(&bytes)), bytes, "{bytes:?}");
    }
}

#[test]
fn no_line_split_off_holds_a_newline_in_its_text() {
    for _ in 0..DRAWS {
        let bytes = ragged(random::between(0, 80));

        for line in split(&bytes) {
            assert!(!line.text.contains(&b'\n'), "{bytes:?} gave {line:?}");
        }
    }
}

/// Only the last line may end in nothing, and only when the file does not
/// end in a newline: a line in the middle that lost its ending would be
/// joined to the next when written back.
#[test]
fn only_the_last_line_ends_in_nothing_and_only_when_the_file_does() {
    for _ in 0..DRAWS {
        let bytes = ragged(random::between(1, 80));
        let lines = split(&bytes);
        let (last, rest) = lines.split_last().expect("bytes give a line");

        assert!(
            rest.iter().all(|line| !line.end.is_empty()),
            "{bytes:?} gave {lines:?}"
        );
        assert_eq!(
            last.end.is_empty(),
            bytes.last() != Some(&b'\n'),
            "{bytes:?} gave {lines:?}"
        );
    }
}

#[test]
fn a_file_splits_into_as_many_lines_as_it_has_newlines_and_a_last_line_without_one() {
    for _ in 0..DRAWS {
        let bytes = ragged(random::between(0, 80));
        let newlines = bytes.iter().filter(|&&byte| byte == b'\n').count();
        let unended = usize::from(!bytes.is_empty() && bytes.last() != Some(&b'\n'));

        assert_eq!(split(&bytes).len(), newlines + unended, "{bytes:?}");
    }
}

#[test]
fn a_file_s_ending_is_the_first_one_it_has() {
    assert_eq!(ending(&split(b"a\r\nb\nc\n")), CRLF);
    assert_eq!(ending(&split(b"a\nb\r\nc\r\n")), LF);
}

/// A last line ending in nothing says nothing of how the file ends its lines,
/// so the ending is looked for past it, or is LF.
#[test]
fn a_line_ending_in_nothing_is_passed_over_for_the_ending() {
    assert_eq!(
        ending(&[Line::new("a", b""), Line::new("b", CRLF)]),
        CRLF,
        "an ending is the first that is one"
    );
    assert_eq!(ending(&split(b"only")), LF);
}

#[test]
fn a_file_with_no_ending_at_all_gains_lines_in_lf() {
    assert_eq!(ending(&[]), LF);
    assert_eq!(ending(&split(b"")), LF);
}

#[test]
fn the_ending_of_any_file_is_lf_or_crlf_and_never_nothing() {
    for _ in 0..DRAWS {
        let bytes = ragged(random::between(0, 80));
        let lines = split(&bytes);
        let expected = lines
            .iter()
            .map(|line| line.end)
            .find(|end| !end.is_empty())
            .unwrap_or(LF);

        assert_eq!(ending(&lines), expected, "{bytes:?}");
        assert!([LF, CRLF].contains(&ending(&lines)), "{bytes:?}");
    }
}

#[test]
fn a_line_of_spaces_and_tabs_is_blank() {
    for text in ["", " ", "\t", " \t  ", "\u{c}"] {
        assert!(Line::new(text, LF).is_blank(), "{text:?}");
    }
}

/// A `\r` left in a line's text, as a lone one before a `\n` that ended in
/// CRLF leaves, is ASCII whitespace, and blank with the rest.
#[test]
fn a_line_holding_only_a_carriage_return_is_blank() {
    let lines = split(b"\r\r\n");

    assert_eq!(lines.len(), 1);
    assert!(lines[0].is_blank(), "{lines:?}");
}

#[test]
fn a_line_with_anything_but_ascii_whitespace_is_not_blank() {
    for text in ["#", " x ", "\t-", "\u{a0}", "\u{2003}"] {
        assert!(!Line::new(text, LF).is_blank(), "{text:?}");
    }
}

/// A non-breaking space, or a byte that is not UTF-8, is no ASCII
/// whitespace: a line holding one is somebody's and never blank.
#[test]
fn a_line_of_bytes_that_are_not_utf_8_is_not_blank() {
    let line = Line {
        text: vec![0xa0],
        end: LF,
    };

    assert!(!line.is_blank());
}

#[test]
fn a_line_is_made_from_its_text_and_the_ending_it_is_given() {
    for _ in 0..DRAWS {
        let text = random::text(20);
        let end = random::pick(&[LF, CRLF, b"" as &'static [u8]]);
        let line = Line::new(&text, end);

        assert_eq!(line.text, text.as_bytes(), "{text:?}");
        assert_eq!(line.end, end, "{text:?}");
    }
}
