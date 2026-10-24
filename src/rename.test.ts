import { describe, expect, it } from "vitest";
import { renameErrors } from "./rename";
import type { RenamePlan } from "./types";

function plan(
  pairs: [string, string, string?][],
  occupied?: string[],
): RenamePlan {
  return {
    id: "preview",
    directory: "/test",
    skipped: 0,
    occupiedNames: occupied ?? pairs.map(([from]) => from),
    rows: pairs.map(([from, to, error]) => ({
      from: `/test/${from}`,
      to: `/test/${to}`,
      fromName: from,
      toName: to,
      error: error ?? null,
    })),
  };
}
const selected = (...names: string[]) =>
  new Set(names.map((name) => `/test/${name}`));

describe("改名选择与冲突", () => {
  it("交换、改大小写可执行，取消依赖文件后目标重新被占用", () => {
    const preview = plan([
      ["a.txt", "b.txt"],
      ["b.txt", "a.txt"],
      ["c.txt", "C.txt"],
    ]);
    expect(
      renameErrors(preview, selected("a.txt", "b.txt", "c.txt")).size,
    ).toBe(0);
    expect(
      renameErrors(preview, selected("a.txt")).get("/test/a.txt"),
    ).toContain("占用");
  });
  it("重复目标标记所有相关行，取消一项后重新计算", () => {
    const preview = plan([
      ["a", "x"],
      ["b", "X"],
    ]);
    expect([...renameErrors(preview, selected("a", "b")).values()]).toEqual([
      "目标名称重复",
      "目标名称重复",
    ]);
    expect(renameErrors(preview, selected("a")).size).toBe(0);
  });
  it("目录、隐藏项与未变化的文件依然占用名称", () => {
    const preview = plan(
      [
        ["a", "folder"],
        ["b", ".hidden"],
        ["c", "d"],
        ["d", "d"],
      ],
      ["a", "b", "c", "d", "folder", ".hidden"],
    );
    expect(renameErrors(preview, selected("a", "b", "c", "d")).size).toBe(3);
  });
  it("保留后端非法名称提示，取消选择后不再阻止其他项", () => {
    const preview = plan([
      ["a", "CON", "Windows 保留名称"],
      ["b", "ok"],
    ]);
    expect(renameErrors(preview, selected("a", "b")).get("/test/a")).toBe(
      "Windows 保留名称",
    );
    expect(renameErrors(preview, selected("b")).size).toBe(0);
  });
});
