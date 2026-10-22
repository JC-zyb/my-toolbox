use serde_json::json;
use toolbox_core::news::NewsService;

#[tokio::main]
async fn main() -> Result<(), String> {
    let directory =
        std::env::temp_dir().join(format!("toolbox-agent-news-{}", uuid::Uuid::new_v4()));
    let service = NewsService::new(&directory)?;
    let mut snapshot = service.refresh(true).await?;
    let chinese = std::env::args().any(|arg| arg == "--chinese");
    let mut translations = vec![];
    let mut preview = None;
    if chinese {
        let urls: Vec<_> = [
            "openai",
            "deepmind",
            "huggingface",
            "claude-code",
            "qbitai",
            "deerflow",
        ]
        .iter()
        .filter_map(|id| snapshot.sources.iter().find(|source| &source.id == id))
        .filter_map(|source| {
            source
                .items
                .iter()
                .find(|item| item.summary.chars().count() > 50)
        })
        .map(|item| item.url.clone())
        .collect();
        translations = service.translate(urls.clone()).await?;
        if let Some(url) = urls.first() {
            preview = Some(service.preview(url.clone()).await?);
        }
        snapshot = service.snapshot()?;
    }
    let mut custom_checked = false;
    if std::env::args().any(|arg| arg == "--custom") {
        let with_custom = service
            .add_source(
                "Pydantic AI".into(),
                "https://github.com/pydantic/pydantic-ai".into(),
                "frameworks".into(),
            )
            .await?;
        let custom = with_custom
            .sources
            .iter()
            .find(|source| source.custom)
            .ok_or("自定义来源未保存")?;
        let restarted = NewsService::new(&directory)?;
        if !restarted
            .snapshot()?
            .sources
            .iter()
            .any(|source| source.id == custom.id && !source.items.is_empty())
        {
            return Err("自定义来源重启验证失败".into());
        }
        service.remove_source(custom.id.clone()).await?;
        custom_checked = true;
    }
    let path = directory.join("snapshot.json");
    std::fs::write(
        &path,
        serde_json::to_vec_pretty(&snapshot).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    if let Some(preview) = &preview {
        std::fs::write(
            directory.join("preview.json"),
            serde_json::to_vec_pretty(preview).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
    }
    println!(
        "{}",
        json!({
            "snapshot": path,
            "translations": translations,
            "customChecked": custom_checked,
            "preview": preview.as_ref().map(|value| json!({ "kind": value.kind, "chars": value.content.chars().count(), "translatedChars": value.translated_content.as_ref().map(|text| text.chars().count()), "diagnostic": value.diagnostic })),
            "sources": snapshot.sources.iter().map(|source| json!({
                "name": source.name, "count": source.items.len(), "error": source.error,
                "latest": source.items.iter().take(2).map(|item| json!({"title":item.title,"publishedAt":item.published_at})).collect::<Vec<_>>()
            })).collect::<Vec<_>>()
        })
    );
    if snapshot
        .sources
        .iter()
        .all(|source| source.last_success.is_none())
    {
        return Err("所有资讯来源均获取失败，诊断和缓存已保存到临时目录".into());
    }
    if chinese
        && (translations.is_empty() || translations.iter().any(|result| result.error.is_some()))
    {
        return Err("真实翻译未全部成功，诊断已保存".into());
    }
    Ok(())
}
