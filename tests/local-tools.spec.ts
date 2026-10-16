import { test, expect } from "@playwright/test";
import {
  spawn,
  execFileSync,
  type ChildProcessWithoutNullStreams,
} from "node:child_process";
import { createInterface } from "node:readline";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { randomUUID } from "node:crypto";
import { readFileSync, existsSync, writeFileSync } from "node:fs";

let child: ChildProcessWithoutNullStreams;
let root: string;
let queue: {
  resolve: (value: unknown) => void;
  reject: (error: Error) => void;
}[];
function invoke(
  name: string,
  args: Record<string, unknown> = {},
): Promise<unknown> {
  return new Promise((resolve, reject) => {
    queue.push({ resolve, reject });
    child.stdin.write(JSON.stringify({ name, args }) + "\n");
  });
}
test.beforeAll(() => {
  execFileSync(
    "cargo",
    [
      "build",
      "--manifest-path",
      "src-tauri/Cargo.toml",
      "--no-default-features",
      "--bin",
      "local-tools-fixture",
    ],
    { cwd: process.cwd(), stdio: "pipe" },
  );
});
test.beforeEach(async ({ page }) => {
  root = join(tmpdir(), `toolbox-local-tools-ui-${randomUUID()}`);
  queue = [];
  child = spawn(
    join(
      process.cwd(),
      `src-tauri/target/debug/local-tools-fixture${process.platform === "win32" ? ".exe" : ""}`,
    ),
    [root],
    { cwd: process.cwd() },
  );
  child.stderr.on("data", (data) => process.stderr.write(data));
  await new Promise<void>((resolve, reject) => {
    child.once("error", reject);
    createInterface({ input: child.stdout }).on("line", (line) => {
      const response = JSON.parse(line);
      if (response.ready) {
        resolve();
        return;
      }
      const pending = queue.shift();
      if (!pending) return;
      if (response.error) pending.reject(new Error(response.error));
      else pending.resolve(response.value);
    });
    child.on("exit", (code) => {
      reject(new Error(`local tools bridge exited ${code}`));
      for (const pending of queue)
        pending.reject(new Error(`bridge exited ${code}`));
      queue = [];
    });
  });
  await page.exposeFunction("localToolsTestInvoke", invoke);
  await page.addInitScript(() =>
    Object.assign(window, {
      isTauri: true,
      __TAURI_INTERNALS__: {
        invoke: (name: string, args: Record<string, unknown>) =>
          (
            window as unknown as {
              localToolsTestInvoke: (
                name: string,
                args: Record<string, unknown>,
              ) => Promise<unknown>;
            }
          ).localToolsTestInvoke(name, args ?? {}),
      },
    }),
  );
  await page.goto("/", { waitUntil: "domcontentloaded" });
  await page.getByRole("button", { name: "本地工具", exact: true }).click();
  await expect(
    page.getByRole("button", { name: "查看 Node.js", exact: true }),
  ).toBeVisible();
});
test.afterEach(() => {
  child.stdin.end();
});

test("发现、资料补全、筛选、三种尺寸和浅深主题", async ({ page }) => {
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));
  await page.getByRole("textbox", { name: "筛选本地工具" }).fill("fnm");
  await expect(page.locator(".local-tool-row")).toHaveCount(1);
  await page.getByRole("textbox", { name: "筛选本地工具" }).fill("");
  await page.getByRole("button", { name: "查看 Node.js", exact: true }).click();
  await expect(page.getByText("沙箱中的官方资料样例")).toBeVisible();
  await expect(
    page.getByText("fnm 安装清单与实际文件匹配", { exact: true }),
  ).toBeVisible();
  for (const [width, height] of [
    [1280, 720],
    [1440, 900],
    [1920, 1080],
    [853, 480],
  ]) {
    await page.setViewportSize({ width, height });
    expect(
      await page.evaluate(
        () => document.documentElement.scrollWidth <= innerWidth,
      ),
    ).toBe(true);
  }
  await page.setViewportSize({ width: 1440, height: 900 });
  await page.screenshot({
    path: "test-results/local-tools-detail-light.png",
    fullPage: true,
  });
  await page.getByRole("button", { name: "切换深色主题" }).click();
  await page.screenshot({
    path: "test-results/local-tools-detail-dark.png",
    fullPage: true,
  });
  await page.getByRole("button", { name: "返回工具列表" }).click();
  await page.getByRole("button", { name: "工具目录", exact: true }).click();
  await expect(
    page.getByRole("button", { name: "查看 Java", exact: true }),
  ).toBeVisible();
  expect(errors).toEqual([]);
});

test("版本安装、默认选择、更新查询和失败记录", async ({ page }) => {
  await page.getByRole("button", { name: "查看 Node.js", exact: true }).click();
  await expect(page.getByText("沙箱中的官方资料样例")).toBeVisible();
  await page.getByRole("button", { name: "安装与版本", exact: true }).click();
  await expect(
    page.getByText("22.12.0", { exact: true }).first(),
  ).toBeVisible();
  await page.getByRole("button", { name: "查询可用版本" }).click();
  const row = page
    .locator(".local-tool-version")
    .filter({ hasText: "24.16.0" });
  await row.getByRole("button", { name: "选择版本" }).click();
  await page.getByRole("button", { name: "预览安装", exact: true }).click();
  const confirm = page.getByRole("dialog", { name: "确认工具操作" });
  await expect(confirm).toContainText("fnm");
  await expect(confirm).toContainText('"install" "24.16.0"');
  await confirm.getByRole("button", { name: "确认执行" }).click();
  const log = page.getByRole("dialog", { name: "工具操作详情" });
  await expect(log).toContainText("已完成");
  await expect(log).toContainText("退出码与执行后状态校验通过");
  expect(
    existsSync(
      join(
        root,
        "home/.local/share/fnm/node-versions/v24.16.0/installation",
        process.platform === "win32" ? "node.exe" : "bin/node",
      ),
    ),
  ).toBe(true);
  await log.getByRole("button", { name: "关闭", exact: true }).click();
  await page.getByRole("button", { name: "读取已安装版本" }).click();
  await page
    .locator(".local-tool-version")
    .filter({ hasText: "24.16.0" })
    .getByRole("button", { name: "设为默认…" })
    .click();
  await confirm.getByRole("button", { name: "确认执行" }).click();
  await expect(log).toContainText("已完成");
  await log.getByRole("button", { name: "关闭", exact: true }).click();
  await invoke("fixture_fail_next");
  await page.getByLabel("目标版本", { exact: true }).fill("24.17.0");
  await page.getByRole("button", { name: "预览安装", exact: true }).click();
  await confirm.getByRole("button", { name: "确认执行" }).click();
  await expect(log).toContainText("失败");
  await expect(log).toContainText("manager download failed");
  await log.getByRole("button", { name: "关闭", exact: true }).click();
  await page.getByRole("button", { name: "返回工具列表" }).click();
  await page.getByRole("button", { name: "可更新", exact: true }).click();
  await page.getByRole("button", { name: "检查更新", exact: true }).click();
  await expect(
    page.getByRole("button", { name: "查看 Node.js", exact: true }),
  ).toBeVisible();
});

test("项目预览、防过期覆盖、确认备份和搜索添加", async ({ page }) => {
  await page.getByRole("button", { name: "查看 Node.js", exact: true }).click();
  await expect(page.getByText("沙箱中的官方资料样例")).toBeVisible();
  await page.getByRole("button", { name: "项目配置", exact: true }).click();
  await page
    .getByLabel("项目目录", { exact: true })
    .fill(join(root, "project"));
  await page.getByRole("button", { name: "读取配置", exact: true }).click();
  await expect(
    page.getByText("项目声明：22.12.0", { exact: false }),
  ).toBeVisible();
  await page.getByLabel("固定版本", { exact: true }).fill("24.16.0");
  await page.getByRole("button", { name: "预览项目修改" }).click();
  const confirm = page.getByRole("dialog", { name: "确认工具操作" });
  await expect(confirm).toContainText("修改前");
  writeFileSync(join(root, "project/.node-version"), "22.13.0\n");
  await confirm.getByRole("button", { name: "确认执行" }).click();
  const log = page.getByRole("dialog", { name: "工具操作详情" });
  await expect(log).toContainText("失败");
  await expect(log).toContainText("预览已过期");
  expect(readFileSync(join(root, "project/.node-version"), "utf8")).toBe(
    "22.13.0\n",
  );
  await log.getByRole("button", { name: "关闭", exact: true }).click();
  await page.getByRole("button", { name: "预览项目修改" }).click();
  await confirm.getByRole("button", { name: "确认执行" }).click();
  await expect(log).toContainText("已完成");
  await expect(log).toContainText("备份：");
  expect(readFileSync(join(root, "project/.node-version"), "utf8")).toBe(
    "24.16.0\n",
  );
  await log.getByRole("button", { name: "关闭", exact: true }).click();
  await page.getByRole("button", { name: "返回工具列表" }).click();
  await page.getByRole("button", { name: "添加工具", exact: true }).click();
  const search = page.getByRole("dialog", { name: "搜索并添加工具" });
  await search.getByLabel("工具名称或 GitHub 地址").fill("uv");
  await search.getByRole("button", { name: "搜索", exact: true }).click();
  await search.getByRole("button", { name: "添加资料" }).first().click();
  await expect(search).toHaveCount(0);
  await expect(page.getByText("已添加 uv 的资料，未安装软件。")).toBeVisible();
  await expect(
    page.getByRole("button", { name: "查看 uv", exact: true }),
  ).toBeVisible();
});
