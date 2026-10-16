// UI 测试专用：真实服务/数据库/项目文件，管理器进程仅替换为受控临时沙箱。
use serde_json::{json, Value};
use std::{
    future::Future,
    io::{self, BufRead, Read, Write},
    path::{Path, PathBuf},
    pin::Pin,
    sync::{Arc, Mutex},
};
use toolbox_core::{
    local_tools::{
        process::{CommandRunner, Invocation, ProcessOutput},
        *,
    },
    models::Settings,
};

struct SandboxRunner {
    home: PathBuf,
    versions: Mutex<Vec<String>>,
    default: Mutex<String>,
    fail: Mutex<bool>,
}
fn node_path(home: &Path, version: &str) -> PathBuf {
    home.join(".local/share/fnm/node-versions")
        .join(format!("v{version}"))
        .join("installation")
        .join(if cfg!(windows) {
            "node.exe"
        } else {
            "bin/node"
        })
}
fn create_file(path: &Path) -> Result<(), String> {
    std::fs::create_dir_all(path.parent().ok_or("缺少父目录")?).map_err(|e| e.to_string())?;
    std::fs::write(path, "fixture executable").map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}
impl CommandRunner for SandboxRunner {
    fn run<'a>(
        &'a self,
        invocation: &'a Invocation,
    ) -> Pin<Box<dyn Future<Output = Result<ProcessOutput, String>> + Send + 'a>> {
        Box::pin(async move {
            let executable = invocation
                .executable
                .file_stem()
                .and_then(|p| p.to_str())
                .ok_or("无效命令")?;
            let args = invocation
                .args
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>();
            let stdout;
            if executable == "fnm" {
                match args.as_slice() {
                    ["--version"] => stdout = "fnm 1.39.0\n".into(),
                    ["list"] => {
                        let default = self.default.lock().map_err(|e| e.to_string())?;
                        stdout = self
                            .versions
                            .lock()
                            .map_err(|e| e.to_string())?
                            .iter()
                            .map(|v| {
                                format!("* v{v} {}\n", if *default == *v { "default" } else { "" })
                            })
                            .collect();
                    }
                    ["list-remote"] => stdout = "v22.12.0\nv24.16.0\nv24.17.0\n".into(),
                    ["default", version] => {
                        *self.default.lock().map_err(|e| e.to_string())? = version.to_string();
                        stdout = format!("default -> {version}\n");
                    }
                    ["install", version, "--progress", "never"] => {
                        if std::mem::take(&mut *self.fail.lock().map_err(|e| e.to_string())?) {
                            return Ok(ProcessOutput {
                                code: Some(9),
                                stdout: String::new(),
                                stderr: "fixture: manager download failed\n".into(),
                            });
                        }
                        create_file(&node_path(&self.home, version))?;
                        self.versions
                            .lock()
                            .map_err(|e| e.to_string())?
                            .push(version.to_string());
                        stdout = format!("installed {version}\n");
                    }
                    ["uninstall", version] => {
                        std::fs::remove_file(node_path(&self.home, version))
                            .map_err(|e| e.to_string())?;
                        self.versions
                            .lock()
                            .map_err(|e| e.to_string())?
                            .retain(|v| v != version);
                        stdout = format!("removed {version}\n");
                    }
                    _ => return Err(format!("fixture 未实现 fnm 参数：{args:?}")),
                }
            } else if executable == "node" && args == ["--version"] {
                stdout = if invocation.executable.to_string_lossy().contains("v22.12.0") {
                    "v22.12.0\n"
                } else {
                    "v24.16.0\n"
                }
                .into();
            } else {
                return Err(format!(
                    "fixture 禁止执行非沙箱命令：{}",
                    invocation.executable.display()
                ));
            }
            Ok(ProcessOutput {
                code: Some(0),
                stdout,
                stderr: String::new(),
            })
        })
    }
}

fn metadata_server() -> Result<String, Box<dyn std::error::Error>> {
    let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
    let address = listener.local_addr()?;
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else {
                break;
            };
            let result = (|| -> io::Result<()> {
                stream.set_read_timeout(Some(std::time::Duration::from_secs(5)))?;
                let mut request = [0u8; 4096];
                let n = stream.read(&mut request)?;
                let request = String::from_utf8_lossy(&request[..n]);
                let (status, body) = if request.contains("/releases/latest") {
                    (
                        "200 OK",
                        json!({"tag_name":"v24.17.0","html_url":"https://github.com/nodejs/node/releases/tag/v24.17.0"}),
                    )
                } else if request.contains("/search/repositories") {
                    (
                        "200 OK",
                        json!({"items":[{"full_name":"astral-sh/uv","description":"Python 工具管理","html_url":"https://github.com/astral-sh/uv"}]}),
                    )
                } else {
                    (
                        "200 OK",
                        json!({"full_name":"nodejs/node","description":"沙箱中的官方资料样例","homepage":"https://nodejs.org","license":{"spdx_id":"MIT"}}),
                    )
                };
                let body = body.to_string();
                write!(stream,"HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len())
            })();
            if let Err(error) = result {
                eprintln!("fixture 元数据服务错误：{error}");
            }
        }
    });
    Ok(format!("http://{address}"))
}
fn text<'a>(args: &'a Value, key: &str) -> Result<&'a str, String> {
    args[key].as_str().ok_or_else(|| format!("缺少 {key}"))
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(std::env::args().nth(1).ok_or("需要新临时目录")?);
    if !root.is_absolute() || !root.starts_with(std::env::temp_dir()) || root.exists() {
        return Err("fixture 仅允许不存在的系统临时子目录".into());
    }
    let home = root.join("home");
    let bin = home.join("bin");
    create_file(&bin.join(if cfg!(windows) { "fnm.exe" } else { "fnm" }))?;
    create_file(&node_path(&home, "22.12.0"))?;
    std::fs::create_dir_all(root.join("project"))?;
    std::fs::write(root.join("project/.node-version"), "22.12.0\n")?;
    let runner = Arc::new(SandboxRunner {
        home: home.clone(),
        versions: Mutex::new(vec!["22.12.0".into()]),
        default: Mutex::new("22.12.0".into()),
        fail: Mutex::new(false),
    });
    let service = Arc::new(LocalToolsService::isolated(
        root.join("data"),
        home,
        runner.clone(),
        vec![bin],
        Some(metadata_server()?),
    )?);
    let mut settings = Settings::default();
    settings.sources.clear();
    println!("{}", json!({"ready":true}));
    // stdin 在专用阻塞任务中读取，操作队列可以同时继续推进。
    let (send, mut receive) = tokio::sync::mpsc::unbounded_channel();
    std::thread::spawn(move || {
        for line in io::stdin().lock().lines() {
            if send.send(line).is_err() {
                break;
            }
        }
    });
    while let Some(line) = receive.recv().await {
        let request: Value = serde_json::from_str(&line?)?;
        let args = &request["args"];
        let result=async {Ok::<Value,String>(match text(&request,"name")? {
            "get_settings"=>json!(settings),
            "update_settings"=>{settings=serde_json::from_value(args["settings"].clone()).map_err(|e|e.to_string())?;Value::Null},
            "get_snapshot"=>Value::Null,
            "refresh_usage"=>json!({"records":[],"sources":[],"collectedAt":"2026-10-16T00:00:00Z","dataDirectory":root.join("data")}),
            "list_local_tools"=>json!(service.snapshot()?),
            "scan_local_tools"=>json!(service.scan().await?),
            "search_local_tools"=>json!(service.search(text(args,"query")?.into()).await?),
            "add_local_tool"=>json!(service.register(serde_json::from_value(args["candidate"].clone()).map_err(|e|e.to_string())?).await?),
            "refresh_local_tool_metadata"=>json!(service.refresh_metadata(text(args,"toolId")?.into()).await?),
            "local_tool_versions"=>json!(service.versions(text(args,"toolId")?.into(),args["remote"].as_bool().unwrap_or(false)).await?),
            "local_tool_project"=>json!(service.project(text(args,"toolId")?.into(),text(args,"directory")?.into()).await?),
            "preview_local_tool_operation"=>json!(service.preview(serde_json::from_value(args["request"].clone()).map_err(|e|e.to_string())?).await?),
            "execute_local_tool_operation"=>json!(service.execute(text(args,"id")?.into())?),
            "list_local_tool_operations"=>json!(service.operations()?),
            "set_local_tool_directories"=>{service.set_directories(serde_json::from_value(args["directories"].clone()).map_err(|e|e.to_string())?)?;Value::Null},
            "check_local_tool_updates"=>json!(service.check_updates().await?),
            "fixture_fail_next"=>{*runner.fail.lock().map_err(|e|e.to_string())?=true;Value::Null},
            name=>return Err(format!("未实现 fixture 命令：{name}")),
        })}.await;
        println!(
            "{}",
            match result {
                Ok(value) => json!({"value":value}),
                Err(error) => json!({"error":error}),
            }
        );
    }
    Ok(())
}
