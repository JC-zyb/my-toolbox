# Agent Note: 本地工作台与只读用量采集

Status: implemented

## Problem

个人工具箱需要统一呈现三个客户端的会话、用量和本地操作。来源日志格式不同，直接累计会重复计算缓存、流式更新和继承历史；静态界面无法验证实际使用。

## Decision

采用 Tauri 2 / Rust / React / TypeScript。SQLite 保存统计缓存，JSON 保存偏好与登记工具。Rust 按来源解析、去重和持久化，React 统一筛选；浏览器模式明确为无桌面连接，不提供假成功或模拟使用数据。客户端原文件只读，不采集对话正文与凭据。当前仓库无既有笔记，此记录没有重叠决策。

Codex 使用累计快照与继承基线，调用数保持未知；Claude 使用消息身份去重；DSH 只使用 cost-meter version 1 账本，不叠加原会话。金额区分来源记录与未知。UI 默认七天、暖色主题，日期边界采用 Asia/Shanghai；其他时区需独立配置。解析版本纳入文件缓存戳，规则变化触发重建。暂停来源保留历史，并明确暂停状态。

每次采集先更新当前可读文件的缓存，再从该来源的全部持久化文件缓存构建快照。已采集文件移除、移出扫描目录或来源目录变更时，历史仍参与统计；Codex 活动与 archived_sessions 中相同会话桶沿用取完整记录而不叠加的规则。文件重解析成功仍整体替换该文件缓存，避免旧解析结果阻止用量修正；这不是每次快照的永久事件账本，DSH 原地裁剪账本中的旧桶不由此保留。

Codex 扫描同时接受 `.jsonl`、`.jsonl.zst` 与 `.jsonl.zstd`，用 Rust zstd 流式解压后复用相同解析与去重规则，单行上限按解压后的字节计算，不生成解压临时文件。压缩文件损坏显式进入来源诊断并保留上次成功缓存。2026-10-22 本机活动 / 归档目录分别发现 84 / 163 个 `.zst` 文件，原来的扩展名过滤将它们全部排除，归档目录已扫描并不等于归档用量完整。

改名采用服务端预览计划，拒绝覆盖和跨目录目标；执行失败尝试回滚并报告。组合规则、选择依赖、两阶段移动与会话撤销由[改名决策](../feature/2026-10-23-batch-rename.md)补充。脚本参数结构化传递，限时运行与限量输出。客户端启动只登记路径，不接管模型请求。

Skills 使用独立核心模块与 SQLite 元数据，沿用阻塞命令执行；中央库、Agent 分发、冲突与备份边界由[Skills 决策](../feature/2026-10-13-skills-library.md)补充，不混入用量采集器。

## Alternatives considered

- Electron 能复用大量网页模块，但增加常驻 Chromium 与 Node 系统能力边界，用户已选择 Tauri。
- JSON 单文件最容易备份，但历史记录与去重查询不适合整体读写，因此配置用 JSON、统计用 SQLite。
- API 代理可统一每次调用，但改变现有请求路径，当前采用只读日志。
- 只汇总本次发现的文件可直接反映磁盘现状，但部分文件移除后会漏掉已采集历史，因此快照使用来源的全部文件缓存；另建永久事件账本可保留文件内部裁剪的历史，但需要定义修正、解析版本迁移和清理语义，超出本次文件遗漏修复。
- 调用外部解压命令可以避免 Rust 新依赖，但本机无 zstd 命令，依赖用户安装会扩大运行环境要求；内置 zstd crate 让打包程序自行解压，代价是新增编译依赖。

## Consequences

桌面与前端均可构建。本地真实来源验证 Codex、Claude Code、DSH 可读取。Rust 测试覆盖继承基线、重复用量、流式去重、缓存包含关系、未写完尾行、日期、重复扫描、解析失败保留旧缓存以及改名覆盖。浏览器测试覆盖常见尺寸、缩放等效窗口、主题、键盘、真实统计快照展示与失败保留数据；真实快照测试选择全部历史后再断言非零用量，因为默认七天范围可能没有记录。测试桥仅用于隔离 UI 验证，生产无桥接或虚拟数据。

客户端格式可能变化；缺失必需字段会报错，缺少价格显示未知。按文件修改信息缓存整文件解析结果，首次扫描可能耗时，尚未实现追加日志的字节偏移索引。成本不是账单。脚本超时终止直接进程，派生进程仍需脚本管理。Windows 实际 DPI、多屏、托盘菜单和安装包仍需人工验收；客户端版本目前显示未读取。

文件缓存随历史增长，快照读取同样增长；尚未采集且已被清除的日志无法恢复。采集回归覆盖归档迁移与副本去重、部分日志移除后重启、三个来源变更目录后的历史保留，以及当前文件更新不重复累计。界面所选日期和来源仍控制显示范围。

2026-10-22 在隔离临时目录使用 inspect-usage 读取本机真实来源：Codex 262 个文件、374 条会话日期模型桶、209 个会话、2,399,483,617 Token，Claude Code 14 个文件和 DSH 1 个账本均 ready，无来源诊断；其中 Codex 压缩文件贡献 2,326,157,841 Token。原文件和实际应用数据库均未修改。78 项 Rust 库测试通过，压缩回归另覆盖拼接多帧、两种扩展名、归档副本去重和损坏后保留缓存。

## References

2026-10-22 对照用户指定的 TokenTracker，一手源码固定于 `d3946d4a70efa7d388a34409649adde8f2bca020`，以下为已核实行为，不将 README 的工具支持数量当成所有格式均完整覆盖的证据。

压缩 Codex rollout 的参考实现另见 `readCodexRolloutSnapshot`：识别 `.jsonl.zst` / `.jsonl.zstd`，支持多帧 zstd，并将压缩文件当作完整快照解析，不将压缩字节游标混作解压后文本偏移；解码失败保留原游标等待重试。[压缩识别、限量解码与失败保留](https://github.com/xiufengsun/TokenTracker/blob/d3946d4a70efa7d388a34409649adde8f2bca020/src/lib/rollout.js#L191-L255)。

- **归档与去重：**`sync.js` 为原生、WSL 和额外 Codex 根目录登记 `sessions` 与 `archived_sessions`；后台轻量同步通常跳过归档，完整同步使用递归扫描处理平铺归档。`codexSessionIdFromPath` 从 rollout 文件名提取 UUID，`parseRolloutFile` 以会话 UUID 加事件时间戳去重，避免文件改写和归档迁移重复入账；没有 UUID 时回退路径。[归档扫描入口](https://github.com/xiufengsun/TokenTracker/blob/d3946d4a70efa7d388a34409649adde8f2bca020/src/commands/sync.js#L822-L843)、[递归归档发现](https://github.com/xiufengsun/TokenTracker/blob/d3946d4a70efa7d388a34409649adde8f2bca020/src/lib/rollout.js#L267-L273)、[会话键](https://github.com/xiufengsun/TokenTracker/blob/d3946d4a70efa7d388a34409649adde8f2bca020/src/lib/rollout.js#L2347-L2360)、[事件去重与累计](https://github.com/xiufengsun/TokenTracker/blob/d3946d4a70efa7d388a34409649adde8f2bca020/src/lib/rollout.js#L2880-L2909)。
- **历史持久化：**`parseRolloutIncremental` 持久保留已见事件键与累计桶，`enqueueTouchedBuckets` 将桶快照追加到本地 `queue.jsonl`，本地 API 按来源、模型、时间桶读取队列的最后一版数据，而非每次由现存日志重建总量。因此已入账用量不依赖原日志一直存在；源码的重建修复也在缺少曾贡献日志时跳过，防止丢失不可重建历史。这个队列架构与本次复用 SQLite 文件缓存的修复不同。[持久去重状态](https://github.com/xiufengsun/TokenTracker/blob/d3946d4a70efa7d388a34409649adde8f2bca020/src/lib/rollout.js#L364-L452)、[追加队列](https://github.com/xiufengsun/TokenTracker/blob/d3946d4a70efa7d388a34409649adde8f2bca020/src/lib/rollout.js#L3829-L3833)、[API 桶键](https://github.com/xiufengsun/TokenTracker/blob/d3946d4a70efa7d388a34409649adde8f2bca020/src/lib/local-api.js#L204-L213)、[最后快照覆盖](https://github.com/xiufengsun/TokenTracker/blob/d3946d4a70efa7d388a34409649adde8f2bca020/src/lib/async-queue-reader.js#L102-L108)、[缺失日志重建保护](https://github.com/xiufengsun/TokenTracker/blob/d3946d4a70efa7d388a34409649adde8f2bca020/src/commands/sync.js#L277-L299)。
- **子 Agent / fork 边界：**`consumeUsageDelta` 为父 Agent 与嵌入 reviewer 交错出现的累计计数维护最多 32 个流基线，避免不同流相减产生虚增；fork 历史另用跨日判断和开头密集时间戳启发式跳过，源码明确保留第一条而可能少量多算，不应视作严格去重证明。会话解析只接受第一条 `session_meta` 的 lineage，避免复制的祖先元数据覆盖子会话。建议后续改进先用真实样本验证这些边界，本次不直接移植启发式。[计数流基线](https://github.com/xiufengsun/TokenTracker/blob/d3946d4a70efa7d388a34409649adde8f2bca020/src/lib/codex-token-usage.js#L3-L18)、[增量计算](https://github.com/xiufengsun/TokenTracker/blob/d3946d4a70efa7d388a34409649adde8f2bca020/src/lib/codex-token-usage.js#L98-L157)、[fork 启发式与限制](https://github.com/xiufengsun/TokenTracker/blob/d3946d4a70efa7d388a34409649adde8f2bca020/src/lib/rollout.js#L2828-L2875)、[首条 lineage](https://github.com/xiufengsun/TokenTracker/blob/d3946d4a70efa7d388a34409649adde8f2bca020/src/lib/codex-rollout-parser.js#L1070-L1082)。
- **DSH 范围：**TokenTracker 的 `resolveDshSessionFiles` 扫描 `sessions/<project>/<session>/` 下普通、版本化及 zstd 压缩 JSONL，`extractDshSessionUsage` 使用 seq 水位，`parseDshIncremental` 维护文件及会话贡献状态再累计到公共桶。它与本工具箱只读 `storages/cost-meter/ledger.json` version 1 的入口不同；本次保留的是已采集文件缓存，不包含 DSH 原地裁剪账本旧桶的永久保留，也未验证所有 DSH 版本。[日志匹配与发现](https://github.com/xiufengsun/TokenTracker/blob/d3946d4a70efa7d388a34409649adde8f2bca020/src/lib/rollout.js#L23918-L23980)、[seq 用量提取](https://github.com/xiufengsun/TokenTracker/blob/d3946d4a70efa7d388a34409649adde8f2bca020/src/lib/rollout.js#L24649-L24728)、[贡献状态与缺失文件保留](https://github.com/xiufengsun/TokenTracker/blob/d3946d4a70efa7d388a34409649adde8f2bca020/src/lib/rollout.js#L24996-L25078)。
