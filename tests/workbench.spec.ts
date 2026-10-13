import { test, expect } from "@playwright/test";
import { join } from "node:path";
import { readFileSync, existsSync } from "node:fs";

test("常见尺寸不溢出，浅深色及命令键盘导航可用", async ({ page }) => {
  const errors: string[] = [];
  page.on("pageerror", (e) => errors.push(e.message));
  await page.goto("/", { waitUntil: "domcontentloaded" });
  await expect(page.getByRole("heading", { level: 1 })).toHaveText("用量总览.");
  for (const [width, height] of [
    [1280, 720],
    [1440, 900],
    [1920, 1080],
    [1024, 576],
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
  if (await page.getByRole("button", { name: "切换浅色主题" }).count())
    await page.getByRole("button", { name: "切换浅色主题" }).click();
  await page.screenshot({
    path: join(process.cwd(), "test-results", "workbench-light.png"),
    fullPage: true,
  });
  await page.getByRole("button", { name: "切换深色主题" }).click();
  await expect(page.locator("html")).toHaveAttribute("data-theme", "dark");
  await page.screenshot({
    path: join(process.cwd(), "test-results", "workbench-dark.png"),
    fullPage: true,
  });
  await page.keyboard.press("Control+k");
  await expect(page.getByRole("dialog")).toBeVisible();
  await page.getByRole("combobox", { name: "搜索命令" }).fill("批量改名");
  await page.keyboard.press("Enter");
  await expect(page.getByRole("heading", { level: 1 })).toHaveText("批量改名.");
  await page.keyboard.press("Control+3");
  await expect(page.getByRole("heading", { level: 1 })).toHaveText("会话.");
  await page.getByRole("button", { name: "刷新数据", exact: true }).click();
  await expect(page.getByRole("alert")).toContainText("需要桌面环境");
  expect(errors).toEqual([]);
});

test("真实来源快照的展示与失败保留旧数据", async ({ page }) => {
  const snapshotPath = join(
    process.cwd(),
    "src-tauri",
    "target",
    "inspect-data",
    "snapshot.json",
  );
  test.skip(!existsSync(snapshotPath), "需要先运行隔离统计检查");
  const snapshot = JSON.parse(readFileSync(snapshotPath, "utf8"));
  await page.addInitScript(
    ({ snapshot }) => {
      let scans = 0;
      Object.assign(window, {
        isTauri: true,
        __TAURI_INTERNALS__: {
          invoke: async (name: string) => {
            if (name === "get_settings")
              return {
                theme: "light",
                refreshSeconds: 3600,
                closeToTray: true,
                sources: snapshot.sources,
                entries: [],
                scripts: [],
              };
            if (name === "update_settings") return;
            if (name === "get_snapshot") return snapshot;
            if (name === "refresh_usage") {
              if (++scans === 1) return snapshot;
              throw new Error("测试：来源读取失败");
            }
            throw new Error(`未配置的测试命令：${name}`);
          },
        },
      });
    },
    { snapshot },
  );
  await page.setViewportSize({ width: 1440, height: 900 });
  await page.goto("/", { waitUntil: "domcontentloaded" });
  await expect(page.getByText("正在读取本地用量数据…")).toHaveCount(0);
  await expect(page.locator(".source-row")).toHaveCount(3);
  await expect(page.locator(".metric-value").first()).not.toHaveText("—");
  await page.getByRole("button", { name: "全部", exact: true }).click();
  await expect(page.locator(".trend-chart svg").first()).toBeVisible();
  await page.screenshot({
    path: join(process.cwd(), "test-results", "workbench-data-light.png"),
    fullPage: true,
  });
  await page.getByRole("button", { name: "切换深色主题" }).click();
  await expect(page.locator("html")).toHaveAttribute("data-theme", "dark");
  await page.screenshot({
    path: join(process.cwd(), "test-results", "workbench-data-dark.png"),
    fullPage: true,
  });
  await page.getByRole("button", { name: "刷新数据", exact: true }).click();
  await expect(page.getByRole("alert")).toContainText("保留上次成功数据");
  await expect(page.locator(".metric-value").first()).not.toHaveText("—");
  await page.getByRole("button", { name: "会话", exact: true }).click();
  await expect(page.locator(".session-table tbody tr")).toHaveCount(20);
  await page
    .locator(".session-table tbody tr")
    .first()
    .getByRole("button")
    .click();
  await expect(page.getByRole("dialog")).toContainText("会话详情");
  await page.keyboard.press("Escape");
  await expect(page.getByRole("dialog")).toHaveCount(0);
});
