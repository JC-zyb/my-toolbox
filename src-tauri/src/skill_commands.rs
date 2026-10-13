use crate::{collection, desktop::AppState, skills::*};
use std::path::PathBuf;
use tauri::{AppHandle, Manager};

async fn work<T: Send + 'static>(
    app: AppHandle,
    task: impl FnOnce(&mut SkillsService) -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let mut service = state.skills.lock().map_err(|e| e.to_string())?;
        task(&mut service)
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
pub async fn list_skills(app: AppHandle) -> Result<ScanResult, String> {
    work(app, |s| s.scan()).await
}
#[tauri::command]
pub async fn scan_skill_sources(app: AppHandle) -> Result<ScanResult, String> {
    work(app, |s| s.scan()).await
}
#[tauri::command]
pub async fn skill_matrix(
    project_path: Option<String>,
    app: AppHandle,
) -> Result<SkillMatrix, String> {
    work(app, move |s| s.skill_matrix(project_path.as_deref())).await
}
#[tauri::command]
pub async fn preview_batch_skill_operation(
    request: BatchSkillRequest,
    app: AppHandle,
) -> Result<BatchSkillPreview, String> {
    work(app, move |s| s.preview_batch_skill_operation(request)).await
}
#[tauri::command]
pub async fn remove_agent_skill_entries(
    request: BatchSkillRequest,
    app: AppHandle,
) -> Result<BatchSkillPreview, String> {
    work(app, move |s| s.remove_agent_skill_entries(request)).await
}
#[tauri::command]
pub async fn apply_batch_skill_operation(
    id: String,
    resolutions: Vec<BatchResolution>,
    app: AppHandle,
) -> Result<OperationResult, String> {
    work(app, move |s| {
        s.apply_batch_skill_operation(&id, &resolutions)
    })
    .await
}
#[tauri::command]
pub async fn preview_skill_activation(
    request: SkillActivationRequest,
    app: AppHandle,
) -> Result<SkillActivationPreview, String> {
    work(app, move |s| s.preview_skill_activation(request)).await
}
#[tauri::command]
pub async fn apply_skill_activation(
    id: String,
    resolutions: Vec<BatchResolution>,
    app: AppHandle,
) -> Result<OperationResult, String> {
    work(app, move |s| s.apply_skill_activation(&id, &resolutions)).await
}
#[tauri::command]
pub async fn quick_skill_activation(
    request: QuickSkillActivationRequest,
    app: AppHandle,
) -> Result<QuickSkillActivationResult, String> {
    work(app, move |s| s.quick_skill_activation(request)).await
}
#[tauri::command]
pub async fn get_skill(id: String, app: AppHandle) -> Result<Skill, String> {
    work(app, move |s| s.get(&id)).await
}
#[tauri::command]
pub async fn get_discovered_skill(id: String, project_path: Option<String>, app: AppHandle) -> Result<Skill, String> {
    work(app, move |s| s.get_discovered_skill(&id, project_path.as_deref())).await
}
#[tauri::command]
pub async fn create_skill(name: String, document: String, app: AppHandle) -> Result<Skill, String> {
    work(app, move |s| s.create(&name, &document)).await
}
#[tauri::command]
pub async fn update_skill(
    id: String,
    document: String,
    expected_hash: String,
    app: AppHandle,
) -> Result<Skill, String> {
    work(app, move |s| s.update(&id, &document, &expected_hash)).await
}
#[tauri::command]
pub async fn delete_skill(
    id: String,
    permanent: bool,
    expected_hash: String,
    app: AppHandle,
) -> Result<OperationResult, String> {
    work(app, move |s| s.delete(&id, permanent, &expected_hash)).await
}
#[tauri::command]
pub async fn import_local_skill(path: String, app: AppHandle) -> Result<Skill, String> {
    work(app, move |s| s.import_local(&PathBuf::from(path))).await
}
#[tauri::command]
pub async fn export_skill(
    id: String,
    destination: String,
    app: AppHandle,
) -> Result<OperationResult, String> {
    work(app, move |s| s.export(&id, &PathBuf::from(destination))).await
}
#[tauri::command]
pub async fn list_skill_targets(id: String, app: AppHandle) -> Result<Vec<SkillTarget>, String> {
    let directory = app.path().app_local_data_dir().map_err(|e| e.to_string())?;
    work(app, move |s| {
        let settings = collection::load_settings(&directory)?;
        s.targets(&id, &settings.skills.projects)
    })
    .await
}
#[tauri::command]
pub async fn preview_skill_sync(
    request: SyncRequest,
    app: AppHandle,
) -> Result<SyncPreview, String> {
    work(app, move |s| s.preview_sync(request)).await
}
#[tauri::command]
pub async fn apply_skill_sync(
    id: String,
    choice: String,
    app: AppHandle,
) -> Result<SkillTarget, String> {
    work(app, move |s| s.apply_sync(&id, &choice)).await
}
#[tauri::command]
pub async fn remove_skill_target(id: String, app: AppHandle) -> Result<OperationResult, String> {
    work(app, move |s| s.remove_target(&id)).await
}
#[tauri::command]
pub async fn scan_projects(project: String, app: AppHandle) -> Result<Vec<SkillTarget>, String> {
    work(app, move |s| s.projects(&project)).await
}
#[tauri::command]
pub async fn get_project_skills(
    project: String,
    app: AppHandle,
) -> Result<Vec<SkillTarget>, String> {
    work(app, move |s| s.projects(&project)).await
}
#[tauri::command]
pub async fn preview_migration(app: AppHandle) -> Result<MigrationPreview, String> {
    let directory = app.path().app_local_data_dir().map_err(|e| e.to_string())?;
    work(app, move |s| {
        s.preview_migration(&collection::load_settings(&directory)?.skills)
    })
    .await
}
#[tauri::command]
pub async fn apply_migration(
    id: String,
    resolutions: Vec<Resolution>,
    app: AppHandle,
) -> Result<OperationResult, String> {
    let directory = app.path().app_local_data_dir().map_err(|e| e.to_string())?;
    work(app, move |s| {
        let result = s.apply_migration(&id, &resolutions)?;
        let mut settings = collection::load_settings(&directory)?;
        settings.skills.migration.status = if resolutions.iter().any(|r| r.choice == "skip") {
            "pending"
        } else {
            "completed"
        }
        .into();
        settings.skills.migration.backup_path = result.backup_path.clone();
        collection::save_settings(&directory, &settings)?;
        Ok(result)
    })
    .await
}
#[tauri::command]
pub async fn import_github_repository(
    url: Option<String>,
    reference: Option<String>,
    repository_id: Option<String>,
    paths: Option<Vec<String>>,
    app: AppHandle,
) -> Result<serde_json::Value, String> {
    work(app,move |s| if let Some(id) = repository_id { Ok(serde_json::json!({"skills":s.import_repository(&id,&paths.ok_or("请选择 Skill")?)?})) } else { let (repository,candidates) = s.checkout(&url.ok_or("缺少仓库 URL")?,&reference.unwrap_or_else(|| "HEAD".into()))?; Ok(serde_json::json!({"repository":repository,"candidates":candidates})) }).await
}
#[tauri::command]
pub async fn check_skill_updates(check: bool, app: AppHandle) -> Result<Vec<Repository>, String> {
    work(app, move |s| {
        if check {
            s.check_updates()
        } else {
            s.repositories()
        }
    })
    .await
}
#[tauri::command]
pub async fn list_skill_operations(app: AppHandle) -> Result<Vec<serde_json::Value>, String> {
    work(app, |s| s.operations()).await
}
