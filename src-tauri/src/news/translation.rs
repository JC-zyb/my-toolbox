use super::*;
use sha2::{Digest, Sha256};
use std::collections::HashMap;

pub(super) const TRANSLATE_URL: &str = "https://translate.googleapis.com/translate_a/t";
const MAX_SUMMARY: usize = 800;
const MAX_TRANSLATION_BYTES: usize = 128 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewsTranslation {
    pub title: String,
    pub summary: String,
    pub translated: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TranslationResult {
    pub url: String,
    pub translation: Option<NewsTranslation>,
    pub error: Option<String>,
}

pub(super) fn fingerprint(item: &NewsItem) -> String {
    format!(
        "{:x}",
        Sha256::digest(format!("zh-v1\0{}\0{}", item.title, item.summary).as_bytes())
    )
}

pub fn plain_text(value: &str) -> String {
    let document = dom_query::Document::fragment(value);
    document.select("script,style,template").remove();
    document
        .formatted_text()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

pub(super) fn needs_translation(value: &str) -> bool {
    if !value.contains(char::is_whitespace) && value.chars().any(|c| c.is_ascii_digit()) {
        return false;
    }
    let letters = value.chars().filter(|c| c.is_alphabetic()).count();
    let chinese = value
        .chars()
        .filter(|c| matches!(*c as u32, 0x3400..=0x9fff | 0x20000..=0x2fa1f))
        .count();
    letters > 2 && chinese * 3 < letters
}

pub(super) fn load_cached(
    connection: &Connection,
    snapshot: &mut NewsSnapshot,
) -> Result<(), String> {
    let mut statement = connection
        .prepare("SELECT url, hash, payload FROM news_translations")
        .map_err(|e| e.to_string())?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .map_err(|e| e.to_string())?;
    let mut cache = HashMap::new();
    for row in rows {
        let (url, hash, payload) = row.map_err(|e| e.to_string())?;
        let translation = serde_json::from_str::<NewsTranslation>(&payload)
            .map_err(|e| format!("译文缓存损坏：{e}"))?;
        cache.insert(url, (hash, translation));
    }
    for item in snapshot
        .sources
        .iter_mut()
        .flat_map(|source| &mut source.items)
    {
        item.translation = cache
            .get(&item.url)
            .filter(|(hash, _)| hash == &fingerprint(item))
            .map(|(_, translation)| translation.clone());
    }
    Ok(())
}

pub(super) async fn request_translation(
    client: &Client,
    endpoint: &str,
    texts: &[String],
) -> Result<Vec<String>, String> {
    if texts.is_empty() {
        return Ok(vec![]);
    }
    let form: Vec<_> = texts.iter().map(|text| ("q", text)).collect();
    let mut response = client
        .post(endpoint)
        .query(&[("client", "gtx"), ("sl", "auto"), ("tl", "zh-CN")])
        .form(&form)
        .send()
        .await
        .map_err(|error| format!("翻译服务连接失败：{}", error.without_url()))?;
    if !response.status().is_success() {
        return Err(format!("翻译服务返回 HTTP {}", response.status()));
    }
    let mut body = vec![];
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|error| format!("读取译文失败：{}", error.without_url()))?
    {
        if body.len() + chunk.len() > MAX_TRANSLATION_BYTES {
            return Err("翻译响应超过大小限制".into());
        }
        body.extend_from_slice(&chunk);
    }
    let response: serde_json::Value =
        serde_json::from_slice(&body).map_err(|_| "翻译服务返回无效 JSON")?;
    let entries = response
        .as_array()
        .filter(|array| array.len() == texts.len())
        .ok_or("翻译结果数量与原文不匹配")?;
    entries
        .iter()
        .map(|entry| {
            entry
                .as_str()
                .or_else(|| {
                    entry
                        .as_array()
                        .and_then(|array| array.first())
                        .and_then(|value| value.as_str())
                })
                .filter(|text| !text.trim().is_empty())
                .map(str::to_owned)
                .ok_or_else(|| "翻译服务返回空译文或异常格式".into())
        })
        .collect()
}

impl NewsService {
    pub async fn translate(&self, urls: Vec<String>) -> Result<Vec<TranslationResult>, String> {
        self.translate_with_endpoint(urls, TRANSLATE_URL).await
    }

    async fn translate_with_endpoint(
        &self,
        urls: Vec<String>,
        endpoint: &str,
    ) -> Result<Vec<TranslationResult>, String> {
        if urls.len() > 6 {
            return Err("一次最多翻译 6 条资讯".into());
        }
        let _guard = self.translation_lock.lock().await;
        let snapshot = self.snapshot()?;
        if snapshot.language == "original" {
            return Ok(vec![]);
        }
        let mut seen = HashSet::new();
        let items = urls
            .iter()
            .filter(|url| seen.insert((*url).clone()))
            .map(|url| {
                snapshot
                    .sources
                    .iter()
                    .filter(|source| source.enabled)
                    .flat_map(|source| &source.items)
                    .find(|item| &item.url == url)
                    .cloned()
                    .ok_or_else(|| format!("资讯已失效或订阅已暂停：{url}"))
            })
            .collect::<Result<Vec<_>, String>>()?;
        let mut results = vec![];
        // 每批最多两篇，控制公开接口单次文本量；不翻译尚未浏览的整库内容。
        for batch in items.chunks(2) {
            if self.snapshot()?.language == "original" {
                break;
            }
            let mut prepared = vec![];
            let mut texts = vec![];
            for item in batch {
                if let Some(translation) = &item.translation {
                    results.push(TranslationResult {
                        url: item.url.clone(),
                        translation: Some(translation.clone()),
                        error: None,
                    });
                    continue;
                }
                let mut translation = NewsTranslation {
                    title: plain_text(&item.title),
                    summary: plain_text(&item.summary)
                        .chars()
                        .take(MAX_SUMMARY)
                        .collect(),
                    translated: false,
                };
                let title_index = needs_translation(&translation.title).then(|| {
                    texts.push(translation.title.clone());
                    texts.len() - 1
                });
                let summary_index = needs_translation(&translation.summary).then(|| {
                    texts.push(translation.summary.clone());
                    texts.len() - 1
                });
                translation.translated = title_index.is_some() || summary_index.is_some();
                prepared.push((item, translation, title_index, summary_index));
            }
            let translated = request_translation(&self.client, endpoint, &texts).await;
            for (item, mut translation, title_index, summary_index) in prepared {
                let error = match &translated {
                    Ok(texts) => {
                        if let Some(index) = title_index {
                            translation.title = texts[index].clone();
                        }
                        if let Some(index) = summary_index {
                            translation.summary = texts[index].clone();
                        }
                        None
                    }
                    Err(error) if translation.translated => Some(error.clone()),
                    Err(_) => None,
                };
                if error.is_none() {
                    self.connection
                        .lock()
                        .map_err(|e| e.to_string())?
                        .execute(
                            "INSERT OR REPLACE INTO news_translations VALUES(?1, ?2, ?3)",
                            params![
                                item.url,
                                fingerprint(item),
                                serde_json::to_string(&translation).map_err(|e| e.to_string())?
                            ],
                        )
                        .map_err(|e| e.to_string())?;
                }
                results.push(TranslationResult {
                    url: item.url.clone(),
                    translation: error.is_none().then_some(translation),
                    error,
                });
            }
            if translated.is_err() {
                break;
            }
        }
        if let Some(error) = results.iter().find_map(|result| result.error.clone()) {
            for item in &items {
                if !results.iter().any(|result| result.url == item.url) {
                    results.push(TranslationResult {
                        url: item.url.clone(),
                        translation: None,
                        error: Some(error.clone()),
                    });
                }
            }
        }
        Ok(results)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_and_language_detection_exclude_markup_and_code() {
        assert_eq!(
            plain_text(
                "<p>Agent &amp; tools</p><script>secret()</script><code>token</code><p>Memory</p>"
            ),
            "Agent & tools token Memory"
        );
        assert!(!needs_translation("智能体支持调用工具"));
        assert!(!needs_translation("v2.1.302"));
        assert!(needs_translation("New agent tools and memory"));
    }

    #[tokio::test]
    async fn translates_persists_invalidates_and_reports_http_errors() {
        let directory = tempfile::tempdir().unwrap();
        let mut service = NewsService::new(directory.path()).unwrap();
        service.client = Client::builder().no_proxy().build().unwrap();
        let mut snapshot = service.snapshot().unwrap();
        snapshot.sources[0].items = vec![NewsItem {
            url: "https://example.com/agent".into(),
            title: "New agent features".into(),
            summary: "<p>Memory and tool use</p>".into(),
            category: "features".into(),
            published_at: None,
            translation: None,
        }];
        service.save(&snapshot).unwrap();
        let (url, thread) = super::super::tests::server(
            "200 OK",
            r#"[["智能体新功能","en"],["记忆与工具调用","en"]]"#,
            "Content-Type: application/json\r\n",
        );
        let result = service
            .translate_with_endpoint(vec!["https://example.com/agent".into()], &url)
            .await
            .unwrap();
        assert!(thread.join().unwrap().starts_with("POST"));
        assert_eq!(
            result[0].translation.as_ref().unwrap().title,
            "智能体新功能"
        );
        drop(service);
        let mut reopened = NewsService::new(directory.path()).unwrap();
        reopened.client = Client::builder().no_proxy().build().unwrap();
        let mut cached = reopened.snapshot().unwrap();
        assert_eq!(
            cached.sources[0].items[0]
                .translation
                .as_ref()
                .unwrap()
                .summary,
            "记忆与工具调用"
        );
        assert!(reopened
            .translate_with_endpoint(
                vec!["https://example.com/agent".into()],
                "http://127.0.0.1:1"
            )
            .await
            .unwrap()[0]
            .error
            .is_none());
        cached.sources[0].items[0].title = "Updated agent features".into();
        reopened.save(&cached).unwrap();
        assert!(reopened.snapshot().unwrap().sources[0].items[0]
            .translation
            .is_none());
        let (url, thread) = super::super::tests::server("429 Too Many Requests", "limited", "");
        let failure = reopened
            .translate_with_endpoint(vec!["https://example.com/agent".into()], &url)
            .await
            .unwrap();
        thread.join().unwrap();
        assert!(failure[0].error.as_ref().unwrap().contains("429"));
        assert!(reopened.snapshot().unwrap().sources[0].items[0]
            .translation
            .is_none());
        reopened.set_language("original".into()).await.unwrap();
        assert!(reopened
            .translate_with_endpoint(
                vec!["https://example.com/agent".into()],
                "http://127.0.0.1:1"
            )
            .await
            .unwrap()
            .is_empty());
    }

    #[tokio::test]
    async fn rejects_incomplete_and_malformed_translation_responses() {
        let client = Client::builder().no_proxy().build().unwrap();
        for body in [r#"[["中文","en"]]"#, r#"["中文", ""]"#, "not-json"] {
            let (endpoint, thread) =
                super::super::tests::server("200 OK", body, "Content-Type: application/json\r\n");
            assert!(
                request_translation(&client, &endpoint, &["first".into(), "second".into()])
                    .await
                    .is_err()
            );
            thread.join().unwrap();
        }
    }
}
