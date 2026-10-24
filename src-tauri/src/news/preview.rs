use super::*;
use rusqlite::OptionalExtension;

const PREVIEW_CHARS: usize = 3500;

fn translation_chunks(text: &str) -> Vec<String> {
    let mut chunks = vec![];
    let mut current = String::new();
    let mut count = 0;
    for token in text.split_inclusive(char::is_whitespace) {
        let length = token.chars().count();
        if count + length > 1000 && !current.is_empty() {
            chunks.push(std::mem::take(&mut current));
            count = 0;
        }
        if length > 1000 {
            let chars: Vec<_> = token.chars().collect();
            chunks.extend(chars.chunks(1000).map(|part| part.iter().collect()));
        } else {
            current.push_str(token);
            count += length;
        }
    }
    if !current.is_empty() {
        chunks.push(current);
    }
    chunks
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewsPreview {
    pub url: String,
    pub title: String,
    pub source_name: String,
    pub content: String,
    pub translated_content: Option<String>,
    pub kind: String,
    pub diagnostic: Option<String>,
    pub fetched_at: DateTime<Utc>,
}

async fn page_excerpt(client: &Client, url: &str) -> Result<String, String> {
    let mut response = client
        .get(url)
        .send()
        .await
        .map_err(|error| format!("网页请求失败：{}", error.without_url()))?;
    if !response.status().is_success() {
        return Err(format!("网页返回 HTTP {}", response.status()));
    }
    let content_type = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("");
    if !(content_type.contains("html") || content_type.contains("text/plain")) {
        return Err("该网页不是可预览的文本页面".into());
    }
    let mut bytes = vec![];
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|error| format!("读取网页失败：{}", error.without_url()))?
    {
        if bytes.len() + chunk.len() > MAX_BYTES {
            return Err("网页响应超过 4 MiB".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    let html = String::from_utf8(bytes).map_err(|_| "网页不是 UTF-8 文本")?;
    let document = dom_query::Document::from(html);
    document
        .select("script,style,template,nav,footer,header,aside,form,noscript,svg")
        .remove();
    for selector in [
        "article",
        ".entry-content",
        ".post-content",
        ".markdown-body",
        "[itemprop=articleBody]",
        "main",
        "[role=main]",
    ] {
        let text = document.select(selector).formatted_text().to_string();
        if text.chars().count() >= 100 {
            return Ok(text.chars().take(PREVIEW_CHARS).collect());
        }
    }
    Err("网页未提供可提取的正文，可能需要登录或动态加载".into())
}

impl NewsService {
    pub async fn preview(&self, url: String) -> Result<NewsPreview, String> {
        self.preview_with_endpoint(url, translation::TRANSLATE_URL)
            .await
    }

    async fn preview_with_endpoint(
        &self,
        url: String,
        endpoint: &str,
    ) -> Result<NewsPreview, String> {
        let snapshot = self.snapshot()?;
        let (source, item) = snapshot
            .sources
            .iter()
            .filter(|source| source.enabled)
            .find_map(|source| {
                source
                    .items
                    .iter()
                    .find(|item| item.url == url)
                    .map(|item| (source, item))
            })
            .ok_or("资讯已失效或订阅已暂停")?;
        let public_page = Url::parse(&url)
            .is_ok_and(|url| super::network::validate_public_https_url(&url).is_ok());
        let hash = translation::fingerprint(item);
        let cached: Option<String> = if public_page {
            self.connection
                .lock()
                .map_err(|e| e.to_string())?
                .query_row(
                    "SELECT payload FROM news_previews WHERE url = ?1 AND hash = ?2",
                    params![url, hash],
                    |row| row.get(0),
                )
                .optional()
                .map_err(|e| e.to_string())?
        } else {
            None
        };
        let mut preview = if let Some(payload) = cached {
            serde_json::from_str::<NewsPreview>(&payload)
                .map_err(|e| format!("预览缓存损坏：{e}"))?
        } else {
            let fallback = translation::plain_text(&item.summary);
            let mut preview = NewsPreview {
                url: url.clone(),
                title: item.title.clone(),
                source_name: source.name.clone(),
                content: fallback.chars().take(PREVIEW_CHARS).collect(),
                translated_content: None,
                kind: "feed".into(),
                diagnostic: None,
                fetched_at: Utc::now(),
            };
            // 订阅正文已足够阅读时直接预览，只在摘要很短时读取公开网页。
            if fallback.chars().count() < 300 {
                if public_page {
                    match page_excerpt(&self.client, &url).await {
                        Ok(content) => {
                            preview.content = content;
                            preview.kind = "web".into();
                        }
                        Err(error) => {
                            preview.diagnostic = Some(format!("{error}；当前展示订阅摘要"))
                        }
                    }
                } else {
                    preview.diagnostic =
                        Some("原文链接不是公开 HTTPS 页面；当前展示订阅摘要".into());
                }
            }
            preview
        };
        if snapshot.language != "original"
            && preview.translated_content.is_none()
            && translation::needs_translation(&preview.content)
        {
            let texts = translation_chunks(&preview.content);
            let _guard = self.translation_lock.lock().await;
            if self.snapshot()?.language != "original" {
                match translation::request_translation(&self.client, endpoint, &texts).await {
                    Ok(translations) => {
                        preview.translated_content = Some(translations.join("\n\n"))
                    }
                    Err(error) => {
                        preview.diagnostic = Some(match preview.diagnostic {
                            Some(previous) => format!("{previous}；{error}"),
                            None => error,
                        })
                    }
                }
            }
        }
        if preview.diagnostic.is_none() {
            self.connection
                .lock()
                .map_err(|e| e.to_string())?
                .execute(
                    "INSERT OR REPLACE INTO news_previews VALUES(?1, ?2, ?3)",
                    params![
                        url,
                        hash,
                        serde_json::to_string(&preview).map_err(|e| e.to_string())?
                    ],
                )
                .map_err(|e| e.to_string())?;
        }
        Ok(preview)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn translation_batches_preserve_words_and_limit_multibyte_text() {
        let text = format!(
            "{}Carlos Eduardo Mazzei.\n\n{}",
            "Agents plan tasks. ".repeat(52),
            "智能体".repeat(500)
        );
        let chunks = translation_chunks(&text);
        assert_eq!(chunks.concat(), text);
        assert!(chunks.iter().all(|chunk| chunk.chars().count() <= 1000));
        assert!(chunks.iter().any(|chunk| chunk.contains("Mazzei")));
        assert!(chunks[..chunks.len() - 2]
            .iter()
            .all(|chunk| chunk.ends_with(char::is_whitespace)));
    }

    #[tokio::test]
    async fn previews_chinese_feed_content_without_web_or_translation_calls() {
        let directory = tempfile::tempdir().unwrap();
        let service = NewsService::new(directory.path()).unwrap();
        let mut snapshot = service.snapshot().unwrap();
        snapshot.sources[0].items = vec![NewsItem {
            url: "https://example.com/preview".into(),
            title: "智能体预览".into(),
            summary: format!(
                "<p>{}</p>",
                "智能体可以搜索网页、规划任务、调用工具和协作执行。".repeat(30)
            ),
            category: "technology".into(),
            published_at: None,
            translation: None,
        }];
        service.save(&snapshot).unwrap();
        let preview = service
            .preview("https://example.com/preview".into())
            .await
            .unwrap();
        assert_eq!(preview.kind, "feed");
        assert!(preview.content.contains("协作执行"));
        assert!(!preview.content.contains("<p>"));
        assert!(preview.translated_content.is_none());
        assert!(preview.diagnostic.is_none());
        assert_eq!(
            service.preview(preview.url).await.unwrap().content,
            preview.content
        );
        assert!(service
            .preview("https://example.com/not-subscribed".into())
            .await
            .is_err());
    }

    #[tokio::test]
    async fn private_article_links_ignore_cached_web_content() {
        let directory = tempfile::tempdir().unwrap();
        let service = NewsService::new(directory.path()).unwrap();
        let mut snapshot = service.snapshot().unwrap();
        let article = NewsItem {
            url: "https://127.0.0.1/private".into(),
            title: "内网文章".into(),
            summary: "订阅摘要仍可阅读。".into(),
            category: "technology".into(),
            published_at: None,
            translation: None,
        };
        snapshot.sources[0].items = vec![article.clone()];
        service.save(&snapshot).unwrap();
        let cached = NewsPreview {
            url: article.url.clone(),
            title: article.title.clone(),
            source_name: snapshot.sources[0].name.clone(),
            content: "旧版缓存中的内网页面内容".into(),
            translated_content: None,
            kind: "web".into(),
            diagnostic: None,
            fetched_at: Utc::now(),
        };
        service
            .connection
            .lock()
            .unwrap()
            .execute(
                "INSERT INTO news_previews VALUES(?1, ?2, ?3)",
                params![
                    article.url,
                    translation::fingerprint(&article),
                    serde_json::to_string(&cached).unwrap()
                ],
            )
            .unwrap();

        let preview = service
            .preview_with_endpoint(article.url, "http://127.0.0.1:1")
            .await
            .unwrap();
        assert_eq!(preview.kind, "feed");
        assert_eq!(preview.content, "订阅摘要仍可阅读。");
        assert!(preview
            .diagnostic
            .as_deref()
            .is_some_and(|value| value.contains("公开 HTTPS")));
    }

    #[tokio::test]
    async fn extracts_article_text_and_excludes_navigation() {
        let html = format!("<html><body><nav>navigation</nav><article><h1>Agent</h1><p>{}</p><script>bad()</script></article></body></html>", "A useful agent supports planning and tool use. ".repeat(10));
        let (url, thread) =
            super::super::tests::server("200 OK", &html, "Content-Type: text/html\r\n");
        let text = page_excerpt(&Client::builder().no_proxy().build().unwrap(), &url)
            .await
            .unwrap();
        thread.join().unwrap();
        assert!(text.contains("planning"));
        assert!(!text.contains("navigation"));
        assert!(!text.contains("bad()"));
    }

    #[tokio::test]
    async fn translates_preview_caches_success_and_exposes_failures() {
        let directory = tempfile::tempdir().unwrap();
        let mut service = NewsService::new(directory.path()).unwrap();
        service.client = Client::builder().no_proxy().build().unwrap();
        let mut snapshot = service.snapshot().unwrap();
        let original = "Agents support planning, memory, and tool use. ".repeat(10);
        let article = NewsItem {
            url: "https://example.com/preview".into(),
            title: "Agent update".into(),
            summary: original.clone(),
            category: "technology".into(),
            published_at: None,
            translation: None,
        };
        snapshot.sources[0].items = vec![article.clone()];
        service.save(&snapshot).unwrap();
        let (endpoint, thread) = super::super::tests::server(
            "200 OK",
            r#"[["智能体支持规划、记忆与工具调用。","en"]]"#,
            "Content-Type: application/json\r\n",
        );
        let preview = service
            .preview_with_endpoint(article.url.clone(), &endpoint)
            .await
            .unwrap();
        thread.join().unwrap();
        assert_eq!(
            preview.translated_content.as_deref(),
            Some("智能体支持规划、记忆与工具调用。")
        );
        assert_eq!(preview.content, original.trim());
        let cached = service
            .preview_with_endpoint(article.url.clone(), "http://127.0.0.1:1")
            .await
            .unwrap();
        assert!(cached.diagnostic.is_none());
        snapshot.sources[0].items[0]
            .summary
            .push_str("Changed release notes.");
        service.save(&snapshot).unwrap();
        let (endpoint, thread) =
            super::super::tests::server("503 Service Unavailable", "offline", "");
        let failed = service
            .preview_with_endpoint(article.url, &endpoint)
            .await
            .unwrap();
        thread.join().unwrap();
        assert!(failed.diagnostic.as_ref().unwrap().contains("503"));
        assert!(failed.translated_content.is_none());
        assert!(failed.content.contains("Changed release notes."));

        snapshot.language = "original".into();
        let url = "https://127.0.0.1/private".to_string();
        snapshot.sources[0].items[0].url = url.clone();
        snapshot.sources[0].items[0].summary = "Short excerpt".into();
        service.save(&snapshot).unwrap();
        let fallback = service.preview(url).await.unwrap();
        assert_eq!(fallback.content, "Short excerpt");
        assert_eq!(fallback.kind, "feed");
        assert!(fallback.diagnostic.unwrap().contains("公开 HTTPS"));
    }
}
