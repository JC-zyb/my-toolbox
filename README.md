# 个人工具箱

Rust / Tauri 2 / React / TypeScript 的本地个人工作台。默认七天的 Agents 用量总览，配套浅色和深色主题。

## 启动

需要 Node.js 20.19+、Rust 和 Windows Tauri 构建环境（MSVC Build Tools 与 WebView2）。

```text
npm ci
npm run desktop
```

`npm run dev` 只打开浏览器 UI，明确显示桌面能力不可用，不模拟本机用量或系统操作。`npm run desktop` 启动开发服务器和开发窗口。

可直接启动的桌面程序使用 `npm run build:desktop` 构建，前端文件嵌入可执行文件，输出为 `src-tauri/target/debug/my-toolbox.exe`，无需运行 Vite。普通 `cargo build` 生成的开发程序仍依赖 `127.0.0.1:1420`，不能作为独立程序交付，也会覆盖同路径文件。安装包可使用 `npm run tauri -- build` 构建。

## 已实现

- 用量总览：时间/来源筛选、输入输出与缓存、来源成本、趋势表格与 CSV、Agent 排名、模型分布、最近会话。
- Agents / 数据源：目录登记、独立暂停、来源状态与解析诊断、启动客户端、打开目录。
- 会话：搜索、模型筛选、分页、CSV、元数据与用量详情。
- Skills：中央库卡片浏览、完整详情与 Agent 即时启停、批量操作、Agent 独有技能导入、新建/编辑、本地与 GitHub 导入、全局/项目分发预览、差异与冲突处理、迁移备份、更新提示与导出。
- 工具：快速入口登记、批量改名预览/冲突检查/确认执行、脚本登记/确认执行/超时/输出。
- 系统：本地持久化设置、自动采集、命令面板、键盘导航、托盘和单实例。

## 数据边界

原始客户端文件只读。Codex 读取 `.codex/sessions` 与 `archived_sessions` 的 JSONL；Claude Code 读取 `.claude/projects`；DSH 读取 `.dsh/storages/cost-meter/ledger.json` 的 version 1 账本。仅保存会话元数据与用量，不保存对话、工具输出或凭据。工具箱缓存位于 Tauri 应用本地数据目录（可在设置中打开），SQLite 缓存按文件修改信息更新，失败保留上次成功数据并暴露诊断。

输入包括缓存读取/创建，推理若属于输出不再次累加。金额目前只汇总来源成本（USD）；未提供价格的模型显示未知，不推算订阅账单。Codex 无法从用量事件确定请求数，调用显示未知；Claude 按消息身份去重，DSH 使用来源调用数。日界限采用 Asia/Shanghai，DSH 保留账本日期。客户端版本未自动探测，显示未读取。

批量改名只处理指定目录下的文件、不递归且不覆盖已存在目标；失败尝试回滚并报告。脚本输出最多保留标准输出和错误各 1 MiB；超时终止直接进程，脚本自行启动的子进程仍需脚本管理。脚本参数通过结构化参数传递。

## Skills 使用

在桌面版左侧打开 Skills。中央库固定为 `~/.agents/skills`；全局 Codex 直接发现中央库，`.codex/skills` 只作为兼容扫描和清理入口。Claude Code 与 DSH 分别使用 `.claude/skills` 和 `.dsh/skills` 的单技能入口。项目需要手动登记，只扫描和分发到所选工作区；项目 Codex 使用项目 `.agents/skills` 安装入口。

技能库默认显示卡片：名称、描述、来源及右下角 Claude Code、Codex、DSH 状态图标。彩色表示已启用，灰淡表示已关闭，读取失败显示未知状态；同名不同内容另标记版本冲突。点击卡片进入主内容区完整详情，可查看渲染后的 SKILL.md 正文、源码、来源和真实差异；返回保留搜索、筛选与滚动位置。勾选 Agent 立即启停，冲突、外部目录和首次导入会先显示确认。Agent 独有技能可在详情中确认来源后导入中央库并启用其他 Agent。

全局 Codex 通过 `~/.codex/config.toml` 的 `[[skills.config]]` 对实际 `SKILL.md` 路径设置 `enabled`，默认无配置视为启用；修改后重启 Codex 生效。项目 Codex 使用项目安装入口，关闭项目入口不影响全局技能。Claude Code、DSH 的关闭会备份并移走对应安装入口，中央库内容保持。多选模式一次预览并应用所选技能和 Agent 的启停；兼容入口清理放在高级操作中，与关闭技能分开。

内部统一矩阵继续合并中央库、各 Agent 独有技能和旧 `.skills-manager/skills` 中的同名记录，供卡片、筛选和预览共用状态。可筛选重复入口、冲突、Agent 独有及未安装；路径、模式和指纹在来源详情及操作预览中查看。

批量分发使用自动链接：Windows 先 junction 再 symlink，失败原因直接显示；高级设置明确允许后才可复制降级。原单技能详情的同步模式继续作为高级选项。批量冲突默认保留已有版本并跳过，可逐项改为中央版本或保留目标并导入中央。同一技能不能同时选择不同中央版本。确认后再次核对内容指纹和父目录边界，过期预览不能执行，任一运行时错误逆序恢复文件并回滚 SQLite。

高级兼容入口清理作用于所选 Agent 的安装，包括已发现的外部目录和联接，先预览完整路径和修改状态，原目标移入相邻备份，中央内容及联接源保留；本地已修改的副本默认跳过。清理全局 Codex 只移除 `.codex/skills` 的兼容入口，全局 Codex 的启停须使用详情勾选或批量启停。删除中央内容仍需在详情中单独确认名称、指纹及备份。原单技能解除分发接口继续仅处理工具管理且未被外部改变的目标。

首次访问发现 `.skills-manager/skills` 内容时显示只读迁移向导，冲突默认保留中央版本，来源独有项默认导入。确认后备份内容、校验复制、重建 Claude/DSH 旧库入口，并清理 Codex 兼容入口；迁移源保留，重复执行不覆盖已相同的内容。若整个 Agent `skills` 目录联接到旧库，迁移备份目录联接后改为普通目录和单技能入口；此时必须处理全部迁移项，避免部分迁移影响未处理内容。普通批量操作会阻止穿透整目录联接，提示先迁移。

备份位置显示在公共仓库设置与最近操作中。同一磁盘的替换使用相邻 `.toolbox-backup-*` 目录保留原版本，应用数据目录保存迁移备份及 `skills.sqlite` 元数据。运行时错误尝试回滚；进程突然退出仍可能需要人工从备份恢复。

GitHub 导入支持 HTTPS 仓库 URL 与 branch/tag/commit，浏览有效 Skill 后选择导入。Git 必须可用；网络、认证及格式错误直接显示。仓库 checkout 与来源 commit 保存在应用数据目录，更新检查只提示，不覆盖中央库。自动检查仅在 Skills 页面打开期间每小时运行。默认拒绝同名不同内容的导入与 Skill 内部链接；不会读取链接指向的库外文件。Skills 命令面板支持技能搜索、导入仓库、项目工作区与当前技能分发预览。

## 验证

```text
npm run build
npm test
npm run test:rust
npx playwright test
npm run verify-notes
cargo check --manifest-path src-tauri/Cargo.toml
```

界面验收使用已安装的 Microsoft Edge。覆盖 1280×720、1440×900、1920×1080 和缩放等效尺寸；Windows 实际 DPI、多屏及安装包仍需人工验收。截图保存在 `test-results/`，不会提交。

Skills UI 测试使用 `skills-fixture` 进程和新建系统临时目录，直接调用真实 Rust 核心，断言迁移、分发和创建后的文件结果。该桥仅供测试，不接入生产应用，也不操作真实用户库。
