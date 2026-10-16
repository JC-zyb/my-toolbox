use super::{providers::validate_token, types::FileChange};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};
use toml_edit::{value, Document};

pub fn project_root(path: &str) -> Result<PathBuf, String> {
    let root = Path::new(path)
        .canonicalize()
        .map_err(|e| format!("项目目录不可访问：{e}"))?;
    if !root.is_dir() {
        return Err("请选择项目目录".into());
    }
    Ok(root)
}

fn read(root: &Path, relative: &str) -> Result<Option<String>, String> {
    let path = root.join(relative);
    match fs::symlink_metadata(&path) {
        Ok(meta) => {
            if meta.file_type().is_symlink() {
                return Err(format!("配置是链接，不能覆盖：{}", path.display()));
            }
            let canonical = path.canonicalize().map_err(|e| e.to_string())?;
            if !canonical.starts_with(root) {
                return Err("配置路径超出项目目录".into());
            }
            if meta.len() > 1024 * 1024 {
                return Err("配置文件超过 1 MiB，不能自动修改".into());
            }
            fs::read_to_string(path)
                .map(Some)
                .map_err(|e| format!("配置读取失败：{e}"))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(format!("无法读取配置元信息：{error}")),
    }
}

fn change(root: &Path, relative: &str, before: Option<String>, after: String) -> FileChange {
    FileChange {
        path: root.join(relative).to_string_lossy().into_owned(),
        before,
        after,
    }
}

pub fn preview(root: &Path, tool: &str, version: &str) -> Result<Vec<FileChange>, String> {
    validate_token(version)?;
    match tool {
        "node" => {
            let node = read(root, ".node-version")?;
            let nvm = read(root, ".nvmrc")?;
            if let (Some(a), Some(b)) = (&node, &nvm) {
                if a.trim().trim_start_matches('v') != b.trim().trim_start_matches('v') {
                    return Err(".node-version 与 .nvmrc 冲突，请先统一现有配置".into());
                }
            }
            let mut changes = vec![];
            if node.is_some() || nvm.is_none() {
                changes.push(change(root, ".node-version", node, format!("{version}\n")));
            }
            if nvm.is_some() {
                changes.push(change(root, ".nvmrc", nvm, format!("{version}\n")));
            }
            Ok(changes)
        }
        "python" => Ok(vec![change(
            root,
            ".python-version",
            read(root, ".python-version")?,
            format!("{version}\n"),
        )]),
        "rust" | "cargo" => {
            let legacy = read(root, "rust-toolchain")?;
            let modern = read(root, "rust-toolchain.toml")?;
            if legacy.is_some() && modern.is_some() {
                return Err(
                    "同时存在 rust-toolchain 与 rust-toolchain.toml，请先解决优先级冲突".into(),
                );
            }
            let (relative, before) = if legacy.is_some() {
                ("rust-toolchain", legacy)
            } else {
                ("rust-toolchain.toml", modern)
            };
            let after = if relative == "rust-toolchain"
                && before
                    .as_ref()
                    .is_some_and(|s| !s.trim_start().starts_with('['))
            {
                format!("{version}\n")
            } else {
                let mut doc = before
                    .as_deref()
                    .unwrap_or("")
                    .parse::<Document>()
                    .map_err(|e| format!("工具链 TOML 无效：{e}"))?;
                if doc.get("toolchain").is_some_and(|t| !t.is_table()) {
                    return Err("toolchain 不是 TOML 表，不能自动修改".into());
                }
                if doc.get("toolchain").and_then(|t| t.get("path")).is_some() {
                    return Err("工具链指定了本地 path，请先人工选择 channel 或 path".into());
                }
                doc["toolchain"]["channel"] = value(version);
                doc.to_string()
            };
            Ok(vec![change(root, relative, before, after)])
        }
        "java" | "bun" | "deno" | "pnpm" | "maven" => {
            let mut existing = Vec::new();
            for relative in [
                "mise.toml",
                ".mise.toml",
                ".config/mise.toml",
                ".config/mise/config.toml",
                ".tool-versions",
            ] {
                if let Some(text) = read(root, relative)? {
                    existing.push((relative, text));
                }
            }
            if existing.len() > 1 {
                return Err("项目存在多份 mise 版本配置，请先明确唯一配置文件".into());
            }
            let (relative, before) = existing
                .into_iter()
                .next()
                .map(|(p, t)| (p, Some(t)))
                .unwrap_or(("mise.toml", None));
            let after = if relative == ".tool-versions" {
                let mut found = false;
                let mut lines = Vec::new();
                for line in before.as_deref().unwrap_or("").lines() {
                    if line.split_whitespace().next() == Some(tool) {
                        if found {
                            return Err(format!(".tool-versions 中 {tool} 重复"));
                        }
                        found = true;
                        let comment = line
                            .find('#')
                            .map(|i| format!(" {}", &line[i..]))
                            .unwrap_or_default();
                        lines.push(format!("{tool} {version}{comment}"));
                    } else {
                        lines.push(line.to_string());
                    }
                }
                if !found {
                    lines.push(format!("{tool} {version}"));
                }
                format!("{}\n", lines.join("\n"))
            } else {
                let mut doc = before
                    .as_deref()
                    .unwrap_or("")
                    .parse::<Document>()
                    .map_err(|e| format!("mise TOML 无效：{e}"))?;
                if doc.get("tools").is_some_and(|t| !t.is_table()) {
                    return Err("tools 不是 TOML 表，不能自动修改".into());
                }
                if let Some(item) = doc.get("tools").and_then(|t| t.get(tool)) {
                    if item.as_value().is_some_and(|v| v.is_inline_table()) {
                        doc["tools"][tool]["version"] = value(version);
                    } else if item.is_str() {
                        doc["tools"][tool] = value(version);
                    } else {
                        return Err("该工具使用复杂版本配置，请人工调整以保留现有含义".into());
                    }
                } else {
                    doc["tools"][tool] = value(version);
                }
                doc.to_string()
            };
            Ok(vec![change(root, relative, before, after)])
        }
        _ => Err("此工具不支持项目版本固定".into()),
    }
}

pub fn inspect(root: &Path, tool: &str) -> Result<(Vec<FileChange>, Option<String>), String> {
    let names: &[&str] = match tool {
        "node" => &[".node-version", ".nvmrc"],
        "python" => &[".python-version"],
        "rust" | "cargo" => &["rust-toolchain", "rust-toolchain.toml"],
        "java" | "bun" | "deno" | "pnpm" | "maven" => &[
            "mise.toml",
            ".mise.toml",
            ".config/mise.toml",
            ".config/mise/config.toml",
            ".tool-versions",
        ],
        _ => return Err("此工具不支持项目版本固定".into()),
    };
    let mut files = Vec::new();
    let mut selected = None;
    for name in names {
        if let Some(text) = read(root, name)? {
            let version = if *name == ".tool-versions" {
                text.lines().find_map(|l| {
                    let mut fields = l.split_whitespace();
                    (fields.next() == Some(tool))
                        .then(|| fields.next().map(str::to_string))
                        .flatten()
                })
            } else if name.ends_with("toml")
                || *name == ".mise.toml"
                || (*name == "rust-toolchain" && text.trim_start().starts_with('['))
            {
                let doc = text.parse::<Document>().map_err(|e| e.to_string())?;
                if matches!(tool, "rust" | "cargo") {
                    doc.get("toolchain")
                        .and_then(|t| t.get("channel"))
                        .and_then(|v| v.as_str())
                        .map(str::to_string)
                } else {
                    doc.get("tools")
                        .and_then(|t| t.get(tool))
                        .and_then(|v| {
                            v.as_str()
                                .or_else(|| v.get("version").and_then(|v| v.as_str()))
                        })
                        .map(str::to_string)
                }
            } else {
                Some(text.trim().into())
            };
            if let (Some(a), Some(b)) = (&selected, &version) {
                if a != b {
                    return Err("项目版本文件存在冲突，请先统一".into());
                }
            }
            if version.is_some() {
                selected = version;
            }
            files.push(change(root, name, Some(text.clone()), text));
        }
    }
    Ok((files, selected))
}

pub fn verify(changes: &[FileChange]) -> Result<(), String> {
    for file in changes {
        let path = Path::new(&file.path);
        let parent = path.parent().ok_or("配置路径无父目录")?;
        if parent.canonicalize().map_err(|e| e.to_string())? != parent {
            return Err("配置父目录是链接或已经改变，请重新预览".into());
        }
        if fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink()) {
            return Err("配置已经变成链接，请重新预览".into());
        }
        let current = match fs::read_to_string(path) {
            Ok(s) => Some(s),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => return Err(e.to_string()),
        };
        if current != file.before {
            return Err("预览已过期：配置文件已被外部修改".into());
        }
    }
    Ok(())
}

fn publish(path: &Path, text: &str) -> Result<(), String> {
    let temporary = path.with_file_name(format!(".toolbox-write-{}", uuid::Uuid::new_v4()));
    let result = (|| {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|e| e.to_string())?;
        file.write_all(text.as_bytes()).map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
        fs::rename(&temporary, path).map_err(|e| e.to_string())
    })();
    if result.is_err() && temporary.exists() {
        fs::remove_file(&temporary).map_err(|e| format!("写入失败且临时文件清理失败：{e}"))?;
    }
    result
}

pub fn apply(changes: &[FileChange]) -> Result<Vec<String>, String> {
    verify(changes)?;
    let mut backups = Vec::new();
    for file in changes {
        if let Some(before) = &file.before {
            let path = Path::new(&file.path).with_file_name(format!(
                ".toolbox-backup-{}-{}",
                uuid::Uuid::new_v4(),
                Path::new(&file.path)
                    .file_name()
                    .ok_or("配置名称无效")?
                    .to_string_lossy()
            ));
            let mut output = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
                .map_err(|e| e.to_string())?;
            output
                .write_all(before.as_bytes())
                .map_err(|e| e.to_string())?;
            output.sync_all().map_err(|e| e.to_string())?;
            backups.push(path.to_string_lossy().into_owned());
        }
    }
    for (index, file) in changes.iter().enumerate() {
        if let Err(error) = publish(Path::new(&file.path), &file.after) {
            let mut rollback_errors = Vec::new();
            for old in changes[..index].iter().rev() {
                let result = if let Some(text) = &old.before {
                    publish(Path::new(&old.path), text)
                } else {
                    fs::remove_file(&old.path).map_err(|e| e.to_string())
                };
                if let Err(e) = result {
                    rollback_errors.push(e);
                }
            }
            return Err(format!(
                "配置写入失败：{error}；备份：{}；回滚错误：{}",
                backups.join("，"),
                rollback_errors.join("；")
            ));
        }
    }
    Ok(backups)
}
