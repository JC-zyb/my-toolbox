use super::*;

fn numeric(version: &str) -> Option<(&str, Vec<u64>)> {
    let start = version.find(|c: char| c.is_ascii_digit())?;
    let prefix = &version[..start];
    let numeric = version[start..]
        .split(|c: char| !c.is_ascii_digit() && c != '.')
        .next()?;
    let parts = numeric
        .split('.')
        .map(str::parse)
        .collect::<Result<Vec<u64>, _>>()
        .ok()?;
    Some((prefix, parts))
}
pub(super) fn newer_same_series(installed: &str, available: &str) -> bool {
    let (Some((a, x)), Some((b, y))) = (numeric(installed), numeric(available)) else {
        return false;
    };
    a.trim_start_matches('v') == b.trim_start_matches('v') && x.first() == y.first() && y > x
}

impl LocalToolsService {
    pub async fn check_updates(&self) -> Result<ToolUpdateCheck, String> {
        let snapshot = self.snapshot()?;
        let mut result = ToolUpdateCheck {
            updates: vec![],
            checked_at: now(),
            diagnostics: vec![],
        };
        let mut remote: HashMap<String, Vec<String>> = HashMap::new();
        for item in snapshot
            .installations
            .iter()
            .filter(|i| i.provider.is_some() && i.version.is_some() && i.status == "ready")
        {
            let provider = item.provider.as_deref().ok_or("缺少来源")?;
            let installed = item.version.as_deref().ok_or("缺少版本")?;
            let key = format!("{provider}/{}", item.tool_id);
            if !remote.contains_key(&key) {
                let fetched: Result<Vec<String>, String> = async {
                    if matches!(provider, "fnm" | "uv" | "mise") {
                        return Ok(self
                            .versions(item.tool_id.clone(), true)
                            .await?
                            .versions
                            .into_iter()
                            .map(|v| v.version)
                            .collect::<Vec<_>>());
                    }
                    let package = item.package.as_deref().ok_or("缺少包映射")?;
                    match provider {
                        "brew" => {
                            let out = self
                                .provider_command(
                                    &snapshot,
                                    "brew",
                                    vec![
                                        "info".into(),
                                        "--json=v2".into(),
                                        "--formula".into(),
                                        package.into(),
                                    ],
                                )
                                .await?;
                            let value: serde_json::Value =
                                serde_json::from_str(out.checked()?).map_err(|e| e.to_string())?;
                            let version = value
                                .get("formulae")
                                .and_then(|v| v.get(0))
                                .and_then(|v| v.get("versions"))
                                .and_then(|v| v.get("stable"))
                                .and_then(|v| v.as_str())
                                .ok_or("Homebrew 响应没有 stable 版本")?;
                            Ok(vec![version.into()])
                        }
                        "scoop" => {
                            let out = self
                                .provider_command(
                                    &snapshot,
                                    "scoop",
                                    vec!["cat".into(), package.into()],
                                )
                                .await?;
                            let value: serde_json::Value =
                                serde_json::from_str(out.checked()?).map_err(|e| e.to_string())?;
                            Ok(vec![value
                                .get("version")
                                .and_then(|v| v.as_str())
                                .ok_or("Scoop manifest 缺少 version")?
                                .into()])
                        }
                        "cargo" if item.tool_id == "fnm" => {
                            let out = self
                                .provider_command(
                                    &snapshot,
                                    "cargo",
                                    vec![
                                        "search".into(),
                                        "fnm".into(),
                                        "--limit".into(),
                                        "1".into(),
                                    ],
                                )
                                .await?;
                            let line = out
                                .checked()?
                                .lines()
                                .find(|l| l.starts_with("fnm = "))
                                .ok_or("Cargo 搜索未返回 fnm")?;
                            Ok(vec![line
                                .split('"')
                                .nth(1)
                                .ok_or("Cargo 搜索缺少版本")?
                                .into()])
                        }
                        "winget" => {
                            let out = self
                                .provider_command(
                                    &snapshot,
                                    "winget",
                                    vec![
                                        "show".into(),
                                        "--id".into(),
                                        package.into(),
                                        "--exact".into(),
                                        "--source".into(),
                                        "winget".into(),
                                        "--versions".into(),
                                        "--disable-interactivity".into(),
                                    ],
                                )
                                .await?;
                            Ok(out
                                .checked()?
                                .lines()
                                .map(str::trim)
                                .filter(|s| {
                                    s.starts_with(|c: char| c.is_ascii_digit())
                                        && providers::validate_token(s).is_ok()
                                })
                                .map(str::to_string)
                                .collect())
                        }
                        "rustup" => {
                            let out = self
                                .provider_command(&snapshot, "rustup", vec!["check".into()])
                                .await?;
                            Ok(out
                                .checked()?
                                .lines()
                                .filter(|l| l.contains("->"))
                                .filter_map(|l| l.split_whitespace().next())
                                .map(str::to_string)
                                .collect())
                        }
                        _ => Err(
                            "此来源未提供可靠的只读更新查询；可查看官方资料或执行原来源升级预览"
                                .into(),
                        ),
                    }
                }
                .await;
                match fetched {
                    Ok(versions) => {
                        remote.insert(key.clone(), versions);
                    }
                    Err(error) => {
                        result.diagnostics.push(format!("{key}：{error}"));
                        remote.insert(key.clone(), vec![]);
                    }
                }
            }
            let available = &remote[&key];
            let target = if provider == "rustup" {
                available.iter().find(|v| *v == installed)
            } else {
                available
                    .iter()
                    .filter(|v| newer_same_series(installed, v))
                    .max_by_key(|v| numeric(v).map(|(_, n)| n))
            };
            if let Some(target) = target {
                result.updates.push(ToolUpdate {
                    tool_id: item.tool_id.clone(),
                    installation_id: item.id.clone(),
                    provider: provider.into(),
                    installed_version: installed.into(),
                    target_version: target.clone(),
                });
            }
        }
        self.save("updates", &result)?;
        Ok(result)
    }
}
