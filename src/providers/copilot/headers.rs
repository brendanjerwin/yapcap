// SPDX-License-Identifier: MPL-2.0

pub const EDITOR_VERSION: &str = "vscode/1.107.0";
pub const EDITOR_PLUGIN_VERSION: &str = "copilot-chat/0.35.0";
pub const USER_AGENT: &str = "GitHubCopilotChat/0.35.0";
pub const GITHUB_API_VERSION: &str = "2026-03-10";

pub const OAUTH_CLIENT_ID: &str = "Iv1.b507a08c87ecfe98";
pub const OAUTH_SCOPE: &str = "read:user";

pub const DEVICE_CODE_URL: &str = "https://github.com/login/device/code";
pub const OAUTH_TOKEN_URL: &str = "https://github.com/login/oauth/access_token";
pub const GITHUB_USER_URL: &str = "https://api.github.com/user";
pub const COPILOT_USER_URL: &str = "https://api.github.com/copilot_internal/user";

pub fn apply_copilot_headers(
    builder: reqwest::RequestBuilder,
    access_token: &str,
) -> reqwest::RequestBuilder {
    builder
        .header("Authorization", format!("token {access_token}"))
        .header("Accept", "application/json")
        .header("Editor-Version", EDITOR_VERSION)
        .header("Editor-Plugin-Version", EDITOR_PLUGIN_VERSION)
        .header("User-Agent", USER_AGENT)
        .header("X-Github-Api-Version", GITHUB_API_VERSION)
}
