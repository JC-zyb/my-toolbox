import { useEffect, useMemo, useRef, useState } from "react";
import { ChevronDown, ChevronLeft, ChevronRight, ChevronUp, Newspaper, Pause, Play } from "lucide-react";
import { desktop } from "../api";
import { newsCategories, newsText, recommendNews, useNews, type NewsArticle } from "../news";
import { NewsPreviewDialog } from "./NewsPreviewDialog";
import "./news-recommendations.css";

const ROTATION_MS = 8_000;

export function NewsRecommendations({ onBrowse }: { onBrowse: () => void }) {
  const { snapshot, busy, error, translating, translationError, translate } = useNews();
  const [expanded, setExpanded] = useState(false);
  const [activeUrl, setActiveUrl] = useState<string | null>(null);
  const [paused, setPaused] = useState(() => matchMedia("(prefers-reduced-motion: reduce)").matches);
  const [hovered, setHovered] = useState(false);
  const [focused, setFocused] = useState(false);
  const [visible, setVisible] = useState(() => !document.hidden);
  const [selected, setSelected] = useState<NewsArticle | null>(null);
  const previewButton = useRef<HTMLButtonElement>(null);
  const recommendations = useMemo(() => recommendNews(snapshot), [snapshot]);
  const index = Math.max(0, recommendations.findIndex(item => item.url === activeUrl));
  const item = recommendations[index];
  const mode = snapshot?.language ?? "zh";
  const translation = mode !== "original" ? item?.translation : null;
  const title = item ? newsText(translation?.title ?? item.title) : "";
  const summary = item ? newsText(translation?.summary ?? item.summary) : "";
  const canRotate = expanded && recommendations.length > 1 && !paused && !hovered && !focused && visible && !selected;

  useEffect(() => {
    if (item?.url && item.url !== activeUrl) setActiveUrl(item.url);
  }, [item?.url, activeUrl]);

  useEffect(() => {
    const update = () => setVisible(!document.hidden);
    const preference = matchMedia("(prefers-reduced-motion: reduce)");
    const reduce = () => { if (preference.matches) setPaused(true); };
    document.addEventListener("visibilitychange", update);
    preference.addEventListener("change", reduce);
    return () => { document.removeEventListener("visibilitychange", update); preference.removeEventListener("change", reduce); };
  }, []);

  useEffect(() => {
    if (!canRotate) return;
    const timer = setTimeout(() => setActiveUrl(recommendations[(index + 1) % recommendations.length].url), ROTATION_MS);
    return () => clearTimeout(timer);
  }, [canRotate, recommendations, index]);

  useEffect(() => {
    if (expanded && visible && item && !busy && mode !== "original" && !translating.length && !translationError && !item.translation) void translate([item.url]);
  }, [expanded, visible, item, busy, mode, translating.length, translationError, translate]);

  const move = (direction: number) => {
    if (recommendations.length > 1) setActiveUrl(recommendations[(index + direction + recommendations.length) % recommendations.length].url);
  };
  const browse = () => { setExpanded(false); onBrowse(); };

  return <>
    <aside className={`news-recommendations ${expanded ? "expanded" : "collapsed"}`} aria-label="资讯推荐" onMouseEnter={() => setHovered(true)} onMouseLeave={() => setHovered(false)} onFocusCapture={() => setFocused(true)} onBlurCapture={event => { if (!event.currentTarget.contains(event.relatedTarget)) setFocused(false); }} onKeyDown={event => {
      if (!expanded) return;
      if (event.key === "ArrowLeft" || event.key === "ArrowRight") { event.preventDefault(); move(event.key === "ArrowLeft" ? -1 : 1); }
      if (event.key === "Escape") { setExpanded(false); event.currentTarget.querySelector<HTMLButtonElement>(".news-recommendation-toggle")?.focus(); }
    }}>
      <button className="news-recommendation-toggle" aria-label={expanded ? "收起资讯推荐" : "展开资讯推荐"} title={expanded ? "收起资讯推荐" : "展开资讯推荐"} aria-expanded={expanded} aria-controls="news-recommendation-panel" onClick={() => setExpanded(value => !value)}>
        <Newspaper size={17} aria-hidden="true" /><span>资讯推荐</span><span className="news-recommendation-count">{busy && !item ? "更新中" : `${recommendations.length} 条`}</span>{expanded ? <ChevronDown size={16} aria-hidden="true" /> : <ChevronUp size={16} aria-hidden="true" />}
      </button>
      <div id="news-recommendation-panel" hidden={!expanded} className="news-recommendation-panel" role="region" aria-roledescription="轮播" aria-label="近期资讯精选">
        {expanded && <>
        {item ? <>
          <article className="news-recommendation-slide" aria-roledescription="幻灯片" aria-label={`第 ${index + 1} 条，共 ${recommendations.length} 条`}>
            <div className="news-recommendation-meta"><span>{newsCategories[item.category]}</span><time dateTime={item.publishedAt!}>{new Date(item.publishedAt!).toLocaleDateString("zh-CN", { timeZone: "Asia/Shanghai", month: "2-digit", day: "2-digit" })}</time></div>
            <h2><button className="news-recommendation-title" ref={previewButton} onClick={() => setSelected(item)} title={title}>{title}</button></h2>
            <p className="news-recommendation-summary">{summary || "该来源仅提供标题，打开预览查看详情。"}</p>
            {mode === "bilingual" && translation?.translated && <p className="news-recommendation-original">{newsText(item.title)}</p>}
            <div className="news-recommendation-source"><span title={item.sourceName}>{item.sourceName}</span><span>{translation?.translated ? "机器翻译" : item.sourceLanguage === "zh" ? "中文" : translating.includes(item.url) ? "正在翻译…" : ""}</span></div>
            <button className="news-recommendation-read" onClick={() => { previewButton.current?.focus(); setSelected(item); }}>阅读预览<ChevronRight size={14} aria-hidden="true" /></button>
          </article>
          {mode !== "original" && translationError && !translation && <div className="news-recommendation-error" role="status">中文翻译暂不可用，当前显示原文。<button onClick={() => void translate([item.url], true)}>重试</button></div>}
          <div className="news-recommendation-controls">
            <button aria-label="上一条推荐" disabled={recommendations.length < 2} onClick={() => move(-1)}><ChevronLeft size={17} aria-hidden="true" /></button>
            <span className="news-recommendation-position" aria-live="off">{index + 1} / {recommendations.length}</span>
            <button aria-label="下一条推荐" disabled={recommendations.length < 2} onClick={() => move(1)}><ChevronRight size={17} aria-hidden="true" /></button>
            <button className="news-recommendation-play" aria-label={paused ? "播放资讯轮播" : "暂停资讯轮播"} disabled={recommendations.length < 2} onClick={() => setPaused(value => !value)}>{paused ? <Play size={14} aria-hidden="true" /> : <Pause size={14} aria-hidden="true" />}<span>{recommendations.length < 2 ? "单条" : paused ? "已暂停" : canRotate ? "轮播中" : "阅读中"}</span></button>
            <button className="news-recommendation-browse" onClick={browse}>全部资讯</button>
          </div>
        </> : <div className="news-recommendation-empty"><Newspaper size={26} aria-hidden="true" /><p>{!desktop ? "请在桌面版获取最新资讯" : busy ? "正在获取最新资讯…" : error ? "资讯读取失败，请查看来源诊断" : "最近 30 天暂无可推荐资讯"}</p><button onClick={browse}>进入资讯专栏<ChevronRight size={14} aria-hidden="true" /></button></div>}
        </>}
      </div>
    </aside>
    <NewsPreviewDialog article={selected} onClose={() => { setSelected(null); requestAnimationFrame(() => previewButton.current?.focus()); }} />
  </>;
}
