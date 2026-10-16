use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolDefinition {
    pub id: String,
    pub name: String,
    pub description: String,
    pub commands: Vec<String>,
    pub homepage: String,
    pub repository: String,
    pub runtime_provider: Option<String>,
    pub packages: Vec<PackageMapping>,
    pub registered: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageMapping {
    pub provider: String,
    pub package: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ToolInstallation {
    pub id: String,
    pub tool_id: String,
    pub version: Option<String>,
    pub path: String,
    pub provider: Option<String>,
    pub package: Option<String>,
    pub evidence: String,
    pub resolved: bool,
    pub status: String,
    pub diagnostic: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderCapabilities {
    pub id: String,
    pub available: bool,
    pub executable: Option<String>,
    pub version: Option<String>,
    pub diagnostic: Option<String>,
    pub install: bool,
    pub upgrade: bool,
    pub uninstall: bool,
    pub versions: bool,
    pub set_default: bool,
    pub guide: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ToolMetadata {
    pub tool_id: String,
    pub description: String,
    pub homepage: String,
    pub repository: String,
    pub license: Option<String>,
    pub latest_release: Option<String>,
    pub release_url: Option<String>,
    pub fetched_at: String,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolSnapshot {
    pub definitions: Vec<ToolDefinition>,
    pub installations: Vec<ToolInstallation>,
    pub providers: Vec<ProviderCapabilities>,
    pub metadata: Vec<ToolMetadata>,
    pub projects: Vec<String>,
    pub directories: Vec<String>,
    pub scanned_at: Option<String>,
    pub diagnostics: Vec<String>,
    #[serde(default)]
    pub updates: Vec<ToolUpdate>,
    #[serde(default)]
    pub updates_checked_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolUpdate {
    pub tool_id: String,
    pub installation_id: String,
    pub provider: String,
    pub installed_version: String,
    pub target_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolUpdateCheck {
    pub updates: Vec<ToolUpdate>,
    pub checked_at: String,
    pub diagnostics: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolCandidate {
    pub name: String,
    pub description: String,
    pub repository: String,
    pub homepage: String,
    pub provider: String,
    pub package: Option<String>,
    pub tool_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolSearch {
    pub candidates: Vec<ToolCandidate>,
    pub diagnostics: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolVersion {
    pub version: String,
    #[serde(default)]
    pub identity: Option<String>,
    pub installed: bool,
    pub default: bool,
    pub path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolVersions {
    pub provider: String,
    pub versions: Vec<ToolVersion>,
    pub default_version: Option<String>,
    pub diagnostic: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectToolConfig {
    pub directory: String,
    pub files: Vec<FileChange>,
    pub selected_version: Option<String>,
    pub effective_version: Option<String>,
    pub diagnostics: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolOperationRequest {
    pub tool_id: String,
    pub action: String,
    pub installation_id: Option<String>,
    pub provider: Option<String>,
    pub version: Option<String>,
    pub project_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileChange {
    pub path: String,
    pub before: Option<String>,
    pub after: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolOperationPlan {
    pub id: String,
    pub request: ToolOperationRequest,
    pub executable: Option<String>,
    pub args: Vec<String>,
    pub files: Vec<FileChange>,
    pub impact: Vec<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolOperation {
    pub id: String,
    pub tool_id: String,
    pub action: String,
    pub provider: String,
    pub status: String,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub output: String,
    pub exit_code: Option<i32>,
    pub backup_paths: Vec<String>,
}
