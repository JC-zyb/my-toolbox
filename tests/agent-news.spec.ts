import { test, expect } from "@playwright/test";
import { join } from "node:path";
import type { NewsSnapshot } from "../src/news";

test.beforeEach(async ({ page }) => {
  if (test.info().title.startsWith("浏览器")) return;
  await page.addInitScript(() => {
    const publishedAt = new Date().toISOString();
    const snapshot: NewsSnapshot = {
      automatic: true, language: "original",
      sources: [
        { id: "openai", name: "OpenAI News", url: "https://openai.com/news/rss.xml", group: "models", category: "models", enabled: true, lastAttempt: publishedAt, lastSuccess: publishedAt, error: null, items: [
          { title: "A new model for agents", url: "https://openai.com/test-model", category: "models", publishedAt, summary: "<p>A model with <strong>reasoning</strong> and tool use.</p><script>window.badNews = true</script>" },
          { title: "Older model", url: "https://openai.com/old-model", category: "models", publishedAt: "2020-01-01T00:00:00Z", summary: "Archived release" },
        ] },
        { id: "codex", name: "OpenAI Codex", url: "https://github.com/openai/codex/releases.atom", group: "coding", category: "features", enabled: true, lastAttempt: publishedAt, lastSuccess: publishedAt, error: null, items: [
          { title: "Codex feature update", url: "https://github.com/openai/codex/releases/tag/test", category: "features", publishedAt, summary: "New editor integration" },
        ] },
        { id: "mcp", name: "MCP SDK", url: "https://github.com/modelcontextprotocol/typescript-sdk/releases.atom", category: "technology", enabled: true, lastAttempt: publishedAt, lastSuccess: null, error: "HTTP 503 Service Unavailable", items: [] },
      ],
    };
    const state = window as unknown as { translationFails?: boolean; newsCommands: string[]; newsInsertOnRefresh?: boolean };
    state.newsCommands = [];
    Object.assign(window, {
      isTauri: true,
      __TAURI_INTERNALS__: {
        invoke: async (name: string, args: Record<string, unknown> = {}) => {
          state.newsCommands.push(name);
          if (name === "get_agent_news") return structuredClone(snapshot);
          if (name === "refresh_agent_news") {
            if (args.force) { snapshot.sources[0].error = "测试：网络不可用"; }
            if (state.newsInsertOnRefresh) {
              state.newsInsertOnRefresh = false;
              snapshot.sources[0].items.unshift({ title: "Newest agent model", url: "https://openai.com/newest", category: "models", publishedAt: new Date(Date.now() + 60000).toISOString(), summary: "New release" });
            }
            return structuredClone(snapshot);
          }
          if (name === "set_agent_news_preferences") {
            snapshot.automatic = args.automatic as boolean;
            snapshot.sources.forEach(source => { source.enabled = (args.enabled as string[]).includes(source.id); });
            return structuredClone(snapshot);
          }
          if (name === "set_agent_news_language") { snapshot.language = args.language as NewsSnapshot["language"]; return structuredClone(snapshot); }
          if (name === "translate_agent_news") return (args.urls as string[]).map(url => {
            const item = snapshot.sources.flatMap(source => source.items).find(item => item.url === url)!;
            if (state.translationFails) return { url, translation: null, error: "HTTP 429，请稍后重试" };
            const translation = { title: item.title.includes("Codex") ? "Codex 功能更新" : "面向智能体的新模型", summary: "新模型增强了推理、工具调用和任务规划能力。", translated: true };
            item.translation = translation;
            return { url, translation, error: null };
          });
          if (name === "preview_agent_news") return { url: args.url, title: "面向智能体的新模型", sourceName: "OpenAI News", content: "Agent planning and tool use. ".repeat(40), translatedContent: snapshot.language === "original" ? null : "这是工具内预览的中文正文，包含智能体规划和工具调用的技术细节。".repeat(40), kind: "web", diagnostic: null };
          if (name === "add_agent_news_source") {
            if (snapshot.sources.some(source => source.url === args.url)) throw new Error("该资讯来源已订阅");
            snapshot.sources.push({ id: "custom-test", name: args.name as string, url: args.url as string, group: args.group as string, category: "technology", language: "zh", custom: true, enabled: true, lastAttempt: publishedAt, lastSuccess: publishedAt, error: null, items: [{ title: "多智能体协作的新进展", url: "https://example.com/agents", summary: "中文订阅内容", category: "technology", publishedAt, translation: { title: "多智能体协作的新进展", summary: "中文订阅内容", translated: false } }] });
            return structuredClone(snapshot);
          }
          if (name === "remove_agent_news_source") { snapshot.sources = snapshot.sources.filter(source => source.id !== args.id); return structuredClone(snapshot); }
          if (name === "open_target") { (window as unknown as { openedUrl: string }).openedUrl = args.target as string; return; }
          if (name === "get_settings") return { theme: "light", refreshSeconds: 3600, sources: [], entries: [], scripts: [] };
          if (name === "update_settings") return;
          if (name === "get_snapshot") return null;
          if (name === "refresh_usage") return { records: [], sources: [], collectedAt: publishedAt, dataDirectory: "test-only" };
          throw new Error(`未配置的测试命令：${name}`);
        },
      },
    });
  });
});

test("资讯筛选、订阅、更新失败保留和浅深主题布局", async ({ page }) => {
  const errors: string[] = [];
  page.on("pageerror", error => errors.push(error.message));
  await page.goto("/");
  await page.getByRole("button", { name: "Agent 资讯", exact: true }).click();
  await expect(page.getByRole("heading", { level: 1 })).toHaveText("Agent 资讯");
  await expect(page.locator(".news-item")).toHaveCount(2);
  await expect(page.locator(".news-item").first()).not.toContainText("window.badNews");
  expect(await page.evaluate(() => (window as unknown as { badNews?: boolean }).badNews)).toBeUndefined();
  await page.getByRole("button", { name: "A new model for agents" }).click();
  await expect(page.getByRole("dialog")).toBeVisible();
  expect(await page.evaluate(() => (window as unknown as { openedUrl?: string }).openedUrl)).toBeUndefined();
  await expect(page.getByRole("dialog")).toContainText("Agent planning and tool use.");
  await page.getByRole("button", { name: "打开原网页", exact: true }).click();
  expect(await page.evaluate(() => (window as unknown as { openedUrl: string }).openedUrl)).toBe("https://openai.com/test-model");
  await page.getByRole("button", { name: "关闭", exact: true }).click();
  await page.getByRole("button", { name: "功能更新 1" }).click();
  await expect(page.locator(".news-item")).toHaveCount(1);
  await expect(page.locator(".news-item")).toContainText("Codex feature update");
  await page.getByRole("textbox", { name: "搜索资讯" }).fill("no-match");
  await expect(page.getByText("没有匹配的资讯", { exact: true })).toBeVisible();
  await page.getByRole("button", { name: "清除筛选" }).click();
  await expect(page.locator(".news-item")).toHaveCount(3);
  await page.getByRole("combobox", { name: "资讯来源", exact: true }).selectOption("codex");
  await expect(page.locator(".news-item")).toHaveCount(1);
  await page.getByRole("combobox", { name: "资讯来源", exact: true }).selectOption("all");
  await page.getByRole("button", { name: "刷新资讯", exact: true }).click();
  await expect(page.getByText("2 个来源更新失败，保留已有资讯")).toBeVisible();
  await expect(page.locator(".news-item")).toHaveCount(3);
  await page.getByRole("checkbox", { name: "自动更新", exact: true }).uncheck();
  await expect(page.getByRole("checkbox", { name: "自动更新", exact: true })).not.toBeChecked();
  await page.getByRole("button", { name: "订阅来源", exact: true }).click();
  await expect(page.getByText("测试：网络不可用", { exact: true })).toBeVisible();
  await page.getByRole("combobox", { name: "资讯来源", exact: true }).selectOption("codex");
  await page.getByRole("checkbox", { name: "OpenAI Codex", exact: true }).uncheck();
  await expect(page.getByRole("combobox", { name: "资讯来源", exact: true })).toHaveValue("all");
  await expect(page.locator(".news-item")).toHaveCount(2);
  for (const [width, height] of [[1280, 720], [1440, 900], [1920, 1080], [853, 480], [375, 812]]) {
    await page.setViewportSize({ width, height });
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
    await expect(page.getByRole("heading", { level: 1 })).toBeVisible();
  }
  await page.setViewportSize({ width: 1440, height: 900 });
  await page.screenshot({ path: join(process.cwd(), "test-results", "agent-news-light.png"), fullPage: true });
  await page.getByRole("button", { name: "切换深色主题", exact: true }).click();
  await expect(page.locator("html")).toHaveAttribute("data-theme", "dark");
  await page.screenshot({ path: join(process.cwd(), "test-results", "agent-news-dark.png"), fullPage: true });
  await page.setViewportSize({ width: 375, height: 812 });
  await page.screenshot({ path: join(process.cwd(), "test-results", "agent-news-mobile.png"), fullPage: true });
  expect(errors).toEqual([]);
});

test("中文翻译、双语预览、领域筛选和自定义订阅", async ({ page }) => {
  const errors: string[] = [];
  page.on("pageerror", error => errors.push(error.message));
  await page.goto("/");
  await page.getByRole("button", { name: "Agent 资讯", exact: true }).click();
  expect(await page.evaluate(() => (window as unknown as { newsCommands: string[] }).newsCommands.filter(name => name === "translate_agent_news"))).toHaveLength(0);
  await page.evaluate(() => { (window as unknown as { translationFails: boolean }).translationFails = true; });
  await page.getByRole("button", { name: "中文", exact: true }).click();
  await expect(page.getByText("中文翻译暂不可用：HTTP 429，请稍后重试")).toBeVisible();
  await page.evaluate(() => { (window as unknown as { translationFails: boolean }).translationFails = false; });
  await page.getByRole("button", { name: "重试翻译", exact: true }).click();
  await expect(page.getByRole("button", { name: "面向智能体的新模型", exact: true })).toBeVisible();
  await page.getByRole("textbox", { name: "搜索资讯", exact: true }).fill("推理");
  await expect(page.locator(".news-item")).toHaveCount(2);
  await page.getByRole("textbox", { name: "搜索资讯", exact: true }).fill("");
  await page.getByRole("combobox", { name: "资讯领域", exact: true }).selectOption("coding");
  await expect(page.locator(".news-item")).toHaveCount(1);
  await expect(page.locator(".news-item")).toContainText("Codex 功能更新");
  await page.getByRole("combobox", { name: "资讯领域", exact: true }).selectOption("all");
  await page.getByRole("button", { name: "双语", exact: true }).click();
  await expect(page.locator(".news-item").filter({ has: page.getByRole("button", { name: "面向智能体的新模型", exact: true }) })).toContainText("A new model for agents");
  await page.getByRole("button", { name: "面向智能体的新模型", exact: true }).click();
  const dialog = page.getByRole("dialog");
  await expect(dialog).toContainText("这是工具内预览的中文正文");
  await expect(dialog.locator("details")).toHaveAttribute("open", "");
  await expect(dialog).toContainText("Agent planning and tool use.");
  expect(await page.evaluate(() => (window as unknown as { openedUrl?: string }).openedUrl)).toBeUndefined();
  await page.setViewportSize({ width: 375, height: 812 });
  expect(await dialog.evaluate(element => element.scrollWidth <= element.clientWidth)).toBe(true);
  await page.screenshot({ path: join(process.cwd(), "test-results", "agent-news-preview-mobile.png") });
  await page.keyboard.press("Escape");
  await page.getByRole("button", { name: "订阅来源", exact: true }).click();
  await page.getByRole("textbox", { name: "订阅名称", exact: true }).fill("中文 Agent 动态");
  await page.getByRole("textbox", { name: "订阅地址", exact: true }).fill("https://example.com/feed");
  await page.getByRole("combobox", { name: "订阅领域", exact: true }).selectOption("general");
  await page.getByRole("button", { name: "添加订阅", exact: true }).click();
  await expect(page.getByRole("checkbox", { name: "中文 Agent 动态", exact: true })).toBeVisible();
  await expect(page.getByRole("button", { name: "多智能体协作的新进展", exact: true })).toBeVisible();
  await page.getByRole("textbox", { name: "订阅地址", exact: true }).fill("https://example.com/feed");
  await page.getByRole("button", { name: "添加订阅", exact: true }).click();
  await expect(page.getByRole("alert")).toContainText("该资讯来源已订阅");
  await page.getByRole("button", { name: "移除 中文 Agent 动态 订阅", exact: true }).click();
  await expect(page.getByRole("checkbox", { name: "中文 Agent 动态", exact: true })).toHaveCount(0);
  await expect(page.getByRole("button", { name: "多智能体协作的新进展", exact: true })).toHaveCount(0);
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
  await page.getByRole("button", { name: "订阅来源", exact: true }).click();
  await page.getByRole("button", { name: "中文", exact: true }).click();
  await page.setViewportSize({ width: 1440, height: 900 });
  await page.screenshot({ path: join(process.cwd(), "test-results", "agent-news-chinese.png"), fullPage: true });
  await page.getByRole("button", { name: "原文", exact: true }).click();
  await expect(page.getByRole("button", { name: "A new model for agents", exact: true })).toBeVisible();
  expect(errors).toEqual([]);
});

test("浏览器明确显示采集边界且命令面板可进入专栏", async ({ page }) => {
  await page.goto("/");
  await page.keyboard.press("Control+k");
  await page.getByRole("combobox", { name: "搜索命令" }).fill("Agent 资讯");
  await page.keyboard.press("Enter");
  await expect(page.getByRole("heading", { level: 1 })).toHaveText("Agent 资讯");
  await expect(page.getByText("资讯采集需要桌面环境，请使用桌面版个人工具箱。")).toBeVisible();
  await expect(page.getByRole("button", { name: "刷新资讯", exact: true })).toBeDisabled();
});

test("全局推荐展开收起、键盘切换、预览和全部入口", async ({ page }) => {
  await page.goto("/");
  const dock = page.getByRole("complementary", { name: "资讯推荐", exact: true });
  await expect(dock).toContainText("2 条");
  await expect(dock.getByRole("button", { name: "展开资讯推荐" })).toHaveAttribute("aria-expanded", "false");
  await dock.getByRole("button", { name: "展开资讯推荐" }).click();
  await expect(dock.locator(".news-recommendation-position")).toHaveText("1 / 2");
  const first = await dock.locator("h2").innerText();
  await dock.getByRole("button", { name: "下一条推荐" }).click();
  await expect(dock.locator("h2")).not.toHaveText(first);
  await dock.getByRole("button", { name: "上一条推荐" }).press("ArrowRight");
  await expect(dock.locator("h2")).toHaveText(first);
  await dock.getByRole("button", { name: "收起资讯推荐" }).press("Escape");
  await expect(dock.getByRole("button", { name: "展开资讯推荐" })).toBeFocused();
  await page.getByRole("button", { name: "Agents", exact: true }).click();
  await dock.getByRole("button", { name: "展开资讯推荐" }).click();
  await expect(dock.locator("h2")).toHaveText(first);
  await dock.getByRole("button", { name: "阅读预览", exact: true }).click();
  await expect(page.getByRole("dialog")).toContainText("Agent planning and tool use.");
  expect(await page.evaluate(() => (window as unknown as { openedUrl?: string }).openedUrl)).toBeUndefined();
  await page.getByRole("dialog").getByRole("button", { name: "打开原网页", exact: true }).click();
  expect(await page.evaluate(() => (window as unknown as { openedUrl?: string }).openedUrl)).toMatch(/^https:/);
  await page.keyboard.press("Escape");
  await expect(dock.locator(".news-recommendation-title")).toBeFocused();
  await dock.getByRole("button", { name: "全部资讯", exact: true }).click();
  await expect(page.getByRole("heading", { level: 1 })).toHaveText("Agent 资讯");
  await expect(dock.getByRole("button", { name: "展开资讯推荐" })).toBeVisible();
  await dock.getByRole("button", { name: "展开资讯推荐" }).click();
  await dock.getByRole("button", { name: "下一条推荐" }).click();
  const beforeRefresh = await dock.locator("h2").innerText();
  await page.evaluate(() => { (window as unknown as { newsInsertOnRefresh: boolean }).newsInsertOnRefresh = true; });
  await page.getByRole("button", { name: "刷新资讯", exact: true }).click();
  await expect(dock).toContainText("3 条");
  await expect(dock.locator("h2")).toHaveText(beforeRefresh);
});

test("轮播自动循环、阅读暂停、手动暂停和后台停止", async ({ page }) => {
  await page.emulateMedia({ reducedMotion: "no-preference" });
  await page.clock.install();
  await page.goto("/");
  const dock = page.getByRole("complementary", { name: "资讯推荐", exact: true });
  await dock.getByRole("button", { name: "展开资讯推荐" }).click();
  const position = dock.locator(".news-recommendation-position");
  const leave = async () => { await page.mouse.move(0, 0); await page.evaluate(() => (document.activeElement as HTMLElement)?.blur()); };
  await leave();
  await page.clock.fastForward(8100);
  await expect(position).toHaveText("2 / 2");
  await page.clock.fastForward(8100);
  await expect(position).toHaveText("1 / 2");
  await dock.hover();
  await page.clock.fastForward(16000);
  await expect(position).toHaveText("1 / 2");
  await page.mouse.move(0, 0);
  await dock.getByRole("button", { name: "下一条推荐" }).focus();
  await page.clock.fastForward(16000);
  await expect(position).toHaveText("1 / 2");
  await dock.getByRole("button", { name: "暂停资讯轮播" }).click();
  await leave();
  await page.clock.fastForward(16000);
  await expect(position).toHaveText("1 / 2");
  await dock.getByRole("button", { name: "播放资讯轮播" }).click();
  await leave();
  await page.evaluate(() => { Object.defineProperty(document, "hidden", { configurable: true, value: true }); document.dispatchEvent(new Event("visibilitychange")); });
  await page.clock.fastForward(16000);
  await expect(position).toHaveText("1 / 2");
  await page.evaluate(() => { Object.defineProperty(document, "hidden", { configurable: true, value: false }); document.dispatchEvent(new Event("visibilitychange")); });
  await page.clock.fastForward(8100);
  await expect(position).toHaveText("2 / 2");
  await dock.getByRole("button", { name: "阅读预览", exact: true }).click();
  await page.clock.fastForward(16000);
  await expect(position).toHaveText("2 / 2");
  await page.keyboard.press("Escape");
  await dock.getByRole("button", { name: "收起资讯推荐" }).click();
  await leave();
  await page.clock.fastForward(16000);
  await dock.getByRole("button", { name: "展开资讯推荐" }).click();
  await expect(position).toHaveText("2 / 2");
});

test("推荐中文、停用来源、单条空态及浅深色响应布局", async ({ page }) => {
  await page.goto("/");
  const dock = page.getByRole("complementary", { name: "资讯推荐", exact: true });
  await page.getByRole("button", { name: "Agent 资讯", exact: true }).click();
  await page.getByRole("button", { name: "中文", exact: true }).click();
  await expect(page.locator(".news-item")).toContainText(["Codex 功能更新", "面向智能体的新模型"]);
  await page.getByRole("button", { name: "用量总览", exact: true }).click();
  await page.setViewportSize({ width: 375, height: 812 });
  const collapsedBounds = await dock.boundingBox();
  expect(collapsedBounds!.width).toBeLessThanOrEqual(56);
  await expect(dock.getByRole("button", { name: "展开资讯推荐" })).toHaveAttribute("title", "展开资讯推荐");
  await dock.getByRole("button", { name: "展开资讯推荐" }).click();
  await expect(dock.getByRole("button", { name: "播放资讯轮播" })).toBeVisible();
  await expect(dock).toContainText("机器翻译");
  for (const [width, height] of [[1440, 900], [853, 480], [375, 812]]) {
    await page.setViewportSize({ width, height });
    const bounds = await dock.boundingBox();
    expect(bounds!.x).toBeGreaterThanOrEqual(0);
    expect(bounds!.y).toBeGreaterThanOrEqual(0);
    expect(bounds!.x + bounds!.width).toBeLessThanOrEqual(width);
    expect(bounds!.y + bounds!.height).toBeLessThanOrEqual(height - 40);
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
  }
  await page.screenshot({ path: join(process.cwd(), "test-results", "news-recommendations-mobile.png") });
  await page.setViewportSize({ width: 1440, height: 900 });
  await page.screenshot({ path: join(process.cwd(), "test-results", "news-recommendations-light.png") });
  await page.getByRole("button", { name: "切换深色主题", exact: true }).click();
  await page.screenshot({ path: join(process.cwd(), "test-results", "news-recommendations-dark.png") });
  await dock.getByRole("button", { name: "全部资讯", exact: true }).click();
  await page.getByRole("button", { name: "订阅来源", exact: true }).click();
  await page.getByRole("checkbox", { name: "OpenAI Codex", exact: true }).uncheck();
  await dock.getByRole("button", { name: "展开资讯推荐" }).click();
  await expect(dock.locator(".news-recommendation-position")).toHaveText("1 / 1");
  await expect(dock.getByRole("button", { name: "下一条推荐" })).toBeDisabled();
  await page.getByRole("checkbox", { name: "OpenAI News", exact: true }).uncheck();
  await expect(dock).toContainText("最近 30 天暂无可推荐资讯");
  await expect(dock.locator(".news-recommendation-slide")).toHaveCount(0);
});
