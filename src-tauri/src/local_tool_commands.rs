use crate::{desktop::AppState, local_tools::*};
use tauri::State;

#[tauri::command]
pub fn list_local_tools(state: State<AppState>) -> Result<ToolSnapshot, String> {
    state.local_tools.snapshot()
}
#[tauri::command]
pub async fn scan_local_tools(state: State<'_, AppState>) -> Result<ToolSnapshot, String> {
    state.local_tools.clone().scan().await
}
#[tauri::command]
pub async fn search_local_tools(
    query: String,
    state: State<'_, AppState>,
) -> Result<ToolSearch, String> {
    state.local_tools.clone().search(query).await
}
#[tauri::command]
pub async fn add_local_tool(
    candidate: ToolCandidate,
    state: State<'_, AppState>,
) -> Result<ToolSnapshot, String> {
    state.local_tools.clone().register(candidate).await
}
#[tauri::command]
pub async fn refresh_local_tool_metadata(
    tool_id: String,
    state: State<'_, AppState>,
) -> Result<ToolMetadata, String> {
    state.local_tools.clone().refresh_metadata(tool_id).await
}
#[tauri::command]
pub async fn local_tool_versions(
    tool_id: String,
    remote: bool,
    state: State<'_, AppState>,
) -> Result<ToolVersions, String> {
    state.local_tools.clone().versions(tool_id, remote).await
}
#[tauri::command]
pub async fn local_tool_project(
    tool_id: String,
    directory: String,
    state: State<'_, AppState>,
) -> Result<ProjectToolConfig, String> {
    state.local_tools.clone().project(tool_id, directory).await
}
#[tauri::command]
pub async fn preview_local_tool_operation(
    request: ToolOperationRequest,
    state: State<'_, AppState>,
) -> Result<ToolOperationPlan, String> {
    state.local_tools.clone().preview(request).await
}
#[tauri::command]
pub async fn execute_local_tool_operation(
    id: String,
    state: State<'_, AppState>,
) -> Result<ToolOperation, String> {
    state.local_tools.execute(id)
}
#[tauri::command]
pub fn list_local_tool_operations(state: State<AppState>) -> Result<Vec<ToolOperation>, String> {
    state.local_tools.operations()
}
#[tauri::command]
pub fn set_local_tool_directories(
    directories: Vec<String>,
    state: State<AppState>,
) -> Result<(), String> {
    state.local_tools.set_directories(directories)
}
#[tauri::command]
pub async fn check_local_tool_updates(
    state: State<'_, AppState>,
) -> Result<ToolUpdateCheck, String> {
    state.local_tools.clone().check_updates().await
}
