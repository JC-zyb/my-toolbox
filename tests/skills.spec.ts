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
import {
  readFileSync,
  existsSync,
  mkdirSync,
  writeFileSync,
  lstatSync,
} from "node:fs";

let child: ChildProcessWithoutNullStreams;
let root: string;
let queue: { resolve: (v: unknown) => void; reject: (e: Error) => void }[] = [];
test.beforeAll(() => {
  execFileSync(
    "cargo",
    [
      "build",
      "--manifest-path",
      "src-tauri/Cargo.toml",
      "--no-default-features",
      "--bin",
      "skills-fixture",
    ],
    { cwd: process.cwd(), stdio: "pipe" },
  );
});
test.beforeEach(async ({ page }) => {
  root = join(tmpdir(), `toolbox-skills-ui-${randomUUID()}`);
  queue = [];
  child = spawn(
    join(process.cwd(), "src-tauri/target/debug/skills-fixture.exe"),
    [root],
  );
  const ready = new Promise<void>((resolve, reject) => {
    child.once("error", reject);
    const lines = createInterface({ input: child.stdout });
    lines.on("line", (line) => {
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
      for (const pending of queue)
        pending.reject(new Error(`Rust bridge exited ${code}`));
      queue = [];
    });
  });
  await ready;
  await page.exposeFunction(
    "skillsTestInvoke",
    (name: string, args: Record<string, unknown>) =>
      new Promise((resolve, reject) => {
        queue.push({ resolve, reject });
        child.stdin.write(JSON.stringify({ name, args: args ?? {} }) + "\n");
      }),
  );
  await page.addInitScript(() =>
    Object.assign(window, {
      isTauri: true,
      __TAURI_INTERNALS__: {
        invoke: (name: string, args: unknown) =>
          (
            window as unknown as {
              skillsTestInvoke: (n: string, a: unknown) => Promise<unknown>;
            }
          ).skillsTestInvoke(name, args),
      },
    }),
  );
  await page.goto("/", { waitUntil: "domcontentloaded" });
  await page.getByRole("button", { name: "Skills", exact: true }).click();
  await expect(
    page.getByRole("dialog", { name: "迁移现有公共 Skills" }),
  ).toBeVisible();
});
test.afterEach(() => {
  child.stdin.end();
});

test("迁移冲突预览、确认备份及列表筛选", async ({ page }) => {
  const dialog = page.getByRole("dialog", { name: "迁移现有公共 Skills" });
  await expect(dialog).toContainText("冲突 1 个");
  await expect(
    dialog.getByRole("button", { name: "备份并迁移" }),
  ).toBeEnabled();
  await dialog.locator("summary").first().click();
  await dialog.getByLabel("迁移选择").selectOption("target");
  await dialog.getByRole("button", { name: "备份并迁移" }).click();
  await expect(dialog).toHaveCount(0);
  await expect(
    page.locator(".skill-toast").filter({ hasText: "已迁移" }),
  ).toBeVisible();
  expect(
    readFileSync(
      join(root, "home/.agents/skills/code-review/SKILL.md"),
      "utf8",
    ),
  ).toContain("旧库内容");
  expect(
    existsSync(join(root, "home/.skills-manager/skills/code-review/SKILL.md")),
  ).toBe(true);
  await page.getByLabel("搜索 Skill", { exact: true }).fill("ui-kit");
  await expect(page.locator(".skill-card")).toHaveCount(1);
  await page.getByLabel("搜索 Skill", { exact: true }).fill("");
  await page.setViewportSize({ width: 1440, height: 900 });
  await page.screenshot({
    path: join(process.cwd(), "test-results/skills-light.png"),
    fullPage: true,
  });
  await page.getByRole("button", { name: "切换深色主题" }).click();
  await expect(page.locator("html")).toHaveAttribute("data-theme", "dark");
  await page.screenshot({
    path: join(process.cwd(), "test-results/skills-dark.png"),
    fullPage: true,
  });
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
});
test("卡片详情即时启停 Agent、渲染正文并从命令面板打开", async ({ page }) => {
  await page
    .getByRole("dialog")
    .getByRole("button", { name: "暂不迁移" })
    .click();
  const card = page.locator(".skill-card").filter({ hasText: "code-review" });
  await expect(card.getByLabel("Codex · 已启用")).toBeVisible();
  await expect(card.getByLabel("Claude Code · 已关闭")).toBeVisible();
  await card.getByRole("button", { name: "查看 code-review" }).click();
  const detail = page.getByRole("region", { name: "code-review 详情" });
  await expect(detail).toBeVisible({ timeout: 5000 });
  await expect(detail.getByRole("heading", { name: "检查代码变化与边界" })).toBeVisible();
  expect(existsSync(join(root, "home/.claude/skills/code-review"))).toBe(false);
  await detail.getByRole("checkbox", { name: "启用 Claude Code" }).click();
  await expect(detail.getByRole("checkbox", { name: "启用 Claude Code" })).toBeChecked();
  await expect.poll(() => existsSync(join(root, "home/.claude/skills/code-review"))).toBe(true);
  expect(lstatSync(join(root, "home/.claude/skills/code-review")).isSymbolicLink()).toBe(true);
  await detail.getByRole("checkbox", { name: "启用 Claude Code" }).click();
  await expect(detail.getByRole("checkbox", { name: "启用 Claude Code" })).not.toBeChecked();
  await expect.poll(() => existsSync(join(root, "home/.claude/skills/code-review"))).toBe(false);
  await detail.getByRole("button", { name: "查看源码" }).click();
  await expect(detail.locator("pre.skill-document")).toContainText("name: code-review");
  await detail.getByRole("tab", { name: "来源" }).click();
  await expect(detail).toContainText("SHA-256");
  await detail.getByRole("button", { name: "返回技能库" }).click();
  await expect(card).toBeVisible();
  await page.keyboard.press("Control+k");
  await page.getByRole("combobox", { name: "搜索命令" }).fill("Skill ui-kit");
  await page.keyboard.press("Enter");
  await expect(page.getByRole("region", { name: "ui-kit 详情" })).toBeVisible();
});
test("创建校验和 GitHub 导入错误不会显示成功", async ({ page }) => {
  await page
    .getByRole("dialog")
    .getByRole("button", { name: "暂不迁移" })
    .click();
  await page.getByRole("button", { name: "新建 Skill", exact: true }).click();
  const editor = page.getByRole("dialog", { name: "新建 Skill", exact: true });
  await editor.getByLabel("名称", { exact: true }).fill("example");
  await editor
    .locator("#skill-document")
    .fill("---\nname: example\ndescription: 测试创建\n---\n\n正文\n");
  await editor.getByRole("button", { name: "保存 Skill" }).click();
  await expect(editor).toHaveCount(0);
  expect(existsSync(join(root, "home/.agents/skills/example/SKILL.md"))).toBe(
    true,
  );
  await page.getByRole("button", { name: "导入 GitHub", exact: true }).click();
  const github = page.getByRole("dialog", { name: "导入 GitHub", exact: true });
  await github.getByLabel("GitHub 仓库 URL").fill("https://example.com/repo");
  await github.getByRole("button", { name: "浏览仓库" }).click();
  await expect(github.getByRole("alert")).toContainText("只支持 HTTPS GitHub");
  await expect(github).toBeVisible();
  await expect(github.getByRole("button", { name: /导入所选/ })).toHaveCount(0);
});

test("全局 Codex 勾选更新隔离配置并提示重启", async ({ page }) => {
  await page.getByRole("dialog").getByRole("button", { name: "暂不迁移" }).click();
  await page.locator(".skill-card").filter({ hasText: "ui-kit" }).getByRole("button", { name: "查看 ui-kit" }).click();
  await page.getByRole("button", { name: "展开资讯推荐" }).click();
  const detail = page.getByRole("region", { name: "ui-kit 详情" });
  await expect(detail.getByRole("checkbox", { name: "启用 Codex" })).toBeChecked();
  await detail.getByRole("checkbox", { name: "启用 Codex" }).click();
  await expect(detail.getByRole("checkbox", { name: "启用 Codex" })).not.toBeChecked();
  await expect(detail).toContainText("重启 Codex 后生效");
  const notification = page.locator("#corner-notifications .skill-toast").last();
  await expect(notification).toBeVisible();
  const toastBounds = await notification.boundingBox();
  const recommendationBounds = await page.locator(".news-recommendations").boundingBox();
  expect(toastBounds!.y + toastBounds!.height).toBeLessThanOrEqual(recommendationBounds!.y);
  await expect.poll(() => existsSync(join(root, "home/.codex/config.toml"))).toBe(true);
  const config = readFileSync(join(root, "home/.codex/config.toml"), "utf8");
  expect(config).toContain("enabled = false");
  expect(config).toContain("ui-kit\\SKILL.md");
  await detail.getByRole("button", { name: "返回技能库" }).click();
  await expect(page.locator(".skill-card").filter({ hasText: "ui-kit" }).getByLabel("Codex · 已关闭")).toBeVisible();
});

test("卡片多选批量启停保留中央库，支持键盘选择", async ({ page }) => {
  await page
    .getByRole("dialog")
    .getByRole("button", { name: "暂不迁移" })
    .click();
  const cards = page.locator(".skill-card");
  await expect(cards).toHaveCount(2);
  await page.getByRole("button", { name: "多选", exact: true }).click();
  const check = page.getByLabel("选择技能 code-review");
  await check.focus();
  await page.keyboard.press("Space");
  await page.getByLabel("选择技能 ui-kit").check();
  await page.getByRole("button", { name: "批量启用" }).click();
  const dialog = page.getByRole("dialog", { name: "批量操作预览" });
  await expect(dialog).toContainText("共 4 项");
  await expect(dialog).toContainText(
    join(root, "home/.claude/skills/code-review"),
  );
  expect(existsSync(join(root, "home/.claude/skills/code-review"))).toBe(false);
  await dialog.getByRole("button", { name: "确认应用批量操作" }).click();
  await expect(dialog).toHaveCount(0);
  for (const agent of ["claude", "dsh"])
    for (const name of ["code-review", "ui-kit"]) {
      expect(
        lstatSync(join(root, `home/.${agent}/skills/${name}`)).isSymbolicLink(),
      ).toBe(true);
    }
  expect(existsSync(join(root, "home/.codex/skills/code-review"))).toBe(false);
  await expect(cards.getByLabel("Claude Code · 已启用")).toHaveCount(2);
  await page.getByRole("button", { name: "批量关闭" }).click();
  await expect(dialog).toContainText("移除安装并保留备份");
  await dialog.getByRole("button", { name: "确认应用批量操作" }).click();
  await expect(dialog).toHaveCount(0);
  expect(existsSync(join(root, "home/.claude/skills/code-review"))).toBe(false);
  expect(
    existsSync(join(root, "home/.agents/skills/code-review/SKILL.md")),
  ).toBe(true);
  expect(existsSync(join(root, "home/.agents/skills/ui-kit/SKILL.md"))).toBe(
    true,
  );
  await expect(cards.getByLabel("Claude Code · 已关闭")).toHaveCount(2);
});

test("Agent 独有技能经确认导入；外部目录关闭保留备份", async ({ page }) => {
  await page.getByRole("dialog").getByRole("button", { name: "暂不迁移" }).click();
  const origin = join(root, "home/.claude/skills/external");
  mkdirSync(origin, { recursive: true });
  writeFileSync(join(origin, "SKILL.md"), "---\nname: external\ndescription: 外部技能\n---\n外部版本\n");
  await page.getByRole("button", { name: "扫描", exact: true }).click();
  await page.getByLabel("筛选 Skill 状态").selectOption("target_only");
  const card = page.locator(".skill-card");
  await expect(card).toHaveCount(1);
  await expect(card).toContainText("尚未导入");
  await card.getByRole("button", { name: "查看 external" }).click();
  const detail = page.getByRole("region", { name: "external 详情" });
  await detail.getByRole("checkbox", { name: "启用 DSH" }).click();
  const confirm = page.getByRole("dialog", { name: "启停确认" });
  await expect(confirm).toContainText("首次导入中央库");
  await confirm.getByRole("button", { name: "确认应用启停" }).click();
  await expect(confirm).toHaveCount(0);
  expect(existsSync(join(root, "home/.agents/skills/external/SKILL.md"))).toBe(true);
  expect(lstatSync(join(root, "home/.dsh/skills/external")).isSymbolicLink()).toBe(true);
  await detail.getByRole("checkbox", { name: "启用 Claude Code" }).click();
  await expect(confirm).toContainText(origin);
  await confirm.getByRole("button", { name: "确认应用启停" }).click();
  await expect(confirm).toHaveCount(0);
  expect(existsSync(origin)).toBe(false);
  expect(existsSync(join(root, "home/.agents/skills/external/SKILL.md"))).toBe(true);
});

test("连续点击保留最后意图，详情和卡片同步且不触发全量扫描", async ({ page }, testInfo) => {
  await page.getByRole("dialog").getByRole("button", { name: "暂不迁移" }).click();
  const card = page.locator(".skill-card").filter({ hasText: "ui-kit" });
  await expect(card.getByLabel("Claude Code · 已关闭")).toBeVisible();
  // 只延迟真实 Rust 请求，不替换任何成功或失败结果。
  await page.evaluate(() => {
    const bridge = window as unknown as { __TAURI_INTERNALS__: { invoke: (name: string, args: Record<string, unknown>) => Promise<unknown> }; activationCalls: { name: string; args: Record<string, unknown> }[] };
    const invoke = bridge.__TAURI_INTERNALS__.invoke;
    bridge.activationCalls = [];
    bridge.__TAURI_INTERNALS__.invoke = async (name, args) => {
      bridge.activationCalls.push({ name, args });
      if (name === "quick_skill_activation") await new Promise(resolve => setTimeout(resolve, 800));
      return invoke(name, args);
    };
  });
  const agent = card.getByRole("button", { name: /^Claude Code/ });
  await agent.click();
  await expect(agent).toHaveAttribute("aria-pressed", "true");
  await expect(agent).toBeEnabled();
  await agent.click(); await agent.click(); await agent.click();
  await expect(agent).toHaveAttribute("aria-pressed", "false");
  await card.getByRole("button", { name: "查看 ui-kit" }).click();
  const detail = page.getByRole("region", { name: "ui-kit 详情" });
  await expect(detail.getByRole("checkbox", { name: "启用 Claude Code" })).not.toBeChecked();
  await expect(detail.getByRole("checkbox", { name: "启用 Claude Code" })).toBeEnabled();
  await detail.getByRole("button", { name: "返回技能库" }).click();
  await expect(page.locator(".skill-toast.success")).toContainText("ui-kit 已关闭");
  await expect.poll(() => existsSync(join(root, "home/.claude/skills/ui-kit"))).toBe(false);
  const calls = await page.evaluate(() => (window as unknown as { activationCalls: { name: string; args: Record<string, unknown> }[] }).activationCalls);
  expect(calls.filter(call => call.name === "quick_skill_activation")).toHaveLength(2);
  expect(calls.filter(call => ["scan_skill_sources", "list_skills"].includes(call.name))).toHaveLength(0);
  expect(calls.filter(call => call.name === "skill_matrix").every(call => call.args.skillId === "ui-kit")).toBe(true);
  await card.getByRole("button", { name: "ui-kit 更多操作" }).focus();
  await page.keyboard.press("ArrowDown");
  await expect(card.getByRole("menuitem", { name: "查看完整详情" })).toBeFocused();
  await page.keyboard.press("Escape");
  await expect(card.getByRole("menu")).toHaveCount(0);
  await page.getByRole("button", { name: "多选", exact: true }).click();
  await page.getByRole("checkbox", { name: "选择技能 ui-kit" }).check();
  await expect(page.getByRole("checkbox", { name: "全选当前结果" })).toHaveJSProperty("indeterminate", true);
  await page.getByRole("checkbox", { name: "全选当前结果" }).check();
  await expect(page.getByRole("checkbox", { name: "选择技能 code-review" })).toBeChecked();
  await page.getByRole("button", { name: "退出多选" }).click();
  await page.screenshot({ path: testInfo.outputPath("skills-light.png"), fullPage: true });
  await page.getByRole("button", { name: "切换深色主题" }).click();
  await page.screenshot({ path: testInfo.outputPath("skills-dark.png"), fullPage: true });
});

test("总开关一次提交三个 Agent，真实配置失败会局部回滚", async ({ page }) => {
  await page.getByRole("dialog").getByRole("button", { name: "暂不迁移" }).click();
  const card = page.locator(".skill-card").filter({ hasText: "ui-kit" });
  const toggle = card.getByRole("button", { name: "ui-kit 全部 Agent 启停" });
  await expect(toggle).toHaveAttribute("aria-pressed", "mixed");
  await toggle.click();
  await expect(toggle).toHaveAttribute("aria-pressed", "true");
  await expect(page.locator(".skill-toast.success")).toContainText("ui-kit 已启用");
  await expect(card.getByLabel("DSH · 已启用")).toBeVisible();
  await toggle.click();
  await expect(toggle).toHaveAttribute("aria-pressed", "false");
  await expect(page.locator(".skill-toast.success").filter({ hasText: "ui-kit 已关闭" })).toBeVisible();
  mkdirSync(join(root, "home/.codex"), { recursive: true });
  writeFileSync(join(root, "home/.codex/config.toml"), "[[skills.config]]\npath = 123\n");
  await card.getByRole("button", { name: "Codex · 已关闭" }).click();
  await expect(page.locator(".skill-toast.error")).toContainText("缺少字符串 path");
  await expect(card.getByRole("button", { name: "Codex · 已关闭" })).toBeVisible();
  await expect(card.getByRole("button", { name: "Claude Code · 已关闭" })).toBeVisible();
  const toast = page.locator(".skill-toast.error");
  await toast.getByRole("button", { name: /关闭通知/ }).click();
  await expect(toast).toHaveCount(0);
});
