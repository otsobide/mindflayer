//! The environment variables Claude Code reads as empty where it expands
//! a reference to one in a remote server's `url` or `headers`: credentials
//! of its own and of the tools on a machine, which a repository's
//! `.mcp.json` must not be able to send to a server of its choosing.
//!
//! An input of an address read from one of them reaches its server empty
//! in Claude, so Claude cannot be given it
//! ([`Inexpressible::VariableBlanked`](super::Inexpressible::VariableBlanked)),
//! and its renderer refuses it rather than write a header that is sent
//! with nothing in it. Nothing of this holds for a program, whose `args`
//! and `env` Claude expands in full but for a shorter list of its own
//! session's, which no server's input is read from.
//!
//! Read from Claude Code 2.1.318's binary (its `n8` expansion, given a
//! remote sink, and the lists it is handed), and not run: docs-only, as
//! `docs/agents.md`'s "MCP servers" says, since none of #18's checks names
//! one of them. Every name is compared in any case, as Claude compares
//! it, and the names it keeps under a GitHub Actions input's `INPUT_` too
//! wherever it does. Not followed: a credential-shaped name under a
//! packaging tool's prefix (`ORG_GRADLE_PROJECT_`, `POETRY_`, `CONAN_`,
//! `BUNDLE_`, `INPUT_`), which Claude judges by a pattern of words, and
//! the names it empties only in some environments (under GitHub Actions
//! or `CLAUDE_CODE_SUBPROCESS_ENV_SCRUB`, every credential-shaped name but
//! GitHub's own tokens), which no file can know it will be read in.

/// Claude's own credentials and the tools' it keeps from a remote server,
/// read as empty there both as they are and under `INPUT_`.
const BLANKED: &[&str] = &[
    // Claude's own, and the session's it runs in.
    "ANTHROPIC_API_KEY",
    "ANTHROPIC_AUTH_TOKEN",
    "ANTHROPIC_AWS_API_KEY",
    "ANTHROPIC_CUSTOM_HEADERS",
    "ANTHROPIC_FOUNDRY_API_KEY",
    "ANTHROPIC_FOUNDRY_AUTH_TOKEN",
    "ANTHROPIC_IDENTITY_TOKEN",
    "ANTHROPIC_IDENTITY_TOKEN_FILE",
    "CLAUDE_CODE_ARTIFACTS_API_TOKEN",
    "CLAUDE_CODE_CLIENT_CERT",
    "CLAUDE_CODE_CLIENT_KEY",
    "CLAUDE_CODE_CLIENT_KEY_PASSPHRASE",
    "CLAUDE_CODE_HOST_CREDS_FILE",
    "CLAUDE_CODE_MEMORY_API_TOKEN",
    "CLAUDE_CODE_MESSAGING_TOKEN",
    "CLAUDE_CODE_OAUTH_REFRESH_TOKEN",
    "CLAUDE_CODE_OAUTH_TOKEN",
    "CLAUDE_CODE_SESSIONS_OAUTH_TOKEN",
    "CLAUDE_CODE_SLACK_TAG_TOKEN",
    "CLAUDE_SESSION_INGRESS_TOKEN_FILE",
    "ENVIRONMENT_SERVICE_KEY",
    "MCP_CLIENT_SECRET",
    "MCP_XAA_IDP_CLIENT_SECRET",
    "SELF_HOSTED_RUNNER_ENVIRONMENT_SECRET",
    "SELF_HOSTED_RUNNER_POOL_SECRET",
    // Cloud accounts.
    "AWS_BEARER_TOKEN_BEDROCK",
    "AWS_CONTAINER_AUTHORIZATION_TOKEN",
    "AWS_CONTAINER_AUTHORIZATION_TOKEN_FILE",
    "AWS_CONTAINER_CREDENTIALS_FULL_URI",
    "AWS_CONTAINER_CREDENTIALS_RELATIVE_URI",
    "AWS_SECRET_ACCESS_KEY",
    "AWS_SESSION_TOKEN",
    "AWS_WEB_IDENTITY_TOKEN_FILE",
    "AZURE_AUTH_LOCATION",
    "AZURE_CLIENT_CERTIFICATE_PASSWORD",
    "AZURE_CLIENT_CERTIFICATE_PATH",
    "AZURE_CLIENT_SECRET",
    "AZURE_FEDERATED_TOKEN_FILE",
    "AZURE_PASSWORD",
    "CLOUDSDK_AUTH_ACCESS_TOKEN",
    "CLOUDSDK_AUTH_ACCESS_TOKEN_FILE",
    "CLOUDSDK_AUTH_AUTHORIZATION_TOKEN_FILE",
    "GOOGLE_APPLICATION_CREDENTIALS",
    "GOOGLE_GHA_CREDS_PATH",
    "GOOGLE_OAUTH_ACCESS_TOKEN",
    "IDENTITY_HEADER",
    "MSI_SECRET",
    // Package registries, and the proxies and indexes a tool is sent to.
    "ALL_PROXY",
    "ANACONDA_API_TOKEN",
    "ARTIFACTS_CREDENTIALPROVIDER_ACCESSTOKEN",
    "ARTIFACTS_CREDENTIALPROVIDER_EXTERNAL_FEED_ENDPOINTS",
    "BINSTAR_API_TOKEN",
    "CARGO_REGISTRY_TOKEN",
    "CODEARTIFACT_AUTH_TOKEN",
    "COMPOSER_AUTH",
    "CONAN_LOGIN_USERNAME",
    "CONAN_PASSWORD",
    "FLIT_PASSWORD",
    "FLIT_USERNAME",
    "GEM_HOST_API_KEY",
    "GOAUTH",
    "GOPROXY",
    "HATCH_INDEX_AUTH",
    "HATCH_INDEX_USER",
    "HTTPS_PROXY",
    "HTTP_PROXY",
    "MATURIN_PASSWORD",
    "MATURIN_PYPI_TOKEN",
    "MATURIN_USERNAME",
    "NODE_AUTH_TOKEN",
    "NPM_TOKEN",
    "NUGET_AUTH_TOKEN",
    "PIP_EXTRA_INDEX_URL",
    "PIP_INDEX_URL",
    "PYPI_API_TOKEN",
    "PYPI_TOKEN",
    "TWINE_PASSWORD",
    "TWINE_USERNAME",
    "UV_DEFAULT_INDEX",
    "UV_EXTRA_INDEX_URL",
    "UV_INDEX",
    "UV_INDEX_URL",
    "UV_PUBLISH_PASSWORD",
    "UV_PUBLISH_TOKEN",
    "UV_PUBLISH_USERNAME",
    "VSS_NUGET_ACCESSTOKEN",
    "VSS_NUGET_EXTERNAL_FEED_ENDPOINTS",
    // Secret stores, build and review services, and webhooks.
    "ACTIONS_ID_TOKEN_REQUEST_TOKEN",
    "ACTIONS_ID_TOKEN_REQUEST_URL",
    "ACTIONS_RUNTIME_TOKEN",
    "ACTIONS_RUNTIME_URL",
    "ALL_INPUTS",
    "CI_DEPLOY_USER",
    "CI_REGISTRY_USER",
    "CONSUL_HTTP_AUTH",
    "CONSUL_HTTP_TOKEN",
    "DEFAULT_WORKFLOW_TOKEN",
    "DISCORD_WEBHOOK",
    "DISCORD_WEBHOOK_URL",
    "FASTLANE_SESSION",
    "JF_USER",
    "MATCH_GIT_BASIC_AUTHORIZATION",
    "MS_TEAMS_WEBHOOK_URI",
    "NOMAD_HTTP_AUTH",
    "NOMAD_TOKEN",
    "OVERRIDE_GITHUB_TOKEN",
    "SLACK_WEBHOOK",
    "SLACK_WEBHOOK_URL",
    "SONARQUBE_SCANNER_PARAMS",
    "SONAR_SCANNER_JSON_PARAMS",
    "SONAR_TOKEN",
    "SSH_SIGNING_KEY",
    "TEAMS_WEBHOOK_URL",
    "VAULT_AUTH_TOKEN",
    "VAULT_ROLE_ID",
    "VAULT_SECRET_ID",
    "VAULT_TOKEN",
];

/// Claude's own credentials it keeps from a remote server only as they
/// are: what its background sessions and its bridge are handed, which it
/// never looks for under `INPUT_`.
const BLANKED_AS_THEY_ARE: &[&str] = &[
    "AGENT_PROXY_AUTH_TOKEN",
    "CCR_AGENT_PROXY_TOKEN_FILE_DESCRIPTOR",
    "CLAUDE_BG_AUTH_SNAPSHOT_PATH",
    "CLAUDE_BG_CLAIM_AUTH",
    "CLAUDE_BG_PTY_AUTH",
    "CLAUDE_BG_RV_AUTH",
    "CLAUDE_BG_SOCKET_TOKENS_PATH",
    "CLAUDE_BRIDGE_OAUTH_TOKEN",
    "CLAUDE_CODE_API_KEY_FILE_DESCRIPTOR",
    "CLAUDE_CODE_GATEWAY_TOKEN_FILE_DESCRIPTOR",
    "CLAUDE_CODE_HFI_BEARER_TOKEN",
    "CLAUDE_CODE_MCP_SERVE_AUTH_TOKEN",
    "CLAUDE_CODE_OAUTH_TOKEN_FILE_DESCRIPTOR",
    "CLAUDE_CODE_WEBSOCKET_AUTH_FILE_DESCRIPTOR",
    "CLAUDE_TRUSTED_DEVICE_TOKEN",
];

/// A GitHub Actions input, under which Claude looks for each of
/// [`BLANKED`] and every shape below as well.
const INPUT: &str = "INPUT_";

/// Whether Claude reads `variable` as empty in a remote server's `url` or
/// `headers`, in any case: one of the names above, or a name of a shape it
/// keeps from one whatever it is called, telemetry's settings (`OTEL_`,
/// whose exporter's headers carry its credential), git's configuration
/// given through the environment (`GIT_CONFIG_PARAMETERS`,
/// `GIT_CONFIG_KEY_<n>`, `GIT_CONFIG_VALUE_<n>`) and a Cargo registry's
/// token (`CARGO_REGISTRIES_<name>_TOKEN`).
pub(super) fn by_claude(variable: &str) -> bool {
    let name = variable.to_ascii_uppercase();
    if BLANKED_AS_THEY_ARE.contains(&name.as_str()) {
        return true;
    }
    let bare = name.strip_prefix(INPUT).unwrap_or(&name);
    BLANKED.contains(&bare) || shaped(bare)
}

/// Whether `name`, in capitals and with no `INPUT_` before it, is of a
/// shape Claude keeps from a remote server whatever it is called.
fn shaped(name: &str) -> bool {
    let numbered = |rest: &str| !rest.is_empty() && rest.bytes().all(|byte| byte.is_ascii_digit());
    let git = name == "GIT_CONFIG_PARAMETERS"
        || name
            .strip_prefix("GIT_CONFIG_KEY_")
            .or_else(|| name.strip_prefix("GIT_CONFIG_VALUE_"))
            .is_some_and(numbered);
    let cargo = name
        .strip_prefix("CARGO_REGISTRIES_")
        .and_then(|rest| rest.strip_suffix("_TOKEN"))
        .is_some_and(|registry| {
            !registry.is_empty()
                && registry
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        });
    name.starts_with("OTEL_") || git || cargo
}

#[cfg(test)]
#[path = "blanked_test.rs"]
mod tests;
