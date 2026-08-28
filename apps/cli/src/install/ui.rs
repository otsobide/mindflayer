//! Drawing the install screen.
//!
//! Two columns: the projects a workspace manages, and — for whichever project
//! the cursor is on — every shelf entry, ticked when that project already
//! holds it. Nothing here decides anything; [`super::state`] does.

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Clear, List, ListItem, ListState, Paragraph, Wrap};
use ratatui::Frame;

use super::state::{Focus, Pending, Screen, Target};

/// Draw the whole screen.
pub fn draw(screen: &Screen, frame: &mut Frame) {
    let area = frame.area();
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(3), Constraint::Length(2)])
        .split(area);
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(30), Constraint::Percentage(70)])
        .split(rows[0]);

    projects(screen, frame, columns[0]);
    skills(screen, frame, columns[1]);
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

/// The right column: the shelf, seen from the project on the left.
fn skills(screen: &Screen, frame: &mut Frame, area: Rect) {
    let focused = screen.focus == Focus::Skills;
    let Some(target) = screen.current() else {
        let empty =
            Paragraph::new("this workspace manages no projects yet").block(pane("Skills", focused));
        frame.render_widget(empty, area);
        return;
    };

    let heading = title(target);
    if target.rows.is_empty() {
        let empty = Paragraph::new("nothing on the shelf — `flayer gather git <url>` first")
            .block(pane(&heading, focused));
        frame.render_widget(empty, area);
        return;
    }

    let items: Vec<ListItem> = target.rows.iter().map(row).collect();
    let mut state = ListState::default();
    state.select(Some(target.cursor));
    let list = List::new(items)
        .block(pane(&heading, focused))
        .highlight_style(selected(focused))
        .highlight_symbol("> ");
    frame.render_stateful_widget(list, area, &mut state);
}

/// One shelf entry: its box, its name, what it is for, and where it came from.
fn row(entry: &super::state::Row) -> ListItem<'_> {
    let box_ = if entry.checked { "[x] " } else { "[ ] " };
    let mut spans = vec![
        Span::styled(box_, Style::default().fg(mark(entry))),
        Span::styled(
            entry.candidate.name().to_owned(),
            Style::default().add_modifier(Modifier::BOLD),
        ),
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
        None => {}
    }

    let summary = entry.candidate.gathered.summary.as_deref().unwrap_or("");
    let detail = Line::from(vec![
        Span::raw("    "),
        Span::styled(summary.to_owned(), Style::default().fg(Color::Gray)),
        Span::styled(
            format!("  [{}]", entry.candidate.origin()),
            Style::default().fg(Color::DarkGray),
        ),
    ]);
    ListItem::new(vec![Line::from(spans), detail])
}

/// The keys, what is waiting, and whatever the last key had to say.
fn footer(screen: &Screen, frame: &mut Frame, area: Rect) {
    let keys = match screen.focus {
        Focus::Projects => "↑↓ project   →/enter skills   a apply   q quit",
        Focus::Skills => "↑↓ skill   space mark   ←/esc back   a apply   q quit",
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
    let text = vec![
        Line::from(Span::styled(keys, Style::default().fg(Color::DarkGray))),
        Line::from(second),
    ];
    frame.render_widget(Paragraph::new(text).wrap(Wrap { trim: true }), area);
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
            "removals delete the skill's directory from the project",
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

fn title(target: &Target) -> String {
    format!("Skills in {}", target.name())
}

/// A bordered pane, drawn heavier when the keyboard is in it.
fn pane(title: &str, focused: bool) -> Block<'_> {
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
fn selected(focused: bool) -> Style {
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
        None if entry.foreign() => Color::DarkGray,
        None => Color::Reset,
    }
}

/// A box of that size in the middle of `area`.
fn centred(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);
    Rect {
        x: area.x + (area.width - width) / 2,
        y: area.y + (area.height - height) / 2,
        width,
        height,
    }
}
