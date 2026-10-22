use super::*;
use sha2::{Digest, Sha256};

pub const GROUPS: &[&str] = &[
    "media",
    "general",
    "coding",
    "frameworks",
    "platforms",
    "models",
    "protocols",
    "custom",
];

fn source(
    id: &str,
    name: &str,
    url: &str,
    category: &str,
    group: &str,
    language: &str,
) -> NewsSource {
    NewsSource {
        id: id.into(),
        name: name.into(),
        url: url.into(),
        category: category.into(),
        group: group.into(),
        language: language.into(),
        custom: false,
        filter_ai: false,
        enabled: true,
        last_attempt: None,
        last_success: None,
        error: None,
        items: vec![],
        etag: None,
        last_modified: None,
    }
}

pub fn catalog() -> Vec<NewsSource> {
    let mut sources = vec![
        source(
            "openai",
            "OpenAI News",
            "https://openai.com/news/rss.xml",
            "technology",
            "models",
            "en",
        ),
        source(
            "deepmind",
            "Google DeepMind",
            "https://deepmind.google/blog/rss.xml",
            "technology",
            "models",
            "en",
        ),
        source(
            "huggingface",
            "Hugging Face",
            "https://huggingface.co/blog/feed.xml",
            "technology",
            "models",
            "en",
        ),
    ];
    for (id, name, repo, group) in [
        ("codex", "OpenAI Codex", "openai/codex", "coding"),
        (
            "claude-code",
            "Claude Code",
            "anthropics/claude-code",
            "coding",
        ),
        (
            "agents-sdk",
            "OpenAI Agents SDK",
            "openai/openai-agents-python",
            "frameworks",
        ),
        (
            "langgraph",
            "LangGraph",
            "langchain-ai/langgraph",
            "frameworks",
        ),
        ("adk", "Google ADK", "google/adk-python", "frameworks"),
        (
            "mcp",
            "MCP SDK",
            "modelcontextprotocol/typescript-sdk",
            "protocols",
        ),
        (
            "gemini-cli",
            "Gemini CLI",
            "google-gemini/gemini-cli",
            "coding",
        ),
        ("cline", "Cline", "cline/cline", "coding"),
        ("roo-code", "Roo Code", "RooCodeInc/Roo-Code", "coding"),
        ("opencode", "OpenCode", "anomalyco/opencode", "coding"),
        ("aider", "Aider", "Aider-AI/aider", "coding"),
        (
            "openhands",
            "OpenHands",
            "All-Hands-AI/OpenHands",
            "general",
        ),
        ("openclaw", "OpenClaw", "openclaw/openclaw", "general"),
        ("deerflow", "DeerFlow", "bytedance/deer-flow", "general"),
        ("autogen", "AutoGen", "microsoft/autogen", "frameworks"),
        (
            "agent-framework",
            "Microsoft Agent Framework",
            "microsoft/agent-framework",
            "frameworks",
        ),
        ("crewai", "CrewAI", "crewAIInc/crewAI", "frameworks"),
        (
            "smolagents",
            "smolagents",
            "huggingface/smolagents",
            "frameworks",
        ),
        (
            "qwen-agent",
            "Qwen-Agent",
            "QwenLM/Qwen-Agent",
            "frameworks",
        ),
        (
            "agentscope",
            "AgentScope",
            "alibaba/agentscope",
            "frameworks",
        ),
        (
            "spring-ai",
            "Spring AI Alibaba",
            "alibaba/spring-ai-alibaba",
            "frameworks",
        ),
        ("dify", "Dify", "langgenius/dify", "platforms"),
        ("coze", "Coze Studio", "coze-dev/coze-studio", "platforms"),
        ("flowise", "Flowise", "FlowiseAI/Flowise", "platforms"),
        ("n8n", "n8n", "n8n-io/n8n", "platforms"),
        ("a2a", "A2A 协议", "a2aproject/A2A", "protocols"),
    ] {
        sources.push(source(
            id,
            name,
            &format!("https://github.com/{repo}/releases.atom"),
            if matches!(group, "coding" | "general" | "platforms") {
                "features"
            } else {
                "technology"
            },
            group,
            "auto",
        ));
    }
    for (id, name, url, filter_ai) in [
        ("qbitai", "量子位", "https://www.qbitai.com/feed", false),
        ("ithome", "IT之家 · AI", "https://www.ithome.com/rss/", true),
        (
            "infoq",
            "InfoQ 中文 · AI",
            "https://www.infoq.cn/feed",
            true,
        ),
    ] {
        let mut item = source(id, name, url, "technology", "media", "zh");
        item.filter_ai = filter_ai;
        sources.push(item);
    }
    sources
}

pub fn is_ai_topic(title: &str) -> bool {
    [
        "agent",
        "智能体",
        "大模型",
        "人工智能",
        "生成式",
        "ai ",
        "ai+",
        "ai：",
        "ai，",
        "ai模型",
        "ai应用",
        "ai编程",
        "openai",
        "anthropic",
        "codex",
        "claude",
        "gemini",
        "deepseek",
        "qwen",
        "kimi",
        "manus",
        "cursor",
        "windsurf",
        "trae",
        "qoder",
        "copilot",
        "gpt",
        "llm",
        "mcp",
        "dify",
        "coze",
        "千问",
        "豆包",
        "混元",
        "扣子",
        "文心",
        "智谱",
        "minimax",
        "多模态",
    ]
    .iter()
    .any(|keyword| title.contains(keyword))
}

pub fn custom_source(name: &str, address: &str, group: &str) -> Result<NewsSource, String> {
    if !GROUPS.contains(&group) {
        return Err("未知资讯领域".into());
    }
    let mut url =
        Url::parse(address.trim()).map_err(|_| "请输入完整 HTTPS 订阅或 GitHub 仓库地址")?;
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.host_str().is_none()
    {
        return Err("仅支持不含凭据的公开 HTTPS 地址".into());
    }
    let host = url.host_str().unwrap_or_default();
    if host == "localhost" || host.ends_with(".localhost") || host.ends_with(".local") {
        return Err("订阅地址必须是公开网站".into());
    }
    // URL 库将 IP 字面值解析为统一格式，避免接受本地回环或私有地址。
    if let Ok(ip) = host.trim_matches(['[', ']']).parse::<std::net::IpAddr>() {
        let local = match ip {
            std::net::IpAddr::V4(ip) => {
                ip.is_private() || ip.is_loopback() || ip.is_link_local() || ip.is_unspecified()
            }
            std::net::IpAddr::V6(ip) => {
                ip.is_loopback()
                    || ip.is_unspecified()
                    || ip.is_unique_local()
                    || ip.is_unicast_link_local()
            }
        };
        if local {
            return Err("订阅地址必须是公开网站".into());
        }
    }
    if host.eq_ignore_ascii_case("github.com") {
        let parts: Vec<_> = url.path().trim_matches('/').split('/').collect();
        if parts.len() == 2
            && parts.iter().all(|part| {
                !part.is_empty()
                    && part
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c))
            })
        {
            let path = format!(
                "/{}/{}/releases.atom",
                parts[0],
                parts[1].trim_end_matches(".git")
            );
            url.set_path(&path);
            url.set_query(None);
        } else if !(parts.len() == 3 && parts[2] == "releases.atom") {
            return Err("GitHub 地址应指向仓库首页或 releases.atom".into());
        }
    }
    url.set_fragment(None);
    let name = if name.trim().is_empty() {
        url.host_str().unwrap_or("订阅来源")
    } else {
        name.trim()
    };
    if name.chars().count() > 80 {
        return Err("订阅名称不能超过 80 个字符".into());
    }
    let id = format!("custom-{:x}", Sha256::digest(url.as_str().as_bytes()));
    let mut item = source(&id, name, url.as_str(), "technology", group, "auto");
    item.custom = true;
    Ok(item)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_public_feeds_and_normalizes_github_repositories() {
        let repo = custom_source(
            "Pydantic AI",
            "https://github.com/pydantic/pydantic-ai.git#readme",
            "frameworks",
        )
        .unwrap();
        assert_eq!(
            repo.url,
            "https://github.com/pydantic/pydantic-ai/releases.atom"
        );
        assert!(repo.custom);
        assert_eq!(
            repo.id,
            custom_source("另一个名称", &repo.url, "custom").unwrap().id
        );
        assert_eq!(
            custom_source("", "https://example.com/feed", "media")
                .unwrap()
                .name,
            "example.com"
        );
        for url in [
            "http://example.com/rss",
            "https://user:password@example.com/feed",
            "https://localhost/feed",
            "https://127.0.0.1/rss",
            "https://192.168.1.1/rss",
            "https://[::1]/rss",
            "https://github.com/pydantic/pydantic-ai/issues",
        ] {
            assert!(custom_source("", url, "custom").is_err(), "{url}");
        }
        assert!(custom_source("", "https://example.com/feed", "unknown").is_err());
    }

    #[tokio::test]
    async fn migrates_old_preferences_and_preserves_custom_subscriptions() {
        let directory = tempfile::tempdir().unwrap();
        let service = NewsService::new(directory.path()).unwrap();
        let mut old = serde_json::to_value(service.snapshot().unwrap()).unwrap();
        old.as_object_mut().unwrap().remove("language");
        old["automatic"] = false.into();
        let sources = old["sources"].as_array_mut().unwrap();
        sources.truncate(9);
        sources[0]["enabled"] = false.into();
        for source in sources {
            for field in ["group", "language", "custom", "filterAi"] {
                source.as_object_mut().unwrap().remove(field);
            }
        }
        service
            .connection
            .lock()
            .unwrap()
            .execute(
                "INSERT OR REPLACE INTO news_cache VALUES(1, ?1)",
                [old.to_string()],
            )
            .unwrap();
        let mut migrated = service.snapshot().unwrap();
        assert_eq!(migrated.language, "zh");
        assert!(!migrated.automatic);
        assert!(!migrated.sources[0].enabled);
        assert!(migrated
            .sources
            .iter()
            .any(|source| source.id == "qbitai" && source.language == "zh"));
        for group in [
            "general",
            "coding",
            "frameworks",
            "platforms",
            "models",
            "protocols",
            "media",
        ] {
            assert!(migrated.sources.iter().any(|source| source.group == group));
        }
        let custom = custom_source("自选来源", "https://example.com/feed", "general").unwrap();
        migrated.sources.push(custom.clone());
        service.save(&migrated).unwrap();
        drop(service);
        let restarted = NewsService::new(directory.path()).unwrap();
        assert!(restarted
            .snapshot()
            .unwrap()
            .sources
            .iter()
            .any(|source| source.id == custom.id));
        restarted
            .preferences(false, vec![custom.id.clone()])
            .await
            .unwrap();
        assert!(
            restarted
                .snapshot()
                .unwrap()
                .sources
                .iter()
                .find(|source| source.id == custom.id)
                .unwrap()
                .enabled
        );
        assert!(restarted.remove_source("openai".into()).await.is_err());
        restarted.remove_source(custom.id.clone()).await.unwrap();
        assert!(!restarted
            .snapshot()
            .unwrap()
            .sources
            .iter()
            .any(|source| source.id == custom.id));
        assert!(restarted.set_language("invalid".into()).await.is_err());
        restarted.set_language("bilingual".into()).await.unwrap();
        assert_eq!(
            NewsService::new(directory.path())
                .unwrap()
                .snapshot()
                .unwrap()
                .language,
            "bilingual"
        );
    }

    #[test]
    fn filters_general_chinese_technology_news_by_ai_topics() {
        let source = catalog()
            .into_iter()
            .find(|source| source.id == "ithome")
            .unwrap();
        let feed = br#"<rss version="2.0"><channel><title>Tech</title><item><title>Cursor agent update</title><link>https://example.com/ai</link></item><item><title>New phone release</title><link>https://example.com/phone</link></item></channel></rss>"#;
        let items = parse(feed, &source).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].url, "https://example.com/ai");
        assert!(is_ai_topic("manus 发布新功能"));
        assert!(is_ai_topic("智谱发布新模型"));
    }
}
