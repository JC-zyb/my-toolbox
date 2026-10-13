use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenameRow {
    pub from: String,
    pub to: String,
    pub error: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenamePlan {
    pub id: String,
    pub rows: Vec<RenameRow>,
}

pub fn preview(
    directory: &Path,
    find: &str,
    replace: &str,
    prefix: &str,
    suffix: &str,
) -> Result<RenamePlan, String> {
    let root = directory.canonicalize().map_err(|e| e.to_string())?;
    if !root.is_dir() {
        return Err("请选择目录".into());
    }
    if [replace, prefix, suffix]
        .iter()
        .any(|s| s.contains(['/', '\\', ':', '*', '?', '"', '<', '>', '|']))
    {
        return Err("名称包含 Windows 不允许的字符".into());
    }
    let mut paths: Vec<PathBuf> = std::fs::read_dir(&root)
        .map_err(|e| e.to_string())?
        .map(|e| e.map(|e| e.path()).map_err(|e| e.to_string()))
        .collect::<Result<_, _>>()?;
    paths.sort();
    let mut used = HashSet::new();
    let mut rows = vec![];
    for path in paths {
        if !path.is_file() || path.is_symlink() {
            continue;
        }
        let stem = path.file_stem().ok_or("文件名无效")?.to_string_lossy();
        let name = if find.is_empty() {
            stem.to_string()
        } else {
            stem.replace(find, replace)
        };
        let mut target = format!("{prefix}{name}{suffix}");
        if let Some(extension) = path.extension() {
            target.push('.');
            target.push_str(&extension.to_string_lossy());
        }
        let to = root.join(&target);
        let error = if target.is_empty() || target.ends_with(['.', ' ']) || name.is_empty() {
            Some("文件名无效".into())
        } else if !used.insert(target.to_lowercase()) {
            Some("目标名称重复".into())
        } else if to != path && to.exists() {
            Some("目标文件已存在".into())
        } else {
            None
        };
        rows.push(RenameRow {
            from: path.to_string_lossy().into_owned(),
            to: to.to_string_lossy().into_owned(),
            error,
        });
    }
    Ok(RenamePlan {
        id: uuid::Uuid::new_v4().to_string(),
        rows,
    })
}

// 执行前再次校验目标，防止预览后发生覆盖；失败时逐项回滚并报告回滚结果。
pub fn apply(rows: &[RenameRow]) -> Result<usize, String> {
    let mut targets = HashSet::new();
    for row in rows {
        if row.error.is_some() {
            return Err("预览存在冲突，请先修正".into());
        }
        let from = Path::new(&row.from);
        let to = Path::new(&row.to);
        if from.parent() != to.parent() || !from.is_file() || from.is_symlink() {
            return Err("文件位置已改变或不在同一目录".into());
        }
        if !targets.insert(row.to.to_lowercase()) || from != to && to.exists() {
            return Err("目标名称重复或已存在".into());
        }
    }
    let mut completed: Vec<&RenameRow> = vec![];
    for row in rows.iter().filter(|r| r.from != r.to) {
        if let Err(error) = std::fs::rename(&row.from, &row.to) {
            let mut failures = vec![];
            for done in completed.iter().rev() {
                if let Err(e) = std::fs::rename(&done.to, &done.from) {
                    failures.push(e.to_string());
                }
            }
            return Err(format!(
                "改名失败：{error}；回滚{}",
                if failures.is_empty() {
                    "完成".into()
                } else {
                    format!("失败：{}", failures.join("；"))
                }
            ));
        }
        completed.push(row);
    }
    Ok(completed.len())
}

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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rename_preserves_extension_and_rejects_overwrite() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.txt"), "a").unwrap();
        std::fs::write(dir.path().join("b.txt"), "b").unwrap();
        let p = preview(dir.path(), "a", "b", "", "").unwrap();
        assert!(p.rows.iter().any(|r| r.error.is_some()));
        assert!(apply(&p.rows).is_err());
        assert_eq!(
            std::fs::read_to_string(dir.path().join("a.txt")).unwrap(),
            "a"
        );
    }
    #[test]
    fn rename_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.txt"), "a").unwrap();
        let p = preview(dir.path(), "", "", "new_", "").unwrap();
        assert_eq!(apply(&p.rows).unwrap(), 1);
        assert!(dir.path().join("new_a.txt").exists());
    }
}
