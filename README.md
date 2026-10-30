# 🧠 Mindflayer

Manage what your coding agents read, the way git manages code: a project
carries a `.mind` directory beside its **skills** and **rules**, and a
workspace above it carries a `.mindflayer` that orchestrates several such
projects at once.

One shared Rust engine (`mindflayer-core`) behind every front end: the CLI
(`mind` for a project, `flayer` for a workspace), the TUI it opens, and a
[control panel](docs/reference.md#control-panel) in your browser, served by
the same binary.

Everything it does, command by command, is in [the
reference](docs/reference.md), and working on it in
[docs/development.md](docs/development.md).

## Roadmap

What comes next is planned as
[milestones](https://github.com/otsobide/mindflayer/milestones), each ending
with something usable on real repositories, and each issue in them is one pull
request. [#15](https://github.com/otsobide/mindflayer/issues/15) tracks the
whole plan, and the decisions it rests on are recorded in
[docs/architecture.md](docs/architecture.md#decisions).

## License

GPL-3.0-only. See [LICENSE](LICENSE).
