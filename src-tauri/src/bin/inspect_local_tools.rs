// 只读诊断将缓存写入新临时目录，不触发安装、升级、默认值或项目配置修改。
use toolbox_core::local_tools::LocalToolsService;
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let directory =
        std::env::temp_dir().join(format!("toolbox-tools-inspect-{}", uuid::Uuid::new_v4()));
    let home = std::env::var("USERPROFILE").or_else(|_| std::env::var("HOME"))?;
    let service = LocalToolsService::new(directory.clone(), home.into())?;
    let snapshot = service.scan().await?;
    println!("{}", serde_json::to_string_pretty(&snapshot)?);
    eprintln!("诊断缓存：{}", directory.display());
    Ok(())
}
