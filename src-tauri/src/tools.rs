use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScriptResult {
    pub id: String,
    pub status: String,
    pub output: String,
    pub exit_code: Option<i32>,
    pub started_at: String,
    pub finished_at: Option<String>,
}

pub async fn run(script: &crate::models::Script) -> Result<ScriptResult, String> {
    use tokio::io::AsyncReadExt;
    if !(1..=3600).contains(&script.timeout_seconds) {
        return Err("超时时间须在 1–3600 秒之间".into());
    }
    if !Path::new(&script.path).is_file() {
        return Err("脚本文件不存在".into());
    }
    let mut cmd = tokio::process::Command::new(&script.interpreter);
    if Path::new(&script.path)
        .extension()
        .is_some_and(|s| s.eq_ignore_ascii_case("ps1"))
    {
        cmd.args(["-NoProfile", "-NonInteractive", "-File"]);
    }
    cmd.arg(&script.path)
        .args(&script.args)
        .kill_on_drop(true)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    if !script.cwd.is_empty() {
        cmd.current_dir(&script.cwd);
    }
    #[cfg(windows)]
    cmd.creation_flags(0x08000000);
    let started = chrono::Utc::now().to_rfc3339();
    let mut child = cmd.spawn().map_err(|e| format!("无法启动解释器：{e}"))?;
    let stdout = child.stdout.take().ok_or("无法读取标准输出")?;
    let stderr = child.stderr.take().ok_or("无法读取标准错误")?;
    // 同时排空输出，最多保留 1 MiB，避免阻塞管道或无限增长。
    async fn capture(mut stream: impl tokio::io::AsyncRead + Unpin) -> Result<Vec<u8>, String> {
        let mut kept = Vec::new();
        let mut buffer = [0u8; 8192];
        loop {
            let n = stream.read(&mut buffer).await.map_err(|e| e.to_string())?;
            if n == 0 {
                break;
            }
            let available = (1024 * 1024usize).saturating_sub(kept.len());
            kept.extend_from_slice(&buffer[..n.min(available)]);
        }
        Ok(kept)
    }
    let out = tokio::spawn(capture(stdout));
    let err = tokio::spawn(capture(stderr));
    let status = match tokio::time::timeout(
        std::time::Duration::from_secs(script.timeout_seconds),
        child.wait(),
    )
    .await
    {
        Ok(status) => status.map_err(|e| e.to_string())?,
        Err(_) => {
            child.kill().await.map_err(|e| e.to_string())?;
            out.abort();
            err.abort();
            return Err("脚本已超时并终止；请检查脚本是否启动了子进程".into());
        }
    };
    let out = out.await.map_err(|e| e.to_string())??;
    let err = err.await.map_err(|e| e.to_string())??;
    Ok(ScriptResult {
        id: script.id.clone(),
        status: if status.success() {
            "success"
        } else {
            "failed"
        }
        .into(),
        output: format!(
            "{}{}",
            String::from_utf8_lossy(&out),
            String::from_utf8_lossy(&err)
        ),
        exit_code: status.code(),
        started_at: started,
        finished_at: Some(chrono::Utc::now().to_rfc3339()),
    })
}
