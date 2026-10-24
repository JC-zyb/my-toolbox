import type { RenamePlan, RenameRules } from "./types";

export const defaultRenameRules: RenameRules = {
  mode: "replace",
  scope: "stem",
  find: "",
  replace: "",
  regex: false,
  caseSensitive: true,
  replaceAll: true,
  prefix: "",
  suffix: "",
  letterCase: "keep",
  trim: false,
  template: "{name}_{n}",
  start: 1,
  step: 1,
  padding: 3,
  nameFilter: "",
  extensions: "",
  includeHidden: false,
  sort: "natural",
  descending: false,
};

// 只评估当前选择。批内依赖取消后，原目标重新成为占用；执行时 Rust 再检查真实目录。
export function renameErrors(
  plan: RenamePlan,
  selected: Set<string>,
): Map<string, string> {
  const errors = new Map<string, string>();
  const moving = new Set<string>();
  const targets = new Map<string, string[]>();
  for (const row of plan.rows) {
    if (!selected.has(row.from) || row.from === row.to) continue;
    moving.add(row.fromName);
    const key = row.toName.toLowerCase();
    const group = targets.get(key) ?? [];
    group.push(row.from);
    targets.set(key, group);
    if (row.error) errors.set(row.from, row.error);
  }
  const occupied = new Set(
    plan.occupiedNames
      .filter((name) => !moving.has(name))
      .map((name) => name.toLowerCase()),
  );
  for (const row of plan.rows) {
    if (!selected.has(row.from) || row.from === row.to || errors.has(row.from))
      continue;
    const key = row.toName.toLowerCase();
    if (targets.get(key)!.length > 1) errors.set(row.from, "目标名称重复");
    else if (occupied.has(key))
      errors.set(row.from, "目标被未选择的文件或目录占用");
  }
  return errors;
}
