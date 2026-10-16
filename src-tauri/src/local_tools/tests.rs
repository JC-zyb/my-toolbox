use super::*;

#[test]
fn parses_real_fnm_and_rejects_unrecognised_output() {
    let versions = providers::parse_fnm(
        "\u{1b}[32m* v24.16.0 default, lts-latest\u{1b}[0m\n* system\n  v22.12.0\n",
    )
    .unwrap();
    assert_eq!(versions.len(), 2);
    assert!(versions[0].default);
    assert!(providers::parse_fnm("warning: unsupported format").is_err());
}

#[test]
fn parses_uv_mise_rustup_and_java_stderr() {
    let uv=providers::parse_uv(r#"[{"key":"cpython-3.14.7-linux-x86_64-gnu","version":"3.14.7","path":"/home/me/uv/python/bin/python3","extra":"future"},{"version":"3.13.1","path":null}]"#).unwrap();
    assert!(uv[0].installed);
    assert!(!uv[1].installed);
    assert_eq!(
        uv[0].identity.as_deref(),
        Some("cpython-3.14.7-linux-x86_64-gnu")
    );
    let mise=providers::parse_mise(r#"{"java":[{"version":"temurin-25.0.1","install_path":"/mise/java/25","installed":true,"active":false}]}"#,"java").unwrap();
    assert!(!mise[0].default);
    assert_eq!(mise[0].version, "temurin-25.0.1");
    assert!(providers::parse_mise("{}", "java").unwrap().is_empty());
    assert!(providers::parse_mise("{\"java\": [{}]}", "java").is_err());
    let rust=providers::parse_rustup("stable-x86_64-pc-windows-msvc (active, default) C:\\Users\\中文 用户\\.rustup\\toolchains\\stable-x86_64-pc-windows-msvc\n").unwrap();
    assert!(rust[0].default);
    assert!(rust[0].path.as_ref().unwrap().contains("中文 用户"));
    assert_eq!(
        providers::version_from_output("java", "", "openjdk version \"25.0.1\" 2025-10-21\n")
            .unwrap(),
        "25.0.1"
    );
}

#[test]
fn package_receipts_and_command_arguments_are_explicit() {
    let scoop = providers::parse_scoop(
        r#"{"apps":[{"Name":"fnm","Version":"1.39.0","Source":"main"}]}"#,
        Path::new("/scoop"),
    )
    .unwrap();
    assert_eq!(scoop[0].package, "main/fnm");
    let brew = providers::parse_brew(
        r#"{"formulae":[{"name":"fnm","full_name":"fnm","installed":[{"version":"1.39.0"}]}]}"#,
        Path::new("/Cellar"),
    )
    .unwrap();
    assert_eq!(brew[0].root, PathBuf::from("/Cellar/fnm/1.39.0"));
    assert!(providers::validate_token("1.0; Remove-Item").is_err());
    assert!(providers::validate_token("--all").is_err());
    assert_eq!(
        providers::command_args("fnm", "set-default", "node", Some("24.16.0")).unwrap(),
        ["default", "24.16.0"]
    );
    assert_eq!(
        providers::command_args("fnm", "upgrade", "node", Some("24.17.0")).unwrap(),
        ["install", "24.17.0"]
    );
    assert_eq!(
        providers::command_args("uv", "upgrade", "python", Some("3.14.7")).unwrap(),
        ["python", "install", "3.14.7"]
    );
    assert!(providers::command_args("brew", "install", "fnm", Some("1.0")).is_err());
    assert!(providers::command_args("cargo", "upgrade", "unknown", None).is_err());
}

#[test]
fn project_conflicts_backups_and_stale_previews() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    std::fs::write(root.join(".node-version"), "22.0.0\n").unwrap();
    std::fs::write(root.join(".nvmrc"), "v20.0.0\n").unwrap();
    assert!(projects::preview(&root, "node", "24.0.0").is_err());
    std::fs::write(root.join(".nvmrc"), "v22.0.0\n").unwrap();
    let preview = projects::preview(&root, "node", "24.0.0").unwrap();
    assert_eq!(preview.len(), 2);
    let backups = projects::apply(&preview).unwrap();
    assert_eq!(backups.len(), 2);
    assert_eq!(std::fs::read_to_string(&backups[0]).unwrap(), "22.0.0\n");
    assert!(projects::apply(&preview).is_err());
    assert_eq!(
        std::fs::read_to_string(root.join(".nvmrc")).unwrap(),
        "24.0.0\n"
    );
}

#[test]
fn project_toml_preserves_unrelated_fields_and_legacy_files() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    std::fs::write(
        root.join("rust-toolchain.toml"),
        "# pinned\n[toolchain]\nchannel = \"stable\"\ncomponents = [\"clippy\"]\n",
    )
    .unwrap();
    let plan = projects::preview(&root, "rust", "1.90.0").unwrap();
    assert!(plan[0].after.contains("clippy"));
    assert!(plan[0].after.contains("# pinned"));
    std::fs::write(
        root.join(".tool-versions"),
        "node 22.0.0\njava temurin-21 # JDK\n",
    )
    .unwrap();
    let plan = projects::preview(&root, "java", "temurin-25").unwrap();
    assert!(plan[0].after.contains("node 22.0.0"));
    assert!(plan[0].after.contains("java temurin-25 # JDK"));
    std::fs::write(root.join("mise.toml"), "[tools]\njava = \"21\"\n").unwrap();
    assert!(projects::preview(&root, "java", "25").is_err());
}

#[test]
fn mise_inline_options_survive_version_changes() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    std::fs::write(
        root.join("mise.toml"),
        "[tools]\njava = { version = 'temurin-21', arch = 'x64' }\n",
    )
    .unwrap();
    let plan = projects::preview(&root, "java", "temurin-25").unwrap();
    assert!(plan[0].after.contains("arch = 'x64'"));
    projects::apply(&plan).unwrap();
    assert_eq!(
        projects::inspect(&root, "java").unwrap().1.as_deref(),
        Some("temurin-25")
    );
}

#[test]
fn missing_configs_are_created_and_duplicate_formats_fail_explicitly() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    let rust = projects::preview(&root, "rust", "stable").unwrap();
    projects::apply(&rust).unwrap();
    assert_eq!(
        projects::inspect(&root, "rust").unwrap().1.as_deref(),
        Some("stable")
    );
    let mise = projects::preview(&root, "bun", "1.3.0").unwrap();
    projects::apply(&mise).unwrap();
    assert_eq!(
        projects::inspect(&root, "bun").unwrap().1.as_deref(),
        Some("1.3.0")
    );
    std::fs::write(root.join("rust-toolchain"), "stable\n").unwrap();
    assert!(projects::preview(&root, "rust", "beta")
        .unwrap_err()
        .contains("同时存在"));
    assert!(providers::parse_fnm("").unwrap().is_empty());
    assert!(providers::parse_uv("[]").unwrap().is_empty());
    assert!(providers::parse_uv("not json").is_err());
}

#[tokio::test]
async fn offline_metadata_keeps_the_last_successful_document() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().join("home");
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    drop(listener);
    let service = LocalToolsService::isolated(
        dir.path().join("data"),
        home,
        Arc::new(SystemRunner),
        vec![],
        Some(format!("http://{address}")),
    )
    .unwrap();
    let original = ToolMetadata {
        tool_id: "uv".into(),
        description: "cached description".into(),
        fetched_at: "2026-10-01T00:00:00Z".into(),
        latest_release: Some("v0.12.0".into()),
        ..Default::default()
    };
    service.save("metadata", &vec![original.clone()]).unwrap();
    let refreshed = service.refresh_metadata("uv".into()).await.unwrap();
    assert_eq!(refreshed.fetched_at, original.fetched_at);
    assert_eq!(refreshed.latest_release, original.latest_release);
    assert_eq!(refreshed.description, original.description);
    assert!(refreshed.error.is_some());
    assert!(service.search("--option".into()).await.is_err());
}

#[tokio::test]
async fn expired_previews_and_unknown_sources_do_not_execute() {
    let (_root, service, _runner) = isolated_manager();
    let mut snapshot = service.scan().await.unwrap();
    let plan = service.preview(install_request("24.17.0")).await.unwrap();
    service
        .plans
        .lock()
        .unwrap()
        .get_mut(&plan.id)
        .unwrap()
        .plan
        .created_at = (chrono::Utc::now() - chrono::Duration::minutes(16)).to_rfc3339();
    assert!(service.execute(plan.id).unwrap_err().contains("15 分钟"));
    assert!(service.operations().unwrap().is_empty());
    let item = snapshot
        .installations
        .iter_mut()
        .find(|i| i.tool_id == "node")
        .unwrap();
    item.provider = None;
    let id = item.id.clone();
    service.save("snapshot", &snapshot).unwrap();
    let mut request = install_request("24.16.0");
    request.action = "uninstall".into();
    request.installation_id = Some(id);
    assert!(service
        .preview(request)
        .await
        .unwrap_err()
        .contains("来源尚未核实"));
}

#[test]
fn repository_input_cannot_escape_github() {
    assert_eq!(
        metadata::github_repository("https://github.com/astral-sh/uv.git/").unwrap(),
        "astral-sh/uv"
    );
    assert!(metadata::github_repository("https://example.org/a/b").is_err());
    assert!(metadata::github_repository("../repo").is_err());
}

#[test]
fn restart_marks_unfinished_operations_and_keeps_cached_snapshot() {
    let dir = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let service = LocalToolsService::new(dir.path().into(), home.path().into()).unwrap();
    let operation = ToolOperation {
        id: "pending".into(),
        tool_id: "node".into(),
        action: "install".into(),
        provider: "fnm".into(),
        status: "running".into(),
        started_at: now(),
        finished_at: None,
        output: String::new(),
        exit_code: None,
        backup_paths: vec![],
    };
    service.save_operation(&operation).unwrap();
    drop(service);
    let service = LocalToolsService::new(dir.path().into(), home.path().into()).unwrap();
    assert_eq!(service.operations().unwrap()[0].status, "interrupted");
    assert!(service.snapshot().unwrap().scanned_at.is_none());
}

fn executable(path: &Path) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, "isolated fixture").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
}

#[cfg(windows)]
struct WingetRunner {
    bin: PathBuf,
    installed: Mutex<bool>,
}
#[cfg(windows)]
impl CommandRunner for WingetRunner {
    fn run<'a>(
        &'a self,
        invocation: &'a Invocation,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<ProcessOutput, String>> + Send + 'a>,
    > {
        Box::pin(async move {
            let name = invocation.executable.file_stem().unwrap().to_string_lossy();
            let args = invocation
                .args
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>();
            let mut installed = self.installed.lock().unwrap();
            let stdout = match (name.as_ref(), args.first().copied()) {
                ("winget", Some("--version")) => "v1.12.0".into(),
                ("winget", Some("install")) => {
                    executable(&self.bin.join("git.exe"));
                    *installed = true;
                    "installed".into()
                }
                ("winget", Some("list")) if *installed => "Git.Git 2.56.0".into(),
                ("winget", Some("uninstall")) => {
                    assert!(args.windows(2).any(|pair| pair == ["--version", "2.56.0"]));
                    std::fs::remove_file(self.bin.join("git.exe")).map_err(|e| e.to_string())?;
                    *installed = false;
                    "uninstalled".into()
                }
                ("git", Some("--version")) if *installed => "git version 2.56.0".into(),
                _ => return Err(format!("隔离测试禁止命令 {name} {args:?}")),
            };
            Ok(ProcessOutput {
                code: Some(0),
                stdout,
                stderr: String::new(),
            })
        })
    }
}

#[cfg(windows)]
#[tokio::test]
async fn winget_receipt_survives_rescan_and_targets_the_selected_version() {
    let root = tempfile::tempdir().unwrap();
    let bin = root.path().join("bin");
    executable(&bin.join("winget.exe"));
    let service = Arc::new(
        LocalToolsService::isolated(
            root.path().join("data"),
            root.path().join("home"),
            Arc::new(WingetRunner {
                bin: bin.clone(),
                installed: Mutex::new(false),
            }),
            vec![bin],
            None,
        )
        .unwrap(),
    );
    service.scan().await.unwrap();
    let request = ToolOperationRequest {
        tool_id: "git".into(),
        action: "install".into(),
        installation_id: None,
        provider: Some("winget".into()),
        version: None,
        project_path: None,
    };
    let plan = service.preview(request.clone()).await.unwrap();
    service.execute(plan.id.clone()).unwrap();
    assert_eq!(finished(&service, &plan.id).await.status, "success");
    let before = service
        .snapshot()
        .unwrap()
        .installations
        .into_iter()
        .find(|i| i.tool_id == "git")
        .unwrap();
    let after = service
        .scan()
        .await
        .unwrap()
        .installations
        .into_iter()
        .find(|i| i.tool_id == "git")
        .unwrap();
    assert_eq!(before, after);
    let plan = service
        .preview(ToolOperationRequest {
            action: "uninstall".into(),
            installation_id: Some(before.id),
            ..request
        })
        .await
        .unwrap();
    service.execute(plan.id.clone()).unwrap();
    let operation = finished(&service, &plan.id).await;
    assert_eq!(operation.status, "success", "{}", operation.output);
}
fn fake_node(home: &Path, version: &str) -> PathBuf {
    home.join(".local/share/fnm/node-versions")
        .join(format!("v{version}"))
        .join("installation")
        .join(if cfg!(windows) {
            "node.exe"
        } else {
            "bin/node"
        })
}
struct ManagerState {
    versions: Vec<String>,
    default: String,
    fail: bool,
    lie: bool,
}
struct TestRunner {
    home: PathBuf,
    state: Mutex<ManagerState>,
    active: std::sync::atomic::AtomicUsize,
    max_active: std::sync::atomic::AtomicUsize,
}
impl CommandRunner for TestRunner {
    fn run<'a>(
        &'a self,
        invocation: &'a Invocation,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<ProcessOutput, String>> + Send + 'a>,
    > {
        Box::pin(async move {
            let args = invocation
                .args
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>();
            let name = invocation.executable.file_stem().unwrap().to_string_lossy();
            let mut output = ProcessOutput {
                code: Some(0),
                ..Default::default()
            };
            if name == "fnm" {
                if args.first() == Some(&"install") {
                    use std::sync::atomic::Ordering::SeqCst;
                    let active = self.active.fetch_add(1, SeqCst) + 1;
                    self.max_active.fetch_max(active, SeqCst);
                    tokio::time::sleep(Duration::from_millis(30)).await;
                    self.active.fetch_sub(1, SeqCst);
                }
                let mut state = self.state.lock().unwrap();
                output.stdout = match args.as_slice() {
                    ["--version"] => "fnm 1.39.0\n".into(),
                    ["list"] => state
                        .versions
                        .iter()
                        .map(|v| {
                            format!(
                                "* v{v} {}\n",
                                if &state.default == v { "default" } else { "" }
                            )
                        })
                        .collect(),
                    ["list-remote"] => "v22.12.0\nv24.16.0\nv24.17.0\n".into(),
                    ["default", v] => {
                        state.default = v.to_string();
                        format!("{v}\n")
                    }
                    ["install", v, "--progress", "never"] => {
                        if state.fail {
                            output.code = Some(5);
                            output.stderr = "下载失败".into();
                            String::new()
                        } else if state.lie {
                            "reported success without files".into()
                        } else {
                            executable(&fake_node(&self.home, v));
                            state.versions.push(v.to_string());
                            format!("installed {v}")
                        }
                    }
                    _ => return Err(format!("未实现测试命令 {args:?}")),
                };
            } else if name == "node" {
                output.stdout = "v24.16.0\n".into();
            } else {
                return Err("测试禁止访问真实工具".into());
            }
            Ok(output)
        })
    }
}
fn isolated_manager() -> (tempfile::TempDir, Arc<LocalToolsService>, Arc<TestRunner>) {
    let root = tempfile::tempdir().unwrap();
    let home = root.path().join("home");
    let bin = home.join("bin");
    executable(&bin.join(if cfg!(windows) { "fnm.exe" } else { "fnm" }));
    executable(&fake_node(&home, "24.16.0"));
    let runner = Arc::new(TestRunner {
        home: home.clone(),
        state: Mutex::new(ManagerState {
            versions: vec!["24.16.0".into()],
            default: "24.16.0".into(),
            fail: false,
            lie: false,
        }),
        active: std::sync::atomic::AtomicUsize::new(0),
        max_active: std::sync::atomic::AtomicUsize::new(0),
    });
    let service = Arc::new(
        LocalToolsService::isolated(
            root.path().join("data"),
            home,
            runner.clone(),
            vec![bin],
            None,
        )
        .unwrap(),
    );
    (root, service, runner)
}
fn install_request(version: &str) -> ToolOperationRequest {
    ToolOperationRequest {
        tool_id: "node".into(),
        action: "install".into(),
        installation_id: None,
        provider: Some("fnm".into()),
        version: Some(version.into()),
        project_path: None,
    }
}
async fn finished(service: &LocalToolsService, id: &str) -> ToolOperation {
    for _ in 0..200 {
        let operation = service
            .operations()
            .unwrap()
            .into_iter()
            .find(|o| o.id == id)
            .unwrap();
        if !matches!(operation.status.as_str(), "queued" | "running") {
            return operation;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("操作没有在期限内结束");
}

#[tokio::test]
async fn discovers_manager_versions_outside_path_and_serialises_operations() {
    let (_root, service, runner) = isolated_manager();
    let snapshot = service.scan().await.unwrap();
    assert!(snapshot
        .installations
        .iter()
        .any(|i| i.tool_id == "node" && i.provider.as_deref() == Some("fnm")));
    assert!(!snapshot
        .installations
        .iter()
        .any(|i| i.tool_id == "node" && i.resolved));
    let first = service.preview(install_request("24.17.0")).await.unwrap();
    let second = service.preview(install_request("22.12.0")).await.unwrap();
    service.execute(first.id.clone()).unwrap();
    service.execute(second.id.clone()).unwrap();
    assert_eq!(finished(&service, &first.id).await.status, "success");
    assert_eq!(finished(&service, &second.id).await.status, "success");
    assert_eq!(
        runner.max_active.load(std::sync::atomic::Ordering::SeqCst),
        1
    );
    assert!(service.execute(first.id).is_err());
    assert!(fake_node(&runner.home, "22.12.0").is_file());
}

#[tokio::test]
async fn manager_errors_and_false_success_are_reported() {
    for lie in [false, true] {
        let (_root, service, runner) = isolated_manager();
        service.scan().await.unwrap();
        {
            let mut state = runner.state.lock().unwrap();
            state.fail = !lie;
            state.lie = lie;
        }
        let plan = service.preview(install_request("24.17.0")).await.unwrap();
        service.execute(plan.id.clone()).unwrap();
        let operation = finished(&service, &plan.id).await;
        assert_eq!(operation.status, "failed");
        if lie {
            assert!(operation.output.contains("未找到目标版本安装"));
        } else {
            assert_eq!(operation.exit_code, Some(5));
            assert!(operation.output.contains("下载失败"));
        }
    }
}

#[tokio::test]
async fn rejects_wrong_source_and_changed_provider_before_execution() {
    let (_root, service, _runner) = isolated_manager();
    let snapshot = service.scan().await.unwrap();
    let item = snapshot
        .installations
        .iter()
        .find(|i| i.tool_id == "node")
        .unwrap();
    let mut request = install_request("24.16.0");
    request.action = "uninstall".into();
    request.installation_id = Some(item.id.clone());
    request.provider = Some("winget".into());
    assert!(service
        .preview(request)
        .await
        .unwrap_err()
        .contains("不一致"));
    let plan = service.preview(install_request("24.17.0")).await.unwrap();
    std::fs::write(plan.executable.as_ref().unwrap(), "changed manager binary").unwrap();
    service.execute(plan.id.clone()).unwrap();
    assert!(finished(&service, &plan.id).await.output.contains("已变化"));
}

#[tokio::test]
async fn project_operations_do_not_require_or_execute_a_manager() {
    let (root, service, _runner) = isolated_manager();
    let project = root.path().join("中文 项目");
    std::fs::create_dir(&project).unwrap();
    std::fs::write(project.join(".python-version"), "3.11.9\n").unwrap();
    let request = ToolOperationRequest {
        tool_id: "python".into(),
        action: "pin-project".into(),
        installation_id: None,
        provider: None,
        version: Some("3.14.7".into()),
        project_path: Some(path_text(&project)),
    };
    let plan = service.preview(request.clone()).await.unwrap();
    std::fs::write(project.join(".python-version"), "3.12.0\n").unwrap();
    service.execute(plan.id.clone()).unwrap();
    assert_eq!(finished(&service, &plan.id).await.status, "failed");
    let plan = service.preview(request).await.unwrap();
    service.execute(plan.id.clone()).unwrap();
    let operation = finished(&service, &plan.id).await;
    assert_eq!(operation.status, "success");
    assert_eq!(operation.backup_paths.len(), 1);
    assert_eq!(
        std::fs::read_to_string(project.join(".python-version")).unwrap(),
        "3.14.7\n"
    );
}

#[test]
fn update_comparison_keeps_version_families_separate() {
    assert!(updates::newer_same_series("24.16.0", "24.17.0"));
    assert!(!updates::newer_same_series("24.16.0", "25.0.0"));
    assert!(updates::newer_same_series(
        "temurin-25.0.0",
        "temurin-25.0.1"
    ));
    assert!(!updates::newer_same_series(
        "temurin-25.0.0",
        "openjdk-25.0.1"
    ));
}

#[tokio::test]
async fn real_process_runner_exposes_stderr_exit_and_timeout() {
    let root = tempfile::tempdir().unwrap();
    let (path, body) = if cfg!(windows) {
        (
            root.path().join("检查 script.ps1"),
            "[Console]::Error.WriteLine('expected failure'); exit 7",
        )
    } else {
        (
            root.path().join("script"),
            "#!/bin/sh\necho 'expected failure' >&2\nexit 7\n",
        )
    };
    executable(&path);
    std::fs::write(&path, body).unwrap();
    let invocation = Invocation {
        executable: path.clone(),
        args: vec![],
        cwd: root.path().into(),
        timeout: Duration::from_secs(5),
        env: vec![],
        remove_env: vec![],
        output_path: None,
    };
    let output = SystemRunner.run(&invocation).await.unwrap();
    assert_eq!(output.code, Some(7));
    assert!(output.stderr.contains("expected failure"));
    std::fs::write(
        &path,
        if cfg!(windows) {
            "Start-Sleep -Seconds 10"
        } else {
            "#!/bin/sh\nsleep 10\n"
        },
    )
    .unwrap();
    let invocation = Invocation {
        timeout: Duration::from_millis(300),
        ..invocation
    };
    assert!(SystemRunner
        .run(&invocation)
        .await
        .unwrap_err()
        .contains("超时"));
}

#[cfg(windows)]
#[tokio::test]
async fn batch_runner_quotes_unicode_paths_and_arguments() {
    let root = tempfile::tempdir().unwrap();
    let dir = root.path().join("中文 工具");
    std::fs::create_dir(&dir).unwrap();
    let path = dir.join("test.cmd");
    std::fs::write(&path, "@echo off\r\necho %~1\r\nexit /b 0\r\n").unwrap();
    let invocation = Invocation {
        executable: path,
        args: vec!["two words".into()],
        cwd: root.path().into(),
        timeout: Duration::from_secs(5),
        env: vec![],
        remove_env: vec![],
        output_path: None,
    };
    let output = SystemRunner.run(&invocation).await.unwrap();
    assert_eq!(output.code, Some(0), "{}", output.combined());
    assert_eq!(output.stdout.trim(), "two words");
}
