use crate::environment::{self, Change, ChangeResult, Snapshot};

#[tauri::command]
pub async fn get_environment_variables() -> Result<Snapshot, String> {
    tauri::async_runtime::spawn_blocking(environment::list)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn change_environment_variable(change: Change) -> Result<ChangeResult, String> {
    tauri::async_runtime::spawn_blocking(move || environment::apply(change))
        .await
        .map_err(|e| e.to_string())?
}
