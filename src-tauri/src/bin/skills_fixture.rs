// 隔离 UI 测试的真实核心桥；只能运行在新建的临时沙箱，永不使用用户主目录。
use serde_json::{json, Value};
use std::{
    io::{self, BufRead},
    path::PathBuf,
};
use toolbox_core::{
    collection,
    models::Settings,
    skills::{BatchResolution, BatchSkillRequest, QuickSkillActivationRequest, Resolution, SkillActivationRequest, SkillsService, SyncRequest},
};
fn text<'a>(v: &'a Value, key: &str) -> Result<&'a str, String> {
    v[key].as_str().ok_or_else(|| format!("缺少 {key}"))
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(std::env::args().nth(1).ok_or("需要隔离根目录")?);
    if !root.is_absolute() || !root.starts_with(std::env::temp_dir()) || root.exists() {
        return Err("只能使用不存在的系统临时子目录".into());
    }
    std::fs::create_dir_all(root.join("home"))?;
    let mut service = SkillsService::new(root.join("data/skills"), root.join("home"))?;
    let mut settings = Settings::default();
    settings.sources.clear();
    settings.skills.central_path = service.central_path();
    settings.skills.migration.source_path = root
        .join("home/.skills-manager/skills")
        .to_string_lossy()
        .into();
    for (name, body) in [
        ("code-review", "检查代码变化与边界"),
        ("ui-kit", "界面组件约定"),
    ] {
        service.create(
            name,
            &format!("---\nname: {name}\ndescription: {body}\n---\n\n# {body}\n"),
        )?;
    }
    let legacy = root.join("home/.skills-manager/skills/code-review");
    std::fs::create_dir_all(&legacy)?;
    std::fs::write(
        legacy.join("SKILL.md"),
        "---\nname: code-review\ndescription: 旧版本审查\n---\n\n旧库内容\n",
    )?;
    collection::save_settings(&root.join("data"), &settings)?;
    println!("{}", json!({"ready":true,"root":root}));
    for line in io::stdin().lock().lines() {
        let req: Value = serde_json::from_str(&line?)?;
        let result = (|| -> Result<Value, String> {
            let args = &req["args"];
            let name = text(&req, "name")?;
            Ok(match name {
                "get_settings" => {
                    serde_json::to_value(collection::load_settings(&root.join("data"))?)
                        .map_err(|e| e.to_string())?
                }
                "update_settings" => {
                    let settings = serde_json::from_value(args["settings"].clone())
                        .map_err(|e| e.to_string())?;
                    collection::save_settings(&root.join("data"), &settings)?;
                    Value::Null
                }
                "get_snapshot" => Value::Null,
                "refresh_usage" => {
                    let settings = collection::load_settings(&root.join("data"))?;
                    serde_json::to_value(collection::collect(&settings, &root.join("data"))?)
                        .map_err(|e| e.to_string())?
                }
                "list_skills" | "scan_skill_sources" => json!(service.scan()?),
                "skill_matrix" => json!(service.skill_matrix(args["projectPath"].as_str())?),
                "preview_batch_skill_operation" | "remove_agent_skill_entries" => {
                    let r: BatchSkillRequest = serde_json::from_value(args["request"].clone())
                        .map_err(|e| e.to_string())?;
                    json!(if name == "remove_agent_skill_entries" {
                        service.remove_agent_skill_entries(r)?
                    } else {
                        service.preview_batch_skill_operation(r)?
                    })
                }
                "apply_batch_skill_operation" => {
                    let r: Vec<BatchResolution> =
                        serde_json::from_value(args["resolutions"].clone())
                            .map_err(|e| e.to_string())?;
                    json!(service.apply_batch_skill_operation(text(args, "id")?, &r)?)
                }
                "preview_skill_activation" => {
                    let r: SkillActivationRequest = serde_json::from_value(args["request"].clone()).map_err(|e| e.to_string())?;
                    json!(service.preview_skill_activation(r)?)
                }
                "quick_skill_activation" => {
                    let r: QuickSkillActivationRequest = serde_json::from_value(args["request"].clone()).map_err(|e| e.to_string())?;
                    json!(service.quick_skill_activation(r)?)
                }
                "apply_skill_activation" => {
                    let r: Vec<BatchResolution> = serde_json::from_value(args["resolutions"].clone()).map_err(|e| e.to_string())?;
                    json!(service.apply_skill_activation(text(args, "id")?, &r)?)
                }
                "get_skill" => json!(service.get(text(args, "id")?)?),
                "get_discovered_skill" => json!(service.get_discovered_skill(text(args, "id")?, args["projectPath"].as_str())?),
                "create_skill" => {
                    json!(service.create(text(args, "name")?, text(args, "document")?)?)
                }
                "update_skill" => json!(service.update(
                    text(args, "id")?,
                    text(args, "document")?,
                    text(args, "expectedHash")?
                )?),
                "delete_skill" => json!(service.delete(
                    text(args, "id")?,
                    args["permanent"].as_bool().unwrap_or(false),
                    text(args, "expectedHash")?
                )?),
                "list_skill_targets" => {
                    let settings = collection::load_settings(&root.join("data"))?;
                    json!(service.targets(text(args, "id")?, &settings.skills.projects)?)
                }
                "preview_skill_sync" => {
                    let r: SyncRequest = serde_json::from_value(args["request"].clone())
                        .map_err(|e| e.to_string())?;
                    json!(service.preview_sync(r)?)
                }
                "apply_skill_sync" => {
                    json!(service.apply_sync(text(args, "id")?, text(args, "choice")?)?)
                }
                "remove_skill_target" => json!(service.remove_target(text(args, "id")?)?),
                "preview_migration" => {
                    let settings = collection::load_settings(&root.join("data"))?;
                    json!(service.preview_migration(&settings.skills)?)
                }
                "apply_migration" => {
                    let r: Vec<Resolution> = serde_json::from_value(args["resolutions"].clone())
                        .map_err(|e| e.to_string())?;
                    let result = service.apply_migration(text(args, "id")?, &r)?;
                    let mut settings = collection::load_settings(&root.join("data"))?;
                    settings.skills.migration.status = "completed".into();
                    settings.skills.migration.backup_path = result.backup_path.clone();
                    collection::save_settings(&root.join("data"), &settings)?;
                    json!(result)
                }
                "get_project_skills" | "scan_projects" => {
                    json!(service.projects(text(args, "project")?)?)
                }
                "check_skill_updates" => json!(if args["check"].as_bool() == Some(true) {
                    service.check_updates()?
                } else {
                    service.repositories()?
                }),
                "list_skill_operations" => json!(service.operations()?),
                "import_github_repository" => {
                    if let Some(id) = args["repositoryId"].as_str() {
                        let paths: Vec<String> = serde_json::from_value(args["paths"].clone())
                            .map_err(|e| e.to_string())?;
                        json!({"skills":service.import_repository(id,&paths)?})
                    } else {
                        let (r, c) =
                            service.checkout(text(args, "url")?, text(args, "reference")?)?;
                        json!({"repository":r,"candidates":c})
                    }
                }
                "import_local_skill" => {
                    json!(service.import_local(&PathBuf::from(text(args, "path")?))?)
                }
                _ => return Err(format!("隔离桥未开放命令：{name}")),
            })
        })();
        println!(
            "{}",
            match result {
                Ok(value) => json!({"value":value}),
                Err(error) => json!({"error":error}),
            }
        );
    }
    Ok(())
}
