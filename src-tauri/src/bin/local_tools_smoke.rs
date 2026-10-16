// CI 专用的真实 fnm 闭环：明确的新临时根目录，运行时只安装在该根下。
use std::{path::PathBuf, sync::Arc, time::Duration};
use toolbox_core::local_tools::{process::SystemRunner, *};
async fn perform(
    service: &Arc<LocalToolsService>,
    request: ToolOperationRequest,
) -> Result<(), String> {
    let plan = service.preview(request).await?;
    let id = plan.id.clone();
    service.execute(id.clone())?;
    for _ in 0..1800 {
        let operation = service
            .operations()?
            .into_iter()
            .find(|o| o.id == id)
            .ok_or("操作记录缺失")?;
        if operation.status == "success" {
            return Ok(());
        }
        if !matches!(operation.status.as_str(), "queued" | "running") {
            return Err(operation.output);
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    Err("smoke 操作超过总期限".into())
}
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let root = PathBuf::from(args.next().ok_or("需要新建系统临时子目录")?);
    let fnm = PathBuf::from(args.next().ok_or("需要真实 fnm 绝对路径")?).canonicalize()?;
    if !root.is_absolute()
        || !root.starts_with(std::env::temp_dir())
        || root.exists()
        || !fnm.is_file()
    {
        return Err("smoke 仅允许新建系统临时子目录及真实 fnm 文件".into());
    }
    let home = root.join("home");
    let manager_root = home.join(".local/share/fnm");
    std::fs::create_dir_all(&manager_root)?;
    std::fs::create_dir_all(root.join("project"))?;
    // 仅影响这个隔离子进程，不修改用户或系统环境。
    std::env::set_var("FNM_DIR", &manager_root);
    std::env::remove_var("FNM_MULTISHELL_PATH");
    let service = Arc::new(LocalToolsService::isolated(
        root.join("data"),
        home,
        Arc::new(SystemRunner),
        vec![fnm.parent().ok_or("fnm 缺少父目录")?.into()],
        None,
    )?);
    service.scan().await?;
    let request = ToolOperationRequest {
        tool_id: "node".into(),
        action: "install".into(),
        installation_id: None,
        provider: Some("fnm".into()),
        version: Some("24.16.0".into()),
        project_path: None,
    };
    perform(&service, request.clone()).await?;
    perform(
        &service,
        ToolOperationRequest {
            action: "set-default".into(),
            ..request.clone()
        },
    )
    .await?;
    perform(
        &service,
        ToolOperationRequest {
            action: "pin-project".into(),
            project_path: Some(root.join("project").to_string_lossy().into()),
            ..request.clone()
        },
    )
    .await?;
    let project = service
        .project("node".into(), root.join("project").to_string_lossy().into())
        .await?;
    if project.selected_version.as_deref() != Some("24.16.0") {
        return Err("项目固定结果错误".into());
    }
    let snapshot = service.snapshot()?;
    let installation = snapshot
        .installations
        .iter()
        .find(|i| i.tool_id == "node" && i.provider.as_deref() == Some("fnm"))
        .ok_or("真实安装未被发现")?;
    let output = tokio::process::Command::new(&installation.path)
        .arg("--version")
        .output()
        .await?;
    if !output.status.success() || String::from_utf8_lossy(&output.stdout).trim() != "v24.16.0" {
        return Err("真实 Node 版本不符合目标".into());
    }
    println!(
        "真实 fnm 闭环通过：安装 → 发现 → 默认版本 → 项目固定 → 绝对路径版本校验；沙箱 {}",
        root.display()
    );
    Ok(())
}
