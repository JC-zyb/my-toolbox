import {
  ArrowUpRight,
  FolderOpen,
  Pause,
  Play,
  Plus,
  Settings2,
  ShieldCheck,
} from "lucide-react";
import { AgentMark, Empty, Panel, Status } from "../components";
import { aggregate, dateTime, compact, money } from "../stats";
import { useStore } from "../store";
import { api } from "../api";
import type { SourceStatus, UsageRecord } from "../types";

export function Agents({
  records,
  onSource,
  perform,
}: {
  records: UsageRecord[];
  onSource: (s: SourceStatus) => void;
  perform: (fn: () => Promise<unknown>) => void;
}) {
  const { snapshot, settings, source: filter, save, setPage } = useStore();
  const sources = (snapshot?.sources ?? []).filter(
    (s) => filter === "all" || s.id === filter,
  );
  return (
    <div className="page-flow">
      <div className="page-intro">
        <div>
          <span className="eyebrow">客户端与本地来源</span>
          <h1>
            Agents<span className="heading-dot">.</span>
          </h1>
          <p>沿用你熟悉的客户端，在这里统一查看使用情况。</p>
        </div>
        <button className="button primary" onClick={() => setPage("sources")}>
          <Plus size={16} />
          连接数据源
        </button>
      </div>
      <div className="agent-cards">
        {sources.map((s) => {
          const stats = aggregate(records.filter((r) => r.agent === s.id));
          return (
            <section className="agent-card" key={s.id}>
              <div className="agent-card-heading">
                <AgentMark agent={s.id} large />
                <div className="grow">
                  <h2>{s.name}</h2>
                  <p>
                    {s.version ? `版本 ${s.version}` : "客户端版本未读取"} ·{" "}
                    {s.executable ? "已登记启动路径" : "尚未登记启动路径"}
                  </p>
                </div>
                <Status status={s.status} />
              </div>
              <div className="agent-stats">
                <div>
                  <span>所选范围 Token</span>
                  <strong>{stats.sessions ? compact(stats.total) : "—"}</strong>
                </div>
                <div>
                  <span>会话数</span>
                  <strong>{stats.sessions || "—"}</strong>
                </div>
                <div>
                  <span>已知成本 · USD</span>
                  <strong>{stats.priced ? money(stats.cost) : "未知"}</strong>
                </div>
                <div>
                  <span>最近活动</span>
                  <strong className="date-value">
                    {s.lastActivity ? dateTime(s.lastActivity) : "未记录"}
                  </strong>
                </div>
              </div>
              <div className="source-path">
                <FolderOpen size={16} />
                <span title={s.root}>{s.root}</span>
              </div>
              <div className="agent-card-footer">
                <span>
                  {s.files} 个文件 · 最后成功 {dateTime(s.lastScan)}
                </span>
                <div className="button-row">
                  <button
                    className="button"
                    onClick={() => perform(() => api.launch(s.id))}
                  >
                    <ArrowUpRight size={15} />
                    打开客户端
                  </button>
                  <button
                    className="icon-button"
                    aria-label={`打开 ${s.name} 数据目录`}
                    onClick={() => perform(() => api.open(s.root))}
                  >
                    <FolderOpen size={18} />
                  </button>
                  <button
                    className="icon-button"
                    aria-label={`${s.name} 来源设置`}
                    onClick={() => onSource(s)}
                  >
                    <Settings2 size={18} />
                  </button>
                  <button
                    className="icon-button"
                    aria-label={`${s.enabled ? "暂停" : "启用"} ${s.name} 采集`}
                    onClick={() =>
                      perform(async () => {
                        await save({
                          ...settings,
                          sources: settings.sources.map((x) =>
                            x.id === s.id ? { ...x, enabled: !x.enabled } : x,
                          ),
                        });
                        await useStore.getState().refresh();
                      })
                    }
                  >
                    {s.enabled ? <Pause size={18} /> : <Play size={18} />}
                  </button>
                </div>
              </div>
              {s.diagnostics.length > 0 && (
                <p className="inline-warning">{s.diagnostics[0]}</p>
              )}
            </section>
          );
        })}
      </div>
      {!sources.length && (
        <Panel title="连接你的第一个 Agent">
          <Empty
            title="客户端尚未连接"
            description="在桌面版中检测 Codex、Claude Code 和 DSH，或手动指定数据目录。"
            action={
              <button className="button" onClick={() => setPage("sources")}>
                前往数据源
                <ArrowRightIcon />
              </button>
            }
          />
        </Panel>
      )}
      <div className="info-strip">
        <ShieldCheck size={18} />
        <span>只读取会话元数据和用量，不接管登录，不改变模型请求。</span>
      </div>
    </div>
  );
}
function ArrowRightIcon() {
  return <ArrowUpRight size={15} />;
}
