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
  existsSync,
  readFileSync,
  writeFileSync,
  readdirSync,
  rmSync,
} from "node:fs";

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
      "rename-fixture",
    ],
    { cwd: process.cwd(), stdio: "pipe" },
  );
});
test.beforeEach(async ({ page }) => {
  root = join(tmpdir(), `toolbox-rename-ui-${randomUUID()}`);
  queue = [];
  child = spawn(
    join(
      process.cwd(),
      `src-tauri/target/debug/rename-fixture${process.platform === "win32" ? ".exe" : ""}`,
    ),
    [root],
    { cwd: process.cwd() },
  );
  child.stderr.on("data", (data) => process.stderr.write(data));
  await new Promise<void>((resolve, reject) => {
    child.once("error", reject);
    createInterface({ input: child.stdout }).on("line", (line) => {
      const response = JSON.parse(line);
      if (response.ready) return resolve();
      const pending = queue.shift();
      if (response.error) pending?.reject(new Error(response.error));
      else pending?.resolve(response.value);
    });
    child.on("exit", (code) => {
      reject(new Error(`rename bridge exited ${code}`));
      for (const pending of queue)
        pending.reject(new Error(`bridge exited ${code}`));
      queue = [];
    });
  });
  for (const name of ["IMG_2.JPG", "IMG_10.JPG", "IMG_1.JPG"])
    writeFileSync(join(root, name), `content:${name}`);
  await page.exposeFunction("renameTestInvoke", invoke);
  await page.addInitScript(() =>
    Object.assign(window, {
      isTauri: true,
      __TAURI_INTERNALS__: {
        invoke: (name: string, args: Record<string, unknown>) =>
          (
            window as unknown as { renameTestInvoke: typeof invoke }
          ).renameTestInvoke(name, args ?? {}),
      },
    }),
  );
  await page.goto("/", { waitUntil: "domcontentloaded" });
  await page.getByRole("button", { name: "批量改名", exact: true }).click();
  await page.getByLabel("目标目录", { exact: true }).fill(root);
});
test.afterEach(async () => {
  if (child.exitCode === null) {
    const exited = new Promise<void>((resolve) =>
      child.once("exit", () => resolve()),
    );
    child.stdin.end();
    await exited;
  }
  // root 仅由上面的固定临时目录前缀与随机 UUID 构造。
  rmSync(root, { recursive: true, force: true });
});

test("模板自然编号、选择、真实改名与撤销；窄屏和主题布局", async ({ page }) => {
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));
  await page
    .getByRole("combobox", { name: "改名方式", exact: true })
    .selectOption("template");
  await page.getByLabel("命名模板", { exact: true }).fill("照片_{n}");
  await page.getByRole("button", { name: "生成预览", exact: true }).click();
  const rows = page.locator(".rename-table tbody tr");
  await expect(rows).toHaveCount(3);
  await expect(rows.nth(1)).toContainText("IMG_2.JPG");
  await expect(rows.nth(1)).toContainText("照片_002.JPG");
  expect(readdirSync(root)).toEqual(
    expect.arrayContaining(["IMG_1.JPG", "IMG_2.JPG", "IMG_10.JPG"]),
  );
  await page.getByLabel("选择 IMG_2.JPG", { exact: true }).uncheck();
  await expect(
    page.getByLabel("选择全部变化项", { exact: true }),
  ).toHaveJSProperty("indeterminate", true);
  for (const [width, height] of [
    [1440, 900],
    [853, 480],
    [375, 600],
  ]) {
    await page.setViewportSize({ width, height });
    expect(
      await page.evaluate(
        () => document.documentElement.scrollWidth <= innerWidth,
      ),
    ).toBe(true);
  }
  await page.screenshot({
    path: "test-results/rename-narrow.png",
    fullPage: true,
  });
  await page.setViewportSize({ width: 1440, height: 1000 });
  await page.screenshot({
    path: "test-results/rename-light.png",
    fullPage: true,
  });
  await page.getByRole("button", { name: "切换深色主题" }).click();
  await page.screenshot({
    path: "test-results/rename-dark.png",
    fullPage: true,
  });
  await page.getByRole("button", { name: "应用改名 (2)", exact: true }).click();
  const confirm = page.getByRole("dialog", { name: "确认应用改名" });
  await expect(confirm).toContainText("IMG_10.JPG → 照片_003.JPG");
  await confirm.getByRole("button", { name: "确认改名", exact: true }).click();
  await expect(page.locator(".rename-page").getByRole("status")).toContainText(
    "已完成 2 个文件改名",
  );
  expect(readFileSync(join(root, "照片_003.JPG"), "utf8")).toBe(
    "content:IMG_10.JPG",
  );
  expect(readFileSync(join(root, "IMG_2.JPG"), "utf8")).toBe(
    "content:IMG_2.JPG",
  );
  expect(existsSync(join(root, "照片_002.JPG"))).toBe(false);
  await page
    .getByRole("button", { name: "撤销最近一批 (2)", exact: true })
    .click();
  const undo = page.getByRole("dialog", { name: "确认撤销改名" });
  await expect(undo).toContainText("照片_003.JPG → IMG_10.JPG");
  await undo.getByRole("button", { name: "确认撤销", exact: true }).click();
  await expect(page.locator(".rename-page").getByRole("status")).toContainText(
    "已恢复 2 个文件的原名称",
  );
  expect(readdirSync(root).sort()).toEqual([
    "IMG_1.JPG",
    "IMG_10.JPG",
    "IMG_2.JPG",
  ]);
  expect(errors).toEqual([]);
});

test("重复目标随选择重算，修改规则会废弃旧预览", async ({ page }) => {
  await page.getByLabel("使用正则表达式", { exact: true }).check();
  await page.getByLabel("查找", { exact: true }).fill("^IMG_\\d+$");
  await page.getByLabel("替换为", { exact: true }).fill("相同");
  await page.getByRole("button", { name: "生成预览", exact: true }).click();
  await expect(page.locator(".rename-conflict")).toHaveCount(3);
  await expect(
    page.getByRole("button", { name: "应用改名 (3)", exact: true }),
  ).toBeDisabled();
  await page.getByLabel("选择 IMG_2.JPG", { exact: true }).uncheck();
  await page.getByLabel("选择 IMG_10.JPG", { exact: true }).uncheck();
  await expect(page.locator(".rename-conflict")).toHaveCount(0);
  await expect(
    page.getByRole("button", { name: "应用改名 (1)", exact: true }),
  ).toBeEnabled();
  await page.getByLabel("前缀", { exact: true }).fill("新_");
  await expect(page.locator(".rename-table")).toHaveCount(0);
  await expect(
    page.getByRole("button", { name: "应用改名", exact: true }),
  ).toBeDisabled();
  await page.getByLabel("查找", { exact: true }).fill("[");
  await page.getByRole("button", { name: "生成预览", exact: true }).click();
  await expect(page.getByRole("alert")).toContainText("查找规则无效");
  expect(readdirSync(root)).toHaveLength(3);
});

test("预览后文件变化拒绝执行，原名称被占用拒绝撤销", async ({ page }) => {
  await page.getByLabel("前缀", { exact: true }).fill("new_");
  await page.getByRole("button", { name: "生成预览", exact: true }).click();
  await page.getByRole("button", { name: "应用改名 (3)", exact: true }).click();
  writeFileSync(join(root, "IMG_2.JPG"), "externally changed");
  await page
    .getByRole("dialog")
    .getByRole("button", { name: "确认改名", exact: true })
    .click();
  await expect(page.getByRole("alert")).toContainText("文件已改变");
  expect(readdirSync(root)).toHaveLength(3);
  await page.getByRole("button", { name: "生成预览", exact: true }).click();
  await page.getByRole("button", { name: "应用改名 (3)", exact: true }).click();
  await page
    .getByRole("dialog")
    .getByRole("button", { name: "确认改名", exact: true })
    .click();
  await expect(page.locator(".rename-page").getByRole("status")).toContainText(
    "已完成 3 个文件改名",
  );
  writeFileSync(join(root, "IMG_2.JPG"), "new occupant");
  await page
    .getByRole("button", { name: "撤销最近一批 (3)", exact: true })
    .click();
  await expect(page.getByRole("alert")).toContainText("占用");
  expect(readFileSync(join(root, "IMG_2.JPG"), "utf8")).toBe("new occupant");
  expect(readFileSync(join(root, "new_IMG_2.JPG"), "utf8")).toBe(
    "externally changed",
  );
});

test("扩展名筛选、自然排序、分页和预览搜索", async ({ page }) => {
  for (let i = 1; i <= 101; i++)
    writeFileSync(join(root, `photo${i}.txt`), "file");
  writeFileSync(join(root, ".photo0.txt"), "hidden");
  await page
    .getByRole("combobox", { name: "改名方式", exact: true })
    .selectOption("template");
  await page.getByLabel("命名模板", { exact: true }).fill("文档_{n}");
  await page.getByLabel("起始编号", { exact: true }).fill("5");
  await page.getByLabel("编号步长", { exact: true }).fill("2");
  await page.getByLabel("补零位数", { exact: true }).fill("2");
  await page.getByText("筛选与排序", { exact: true }).click();
  await page.getByLabel("名称包含", { exact: true }).fill("PHOTO");
  await page.getByLabel("扩展名筛选", { exact: true }).fill(".txt");
  await page.getByRole("button", { name: "生成预览", exact: true }).click();
  const rows = page.locator(".rename-table tbody tr");
  await expect(rows).toHaveCount(100);
  await expect(rows.first()).toContainText("photo1.txt");
  await expect(rows.first()).toContainText("文档_05.txt");
  await page.getByRole("button", { name: "下一页", exact: true }).click();
  await expect(rows).toHaveCount(1);
  await expect(rows.first()).toContainText("photo101.txt");
  await expect(rows.first()).toContainText("文档_205.txt");
  await page.getByLabel("搜索预览", { exact: true }).fill("photo10.");
  await expect(rows).toHaveCount(1);
  await expect(rows.first()).toContainText("文档_23.txt");
  await expect(
    page.getByRole("button", { name: "应用改名 (101)", exact: true }),
  ).toBeEnabled();
  expect(existsSync(join(root, "photo1.txt"))).toBe(true);
});
