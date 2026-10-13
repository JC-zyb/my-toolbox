import { useState } from "react";
import {
  Area,
  AreaChart,
  CartesianGrid,
  XAxis,
  YAxis,
  Tooltip,
  ResponsiveContainer,
  Pie,
  PieChart,
  Cell,
} from "recharts";
import {
  ArrowDownLeft,
  ArrowUpRight,
  Coins,
  Layers3,
  MessageSquare,
  Activity,
  ArrowRight,
  FolderOpen,
  Terminal,
  Table2,
} from "lucide-react";
import { useStore } from "../store";
import {
  aggregate,
  agentNames,
  compact,
  comparison,
  csv,
  dateTime,
  dayOffset,
  groups,
  money,
  number,
  sessionRows,
} from "../stats";
import { AgentMark, Empty, Panel, SourceHealth } from "../components";
import type { SourceStatus, UsageRecord } from "../types";
import { api, exportCsv } from "../api";

export function Dashboard({
  records,
  previous,
  start,
  end,
  onSource,
  onSession,
  perform,
}: {
  records: UsageRecord[];
  previous: UsageRecord[];
  start: string;
  end: string;
  onSource: (s: SourceStatus) => void;
  onSession: (r: UsageRecord) => void;
  perform: (action: () => Promise<unknown>) => void;
}) {
  const { snapshot, setPage, patch } = useStore();
  const [metric, setMetric] = useState("total");
  const [ranking, setRanking] = useState("total");
  const [modelMetric, setModelMetric] = useState("total");
  const [showTable, setShowTable] = useState(false);
  const current = aggregate(records),
    before = aggregate(previous),
    hasData = records.length > 0;
  const bucket = groups(records, "day").sort((a, b) =>
    a.name.localeCompare(b.name),
  );
  const series =
    start.startsWith("0000") ||
    (Date.parse(end) - Date.parse(start)) / 86400000 > 365
      ? bucket
      : (() => {
          const map = new Map(bucket.map((b) => [b.name, b]));
          const rows = [];
          for (
            let date = start;
            date <= end && rows.length < 366;
            date = dayOffset(date, 1)
          ) {
            rows.push(map.get(date) ?? { name: date, ...aggregate([]) });
          }
          return rows;
        })();
  const agents = groups(records, "agent").sort((a, b) =>
    ranking === "recent"
      ? b.lastActivity.localeCompare(a.lastActivity)
      : ranking === "cost"
        ? b.cost - a.cost
        : ranking === "calls"
          ? (b.calls ?? 0) - (a.calls ?? 0)
          : b.total - a.total,
  );
  const models = groups(records, "model")
    .filter(m=>modelMetric!=='cost'||m.priced>0)
    .sort((a, b) =>
      modelMetric === "cost" ? b.cost - a.cost : a.total - b.total,
    )
    .reverse();
  if (modelMetric === "cost") models.sort((a, b) => b.cost - a.cost);
  const modelTotal = models.reduce(
    (s, m) => s + (modelMetric === "cost" ? m.cost : m.total),
    0,
  );
  const colors = [
    "var(--accent)",
    "var(--teal)",
    "var(--amber)",
    "var(--chart-4)",
    "var(--chart-5)",
    "var(--chart-6)",
  ];
  const recent = sessionRows(records).slice(0, 5);
  const cards = [
    {
      label: "总 Token",
      icon: Layers3,
      value: hasData ? compact(current.total) : "—",
      change: comparison(current.total, before.total),
      detail: "输入与输出合计",
      tone: "accent",
    },
    {
      label: "输入 / 输出",
      icon: ArrowDownLeft,
      value: hasData
        ? `${compact(current.input)} / ${compact(current.output)}`
        : "—",
      change: comparison(
        current.input + current.output,
        before.input + before.output,
      ),
      detail:
        hasData && current.input > 0
          ? `缓存读取占输入 ${((current.cached / current.input) * 100).toFixed(1)}%`
          : "包含缓存输入，不重复计数",
      tone: "teal",
    },
    {
      label: "已知成本",
      icon: Coins,
      value: current.priced ? money(current.cost) : "—",
      change:
        current.priced && before.priced
          ? comparison(current.cost, before.cost)
          : "无可比基线",
      detail:
        current.priced === records.length && hasData
          ? "来源记录 · USD"
          : current.priced
            ? `${records.length - current.priced} 条记录未定价 · USD`
            : "模型价格未知",
      tone: "amber",
    },
    {
      label: "活动会话",
      icon: MessageSquare,
      value: hasData ? number(current.sessions) : "—",
      change: comparison(current.sessions, before.sessions),
      detail: "所选日期内有用量的会话",
      tone: "neutral",
    },
  ];
  return (
    <div className="page-flow">
      <div className="page-intro">
        <div>
          <span className="eyebrow">你的 AI 工作台</span>
          <h1>
            用量总览<span className="heading-dot">.</span>
          </h1>
          <p>看看最近的使用情况，把注意力留给真正重要的工作。</p>
        </div>
        <span className="local-tag">
          <span className="status-dot" />
          本地数据 · 只读采集
        </span>
      </div>
      <div className="metrics">
        {cards.map(({ label, icon: Icon, value, detail, change, tone }) => (
          <section className={`metric metric-${tone}`} key={label}>
            <div className="metric-label">
              {label}
              <Icon size={18} aria-hidden="true" />
            </div>
            <strong
              className="metric-value"
              title={label === "总 Token" ? number(current.total) : undefined}
            >
              {value}
            </strong>
            <div className="metric-meta">
              <span>{detail}</span>
              <small>
                {start.startsWith("0000")
                  ? "全部历史"
                  : `${start.slice(5)} — ${end.slice(5)}`}{" "}
                ·{" "}
                {snapshot?.sources.some((s) =>
                  ["partial", "error", "missing", "stale"].includes(s.status),
                )
                  ? "覆盖不完整"
                  : "已采集记录"}
              </small>
            </div>
            <div className="metric-comparison">
              {hasData ? (
                <>
                  <Activity size={12} aria-hidden="true" />
                  {change}
                  <span>较上一周期</span>
                </>
              ) : (
                <span>等待本地用量数据</span>
              )}
            </div>
          </section>
        ))}
      </div>
      <div className="dashboard-grid">
        <Panel
          title="用量趋势"
          subtitle="按日查看消耗，缓存已包含在输入中"
          className="trend-panel"
          action={
            <div className="segmented small">
              {[
                ["total", "Token"],
                ["cost", "成本"],
                ["calls", "调用"],
              ].map(([id, label]) => (
                <button
                  key={id}
                  aria-pressed={metric === id}
                  className={metric === id ? "active" : ""}
                  onClick={() => setMetric(id)}
                >
                  {label}
                </button>
              ))}
            </div>
          }
        >
          {hasData ? (
            <>
              <div className={`chart-legend ${metric!=='total'?'single-metric':''}`}>
                {metric!=='total'&&<span className="metric-legend">{metric==='cost'?'已知来源成本 · 未定价日期留空':'已知调用次数 · 未知日期留空'}</span>}
                <span>
                  <i
                    className="legend-dot"
                    style={{ background: "var(--accent)" }}
                  />
                  输入
                </span>
                <span>
                  <i
                    className="legend-dot"
                    style={{ background: "var(--teal)" }}
                  />
                  输出
                </span>
                <span>
                  <i
                    className="legend-dot"
                    style={{ background: "var(--amber)" }}
                  />
                  缓存读取
                </span>
              </div>
              <div className="trend-chart">
                <ResponsiveContainer width="100%" height="100%">
                  <AreaChart
                    data={series.map(s=>({...s,cost:s.priced?s.cost:null}))}
                    accessibilityLayer
                    margin={{ left: -18, right: 12, top: 10, bottom: 0 }}
                  >
                    <CartesianGrid
                      stroke="var(--border)"
                      strokeDasharray="3 5"
                      vertical={false}
                    />
                    <XAxis
                      dataKey="name"
                      tickFormatter={(v) => String(v).slice(5)}
                      tickLine={false}
                      axisLine={false}
                      minTickGap={32}
                    />
                    <YAxis
                      tickFormatter={(v) =>
                        metric === "cost" ? `$${v}` : compact(Number(v))
                      }
                      tickLine={false}
                      axisLine={false}
                    />
                    <Tooltip
                      contentStyle={{
                        background: "var(--surface)",
                        border: "1px solid var(--border)",
                        borderRadius: 8,
                        color: "var(--text)",
                      }}
                      formatter={(v) =>
                        metric === "cost" ? money(Number(v)) : number(Number(v))
                      }
                    />
                    {(metric === "total"
                      ? [
                          ["input", "输入", "var(--accent)"],
                          ["output", "输出", "var(--teal)"],
                          ["cached", "缓存读取", "var(--amber)"],
                        ]
                      : [
                          [
                            metric,
                            metric === "cost" ? "来源成本" : "已知调用次数",
                            "var(--accent)",
                          ],
                        ]
                    ).map(([key, label, color]) => (
                      <Area
                        key={key}
                        type="monotone"
                        dataKey={key}
                        name={label}
                        stroke={color}
                        fill={color}
                        fillOpacity={0.045}
                        strokeWidth={2.2}
                        isAnimationActive={false}
                      />
                    ))}
                  </AreaChart>
                </ResponsiveContainer>
              </div>
              <button
                className="text-button"
                onClick={() => setShowTable(!showTable)}
                aria-expanded={showTable}
              >
                <Table2 size={15} />
                {showTable ? "收起" : "查看"}每日数值
              </button>
              {showTable && (
                <div className="table-wrap">
                  <table>
                    <caption className="sr-only">每日用量趋势的数值</caption>
                    <thead>
                      <tr>
                        <th>日期</th>
                        <th>输入</th>
                        <th>输出</th>
                        <th>缓存读取</th>
                        <th>已知成本</th>
                      </tr>
                    </thead>
                    <tbody>
                      {series.map((s) => (
                        <tr key={s.name}>
                          <td>{s.name}</td>
                          <td>{number(s.input)}</td>
                          <td>{number(s.output)}</td>
                          <td>{number(s.cached)}</td>
                          <td>{s.priced ? money(s.cost) : "未知"}</td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </div>
              )}
            </>
          ) : (
            <Empty
              title="还没有用量趋势"
              description="连接 Codex、Claude Code 或 DSH，真实用量会在这里呈现。"
              action={
                <button className="button" onClick={() => setPage("sources")}>
                  连接数据源
                  <ArrowRight size={15} />
                </button>
              }
            />
          )}
          <div className="panel-foot">
            <span>统计日界限：Asia/Shanghai；DSH 保留来源账本日期</span>
            <button
              className="text-button"
              disabled={!hasData}
              onClick={() =>
                perform(() =>
                  exportCsv(
                    csv([
                      ["日期", "输入", "输出", "缓存读取", "成本USD"],
                      ...series.map((s) => [
                        s.name,
                        String(s.input),
                        String(s.output),
                        String(s.cached),
                        s.priced ? String(s.cost) : "未知",
                      ]),
                    ]),
                    "每日用量.csv",
                  ),
                )
              }
            >
              导出
              <ArrowUpRight size={13} />
            </button>
          </div>
        </Panel>
        <Panel
          title="数据源状态"
          subtitle="每个客户端，独立采集"
          action={
            <button
              className="icon-button"
              aria-label="管理数据源"
              onClick={() => setPage("sources")}
            >
              <ArrowUpRight size={17} />
            </button>
          }
        >
          <SourceHealth sources={snapshot?.sources ?? []} onDetail={onSource} />
        </Panel>
      </div>
      <div className="dashboard-grid equal">
        <Panel
          title="Agent 使用排名"
          subtitle="从不同角度了解你的工作习惯"
          action={
            <select
              aria-label="排名方式"
              value={ranking}
              onChange={(e) => setRanking(e.target.value)}
            >
              <option value="total">按 Token</option>
              <option value="cost">按成本</option>
              <option value="calls">按调用</option>
              <option value="recent">最近活动</option>
            </select>
          }
        >
          <div className="rank-list">
            {agents.map((a, i) => (
              <button
                className="rank-row"
                key={a.name}
                onClick={() => {
                  patch({ source: a.name });
                  setPage("agents");
                }}
              >
                <span className="rank-index">0{i + 1}</span>
                <AgentMark agent={a.name} />
                <div className="grow">
                  <div className="rank-title">
                    <strong>{agentNames[a.name] ?? a.name}</strong>
                    <span className="mono">
                      {ranking === "cost"
                        ? a.priced
                          ? money(a.cost)
                          : "未知"
                        : ranking === "calls"
                          ? a.calls === null
                            ? "未知"
                            : number(a.calls)
                          : ranking === "recent"
                            ? dateTime(a.lastActivity)
                            : compact(a.total)}
                    </span>
                  </div>
                  <span className="rank-track">
                    <i
                      style={{
                        width: `${current.total ? Math.max(2, (a.total / current.total) * 100) : 0}%`,
                      }}
                    />
                  </span>
                  <small>
                    {a.sessions} 个会话 ·{" "}
                    {a.calls === null
                      ? "调用次数未知"
                      : `${number(a.calls)} 次调用`}
                  </small>
                </div>
                <ArrowUpRight size={15} />
              </button>
            ))}
            {!hasData && (
              <Empty
                title="等待 Agent 使用记录"
                description="接入后，可按 Token、成本、调用或最近活动查看。"
              />
            )}
          </div>
        </Panel>
        <Panel
          title="模型分布"
          subtitle="让每一份消耗都有迹可循"
          action={
            <div className="segmented small">
              {["total", "cost"].map((m) => (
                <button
                  key={m}
                  aria-pressed={modelMetric === m}
                  className={modelMetric === m ? "active" : ""}
                  onClick={() => setModelMetric(m)}
                >
                  {m === "total" ? "Token" : "成本"}
                </button>
              ))}
            </div>
          }
        >
          {models.length && modelTotal > 0 ? (
            <div
              className={`model-distribution ${models.length > 6 ? "list-only" : ""}`}
            >
              {models.length <= 6 && (
                <div className="donut">
                  <ResponsiveContainer width="100%" height={180}>
                    <PieChart>
                      <Pie
                        data={models}
                        dataKey={modelMetric}
                        innerRadius={59}
                        outerRadius={78}
                        paddingAngle={3}
                        stroke="none"
                        isAnimationActive={false}
                      >
                        {models.map((m, i) => (
                          <Cell key={m.name} fill={colors[i % colors.length]} />
                        ))}
                      </Pie>
                      <Tooltip
                        formatter={(v) =>
                          modelMetric === "cost"
                            ? money(Number(v))
                            : number(Number(v))
                        }
                      />
                    </PieChart>
                  </ResponsiveContainer>
                  <span className="donut-label">
                    <strong>
                      {modelMetric === "cost"
                        ? money(modelTotal)
                        : compact(modelTotal)}
                    </strong>
                    <small>
                      {modelMetric === "cost" ? "已知成本" : "总 Token"}
                    </small>
                  </span>
                </div>
              )}
              <div className="model-list">
                {models.map((m, i) => (
                  <button
                    className="model-row"
                    key={m.name}
                    onClick={() => {
                      patch({ modelFilter: m.name });
                      setPage("sessions");
                    }}
                  >
                    <i
                      className="legend-dot"
                      style={{ background: colors[i % colors.length] }}
                    />
                    <span title={m.name}>
                      {m.name}
                      {models.length > 6 && (
                        <i
                          className="model-bar"
                          style={{
                            width: `${Math.max(1, ((modelMetric === "cost" ? m.cost : m.total) / modelTotal) * 100)}%`,
                            background: colors[i % colors.length],
                          }}
                        />
                      )}
                    </span>
                    <small className="mono">
                      {modelMetric === "cost"
                        ? money(m.cost)
                        : compact(m.total)}
                    </small>
                    <strong className="mono">
                      {(
                        ((modelMetric === "cost" ? m.cost : m.total) /
                          modelTotal) *
                        100
                      ).toFixed(1)}
                      %
                    </strong>
                  </button>
                ))}
              </div>
            </div>
          ) : (
            <Empty
              title={
                modelMetric === "cost"
                  ? "没有可分组的已知成本"
                  : "还没有模型使用数据"
              }
              description={
                modelMetric === "cost"
                  ? "未知价格不会按零成本参与占比。"
                  : "采集完成后，这里会展示真实模型分布。"
              }
            />
          )}
          <div className="panel-foot">
            <span>
              {models.length} 个模型 · {current.priced}/{records.length}{" "}
              条记录有成本依据
            </span>
            <button className="text-button" onClick={() => setPage("sessions")}>
              查看明细
              <ArrowRight size={13} />
            </button>
          </div>
        </Panel>
      </div>
      <div className="dashboard-grid">
        <Panel
          title="最近会话"
          subtitle="从最近一次工作，继续向前"
          action={
            <button className="text-button" onClick={() => setPage("sessions")}>
              全部会话
              <ArrowRight size={14} />
            </button>
          }
        >
          {recent.length ? (
            <div className="recent-list">
              {recent.map((r) => (
                <button
                  key={`${r.agent}:${r.sessionId}`}
                  onClick={() => onSession(r)}
                  className="recent-row"
                >
                  <AgentMark agent={r.agent} />
                  <span className="grow">
                    <strong>
                      {r.project.split(/[\\/]/).filter(Boolean).at(-1) ||
                        "未记录项目"}
                    </strong>
                    <small>
                      {agentNames[r.agent]} · {r.model}
                    </small>
                  </span>
                  <span className="mono">{compact(r.stats.total)}</span>
                  <small>{dateTime(r.timestamp)}</small>
                  <ArrowUpRight size={14} />
                </button>
              ))}
            </div>
          ) : (
            <Empty
              title="还没有可展示的会话"
              description="这里只展示会话元数据与用量，不保存对话正文。"
            />
          )}
        </Panel>
        <Panel title="快速操作" subtitle="常用动作，就在手边">
          <div className="quick-actions">
            {(snapshot?.sources ?? []).map((s) => (
              <button
                className="quick-action"
                key={s.id}
                onClick={() => perform(() => api.launch(s.id))}
              >
                <Terminal size={17} />
                <span>打开 {s.id === "dsh" ? "DSH" : s.name}</span>
                <ArrowUpRight size={15} />
              </button>
            ))}
            <button
              className="quick-action"
              disabled={!snapshot}
              onClick={() =>
                snapshot && perform(() => api.open(snapshot.dataDirectory))
              }
            >
              <FolderOpen size={17} />
              <span>打开数据目录</span>
              <ArrowUpRight size={15} />
            </button>
            <button
              className="quick-action"
              onClick={() => setPage("launchers")}
            >
              <Layers3 size={17} />
              <span>常用工具与入口</span>
              <ArrowRight size={15} />
            </button>
          </div>
        </Panel>
      </div>
    </div>
  );
}
