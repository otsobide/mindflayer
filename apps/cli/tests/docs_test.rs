//! The README and every page under `docs/`, held to their relative links: a
//! file one names is there, an anchor names a heading of the page it points
//! into, and every image in `docs/images` is shown somewhere.
//!
//! A test of this crate's rather than a tool of the panel's, so it runs
//! wherever the Rust suites do, CI's `test (cli)` among them, with neither
//! Node nor the network: a link checker from npm would need both, and one
//! that also fetched every `https:` link would fail on somebody else's
//! outage. A link to another site is left alone for that reason.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Component, Path, PathBuf};

/// The repository, two folders above this crate.
fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("apps/cli sits two folders below the repository")
        .to_path_buf()
}

/// The README and every Markdown page under `docs/`, as routes from the
/// repository.
fn pages(root: &Path) -> Vec<PathBuf> {
    let mut pages = vec![PathBuf::from("README.md")];
    let mut pending = vec![root.join("docs")];
    while let Some(folder) = pending.pop() {
        for entry in fs::read_dir(&folder).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                pending.push(path);
            } else if path.extension().is_some_and(|extension| extension == "md") {
                pages.push(path.strip_prefix(root).unwrap().to_path_buf());
            }
        }
    }
    pages.sort();
    pages
}

/// `text` with every fenced code block and every code span blanked out,
/// newlines kept, so that what is left is the prose and its markup, each
/// on the line it was on. A link shown in code is an example, not a link,
/// and a `#` in a block is a shell comment, not a heading.
fn prose(text: &str) -> String {
    let mut kept = String::with_capacity(text.len());
    // The fence that opened the block the line is in: its character and how
    // many of it, which only as many or more of the same close.
    let mut fence: Option<(char, usize)> = None;
    let mut paragraph = String::new();
    for line in text.split_inclusive('\n') {
        let trimmed = line.trim_start_matches(' ');
        let indent = line.len() - trimmed.len();
        let opener = trimmed
            .chars()
            .next()
            .filter(|first| *first == '`' || *first == '~')
            .map(|first| (first, trimmed.chars().take_while(|c| *c == first).count()))
            .filter(|(_, run)| *run >= 3 && indent <= 3);
        match (fence, opener) {
            (None, Some(opened)) => {
                kept.push_str(&spans_blanked(&paragraph));
                paragraph.clear();
                fence = Some(opened);
                kept.push_str(&blank(line));
            }
            (Some((character, length)), Some((closing, run)))
                if closing == character
                    && run >= length
                    && trimmed.trim_end().chars().all(|c| c == character) =>
            {
                fence = None;
                kept.push_str(&blank(line));
            }
            (Some(_), _) => kept.push_str(&blank(line)),
            (None, None) => paragraph.push_str(line),
        }
    }
    kept.push_str(&spans_blanked(&paragraph));
    kept
}

/// `text` with every character but a newline made a space.
fn blank(text: &str) -> String {
    text.chars()
        .map(|c| if c == '\n' { '\n' } else { ' ' })
        .collect()
}

/// `text` with each code span, backticks included, blanked: a run of
/// backticks opens one, and only a run as long closes it, across lines as
/// a paragraph's may run. A run nothing closes is literal backticks.
fn spans_blanked(text: &str) -> String {
    let characters: Vec<char> = text.chars().collect();
    let mut kept = String::with_capacity(text.len());
    let mut at = 0;
    while at < characters.len() {
        if characters[at] != '`' {
            kept.push(characters[at]);
            at += 1;
            continue;
        }
        let run = characters[at..].iter().take_while(|c| **c == '`').count();
        let mut close = at + run;
        let closed = loop {
            match characters[close..].iter().position(|c| *c == '`') {
                None => break None,
                Some(offset) => {
                    let start = close + offset;
                    let length = characters[start..]
                        .iter()
                        .take_while(|c| **c == '`')
                        .count();
                    if length == run {
                        break Some(start + length);
                    }
                    close = start + length;
                }
            }
        };
        match closed {
            Some(end) => {
                kept.push_str(&blank(&characters[at..end].iter().collect::<String>()));
                at = end;
            }
            None => {
                kept.extend(&characters[at..at + run]);
                at += run;
            }
        }
    }
    kept
}

/// Every link `prose` holds, each with its line: a Markdown link's or an
/// image's target, a reference definition's, and an HTML `href`, `src` or
/// each image of a `srcset`.
fn links(prose: &str) -> Vec<(usize, String)> {
    let mut found = Vec::new();
    for (index, line) in prose.lines().enumerate() {
        let number = index + 1;
        let mut rest = line;
        while let Some(at) = rest.find("](") {
            let target = &rest[at + 2..];
            let end = target
                .find(|c: char| c == ')' || c.is_whitespace())
                .unwrap_or(target.len());
            found.push((number, target[..end].trim_matches(['<', '>']).to_owned()));
            rest = &target[end..];
        }
        let definition = line.trim_start();
        if let Some(label) = definition.strip_prefix('[') {
            if let Some((_, target)) = label.split_once("]:") {
                if let Some(target) = target.split_whitespace().next() {
                    found.push((number, target.trim_matches(['<', '>']).to_owned()));
                }
            }
        }
        for attribute in ["href=\"", "src=\"", "srcset=\""] {
            let mut rest = line;
            while let Some(at) = rest.find(attribute) {
                let value = &rest[at + attribute.len()..];
                let end = value.find('"').unwrap_or(value.len());
                for candidate in value[..end].split(',') {
                    if let Some(url) = candidate.split_whitespace().next() {
                        found.push((number, url.to_owned()));
                    }
                }
                rest = &value[end..];
            }
        }
    }
    found
}

/// The anchor GitHub gives a heading reading `text`: lowercased, every
/// character but a letter, a digit, a space, `-` or `_` dropped, and each
/// space a `-`. Markup is read as GitHub draws it first: a code span's
/// text, a link's words.
fn slug(text: &str) -> String {
    let mut drawn = String::new();
    let mut rest = text;
    // A link's words, without its target.
    while let Some(open) = rest.find('[') {
        match rest[open..].find("](") {
            Some(middle) => {
                let target = &rest[open + middle + 2..];
                let close = target.find(')').unwrap_or(target.len());
                drawn.push_str(&rest[..open]);
                drawn.push_str(&rest[open + 1..open + middle]);
                rest = &target[(close + 1).min(target.len())..];
            }
            None => break,
        }
    }
    drawn.push_str(rest);
    drawn
        .to_lowercase()
        .chars()
        .filter_map(|c| match c {
            ' ' => Some('-'),
            '-' | '_' => Some(c),
            c if c.is_alphanumeric() => Some(c),
            _ => None,
        })
        .collect()
}

/// Every anchor a page offers: each heading's, a repeated one numbered as
/// GitHub numbers it (`-1`, `-2`), and each `id` or `name` an HTML tag
/// gives.
fn anchors(prose: &str, text: &str) -> BTreeSet<String> {
    let mut found = BTreeSet::new();
    let mut seen: BTreeMap<String, usize> = BTreeMap::new();
    // Headings are read from the prose, so a `#` in a block is none, but
    // their words from the text, where a code span's are still there.
    for (blanked, line) in prose.lines().zip(text.lines()) {
        let level = blanked.chars().take_while(|c| *c == '#').count();
        if !(1..=6).contains(&level) || !blanked[level..].starts_with(' ') {
            continue;
        }
        let words = line[level..].trim().trim_end_matches('#').trim();
        let base = slug(&words.replace('`', ""));
        let count = seen.entry(base.clone()).or_insert(0);
        found.insert(if *count == 0 {
            base.clone()
        } else {
            format!("{base}-{count}")
        });
        *count += 1;
    }
    for attribute in ["id=\"", "name=\""] {
        let mut rest = prose;
        while let Some(at) = rest.find(attribute) {
            let value = &rest[at + attribute.len()..];
            let end = value.find('"').unwrap_or(value.len());
            found.insert(value[..end].to_owned());
            rest = &value[end..];
        }
    }
    found
}

/// `route` with `.` and `..` taken out, or none when it climbs above
/// where it starts.
fn lexical(route: &Path) -> Option<PathBuf> {
    let mut kept = PathBuf::new();
    for component in route.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if !kept.pop() {
                    return None;
                }
            }
            Component::Normal(name) => kept.push(name),
            Component::RootDir | Component::Prefix(_) => return None,
        }
    }
    Some(kept)
}

/// `%20` and its like read back as what they stand for.
fn decoded(target: &str) -> String {
    let bytes = target.as_bytes();
    let mut kept = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        let hex = bytes
            .get(at + 1..at + 3)
            .and_then(|pair| std::str::from_utf8(pair).ok())
            .and_then(|pair| u8::from_str_radix(pair, 16).ok());
        match (bytes[at], hex) {
            (b'%', Some(byte)) => {
                kept.push(byte);
                at += 3;
            }
            (byte, _) => {
                kept.push(byte);
                at += 1;
            }
        }
    }
    String::from_utf8_lossy(&kept).into_owned()
}

/// What is wrong with each relative link in the pages under `root`, and
/// every route from the repository a link reached.
fn broken(root: &Path) -> (Vec<String>, BTreeSet<PathBuf>) {
    let mut problems = Vec::new();
    let mut reached = BTreeSet::new();
    let mut offered: BTreeMap<PathBuf, BTreeSet<String>> = BTreeMap::new();
    let mut read = |page: &Path| -> BTreeSet<String> {
        offered
            .entry(page.to_path_buf())
            .or_insert_with(|| {
                let text = fs::read_to_string(root.join(page))
                    .unwrap()
                    .replace("\r\n", "\n");
                anchors(&prose(&text), &text)
            })
            .clone()
    };
    for page in pages(root) {
        // A checkout on Windows may have given the pages CRLF line endings.
        let text = fs::read_to_string(root.join(&page))
            .unwrap()
            .replace("\r\n", "\n");
        let folder = page.parent().unwrap_or(Path::new(""));
        for (line, target) in links(&prose(&text)) {
            let at = format!("{}:{line}: {target}", page.display());
            if target.is_empty() {
                problems.push(format!("{at}: an empty link"));
                continue;
            }
            if target.starts_with("//") || target.contains("://") || target.starts_with("mailto:") {
                continue;
            }
            let (path, anchor) = match target.split_once('#') {
                Some((path, anchor)) => (path, Some(anchor)),
                None => (target.as_str(), None),
            };
            let path = path.split('?').next().unwrap_or_default();
            let route = if path.is_empty() {
                page.clone()
            } else {
                match lexical(&folder.join(decoded(path))) {
                    Some(route) => route,
                    None => {
                        problems.push(format!("{at}: leads out of the repository"));
                        continue;
                    }
                }
            };
            if !root.join(&route).exists() {
                problems.push(format!("{at}: no {} in the repository", route.display()));
                continue;
            }
            reached.insert(route.clone());
            if let Some(anchor) = anchor {
                if route.extension().is_some_and(|extension| extension == "md")
                    && !read(&route).contains(&decoded(anchor))
                {
                    problems.push(format!(
                        "{at}: no heading in {} gives #{anchor}",
                        route.display()
                    ));
                }
            }
        }
    }
    (problems, reached)
}

#[test]
fn every_relative_link_in_the_readme_and_docs_leads_somewhere() {
    let (problems, _) = broken(&root());

    assert!(
        problems.is_empty(),
        "broken links:\n{}",
        problems.join("\n")
    );
}

#[test]
fn every_image_in_docs_images_is_shown_somewhere() {
    // `make docs/screenshots` writes every one of them, so one no page
    // shows is one the history carries for nothing.
    let root = root();
    let (_, reached) = broken(&root);
    let images = root.join("docs/images");
    let mut unshown = Vec::new();
    for entry in fs::read_dir(&images).unwrap() {
        let route = entry
            .unwrap()
            .path()
            .strip_prefix(&root)
            .unwrap()
            .to_path_buf();
        if !reached.contains(&route) {
            unshown.push(route.display().to_string());
        }
    }

    assert!(unshown.is_empty(), "shown by no page: {unshown:?}");
}

#[test]
fn a_heading_is_reached_by_the_anchor_github_gives_it() {
    let text = "# 🧠 Mindflayer\n\
                ## The agents' own folders\n\
                ### `flayer ui`\n\
                ## A repository that already has .claude/skills\n\
                ## Changing one\n\
                ## Changing one\n\
                ## Seen in [the TUI](#the-tui)\n\
                <a id=\"kept\"></a>\n";

    assert_eq!(
        anchors(&prose(text), text),
        BTreeSet::from(
            [
                "-mindflayer",
                "the-agents-own-folders",
                "flayer-ui",
                "a-repository-that-already-has-claudeskills",
                "changing-one",
                "changing-one-1",
                "seen-in-the-tui",
                "kept",
            ]
            .map(String::from)
        )
    );
}

#[test]
fn a_link_shown_as_code_is_no_link() {
    let text = "See [the reference](docs/reference.md#install) and `[not](a.md)`.\n\
                ```\n\
                [nor](b.md)\n\
                # nor a heading\n\
                ```\n\
                A span ``across `[lines](c.md)\n\
                still`` holds it, and <img src=\"docs/images/x.png\" alt=\"x\">.\n\
                [kept]: docs/agents.md\n";

    assert_eq!(
        links(&prose(text)),
        [
            (1, String::from("docs/reference.md#install")),
            (7, String::from("docs/images/x.png")),
            (8, String::from("docs/agents.md")),
        ]
    );
    assert!(anchors(&prose(text), text).is_empty());
}
