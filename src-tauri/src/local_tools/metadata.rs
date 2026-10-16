use super::*;
use serde_json::Value;

pub fn github_repository(input: &str) -> Result<String, String> {
    let value = input.trim().trim_end_matches('/').trim_end_matches(".git");
    let value = value
        .strip_prefix("https://github.com/")
        .or_else(|| value.strip_prefix("http://github.com/"))
        .unwrap_or(value);
    let parts = value.split('/').collect::<Vec<_>>();
    if parts.len() != 2
        || parts.iter().any(|s| {
            s.is_empty()
                || *s == "."
                || *s == ".."
                || !s
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c))
        })
    {
        return Err("请输入 GitHub 仓库地址或 owner/repo".into());
    }
    Ok(format!("{}/{}", parts[0], parts[1]))
}

impl LocalToolsService {
    async fn github_json(&self, url: &str) -> Result<Option<Value>, String> {
        let mut response = self
            .client
            .get(url)
            .header("Accept", "application/vnd.github+json")
            .send()
            .await
            .map_err(|e| format!("GitHub 请求失败：{e}"))?;
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        if !response.status().is_success() {
            let remaining = response
                .headers()
                .get("x-ratelimit-remaining")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("");
            return Err(if remaining == "0" {
                "GitHub API 已限流，请稍后重试；本机数据仍可使用".into()
            } else {
                format!("GitHub 返回 HTTP {}", response.status())
            });
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|e| e.to_string())? {
            if bytes.len() + chunk.len() > 2 * 1024 * 1024 {
                return Err("GitHub 响应超过 2 MiB".into());
            }
            bytes.extend_from_slice(&chunk);
        }
        serde_json::from_slice(&bytes)
            .map(Some)
            .map_err(|e| format!("GitHub JSON 无效：{e}"))
    }
    pub async fn refresh_metadata(&self, tool_id: String) -> Result<ToolMetadata, String> {
        let tool = self
            .definitions()?
            .into_iter()
            .find(|t| t.id == tool_id)
            .ok_or("未知工具")?;
        let mut all: Vec<ToolMetadata> = self.load("metadata")?.unwrap_or_default();
        let mut metadata = all
            .iter()
            .find(|m| m.tool_id == tool_id)
            .cloned()
            .unwrap_or(ToolMetadata {
                tool_id: tool_id.clone(),
                description: tool.description.clone(),
                homepage: tool.homepage.clone(),
                repository: tool.repository.clone(),
                ..Default::default()
            });
        let result = async {
            let repo = github_repository(&tool.repository)?;
            let data = self
                .github_json(&format!("{}/repos/{repo}", self.github_api))
                .await?
                .ok_or("GitHub 仓库不存在")?;
            let release = self
                .github_json(&format!("{}/repos/{repo}/releases/latest", self.github_api))
                .await?;
            Ok::<_, String>(ToolMetadata {
                tool_id: tool_id.clone(),
                description: data
                    .get("description")
                    .and_then(Value::as_str)
                    .unwrap_or(&tool.description)
                    .into(),
                homepage: data
                    .get("homepage")
                    .and_then(Value::as_str)
                    .filter(|s| s.starts_with("https://"))
                    .unwrap_or(&tool.homepage)
                    .into(),
                repository: tool.repository.clone(),
                license: data
                    .get("license")
                    .and_then(|v| v.get("spdx_id"))
                    .and_then(Value::as_str)
                    .map(str::to_string),
                latest_release: release
                    .as_ref()
                    .and_then(|r| r.get("tag_name"))
                    .and_then(Value::as_str)
                    .map(str::to_string),
                release_url: release
                    .as_ref()
                    .and_then(|r| r.get("html_url"))
                    .and_then(Value::as_str)
                    .map(str::to_string),
                fetched_at: now(),
                error: None,
            })
        }
        .await;
        match result {
            Ok(value) => metadata = value,
            Err(error) => metadata.error = Some(error),
        }
        // 网络等待期间其他工具可能完成补全，重新读取以免覆盖其缓存。
        all = self.load("metadata")?.unwrap_or_default();
        all.retain(|m| m.tool_id != tool_id);
        all.push(metadata.clone());
        self.save("metadata", &all)?;
        Ok(metadata)
    }
    pub async fn search(&self, query: String) -> Result<ToolSearch, String> {
        let query = query.trim();
        if query.is_empty()
            || query.len() > 160
            || query.starts_with('-')
            || query.contains(['\0', '\r', '\n'])
        {
            return Err("请输入有效的 1–160 字工具名称或 GitHub 地址，不能使用命令选项".into());
        }
        let mut result = ToolSearch {
            candidates: Vec::new(),
            diagnostics: Vec::new(),
        };
        let definitions = self.definitions()?;
        let needle = query.to_lowercase();
        for tool in &definitions {
            if format!("{} {} {}", tool.id, tool.name, tool.description)
                .to_lowercase()
                .contains(&needle)
            {
                result.candidates.push(ToolCandidate {
                    name: tool.name.clone(),
                    description: tool.description.clone(),
                    repository: tool.repository.clone(),
                    homepage: tool.homepage.clone(),
                    provider: "catalog".into(),
                    package: None,
                    tool_id: Some(tool.id.clone()),
                });
            }
        }
        let github = async {
            if let Ok(repo) = github_repository(query) {
                let data = self
                    .github_json(&format!("{}/repos/{repo}", self.github_api))
                    .await?
                    .ok_or("仓库不存在")?;
                Ok::<_, String>(vec![data])
            } else {
                let mut url =
                    reqwest::Url::parse(&format!("{}/search/repositories", self.github_api))
                        .map_err(|e| e.to_string())?;
                url.query_pairs_mut()
                    .append_pair("q", query)
                    .append_pair("per_page", "12");
                let value = self
                    .github_json(url.as_str())
                    .await?
                    .ok_or("GitHub 搜索不存在")?;
                Ok(value
                    .get("items")
                    .and_then(Value::as_array)
                    .ok_or("GitHub 搜索响应缺少 items")?
                    .clone())
            }
        }
        .await;
        match github {
            Ok(rows) => {
                for row in rows {
                    let Some(repository) = row.get("html_url").and_then(Value::as_str) else {
                        result.diagnostics.push("GitHub 候选缺少仓库地址".into());
                        continue;
                    };
                    if result
                        .candidates
                        .iter()
                        .any(|c| c.repository.eq_ignore_ascii_case(repository))
                    {
                        continue;
                    }
                    result.candidates.push(ToolCandidate {
                        name: row
                            .get("full_name")
                            .and_then(Value::as_str)
                            .unwrap_or(repository)
                            .into(),
                        description: row
                            .get("description")
                            .and_then(Value::as_str)
                            .unwrap_or("未提供简介")
                            .into(),
                        repository: repository.into(),
                        homepage: repository.into(),
                        provider: "github".into(),
                        package: None,
                        tool_id: definitions
                            .iter()
                            .find(|t| t.repository.eq_ignore_ascii_case(repository))
                            .map(|t| t.id.clone()),
                    });
                }
            }
            Err(error) => result.diagnostics.push(error),
        }
        let snapshot = self.snapshot()?;
        // 包管理器结果仅作候选资料；执行仍要求内置的已核实映射。
        for provider in ["winget", "scoop", "brew"] {
            if !snapshot
                .providers
                .iter()
                .any(|p| p.id == provider && p.available)
            {
                continue;
            }
            if query.starts_with("http") || query.contains('/') {
                continue;
            }
            let args = match provider {
                "winget" => vec![
                    "search",
                    query,
                    "--source",
                    "winget",
                    "--disable-interactivity",
                ],
                "scoop" => vec!["search", query],
                "brew" => vec!["search", "--formula", query],
                _ => vec![],
            }
            .into_iter()
            .map(str::to_string)
            .collect();
            let output = self.provider_command(&snapshot, provider, args).await;
            match output.and_then(|o| o.checked().map(str::to_string)) {
                Ok(text) => {
                    // 文本表格只关联预定义包名，避免依赖本地化表头及把任意行变成执行指令。
                    for tool in &definitions {
                        for mapping in tool.packages.iter().filter(|p| p.provider == provider) {
                            if text.split_whitespace().any(|token| {
                                token.eq_ignore_ascii_case(&mapping.package)
                                    || (provider != "winget"
                                        && token
                                            == mapping.package.rsplit('/').next().unwrap_or(""))
                            }) {
                                result.candidates.push(ToolCandidate {
                                    name: tool.name.clone(),
                                    description: tool.description.clone(),
                                    repository: tool.repository.clone(),
                                    homepage: tool.homepage.clone(),
                                    provider: provider.into(),
                                    package: Some(mapping.package.clone()),
                                    tool_id: Some(tool.id.clone()),
                                });
                            }
                        }
                    }
                }
                Err(error) => result.diagnostics.push(format!("{provider} 搜索：{error}")),
            }
        }
        Ok(result)
    }
    pub async fn register(&self, candidate: ToolCandidate) -> Result<ToolSnapshot, String> {
        let repo = github_repository(&candidate.repository)?;
        let definitions = self.definitions()?;
        let known = definitions
            .iter()
            .find(|t| {
                t.repository
                    .eq_ignore_ascii_case(&format!("https://github.com/{repo}"))
            })
            .cloned();
        let definition = if let Some(mut known) = known {
            known.registered = true;
            known
        } else {
            // 未知仓库重新获取身份，客户端不能指定命令或包管理器映射。
            let data = self
                .github_json(&format!("{}/repos/{repo}", self.github_api))
                .await?
                .ok_or("仓库不存在")?;
            let name = data
                .get("full_name")
                .and_then(Value::as_str)
                .ok_or("仓库缺少 full_name")?;
            ToolDefinition {
                id: format!("github:{repo}"),
                name: name.into(),
                description: data
                    .get("description")
                    .and_then(Value::as_str)
                    .unwrap_or("未提供简介")
                    .into(),
                commands: vec![],
                homepage: data
                    .get("homepage")
                    .and_then(Value::as_str)
                    .filter(|s| s.starts_with("https://"))
                    .unwrap_or(&candidate.repository)
                    .into(),
                repository: format!("https://github.com/{repo}"),
                runtime_provider: None,
                packages: vec![],
                registered: true,
            }
        };
        let id = definition.id.clone();
        let mut registered: Vec<ToolDefinition> = self.load("registered")?.unwrap_or_default();
        registered.retain(|r| r.id != id);
        registered.push(definition);
        self.save("registered", &registered)?;
        self.refresh_metadata(id).await?;
        self.snapshot()
    }
}
