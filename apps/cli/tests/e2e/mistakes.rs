//! A line the commands cannot take, and what each says to get it right: a
//! parser's refusal exits 2 with the usage, as clap gives it, and a
//! refusal of Mindflayer's exits 1 with what to do instead.

use crate::Scenario;

#[test]
fn an_unknown_command_is_refused_with_the_usage() {
    let scenario = Scenario::projects(&["collapse"]);

    scenario
        .run("~/Projects", "flayer bogus")
        .exits(2)
        .prints("")
        .says(concat!(
            "error: unrecognized subcommand 'bogus'\n",
            "\n",
            "Usage: flayer [OPTIONS] [COMMAND]\n",
            "\n",
            "For more information, try '--help'.\n",
        ));
}

#[test]
fn a_command_missing_what_it_acts_on_says_what_it_needs() {
    let scenario = Scenario::projects(&["collapse"]);

    scenario
        .run("~/Projects/collapse", "mind show")
        .exits(2)
        .prints("")
        .says(concat!(
            "error: the following required arguments were not provided:\n",
            "  <REFERENCE>\n",
            "\n",
            "Usage: mind show <REFERENCE>\n",
            "\n",
            "For more information, try '--help'.\n",
        ));
}

#[test]
fn installing_a_name_nothing_offers_says_how_to_offer_it() {
    let scenario = Scenario::projects(&["collapse"]);

    scenario
        .run("~/Projects", "flayer install -p collapse commit-style")
        .exits(1)
        .prints("")
        .says(
            "error: nothing called `commit-style` can be installed: gather it, load a workspace \
             that has it with `flayer load`, or add it to the workspace with `flayer add`\n",
        );
}

#[test]
fn each_binary_says_its_version() {
    let scenario = Scenario::empty();
    let version = env!("CARGO_PKG_VERSION");

    scenario
        .run("~", "flayer --version")
        .succeeds()
        .prints(&format!("flayer {version}\n"))
        .quiet();
    scenario
        .run("~", "mind --version")
        .succeeds()
        .prints(&format!("mind {version}\n"))
        .quiet();
}
