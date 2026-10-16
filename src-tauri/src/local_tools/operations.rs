use super::*;

pub(super) struct StoredPlan {
    pub plan: ToolOperationPlan,
    pub installation: Option<ToolInstallation>,
    pub executable_fingerprint: Option<String>,
}

fn fingerprint(path: &Path) -> Result<String, String> {
    let metadata = path.metadata().map_err(|e| e.to_string())?;
    Ok(format!(
        "{}:{}:{:?}",
        path_text(&path.canonicalize().map_err(|e| e.to_string())?),
        metadata.len(),
        metadata.modified().map_err(|e| e.to_string())?
    ))
}

impl LocalToolsService {
    pub fn operations(&self) -> Result<Vec<ToolOperation>, String> {
        let db = self.db.lock().map_err(|e| e.to_string())?;
        let mut statement = db
            .prepare("SELECT value FROM operations ORDER BY rowid DESC LIMIT 100")
            .map_err(|e| e.to_string())?;
        let records = statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|e| e.to_string())?;
        let mut output = Vec::new();
        for record in records {
            let mut operation: ToolOperation =
                serde_json::from_str(&record.map_err(|e| e.to_string())?)
                    .map_err(|e| e.to_string())?;
            if operation.status == "running" && operation.exit_code.is_none() {
                let path = self
                    .directory
                    .join(format!("operation-{}.log", operation.id));
                if path.is_file() {
                    operation.output.push_str(&String::from_utf8_lossy(
                        &std::fs::read(&path).map_err(|e| e.to_string())?,
                    ));
                }
            }
            output.push(operation);
        }
        Ok(output)
    }
    pub(super) fn save_operation(&self, operation: &ToolOperation) -> Result<(), String> {
        self.db.lock().map_err(|e|e.to_string())?.execute("INSERT INTO operations(id,value) VALUES(?1,?2) ON CONFLICT(id) DO UPDATE SET value=excluded.value",params![operation.id,serde_json::to_string(operation).map_err(|e|e.to_string())?]).map_err(|e|e.to_string())?;
        Ok(())
    }
    pub async fn project(
        &self,
        tool_id: String,
        directory: String,
    ) -> Result<ProjectToolConfig, String> {
        let root = projects::project_root(&directory)?;
        let (files, selected_version) = projects::inspect(&root, &tool_id)?;
        let mut diagnostics =
            vec!["项目固定只写版本声明；是否生效取决于 shell 初始化及管理器的覆盖优先级。".into()];
        let mut effective_version = None;
        if matches!(tool_id.as_str(), "rust" | "cargo") {
            if let Some(value) = std::env::var_os("RUSTUP_TOOLCHAIN") {
                diagnostics.push(format!(
                    "工具箱继承的 RUSTUP_TOOLCHAIN={} 可能覆盖项目文件",
                    value.to_string_lossy()
                ));
            }
            let snapshot = self.snapshot()?;
            if let Some(executable) = snapshot
                .providers
                .iter()
                .find(|p| p.id == "rustup" && p.available)
                .and_then(|p| p.executable.as_ref())
            {
                // show 不会安装工具链；保留继承的 override 环境，报告这个进程的真实解析结果。
                let mut invocation = self.invocation(
                    Path::new(executable),
                    vec!["show".into(), "active-toolchain".into()],
                    10,
                );
                invocation.cwd = root.clone();
                invocation.remove_env.clear();
                invocation
                    .env
                    .push(("RUSTUP_AUTO_INSTALL".into(), "0".into()));
                match self
                    .runner
                    .run(&invocation)
                    .await
                    .and_then(|o| o.checked().map(str::to_string))
                {
                    Ok(text) => {
                        effective_version = text.split_whitespace().next().map(str::to_string)
                    }
                    Err(error) => diagnostics.push(error),
                }
            }
        } else if let Some(version) = &selected_version {
            diagnostics.push(format!("声明版本：{version}；未执行项目代码，其他管理器/父目录配置的实际生效结果尚未核实。"));
        }
        Ok(ProjectToolConfig {
            directory: path_text(&root),
            files,
            selected_version,
            effective_version,
            diagnostics,
        })
    }
    pub async fn preview(
        &self,
        mut request: ToolOperationRequest,
    ) -> Result<ToolOperationPlan, String> {
        if !matches!(
            request.action.as_str(),
            "install" | "upgrade" | "uninstall" | "set-default" | "pin-project"
        ) {
            return Err("未知工具操作".into());
        }
        let snapshot = self.snapshot()?;
        let tool = snapshot
            .definitions
            .iter()
            .find(|t| t.id == request.tool_id)
            .ok_or("未知工具")?;
        let installation = request
            .installation_id
            .as_ref()
            .map(|id| {
                snapshot
                    .installations
                    .iter()
                    .find(|i| &i.id == id)
                    .cloned()
                    .ok_or("安装实例已改变，请扫描后重试")
            })
            .transpose()?;
        if installation
            .as_ref()
            .is_some_and(|i| i.tool_id != request.tool_id)
        {
            return Err("安装实例不属于所选工具".into());
        }
        if let Some(v) = &request.version {
            providers::validate_token(v)?;
            if v.contains(['/', '@']) || v.starts_with('.') {
                return Err("版本不能包含路径或后端表达式".into());
            }
        }
        if request.action == "pin-project" {
            let root =
                projects::project_root(request.project_path.as_deref().ok_or("请选择项目目录")?)?;
            let version = request.version.as_deref().ok_or("请选择或输入项目版本")?;
            let files = projects::preview(&root, &request.tool_id, version)?;
            request.project_path = Some(path_text(&root));
            request.provider = tool.runtime_provider.clone();
            let plan = ToolOperationPlan {
                id: uuid::Uuid::new_v4().to_string(),
                request,
                executable: None,
                args: vec![],
                files,
                impact: vec![
                    "仅修改所列项目版本文件，保留相邻备份；不会自动安装工具或执行项目任务。".into(),
                    "项目声明与管理器默认值、已打开终端的当前版本相互独立。".into(),
                ],
                created_at: now(),
            };
            self.plans.lock().map_err(|e| e.to_string())?.insert(
                plan.id.clone(),
                StoredPlan {
                    plan: plan.clone(),
                    installation: None,
                    executable_fingerprint: None,
                },
            );
            return Ok(plan);
        }
        let provider = if matches!(request.action.as_str(), "upgrade" | "uninstall") {
            installation
                .as_ref()
                .ok_or("升级或卸载必须选择具体安装实例")?
                .provider
                .clone()
                .ok_or("安装来源尚未核实，不能升级或卸载此实例")?
        } else {
            request
                .provider
                .clone()
                .or_else(|| tool.runtime_provider.clone())
                .ok_or("请选择安装管理器")?
        };
        if request.provider.as_ref().is_some_and(|p| p != &provider) {
            return Err("操作管理器与实例来源不一致".into());
        }
        request.provider = Some(provider.clone());
        if provider == "winget" {
            if let Some(item) = &installation {
                if snapshot.installations.iter().any(|other| {
                    other.tool_id == item.tool_id
                        && other.package == item.package
                        && other.provider == item.provider
                        && other.path != item.path
                }) {
                    return Err(
                        "此 WinGet 包对应多份安装入口，无法唯一指定安装器目标，请先人工核实".into(),
                    );
                }
                if request.action == "uninstall" {
                    request.version = item.version.clone();
                }
            }
        }
        let manager = if provider == "uv-self" {
            "uv"
        } else {
            &provider
        };
        let executable = if provider == "uv-self" {
            installation.as_ref().map(|i| i.path.clone())
        } else {
            snapshot
                .providers
                .iter()
                .find(|p| p.id == manager && p.available)
                .and_then(|p| p.executable.clone())
        }
        .ok_or_else(|| format!("{manager} 不可用，请先安装或完成初始化"))?;
        let package = if let Some(item) = &installation {
            item.package.clone().ok_or("安装实例缺少管理器包名")?
        } else if tool.runtime_provider.as_deref() == Some(&provider) {
            if tool.id == "cargo" {
                "rust".into()
            } else {
                tool.id.clone()
            }
        } else {
            tool.packages
                .iter()
                .find(|p| p.provider == provider)
                .map(|p| p.package.clone())
                .ok_or("此工具没有受支持的管理器映射")?
        };
        if matches!(provider.as_str(), "fnm" | "uv" | "rustup" | "mise") {
            if provider != tool.runtime_provider.as_deref().unwrap_or("") {
                return Err("管理器与工具的版本管理关系不匹配".into());
            }
            if request.action == "uninstall" {
                request.version = installation.as_ref().and_then(|i| {
                    if provider == "uv" {
                        i.package.clone()
                    } else {
                        i.version.clone()
                    }
                });
            }
            if request.action == "set-default" {
                let versions = self
                    .installed_versions(&snapshot, &tool.id, &provider)
                    .await?;
                if !versions
                    .versions
                    .iter()
                    .any(|v| v.installed && Some(&v.version) == request.version.as_ref())
                {
                    return Err("只能选择已安装版本作为默认值".into());
                }
            }
        }
        let mut args = providers::command_args(
            &provider,
            &request.action,
            &package,
            request.version.as_deref(),
        )?;
        if provider == "cargo" {
            args.extend(["--root".into(), path_text(&cargo_root(&self.home)?)]);
        }
        if provider == "rustup" && matches!(request.action.as_str(), "install" | "upgrade") {
            args.push("--no-self-update".into());
        }
        if provider == "fnm" && matches!(request.action.as_str(), "install" | "upgrade") {
            args.extend(["--progress".into(), "never".into()]);
        }
        let mut impact=vec![format!("通过 {provider} 执行 {}，目标 {package}；失败后重新探测实际状态，不承诺回滚第三方安装。",request.action)];
        if let Some(item) = &installation {
            impact.push(format!(
                "安装实例：{}\n来源证据：{}",
                item.path, item.evidence
            ));
        }
        if request.action == "set-default" {
            impact
                .push("仅修改管理器默认选择，不替换系统运行时，也不改变已打开终端的环境。".into());
        }
        if request.action == "upgrade" && matches!(provider.as_str(), "fnm" | "uv" | "mise") {
            impact.push("安装所选新版本，保留旧版本；默认版本需另行设置。".into());
        }
        if request.action == "uninstall" && installation.as_ref().is_some_and(|i| i.resolved) {
            impact.push("所选实例是工具箱当前解析入口，卸载后依赖它的命令可能不可用。".into());
        }
        if provider == "winget" && request.action != "uninstall" {
            impact.push(
                "本次确认包含 WinGet 对应源和软件包协议；可能出现系统安装器或权限提示。".into(),
            );
        }
        let plan = ToolOperationPlan {
            id: uuid::Uuid::new_v4().to_string(),
            request,
            executable: Some(executable.clone()),
            args,
            files: vec![],
            impact,
            created_at: now(),
        };
        let stored = StoredPlan {
            plan: plan.clone(),
            installation,
            executable_fingerprint: Some(fingerprint(Path::new(&executable))?),
        };
        let mut plans = self.plans.lock().map_err(|e| e.to_string())?;
        plans.retain(|_, p| {
            chrono::DateTime::parse_from_rfc3339(&p.plan.created_at)
                .is_ok_and(|t| chrono::Utc::now().signed_duration_since(t).num_minutes() < 15)
        });
        plans.insert(plan.id.clone(), stored);
        Ok(plan)
    }
    pub fn execute(self: &Arc<Self>, id: String) -> Result<ToolOperation, String> {
        let stored = self
            .plans
            .lock()
            .map_err(|e| e.to_string())?
            .remove(&id)
            .ok_or("预览不存在、已执行或已过期，请重新预览")?;
        let created = chrono::DateTime::parse_from_rfc3339(&stored.plan.created_at)
            .map_err(|e| e.to_string())?;
        if chrono::Utc::now()
            .signed_duration_since(created)
            .num_minutes()
            >= 15
        {
            return Err("预览超过 15 分钟，请重新预览".into());
        }
        let operation = ToolOperation {
            id: stored.plan.id.clone(),
            tool_id: stored.plan.request.tool_id.clone(),
            action: stored.plan.request.action.clone(),
            provider: stored
                .plan
                .request
                .provider
                .clone()
                .unwrap_or_else(|| "project".into()),
            status: "queued".into(),
            started_at: now(),
            finished_at: None,
            output: "已确认，等待同一管理器的操作完成。\n".into(),
            exit_code: None,
            backup_paths: vec![],
        };
        self.save_operation(&operation)?;
        let service = self.clone();
        let task_operation = operation.clone();
        tokio::spawn(async move {
            if let Err(error) = service.execute_stored(stored, task_operation.clone()).await {
                let mut failed = service
                    .operations()
                    .ok()
                    .and_then(|v| v.into_iter().find(|o| o.id == task_operation.id))
                    .unwrap_or(task_operation);
                failed.status = "failed".into();
                failed.finished_at = Some(now());
                failed.output.push_str(&format!("\n{error}\n"));
                if let Err(storage) = service.save_operation(&failed) {
                    eprintln!("本地工具操作失败且记录保存失败：{storage}；原始错误：{error}");
                }
            }
        });
        Ok(operation)
    }
    async fn execute_stored(
        &self,
        stored: StoredPlan,
        mut operation: ToolOperation,
    ) -> Result<(), String> {
        let provider_key = if !stored.plan.files.is_empty() {
            "project"
        } else if operation.provider == "uv-self" {
            "uv"
        } else {
            &operation.provider
        };
        let lock = {
            let mut locks = self.provider_locks.lock().map_err(|e| e.to_string())?;
            locks
                .entry(provider_key.into())
                .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
                .clone()
        };
        let _guard = lock.lock().await;
        let created = chrono::DateTime::parse_from_rfc3339(&stored.plan.created_at)
            .map_err(|e| e.to_string())?;
        if chrono::Utc::now()
            .signed_duration_since(created)
            .num_minutes()
            >= 15
        {
            return Err("排队期间预览已过期，请重新预览".into());
        }
        operation.status = "running".into();
        operation.output = "正在核对目标、来源与预览有效性…\n".into();
        self.save_operation(&operation)?;
        if !stored.plan.files.is_empty() {
            let root = projects::project_root(
                stored
                    .plan
                    .request
                    .project_path
                    .as_deref()
                    .ok_or("项目目录缺失")?,
            )?;
            let fresh = projects::preview(
                &root,
                &operation.tool_id,
                stored.plan.request.version.as_deref().ok_or("版本缺失")?,
            )?;
            if serde_json::to_string(&fresh).map_err(|e| e.to_string())?
                != serde_json::to_string(&stored.plan.files).map_err(|e| e.to_string())?
            {
                return Err("预览已过期：项目配置路径或内容发生变化".into());
            }
            operation.backup_paths = projects::apply(&stored.plan.files)?;
            for file in &stored.plan.files {
                if std::fs::read_to_string(&file.path).map_err(|e| e.to_string())? != file.after {
                    return Err("项目配置写入后校验不一致".into());
                }
            }
            let mut registered: Vec<String> = self.load("projects")?.unwrap_or_default();
            let root = path_text(&root);
            if !registered.contains(&root) {
                registered.push(root);
                self.save("projects", &registered)?;
            }
            operation
                .output
                .push_str("项目版本文件已写入并校验；是否生效请查看项目配置解析说明。\n");
        } else {
            let executable = Path::new(
                stored
                    .plan
                    .executable
                    .as_deref()
                    .ok_or("预览缺少管理器路径")?,
            );
            if Some(fingerprint(executable)?) != stored.executable_fingerprint {
                return Err("管理器可执行文件已变化，请重新预览".into());
            }
            let before = self.scan().await?;
            if let Some(item) = &stored.installation {
                if !before.installations.iter().any(|fresh| fresh == item) {
                    return Err("安装实例、来源或版本已改变，请重新预览".into());
                }
            }
            if operation.action == "set-default" {
                let versions = self
                    .installed_versions(&before, &operation.tool_id, &operation.provider)
                    .await?;
                if !versions.versions.iter().any(|v| {
                    v.installed && Some(&v.version) == stored.plan.request.version.as_ref()
                }) {
                    return Err("默认版本目标已不再安装".into());
                }
            }
            operation
                .output
                .push_str("目标核对通过，正在执行管理器；输出持续追加在下方。\n");
            self.save_operation(&operation)?;
            let mut invocation = self.invocation(executable, stored.plan.args.clone(), 1800);
            invocation.output_path = Some(
                self.directory
                    .join(format!("operation-{}.log", operation.id)),
            );
            let output = self.runner.run(&invocation).await;
            // 即使管理器报错，也重新扫描，展示实际状态而非假定回滚成功。
            let after = self.scan().await;
            let output = match output {
                Ok(output) => output,
                Err(error) => {
                    if let Err(scan_error) = after {
                        return Err(format!("{error}；重新探测失败：{scan_error}"));
                    }
                    return Err(error);
                }
            };
            operation.exit_code = output.code;
            operation.output.push_str(&output.combined());
            self.save_operation(&operation)?;
            output.checked()?;
            let mut after = after?;
            self.verify_operation(&stored, &before, &mut after).await?;
            self.save("snapshot", &after)?;
            operation
                .output
                .push_str("\n退出码与执行后状态校验通过。\n");
        }
        operation.status = "success".into();
        operation.finished_at = Some(now());
        self.save_operation(&operation)
    }
    async fn verify_operation(
        &self,
        stored: &StoredPlan,
        before: &ToolSnapshot,
        after: &mut ToolSnapshot,
    ) -> Result<(), String> {
        let request = &stored.plan.request;
        let provider = request.provider.as_deref().ok_or("缺少管理器")?;
        if matches!(provider, "fnm" | "uv" | "rustup" | "mise") {
            let versions = self
                .installed_versions(after, &request.tool_id, provider)
                .await?;
            let selected = request.version.as_deref().ok_or("缺少目标版本")?;
            let matches = |v: &ToolVersion| {
                v.version.trim_start_matches('v') == selected.trim_start_matches('v')
                    || (provider == "uv" && v.identity.as_deref() == Some(selected))
                    || (provider == "rustup" && v.version.starts_with(&format!("{selected}-")))
            };
            match request.action.as_str() {
                "uninstall" => {
                    if versions.versions.iter().any(|v| v.installed && matches(v)) {
                        return Err("管理器返回成功，但目标版本仍在安装清单中".into());
                    }
                }
                "set-default" => {
                    if !versions.versions.iter().any(|v| v.default && matches(v)) {
                        return Err("管理器返回成功，但默认版本未改变".into());
                    }
                }
                _ => {
                    let version = versions
                        .versions
                        .iter()
                        .find(|v| v.installed && matches(v))
                        .ok_or("管理器返回成功，但未找到目标版本安装")?;
                    let path = version
                        .path
                        .as_deref()
                        .ok_or("安装清单没有可核实的实际路径")?;
                    if !runtime_executable(&request.tool_id, Path::new(path)).is_file() {
                        return Err("安装清单记录存在，但可执行文件缺失".into());
                    }
                }
            }
            return Ok(());
        }
        if request.action == "uninstall" {
            let old = stored.installation.as_ref().ok_or("缺少卸载目标")?;
            if after
                .installations
                .iter()
                .any(|i| i.tool_id == old.tool_id && i.path == old.path && i.version == old.version)
            {
                return Err("管理器返回成功，但所选安装路径仍然可用".into());
            }
            if Path::new(&old.path).is_file() {
                return Err(
                    "管理器返回成功，但所选可执行文件仍存在，请检查其他来源或重启要求".into(),
                );
            }
            return Ok(());
        }
        if provider == "winget" {
            let tool = after
                .definitions
                .iter()
                .find(|t| t.id == request.tool_id)
                .ok_or("未知工具")?;
            let package = tool
                .packages
                .iter()
                .find(|p| p.provider == "winget")
                .ok_or("无 WinGet 映射")?;
            let out = self
                .provider_command(
                    after,
                    "winget",
                    vec![
                        "list".into(),
                        "--id".into(),
                        package.package.clone(),
                        "--exact".into(),
                        "--source".into(),
                        "winget".into(),
                        "--disable-interactivity".into(),
                    ],
                )
                .await?;
            if !out
                .checked()?
                .split_whitespace()
                .any(|s| s == package.package)
            {
                return Err("WinGet 返回成功，但未找到对应软件清单".into());
            }
            let candidates = after
                .installations
                .iter()
                .filter(|i| {
                    i.tool_id == request.tool_id
                        && i.status == "ready"
                        && (request
                            .version
                            .as_ref()
                            .is_none_or(|v| i.version.as_ref() == Some(v)))
                })
                .filter(|i| {
                    !before
                        .installations
                        .iter()
                        .any(|old| old.path == i.path && old.version == i.version)
                })
                .cloned()
                .collect::<Vec<_>>();
            if candidates.len() != 1 {
                return Err("WinGet 清单已匹配，但无法唯一核实新增或升级后的可执行文件归属，请重新扫描并人工核实".into());
            }
            let mut receipt = candidates[0].clone();
            receipt.provider = Some("winget".into());
            receipt.package = Some(package.package.clone());
            receipt.evidence = "工具箱 WinGet 操作前后清单及唯一文件变化核实".into();
            receipt.id = installation_id(
                &receipt.tool_id,
                &receipt.path,
                Some("winget"),
                receipt.version.as_deref(),
            );
            let mut receipts: Vec<ToolInstallation> = self.load("receipts")?.unwrap_or_default();
            receipts.retain(|r| r.path != receipt.path);
            receipts.push(receipt.clone());
            self.save("receipts", &receipts)?;
            if let Some(item) = after
                .installations
                .iter_mut()
                .find(|i| i.path == receipt.path && i.tool_id == receipt.tool_id)
            {
                *item = receipt;
            }
        } else {
            let target = after
                .installations
                .iter()
                .find(|i| {
                    i.tool_id == request.tool_id
                        && i.provider.as_deref() == Some(provider)
                        && i.status == "ready"
                        && stored
                            .installation
                            .as_ref()
                            .is_none_or(|old| i.path == old.path)
                })
                .ok_or("管理器返回成功，但没有可核实来源的目标安装实例")?;
            if request.action == "upgrade"
                && stored
                    .installation
                    .as_ref()
                    .is_some_and(|i| i.version == target.version)
            {
                return Err("升级命令返回成功，但版本未变化（可能已是最新）".into());
            }
        }
        Ok(())
    }
}
