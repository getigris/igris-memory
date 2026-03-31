use rmcp::schemars;
use serde::Deserialize;

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SaveArgs {
    #[schemars(
        description = "Short descriptive title (e.g. 'Auth middleware design', 'Fix login null pointer')"
    )]
    pub title: String,
    #[schemars(
        description = "Full content — what happened, why, what was decided, and what to remember. Be detailed; this is what future sessions will read."
    )]
    pub content: String,
    #[schemars(
        description = "Memory category. Values: decision, architecture, bugfix, pattern, config, discovery, learning, plan, manual. Use 'plan' for execution plans (delete when done). Default: manual"
    )]
    #[serde(rename = "type", default = "default_type")]
    pub observation_type: String,
    #[schemars(description = "Project name this memory belongs to (e.g. 'web-api', 'mobile-app')")]
    pub project: Option<String>,
    #[schemars(description = "Visibility scope: 'project' (default) or 'personal'")]
    #[serde(default = "default_scope")]
    pub scope: String,
    #[schemars(
        description = "Stable identifier for evolving knowledge (e.g. 'architecture/auth', 'plan/http-api'). Saving with an existing topic_key updates in place instead of creating a duplicate."
    )]
    pub topic_key: Option<String>,
    #[schemars(description = "Tags for categorization (e.g. ['rust', 'auth', 'jwt'])")]
    pub tags: Option<Vec<String>>,
    #[schemars(description = "Session ID to associate this memory with")]
    pub session_id: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SearchArgs {
    #[schemars(
        description = "Search query — use natural language or keywords (e.g. 'authentication JWT', 'database migration')"
    )]
    pub query: String,
    #[schemars(
        description = "Filter by type: decision, architecture, bugfix, pattern, config, discovery, learning, plan, manual"
    )]
    #[serde(rename = "type")]
    pub observation_type: Option<String>,
    #[schemars(description = "Filter by project name")]
    pub project: Option<String>,
    #[schemars(description = "Max results (default 20, max 50)")]
    pub limit: Option<i64>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GetArgs {
    #[schemars(description = "Memory ID to retrieve")]
    pub id: i64,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct UpdateArgs {
    #[schemars(description = "Memory ID to update")]
    pub id: i64,
    #[schemars(description = "New title (only if changing)")]
    pub title: Option<String>,
    #[schemars(description = "New content (only if changing)")]
    pub content: Option<String>,
    #[schemars(description = "New type (only if changing)")]
    #[serde(rename = "type")]
    pub observation_type: Option<String>,
    #[schemars(description = "New tags (replaces existing)")]
    pub tags: Option<Vec<String>>,
    #[schemars(description = "New topic key (only if changing)")]
    pub topic_key: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct DeleteArgs {
    #[schemars(description = "Memory ID to soft-delete")]
    pub id: i64,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ContextArgs {
    #[schemars(description = "Filter by project name (omit for all projects)")]
    pub project: Option<String>,
    #[schemars(description = "Max results (default 20, max 50)")]
    pub limit: Option<i64>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct TimelineArgs {
    #[schemars(description = "Memory ID to center the timeline on")]
    pub observation_id: i64,
    #[schemars(description = "Memories to show before (default 5, max 50)")]
    pub before: Option<i64>,
    #[schemars(description = "Memories to show after (default 5, max 50)")]
    pub after: Option<i64>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct PurgeArgs {
    #[schemars(
        description = "Delete memories soft-deleted more than this many days ago. Use 0 to purge all."
    )]
    pub older_than_days: i64,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ImportArgs {
    #[schemars(description = "JSON string from igris_export")]
    pub data: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SuggestTopicKeyArgs {
    #[schemars(description = "Observation type (e.g. decision, architecture, plan)")]
    #[serde(rename = "type")]
    pub observation_type: String,
    #[schemars(description = "Title of the observation")]
    pub title: String,
    #[schemars(description = "Content of the observation")]
    pub content: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SessionStartArgs {
    #[schemars(description = "Unique session ID (UUID recommended)")]
    pub id: String,
    #[schemars(description = "Project name")]
    pub project: String,
    #[schemars(description = "Working directory path")]
    pub directory: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SessionEndArgs {
    #[schemars(description = "Session ID to close")]
    pub id: String,
    #[schemars(description = "Brief summary of what was accomplished")]
    pub summary: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SessionSummaryArgs {
    #[schemars(
        description = "Structured summary — what was accomplished, key decisions, next steps. This is what the next session will read first."
    )]
    pub content: String,
    #[schemars(description = "Project name")]
    pub project: String,
}

pub fn default_type() -> String {
    "manual".to_string()
}

pub fn default_scope() -> String {
    "project".to_string()
}
