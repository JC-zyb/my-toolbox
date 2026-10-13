import { invoke, isTauri } from "@tauri-apps/api/core";
import { open, save } from "@tauri-apps/plugin-dialog";
import type { Settings, Snapshot } from "./types";

export const desktop = isTauri();
export async function command<T>(
  name: string,
  args?: Record<string, unknown>,
): Promise<T> {
  if (!desktop)
    throw new Error(
      "需要桌面环境。请使用 npm run desktop 启动；浏览器预览不会读取本机数据或执行系统操作。",
    );
  return invoke<T>(name, args);
}
export const api = {
  settings: () => command<Settings>("get_settings"),
  save: (settings: Settings) => command<void>("update_settings", { settings }),
  refresh: () => command<Snapshot>("refresh_usage"),
  cached: () => command<Snapshot | null>("get_snapshot"),
  open: (target: string) => command<void>("open_target", { target }),
  launch: (id: string) => command<void>("launch_agent", { id }),
};
export async function chooseFile(
  directory = false,
  multiple = false,
): Promise<string[] | null> {
  if (!desktop) return command("unavailable");
  const result = await open({ directory, multiple });
  return result ? (Array.isArray(result) ? result : [result]) : null;
}
export async function exportCsv(content: string, name: string) {
  if (!desktop) return command<void>("unavailable");
  const path = await save({
    defaultPath: name,
    filters: [{ name: "CSV 文件", extensions: ["csv"] }],
  });
  if (path) await command("export_csv", { path, content });
}
