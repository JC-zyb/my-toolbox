use chrono::{DateTime, Utc};
use reqwest::{header, Client, StatusCode, Url};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::{collections::HashSet, path::Path, sync::Mutex, time::Duration};
use tokio::{sync::Mutex as AsyncMutex, task::JoinSet};

const REFRESH_SECONDS: i64 = 30 * 60;
const MAX_BYTES: usize = 4 * 1024 * 1024;
const MAX_ITEMS: usize = 100;

mod catalog;
mod preview;
mod translation;
pub use catalog::catalog;
pub use preview::NewsPreview;
pub use translation::{NewsTranslation, TranslationResult};

fn default_group() -> String {
    "custom".into()
}
fn default_language() -> String {
    "zh".into()
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewsItem {
    pub url: String,
    pub title: String,
    pub summary: String,
    pub category: String,
    pub published_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub translation: Option<NewsTranslation>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewsSource {
    pub id: String,
    pub name: String,
    pub url: String,
    pub category: String,
    #[serde(default = "default_group")]
    pub group: String,
    #[serde(default)]
    pub language: String,
    #[serde(default)]
    pub custom: bool,
    #[serde(default)]
    pub filter_ai: bool,
    pub enabled: bool,
    pub last_attempt: Option<DateTime<Utc>>,
    pub last_success: Option<DateTime<Utc>>,
    pub error: Option<String>,
    pub items: Vec<NewsItem>,
    #[serde(skip_serializing, default)]
    pub etag: Option<String>,
    #[serde(skip_serializing, default)]
    pub last_modified: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewsSnapshot {
    pub automatic: bool,
    #[serde(default = "default_language")]
    pub language: String,
    pub sources: Vec<NewsSource>,
}

pub struct NewsService {
    connection: Mutex<Connection>,
    refresh_lock: AsyncMutex<()>,
    translation_lock: AsyncMutex<()>,
    client: Client,
}

impl NewsService {
    pub fn new(directory: &Path) -> Result<Self, String> {
        std::fs::create_dir_all(directory).map_err(|e| e.to_string())?;
        let connection =
            Connection::open(directory.join("news.sqlite")).map_err(|e| e.to_string())?;
        connection.execute_batch(
            "CREATE TABLE IF NOT EXISTS news_cache (id INTEGER PRIMARY KEY CHECK(id = 1), payload TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS news_validators (source TEXT PRIMARY KEY, etag TEXT, modified TEXT);
             CREATE TABLE IF NOT EXISTS news_translations (url TEXT PRIMARY KEY, hash TEXT NOT NULL, payload TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS news_previews (url TEXT PRIMARY KEY, hash TEXT NOT NULL, payload TEXT NOT NULL);"
        ).map_err(|e| e.to_string())?;
        Ok(Self {
            connection: Mutex::new(connection),
            refresh_lock: AsyncMutex::new(()),
            translation_lock: AsyncMutex::new(()),
            client: Client::builder()
                .user_agent("MyToolbox/0.1 AgentNews RSS Reader")
                .timeout(Duration::from_secs(20))
                .https_only(true)
                .redirect(reqwest::redirect::Policy::limited(5))
                .build()
                .map_err(|e| e.to_string())?,
        })
    }

    pub fn snapshot(&self) -> Result<NewsSnapshot, String> {
        let connection = self.connection.lock().map_err(|e| e.to_string())?;
        let mut snapshot =
            match connection.query_row("SELECT payload FROM news_cache WHERE id = 1", [], |row| {
                row.get::<_, String>(0)
            }) {
                Ok(payload) => serde_json::from_str::<NewsSnapshot>(&payload)
                    .map_err(|e| format!("资讯缓存损坏：{e}"))?,
                Err(rusqlite::Error::QueryReturnedNoRows) => NewsSnapshot {
                    automatic: true,
                    language: default_language(),
                    sources: catalog(),
                },
                Err(e) => return Err(e.to_string()),
            };
        let cached_sources = std::mem::take(&mut snapshot.sources);
        // 内置来源使用当前目录，自定义来源仅接受已验证的公开 HTTPS 订阅。
        snapshot.sources = catalog()
            .into_iter()
            .map(|mut source| {
                if let Some(cached) = cached_sources
                    .iter()
                    .find(|s| !s.custom && s.id == source.id)
                {
                    source.enabled = cached.enabled;
                    source.items = cached.items.clone();
                    source.last_attempt = cached.last_attempt;
                    source.last_success = cached.last_success;
                    source.error = cached.error.clone();
                }
                source
            })
            .collect();
        for cached in cached_sources.into_iter().filter(|source| source.custom) {
            let checked = catalog::custom_source(&cached.name, &cached.url, &cached.group)?;
            if checked.id != cached.id {
                return Err("自定义资讯来源身份不匹配".into());
            }
            snapshot.sources.push(cached);
        }
        for source in &mut snapshot.sources {
            match connection.query_row(
                "SELECT etag, modified FROM news_validators WHERE source = ?1",
                [&source.id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            ) {
                Ok((etag, modified)) => {
                    source.etag = etag;
                    source.last_modified = modified;
                }
                Err(rusqlite::Error::QueryReturnedNoRows) => {}
                Err(e) => return Err(e.to_string()),
            }
        }
        translation::load_cached(&connection, &mut snapshot)?;
        Ok(snapshot)
    }

    fn save(&self, snapshot: &NewsSnapshot) -> Result<(), String> {
        let payload = serde_json::to_string(snapshot).map_err(|e| e.to_string())?;
        let mut connection = self.connection.lock().map_err(|e| e.to_string())?;
        let transaction = connection.transaction().map_err(|e| e.to_string())?;
        transaction
            .execute("INSERT OR REPLACE INTO news_cache VALUES(1, ?1)", [payload])
            .map_err(|e| e.to_string())?;
        for source in &snapshot.sources {
            transaction
                .execute(
                    "INSERT OR REPLACE INTO news_validators VALUES(?1, ?2, ?3)",
                    params![source.id, source.etag, source.last_modified],
                )
                .map_err(|e| e.to_string())?;
        }
        transaction.commit().map_err(|e| e.to_string())
    }

    pub async fn preferences(
        &self,
        automatic: bool,
        enabled: Vec<String>,
    ) -> Result<NewsSnapshot, String> {
        let _guard = self.refresh_lock.lock().await;
        let mut snapshot = self.snapshot()?;
        if enabled
            .iter()
            .any(|id| !snapshot.sources.iter().any(|source| &source.id == id))
        {
            return Err("未知资讯来源".into());
        }
        snapshot.automatic = automatic;
        for source in &mut snapshot.sources {
            source.enabled = enabled.contains(&source.id);
        }
        self.save(&snapshot)?;
        Ok(snapshot)
    }

    pub async fn set_language(&self, language: String) -> Result<NewsSnapshot, String> {
        if !["zh", "original", "bilingual"].contains(&language.as_str()) {
            return Err("未知资讯语言模式".into());
        }
        let _guard = self.refresh_lock.lock().await;
        let mut snapshot = self.snapshot()?;
        snapshot.language = language;
        self.save(&snapshot)?;
        Ok(snapshot)
    }

    pub async fn add_source(
        &self,
        name: String,
        url: String,
        group: String,
    ) -> Result<NewsSnapshot, String> {
        let _guard = self.refresh_lock.lock().await;
        let mut snapshot = self.snapshot()?;
        if snapshot.sources.len() >= 100 {
            return Err("最多保留 100 个订阅来源".into());
        }
        let mut source = catalog::custom_source(&name, &url, &group)?;
        if snapshot.sources.iter().any(|old| old.url == source.url) {
            return Err("该资讯来源已订阅".into());
        }
        let result = fetch(&self.client, &source).await?;
        apply_result(&mut source, Ok(result), Utc::now());
        snapshot.sources.push(source);
        self.save(&snapshot)?;
        Ok(snapshot)
    }

    pub async fn remove_source(&self, id: String) -> Result<NewsSnapshot, String> {
        let _guard = self.refresh_lock.lock().await;
        let mut snapshot = self.snapshot()?;
        if !snapshot
            .sources
            .iter()
            .any(|source| source.id == id && source.custom)
        {
            return Err("只能移除自定义订阅，内置来源可以暂停".into());
        }
        snapshot.sources.retain(|source| source.id != id);
        self.save(&snapshot)?;
        Ok(snapshot)
    }

    pub async fn refresh(&self, force: bool) -> Result<NewsSnapshot, String> {
        let _guard = self.refresh_lock.lock().await;
        let mut snapshot = self.snapshot()?;
        if !force && !snapshot.automatic {
            return Ok(snapshot);
        }
        let now = Utc::now();
        let pending: Vec<_> = snapshot
            .sources
            .iter()
            .filter(|source| {
                source.enabled
                    && (force
                        || source
                            .last_attempt
                            .is_none_or(|at| (now - at).num_seconds() >= REFRESH_SECONDS))
            })
            .cloned()
            .collect();
        if pending.is_empty() {
            return Ok(snapshot);
        }
        for batch in pending.chunks(4) {
            let mut tasks = JoinSet::new();
            for source in batch {
                let source = source.clone();
                let client = self.client.clone();
                tasks.spawn(async move {
                    let result = fetch(&client, &source).await;
                    (source.id, result)
                });
            }
            while let Some(result) = tasks.join_next().await {
                let (id, result) = result.map_err(|e| format!("资讯采集任务失败：{e}"))?;
                let source = snapshot
                    .sources
                    .iter_mut()
                    .find(|s| s.id == id)
                    .ok_or("资讯来源已失效")?;
                apply_result(source, result, Utc::now());
            }
        }
        self.save(&snapshot)?;
        self.snapshot()
    }
}

struct FetchResult {
    items: Option<Vec<NewsItem>>,
    etag: Option<String>,
    modified: Option<String>,
}

async fn fetch(client: &Client, source: &NewsSource) -> Result<FetchResult, String> {
    let mut request = client.get(&source.url).header(
        header::ACCEPT,
        "application/atom+xml, application/rss+xml, application/xml, text/xml",
    );
    if let Some(etag) = &source.etag {
        request = request.header(header::IF_NONE_MATCH, etag);
    }
    if let Some(modified) = &source.last_modified {
        request = request.header(header::IF_MODIFIED_SINCE, modified);
    }
    let mut response = request
        .send()
        .await
        .map_err(|e| format!("请求失败：{}", e.without_url()))?;
    if response.status() == StatusCode::NOT_MODIFIED {
        if source.last_success.is_none() {
            return Err("来源返回 304，但没有成功缓存".into());
        }
        return Ok(FetchResult {
            items: None,
            etag: source.etag.clone(),
            modified: source.last_modified.clone(),
        });
    }
    if !response.status().is_success() {
        return Err(format!("HTTP {}", response.status()));
    }
    if response
        .content_length()
        .is_some_and(|n| n > MAX_BYTES as u64)
    {
        return Err("订阅响应超过 4 MiB".into());
    }
    let etag = response
        .headers()
        .get(header::ETAG)
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);
    let modified = response
        .headers()
        .get(header::LAST_MODIFIED)
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|e| format!("读取订阅失败：{}", e.without_url()))?
    {
        if bytes.len() + chunk.len() > MAX_BYTES {
            return Err("订阅响应超过 4 MiB".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(FetchResult {
        items: Some(parse(&bytes, source)?),
        etag,
        modified,
    })
}

fn apply_result(source: &mut NewsSource, result: Result<FetchResult, String>, now: DateTime<Utc>) {
    source.last_attempt = Some(now);
    match result {
        Ok(result) => {
            if let Some(mut items) = result.items {
                items.extend(source.items.clone());
                let mut seen = HashSet::new();
                items.retain(|item| seen.insert(item.url.clone()));
                items.sort_by(|a, b| b.published_at.cmp(&a.published_at).then(a.url.cmp(&b.url)));
                items.truncate(MAX_ITEMS);
                source.items = items;
            }
            source.etag = result.etag;
            source.last_modified = result.modified;
            source.last_success = Some(now);
            source.error = None;
        }
        Err(error) => source.error = Some(error),
    }
}

fn canonical_url(value: &str, base: &str) -> Option<String> {
    let mut url = Url::parse(base).ok()?.join(value).ok()?;
    if !matches!(url.scheme(), "https" | "http")
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return None;
    }
    url.set_fragment(None);
    let query: Vec<(String, String)> = url
        .query_pairs()
        .filter(|(key, _)| !key.starts_with("utm_"))
        .map(|(key, value)| (key.into_owned(), value.into_owned()))
        .collect();
    url.set_query(None);
    if !query.is_empty() {
        url.query_pairs_mut().extend_pairs(query);
    }
    Some(url.into())
}

fn parse(bytes: &[u8], source: &NewsSource) -> Result<Vec<NewsItem>, String> {
    let feed = feed_rs::parser::Builder::new()
        .base_uri(Some(&source.url))
        .build()
        .parse(bytes)
        .map_err(|e| format!("订阅格式错误：{e}"))?;
    let mut seen = HashSet::new();
    let mut items = vec![];
    for entry in feed.entries {
        let Some(title) = entry
            .title
            .map(|text| text.content)
            .filter(|text| !text.trim().is_empty())
        else {
            continue;
        };
        let Some(url) = entry
            .links
            .iter()
            .filter(|link| link.rel.as_deref().is_none_or(|rel| rel == "alternate"))
            .find_map(|link| canonical_url(&link.href, &source.url))
        else {
            continue;
        };
        if !seen.insert(url.clone()) {
            continue;
        }
        let lower = title.to_lowercase();
        if source.filter_ai && !catalog::is_ai_topic(&lower) {
            continue;
        }
        let category = if [
            "model",
            "gpt-",
            "gemini",
            "claude opus",
            "claude sonnet",
            "deepseek",
            "qwen",
            "llama",
            "模型",
            "千问",
            "混元",
            "智谱",
            "kimi",
            "minimax",
        ]
        .iter()
        .any(|word| lower.contains(word))
        {
            "models"
        } else if [
            "agent",
            "mcp",
            "protocol",
            "reasoning",
            "tool use",
            "multi-agent",
            "智能体",
            "推理",
            "多智能体",
            "工作流",
            "协议",
        ]
        .iter()
        .any(|word| lower.contains(word))
        {
            "technology"
        } else if [
            "codex",
            "chatgpt",
            "api",
            "feature",
            "update",
            "launch",
            "introducing",
            "发布",
            "升级",
            "更新",
            "功能",
            "上线",
        ]
        .iter()
        .any(|word| lower.contains(word))
        {
            "features"
        } else {
            &source.category
        };
        let summary = entry
            .summary
            .map(|text| text.content)
            .or_else(|| entry.content.and_then(|content| content.body))
            .unwrap_or_default();
        items.push(NewsItem {
            url,
            title: title.chars().take(400).collect(),
            summary: summary.chars().take(8000).collect(),
            category: category.into(),
            published_at: entry.published.or(entry.updated),
            translation: None,
        });
    }
    if items.is_empty() && !bytes.is_empty() && feed.title.is_none() {
        return Err("订阅缺少有效内容".into());
    }
    items.sort_by(|a, b| b.published_at.cmp(&a.published_at).then(a.url.cmp(&b.url)));
    items.truncate(MAX_ITEMS);
    Ok(items)
}

#[cfg(test)]
mod tests;
