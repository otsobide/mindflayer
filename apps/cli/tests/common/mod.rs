//! What every CLI suite shares: running the real parsers, and the fixtures
//! and screens they drive.
//!
//! Everything is re-exported so a suite says `use common::*;` and nothing
//! else; not every suite uses every helper.

#![allow(dead_code, unused_imports)]

pub use clap::Parser;
pub use mindflayer_cli::install::state::{Focus, Screen, Step};
pub use mindflayer_cli::install::{self as install_screen, ui};
pub use mindflayer_cli::tui;
pub use mindflayer_cli::tui::state::{App, Holder, Mode, Scope, Task, View};
pub use mindflayer_cli::{run, run_flayer_cli, Cli, CliError, Failure, FlayerCli, Outcome};
pub use mindflayer_core::install::Standing;
pub use mindflayer_core::ledger::Ledger;
pub use mindflayer_core::FlayerWorkspace;
pub use mindflayer_core::{Kind, MindProject, FLAYER_CONFIG, FLAYER_DIR, MIND_CONFIG, MIND_DIR};
pub use ratatui::backend::TestBackend;
pub use ratatui::crossterm::event::KeyCode;
pub use ratatui::crossterm::event::{KeyEvent, KeyModifiers};
pub use ratatui::Terminal;
pub use std::fs;
pub use std::path::{Path, PathBuf};
pub use tempfile::TempDir;

/// Build an argument line with `-C dir` appended, and `--home ""` so no test
/// ever falls back to, or creates, the workspace in the real home.
pub fn line(binary: &str, args: &[&str], dir: &Path) -> Vec<String> {
    let mut line: Vec<String> = vec![binary.to_owned()];
    line.extend(args.iter().map(|arg| (*arg).to_owned()));
    line.push("-C".to_owned());
    line.push(dir.to_string_lossy().into_owned());
    if !args.contains(&"--home") {
        line.push("--home=".to_owned());
    }
    line
}

/// Run `mind ...`.
///
/// `try_parse_from` rather than `parse_from`: the latter exits the process on a
/// parse error, which in a test harness kills the whole run and hides which
/// argument line was at fault.
pub fn mind(dir: &Path, args: &[&str]) -> Result<Outcome, Failure> {
    let line = line("mind", args, dir);
    let cli = Cli::try_parse_from(&line).unwrap_or_else(|error| panic!("{line:?}: {error}"));
    run(&cli)
}

/// Run `flayer ...`, through the second binary's parser.
pub fn flayer(dir: &Path, args: &[&str]) -> Result<Outcome, Failure> {
    let line = line("flayer", args, dir);
    let cli = FlayerCli::try_parse_from(&line).unwrap_or_else(|error| panic!("{line:?}: {error}"));
    run_flayer_cli(&cli)
}

/// Write a skill into an already initialized mind project.
/// Where an initialized project keeps a kind, asked of the project itself
/// rather than assumed, so a test that moves one still writes to the right
/// place.
pub fn directory_for(root: &Path, kind: Kind) -> PathBuf {
    MindProject::open(root)
        .expect("an initialized mind project")
        .directory_for(kind)
}

pub fn write_skill(root: &Path, directory: &str, contents: &str) {
    let dir = directory_for(root, Kind::Skill).join(directory);
    fs::create_dir_all(&dir).expect("create the skill directory");
    fs::write(dir.join("SKILL.md"), contents).expect("write SKILL.md");
}

pub fn skill_file(name: &str, description: &str) -> String {
    format!("---\nname: {name}\ndescription: {description}\n---\n\n# {name}\n\nSteps.\n")
}

/// A workspace with `alpha` and `beta` linked, each holding one skill.
pub fn workspace_with_two() -> TempDir {
    let dir = TempDir::new().unwrap();
    flayer(dir.path(), &["init"]).unwrap();
    for name in ["alpha", "beta"] {
        let root = dir.path().join(name);
        fs::create_dir(&root).unwrap();
        mind(&root, &["init"]).unwrap();
        write_skill(&root, name, &skill_file(name, "A skill"));
        flayer(dir.path(), &["link", name]).unwrap();
    }
    dir
}

/// A workspace with `gamma` and `delta` initialized under it but not linked.
pub fn workspace_with_two_unlinked() -> TempDir {
    let dir = TempDir::new().unwrap();
    flayer(dir.path(), &["init"]).unwrap();
    for name in ["gamma", "delta"] {
        let root = dir.path().join(name);
        fs::create_dir(&root).unwrap();
        mind(&root, &["init"]).unwrap();
    }
    dir
}

/// Run `flayer ...` with `home` as the home directory.
pub fn flayer_at_home(dir: &Path, home: &Path, args: &[&str]) -> Result<Outcome, Failure> {
    let mut args: Vec<&str> = args.to_vec();
    let home = home.to_string_lossy().into_owned();
    args.push("--home");
    args.push(&home);
    flayer(dir, &args)
}

/// Write a rule into an already initialized mind project.
pub fn write_rule(root: &Path, route: &str, contents: &str) {
    let path = directory_for(root, Kind::Rule).join(route);
    fs::create_dir_all(path.parent().unwrap()).expect("create the rule folder");
    fs::write(path, contents).expect("write the rule");
}

/// A git repository holding these files, with one commit.
pub fn repository(files: &[(&str, &str)]) -> TempDir {
    let dir = TempDir::new().unwrap();
    for (path, contents) in files {
        let file = dir.path().join(path);
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(file, contents).unwrap();
    }

    let repo = gix::init(dir.path()).expect("initialize a repository");
    let tree = write_tree(&repo, dir.path());
    let who = gix::actor::Signature {
        name: "Fixture".into(),
        email: "fixture@example.com".into(),
        time: gix::date::Time::new(0, 0),
    };
    let id = repo
        .write_object(&gix::objs::Commit {
            tree,
            parents: Default::default(),
            author: who.clone(),
            committer: who,
            encoding: None,
            message: "fixture".into(),
            extra_headers: Vec::new(),
        })
        .unwrap()
        .detach();

    let head = fs::read_to_string(dir.path().join(".git").join("HEAD")).unwrap();
    let branch = head.trim().strip_prefix("ref: ").unwrap();
    let reference = dir.path().join(".git").join(branch);
    fs::create_dir_all(reference.parent().unwrap()).unwrap();
    fs::write(reference, format!("{id}\n")).unwrap();
    dir
}

pub fn write_tree(repo: &gix::Repository, dir: &Path) -> gix::ObjectId {
    use gix::objs::tree::{Entry, EntryKind};

    let mut entries = Vec::new();
    for entry in fs::read_dir(dir).unwrap() {
        let entry = entry.unwrap();
        let name = entry.file_name();
        if name == ".git" {
            continue;
        }
        let path = entry.path();
        let (kind, oid) = if path.is_dir() {
            (EntryKind::Tree, write_tree(repo, &path))
        } else {
            (
                EntryKind::Blob,
                repo.write_blob(fs::read(&path).unwrap()).unwrap().detach(),
            )
        };
        entries.push(Entry {
            mode: kind.into(),
            filename: name.to_string_lossy().as_bytes().into(),
            oid,
        });
    }
    entries.sort();
    repo.write_object(&gix::objs::Tree { entries })
        .unwrap()
        .detach()
}

/// A workspace with `projects` linked and a shelf holding `skills`.
pub fn shelved(projects: &[&str], skills: &[&str]) -> (TempDir, FlayerWorkspace, Ledger) {
    let dir = TempDir::new().unwrap();
    flayer(dir.path(), &["init"]).unwrap();
    for name in projects {
        let root = dir.path().join(name);
        fs::create_dir(&root).unwrap();
        mind(&root, &["init"]).unwrap();
        flayer(dir.path(), &["link", name]).unwrap();
    }

    let files: Vec<(String, String)> = skills
        .iter()
        .map(|name| {
            (
                format!("skills/{name}/SKILL.md"),
                skill_file(name, &format!("What {name} does")),
            )
        })
        .collect();
    let borrowed: Vec<(&str, &str)> = files
        .iter()
        .map(|(p, b)| (p.as_str(), b.as_str()))
        .collect();
    let source = repository(&borrowed);
    flayer(
        dir.path(),
        &["gather", "git", &source.path().to_string_lossy()],
    )
    .unwrap();

    let workspace = FlayerWorkspace::locate(dir.path()).unwrap().unwrap();
    let ledger = workspace.ledger().unwrap();
    (dir, workspace, ledger)
}

pub fn screen_for(workspace: &FlayerWorkspace, ledger: &Ledger) -> Screen {
    let (projects, _) = install_screen::registered(workspace);
    install_screen::build(workspace, ledger, projects).unwrap()
}

/// Everything the screen would show, as plain text.
pub fn rendered(screen: &Screen) -> String {
    let mut terminal = Terminal::new(TestBackend::new(96, 24)).unwrap();
    terminal.draw(|frame| ui::draw(screen, frame)).unwrap();
    terminal
        .backend()
        .buffer()
        .content()
        .chunks(96)
        .map(|line| line.iter().map(|cell| cell.symbol()).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn press(screen: &mut Screen, keys: &[KeyCode]) -> Step {
    let mut step = Step::Stay;
    for key in keys {
        step = screen.press(*key);
    }
    step
}

pub fn tui_in(dir: &Path) -> App {
    App::new(dir.to_path_buf(), tui::load(dir, None).unwrap())
}

pub fn press_key(app: &mut App, code: KeyCode) -> tui::state::Step {
    app.press(KeyEvent::from(code))
}

pub fn type_text(app: &mut App, text: &str) {
    for c in text.chars() {
        app.press(KeyEvent::from(KeyCode::Char(c)));
    }
}

/// Press a key and, if it asked for work, do the work the way the loop does.
pub fn press_and_run(app: &mut App, code: KeyCode) -> Option<Task> {
    match press_key(app, code) {
        tui::state::Step::Run(task) => {
            tui::step(app, task.clone());
            Some(task)
        }
        _ => None,
    }
}

pub fn tui_drawn(app: &App) -> String {
    let mut terminal = Terminal::new(TestBackend::new(110, 30)).unwrap();
    terminal.draw(|frame| tui::ui::draw(app, frame)).unwrap();
    terminal
        .backend()
        .buffer()
        .content()
        .chunks(110)
        .map(|line| line.iter().map(|cell| cell.symbol()).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn registry(dir: &Path) -> String {
    fs::read_to_string(dir.join(FLAYER_DIR).join(FLAYER_CONFIG)).unwrap()
}

/// `workspace_with_two`, plus `gamma`, a mind project nobody linked, and
/// `repo`, a git repository that is not a mind project yet.
pub fn workspace_with_candidates() -> TempDir {
    let dir = workspace_with_two();
    let gamma = dir.path().join("gamma");
    fs::create_dir(&gamma).unwrap();
    mind(&gamma, &["init"]).unwrap();
    fs::create_dir_all(dir.path().join("repo/.git")).unwrap();
    dir
}

/// Install `deploy` from the shelf into `collapse`, through the screen.
pub fn installed(dir: &Path, workspace: &FlayerWorkspace, ledger: &Ledger) {
    let mut screen = screen_for(workspace, ledger);
    press(&mut screen, &[KeyCode::Right, KeyCode::Char(' ')]);
    install_screen::apply(workspace, ledger, &screen).unwrap();
    assert!(dir.join("collapse/skills/deploy/SKILL.md").is_file());
}

/// `workspace_with_two`, with a skill of the workspace's own.
pub fn workspace_with_own() -> TempDir {
    let dir = workspace_with_two();
    flayer(
        dir.path(),
        &[
            "add",
            "skill",
            "commit-style",
            "How every repo here writes commits",
        ],
    )
    .unwrap();
    dir
}

/// The TUI `mind` opens in `dir`.
pub fn mind_tui_in(dir: &Path) -> App {
    let mut app = App::opened_on(
        Scope::Project,
        dir.to_path_buf(),
        tui::load_project(dir, None).unwrap(),
    );
    app.focus_on(dir);
    app
}
