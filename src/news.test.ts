import { describe, expect, it } from "vitest";
import { recommendNews, selectNews, type NewsSnapshot, type NewsItem } from "./news";

const article = (url: string, title: string, publishedAt: string | null, category: NewsItem["category"] = "models"): NewsItem => ({ url, title, publishedAt, category, summary: "release notes" });
const now = Date.parse("2026-10-18T10:00:00Z");
const snapshot: NewsSnapshot = {
  automatic: true,
  sources: [
    { id: "official", name: "Official", url: "https://example.com/feed", category: "models", enabled: true, lastAttempt: null, lastSuccess: null, error: null, items: [article("https://example.com/new", "New model", "2026-10-18T08:00:00Z"), article("https://example.com/old", "Old model", "2026-01-01T00:00:00Z"), article("https://example.com/undated", "Undated", null)] },
    { id: "github", name: "GitHub", url: "https://example.com/atom", category: "features", enabled: true, lastAttempt: null, lastSuccess: null, error: "offline", items: [article("https://example.com/new", "Duplicate", "2026-10-18T08:00:00Z"), article("https://example.com/agent", "Agent workflow", "2026-10-17T20:00:00Z", "technology")] },
    { id: "paused", name: "Paused", url: "https://example.com/paused", category: "features", enabled: false, lastAttempt: null, lastSuccess: null, error: null, items: [article("https://example.com/hidden", "Paused article", "2026-10-18T09:00:00Z")] },
  ],
};

describe("资讯筛选", () => {
  it("保留失败来源缓存，排除暂停来源并按链接去重、日期排序", () => {
    expect(selectNews(snapshot, "all", "all", "", 0, now).map(item => item.title)).toEqual(["New model", "Agent workflow", "Old model", "Undated"]);
  });
  it("组合分类、来源、搜索和时间范围，未知时间只出现在全部时间", () => {
    expect(selectNews(snapshot, "all", "all", "", 1, now)).toHaveLength(2);
    expect(selectNews(snapshot, "technology", "github", " WORKFLOW ", 7, now)).toHaveLength(1);
    expect(selectNews(snapshot, "models", "github", "workflow", 0, now)).toHaveLength(0);
    expect(selectNews(snapshot, "all", "official", "Official", 30, now)).toHaveLength(1);
    expect(selectNews(null, "all", "all", "", 0, now)).toEqual([]);
  });
  it("组合领域与译文搜索，并兼容没有领域的旧数据", () => {
    const translated = structuredClone(snapshot);
    translated.sources[0].group = "models";
    translated.sources[0].items[0].translation = { title: "智能体新模型", summary: "增强推理", translated: true };
    expect(selectNews(translated, "all", "all", "推理", 0, now, "models").map(item => item.title)).toEqual(["New model"]);
    expect(selectNews(translated, "all", "all", "推理", 0, now, "frameworks")).toHaveLength(0);
    expect(selectNews(snapshot, "all", "all", "workflow", 0, now, "custom")).toHaveLength(1);
  });
});

describe("资讯推荐", () => {
  it("近期多来源轮选，失败来源仍可推荐缓存，排除旧消息和无效日期", () => {
    const input = structuredClone(snapshot);
    input.sources[0].items.push(article("https://example.com/newer", "Newer release", "2026-10-18T09:00:00Z"));
    input.sources[0].items.push(article("https://example.com/future", "Future release", "2027-10-18T09:00:00Z"));
    input.sources[0].items.push(article("https://example.com/invalid", "Invalid date", "invalid"));
    expect(recommendNews(input, now).map(item => item.title)).toEqual(["Newer release", "Agent workflow", "New model"]);
  });
  it("高频来源不会挤掉其他来源，最多八条；空订阅无虚构内容", () => {
    const input = structuredClone(snapshot);
    input.sources[0].items = Array.from({ length: 12 }, (_, index) => article(`https://example.com/release/${index}`, `Release ${index}`, new Date(now - index * 1000).toISOString()));
    const result = recommendNews(input, now);
    expect(result).toHaveLength(8);
    expect(result[1].sourceId).toBe("github");
    expect(new Set(result.map(item => item.url)).size).toBe(8);
    expect(recommendNews(null, now)).toEqual([]);
    input.sources.forEach(source => { source.enabled = false; });
    expect(recommendNews(input, now)).toEqual([]);
  });
});
