mod catalog;
mod metadata;
mod operations;
pub mod process;
mod projects;
pub mod providers;
#[cfg(test)]
mod tests;
pub mod types;
mod updates;

use catalog::{catalog, provider_guide, provider_ids};
use process::{CommandRunner, Invocation, ProcessOutput, SystemRunner};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{de::DeserializeOwned, Serialize};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};
pub use types::*;

pub struct LocalToolsService {
    directory: PathBuf,
    home: PathBuf,
    db: Mutex<Connection>,
    runner: Arc<dyn CommandRunner>,
    client: reqwest::Client,
    plans: Mutex<HashMap<String, operations::StoredPlan>>,
    provider_locks: Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>,
    scan_lock: tokio::sync::Mutex<()>,
    discovery_directories: Option<Vec<PathBuf>>,
    github_api: String,
}

fn now() -> String {
    chrono::Utc::now().to_rfc3339()
}
fn path_text(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}
fn installation_id(
    tool: &str,
    path: &str,
    provider: Option<&str>,
    version: Option<&str>,
) -> String {
    use sha2::{Digest, Sha256};
    format!(
        "{:x}",
        Sha256::digest(
            format!(
                "{tool}\0{path}\0{}\0{}",
                provider.unwrap_or(""),
                version.unwrap_or("")
            )
            .as_bytes()
        )
    )
}

impl LocalToolsService {
    pub fn new(directory: PathBuf, home: PathBuf) -> Result<Self, String> {
        Self::with_runner(directory, home, Arc::new(SystemRunner))
    }
    pub fn with_runner(
        directory: PathBuf,
        home: PathBuf,
        runner: Arc<dyn CommandRunner>,
    ) -> Result<Self, String> {
        std::fs::create_dir_all(directory.join("work")).map_err(|e| e.to_string())?;
        let directory = directory.canonicalize().map_err(|e| e.to_string())?;
        let db = Connection::open(directory.join("tools.sqlite")).map_err(|e| e.to_string())?;
        db.execute_batch("PRAGMA journal_mode=WAL; CREATE TABLE IF NOT EXISTS documents (key TEXT PRIMARY KEY, value TEXT NOT NULL); CREATE TABLE IF NOT EXISTS operations (id TEXT PRIMARY KEY, value TEXT NOT NULL);").map_err(|e|e.to_string())?;
        let client = reqwest::Client::builder()
            .user_agent("my-toolbox-local-tools/0.1")
            .timeout(Duration::from_secs(20))
            .redirect(reqwest::redirect::Policy::limited(3))
            .build()
            .map_err(|e| e.to_string())?;
        let service = Self {
            directory,
            home,
            db: Mutex::new(db),
            runner,
            client,
            plans: Mutex::new(HashMap::new()),
            provider_locks: Mutex::new(HashMap::new()),
            scan_lock: tokio::sync::Mutex::new(()),
            discovery_directories: None,
            github_api: "https://api.github.com".into(),
        };
        for mut operation in service.operations()? {
            if matches!(operation.status.as_str(), "queued" | "running") {
                operation.status = "interrupted".into();
                operation.finished_at = Some(now());
                operation
                    .output
                    .push_str("\n应用上次退出时操作未完成，请重新扫描确认实际状态。\n");
                service.save_operation(&operation)?;
            }
        }
        Ok(service)
    }
    /// 仅供隔离测试与只读诊断注入发现目录，桌面入口始终使用真实环境。
    pub fn isolated(
        directory: PathBuf,
        home: PathBuf,
        runner: Arc<dyn CommandRunner>,
        directories: Vec<PathBuf>,
        metadata_endpoint: Option<String>,
    ) -> Result<Self, String> {
        let mut service = Self::with_runner(directory, home, runner)?;
        service.discovery_directories = Some(directories);
        if let Some(endpoint) = metadata_endpoint {
            let url = reqwest::Url::parse(&endpoint).map_err(|e| e.to_string())?;
            if url.scheme() != "http"
                || !matches!(url.host_str(), Some("127.0.0.1") | Some("localhost"))
            {
                return Err("隔离元数据服务必须是本机 HTTP 地址".into());
            }
            service.github_api = endpoint.trim_end_matches('/').into();
        }
        Ok(service)
    }
    fn load<T: DeserializeOwned>(&self, key: &str) -> Result<Option<T>, String> {
        let db = self.db.lock().map_err(|e| e.to_string())?;
        let text: Option<String> = db
            .query_row("SELECT value FROM documents WHERE key=?1", [key], |row| {
                row.get(0)
            })
            .optional()
            .map_err(|e| e.to_string())?;
        text.map(|s| serde_json::from_str(&s).map_err(|e| e.to_string()))
            .transpose()
    }
    fn save<T: Serialize>(&self, key: &str, value: &T) -> Result<(), String> {
        self.db.lock().map_err(|e|e.to_string())?.execute("INSERT INTO documents(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",params![key,serde_json::to_string(value).map_err(|e|e.to_string())?]).map_err(|e|e.to_string())?;
        Ok(())
    }
    fn definitions(&self) -> Result<Vec<ToolDefinition>, String> {
        let mut definitions = catalog();
        let registered: Vec<ToolDefinition> = self.load("registered")?.unwrap_or_default();
        for definition in &mut definitions {
            definition.registered = registered.iter().any(|r| r.id == definition.id);
        }
        definitions.extend(
            registered
                .into_iter()
                .filter(|d| !catalog().iter().any(|c| c.id == d.id)),
        );
        Ok(definitions)
    }
    pub fn snapshot(&self) -> Result<ToolSnapshot, String> {
        let mut snapshot: ToolSnapshot = self.load("snapshot")?.unwrap_or(ToolSnapshot {
            definitions: vec![],
            installations: vec![],
            providers: vec![],
            metadata: vec![],
            projects: vec![],
            directories: vec![],
            scanned_at: None,
            diagnostics: vec![],
            updates: vec![],
            updates_checked_at: None,
        });
        snapshot.definitions = self.definitions()?;
        snapshot.metadata = self.load("metadata")?.unwrap_or_default();
        snapshot.projects = self.load("projects")?.unwrap_or_default();
        snapshot.directories = self.load("directories")?.unwrap_or_default();
        if let Some(check) = self.load::<ToolUpdateCheck>("updates")? {
            snapshot.updates = check.updates;
            snapshot.updates_checked_at = Some(check.checked_at);
        }
        Ok(snapshot)
    }
    pub fn set_directories(&self, directories: Vec<String>) -> Result<(), String> {
        let roots = directories
            .iter()
            .map(|d| projects::project_root(d).map(|p| path_text(&p)))
            .collect::<Result<Vec<_>, _>>()?;
        self.save("directories", &roots)
    }
    fn search_directories(&self) -> Result<Vec<PathBuf>, String> {
        if let Some(directories) = &self.discovery_directories {
            return Ok(directories.clone());
        }
        let mut directories: Vec<PathBuf> =
            std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
                .filter(|p| p.is_absolute())
                .collect();
        for key in [
            "CARGO_HOME",
            "CARGO_INSTALL_ROOT",
            "JAVA_HOME",
            "MAVEN_HOME",
            "M2_HOME",
        ] {
            if let Some(root) = std::env::var_os(key) {
                directories.push(PathBuf::from(root).join("bin"));
            }
        }
        directories.extend([
            self.home.join(".cargo/bin"),
            self.home.join(".local/bin"),
            self.home.join("scoop/shims"),
            self.home.join(".bun/bin"),
            self.home.join(".deno/bin"),
            self.home.join(".local/share/mise/shims"),
            self.home.join(".local/share/mise/bin"),
        ]);
        if cfg!(windows) {
            directories.extend([
                self.home.join("AppData/Local/Microsoft/WindowsApps"),
                self.home.join("AppData/Local/Microsoft/WinGet/Links"),
                self.home.join("AppData/Roaming/npm"),
            ]);
        } else {
            directories.extend([
                PathBuf::from("/opt/homebrew/bin"),
                PathBuf::from("/usr/local/bin"),
                PathBuf::from("/home/linuxbrew/.linuxbrew/bin"),
            ]);
        }
        let custom: Vec<String> = self.load("directories")?.unwrap_or_default();
        directories.extend(custom.into_iter().map(PathBuf::from));
        let mut seen = std::collections::HashSet::new();
        directories.retain(|d| seen.insert(d.clone()));
        Ok(directories)
    }
    fn invocation(&self, executable: &Path, args: Vec<String>, timeout: u64) -> Invocation {
        Invocation {
            executable: executable.into(),
            args,
            cwd: self.directory.join("work"),
            timeout: Duration::from_secs(timeout),
            env: vec![
                ("NO_COLOR".into(), "1".into()),
                ("CLICOLOR".into(), "0".into()),
                ("HOMEBREW_NO_AUTO_UPDATE".into(), "1".into()),
                ("HOMEBREW_NO_ANALYTICS".into(), "1".into()),
                ("UV_NO_CONFIG".into(), "1".into()),
                ("RUSTUP_AUTO_INSTALL".into(), "0".into()),
                ("MISE_AUTO_INSTALL".into(), "0".into()),
                ("MISE_PARANOID".into(), "1".into()),
                ("MISE_NO_HOOKS".into(), "1".into()),
                ("MISE_NO_ENV".into(), "1".into()),
                ("UV_NO_MODIFY_PATH".into(), "1".into()),
            ],
            remove_env: vec![
                "RUSTUP_TOOLCHAIN".into(),
                "MISE_NODE_VERSION".into(),
                "MISE_PYTHON_VERSION".into(),
                "MISE_JAVA_VERSION".into(),
                "MISE_BUN_VERSION".into(),
                "MISE_DENO_VERSION".into(),
                "MISE_PNPM_VERSION".into(),
                "MISE_MAVEN_VERSION".into(),
            ],
            output_path: None,
        }
    }
    async fn run(
        &self,
        executable: &Path,
        args: Vec<String>,
        timeout: u64,
    ) -> Result<ProcessOutput, String> {
        self.runner
            .run(&self.invocation(executable, args, timeout))
            .await
    }
    async fn provider_command(
        &self,
        snapshot: &ToolSnapshot,
        id: &str,
        args: Vec<String>,
    ) -> Result<ProcessOutput, String> {
        let provider = snapshot
            .providers
            .iter()
            .find(|p| p.id == id && p.available)
            .ok_or_else(|| format!("{id} 不可用，请先安装或查看初始化指引"))?;
        self.run(
            Path::new(
                provider
                    .executable
                    .as_deref()
                    .ok_or("管理器没有可执行路径")?,
            ),
            args,
            30,
        )
        .await
    }
    fn add_installation(
        snapshot: &mut ToolSnapshot,
        tool: &str,
        version: Option<String>,
        path: PathBuf,
        provider: Option<&str>,
        package: Option<String>,
        evidence: String,
        resolved: bool,
        diagnostic: Option<String>,
    ) {
        let path = path_text(&path);
        if let Some(existing) = snapshot
            .installations
            .iter_mut()
            .find(|i| i.tool_id == tool && i.path == path)
        {
            if provider.is_some() {
                existing.provider = provider.map(str::to_string);
                existing.package = package;
                existing.evidence = evidence;
                existing.id = installation_id(tool, &path, provider, version.as_deref());
            }
            existing.resolved |= resolved;
            return;
        }
        snapshot.installations.push(ToolInstallation {
            id: installation_id(tool, &path, provider, version.as_deref()),
            tool_id: tool.into(),
            version,
            path,
            provider: provider.map(str::to_string),
            package,
            evidence,
            resolved,
            status: if diagnostic.is_some() {
                "error"
            } else {
                "ready"
            }
            .into(),
            diagnostic,
        });
    }
    pub async fn scan(&self) -> Result<ToolSnapshot, String> {
        let _guard = self.scan_lock.lock().await;
        let mut snapshot = self.snapshot()?;
        snapshot.installations.clear();
        snapshot.providers.clear();
        snapshot.diagnostics.clear();
        let mut directories = self.search_directories()?;
        #[cfg(windows)]
        if self.discovery_directories.is_none() {
            // 安装器更新注册表 PATH 后，当前进程环境仍是旧值；只读枚举可补充新安装路径。
            match self.windows_install_directories().await {
                Ok(roots) => {
                    for root in roots {
                        if !directories.contains(&root) {
                            directories.push(root);
                        }
                    }
                }
                Err(error) => snapshot
                    .diagnostics
                    .push(format!("Windows 安装目录发现失败：{error}")),
            }
        }
        let path_directories: Vec<PathBuf> = if self.discovery_directories.is_some() {
            directories.clone()
        } else {
            std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
                .filter(|p| p.is_absolute())
                .collect()
        };
        for id in provider_ids() {
            let executable = process::find_commands(id, &directories).into_iter().next();
            let mut capability = ProviderCapabilities {
                id: id.into(),
                available: false,
                executable: executable.as_ref().map(|p| path_text(p)),
                version: None,
                diagnostic: None,
                install: id != "cargo",
                upgrade: true,
                uninstall: true,
                versions: matches!(id, "fnm" | "uv" | "mise" | "rustup"),
                set_default: matches!(id, "fnm" | "uv" | "mise" | "rustup"),
                guide: provider_guide(id).into(),
            };
            if let Some(path) = executable {
                match self
                    .run(&path, vec!["--version".into()], 10)
                    .await
                    .and_then(|out| {
                        out.checked()?;
                        providers::version_from_output(id, &out.stdout, &out.stderr)
                    }) {
                    Ok(version) => {
                        capability.available = true;
                        capability.version = Some(version)
                    }
                    Err(error) => capability.diagnostic = Some(error),
                }
            } else {
                capability.diagnostic = Some("未找到管理器；可打开安装与初始化指引".into());
            }
            snapshot.providers.push(capability);
        }
        for tool in &snapshot.definitions.clone() {
            for command in &tool.commands {
                let path_entry = process::find_commands(command, &path_directories)
                    .into_iter()
                    .next();
                for path in process::find_commands(command, &directories) {
                    let result = self
                        .run(&path, providers::version_args(&tool.id), 10)
                        .await
                        .and_then(|out| {
                            out.checked()?;
                            providers::version_from_output(&tool.id, &out.stdout, &out.stderr)
                        });
                    let (version, diagnostic) = match result {
                        Ok(v) => (Some(v), None),
                        Err(e) => (None, Some(e)),
                    };
                    let resolved = path_entry.as_ref() == Some(&path)
                        && !snapshot
                            .installations
                            .iter()
                            .any(|i| i.tool_id == tool.id && i.resolved);
                    Self::add_installation(
                        &mut snapshot,
                        &tool.id,
                        version,
                        path,
                        None,
                        None,
                        "可执行文件探测；安装来源尚未核实".into(),
                        resolved,
                        diagnostic,
                    );
                }
            }
        }
        for (tool, provider) in [
            ("node", "fnm"),
            ("python", "uv"),
            ("rust", "rustup"),
            ("cargo", "rustup"),
            ("java", "mise"),
            ("bun", "mise"),
            ("deno", "mise"),
            ("pnpm", "mise"),
            ("maven", "mise"),
        ] {
            if !snapshot
                .providers
                .iter()
                .any(|p| p.id == provider && p.available)
            {
                continue;
            }
            match self.installed_versions(&snapshot, tool, provider).await {
                Ok(versions) => {
                    for version in versions.versions.into_iter().filter(|v| v.installed) {
                        if let Some(path) = version.path {
                            let path = PathBuf::from(path);
                            let executable = runtime_executable(tool, &path);
                            if !executable.is_file() {
                                snapshot.diagnostics.push(format!(
                                    "{provider} {tool}@{}：清单路径缺少可执行文件 {}",
                                    version.version,
                                    executable.display()
                                ));
                                continue;
                            }
                            let package = if provider == "uv" {
                                version.identity
                            } else {
                                Some(if tool == "cargo" { "rust" } else { tool }.into())
                            };
                            Self::add_installation(
                                &mut snapshot,
                                tool,
                                Some(version.version),
                                executable,
                                Some(provider),
                                package,
                                format!("{provider} 安装清单与实际文件匹配"),
                                false,
                                None,
                            );
                        }
                    }
                }
                Err(error) => snapshot
                    .diagnostics
                    .push(format!("{provider}/{tool}：{error}")),
            }
        }
        self.package_ownership(&mut snapshot).await;
        // PATH shim 只在实际目标与管理器清单完全一致时继承来源。
        let owned = snapshot
            .installations
            .iter()
            .filter(|i| i.provider.is_some())
            .cloned()
            .collect::<Vec<_>>();
        for item in snapshot
            .installations
            .iter_mut()
            .filter(|i| i.provider.is_none())
        {
            if let Ok(real) = Path::new(&item.path).canonicalize() {
                if let Some(owner) = owned.iter().find(|i| {
                    i.tool_id == item.tool_id
                        && Path::new(&i.path).canonicalize().is_ok_and(|p| p == real)
                }) {
                    item.provider = owner.provider.clone();
                    item.package = owner.package.clone();
                    item.evidence = owner.evidence.clone();
                    item.id = installation_id(
                        &item.tool_id,
                        &item.path,
                        item.provider.as_deref(),
                        item.version.as_deref(),
                    );
                }
            }
        }
        snapshot.scanned_at = Some(now());
        self.save("snapshot", &snapshot)?;
        Ok(snapshot)
    }
    #[cfg(windows)]
    async fn windows_install_directories(&self) -> Result<Vec<PathBuf>, String> {
        let root = std::env::var_os("SystemRoot").ok_or("缺少 SystemRoot")?;
        let powershell = PathBuf::from(root)
            .join("System32")
            .join("WindowsPowerShell")
            .join("v1.0")
            .join("powershell.exe");
        let script = r#"$ErrorActionPreference='Stop'; [Console]::OutputEncoding=[System.Text.UTF8Encoding]::new($false); $paths=@(); foreach($scope in @('Machine','User')) { $paths += ([Environment]::GetEnvironmentVariable('Path',$scope) -split ';') }; foreach($key in @('HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\*','HKLM:\Software\Microsoft\Windows\CurrentVersion\Uninstall\*','HKLM:\Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\*')) { if(Test-Path ($key -replace '\\\*$','')) { foreach($app in Get-ItemProperty $key) { if($app.InstallLocation) { $location=$app.InstallLocation.Trim().Trim('"'); $paths += $location; $paths += [IO.Path]::Combine($location,'bin') }; if($app.DisplayIcon) { $icon=($app.DisplayIcon -replace ',\s*-?\d+$','').Trim('"'); if([IO.Path]::IsPathRooted($icon)) { $paths += [IO.Path]::GetDirectoryName($icon) } } } } }; ConvertTo-Json -InputObject @($paths | Where-Object { $_ -and [IO.Path]::IsPathRooted($_) } | ForEach-Object { [Environment]::ExpandEnvironmentVariables($_) } | Select-Object -Unique) -Compress"#;
        let out = self
            .run(
                &powershell,
                vec![
                    "-NoProfile".into(),
                    "-NonInteractive".into(),
                    "-Command".into(),
                    script.into(),
                ],
                15,
            )
            .await?;
        let paths: Vec<String> =
            serde_json::from_str(out.checked()?.trim_start_matches('\u{feff}'))
                .map_err(|e| format!("安装目录 JSON 无效：{e}"))?;
        Ok(paths.into_iter().map(PathBuf::from).collect())
    }
    async fn installed_versions(
        &self,
        snapshot: &ToolSnapshot,
        tool: &str,
        provider: &str,
    ) -> Result<ToolVersions, String> {
        let args: Vec<String> = match provider {
            "fnm" => vec!["list"],
            "uv" => vec![
                "python",
                "list",
                "--only-installed",
                "--managed-python",
                "--output-format",
                "json",
                "--offline",
            ],
            "rustup" => vec!["toolchain", "list", "--verbose"],
            "mise" => vec!["ls", tool, "--installed", "--json"],
            _ => return Err("此管理器没有版本清单".into()),
        }
        .into_iter()
        .map(str::to_string)
        .collect();
        let out = self.provider_command(snapshot, provider, args).await?;
        let mut versions = match provider {
            "fnm" => providers::parse_fnm(out.checked()?)?,
            "uv" => providers::parse_uv(out.checked()?)?,
            "rustup" => providers::parse_rustup(out.checked()?)?,
            "mise" => providers::parse_mise(out.checked()?, tool)?,
            _ => unreachable!(),
        };
        if provider == "fnm" {
            let roots = fnm_roots(&self.home);
            for v in &mut versions {
                v.path = roots
                    .iter()
                    .map(|root| {
                        root.join("node-versions")
                            .join(format!("v{}", v.version))
                            .join("installation")
                    })
                    .find(|p| runtime_executable("node", p).is_file())
                    .map(|p| path_text(&p));
            }
        }
        if provider == "uv" {
            let root = self
                .provider_command(snapshot, "uv", vec!["python".into(), "dir".into()])
                .await?;
            let root = PathBuf::from(root.checked()?.trim());
            if !versions.is_empty() {
                let root = root
                    .canonicalize()
                    .map_err(|e| format!("uv Python 根目录不可访问：{e}"))?;
                versions.retain(|v| {
                    v.path.as_ref().is_some_and(|p| {
                        Path::new(p)
                            .canonicalize()
                            .is_ok_and(|p| p.starts_with(&root))
                    })
                });
            }
        }
        let mut default = versions
            .iter()
            .find(|v| v.default)
            .map(|v| v.version.clone());
        if provider == "uv" {
            let out = self
                .provider_command(
                    snapshot,
                    "uv",
                    vec![
                        "python".into(),
                        "pin".into(),
                        "--global".into(),
                        "--offline".into(),
                    ],
                )
                .await?;
            // 无全局 pin 时 CLI 返回非零；只有明确缺失信息属于正常未配置。
            if out.code == Some(0) {
                default = Some(out.stdout.trim().into());
            } else if !out.stderr.contains("No pinned Python version")
                && !out.stderr.contains("No Python version")
            {
                return Err(format!("无法读取 uv 默认版本：{}", out.stderr));
            }
        }
        if provider == "mise" {
            let out = self
                .provider_command(
                    snapshot,
                    "mise",
                    vec!["ls".into(), tool.into(), "--global".into(), "--json".into()],
                )
                .await?;
            let global = providers::parse_mise(out.checked()?, tool)?;
            if global.len() == 1 {
                default = Some(global[0].version.clone());
            }
        }
        for v in &mut versions {
            v.default = default.as_deref() == Some(v.version.as_str());
        }
        Ok(ToolVersions {
            provider: provider.into(),
            versions,
            default_version: default,
            diagnostic: None,
        })
    }
    pub async fn versions(&self, tool_id: String, remote: bool) -> Result<ToolVersions, String> {
        let snapshot = self.snapshot()?;
        let tool = snapshot
            .definitions
            .iter()
            .find(|t| t.id == tool_id)
            .ok_or("未知工具")?;
        let provider = tool
            .runtime_provider
            .as_deref()
            .ok_or("此工具不提供多版本管理")?;
        let mut result = self
            .installed_versions(&snapshot, &tool_id, provider)
            .await?;
        if remote {
            let args: Vec<String> = match provider {
                "fnm" => vec!["list-remote".into()],
                "uv" => vec![
                    "python".into(),
                    "list".into(),
                    "--all-versions".into(),
                    "--output-format".into(),
                    "json".into(),
                ],
                "mise" => vec!["ls-remote".into(), tool_id.clone(), "--json".into()],
                "rustup" => vec![],
                _ => vec![],
            };
            let remote_versions = if provider == "rustup" {
                vec!["stable".to_string(), "beta".into(), "nightly".into()]
            } else {
                let out = self.provider_command(&snapshot, provider, args).await?;
                match provider {
                    "uv" => providers::parse_uv(out.checked()?)?
                        .into_iter()
                        .map(|v| v.version)
                        .collect(),
                    "mise" => {
                        let values: serde_json::Value =
                            serde_json::from_str(out.checked()?).map_err(|e| e.to_string())?;
                        values
                            .as_array()
                            .ok_or("mise 远程版本不是数组")?
                            .iter()
                            .map(|v| {
                                v.as_str()
                                    .map(str::to_string)
                                    .or_else(|| {
                                        v.get("version")
                                            .and_then(|v| v.as_str())
                                            .map(str::to_string)
                                    })
                                    .ok_or_else(|| "mise 远程版本缺少 version".into())
                            })
                            .collect::<Result<Vec<_>, String>>()?
                    }
                    "fnm" => process::clean_text(out.checked()?)
                        .lines()
                        .filter_map(|l| l.split_whitespace().next())
                        .map(|s| s.trim_start_matches('v').to_string())
                        .collect(),
                    _ => vec![],
                }
            };
            for version in remote_versions {
                if !result.versions.iter().any(|v| v.version == version) {
                    result.versions.push(ToolVersion {
                        version,
                        identity: None,
                        installed: false,
                        default: false,
                        path: None,
                    });
                }
            }
            if provider == "rustup" {
                result.diagnostic = Some("列出官方通道；也可输入精确版本或完整工具链名称".into());
            }
        }
        Ok(result)
    }
    async fn package_ownership(&self, snapshot: &mut ToolSnapshot) {
        for provider in ["scoop", "brew", "cargo"] {
            if !snapshot
                .providers
                .iter()
                .any(|p| p.id == provider && p.available)
            {
                continue;
            }
            let result = async {
                let receipts = if provider == "scoop" {
                    let out = self
                        .provider_command(snapshot, "scoop", vec!["export".into()])
                        .await?;
                    let root = std::env::var_os("SCOOP")
                        .map(PathBuf::from)
                        .unwrap_or_else(|| self.home.join("scoop"));
                    providers::parse_scoop(out.checked()?, &root)?
                } else if provider == "brew" {
                    let prefix = self
                        .provider_command(snapshot, "brew", vec!["--cellar".into()])
                        .await?;
                    let cellar = PathBuf::from(prefix.checked()?.trim());
                    let out = self
                        .provider_command(
                            snapshot,
                            "brew",
                            vec!["info".into(), "--json=v2".into(), "--installed".into()],
                        )
                        .await?;
                    providers::parse_brew(out.checked()?, &cellar)?
                } else {
                    let root = cargo_root(&self.home)?;
                    let out = self
                        .provider_command(
                            snapshot,
                            "cargo",
                            vec![
                                "install".into(),
                                "--list".into(),
                                "--root".into(),
                                path_text(&root),
                            ],
                        )
                        .await?;
                    providers::cargo_has_fnm(out.checked()?)
                        .map(|version| {
                            vec![providers::PackageReceipt {
                                package: "fnm".into(),
                                version,
                                root: root.join("bin"),
                            }]
                        })
                        .unwrap_or_default()
                };
                for receipt in receipts {
                    let Some(tool) = snapshot
                        .definitions
                        .iter()
                        .find(|t| {
                            if provider == "cargo" {
                                t.id == "fnm"
                            } else {
                                t.packages.iter().any(|p| {
                                    p.provider == provider
                                        && (p.package == receipt.package
                                            || p.package.rsplit('/').next()
                                                == receipt.package.rsplit('/').next())
                                })
                            }
                        })
                        .cloned()
                    else {
                        continue;
                    };
                    let mut bins = vec![
                        receipt.root.clone(),
                        receipt.root.join("bin"),
                        receipt.root.join("current"),
                        receipt.root.join("current/bin"),
                    ];
                    if tool.id == "java" {
                        bins.extend([
                            receipt.root.join("libexec/openjdk.jdk/Contents/Home/bin"),
                            receipt.root.join("Contents/Home/bin"),
                        ]);
                    }
                    for command in &tool.commands {
                        for path in process::find_commands(command, &bins) {
                            if !path.canonicalize().is_ok_and(|p| {
                                receipt.root.canonicalize().is_ok_and(|r| p.starts_with(r))
                            }) {
                                continue;
                            }
                            let out = self
                                .run(&path, providers::version_args(&tool.id), 10)
                                .await?;
                            out.checked()?;
                            let version =
                                providers::version_from_output(&tool.id, &out.stdout, &out.stderr)?;
                            Self::add_installation(
                                snapshot,
                                &tool.id,
                                Some(version),
                                path,
                                Some(provider),
                                Some(receipt.package.clone()),
                                format!("{provider} 安装收据与文件归属匹配"),
                                false,
                                None,
                            );
                        }
                    }
                    for item in snapshot
                        .installations
                        .iter_mut()
                        .filter(|i| i.tool_id == tool.id && i.provider.is_none())
                    {
                        if Path::new(&item.path).canonicalize().is_ok_and(|p| {
                            receipt
                                .root
                                .canonicalize()
                                .is_ok_and(|root| p.starts_with(root))
                        }) {
                            item.provider = Some(provider.into());
                            item.package = Some(receipt.package.clone());
                            item.evidence = format!(
                                "{provider} 安装收据 {}@{} 与实际文件归属匹配",
                                receipt.package, receipt.version
                            );
                            item.id = installation_id(
                                &item.tool_id,
                                &item.path,
                                Some(provider),
                                item.version.as_deref(),
                            );
                        }
                    }
                }
                Ok::<_, String>(())
            }
            .await;
            if let Err(error) = result {
                snapshot
                    .diagnostics
                    .push(format!("{provider} 来源核实失败：{error}"));
            }
        }
        // uv 安装器收据是来源证据；不能仅凭 .local/bin 的位置认定独立安装。
        let receipt_roots = if cfg!(windows) {
            vec![
                self.home.join("AppData/Local/uv"),
                self.home.join(".config/uv"),
            ]
        } else {
            vec![std::env::var_os("XDG_CONFIG_HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| self.home.join(".config"))
                .join("uv")]
        };
        for receipt in receipt_roots
            .into_iter()
            .map(|p| p.join("uv-receipt.json"))
            .filter(|p| p.exists())
        {
            match std::fs::read_to_string(&receipt)
                .map_err(|e| e.to_string())
                .and_then(|s| {
                    serde_json::from_str::<serde_json::Value>(&s).map_err(|e| e.to_string())
                }) {
                Ok(value) => {
                    if let Some(prefix) = value
                        .get("install_prefix")
                        .and_then(|v| v.as_str())
                        .filter(|_| {
                            value.pointer("/source/owner").and_then(|v| v.as_str())
                                == Some("astral-sh")
                                && value.pointer("/source/name").and_then(|v| v.as_str())
                                    == Some("uv")
                        })
                    {
                        let root = PathBuf::from(prefix);
                        for item in snapshot
                            .installations
                            .iter_mut()
                            .filter(|i| i.tool_id == "uv" && i.provider.is_none())
                        {
                            if Path::new(&item.path).canonicalize().is_ok_and(|p| {
                                root.canonicalize()
                                    .is_ok_and(|r| p.parent() == Some(r.as_path()))
                            }) {
                                item.provider = Some("uv-self".into());
                                item.package = Some("uv".into());
                                item.evidence = format!("uv 独立安装收据：{}", receipt.display());
                                item.id = installation_id(
                                    "uv",
                                    &item.path,
                                    Some("uv-self"),
                                    item.version.as_deref(),
                                );
                            }
                        }
                    }
                }
                Err(error) => snapshot
                    .diagnostics
                    .push(format!("uv 安装收据读取失败：{error}")),
            }
        }
        let receipts: Vec<ToolInstallation> = match self.load("receipts") {
            Ok(v) => v.unwrap_or_default(),
            Err(e) => {
                snapshot.diagnostics.push(e);
                vec![]
            }
        };
        for item in snapshot
            .installations
            .iter_mut()
            .filter(|i| i.provider.is_none())
        {
            if let Some(receipt) = receipts.iter().find(|r| {
                r.tool_id == item.tool_id && r.path == item.path && r.version == item.version
            }) {
                item.provider = receipt.provider.clone();
                item.package = receipt.package.clone();
                item.evidence = receipt.evidence.clone();
                item.id = installation_id(
                    &item.tool_id,
                    &item.path,
                    item.provider.as_deref(),
                    item.version.as_deref(),
                );
            }
        }
    }
}

fn fnm_roots(home: &Path) -> Vec<PathBuf> {
    let mut roots = vec![];
    if let Some(root) = std::env::var_os("FNM_DIR") {
        roots.push(PathBuf::from(root));
    }
    roots.extend([
        home.join("AppData/Roaming/fnm"),
        home.join(".local/share/fnm"),
        home.join("Library/Application Support/fnm"),
    ]);
    roots
}

fn cargo_root(home: &Path) -> Result<PathBuf, String> {
    if let Some(root) = std::env::var_os("CARGO_INSTALL_ROOT") {
        return Ok(PathBuf::from(root));
    }
    let cargo_home = std::env::var_os("CARGO_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".cargo"));
    for name in ["config.toml", "config"] {
        let path = cargo_home.join(name);
        if path.is_file() {
            let doc = std::fs::read_to_string(&path)
                .map_err(|e| e.to_string())?
                .parse::<toml_edit::Document>()
                .map_err(|e| e.to_string())?;
            if let Some(root) = doc
                .get("install")
                .and_then(|i| i.get("root"))
                .and_then(|v| v.as_str())
            {
                let path = PathBuf::from(root);
                return Ok(if path.is_absolute() {
                    path
                } else {
                    cargo_home
                        .parent()
                        .ok_or("Cargo 配置根无父目录")?
                        .join(path)
                });
            }
            break;
        }
    }
    Ok(cargo_home)
}

fn runtime_executable(tool: &str, path: &Path) -> PathBuf {
    if path.is_file() {
        return path.into();
    }
    let name = match tool {
        "node" => "node",
        "python" => "python",
        "rust" => "rustc",
        "cargo" => "cargo",
        "java" => "java",
        "maven" => "mvn",
        other => other,
    };
    let filename = if cfg!(windows) {
        format!(
            "{name}{}",
            if matches!(tool, "maven" | "pnpm") {
                ".cmd"
            } else {
                ".exe"
            }
        )
    } else {
        name.into()
    };
    let direct = path.join(&filename);
    if direct.is_file() {
        direct
    } else {
        path.join("bin").join(filename)
    }
}
