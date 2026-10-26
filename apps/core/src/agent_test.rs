//! Unit tests for `agent.rs`, kept beside it rather than inside it.

use super::*;

#[test]
fn each_agents_default_folder_is_its_own_in_the_users_home() {
    let user = Path::new("/home/user");

    assert_eq!(
        AgentHomes::defaults_under(user),
        AgentHomes {
            claude_config_dir: Some(user.join(".claude")),
            codex_home: Some(user.join(".codex")),
            opencode_config_dir: Some(user.join(".config").join("opencode")),
            agents_dir: Some(user.join(".agents")),
        }
    );
}
