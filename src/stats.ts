import type { UsageRecord } from "./types";
export const agentNames: Record<string, string> = {
  codex: "Codex",
  claude: "Claude Code",
  dsh: "DeepSeek Harness",
};
export function compact(value: number): string {
  return new Intl.NumberFormat("en", {
    notation: "compact",
    maximumFractionDigits: 2,
  }).format(value);
}
export function number(value: number): string {
  return new Intl.NumberFormat("zh-CN").format(value);
}
export function money(value: number): string {
  return new Intl.NumberFormat("en-US", {
    style: "currency",
    currency: "USD",
    maximumFractionDigits: 2,
  }).format(value);
}
export function localDay(date = new Date()): string {
  return new Intl.DateTimeFormat("en-CA", {
    timeZone: "Asia/Shanghai",
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
  }).format(date);
}
export function dateTime(value: string | null): string {
  if (!value) return "从未采集";
  return new Intl.DateTimeFormat("zh-CN", {
    timeZone: "Asia/Shanghai",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  }).format(new Date(value));
}
export function dayOffset(date: string, offset: number): string {
  const d = new Date(`${date}T00:00:00+08:00`);
  d.setUTCDate(d.getUTCDate() + offset);
  return localDay(d);
}
export function bounds(
  range: string,
  start: string,
  end: string,
  today = localDay(),
) {
  if (range === "all")
    return {
      start: "0000-00-00",
      end: today,
      previousStart: "",
      previousEnd: "",
    };
  const last = range === "custom" ? end || today : today;
  const first =
    range === "custom"
      ? start || today
      : dayOffset(today, -(Number(range) - 1));
  const days =
    Math.round(
      (Date.parse(`${last}T00:00:00Z`) - Date.parse(`${first}T00:00:00Z`)) /
        86400000,
    ) + 1;
  return {
    start: first,
    end: last,
    previousStart: dayOffset(first, -days),
    previousEnd: dayOffset(first, -1),
  };
}
export function filterRecords(
  records: UsageRecord[],
  start: string,
  end: string,
  source: string,
) {
  return records.filter(
    (r) =>
      r.day >= start &&
      r.day <= end &&
      (source === "all" || r.agent === source),
  );
}
export function aggregate(records: UsageRecord[]) {
  return {
    input: records.reduce((s, r) => s + r.usage.input, 0),
    output: records.reduce((s, r) => s + r.usage.output, 0),
    cached: records.reduce((s, r) => s + r.usage.cached, 0),
    cacheWrite: records.reduce((s, r) => s + r.usage.cacheWrite, 0),
    total: records.reduce((s, r) => s + r.usage.input + r.usage.output, 0),
    calls: records.every((r) => r.usage.calls !== null)
      ? records.reduce((s, r) => s + (r.usage.calls ?? 0), 0)
      : null,
    cost: records.reduce((s, r) => s + (r.usage.cost ?? 0), 0),
    priced: records.filter((r) => r.usage.cost !== null).length,
    sessions: new Set(records.map((r) => `${r.agent}:${r.sessionId}`)).size,
  };
}
export function groups(records: UsageRecord[], key: "agent" | "model" | "day") {
  const buckets = new Map<string, UsageRecord[]>();
  for (const record of records) {
    const group = record[key];
    buckets.set(group, [...(buckets.get(group) ?? []), record]);
  }
  return [...buckets].map(([name, rows]) => ({
    name,
    ...aggregate(rows),
    lastActivity:
      rows
        .map((r) => r.timestamp)
        .sort()
        .at(-1) ?? "",
  }));
}
export function comparison(now: number, before: number) {
  return before > 0
    ? `${now >= before ? "+" : ""}${(((now - before) / before) * 100).toFixed(1)}%`
    : "无可比基线";
}
export function csv(rows: string[][]): string {
  return (
    "\uFEFF" +
    rows
      .map((row) =>
        row
          .map((cell) => {
            const value = /^[=+@-]/.test(cell) ? `'${cell}` : cell;
            return `"${value.replaceAll('"', '""')}"`;
          })
          .join(","),
      )
      .join("\r\n")
  );
}
export function sessionRows(records: UsageRecord[]) {
  const map = new Map<string, UsageRecord[]>();
  records.forEach((r) => {
    const key = `${r.agent}:${r.sessionId}`;
    map.set(key, [...(map.get(key) ?? []), r]);
  });
  return [...map.values()]
    .map((rows) => {
      const recent = [...rows].sort((a, b) =>
        b.timestamp.localeCompare(a.timestamp),
      )[0];
      return { ...recent, stats: aggregate(rows), rows };
    })
    .sort((a, b) => b.timestamp.localeCompare(a.timestamp));
}
