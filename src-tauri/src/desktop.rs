use crate::skill_commands::*;
use crate::local_tool_commands::*;
use crate::{
    collection,
    models::{Settings, Snapshot},
    skills::SkillsService,
    tools::{self, RenamePlan, RenameRow, ScriptResult},
};
#[cfg(windows)]
use std::os::windows::process::CommandExt;
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};
use tauri::{Manager, State};
use tauri_plugin_opener::OpenerExt;

pub(crate) struct AppState {
    directory: PathBuf,
    collection_lock: Arc<Mutex<()>>,
    rename_plans: Mutex<std::collections::HashMap<String, Vec<RenameRow>>>,
    pub(crate) skills: Mutex<SkillsService>,
    pub(crate) local_tools: Arc<crate::local_tools::LocalToolsService>,
}
#[tauri::command]
fn get_settings(state: State<AppState>) -> Result<Settings, String> {
    collection::load_settings(&state.directory)
}
#[tauri::command]
fn update_settings(settings: Settings, state: State<AppState>) -> Result<(), String> {
    collection::save_settings(&state.directory, &settings)
}
#[tauri::command]
fn get_snapshot(state: State<AppState>) -> Result<Option<Snapshot>, String> {
    let path = state.directory.join("snapshot.json");
    if !path.exists() {
        return Ok(None);
    }
    serde_json::from_reader(std::fs::File::open(path).map_err(|e| e.to_string())?)
        .map(Some)
        .map_err(|e| e.to_string())
}
#[tauri::command]
async fn refresh_usage(state: State<'_, AppState>) -> Result<Snapshot, String> {
    let directory = state.directory.clone();
    let lock = state.collection_lock.clone();
    // 耗时扫描在阻塞线程执行，保持窗口响应；数据库写入串行化。
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = lock.lock().map_err(|e| e.to_string())?;
        let settings = collection::load_settings(&directory)?;
        let snapshot = collection::collect(&settings, &directory)?;
        let file =
            std::fs::File::create(directory.join("snapshot.tmp")).map_err(|e| e.to_string())?;
        serde_json::to_writer(&file, &snapshot).map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
        std::fs::rename(
            directory.join("snapshot.tmp"),
            directory.join("snapshot.json"),
        )
        .map_err(|e| e.to_string())?;
        Ok(snapshot)
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
fn open_target(target: String, app: tauri::AppHandle) -> Result<(), String> {
    if target.starts_with("https://") || target.starts_with("http://") {
        app.opener()
            .open_url(target, None::<&str>)
            .map_err(|e| e.to_string())
    } else {
        if !std::path::Path::new(&target).exists() {
            return Err("目标不存在".into());
        }
        app.opener()
            .open_path(target, None::<&str>)
            .map_err(|e| e.to_string())
    }
}
fn launch(target: &str, args: &[String], cwd: &str) -> Result<(), String> {
    let mut command = if target.ends_with(".ps1") {
        let mut c = std::process::Command::new("powershell.exe");
        c.args(["-NoProfile", "-File", target]);
        c
    } else {
        std::process::Command::new(target)
    };
    command.args(args);
    #[cfg(windows)]
    command.creation_flags(0x00000010);
    if !cwd.is_empty() {
        command.current_dir(cwd);
    }
    command.spawn().map_err(|e| format!("启动失败：{e}"))?;
    Ok(())
}
#[tauri::command]
fn launch_agent(id: String, state: State<AppState>) -> Result<(), String> {
    let settings = collection::load_settings(&state.directory)?;
    let source = settings
        .sources
        .iter()
        .find(|s| s.id == id)
        .ok_or("未知 Agent")?;
    if source.executable.is_empty() {
        return Err("请先登记客户端启动路径".into());
    }
    launch(&source.executable, &[], "")
}
#[tauri::command]
fn launch_entry(id: String, state: State<AppState>, app: tauri::AppHandle) -> Result<(), String> {
    let settings = collection::load_settings(&state.directory)?;
    let entry = settings
        .entries
        .iter()
        .find(|s| s.id == id)
        .ok_or("入口不存在")?;
    if entry.kind == "application" {
        launch(&entry.target, &entry.args, &entry.cwd)
    } else {
        open_target(entry.target.clone(), app)
    }
}
#[tauri::command]
fn export_csv(path: String, content: String) -> Result<(), String> {
    if std::path::Path::new(&path)
        .extension()
        .is_none_or(|s| !s.eq_ignore_ascii_case("csv"))
    {
        return Err("请选择 CSV 文件".into());
    }
    std::fs::write(path, content).map_err(|e| e.to_string())
}
#[tauri::command]
fn rename_preview(
    directory: String,
    find: String,
    replace: String,
    prefix: String,
    suffix: String,
    state: State<AppState>,
) -> Result<RenamePlan, String> {
    let plan = tools::preview(
        std::path::Path::new(&directory),
        &find,
        &replace,
        &prefix,
        &suffix,
    )?;
    let mut plans = state.rename_plans.lock().map_err(|e| e.to_string())?;
    plans.clear();
    plans.insert(plan.id.clone(), plan.rows.clone());
    Ok(plan)
}
#[tauri::command]
fn rename_apply(id: String, state: State<AppState>) -> Result<usize, String> {
    let rows = state
        .rename_plans
        .lock()
        .map_err(|e| e.to_string())?
        .remove(&id)
        .ok_or("预览已失效，请重新预览")?;
    tools::apply(&rows)
}
#[tauri::command]
async fn run_script(id: String, state: State<'_, AppState>) -> Result<ScriptResult, String> {
    let settings = collection::load_settings(&state.directory)?;
    let script = settings
        .scripts
        .iter()
        .find(|s| s.id == id)
        .ok_or("脚本未登记")?;
    tools::run(script).await
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.show();
                let _ = w.set_focus();
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let directory = app.path().app_local_data_dir()?;
            std::fs::create_dir_all(&directory)?;
            app.manage(AppState {
                local_tools: Arc::new(crate::local_tools::LocalToolsService::new(
                    directory.join("local-tools"),
                    PathBuf::from(std::env::var("USERPROFILE").or_else(|_|std::env::var("HOME")).map_err(std::io::Error::other)?),
                ).map_err(std::io::Error::other)?),
                directory,
                collection_lock: Arc::new(Mutex::new(())),
                rename_plans: Mutex::new(std::collections::HashMap::new()),
                skills: Mutex::new(
                    SkillsService::new(
                        app.path().app_local_data_dir()?.join("skills"),
                        std::path::PathBuf::from(
                            std::env::var("USERPROFILE")
                                .or_else(|_| std::env::var("HOME"))
                                .unwrap_or_default(),
                        ),
                    )
                    .map_err(|e| std::io::Error::other(e))?,
                ),
            });
            let show =
                tauri::menu::MenuItem::with_id(app, "show", "打开工作台", true, None::<&str>)?;
            let quit = tauri::menu::MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
            let menu = tauri::menu::Menu::with_items(app, &[&show, &quit])?;
            let mut tray = tauri::tray::TrayIconBuilder::new()
                .tooltip("个人工具箱 · 本地 Agents 工作台")
                .menu(&menu)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "quit" => app.exit(0),
                    "show" => {
                        if let Some(w) = app.get_webview_window("main") {
                            let _ = w.show();
                            let _ = w.set_focus();
                        }
                    }
                    _ => {}
                });
            if let Some(icon) = app.default_window_icon() {
                tray = tray.icon(icon.clone());
            }
            tray.build(app)?;
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if let Some(state) = window.try_state::<AppState>() {
                    if collection::load_settings(&state.directory).is_ok_and(|s| s.close_to_tray) {
                        api.prevent_close();
                        if let Err(e) = window.hide() {
                            eprintln!("隐藏窗口失败：{e}");
                        }
                    }
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            get_settings,
            update_settings,
            get_snapshot,
            refresh_usage,
            open_target,
            launch_agent,
            launch_entry,
            export_csv,
            rename_preview,
            rename_apply,
            run_script,
            list_local_tools,
            scan_local_tools,
            search_local_tools,
            add_local_tool,
            refresh_local_tool_metadata,
            local_tool_versions,
            local_tool_project,
            preview_local_tool_operation,
            execute_local_tool_operation,
            list_local_tool_operations,
            set_local_tool_directories,
            check_local_tool_updates,
            list_skills,
            get_skill,
            get_discovered_skill,
            scan_skill_sources,
            skill_matrix,
            preview_batch_skill_operation,
            apply_batch_skill_operation,
            preview_skill_activation,
            apply_skill_activation,
            quick_skill_activation,
            remove_agent_skill_entries,
            create_skill,
            update_skill,
            delete_skill,
            import_local_skill,
            import_github_repository,
            list_skill_targets,
            preview_skill_sync,
            apply_skill_sync,
            remove_skill_target,
            scan_projects,
            get_project_skills,
            preview_migration,
            apply_migration,
            check_skill_updates,
            export_skill,
            list_skill_operations
        ])
        .run(tauri::generate_context!())
        .expect("无法启动个人工具箱");
}
