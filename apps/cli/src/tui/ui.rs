//! Drawing the TUI.
//!
//! Two columns — the projects a workspace manages, and what the one under the
//! cursor holds — with the keys underneath and, over them, whatever the mode
//! asks for: a form, a question, a page of output. Every form shows the
//! command it is about to run, as it is typed. The install screen, when it is
//! open, draws itself. Nothing here decides anything; [`super::state`] does.

use mindflayer_core::Kind;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Clear, List, ListItem, ListState, Paragraph, Wrap};
use ratatui::Frame;

use super::minds::{self, Minds};
use super::state::{
    AddField, AddForm, App, Confirm, Focus, GatherForm, Input, Item, LinkForm, LoadForm, Member,
    Mode, Reader, RenameForm, Scope, Task, Tone, View, Workspace, GATHER_FIELDS,
};
use crate::install::ui::{centred, pane, selected};
use crate::printable;

/// How wide a form's labels are, so the values line up.
const LABEL: u16 = 13;

/// Draw the whole screen.
pub fn draw(app: &App, frame: &mut Frame) {
    let area = frame.area();
    if let Mode::Installing(screen) = &app.mode {
        crate::install::ui::draw(screen, frame);
    } else {
        match &app.view {
            View::Nowhere => nowhere(app, frame, area),
            View::Workspace(workspace) => home(app, workspace, frame, area),
        }
        match &app.mode {
            Mode::Adding(form) => adding(app, form, frame, area),
            Mode::Renaming(form) => renaming(app, form, frame, area),
            Mode::Linking(form) => linking(app, form, frame, area),
            Mode::Loading(form) => loading(app, form, frame, area),
            Mode::Gathering(form) => gathering(form, frame, area),
            Mode::Confirming(confirm) => confirming(confirm, frame, area),
            Mode::Reading(reader) => reading(reader, frame, area),
            Mode::Minds(screen) => minds_screen(app, screen, frame, area),
            Mode::Browse | Mode::Installing(_) => {}
        }
    }
    if let Some(busy) = &app.busy {
        working(busy, frame, area);
    }
}

// ---------------------------------------------------------------------------
// The two screens underneath everything
// ---------------------------------------------------------------------------

/// No workspace yet: say so, and what one keypress would do about it.
fn nowhere(app: &App, frame: &mut Frame, area: Rect) {
    const WIDTH: u16 = 78;

    let directory = printable(&app.directory.display().to_string());
    let (missing, what, command) = match app.opened {
        Scope::Workspace => (
            "No flayer workspace here",
            "A workspace keeps track of the mind projects you manage together.",
            "flayer init",
        ),
        Scope::Project => (
            "No mind project here",
            "A project keeps the skills and rules its agents read, beside its code.",
            "mind init",
        ),
    };
    // The directory is the one line of unknown length, so it gets a line of
    // its own and the box grows by however many rows it wraps onto.
    let room = usize::from(WIDTH.min(area.width).saturating_sub(2)).max(1);
    let wrapped = (directory.chars().count() + 2).div_ceil(room);
    let text = vec![
        Line::from(Span::styled(missing, bold())),
        Line::from(""),
        Line::from("Nothing at or above this directory is one:"),
        Line::from(Span::styled(format!("  {directory}"), dim())),
        Line::from(what),
        Line::from(""),
        Line::from(vec![
            Span::styled("enter  ", bold()),
            Span::raw("create one here"),
            Span::styled(format!("  runs `{command}`"), dim()),
        ]),
        Line::from(vec![Span::styled("q      ", bold()), Span::raw("quit")]),
    ];
    let height = u16::try_from(text.len() + wrapped + 1).unwrap_or(u16::MAX);
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .title(" mindflayer ");
    let box_ = centred(area, WIDTH, height);
    frame.render_widget(
        Paragraph::new(text).block(block).wrap(Wrap { trim: false }),
        box_,
    );

    // Whatever the last attempt said, where the eye goes after the box.
    if let Some(message) = &app.message {
        let line = Rect {
            y: area.bottom().saturating_sub(1),
            height: 1,
            ..area
        };
        frame.render_widget(
            Paragraph::new(Span::styled(printable(&message.text), tone(message.tone))),
            line,
        );
    }
}

/// The workspace: a header, the two columns, and the keys.
fn home(app: &App, workspace: &Workspace, frame: &mut Frame, area: Rect) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(3),
            Constraint::Length(2),
        ])
        .split(area);
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(30), Constraint::Percentage(70)])
        .split(rows[1]);

    header(workspace, frame, rows[0]);
    projects(app, workspace, frame, columns[0]);
    holdings(app, workspace, frame, columns[1]);
    footer(app, frame, rows[2]);
}

fn header(workspace: &Workspace, frame: &mut Frame, area: Rect) {
    let shelf = match (workspace.scope, workspace.shelf) {
        (Scope::Project, _) => String::from("a project no workspace manages"),
        (Scope::Workspace, 0) => String::from("nothing on the shelf"),
        (Scope::Workspace, count) => format!("{count} on the shelf"),
    };
    let shelf = match workspace.loaded.len() {
        0 => shelf,
        count => format!("{shelf} · {count} loaded"),
    };
    // Said, because it is not where you are: nothing at or above here was a
    // workspace, so this is the one in your home.
    let shelf = if workspace.default {
        format!("default workspace · {shelf}")
    } else {
        shelf
    };
    let line = Line::from(vec![
        Span::styled(
            " mindflayer ",
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" "),
        Span::styled(printable(&workspace.name), bold()),
        Span::styled(format!("  · {shelf}  · "), dim()),
        // Last, because it is the one thing that can be long, and what a
        // narrow terminal cuts off the end of should be the least needed.
        Span::styled(printable(&workspace.root.display().to_string()), dim()),
    ]);
    frame.render_widget(Paragraph::new(line), area);
}

/// The left column: every registered entry, with how many artifacts it holds
/// and how many of those want attention.
fn projects(app: &App, workspace: &Workspace, frame: &mut Frame, area: Rect) {
    let focused = app.focus == Focus::Projects;
    let title = match workspace.scope {
        Scope::Workspace => format!("Projects ({})", workspace.projects()),
        Scope::Project => String::from("Project"),
    };

    if workspace.members.is_empty() {
        let mut text = vec![
            Line::from("none linked yet"),
            Line::from(""),
            Line::from(Span::styled("l links one", dim())),
        ];
        if !workspace.unlinked.is_empty() {
            text.push(Line::from(Span::styled(
                format!("{} found under here", workspace.unlinked.len()),
                dim(),
            )));
        }
        frame.render_widget(
            Paragraph::new(text)
                .block(pane(&title, focused))
                .wrap(Wrap { trim: true }),
            area,
        );
        return;
    }

    let items: Vec<ListItem> = workspace.members.iter().map(project_row).collect();
    let mut state = ListState::default();
    state.select(Some(app.cursor));
    let list = List::new(items)
        .block(pane(&title, focused))
        .highlight_style(selected(focused))
        .highlight_symbol("> ");
    frame.render_stateful_widget(list, area, &mut state);
}

fn project_row(member: &Member) -> ListItem<'_> {
    let name = printable(&member.name);
    // The workspace's own row reads as something else than a project: it
    // holds what is shared with all of them.
    let name = if member.own {
        format!("◆ {name}")
    } else {
        name
    };
    let spans = match &member.holdings {
        // A stale entry is listed, not hidden: it is exactly the one somebody
        // needs to find in order to unlink it.
        Err(_) => vec![Span::styled(
            format!("! {name}"),
            Style::default().fg(Color::Red),
        )],
        Ok(holdings) => {
            let styled = if member.own {
                Span::styled(name, Style::default().fg(Color::Cyan))
            } else {
                Span::raw(name)
            };
            let mut spans = vec![
                styled,
                Span::styled(format!("  {}", holdings.items.len()), dim()),
            ];
            let problems = member.problems();
            if problems > 0 {
                spans.push(Span::styled(
                    format!("  ✗ {problems}"),
                    Style::default().fg(Color::Red),
                ));
            }
            spans
        }
    };
    ListItem::new(Line::from(spans))
}

/// The right column: what the project under the cursor holds.
fn holdings(app: &App, workspace: &Workspace, frame: &mut Frame, area: Rect) {
    let focused = app.focus == Focus::Items;
    let Some(member) = app.member() else {
        let text = if workspace.unlinked.is_empty() {
            "nothing linked — l links a project by its path"
        } else {
            "nothing linked — l picks from what is under this workspace"
        };
        frame.render_widget(
            Paragraph::new(Span::styled(text, dim()))
                .block(pane("Artifacts", false))
                .wrap(Wrap { trim: true }),
            area,
        );
        return;
    };

    let title = if member.own {
        format!("{} · the workspace's own", printable(&member.name))
    } else if member.name == member.entry || member.entry == "." {
        printable(&member.name)
    } else {
        format!("{} ({})", printable(&member.name), printable(&member.entry))
    };

    let holdings = match &member.holdings {
        Ok(holdings) => holdings,
        Err(error) => {
            let text = vec![
                Line::from(Span::styled(
                    "this project cannot be opened",
                    bold().fg(Color::Red),
                )),
                Line::from(""),
                Line::from(printable(error)),
                Line::from(""),
                Line::from(Span::styled(
                    "u unlinks it; nothing on disk is touched",
                    dim(),
                )),
            ];
            frame.render_widget(
                Paragraph::new(text)
                    .block(pane(&title, focused))
                    .wrap(Wrap { trim: true }),
                area,
            );
            return;
        }
    };

    // Unreadable files get a strip of their own, so they cannot be scrolled
    // out of sight by the artifacts that did read.
    let (list_area, warning_area) = if holdings.warnings.is_empty() {
        (area, None)
    } else {
        let height = u16::try_from(holdings.warnings.len() + 2)
            .unwrap_or(u16::MAX)
            .clamp(3, area.height / 2);
        let parts = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(3), Constraint::Length(height)])
            .split(area);
        (parts[0], Some(parts[1]))
    };

    if holdings.items.is_empty() {
        let mut text = vec![Line::from("nothing here yet"), Line::from("")];
        for (kind, directory) in &holdings.directories {
            text.push(Line::from(Span::styled(
                format!("{} go in {}/", kind.folder(), printable(directory)),
                dim(),
            )));
        }
        text.push(Line::from(""));
        text.push(Line::from(Span::styled("a adds a skill or a rule", dim())));
        if member.own {
            text.push(Line::from(Span::styled(
                "what is here is shared: i installs a skill into any project",
                dim(),
            )));
        }
        frame.render_widget(Paragraph::new(text).block(pane(&title, focused)), list_area);
    } else {
        let width = holdings
            .items
            .iter()
            .map(|item| printable(&item.name).chars().count())
            .max()
            .unwrap_or(0)
            .min(40);
        // Inside the borders and the highlight symbol, less the indent the
        // problems are written at.
        let room =
            usize::from(list_area.width.saturating_sub(4)).saturating_sub(ISSUE_INDENT.len());
        let rows: Vec<ListItem> = holdings
            .items
            .iter()
            .map(|item| item_row(item, width, room))
            .collect();
        let mut state = ListState::default();
        state.select(Some(app.item));
        let list = List::new(rows)
            .block(pane(&title, focused))
            .highlight_style(selected(focused))
            .highlight_symbol("> ");
        frame.render_stateful_widget(list, list_area, &mut state);
    }

    if let Some(warning_area) = warning_area {
        let lines: Vec<Line> = holdings
            .warnings
            .iter()
            .map(|warning| Line::from(Span::styled(printable(warning), warn())))
            .collect();
        let block = Block::bordered()
            .title(" Could not be read ")
            .border_style(warn());
        frame.render_widget(
            Paragraph::new(lines).block(block).wrap(Wrap { trim: true }),
            warning_area,
        );
    }
}

/// Where the problems under an artifact start, so they read as belonging to
/// it rather than as rows of their own.
const ISSUE_INDENT: &str = "        ";

/// One artifact: its kind, its name, what it is for — and, underneath, what
/// `validate` would say about it, right beside the thing it is about, wrapped
/// to `room` rather than cut off, because the end of a reason is usually the
/// part that says what to do.
fn item_row(item: &Item, width: usize, room: usize) -> ListItem<'_> {
    let name = printable(&item.name);
    let padding = width.saturating_sub(name.chars().count());
    let red = Style::default().fg(Color::Red);
    // `!` for a file that does not read at all, `✗` for one that reads and
    // is wrong: two different things to go and fix.
    let (mark, summary) = match &item.broken {
        Some(_) => (
            Span::styled("! ", red),
            Span::styled("cannot be read — e opens it", red),
        ),
        None if !item.issues.is_empty() => (
            Span::styled("✗ ", red),
            Span::styled(printable(&item.summary), Style::default().fg(Color::Gray)),
        ),
        None => (
            Span::raw("  "),
            Span::styled(printable(&item.summary), Style::default().fg(Color::Gray)),
        ),
    };
    let mut lines = vec![Line::from(vec![
        mark,
        Span::styled(format!("{:<5} ", item.kind.slug()), dim()),
        Span::styled(format!("{name}{}", " ".repeat(padding)), bold()),
        Span::raw("  "),
        summary,
    ])];
    for issue in item.broken.iter().chain(&item.issues) {
        for part in wrap(&printable(issue), room) {
            lines.push(Line::from(Span::styled(
                format!("{ISSUE_INDENT}{part}"),
                Style::default().fg(Color::Red),
            )));
        }
    }
    ListItem::new(lines)
}

/// Break text into lines of at most `room` characters, at spaces where there
/// are any.
fn wrap(text: &str, room: usize) -> Vec<String> {
    if room == 0 {
        return vec![text.to_owned()];
    }
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in text.split(' ') {
        let taken = line.chars().count();
        if taken > 0 && taken + 1 + word.chars().count() > room {
            lines.push(std::mem::take(&mut line));
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(word);
        // A word longer than the room is cut where the room ends.
        while line.chars().count() > room {
            let head: String = line.chars().take(room).collect();
            line = line.chars().skip(room).collect();
            lines.push(head);
        }
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
}

/// The keys, most needed first so a narrow terminal cuts the least useful,
/// and the last thing that happened.
fn footer(app: &App, frame: &mut Frame, area: Rect) {
    // Only the keys that do something here: a project on its own has no
    // registry, shelf or installs to offer keys for.
    let keys = match (app.focus, app.scope()) {
        (Focus::Items, _) => {
            "q quit  ? keys  enter show  e edit  r rename  d delete  a add  ← back  v validate"
        }
        (Focus::Projects, Scope::Workspace) => {
            "q quit  ? keys  → open  a add  m minds  l link  u unlink  v validate  g gather  L load  s shelf  i install"
        }
        (Focus::Projects, Scope::Project) => {
            "q quit  ? keys  → open  a add  v validate  l link it to the workspace above  L load"
        }
    };
    let second = match &app.message {
        Some(message) => Span::styled(printable(&message.text), tone(message.tone)),
        None => Span::styled(
            "every change is a command, printed when you quit — ? lists them",
            dim(),
        ),
    };
    let text = vec![Line::from(Span::styled(keys, dim())), Line::from(second)];
    frame.render_widget(Paragraph::new(text), area);
}

// ---------------------------------------------------------------------------
// What goes over them
// ---------------------------------------------------------------------------

fn adding(app: &App, form: &AddForm, frame: &mut Frame, area: Rect) {
    let project = app
        .workspace()
        .and_then(|workspace| workspace.members.get(form.member))
        .map_or_else(String::new, |member| printable(&member.name));
    let inner = popup(frame, area, &format!("Add to {project}"), 76, 10);
    let rows = lines(inner, 8);

    let kinds: Vec<(String, bool)> = Kind::ALL
        .iter()
        .map(|kind| (kind.slug().to_owned(), *kind == form.kind))
        .collect();
    choices(frame, rows[0], "kind", &kinds, form.field == AddField::Kind);

    // No template is a choice like the others, and the first one.
    let offered = app.templates_for(form.member, form.kind);
    let mut templates = vec![(String::from("none"), form.template.is_none())];
    templates.extend(
        offered
            .iter()
            .enumerate()
            .map(|(at, choice)| (printable(&choice.name), form.template == Some(at))),
    );
    choices(
        frame,
        rows[1],
        "template",
        &templates,
        form.field == AddField::Template,
    );

    input(
        frame,
        rows[2],
        "name",
        &form.name,
        form.field == AddField::Name,
    );
    input(
        frame,
        rows[3],
        "description",
        &form.description,
        form.field == AddField::Description,
    );
    runs(frame, rows[5], app.add_task(form).as_ref());
    error(frame, rows[6], form.error.as_deref());
    keys(
        frame,
        rows[7],
        "enter create   tab next   ←→ choose kind and template   esc cancel",
    );
}

fn renaming(app: &App, form: &RenameForm, frame: &mut Frame, area: Rect) {
    let inner = popup(
        frame,
        area,
        &format!("Rename {} {}", form.kind, printable(&form.from)),
        76,
        7,
    );
    let rows = lines(inner, 5);

    input(frame, rows[0], "new name", &form.name, true);
    let what = if form.kind == Kind::Skill {
        "the folder and the `name` it declares move together"
    } else {
        "the file is filed again by its new route"
    };
    let hint = Line::from(vec![
        Span::raw(" ".repeat(usize::from(LABEL))),
        Span::styled(what, dim()),
    ]);
    frame.render_widget(Paragraph::new(hint), rows[1]);
    runs(frame, rows[2], app.rename_task(form).as_ref());
    error(frame, rows[3], form.error.as_deref());
    keys(frame, rows[4], "enter rename   esc cancel");
}

/// A labelled line of choices, the chosen one bracketed.
fn choices(frame: &mut Frame, area: Rect, name: &str, offered: &[(String, bool)], focused: bool) {
    let mut line = vec![label(name, focused)];
    for (text, chosen) in offered {
        line.push(if *chosen {
            Span::styled(format!("[{text}] "), bold().fg(Color::Cyan))
        } else {
            Span::styled(format!(" {text}  "), dim())
        });
    }
    frame.render_widget(Paragraph::new(Line::from(line)), area);
}

fn linking(app: &App, form: &LinkForm, frame: &mut Frame, area: Rect) {
    let Some(workspace) = app.workspace() else {
        return;
    };
    let shown = workspace.unlinked.len().clamp(1, 10);
    let height = u16::try_from(shown).unwrap_or(10) + 8;
    let inner = popup(
        frame,
        area,
        &format!("Link a project to {}", printable(&workspace.name)),
        78,
        height,
    );
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(u16::try_from(shown).unwrap_or(10)),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(inner);

    input(frame, rows[0], "path", &form.path, true);

    if workspace.unlinked.is_empty() {
        frame.render_widget(
            Paragraph::new(Span::styled(
                "nothing unlinked found under this workspace; type a path",
                dim(),
            )),
            rows[2],
        );
    } else {
        // The pick only counts while the path is empty, so it dims as soon as
        // something is typed.
        let picking = form.path.text().trim().is_empty();
        let width = workspace
            .unlinked
            .iter()
            .map(|unlinked| printable(&unlinked.route).chars().count())
            .max()
            .unwrap_or(0);
        let items: Vec<ListItem> = workspace
            .unlinked
            .iter()
            .map(|unlinked| {
                let route = printable(&unlinked.route);
                let padding = " ".repeat(width.saturating_sub(route.chars().count()));
                let what = if unlinked.mind {
                    Span::styled("  mind project", dim())
                } else {
                    Span::styled("  git repository — mind init first", warn())
                };
                ListItem::new(Line::from(vec![
                    Span::raw(format!("{route}{padding}")),
                    what,
                ]))
            })
            .collect();
        let mut state = ListState::default();
        state.select(Some(form.pick));
        let list = List::new(items)
            .highlight_style(selected(picking))
            .highlight_symbol("> ");
        frame.render_stateful_widget(list, rows[2], &mut state);
    }

    runs(frame, rows[4], app.link_task(form).as_ref());
    error(frame, rows[5], form.error.as_deref());
    keys(
        frame,
        rows[6],
        "enter link   ↑↓ pick   type a path relative to the workspace   esc cancel",
    );
}

/// The load form: a path to type, over the workspaces already loaded — each
/// with what it offers, or why it cannot be read, and picked to unload.
fn loading(app: &App, form: &LoadForm, frame: &mut Frame, area: Rect) {
    let Some(workspace) = app.workspace() else {
        return;
    };
    let on_its_own = workspace.scope == Scope::Project;
    let shown = workspace.loaded.len().clamp(1, 10);
    let height = u16::try_from(shown).unwrap_or(10) + 8;
    let title = if on_its_own {
        String::from("Load a workspace into the one above this project")
    } else {
        format!("Load a workspace into {}", printable(&workspace.name))
    };
    let inner = popup(frame, area, &title, 78, height);
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(u16::try_from(shown).unwrap_or(10)),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(inner);

    input(frame, rows[0], "path", &form.path, true);

    if workspace.loaded.is_empty() {
        let hint = if on_its_own {
            "type a workspace's path, relative to this project — nothing loaded yet"
        } else {
            "type a workspace's path, relative to this one — nothing loaded yet"
        };
        frame.render_widget(Paragraph::new(Span::styled(hint, dim())), rows[2]);
    } else {
        // As in the link form, the pick only counts while nothing is typed.
        let picking = form.path.text().trim().is_empty();
        let width = workspace
            .loaded
            .iter()
            .map(|source| printable(&source.entry).chars().count())
            .max()
            .unwrap_or(0);
        let items: Vec<ListItem> = workspace
            .loaded
            .iter()
            .map(|source| {
                let entry = printable(&source.entry);
                let padding = " ".repeat(width.saturating_sub(entry.chars().count()));
                let what = match &source.offers {
                    Ok(0) => Span::styled(
                        format!("  {} · nothing to install yet", printable(&source.name)),
                        dim(),
                    ),
                    Ok(count) => Span::styled(
                        format!("  {} · {count} to install", printable(&source.name)),
                        dim(),
                    ),
                    Err(_) => Span::styled(
                        "  cannot be opened — enter unloads it",
                        Style::default().fg(Color::Red),
                    ),
                };
                ListItem::new(Line::from(vec![
                    Span::raw(format!("{entry}{padding}")),
                    what,
                ]))
            })
            .collect();
        let mut state = ListState::default();
        state.select(Some(form.pick));
        let list = List::new(items)
            .highlight_style(selected(picking))
            .highlight_symbol("> ");
        frame.render_stateful_widget(list, rows[2], &mut state);
    }

    runs(frame, rows[4], app.load_task(form).as_ref());
    error(frame, rows[5], form.error.as_deref());
    keys(
        frame,
        rows[6],
        "enter load, or unload the one picked   ↑↓ pick   esc cancel",
    );
}

fn gathering(form: &GatherForm, frame: &mut Frame, area: Rect) {
    let inner = popup(frame, area, "Gather skills from a git repository", 78, 9);
    let rows = lines(inner, 7);

    for (index, name) in GATHER_FIELDS.iter().enumerate() {
        input(
            frame,
            rows[index],
            name,
            form.input(index),
            form.field == index,
        );
    }
    // What `ref` means, under it, rather than in a key line too long to fit.
    let hint = Line::from(vec![
        Span::raw(" ".repeat(usize::from(LABEL))),
        Span::styled("a branch or tag; empty takes the default branch", dim()),
    ]);
    frame.render_widget(Paragraph::new(hint), rows[3]);
    runs(frame, rows[4], form.task().as_ref());
    error(frame, rows[5], form.error.as_deref());
    keys(frame, rows[6], "enter gather   tab next   esc cancel");
}

fn confirming(confirm: &Confirm, frame: &mut Frame, area: Rect) {
    const WIDTH: u16 = 76;

    let runs = format!("runs  {}", printable(&confirm.task.describe()));
    let mut said: Vec<String> = confirm
        .question
        .iter()
        .map(|line| printable(line))
        .collect();
    said.extend([String::new(), runs, String::new()]);

    // Sized to what it says once wrapped, so the answer keys at the bottom are
    // never the part that falls off.
    let room = usize::from(WIDTH.min(area.width).saturating_sub(2)).max(1);
    let rows: usize = said
        .iter()
        .map(|line| line.chars().count().div_ceil(room).max(1))
        .sum();
    let height = u16::try_from(rows + 3).unwrap_or(u16::MAX);

    let last = said.len() - 2;
    let mut text: Vec<Line> = said
        .into_iter()
        .enumerate()
        .map(|(index, line)| {
            if index == last {
                Line::from(Span::styled(line, dim()))
            } else {
                Line::from(line)
            }
        })
        .collect();
    text.push(Line::from(Span::styled("y  yes        n  no", bold())));

    let box_ = centred(area, WIDTH, height);
    frame.render_widget(Clear, box_);
    let block = Block::bordered()
        .border_type(BorderType::Thick)
        .title(" Confirm ");
    frame.render_widget(
        Paragraph::new(text).block(block).wrap(Wrap { trim: false }),
        box_,
    );
}

fn reading(reader: &Reader, frame: &mut Frame, area: Rect) {
    // Everything below the header: output is read in full, and the keys that
    // apply while reading are in its own border.
    let below = 1.min(area.height);
    let box_ = Rect {
        y: area.y + below,
        height: area.height - below,
        ..area
    };
    frame.render_widget(Clear, box_);
    let lines: Vec<Line> = reader
        .lines
        .iter()
        .map(|(tone_of, text)| Line::from(Span::styled(clean(text), tone(*tone_of))))
        .collect();
    let block = Block::bordered()
        .border_type(BorderType::Thick)
        .border_style(Style::default().fg(Color::Cyan))
        .title(format!(" {} ", printable(&reader.title)))
        .title_bottom(Line::from(" ↑↓ scroll   esc close ").right_aligned());
    let scroll = u16::try_from(reader.scroll).unwrap_or(u16::MAX);
    frame.render_widget(
        Paragraph::new(lines)
            .block(block)
            .wrap(Wrap { trim: false })
            .scroll((scroll, 0)),
        box_,
    );
}

/// The minds screen: every directory the workspace manages or could, ticked
/// where it does, over everything below the header.
fn minds_screen(app: &App, screen: &Minds, frame: &mut Frame, area: Rect) {
    let below = 1.min(area.height);
    let box_ = Rect {
        y: area.y + below,
        height: area.height - below,
        ..area
    };
    frame.render_widget(Clear, box_);

    let name = app
        .workspace()
        .map_or_else(String::new, |workspace| printable(&workspace.name));
    let (link, init, unlink) = screen.counts();
    let waiting = match (link, unlink) {
        (0, 0) => String::from("nothing marked"),
        _ if init > 0 => {
            format!("{link} to link, {init} of them after mind init · {unlink} to unlink")
        }
        _ => format!("{link} to link · {unlink} to unlink"),
    };
    let block = Block::bordered()
        .border_type(BorderType::Thick)
        .border_style(Style::default().fg(Color::Cyan))
        .title(format!(" Minds in {name} "))
        .title_bottom(
            Line::from(format!(
                " ↑↓ move   space tick   a apply   esc back · {waiting} "
            ))
            .right_aligned(),
        );
    let inner = block.inner(box_);
    frame.render_widget(block, box_);

    if screen.rows.is_empty() {
        frame.render_widget(
            Paragraph::new(Span::styled(
                "nothing linked, and no repository under this workspace — l links one by its path",
                dim(),
            ))
            .wrap(Wrap { trim: true }),
            inner,
        );
        return;
    }

    let width = screen
        .rows
        .iter()
        .map(|row| printable(&row.path).chars().count())
        .max()
        .unwrap_or(0)
        .min(50);
    // Padded to the longest standing too, so what applying would do lines
    // up in one column.
    let said = screen
        .rows
        .iter()
        .map(|row| standing(row.standing).0.chars().count())
        .max()
        .unwrap_or(0);
    let items: Vec<ListItem> = screen
        .rows
        .iter()
        .map(|row| mind_row(row, width, said))
        .collect();
    let mut state = ListState::default();
    state.select(Some(screen.cursor));
    let list = List::new(items)
        .highlight_style(selected(true))
        .highlight_symbol("> ");
    frame.render_stateful_widget(list, inner, &mut state);
}

/// One directory: its box, its path, where it stands, and what applying
/// would do to it — coloured by what it will become, not by what it is.
fn mind_row(row: &minds::Row, width: usize, said: usize) -> ListItem<'_> {
    let change = row.change();
    let colour = match change {
        Some(minds::Change::Link | minds::Change::InitAndLink) => Color::Green,
        Some(minds::Change::Unlink) => Color::Red,
        None => Color::Reset,
    };
    let path = printable(&row.path);
    let padding = " ".repeat(width.saturating_sub(path.chars().count()));
    let (standing, style) = standing(row.standing);
    let after = " ".repeat(said.saturating_sub(standing.chars().count()));
    let mut spans = vec![
        Span::styled(
            if row.ticked { "[x] " } else { "[ ] " },
            Style::default().fg(colour),
        ),
        Span::raw(format!("{path}{padding}  ")),
        Span::styled(standing, style),
        Span::raw(after),
    ];
    let pending = match change {
        Some(minds::Change::Link) => Some(("  + link", Color::Green)),
        Some(minds::Change::InitAndLink) => Some(("  + mind init, then link", Color::Green)),
        Some(minds::Change::Unlink) => Some(("  - unlink", Color::Red)),
        None => None,
    };
    if let Some((text, colour)) = pending {
        spans.push(Span::styled(text, Style::default().fg(colour)));
    }
    ListItem::new(Line::from(spans))
}

/// Where a row stands, in words, and how loudly.
fn standing(standing: minds::Standing) -> (&'static str, Style) {
    match standing {
        minds::Standing::Linked { opens: true } => ("linked", dim()),
        minds::Standing::Linked { opens: false } => (
            "cannot be opened — untick it to unlink",
            Style::default().fg(Color::Red),
        ),
        minds::Standing::Found { mind: true } => ("mind project", dim()),
        minds::Standing::Found { mind: false } => ("git repository — mind init first", warn()),
    }
}

/// Drawn while a command runs, which for a gather means the network.
fn working(busy: &str, frame: &mut Frame, area: Rect) {
    let text = printable(busy);
    let width = u16::try_from(text.chars().count() + 4)
        .unwrap_or(u16::MAX)
        .clamp(30, area.width.max(1));
    let box_ = centred(area, width, 3);
    frame.render_widget(Clear, box_);
    let block = Block::bordered()
        .border_type(BorderType::Thick)
        .border_style(Style::default().fg(Color::Yellow))
        .title(" working ");
    frame.render_widget(Paragraph::new(text).block(block), box_);
}

// ---------------------------------------------------------------------------
// Pieces
// ---------------------------------------------------------------------------

/// A cleared, bordered box in the middle of the screen, and the room inside
/// it.
fn popup(frame: &mut Frame, area: Rect, title: &str, width: u16, height: u16) -> Rect {
    let box_ = centred(area, width, height);
    frame.render_widget(Clear, box_);
    let block = Block::bordered()
        .border_type(BorderType::Thick)
        .border_style(Style::default().fg(Color::Cyan))
        .title(format!(" {title} "));
    let inner = block.inner(box_);
    frame.render_widget(block, box_);
    inner
}

/// `count` rows of one line each, from the top of `area`.
fn lines(area: Rect, count: usize) -> Vec<Rect> {
    Layout::default()
        .direction(Direction::Vertical)
        .constraints(vec![Constraint::Length(1); count])
        .split(area)
        .to_vec()
}

/// A form's label, lit when the keyboard is on its line.
fn label(text: &str, focused: bool) -> Span<'static> {
    let style = if focused {
        bold().fg(Color::Cyan)
    } else {
        dim()
    };
    Span::styled(format!("{text:<width$}", width = usize::from(LABEL)), style)
}

/// One labelled line of typing, with the terminal's cursor in it when the
/// keyboard is there.
fn input(frame: &mut Frame, area: Rect, name: &str, input: &Input, focused: bool) {
    let room = usize::from(area.width.saturating_sub(LABEL));
    let (visible, cursor) = window(input, room);
    let line = Line::from(vec![label(name, focused), Span::raw(visible)]);
    frame.render_widget(Paragraph::new(line), area);
    if focused {
        let x = area.x + LABEL + u16::try_from(cursor).unwrap_or(0);
        frame.set_cursor_position((x.min(area.right().saturating_sub(1)), area.y));
    }
}

/// The part of an input that fits in `room` cells, and where the cursor sits
/// inside it. Scrolled so the cursor is always in view.
fn window(input: &Input, room: usize) -> (String, usize) {
    if room == 0 {
        return (String::new(), 0);
    }
    let start = input.cursor().saturating_sub(room - 1);
    let visible = input
        .text()
        .chars()
        .skip(start)
        .take(room)
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    (visible, input.cursor() - start)
}

/// The command a form is about to run, as it would be typed.
fn runs(frame: &mut Frame, area: Rect, task: Option<&Task>) {
    let text = task.map_or_else(String::new, |task| printable(&task.describe()));
    let line = Line::from(vec![
        Span::styled(
            format!("{:<width$}", "runs", width = usize::from(LABEL)),
            dim(),
        ),
        Span::styled(text, Style::default().fg(Color::Gray)),
    ]);
    frame.render_widget(Paragraph::new(line), area);
}

fn error(frame: &mut Frame, area: Rect, error: Option<&str>) {
    if let Some(error) = error {
        frame.render_widget(
            Paragraph::new(Span::styled(
                printable(error),
                Style::default().fg(Color::Red),
            )),
            area,
        );
    }
}

fn keys(frame: &mut Frame, area: Rect, text: &str) {
    frame.render_widget(Paragraph::new(Span::styled(text.to_owned(), dim())), area);
}

/// A line of output made safe to draw, keeping its indentation: a tab becomes
/// spaces, and any other control character a space, so a carriage return in
/// somebody's file cannot move the cursor and draw over another line.
fn clean(text: &str) -> String {
    text.chars()
        .flat_map(|c| match c {
            '\t' => vec![' '; 4],
            c if c.is_control() => vec![' '],
            c => vec![c],
        })
        .collect()
}

fn tone(tone: Tone) -> Style {
    match tone {
        Tone::Plain => Style::default(),
        Tone::Info => Style::default().fg(Color::Gray),
        Tone::Good => Style::default().fg(Color::Green),
        Tone::Warn => warn(),
        Tone::Bad => Style::default().fg(Color::Red),
    }
}

fn bold() -> Style {
    Style::default().add_modifier(Modifier::BOLD)
}

fn dim() -> Style {
    Style::default().fg(Color::DarkGray)
}

fn warn() -> Style {
    Style::default().fg(Color::Yellow)
}
