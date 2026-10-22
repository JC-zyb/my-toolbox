import { useEffect, useState } from "react";
import { AlertCircle, ArrowUpRight } from "lucide-react";
import { api, command, desktop } from "../api";
import { Modal } from "../components";
import { newsText, useNews, type NewsArticle, type NewsPreview } from "../news";
import "./agent-news.css";

export function NewsPreviewDialog({ article, onClose }: { article: NewsArticle | null; onClose: () => void }) {
  const snapshot = useNews(state => state.snapshot);
  const mode = snapshot?.language ?? "zh";
  const [preview, setPreview] = useState<NewsPreview | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [attempt, setAttempt] = useState(0);
  const url = article?.url;
  useEffect(() => {
    setPreview(null);
    setError(null);
    if (!url || !desktop) return;
    let current = true;
    setBusy(true);
    void command<NewsPreview>("preview_agent_news", { url }).then(value => {
      if (current) { setPreview(value); setBusy(false); }
    }, error => {
      if (current) { setError(String(error)); setBusy(false); }
    });
    return () => { current = false; };
  }, [url, mode, attempt]);

  const latest = snapshot?.sources.flatMap(source => source.items).find(item => item.url === url) ?? article;
  const title = latest ? newsText(mode !== "original" && latest.translation ? latest.translation.title : latest.title) : "内容预览";
  const original = preview?.content ?? (article ? newsText(article.summary) : "");
  const chinese = preview ? preview.translatedContent : latest?.translation?.summary;
  return <Modal open={Boolean(article)} onClose={onClose} title={title} description={article ? `${article.sourceName}${article.publishedAt ? ` · ${new Date(article.publishedAt).toLocaleString("zh-CN", { timeZone: "Asia/Shanghai", month: "2-digit", day: "2-digit", hour: "2-digit", minute: "2-digit", year: "numeric", hour12: false })}` : " · 发布时间未知"}` : ""} wide error={error}>
    {article && <div className="news-preview">
      <div className="news-preview-actions"><span className="news-muted">{preview?.kind === "web" ? "网页摘录" : "订阅内容"}{mode !== "original" && preview?.translatedContent ? " · 机器翻译" : ""}</span><button className="button secondary" onClick={() => { setError(null); void api.open(article.url).catch(error => setError(String(error))); }}><ArrowUpRight size={16} aria-hidden="true" />打开原网页</button></div>
      {busy && <div className="news-muted" role="status">正在读取内容…</div>}
      {preview?.diagnostic && <div className="news-warning" role="status"><AlertCircle size={16} aria-hidden="true" /><span>{preview.diagnostic}</span><button className="text-button" onClick={() => setAttempt(value => value + 1)}>重试</button></div>}
      <div className="news-preview-content">{(mode !== "original" && chinese ? chinese : original) || "该来源仅提供标题，可打开原网页阅读。"}</div>
      {mode !== "original" && chinese && chinese !== original && <details className="news-preview-original" open={mode === "bilingual"}><summary>原文内容</summary><div className="news-preview-content">{original}</div></details>}
      {error && <button className="text-button" onClick={() => setAttempt(value => value + 1)}>重试读取</button>}
    </div>}
  </Modal>;
}
