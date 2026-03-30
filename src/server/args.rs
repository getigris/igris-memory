use rmcp::schemars;
use serde::Deserialize;

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SaveArgs {
    #[schemars(description = "Short descriptive title for this memory")]
    pub title: String,
    #[schemars(description = "Full content of the memory — what happened, why, and what was learned")]
    pub content: String,
    #[schemars(description = "Category: decision, architecture, bugfix, pattern, config, discovery, learning, or manual")]
    #[serde(rename = "type", default = "default_type")]
    pub observation_type: String,
    #[schemars(description = "Project name this memory belongs to")]
    pub project: Option<String>,
    #[schemars(description = "Visibility scope: project (default) or personal")]
    #[serde(default = "default_scope")]
    pub scope: String,
    #[schemars(description = "Stable key for evolving topics (e.g. architecture/auth). Updates existing memory with same key")]
    pub topic_key: Option<String>,
    #[schemars(description = "Optional tags for categorization")]
    pub tags: Option<Vec<String>>,
    #[schemars(description = "Session ID to associate this memory with")]
    pub session_id: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SearchArgs {
    #[schemars(description = "Search query — natural language or keywords")]
    pub query: String,
    #[schemars(description = "Filter by observation type")]
    #[serde(rename = "type")]
    pub observation_type: Option<String>,
    #[schemars(description = "Filter by project name")]
    pub project: Option<String>,
    #[schemars(description = "Max results to return (default 20, max 50)")]
    pub limit: Option<i64>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GetArgs {
    #[schemars(description = "Observation ID")]
    pub id: i64,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct UpdateArgs {
    #[schemars(description = "Observation ID to update")]
    pub id: i64,
    #[schemars(description = "New title")]
    pub title: Option<String>,
    #[schemars(description = "New content")]
    pub content: Option<String>,
    #[schemars(description = "New type")]
    #[serde(rename = "type")]
    pub observation_type: Option<String>,
    #[schemars(description = "New tags")]
    pub tags: Option<Vec<String>>,
    #[schemars(description = "New topic key")]
    pub topic_key: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct DeleteArgs {
    #[schemars(description = "Observation ID to delete")]
    pub id: i64,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ContextArgs {
    #[schemars(description = "Filter by project name")]
    pub project: Option<String>,
    #[schemars(description = "Max results (default 20, max 50)")]
    pub limit: Option<i64>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct TimelineArgs {
    #[schemars(description = "Observation ID to center the timeline on")]
    pub observation_id: i64,
    #[schemars(description = "Number of observations to show before the anchor (default 5, max 50)")]
    pub before: Option<i64>,
    #[schemars(description = "Number of observations to show after the anchor (default 5, max 50)")]
    pub after: Option<i64>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct PurgeArgs {
    #[schemars(description = "Permanently delete observations that were soft-deleted more than this many days ago. Use 0 to purge all soft-deleted entries.")]
    pub older_than_days: i64,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ImportArgs {
    #[schemars(description = "JSON string containing the exported data (from igris_export)")]
    pub data: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SuggestTopicKeyArgs {
    #[schemars(description = "Observation type (e.g. decision, architecture, bugfix)")]
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
    #[schemars(description = "Optional session summary")]
    pub summary: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SessionSummaryArgs {
    #[schemars(description = "Structured summary of what was accomplished in this session")]
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
