//! Drawing the install screen.
//!
//! Two columns: the projects a workspace manages, and, for whichever project
//! the cursor is on, everything of the kind shown that can be installed,
//! ticked when that project already holds it. Turned round by artifact, the
//! other way: everything of the kind shown, and for whichever the cursor is
//! on, every project, ticked where it holds it. Nothing here decides
//! anything; [`super::state`] does.

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Clear, List, ListItem, ListState, Paragraph, Wrap};
use ratatui::Frame;

use mindflayer_core::Kind;

use super::state::{nothing, Focus, Pending, Row, Screen, Target};
use crate::commands::install::named;

/// Draw the whole screen.
pub fn draw(screen: &Screen, frame: &mut Frame) {
    let area = frame.area();
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(3), Constraint::Length(2)])
        .split(area);
    // Turned round, the left column holds what a project's offers column
    // does, a summary and an origin a row, so it is given as much room as
    // the projects beside it.
    let left = if screen.focus.by_artifact() { 50 } else { 30 };
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(left),
            Constraint::Percentage(100 - left),
        ])
        .split(rows[0]);

    if screen.focus.by_artifact() {
        artifacts(screen, frame, columns[0]);
        receivers(screen, frame, columns[1]);
    } else {
        projects(screen, frame, columns[0]);
        offers(screen, frame, columns[1]);
    }
    footer(screen, frame, rows[1]);

    if screen.confirming {
        confirm(screen, frame, area);
    }
}

/// The left column: one line per project, with how many of its boxes were
/// touched, because that is what tells you where you have been.
fn projects(screen: &Screen, frame: &mut Frame, area: Rect) {
    let items: Vec<ListItem> = screen
        .targets
        .iter()
        .map(|target| {
            let changes = target.changes();
            let mut spans = vec![Span::raw(target.name().to_owned())];
            if changes > 0 {
                spans.push(Span::styled(
                    format!("  ({changes})"),
                    Style::default().fg(Color::Yellow),
                ));
            }
            ListItem::new(Line::from(spans))
        })
        .collect();

    let mut state = ListState::default();
    state.select(Some(screen.cursor));
    let list = List::new(items)
        .block(pane("Projects", screen.focus == Focus::Projects))
        .highlight_style(selected(screen.focus == Focus::Projects))
        .highlight_symbol("> ");
    frame.render_stateful_widget(list, area, &mut state);
}

/// The right column: what can be installed of the kind shown, seen from the
/// project on the left.
fn offers(screen: &Screen, frame: &mut Frame, area: Rect) {
    let focused = screen.focus == Focus::Offers;
    let Some(target) = screen.current() else {
        let empty = Paragraph::new("this workspace manages no projects yet")
            .block(pane(plural(screen.kind), focused));
        frame.render_widget(empty, area);
        return;
    };

    let heading = title(screen.kind, target);
    let shown = target.shown(screen.kind);
    if shown.is_empty() {
        let said = target
            .unreachable(screen.kind)
            .unwrap_or_else(|| first(screen.kind).to_owned());
        let empty = Paragraph::new(said)
            .wrap(Wrap { trim: true })
            .block(pane(&heading, focused));
        frame.render_widget(empty, area);
        return;
    }

    let items: Vec<ListItem> = shown
        .iter()
        .map(|&index| row(&target.rows[index]))
        .collect();
    let mut state = ListState::default();
    state.select(shown.iter().position(|&index| index == target.cursor));
    let list = List::new(items)
        .block(pane(&heading, focused))
        .highlight_style(selected(focused))
        .highlight_symbol("> ");
    frame.render_stateful_widget(list, area, &mut state);
}

/// The left column turned round: every offer of the kind shown, with how
/// many projects' boxes of it were touched, as a project says how many of
/// its own were.
fn artifacts(screen: &Screen, frame: &mut Frame, area: Rect) {
    let focused = screen.focus == Focus::Artifacts;
    let heading = plural(screen.kind);
    let listed = screen.artifacts();
    if listed.is_empty() {
        let said = if screen.targets.is_empty() {
            String::from("this workspace manages no projects yet")
        } else if screen.none_can_take() {
            screen.none_offered()
        } else {
            first(screen.kind)
        };
        let empty = Paragraph::new(said)
            .wrap(Wrap { trim: true })
            .block(pane(heading, focused));
        frame.render_widget(empty, area);
        return;
    }

    let items: Vec<ListItem> = listed
        .iter()
        .map(|entry| {
            let changes = screen.changes_of(entry);
            let mut spans = vec![Span::styled(
                entry.candidate.name().to_owned(),
                Style::default().add_modifier(Modifier::BOLD),
            )];
            if changes > 0 {
                spans.push(Span::styled(
                    format!("  ({changes})"),
                    Style::default().fg(Color::Yellow),
                ));
            }
            ListItem::new(vec![Line::from(spans), detail(entry)])
        })
        .collect();
    let mut state = ListState::default();
    state.select(Some(screen.artifact));
    let list = List::new(items)
        .block(pane(heading, focused))
        .highlight_style(selected(focused))
        .highlight_symbol("> ");
    frame.render_stateful_widget(list, area, &mut state);
}

/// The right column turned round: every project, for the artifact on the
/// left, ticked where it holds it. A project with nowhere to put its kind
/// has no box, and says so.
fn receivers(screen: &Screen, frame: &mut Frame, area: Rect) {
    let focused = screen.focus == Focus::Receivers;
    let Some(chosen) = screen.chosen() else {
        frame.render_widget(Paragraph::new("").block(pane("Projects", focused)), area);
        return;
    };
    let kind = chosen.candidate.kind();
    let heading = format!("Projects for {}", named(kind, chosen.candidate.name()));
    let items: Vec<ListItem> = screen
        .targets
        .iter()
        .map(|target| match target.row_of(chosen) {
            Some(index) => ListItem::new(Line::from(boxed(
                &target.rows[index],
                target.name().to_owned(),
            ))),
            None => ListItem::new(Line::from(vec![
                Span::raw("    "),
                Span::styled(
                    target.name().to_owned(),
                    Style::default().fg(Color::DarkGray),
                ),
                Span::styled(
                    format!("  (takes no {kind}s)"),
                    Style::default().fg(Color::DarkGray),
                ),
            ])),
        })
        .collect();
    let mut state = ListState::default();
    state.select(Some(screen.receiver));
    let list = List::new(items)
        .block(pane(&heading, focused))
        .highlight_style(selected(focused))
        .highlight_symbol("> ");
    frame.render_stateful_widget(list, area, &mut state);
}

/// One offer: its box, its name, what it is for, and where it came from.
fn row(entry: &Row) -> ListItem<'_> {
    ListItem::new(vec![
        Line::from(boxed(entry, entry.candidate.name().to_owned())),
        detail(entry),
    ])
}

/// What an offer is for and where it came from, under its name.
fn detail(entry: &Row) -> Line<'static> {
    let summary = entry.candidate.summary().unwrap_or("");
    Line::from(vec![
        Span::raw("    "),
        Span::styled(summary.to_owned(), Style::default().fg(Color::Gray)),
        Span::styled(
            format!("  [{}]", entry.candidate.origin()),
            Style::default().fg(Color::DarkGray),
        ),
    ])
}

/// A box, `label` beside it, which is the offer's name the right way round
/// and the project's turned round, and what the box is about to do or why
/// it cannot move.
fn boxed(entry: &Row, label: String) -> Vec<Span<'static>> {
    let box_ = if entry.checked { "[x] " } else { "[ ] " };
    let mut spans = vec![
        Span::styled(box_, Style::default().fg(mark(entry))),
        Span::styled(label, Style::default().add_modifier(Modifier::BOLD)),
    ];

    // Why this row cannot be moved, or what it is about to do. Both are the
    // sort of thing a reader should not have to remember.
    match entry.pending() {
        Some(Pending::Install) => spans.push(Span::styled(
            "  + install",
            Style::default().fg(Color::Green),
        )),
        Some(Pending::Remove) => {
            spans.push(Span::styled("  - remove", Style::default().fg(Color::Red)));
        }
        None if entry.foreign() => spans.push(Span::styled(
            "  (not installed by mindflayer)",
            Style::default().fg(Color::DarkGray),
        )),
        None if entry.edited() => spans.push(Span::styled(
            "  (changed since it was installed)",
            Style::default().fg(Color::DarkGray),
        )),
        None if entry.missing() => spans.push(Span::styled(
            "  (a copy gone since it was installed)",
            Style::default().fg(Color::DarkGray),
        )),
        None if entry.outdated() => spans.push(Span::styled(
            "  (outdated; flayer update refreshes it)",
            Style::default().fg(Color::Yellow),
        )),
        None => {}
    }
    spans
}

/// The keys, what is waiting, and whatever the last key had to say.
///
/// A line each: a line of keys too long for the terminal is cut short
/// rather than wrapped, which would push what is waiting, or why a key did
/// nothing, off the bottom.
fn footer(screen: &Screen, frame: &mut Frame, area: Rect) {
    let (kind, next) = (screen.kind, screen.next_kind(false));
    let keys = match screen.focus {
        Focus::Projects => {
            format!("↑↓ project   →/enter {kind}s   tab {next}s   v by {kind}   a apply   q quit")
        }
        Focus::Offers => format!(
            "↑↓ {kind}   space mark   tab {next}s   ←/esc back   v by {kind}   a apply   q quit"
        ),
        Focus::Artifacts => {
            format!("↑↓ {kind}   →/enter projects   tab {next}s   v by project   a apply   q quit")
        }
        Focus::Receivers => format!(
            "↑↓ project   space mark   tab {next}s   ←/esc back   v by project   a apply   q quit"
        ),
    };
    let (install, remove) = screen.counts();
    let waiting = if install == 0 && remove == 0 {
        String::from("nothing marked")
    } else {
        format!("{install} to install, {remove} to remove")
    };

    let second = match &screen.message {
        Some(message) => Span::styled(message.clone(), Style::default().fg(Color::Yellow)),
        None => Span::styled(waiting, Style::default().fg(Color::Gray)),
    };
    let lines = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Length(1)])
        .split(area);
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            keys,
            Style::default().fg(Color::DarkGray),
        ))),
        lines[0],
    );
    // Wrapped, though only its first line shows, so a long one is cut
    // between words.
    frame.render_widget(
        Paragraph::new(Line::from(second)).wrap(Wrap { trim: true }),
        lines[1],
    );
}

/// The question asked before anything is written or deleted.
fn confirm(screen: &Screen, frame: &mut Frame, area: Rect) {
    let (install, remove) = screen.counts();
    let box_ = centred(area, 60, 7);
    frame.render_widget(Clear, box_);
    let text = vec![
        Line::from(format!("Install {install}, remove {remove}?")),
        Line::from(""),
        Line::from(Span::styled(
            "removals take what Mindflayer installed back out",
            Style::default().fg(Color::Red),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "y  yes        n  no",
            Style::default().add_modifier(Modifier::BOLD),
        )),
    ];
    let block = Block::bordered()
        .border_type(BorderType::Thick)
        .title(" Apply ");
    frame.render_widget(Paragraph::new(text).block(block), box_);
}

fn title(kind: Kind, target: &Target) -> String {
    format!("{} in {}", plural(kind), target.name())
}

/// What a column of `kind` is called.
fn plural(kind: Kind) -> &'static str {
    // Matched on the kind, so a third one has to say what it is called.
    match kind {
        Kind::Skill => "Skills",
        Kind::Rule => "Rules",
    }
}

/// What to do first, for a project offered nothing of `kind`: where that
/// kind comes from, as [`nothing`] says, with the commands that bring some.
fn first(kind: Kind) -> String {
    // Matched on the kind, so a third one has to say what brings it.
    let commands = match kind {
        Kind::Skill => "`flayer gather git <url>`, `flayer load <path>` or `flayer add skill`",
        Kind::Rule => "`flayer load <path>` or `flayer add rule`",
    };
    format!("{}; {commands} first", nothing(kind))
}

/// A bordered pane, drawn heavier when the keyboard is in it.
pub(crate) fn pane(title: &str, focused: bool) -> Block<'_> {
    let block = Block::bordered().title(format!(" {title} "));
    if focused {
        block
            .border_type(BorderType::Thick)
            .border_style(Style::default().fg(Color::Cyan))
    } else {
        block.border_style(Style::default().fg(Color::DarkGray))
    }
}

/// The cursor line, dimmed while the keyboard is in the other column so there
/// is never a question about which one a key will move.
pub(crate) fn selected(focused: bool) -> Style {
    if focused {
        Style::default().add_modifier(Modifier::BOLD | Modifier::REVERSED)
    } else {
        Style::default().add_modifier(Modifier::DIM)
    }
}

/// What colour a box is: what it will become, not what it is.
fn mark(entry: &super::state::Row) -> Color {
    match entry.pending() {
        Some(Pending::Install) => Color::Green,
        Some(Pending::Remove) => Color::Red,
        None if entry.foreign() || entry.edited() => Color::DarkGray,
        None => Color::Reset,
    }
}

/// A box of that size in the middle of `area`.
pub(crate) fn centred(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);
    Rect {
        x: area.x + (area.width - width) / 2,
        y: area.y + (area.height - height) / 2,
        width,
        height,
    }
}
