// UI 测试桥接：调用真实改名服务，操作范围限定为新建的系统临时目录。
use serde_json::{json, Value};
use std::{
    io::{self, BufRead, Write},
    path::PathBuf,
};
use toolbox_core::{
    models::Settings,
    rename::{RenameService, Request},
};

fn text<'a>(args: &'a Value, key: &str) -> Result<&'a str, String> {
    args[key].as_str().ok_or_else(|| format!("缺少 {key}"))
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(std::env::args().nth(1).ok_or("需要新临时目录")?);
    if !root.is_absolute()
        || root.parent() != Some(std::env::temp_dir().as_path())
        || !root
            .file_name()
            .is_some_and(|n| n.to_string_lossy().starts_with("toolbox-rename-ui-"))
        || root.exists()
    {
        return Err("fixture 仅允许不存在的系统临时子目录".into());
    }
    std::fs::create_dir(&root)?;
    let root = root.canonicalize()?;
    let mut service = RenameService::default();
    let mut settings = Settings::default();
    settings.sources.clear();
    println!("{}", json!({"ready":true}));
    io::stdout().flush()?;
    for line in io::stdin().lock().lines() {
        let request: Value = serde_json::from_str(&line?)?;
        let args = &request["args"];
        let result = (|| -> Result<Value, String> {
            Ok(match text(&request, "name")? {
                "get_settings" => json!(settings),
                "update_settings" => {
                    settings = serde_json::from_value(args["settings"].clone())
                        .map_err(|e| e.to_string())?;
                    Value::Null
                }
                "get_snapshot" => Value::Null,
                "refresh_usage" => {
                    json!({"records":[],"sources":[],"collectedAt":"2026-10-23T00:00:00Z","dataDirectory":root})
                }
                "rename_preview" => {
                    let request: Request = serde_json::from_value(args["request"].clone())
                        .map_err(|e| e.to_string())?;
                    if PathBuf::from(&request.directory)
                        .canonicalize()
                        .map_err(|e| e.to_string())?
                        != root
                    {
                        return Err("fixture 禁止操作沙箱以外的目录".into());
                    }
                    json!(service.preview(request)?)
                }
                "rename_apply" => json!(service.apply(
                    text(args, "id")?,
                    &serde_json::from_value::<Vec<String>>(args["selected"].clone())
                        .map_err(|e| e.to_string())?
                )?),
                "rename_history" => json!(service.undo_info()),
                "rename_undo_preview" => json!(service.undo_preview(text(args, "id")?)?),
                "rename_undo" => json!(service.undo(text(args, "id")?)?),
                name => return Err(format!("未实现 fixture 命令：{name}")),
            })
        })();
        println!(
            "{}",
            match result {
                Ok(value) => json!({"value":value}),
                Err(error) => json!({"error":error}),
            }
        );
        io::stdout().flush()?;
    }
    Ok(())
}
