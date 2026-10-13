import { describe, expect, it } from "vitest";
import {
  aggregate,
  bounds,
  csv,
  filterRecords,
  localDay,
  sessionRows,
} from "./stats";
import type { UsageRecord } from "./types";
const record: UsageRecord = {
  id: "one",
  sessionId: "session",
  agent: "codex",
  project: "toolbox",
  model: "model",
  day: "2026-10-12",
  timestamp: "2026-10-12T10:00:00Z",
  startedAt: "2026-10-12T09:00:00Z",
  sourceFile: "fixture.jsonl",
  parentId: null,
  usage: {
    input: 100,
    output: 20,
    cached: 80,
    cacheWrite: 0,
    reasoning: null,
    calls: null,
    cost: null,
  },
};
describe("统一用量统计", () => {
  it("缓存属于输入，不重复计数；未知成本和调用保留未知", () => {
    const stats = aggregate([record]);
    expect(stats.total).toBe(120);
    expect(stats.cached).toBe(80);
    expect(stats.priced).toBe(0);
    expect(stats.calls).toBeNull();
  });
  it("按上海日期筛选并使用等长上一周期", () => {
    expect(localDay(new Date("2026-10-11T23:30:00Z"))).toBe("2026-10-12");
    expect(bounds("7", "", "", "2026-10-12")).toEqual({
      start: "2026-10-06",
      end: "2026-10-12",
      previousStart: "2026-09-29",
      previousEnd: "2026-10-05",
    });
    expect(
      filterRecords([record], "2026-10-12", "2026-10-12", "claude"),
    ).toHaveLength(0);
  });
  it("跨日和模型汇总为同一个会话", () => {
    const rows = sessionRows([
      record,
      { ...record, id: "two", day: "2026-10-13", model: "other" },
    ]);
    expect(rows).toHaveLength(1);
    expect(rows[0].stats.total).toBe(240);
  });
  it("CSV 转义引号并阻止公式注入", () => {
    expect(csv([["=cmd", 'a"b']])).toBe('\uFEFF"\'=cmd","a""b"');
  });
});
