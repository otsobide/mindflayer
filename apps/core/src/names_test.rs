use super::*;

#[test]
fn the_new_names_are_looked_for_before_the_old() {
    assert_eq!(Names::ALL, [Names::Mindweaver, Names::Mindflayer]);
}

#[test]
fn a_workspace_is_spelled_by_whose_names_it_is_under() {
    assert_eq!(
        (
            Names::Mindweaver.workspace_dir(),
            Names::Mindweaver.workspace_config(),
            Names::Mindweaver.ledger_file()
        ),
        (".mindweaver", "weaver.toml", "mindweaver.db")
    );
    assert_eq!(
        (
            Names::Mindflayer.workspace_dir(),
            Names::Mindflayer.workspace_config(),
            Names::Mindflayer.ledger_file()
        ),
        (".mindflayer", "flayer.toml", "mindflayer.db")
    );
}

#[test]
fn names_say_the_product_they_are_of() {
    assert_eq!(Names::Mindweaver.to_string(), "Mindweaver");
    assert_eq!(Names::Mindflayer.to_string(), "Mindflayer");
}
