import { test, expect } from "@playwright/test";
import { initialSettings } from "../src/store";
import type { EnvironmentChange, EnvironmentScopeSnapshot, EnvironmentSnapshot, EnvironmentVariable } from "../src/environment-api";

declare global {
  interface Window {
    environmentTest: {
      snapshot: EnvironmentSnapshot;
      changes: EnvironmentChange[];
      fail: boolean;
      readFail: boolean;
      warning: string | null;
      delay: boolean;
    };
  }
}

test.beforeEach(async ({ page }) => {
  if (test.info().title.startsWith("浏览器")) return;
  await page.addInitScript(settings => {
    // 仅测试页面的 IPC 边界；生产没有此桥，也不访问真实环境变量。
    const user: EnvironmentScopeSnapshot = { scope: "user", writable: true, error: null, variables: [
      { name: "Path", value: "C:\\Tools;;%JAVA_HOME%\\bin;", kind: "expandString", revision: "path-1" },
      { name: "JAVA_HOME", value: "D:\\开发\\jdk", kind: "string", revision: "java-1" },
    ] };
    window.environmentTest = { snapshot: { supported: true, scopes: [user, { scope: "system", writable: false, error: null, variables: [{ name: "Path", value: "C:\\Windows", kind: "expandString", revision: "system-path" }] }] }, changes: [], fail: false, readFail: false, warning: null, delay: false };
    Object.assign(window, { isTauri: true, __TAURI_INTERNALS__: { invoke: async (name: string, args: Record<string, unknown> = {}) => {
      const state = window.environmentTest;
      if (name === "get_environment_variables") {
        if (state.readFail) throw new Error("测试：注册表读取失败");
        return structuredClone(state.snapshot);
      }
      if (name === "change_environment_variable") {
        const change = args.change as EnvironmentChange;
        state.changes.push(structuredClone(change));
        if (state.delay) await new Promise(resolve => setTimeout(resolve, 500));
        if (state.fail) throw new Error("测试：写入权限不足");
        const target = state.snapshot.scopes.find(item => item.scope === change.scope)!;
        if (!target.writable) throw new Error("测试：只读作用域");
        const old = target.variables.find(item => item.name.toLowerCase() === change.name.toLowerCase());
        if ((old?.revision ?? null) !== change.expectedRevision) throw new Error("变量已被外部程序修改或同名变量已存在，请刷新后重试");
        target.variables = target.variables.filter(item => item !== old);
        const variable: EnvironmentVariable | null = change.value === null ? null : { name: change.name, value: change.value, kind: change.kind, revision: `revision-${state.changes.length}` };
        if (variable) target.variables.push(variable);
        return { variable, warning: state.warning };
      }
      if (name === "get_settings") return settings;
      if (name === "update_settings") return null;
      if (name === "get_snapshot") return null;
      if (name === "refresh_usage") return { sources: [], records: [], collectedAt: new Date().toISOString() };
      if (name === "get_agent_news" || name === "refresh_agent_news") return { sources: [], automatic: false, language: "original" };
      throw new Error(`未处理的测试命令：${name}`);
    } } });
  }, initialSettings);
  await page.goto("/");
  await page.getByRole("button", { name: "环境变量", exact: true }).click();
  await expect(page.getByRole("button", { name: "编辑 JAVA_HOME", exact: true })).toBeEnabled();
});

test("新增空值、编辑保留类型、删除需确认且取消不写入", async ({ page }) => {
  await page.getByRole("button", { name: "新增变量", exact: true }).click();
  await page.getByRole("textbox", { name: "变量名", exact: true }).fill("TOOLBOX_TEST");
  await page.getByRole("combobox", { name: "值类型" }).selectOption("string");
  await page.getByRole("button", { name: "保存修改", exact: true }).click();
  await expect(page.getByRole("row", { name: /TOOLBOX_TEST/ })).toContainText("（空字符串）");
  await page.getByRole("button", { name: "编辑 JAVA_HOME", exact: true }).click();
  await expect(page.getByRole("textbox", { name: "变量名", exact: true })).toHaveAttribute("readonly", "");
  await expect(page.getByRole("combobox", { name: "值类型" })).toHaveValue("string");
  await page.getByRole("textbox", { name: "变量值", exact: true }).fill("D:\\新工具\\jdk-25");
  await page.getByRole("button", { name: "保存修改", exact: true }).click();
  await expect(page.getByRole("row", { name: /^JAVA_HOME / })).toContainText("D:\\新工具\\jdk-25");
  await page.getByRole("button", { name: "删除 TOOLBOX_TEST", exact: true }).click();
  await page.getByRole("button", { name: "取消", exact: true }).click();
  expect(await page.evaluate(() => window.environmentTest.changes.length)).toBe(2);
  await page.getByRole("button", { name: "删除 TOOLBOX_TEST", exact: true }).click();
  await page.getByRole("button", { name: "确认删除", exact: true }).click();
  await expect(page.getByRole("row", { name: /TOOLBOX_TEST/ })).toHaveCount(0);
  expect(await page.evaluate(() => window.environmentTest.changes[2].value)).toBeNull();
});

test("Path 模式转换保留引用、顺序、空条目和尾分号", async ({ page }) => {
  await page.getByRole("button", { name: "编辑 Path", exact: true }).click();
  await expect(page.getByRole("textbox", { name: "变量值", exact: true })).toHaveValue("C:\\Tools\n\n%JAVA_HOME%\\bin\n");
  await page.getByRole("button", { name: "原始值", exact: true }).click();
  await expect(page.getByRole("textbox", { name: "变量值", exact: true })).toHaveValue("C:\\Tools;;%JAVA_HOME%\\bin;");
  await page.getByRole("button", { name: "每行一个路径", exact: true }).click();
  await page.getByRole("textbox", { name: "变量值", exact: true }).fill("C:\\Tools\n\n%JAVA_HOME%\\bin\nD:\\新增路径\n");
  await page.getByRole("button", { name: "保存修改", exact: true }).click();
  await expect(page.getByRole("dialog")).toHaveCount(0);
  expect(await page.evaluate(() => window.environmentTest.changes[0])).toMatchObject({ scope: "user", name: "Path", value: "C:\\Tools;;%JAVA_HOME%\\bin;D:\\新增路径;", kind: "expandString", expectedRevision: "path-1" });
});

test("写入失败保留输入，外部修改阻止覆盖，重复点击只提交一次", async ({ page }) => {
  await page.getByRole("button", { name: "编辑 JAVA_HOME", exact: true }).click();
  await page.getByRole("textbox", { name: "变量值", exact: true }).fill("new-value");
  await page.evaluate(() => { window.environmentTest.fail = true; window.environmentTest.delay = true; });
  await page.getByRole("button", { name: "保存修改", exact: true }).click();
  await expect(page.getByRole("button", { name: "正在保存…", exact: true })).toBeDisabled();
  await expect(page.getByRole("dialog")).toContainText("写入权限不足");
  await expect(page.getByRole("textbox", { name: "变量值", exact: true })).toHaveValue("new-value");
  expect(await page.evaluate(() => window.environmentTest.changes.length)).toBe(1);
  await page.evaluate(() => { window.environmentTest.fail = false; window.environmentTest.snapshot.scopes[0].variables[1].revision = "external"; });
  await page.getByRole("button", { name: "保存修改", exact: true }).click();
  await expect(page.getByRole("dialog")).toContainText("外部程序修改");
  await expect(page.getByRole("textbox", { name: "变量值", exact: true })).toHaveValue("new-value");
});

test("只读系统作用域、通知失败提示、刷新失败保留数据", async ({ page }) => {
  await page.getByRole("button", { name: "系统变量", exact: true }).click();
  await expect(page.getByText("系统变量为只读", { exact: false })).toBeVisible();
  await expect(page.getByRole("button", { name: "新增变量", exact: true })).toBeDisabled();
  await expect(page.getByRole("button", { name: "编辑 Path", exact: true })).toBeDisabled();
  await page.getByRole("button", { name: "用户变量", exact: true }).click();
  await page.evaluate(() => { window.environmentTest.warning = "已保存，但环境变更通知未完成。"; });
  await page.getByRole("button", { name: "编辑 JAVA_HOME", exact: true }).click();
  await page.getByRole("button", { name: "保存修改", exact: true }).click();
  await expect(page.getByRole("status")).toContainText("已保存，但环境变更通知未完成");
  await page.evaluate(() => { window.environmentTest.readFail = true; });
  await page.getByRole("button", { name: "刷新变量", exact: true }).click();
  await expect(page.getByRole("alert")).toContainText("注册表读取失败");
  await expect(page.getByRole("row", { name: /^JAVA_HOME / })).toBeVisible();
  await expect(page.getByRole("button", { name: "编辑 JAVA_HOME", exact: true })).toBeDisabled();
});

test("系统变量有写权限时保留独立作用域，读取异常和未知类型禁止编辑", async ({ page }) => {
  await page.evaluate(() => { window.environmentTest.snapshot.scopes[1].writable = true; });
  await page.getByRole("button", { name: "刷新变量", exact: true }).click();
  await page.getByRole("button", { name: "系统变量", exact: true }).click();
  await page.getByRole("button", { name: "编辑 Path", exact: true }).click();
  await expect(page.getByRole("dialog")).toContainText("此次操作影响此电脑所有用户");
  await page.getByRole("textbox", { name: "变量值", exact: true }).fill("C:\\Windows\nD:\\SystemTools");
  await page.getByRole("button", { name: "保存修改", exact: true }).click();
  await expect(page.getByRole("dialog")).toHaveCount(0);
  expect(await page.evaluate(() => window.environmentTest.changes[0])).toMatchObject({ scope: "system", expectedRevision: "system-path" });
  await page.getByRole("button", { name: "用户变量", exact: true }).click();
  await expect(page.getByRole("row", { name: /^Path / }).getByText("C:\\Tools", { exact: true })).toBeVisible();
  await expect(page.getByRole("row", { name: /^Path / }).getByText("%JAVA_HOME%", { exact: true })).toBeVisible();
  await page.evaluate(() => { window.environmentTest.snapshot.scopes[0].variables.push({ name: "UNKNOWN", value: "不支持的类型", kind: "unsupported", revision: "unknown" }); });
  await page.getByRole("button", { name: "刷新变量", exact: true }).click();
  await expect(page.getByRole("button", { name: "编辑 UNKNOWN", exact: true })).toBeDisabled();
  await page.evaluate(() => { window.environmentTest.snapshot.scopes[0].error = "测试：作用域读取失败"; window.environmentTest.snapshot.scopes[0].variables = []; });
  await page.getByRole("button", { name: "刷新变量", exact: true }).click();
  await expect(page.getByRole("alert")).toContainText("作用域读取失败");
  await expect(page.getByRole("row", { name: /^JAVA_HOME / })).toBeVisible();
  await expect(page.getByRole("button", { name: "编辑 JAVA_HOME", exact: true })).toBeDisabled();
});

test("搜索、名称验证、主题、尺寸和键盘弹窗", async ({ page }) => {
  const errors: string[] = [];
  page.on("pageerror", e => errors.push(e.message));
  await page.getByRole("textbox", { name: "搜索环境变量" }).fill("开发");
  await expect(page.locator(".environment-table tbody tr")).toHaveCount(1);
  await page.getByRole("textbox", { name: "搜索环境变量" }).fill("missing");
  await expect(page.getByText("没有匹配的变量", { exact: true })).toBeVisible();
  await page.getByRole("textbox", { name: "搜索环境变量" }).fill("");
  for (const [width, height] of [[1440, 900], [853, 480], [375, 812]]) {
    await page.setViewportSize({ width, height });
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
  }
  await page.setViewportSize({ width: 1440, height: 900 });
  await page.screenshot({ path: "test-results/environment-light.png", fullPage: true });
  await page.getByRole("button", { name: "切换深色主题" }).click();
  await page.screenshot({ path: "test-results/environment-dark.png", fullPage: true });
  await page.getByRole("button", { name: "新增变量", exact: true }).click();
  await page.screenshot({ path: "test-results/environment-editor.png", fullPage: true });
  await expect(page.getByRole("textbox", { name: "变量名", exact: true })).toBeFocused();
  await page.getByRole("textbox", { name: "变量名", exact: true }).fill("BAD=NAME");
  await page.getByRole("button", { name: "保存修改", exact: true }).click();
  await expect(page.getByRole("dialog")).toContainText("不能包含等号");
  expect(await page.evaluate(() => window.environmentTest.changes.length)).toBe(0);
  await page.keyboard.press("Escape");
  await expect(page.getByRole("dialog")).toHaveCount(0);
  expect(errors).toEqual([]);
});

test("Path 分行编号、引用高亮与长值展开不改变原始数据", async ({ page }) => {
  const paths = ["C:\\Windows\\System32", "%JAVA_HOME%\\bin", "D:\\Tools\\node", "", "D:\\开发工具\\Python\\Scripts", "C:\\Users\\Developer\\.cargo\\bin", "C:\\Users\\Developer\\.local\\bin", "%USERPROFILE%\\AppData\\Local\\Microsoft\\WindowsApps", ""];
  const longValue = "D:\\" + "很长的开发工具安装目录\\".repeat(24);
  await page.evaluate(({ paths, longValue }) => {
    const variables = window.environmentTest.snapshot.scopes[0].variables;
    variables[0].value = paths.join(";");
    variables.push(
      { name: "TOOLCHAIN_CACHE_DIRECTORY", value: longValue, kind: "string", revision: "long" },
      { name: "MAVEN_HOME", value: "D:\\Tools\\maven\\current", kind: "string", revision: "maven" },
      { name: "GRADLE_USER_HOME", value: "%USERPROFILE%\\.gradle", kind: "expandString", revision: "gradle" },
      { name: "NODE_OPTIONS", value: "--max-old-space-size=8192", kind: "string", revision: "node" },
      { name: "EMPTY_VALUE", value: "", kind: "string", revision: "empty" },
    );
  }, { paths, longValue });
  await page.getByRole("button", { name: "刷新变量", exact: true }).click();
  const pathRow = page.getByRole("row", { name: /^Path / });
  await expect(pathRow.locator(".environment-path-list li:visible")).toHaveCount(3);
  await expect(pathRow.locator(".environment-reference").first()).toHaveText("%JAVA_HOME%");
  await pathRow.getByText("展开其余 6 项", { exact: true }).click();
  await expect(pathRow.locator(".environment-path-list li:visible")).toHaveCount(9);
  await expect(pathRow.locator(".environment-empty-value:visible")).toHaveCount(2);
  await expect(pathRow.locator("ol[start='4']")).toBeVisible();
  await pathRow.getByText("收起路径", { exact: true }).click();
  await expect(pathRow.locator(".environment-path-list li:visible")).toHaveCount(3);
  const longRow = page.getByRole("row", { name: /^TOOLCHAIN_CACHE_DIRECTORY / });
  await longRow.locator("summary").focus();
  await page.keyboard.press("Enter");
  await expect(longRow.locator("details")).toHaveAttribute("open", "");
  await expect(longRow.locator("details .environment-value-text")).toHaveText(longValue);
  await longRow.locator("summary").click();
  await page.setViewportSize({ width: 1440, height: 1000 });
  await page.locator(".environment-heading h1").click();
  await page.evaluate(() => window.scrollTo(0, 0));
  await page.screenshot({ path: "test-results/environment-readable-light.png" });
  await page.getByRole("button", { name: "切换深色主题" }).click();
  await page.getByRole("button", { name: "关闭提示", exact: true }).click();
  await page.screenshot({ path: "test-results/environment-readable-dark.png" });
  await page.getByRole("button", { name: "编辑 Path", exact: true }).click();
  await expect(page.getByRole("textbox", { name: "变量值", exact: true })).toHaveValue(paths.join("\n"));
  await page.screenshot({ path: "test-results/environment-path-editor.png" });
  await page.getByRole("button", { name: "取消", exact: true }).click();
  for (const [width, height] of [[853, 480], [375, 812]]) {
    await page.setViewportSize({ width, height });
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
  }
  await page.screenshot({ path: "test-results/environment-readable-mobile.png", fullPage: true });
  expect(await page.evaluate(() => window.environmentTest.snapshot.scopes[0].variables[0].value)).toBe(paths.join(";"));
  expect(await page.evaluate(() => window.environmentTest.changes)).toEqual([]);
});

test("浏览器明确不连接本机环境变量", async ({ page }) => {
  await page.goto("/");
  await page.getByRole("button", { name: "环境变量", exact: true }).click();
  await expect(page.getByText("环境变量需要桌面环境", { exact: true })).toBeVisible();
  await expect(page.getByRole("button", { name: "新增变量", exact: true })).toHaveCount(0);
});
