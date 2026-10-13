import { useEffect, useState } from "react";
import {
  ChevronLeft,
  ChevronRight,
  Download,
  Search,
  ArrowUpRight,
} from "lucide-react";
import { AgentMark, Empty, Modal, Panel } from "../components";
import {
  aggregate,
  agentNames,
  compact,
  csv,
  dateTime,
  money,
  number,
  sessionRows,
} from "../stats";
import { exportCsv } from "../api";
import { useStore } from "../store";
import type { UsageRecord } from "../types";

export function Sessions({
  records,
  onSession,
  perform,
}: {
  records: UsageRecord[];
  onSession: (r: UsageRecord) => void;
  perform: (fn: () => Promise<unknown>) => void;
}) {
  const [query, setQuery] = useState(""),
    [page, setPage] = useState(0);
  const { modelFilter: model, patch } = useStore();
  const setModel = (value: string) => patch({ modelFilter: value });
  useEffect(() => setPage(0), [query, model, records]);
  const rows = sessionRows(
    records.filter(
      (r) =>
        (model === "all" || r.model === model) &&
        `${r.project} ${r.sessionId} ${r.model} ${agentNames[r.agent]}`
          .toLowerCase()
          .includes(query.toLowerCase()),
    ),
  );
  const totalPages = Math.max(1, Math.ceil(rows.length / 20));
  return (
    <div className="page-flow">
      <div className="page-intro">
        <div>
          <span className="eyebrow">历史与用量明细</span>
          <h1>
            会话<span className="heading-dot">.</span>
          </h1>
          <p>追溯每一次工作，查看项目、模型与消耗。</p>
        </div>
        <button
          className="button"
          disabled={!rows.length}
          onClick={() =>
            perform(() =>
              exportCsv(
                csv([
                  [
                    "时间",
                    "Agent",
                    "会话ID",
                    "项目",
                    "模型",
                    "输入",
                    "输出",
                    "缓存读取",
                    "成本USD",
                  ],
                  ...rows.map((r) => [
                    r.timestamp,
                    agentNames[r.agent],
                    r.sessionId,
                    r.project,
                    [...new Set(r.rows.map((x) => x.model))].join(" / "),
                    String(r.stats.input),
                    String(r.stats.output),
                    String(r.stats.cached),
                    r.stats.priced ? String(r.stats.cost) : "未知",
                  ]),
                ]),
                "会话用量.csv",
              ),
            )
          }
        >
          <Download size={16} />
          导出 CSV
        </button>
      </div>
      <Panel
        title="会话记录"
        subtitle={`共 ${rows.length} 个会话 · 不包含对话正文`}
        action={
          <div className="session-filters">
            <label className="search-field">
              <Search size={16} />
              <input
                aria-label="搜索会话"
                placeholder="搜索项目、模型或会话 ID"
                value={query}
                onChange={(e) => setQuery(e.target.value)}
              />
            </label>
            <select
              aria-label="筛选模型"
              value={model}
              onChange={(e) => setModel(e.target.value)}
            >
              <option value="all">全部模型</option>
              {[...new Set(records.map((r) => r.model))].sort().map((m) => (
                <option key={m}>{m}</option>
              ))}
            </select>
          </div>
        }
      >
        {rows.length ? (
          <>
            <div className="table-wrap">
              <table className="session-table">
                <thead>
                  <tr>
                    <th>最后活动</th>
                    <th>Agent / 项目</th>
                    <th>模型</th>
                    <th>Token</th>
                    <th>成本依据</th>
                    <th>
                      <span className="sr-only">操作</span>
                    </th>
                  </tr>
                </thead>
                <tbody>
                  {rows.slice(page * 20, page * 20 + 20).map((r) => (
                    <tr key={`${r.agent}:${r.sessionId}`}>
                      <td className="nowrap">{dateTime(r.timestamp)}</td>
                      <td>
                        <div className="session-cell">
                          <AgentMark agent={r.agent} />
                          <div>
                            <strong>{agentNames[r.agent]}</strong>
                            <small title={r.project}>
                              {r.project
                                .split(/[\\/]/)
                                .filter(Boolean)
                                .at(-1) || "未记录项目"}
                            </small>
                          </div>
                        </div>
                      </td>
                      <td
                        className="model-cell"
                        title={[...new Set(r.rows.map((x) => x.model))].join(
                          ", ",
                        )}
                      >
                        {[...new Set(r.rows.map((x) => x.model))].join(", ")}
                      </td>
                      <td className="mono">{compact(r.stats.total)}</td>
                      <td>
                        {r.stats.priced ? (
                          <>
                            <strong className="mono">
                              {money(r.stats.cost)}
                            </strong>
                            <small>
                              来源记录
                              {r.stats.priced < r.rows.length
                                ? " · 部分未知"
                                : ""}
                            </small>
                          </>
                        ) : (
                          <span className="muted">未知价格</span>
                        )}
                      </td>
                      <td>
                        <button
                          className="icon-button"
                          aria-label={`查看会话 ${r.sessionId}`}
                          onClick={() => onSession(r)}
                        >
                          <ArrowUpRight size={17} />
                        </button>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
            <div className="pagination">
              <span>
                第 {page + 1} / {totalPages} 页
              </span>
              <button
                className="icon-button"
                aria-label="上一页"
                disabled={page === 0}
                onClick={() => setPage(page - 1)}
              >
                <ChevronLeft size={18} />
              </button>
              <button
                className="icon-button"
                aria-label="下一页"
                disabled={page + 1 >= totalPages}
                onClick={() => setPage(page + 1)}
              >
                <ChevronRight size={18} />
              </button>
            </div>
          </>
        ) : (
          <Empty
            title={records.length ? "没有匹配的会话" : "还没有会话记录"}
            description={
              records.length
                ? "调整搜索、模型或日期范围后再试。"
                : "连接本地数据源后，会话元数据会出现在这里。"
            }
          />
        )}
      </Panel>
    </div>
  );
}

export function SessionDetail({
  record,
  records,
  onClose,
}: {
  record: UsageRecord | null;
  records: UsageRecord[];
  onClose: () => void;
}) {
  const rows = record
    ? records.filter(
        (r) => r.agent === record.agent && r.sessionId === record.sessionId,
      )
    : [];
  const stats = aggregate(rows);
  return (
    <Modal
      open={!!record}
      title="会话详情"
      description="只读元数据 · 用量不是最终账单"
      onClose={onClose}
      wide
    >
      {record && (
        <>
          <div className="detail-lead">
            <AgentMark agent={record.agent} large />
            <div>
              <h2>{agentNames[record.agent]}</h2>
              <p className="mono break-all">{record.sessionId}</p>
            </div>
          </div>
          <dl className="detail-grid">
            {[
              ["项目", record.project || "未记录"],
              ["模型", [...new Set(rows.map((r) => r.model))].join(" / ")],
              ["开始记录", dateTime(rows.map((r) => r.startedAt).sort()[0])],
              [
                "最后活动",
                dateTime(
                  rows
                    .map((r) => r.timestamp)
                    .sort()
                    .at(-1) ?? null,
                ),
              ],
              ["输入 Token", number(stats.input)],
              ["输出 Token", number(stats.output)],
              ["缓存读取", number(stats.cached)],
              ["缓存创建", number(stats.cacheWrite)],
              [
                "推理 Token",
                rows.every((r) => r.usage.reasoning !== null)
                  ? number(
                      rows.reduce((s, r) => s + (r.usage.reasoning ?? 0), 0),
                    )
                  : "未知",
              ],
              [
                "成本",
                stats.priced
                  ? `${money(stats.cost)} · 来源记录${stats.priced < rows.length ? "（部分未知）" : ""}`
                  : "未知价格",
              ],
              ["父会话", record.parentId ?? "未记录"],
              ["来源文件", record.sourceFile],
            ].map(([label, value]) => (
              <div key={label}>
                <dt>{label}</dt>
                <dd>{value}</dd>
              </div>
            ))}
          </dl>
          <div className="info-strip">
            缓存读取和缓存创建属于输入总量；推理若已包含在输出中，不再次累计。
          </div>
        </>
      )}
    </Modal>
  );
}
