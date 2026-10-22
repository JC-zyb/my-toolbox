import { create } from "zustand";
import { command, desktop } from "./api";

export type NewsCategory = "models" | "features" | "technology";
export const newsCategories: Record<NewsCategory, string> = {
  models: "模型发布", features: "功能更新", technology: "Agent 技术",
};
export const newsGroups: Record<string, string> = { media: "中文资讯", general: "通用与办公 Agent", coding: "编程 Agent", frameworks: "Agent 框架", platforms: "工作流与平台", models: "模型生态", protocols: "协议与工具", custom: "其他订阅" };
export type NewsLanguage = "zh" | "original" | "bilingual";
export interface NewsTranslation { title: string; summary: string; translated: boolean }
export interface NewsItem {
  url: string;
  title: string;
  summary: string;
  category: NewsCategory;
  publishedAt: string | null;
  translation?: NewsTranslation | null;
}
export interface NewsSource {
  id: string;
  name: string;
  url: string;
  category: NewsCategory;
  enabled: boolean;
  lastAttempt: string | null;
  lastSuccess: string | null;
  error: string | null;
  items: NewsItem[];
  group?: string;
  language?: string;
  custom?: boolean;
}
export interface NewsSnapshot { automatic: boolean; sources: NewsSource[]; language?: NewsLanguage }
export interface NewsPreview { url: string; title: string; sourceName: string; content: string; translatedContent: string | null; kind: "feed" | "web"; diagnostic: string | null }
interface TranslationResult { url: string; translation: NewsTranslation | null; error: string | null }

interface NewsState {
  snapshot: NewsSnapshot | null;
  initialized: boolean;
  busy: boolean;
  error: string | null;
  translating: string[];
  translationError: string | null;
  initialize: () => Promise<void>;
  refresh: (force?: boolean) => Promise<void>;
  preferences: (automatic: boolean, enabled: string[]) => Promise<void>;
  language: (language: NewsLanguage) => Promise<void>;
  translate: (urls: string[], retry?: boolean) => Promise<void>;
  addSource: (name: string, url: string, group: string) => Promise<boolean>;
  removeSource: (id: string) => Promise<void>;
}

export const useNews = create<NewsState>((set, get) => ({
  snapshot: null, initialized: false, busy: false, error: null, translating: [], translationError: null,
  initialize: async () => {
    if (!desktop || get().initialized) return;
    set({ initialized: true, busy: true });
    try {
      set({ snapshot: await command<NewsSnapshot>("get_agent_news"), busy: false });
      await get().refresh();
    } catch (error) { set({ error: String(error), busy: false }); }
  },
  refresh: async (force = false) => {
    if (!desktop || get().busy || (!force && get().snapshot?.automatic === false)) return;
    set({ busy: true, error: null });
    try {
      set({ snapshot: await command<NewsSnapshot>("refresh_agent_news", { force }), busy: false });
    } catch (error) { set({ error: String(error), busy: false }); }
  },
  preferences: async (automatic, enabled) => {
    if (get().busy) return;
    set({ busy: true, error: null });
    try {
      set({ snapshot: await command<NewsSnapshot>("set_agent_news_preferences", { automatic, enabled }), busy: false });
      await get().refresh();
    } catch (error) { set({ error: String(error), busy: false }); }
  },
  language: async language => {
    if (get().busy) return;
    set({ busy: true, error: null });
    try { set({ snapshot: await command<NewsSnapshot>("set_agent_news_language", { language }), busy: false }); }
    catch (error) { set({ error: String(error), busy: false }); }
  },
  translate: async (urls, retry = false) => {
    const state = get();
    if (!desktop || !state.snapshot || state.translating.length || state.snapshot.language === "original" || (state.translationError && !retry)) return;
    const originals = new Map(state.snapshot.sources.filter(source => source.enabled).flatMap(source => source.items).map(item => [item.url, item]));
    const pending = [...new Set(urls)].filter(url => originals.has(url) && !originals.get(url)!.translation).slice(0, 6);
    if (!pending.length) return;
    set({ translating: pending, translationError: null });
    try {
      const results = await command<TranslationResult[]>("translate_agent_news", { urls: pending });
      const translated = new Map(results.map(result => [result.url, result]));
      set(state => ({
        translating: [], translationError: results.find(result => result.error)?.error ?? null,
        snapshot: state.snapshot ? { ...state.snapshot, sources: state.snapshot.sources.map(source => ({ ...source, items: source.items.map(item => {
          const original = originals.get(item.url);
          const result = translated.get(item.url);
          return result?.translation && item.title === original?.title && item.summary === original?.summary ? { ...item, translation: result.translation } : item;
        }) })) } : null,
      }));
    } catch (error) { set({ translating: [], translationError: String(error) }); }
  },
  addSource: async (name, url, group) => {
    if (get().busy) return false;
    set({ busy: true, error: null });
    try { set({ snapshot: await command<NewsSnapshot>("add_agent_news_source", { name, url, group }), busy: false }); return true; }
    catch (error) { set({ error: String(error), busy: false }); return false; }
  },
  removeSource: async id => {
    if (get().busy) return;
    set({ busy: true, error: null });
    try { set({ snapshot: await command<NewsSnapshot>("remove_agent_news_source", { id }), busy: false }); }
    catch (error) { set({ error: String(error), busy: false }); }
  },
}));

export function selectNews(snapshot: NewsSnapshot | null, category: string, sourceId: string, query: string, days: number, now = Date.now(), group = "all") {
  const seen = new Set<string>();
  const needle = query.trim().toLocaleLowerCase();
  return (snapshot?.sources ?? []).filter(source => source.enabled && (sourceId === "all" || source.id === sourceId) && (group === "all" || (source.group ?? "custom") === group))
    .flatMap(source => source.items.map(item => ({ ...item, sourceId: source.id, sourceName: source.name, sourceLanguage: source.language ?? "auto", sourceGroup: source.group ?? "custom" })))
    .filter(item => {
      if (category !== "all" && item.category !== category) return false;
      if (needle && !`${item.title} ${item.summary} ${item.translation?.title ?? ""} ${item.translation?.summary ?? ""} ${item.sourceName}`.toLocaleLowerCase().includes(needle)) return false;
      const published = item.publishedAt ? Date.parse(item.publishedAt) : NaN;
      if (days && (!Number.isFinite(published) || published < now - days * 86400000 || published > now + 300000)) return false;
      if (seen.has(item.url)) return false;
      seen.add(item.url);
      return true;
    })
    .sort((a, b) => (b.publishedAt ? Date.parse(b.publishedAt) || 0 : 0) - (a.publishedAt ? Date.parse(a.publishedAt) || 0 : 0) || a.url.localeCompare(b.url));
}

export function newsText(value: string): string {
  const document = new DOMParser().parseFromString(value, "text/html");
  document.querySelectorAll("script,style,template").forEach(element => element.remove());
  document.querySelectorAll("br,p,div,li,h1,h2,h3,pre").forEach(element => element.append(" "));
  return (document.body.textContent ?? "").replace(/\s+/g, " ").trim();
}

export type NewsArticle = ReturnType<typeof selectNews>[number];
const MAX_RECOMMENDATIONS = 8;

export function recommendNews(snapshot: NewsSnapshot | null, now = Date.now()): NewsArticle[] {
  const recent = selectNews(snapshot, "all", "all", "", 30, now);
  const sources = new Map<string, NewsArticle[]>();
  for (const item of recent) {
    const group = sources.get(item.sourceId) ?? [];
    group.push(item);
    sources.set(item.sourceId, group);
  }
  const recommendations: NewsArticle[] = [];
  for (let round = 0; recommendations.length < MAX_RECOMMENDATIONS; round++) {
    let added = false;
    for (const group of sources.values()) {
      if (!group[round]) continue;
      recommendations.push(group[round]);
      added = true;
      if (recommendations.length === MAX_RECOMMENDATIONS) break;
    }
    if (!added) break;
  }
  return recommendations;
}
