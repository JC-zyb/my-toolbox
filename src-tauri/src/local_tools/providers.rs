use super::{process::clean_text, types::*};
use serde_json::Value;
use std::path::{Path, PathBuf};

pub fn validate_token(value: &str) -> Result<(), String> {
    if value.is_empty()
        || value.len() > 160
        || !value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || ".-_+/@".contains(c))
        || value.starts_with('-')
    {
        return Err("名称或版本格式无效".into());
    }
    Ok(())
}

pub fn parse_fnm(text: &str) -> Result<Vec<ToolVersion>, String> {
    let text = clean_text(text);
    let mut versions = Vec::new();
    for line in text.lines().map(str::trim).filter(|x| !x.is_empty()) {
        let line = line.trim_start_matches('*').trim();
        let token = line.split_whitespace().next().ok_or("fnm 版本行为空")?;
        if token == "system" {
            continue;
        }
        let version = token.trim_start_matches('v');
        validate_token(version).map_err(|_| format!("无法解析 fnm 版本行：{line}"))?;
        if !version.as_bytes().first().is_some_and(u8::is_ascii_digit) {
            return Err(format!("无法解析 fnm 版本行：{line}"));
        }
        versions.push(ToolVersion {
            version: version.into(),
            identity: None,
            installed: true,
            default: line
                .split(|c: char| c.is_whitespace() || c == ',')
                .any(|x| x == "default"),
            path: None,
        });
    }
    Ok(versions)
}

pub fn parse_uv(text: &str) -> Result<Vec<ToolVersion>, String> {
    let rows: Vec<Value> =
        serde_json::from_str(text).map_err(|e| format!("uv Python JSON 无效：{e}"))?;
    rows.into_iter()
        .map(|row| {
            let version = row
                .get("version")
                .and_then(Value::as_str)
                .ok_or("uv Python 缺少 version")?;
            let path = row.get("path").and_then(Value::as_str).map(str::to_string);
            Ok(ToolVersion {
                version: version.into(),
                identity: row.get("key").and_then(Value::as_str).map(str::to_string),
                installed: path.is_some(),
                default: false,
                path,
            })
        })
        .collect()
}

pub fn parse_mise(text: &str, tool: &str) -> Result<Vec<ToolVersion>, String> {
    let value: Value = serde_json::from_str(text).map_err(|e| format!("mise JSON 无效：{e}"))?;
    let rows = if let Some(rows) = value.as_array() {
        rows.clone()
    } else if let Some(rows) = value.get(tool).and_then(Value::as_array) {
        rows.clone()
    } else if value.as_object().is_some_and(|v| v.is_empty()) {
        Vec::new()
    } else {
        return Err(format!("mise 输出缺少 {tool} 版本数组"));
    };
    rows.into_iter()
        .map(|row| {
            let version = row
                .get("version")
                .and_then(Value::as_str)
                .ok_or("mise 版本缺少 version")?;
            let path = row
                .get("install_path")
                .and_then(Value::as_str)
                .map(str::to_string);
            Ok(ToolVersion {
                version: version.into(),
                identity: None,
                installed: row
                    .get("installed")
                    .and_then(Value::as_bool)
                    .unwrap_or(path.is_some()),
                default: row
                    .get("source")
                    .and_then(|s| s.get("path"))
                    .and_then(Value::as_str)
                    .is_some_and(|s| {
                        s.contains("/mise/config.toml") || s.contains("\\mise\\config.toml")
                    }),
                path,
            })
        })
        .collect()
}

pub fn parse_rustup(text: &str) -> Result<Vec<ToolVersion>, String> {
    let mut versions = Vec::new();
    for line in text.lines().map(str::trim).filter(|x| !x.is_empty()) {
        if line == "no installed toolchains" {
            continue;
        }
        let name = line
            .split_whitespace()
            .next()
            .ok_or("rustup 工具链行为空")?;
        validate_token(name)?;
        let rest = line[name.len()..].trim();
        let path = if rest.starts_with('(') {
            rest.split_once(')')
                .map(|(_, p)| p.trim())
                .filter(|p| !p.is_empty())
        } else if rest.is_empty() {
            None
        } else {
            Some(rest)
        };
        versions.push(ToolVersion {
            version: name.into(),
            identity: None,
            installed: true,
            default: rest.contains("default"),
            path: path.map(str::to_string),
        });
    }
    Ok(versions)
}

#[derive(Debug, Clone)]
pub struct PackageReceipt {
    pub package: String,
    pub version: String,
    pub root: PathBuf,
}

pub fn parse_scoop(text: &str, root: &Path) -> Result<Vec<PackageReceipt>, String> {
    let value: Value =
        serde_json::from_str(text).map_err(|e| format!("Scoop export JSON 无效：{e}"))?;
    let apps = value
        .get("apps")
        .and_then(Value::as_array)
        .ok_or("Scoop export 缺少 apps")?;
    // 全局 Scoop 安装需要不同根目录及 --global 操作，不可误认成用户安装。
    apps.iter()
        .filter(|app| {
            !app.get("Info")
                .or_else(|| app.get("info"))
                .and_then(Value::as_str)
                .is_some_and(|s| s.to_ascii_lowercase().contains("global"))
        })
        .map(|app| {
            let get = |name: &str| {
                app.get(name)
                    .or_else(|| app.get(name.to_lowercase()))
                    .and_then(Value::as_str)
            };
            let name = get("Name").ok_or("Scoop app 缺少 Name")?;
            let version = get("Version").ok_or("Scoop app 缺少 Version")?;
            let source = get("Source").unwrap_or("");
            validate_token(name)?;
            if !source.is_empty() {
                validate_token(source)?;
            }
            Ok(PackageReceipt {
                package: if source.is_empty() {
                    name.into()
                } else {
                    format!("{source}/{name}")
                },
                version: version.into(),
                root: root.join("apps").join(name),
            })
        })
        .collect()
}

pub fn parse_brew(text: &str, cellar: &Path) -> Result<Vec<PackageReceipt>, String> {
    let value: Value =
        serde_json::from_str(text).map_err(|e| format!("Homebrew JSON 无效：{e}"))?;
    let rows = value
        .get("formulae")
        .and_then(Value::as_array)
        .ok_or("Homebrew JSON 缺少 formulae")?;
    let mut receipts = Vec::new();
    for row in rows {
        let name = row
            .get("name")
            .and_then(Value::as_str)
            .ok_or("Homebrew 缺少 name")?;
        let full_name = row.get("full_name").and_then(Value::as_str).unwrap_or(name);
        let installed = row
            .get("installed")
            .and_then(Value::as_array)
            .ok_or("Homebrew 缺少 installed")?;
        for version in installed {
            let version = version
                .get("version")
                .and_then(Value::as_str)
                .ok_or("Homebrew 安装记录缺少 version")?;
            receipts.push(PackageReceipt {
                package: full_name.into(),
                version: version.into(),
                root: cellar.join(name).join(version),
            });
        }
    }
    Ok(receipts)
}

pub fn cargo_has_fnm(text: &str) -> Option<String> {
    text.lines().find_map(|line| {
        line.strip_prefix("fnm v")
            .and_then(|v| v.strip_suffix(':'))
            .map(str::to_string)
    })
}

pub fn version_from_output(tool: &str, stdout: &str, stderr: &str) -> Result<String, String> {
    let text = clean_text(&format!("{stdout}\n{stderr}"));
    let line = if tool == "maven" {
        text.lines().find(|l| l.contains("Apache Maven"))
    } else if tool == "java" {
        text.lines()
            .find(|l| l.contains("version") || l.starts_with("openjdk "))
    } else {
        text.lines().find(|l| !l.trim().is_empty())
    }
    .ok_or("版本命令未返回版本")?;
    line.split(|c: char| c.is_whitespace() || c == '"')
        .find_map(|v| {
            let v = v.trim_start_matches('v');
            (v.as_bytes().first().is_some_and(u8::is_ascii_digit)
                && v.chars()
                    .all(|c| c.is_ascii_alphanumeric() || ".-_+".contains(c)))
            .then(|| v.to_string())
        })
        .ok_or_else(|| {
            format!(
                "无法识别版本输出：{}",
                line.chars().take(200).collect::<String>()
            )
        })
}

pub fn version_args(tool: &str) -> Vec<String> {
    vec![if tool == "java" {
        "-version"
    } else {
        "--version"
    }
    .into()]
}

pub fn command_args(
    provider: &str,
    action: &str,
    package: &str,
    version: Option<&str>,
) -> Result<Vec<String>, String> {
    validate_token(package)?;
    if let Some(v) = version {
        validate_token(v)?;
    }
    let v = || version.ok_or_else(|| "请选择具体版本".to_string());
    let strings = |xs: Vec<&str>| -> Vec<String> { xs.into_iter().map(str::to_string).collect() };
    Ok(match (provider, action) {
        ("winget", "install" | "upgrade" | "uninstall") => {
            let mut args = strings(vec![
                action,
                "--id",
                package,
                "--exact",
                "--disable-interactivity",
            ]);
            if action != "uninstall" {
                args.extend(strings(vec!["--source", "winget"]));
            }
            if action != "uninstall" {
                args.extend(strings(vec![
                    "--accept-source-agreements",
                    "--accept-package-agreements",
                ]));
            }
            if let Some(v) = version {
                args.extend(strings(vec!["--version", v]));
            }
            args
        }
        ("scoop", "install" | "upgrade" | "uninstall") => {
            if version.is_some() {
                return Err("Scoop 本体安装不支持此处指定历史版本，请使用运行时管理器".into());
            }
            strings(vec![
                if action == "upgrade" {
                    "update"
                } else {
                    action
                },
                package,
            ])
        }
        ("brew", "install" | "upgrade" | "uninstall") => {
            if version.is_some() {
                return Err("Homebrew 本体安装不支持此处指定历史版本，请使用运行时管理器".into());
            }
            strings(vec![action, "--formula", package])
        }
        ("fnm", "install" | "upgrade") => strings(vec!["install", v()?]),
        ("fnm", "uninstall") => strings(vec!["uninstall", v()?]),
        ("fnm", "set-default") => strings(vec!["default", v()?]),
        ("uv", "install" | "upgrade") => strings(vec!["python", "install", v()?]),
        ("uv", "uninstall") => strings(vec!["python", "uninstall", v()?]),
        ("uv", "set-default") => strings(vec!["python", "pin", "--global", v()?]),
        ("uv-self", "upgrade") => strings(vec!["self", "update"]),
        ("rustup", "install") => {
            strings(vec!["toolchain", "install", v()?, "--profile", "minimal"])
        }
        ("rustup", "upgrade") => strings(vec!["update", v()?]),
        ("rustup", "uninstall") => strings(vec!["toolchain", "uninstall", v()?]),
        ("rustup", "set-default") => strings(vec!["default", v()?]),
        ("mise", "install" | "upgrade" | "uninstall" | "set-default") => {
            let spec = format!("{package}@{}", v()?);
            match action {
                "set-default" => vec!["use".into(), "--global".into(), spec],
                "upgrade" => vec!["install".into(), spec],
                _ => vec![action.into(), spec],
            }
        }
        ("cargo", "upgrade") if package == "fnm" => {
            let mut args = strings(vec!["install", "fnm", "--locked"]);
            if let Some(v) = version {
                args.extend(strings(vec!["--version", v]));
            }
            args
        }
        ("cargo", "uninstall") if package == "fnm" => strings(vec!["uninstall", "fnm"]),
        _ => return Err(format!("{provider} 不支持 {action}")),
    })
}
