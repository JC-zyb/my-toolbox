use std::{
    future::Future,
    path::{Path, PathBuf},
    pin::Pin,
    process::Stdio,
    time::Duration,
};
use tokio::io::AsyncReadExt;

pub const OUTPUT_LIMIT: usize = 1024 * 1024;

#[derive(Debug, Clone)]
pub struct Invocation {
    pub executable: PathBuf,
    pub args: Vec<String>,
    pub cwd: PathBuf,
    pub timeout: Duration,
    pub env: Vec<(String, String)>,
    pub remove_env: Vec<String>,
    pub output_path: Option<PathBuf>,
}

#[derive(Debug, Clone, Default)]
pub struct ProcessOutput {
    pub code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}
impl ProcessOutput {
    pub fn checked(&self) -> Result<&str, String> {
        if self.code != Some(0) {
            return Err(format!(
                "命令失败（退出码 {:?}）：{}",
                self.code,
                self.stderr.trim().chars().take(1200).collect::<String>()
            ));
        }
        Ok(&self.stdout)
    }
    pub fn combined(&self) -> String {
        format!("{}{}", self.stdout, self.stderr)
    }
}

pub trait CommandRunner: Send + Sync {
    fn run<'a>(
        &'a self,
        invocation: &'a Invocation,
    ) -> Pin<Box<dyn Future<Output = Result<ProcessOutput, String>> + Send + 'a>>;
}

pub struct SystemRunner;

async fn capture(
    mut stream: impl tokio::io::AsyncRead + Unpin,
    output_path: Option<PathBuf>,
) -> Result<String, String> {
    use std::io::Write;
    let mut log = output_path
        .map(|p| {
            std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(p)
                .map_err(|e| e.to_string())
        })
        .transpose()?;
    let mut bytes = Vec::new();
    let mut buffer = [0u8; 8192];
    let mut truncated = false;
    loop {
        let n = stream.read(&mut buffer).await.map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        let keep = n.min(OUTPUT_LIMIT.saturating_sub(bytes.len()));
        bytes.extend_from_slice(&buffer[..keep]);
        if let Some(file) = &mut log {
            file.write_all(&buffer[..keep]).map_err(|e| e.to_string())?;
        }
        truncated |= keep < n;
    }
    let mut text = String::from_utf8_lossy(&bytes).into_owned();
    if truncated {
        text.push_str("\n[输出超过 1 MiB，已截断]\n");
    }
    Ok(text)
}

impl CommandRunner for SystemRunner {
    fn run<'a>(
        &'a self,
        invocation: &'a Invocation,
    ) -> Pin<Box<dyn Future<Output = Result<ProcessOutput, String>> + Send + 'a>> {
        Box::pin(async move {
            let mut command = build_command(&invocation.executable, &invocation.args)?;
            command
                .current_dir(&invocation.cwd)
                .envs(invocation.env.iter().cloned())
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .kill_on_drop(true);
            for name in &invocation.remove_env {
                command.env_remove(name);
            }
            #[cfg(windows)]
            command.creation_flags(0x08000000);
            #[cfg(unix)]
            {
                use std::os::unix::process::CommandExt;
                command.as_std_mut().process_group(0);
            }
            let mut child = command
                .spawn()
                .map_err(|e| format!("无法启动 {}：{e}", invocation.executable.display()))?;
            let pid = child.id();
            let mut out = tokio::spawn(capture(
                child.stdout.take().ok_or("缺少标准输出管道")?,
                invocation.output_path.clone(),
            ));
            let mut err = tokio::spawn(capture(
                child.stderr.take().ok_or("缺少标准错误管道")?,
                invocation.output_path.clone(),
            ));
            // 超时覆盖整个读取过程，防止继承管道的子进程使读取永久等待。
            let result = tokio::time::timeout(invocation.timeout, async {
                let status = child.wait().await.map_err(|e| e.to_string())?;
                let stdout = (&mut out).await.map_err(|e| e.to_string())??;
                let stderr = (&mut err).await.map_err(|e| e.to_string())??;
                Ok::<_, String>(ProcessOutput {
                    code: status.code(),
                    stdout,
                    stderr,
                })
            })
            .await;
            match result {
                Ok(output) => output,
                Err(_) => {
                    out.abort();
                    err.abort();
                    let termination = if child.try_wait().map_err(|e| e.to_string())?.is_none() {
                        terminate_tree(pid).await
                    } else {
                        Ok("主进程已退出，已停止等待被派生进程占用的输出管道".into())
                    };
                    if child.try_wait().map_err(|e| e.to_string())?.is_none() {
                        child
                            .kill()
                            .await
                            .map_err(|e| format!("超时且终止失败：{e}"))?;
                    }
                    Err(format!("命令已超时；{}", termination.unwrap_or_else(|e| e)))
                }
            }
        })
    }
}

async fn terminate_tree(pid: Option<u32>) -> Result<String, String> {
    let pid = pid.ok_or("无法获取超时进程 ID")?;
    #[cfg(windows)]
    let mut command = {
        let root = std::env::var_os("SystemRoot").ok_or("缺少 SystemRoot")?;
        let mut c = tokio::process::Command::new(PathBuf::from(root).join("System32/taskkill.exe"));
        c.args(["/PID", &pid.to_string(), "/T", "/F"])
            .creation_flags(0x08000000);
        c
    };
    #[cfg(not(windows))]
    let mut command = {
        let mut c = tokio::process::Command::new("/bin/kill");
        c.args(["-KILL", "--", &format!("-{pid}")]);
        c
    };
    let status = command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await
        .map_err(|e| e.to_string())?;
    if status.success() {
        Ok("已终止进程树".into())
    } else {
        Err(format!("终止进程树返回 {status}；请检查管理器派生进程"))
    }
}

fn build_command(executable: &Path, args: &[String]) -> Result<tokio::process::Command, String> {
    #[cfg(windows)]
    {
        if executable
            .extension()
            .is_some_and(|x| x.eq_ignore_ascii_case("ps1"))
        {
            let root = std::env::var_os("SystemRoot").ok_or("缺少 SystemRoot")?;
            let mut c = tokio::process::Command::new(
                PathBuf::from(root).join("System32/WindowsPowerShell/v1.0/powershell.exe"),
            );
            c.args(["-NoProfile", "-NonInteractive", "-File"])
                .arg(executable)
                .args(args);
            return Ok(c);
        }
        if executable
            .extension()
            .is_some_and(|x| x.eq_ignore_ascii_case("cmd") || x.eq_ignore_ascii_case("bat"))
        {
            if executable
                .to_string_lossy()
                .chars()
                .chain(args.iter().flat_map(|x| x.chars()))
                .any(|c| "&|<>^%!\"\r\n".contains(c))
            {
                return Err("批处理路径或参数包含不支持的 shell 字符".into());
            }
            let root = std::env::var_os("SystemRoot").ok_or("缺少 SystemRoot")?;
            let mut c =
                tokio::process::Command::new(PathBuf::from(root).join("System32").join("cmd.exe"));
            let arguments = args
                .iter()
                .map(|arg| format!("\"{arg}\""))
                .collect::<Vec<_>>()
                .join(" ");
            c.args(["/D", "/S", "/C"]).raw_arg(format!(
                "\"\"{}\" {}\"",
                executable.display(),
                arguments
            ));
            return Ok(c);
        }
    }
    let mut c = tokio::process::Command::new(executable);
    c.args(args);
    Ok(c)
}

pub fn find_commands(name: &str, directories: &[PathBuf]) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    let extensions: &[&str] = if cfg!(windows) {
        &[".exe", ".cmd", ".bat", ".ps1"]
    } else {
        &[""]
    };
    for dir in directories {
        for extension in extensions {
            let path = dir.join(format!("{name}{extension}"));
            if !path.is_file() {
                continue;
            }
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                if path
                    .metadata()
                    .map(|m| m.permissions().mode() & 0o111 == 0)
                    .unwrap_or(true)
                {
                    continue;
                }
            }
            // 保留 shim 的入口路径；真实路径只用于去重，不把不同环境的入口误当实际默认值。
            if !paths.contains(&path) {
                paths.push(path);
            }
        }
    }
    paths
}

pub fn clean_text(value: &str) -> String {
    let mut result = String::new();
    let mut chars = value.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' && chars.peek() == Some(&'[') {
            chars.next();
            for c in chars.by_ref() {
                if c.is_ascii_alphabetic() {
                    break;
                }
            }
        } else if c != '\r' {
            result.push(c);
        }
    }
    result
}
