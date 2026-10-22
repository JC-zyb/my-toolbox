import { useEffect, useMemo, useState } from "react";
import { AlertCircle, ArrowUpRight, CheckCircle2, ChevronDown, Github, Languages, Newspaper, Plus, RefreshCw, Rss, Search, Settings2, Trash2 } from "lucide-react";
import { api, desktop } from "../api";
import { NewsPreviewDialog } from "./NewsPreviewDialog";
import { newsCategories, newsGroups, newsText, selectNews, useNews } from "../news";
import "./agent-news.css";

type Article = ReturnType<typeof selectNews>[number];

function timestamp(value: string | null) {
  return value ? new Date(value).toLocaleString("zh-CN", { timeZone: "Asia/Shanghai", month: "2-digit", day: "2-digit", hour: "2-digit", minute: "2-digit", year: "numeric", hour12: false }) : "尚未更新";
}

export function AgentNews() {
  const { snapshot, busy, error, translating, translationError, refresh, preferences, language, translate, addSource, removeSource } = useNews();
  const [category, setCategory] = useState("all");
  const [group, setGroup] = useState("all");
  const [source, setSource] = useState("all");
  const [query, setQuery] = useState("");
  const [days, setDays] = useState(30);
  const [limit, setLimit] = useState(30);
  const [showSources, setShowSources] = useState(false);
  const [sourceQuery, setSourceQuery] = useState("");
  const [sourceName, setSourceName] = useState("");
  const [sourceUrl, setSourceUrl] = useState("");
  const [sourceGroup, setSourceGroup] = useState("custom");
  const [openError, setOpenError] = useState<string | null>(null);
  const [selected, setSelected] = useState<Article | null>(null);
  const mode = snapshot?.language ?? "zh";
  const sources = snapshot?.sources ?? [];
  const enabled = sources.filter(item => item.enabled);
  const failed = enabled.filter(item => item.error);
  const items = useMemo(() => selectNews(snapshot, category, source, query, days, Date.now(), group), [snapshot, category, source, query, days, group]);
  const counts = useMemo(() => selectNews(snapshot, "all", source, query, days, Date.now(), group), [snapshot, source, query, days, group]);
  const lastAttempt = enabled.map(item => item.lastAttempt).filter((value): value is string => Boolean(value)).sort().at(-1);

  useEffect(() => {
    if (mode !== "original" && !busy && !translationError && !translating.length) void translate(items.slice(0, limit).map(item => item.url));
  }, [items, limit, mode, busy, translating.length, translationError, translate]);

  const open = async (url: string) => {
    setOpenError(null);
    try { await api.open(url); } catch (error) { setOpenError(String(error)); }
  };
  const changeFilter = (change: () => void) => { change(); setLimit(30); };

  return <section className="agent-news" aria-labelledby="agent-news-title">
    <div className="news-heading">
      <div className="news-title"><Newspaper size={26} aria-hidden="true" /><div><h1 id="agent-news-title">Agent 资讯</h1><span className="news-muted">{counts.length} 条资讯 · {enabled.length} 个订阅来源</span></div></div>
      <div className="news-actions">
        <label className="news-auto"><input type="checkbox" checked={snapshot?.automatic ?? true} disabled={!desktop || busy || !snapshot} onChange={event => void preferences(event.target.checked, enabled.map(item => item.id))} />自动更新</label>
        <button className="button secondary" disabled={!desktop || busy} onClick={() => void refresh(true)}><RefreshCw size={15} className={busy ? "news-spinning" : ""} aria-hidden="true" />{busy ? "更新中" : "刷新资讯"}</button>
        <button className="button secondary" aria-expanded={showSources} onClick={() => setShowSources(!showSources)}><Settings2 size={15} aria-hidden="true" />订阅来源</button>
      </div>
    </div>
    {!desktop && <div className="notice" role="status">资讯采集需要桌面环境，请使用桌面版个人工具箱。</div>}
    {(error || openError) && <div className="notice error" role="alert"><AlertCircle size={17} aria-hidden="true" /><span>{openError ?? error}</span></div>}
    {failed.length > 0 && <div className="news-warning" role="status"><AlertCircle size={16} aria-hidden="true" /><span>{failed.length} 个来源更新失败，保留已有资讯</span><button className="text-button" onClick={() => setShowSources(true)}>查看来源</button></div>}

    {showSources && <div className="news-subscriptions">
      <form className="news-add-source" onSubmit={event => { event.preventDefault(); void addSource(sourceName, sourceUrl, sourceGroup).then(added => { if (added) { setSourceName(""); setSourceUrl(""); } }); }}>
        <input aria-label="订阅名称" placeholder="订阅名称（可选）" maxLength={80} value={sourceName} onChange={event => setSourceName(event.target.value)} />
        <input aria-label="订阅地址" placeholder="HTTPS RSS / Atom 或 GitHub 仓库地址" type="url" required value={sourceUrl} onChange={event => setSourceUrl(event.target.value)} />
        <select aria-label="订阅领域" value={sourceGroup} onChange={event => setSourceGroup(event.target.value)}>{Object.entries(newsGroups).map(([id, label]) => <option value={id} key={id}>{label}</option>)}</select>
        <button type="submit" className="button secondary" disabled={busy || !desktop}><Plus size={15} aria-hidden="true" />添加订阅</button>
      </form>
      <label className="news-search"><Search size={16} aria-hidden="true" /><input aria-label="搜索订阅来源" placeholder="搜索订阅来源" value={sourceQuery} onChange={event => setSourceQuery(event.target.value)} /></label>
      <div className="news-sources" aria-label="订阅来源状态">
        {sources.filter(item => `${item.name} ${newsGroups[item.group ?? "custom"]}`.toLowerCase().includes(sourceQuery.toLowerCase())).map(item => <div className="news-source" key={item.id}>
          <div className="news-source-name"><label><input type="checkbox" checked={item.enabled} disabled={busy} onChange={event => {
            if (!event.target.checked && source === item.id) setSource("all");
            void preferences(snapshot!.automatic, event.target.checked ? [...enabled.map(source => source.id), item.id] : enabled.filter(source => source.id !== item.id).map(source => source.id));
          }} />{item.name}</label><div className="news-source-tools"><button className="icon-button" aria-label={`打开 ${item.name} 订阅`} title="打开订阅" onClick={() => void open(item.url)}><ArrowUpRight size={15} /></button>{item.custom && <button className="icon-button" title="移除订阅" aria-label={`移除 ${item.name} 订阅`} disabled={busy} onClick={() => { if (source === item.id) setSource("all"); void removeSource(item.id); }}><Trash2 size={15} /></button>}</div></div>
          <span className="news-muted">{newsGroups[item.group ?? "custom"]}{item.language === "zh" ? " · 中文" : ""}</span>
          <div className={`news-source-state ${item.enabled && item.error ? "news-source-error" : ""}`}>
            {item.enabled ? item.error ? <AlertCircle size={14} aria-hidden="true" /> : item.lastSuccess ? <CheckCircle2 size={14} aria-hidden="true" /> : <Rss size={14} aria-hidden="true" /> : null}
            <span>{!item.enabled ? "已暂停" : item.error ?? (item.lastSuccess ? `${item.items.length} 条已缓存` : "等待首次更新")}</span>
          </div>
          <span className="news-muted">上次成功：{timestamp(item.lastSuccess)}</span>
        </div>)}
      </div>
    </div>}

    <div className="news-reading-options">
      <div className="news-tabs news-language" role="group" aria-label="资讯语言"><Languages size={16} aria-hidden="true" />{([ ["zh", "中文"], ["original", "原文"], ["bilingual", "双语"] ] as const).map(([value, label]) => <button key={value} disabled={!desktop || busy} aria-pressed={mode === value} className={mode === value ? "active" : ""} onClick={() => void language(value)}>{label}</button>)}</div>
      {translating.length > 0 && mode !== "original" && <span className="news-muted" role="status">正在翻译中文…</span>}
    </div>
    {translationError && mode !== "original" && <div className="news-warning" role="status"><AlertCircle size={16} aria-hidden="true" /><span>中文翻译暂不可用：{translationError}</span><button className="text-button" onClick={() => void translate(items.slice(0, limit).map(item => item.url), true)}>重试翻译</button></div>}
    <div className="news-toolbar">
      <div className="news-tabs" role="group" aria-label="资讯分类">
        {[["all", "全部"], ...Object.entries(newsCategories)].map(([id, label]) => <button key={id} aria-pressed={category === id} className={category === id ? "active" : ""} onClick={() => changeFilter(() => setCategory(id))}>{label}<span>{id === "all" ? counts.length : counts.filter(item => item.category === id).length}</span></button>)}
      </div>
      <div className="news-filters">
        <label className="news-search"><Search size={16} aria-hidden="true" /><input aria-label="搜索资讯" placeholder="搜索资讯" value={query} onChange={event => changeFilter(() => setQuery(event.target.value))} /></label>
        <select aria-label="资讯领域" value={group} onChange={event => changeFilter(() => { setGroup(event.target.value); setSource("all"); })}><option value="all">全部领域</option>{Object.entries(newsGroups).map(([id, label]) => <option value={id} key={id}>{label}</option>)}</select>
        <select aria-label="资讯来源" value={source} onChange={event => changeFilter(() => setSource(event.target.value))}><option value="all">全部来源</option>{enabled.filter(item => group === "all" || (item.group ?? "custom") === group).map(item => <option value={item.id} key={item.id}>{item.name}</option>)}</select>
        <select aria-label="资讯时间范围" value={days} onChange={event => changeFilter(() => setDays(Number(event.target.value)))}><option value={1}>最近 24 小时</option><option value={7}>最近 7 天</option><option value={30}>最近 30 天</option><option value={0}>全部时间</option></select>
      </div>
    </div>
    <div className="news-list-status" aria-live="polite"><span>{busy ? "正在检查订阅…" : `共 ${items.length} 条`}</span><span>最近检查：{timestamp(lastAttempt ?? null)}</span></div>
    <div className="news-list" aria-busy={busy}>
      {items.slice(0, limit).map(item => {
        const translated = mode !== "original" ? item.translation : null;
        return <article className="news-item" key={item.url}>
          <div className="news-item-meta"><span className="news-publisher">{item.url.startsWith("https://github.com/") ? <Github size={14} aria-hidden="true" /> : <Rss size={14} aria-hidden="true" />}{item.sourceName}</span><span className={`news-category news-category-${item.category}`}>{newsCategories[item.category]}</span><span>{newsGroups[item.sourceGroup]}</span><time dateTime={item.publishedAt ?? undefined}>{item.publishedAt ? timestamp(item.publishedAt) : "发布时间未知"}</time>{translated?.translated ? <span>机器翻译</span> : item.sourceLanguage === "zh" ? <span>中文</span> : null}</div>
          <h2><button onClick={() => { setOpenError(null); setSelected(item); }} title="预览资讯">{newsText(translated?.title ?? item.title)}</button></h2>
          {(translated?.summary || item.summary) && <p>{newsText(translated?.summary ?? item.summary)}</p>}
          {mode === "bilingual" && translated?.translated && <div className="news-original"><p lang="en">{newsText(item.title)}</p><p>{newsText(item.summary)}</p></div>}
        </article>;
      })}
      {!items.length && <div className="news-empty" role="status"><Newspaper size={32} aria-hidden="true" /><h2>{busy ? "正在获取最新资讯" : !snapshot ? "尚无资讯" : !enabled.length ? "订阅已全部暂停" : failed.length === enabled.length && !counts.length ? "暂时无法获取资讯" : "没有匹配的资讯"}</h2>{query || group !== "all" || source !== "all" || category !== "all" || days !== 0 ? <button className="text-button" onClick={() => { setQuery(""); setGroup("all"); setSource("all"); setCategory("all"); setDays(0); }}>清除筛选</button> : null}</div>}
    </div>
    {items.length > limit && <button className="news-more" onClick={() => setLimit(limit + 30)}><ChevronDown size={16} aria-hidden="true" />加载更多 · 还有 {items.length - limit} 条</button>}
    <NewsPreviewDialog article={selected} onClose={() => setSelected(null)} />
  </section>;
}
