use super::*;
use crate::mother::McpMother;

#[test]
fn claude_reads_its_own_and_the_tools_credentials_as_empty_toward_a_remote_server() {
    for (why, variable) in McpMother::blanked_variables() {
        assert!(by_claude(&variable), "{why}: {variable}");
    }
}

#[test]
fn claude_passes_on_a_name_near_one_it_keeps_from_a_remote_server() {
    for (why, variable) in McpMother::passed_variables() {
        assert!(!by_claude(&variable), "{why}: {variable}");
    }
}

#[test]
fn every_name_it_keeps_is_kept_in_capitals_and_once() {
    let mut seen: Vec<&str> = Vec::new();
    for name in BLANKED.iter().chain(BLANKED_AS_THEY_ARE) {
        assert_eq!(*name, name.to_ascii_uppercase(), "{name}");
        assert!(!name.starts_with(INPUT), "{name}");
        assert!(!seen.contains(name), "{name} is listed twice");
        seen.push(name);
    }
}
