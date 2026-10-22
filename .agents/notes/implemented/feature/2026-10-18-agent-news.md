# Agent Note: Agent 资讯专栏

Status: implemented

后续扩展：[全生态覆盖与中文阅读](2026-10-18-agent-news-chinese-coverage.md)部分取代本篇的固定九源、不翻译及直接外跳边界，新增领域与自定义订阅、中文翻译和工具内预览；本篇保留最初采集和缓存决策的历史依据。

## Problem

工具箱缺少集中跟踪 Agent 技术、模型发布和功能变化的入口，用户需要自动获取新内容并能核对原始出处。

## Decision

在既有 Tauri / Rust / React 架构内新增独立资讯模块。Rust 使用 reqwest 获取固定白名单的官方 RSS/Atom 与 GitHub Releases Atom，通过 feed-rs 解析标准订阅，SQLite 保存条目、来源状态、HTTP 验证器和自动更新偏好。feed-rs 避免自行维护 XML、命名空间和日期解析，不引入独立服务、账号或模型调用。

应用窗口运行期间每分钟检查是否到期，来源每 30 分钟更新；自动更新可关闭，手动刷新立即请求。条件请求使用 ETag / Last-Modified；每源超时 20 秒、解压后响应上限 4 MiB、最多并发 4 源，每源最多保留 100 条。失败保留该源成功缓存，展示失败原因与上次成功时间，不改写发布时间。缺少发布日期时使用发布者的更新时间，均缺失则标为未知并排在末尾。按规范化原文 URL 去重，分类基于来源和标题关键词，不声称是语义分类或 AI 摘要。页面提供分类、来源、时间范围、搜索、原文和来源状态。

DeepMind 在不声明接受 gzip 时仍返回 gzip。reqwest 启用标准 gzip 解压，之后再执行响应上限检查和 feed-rs 解析；原始 HTML 摘要在前端通过 DOMParser 转为纯文本，只作为 React 文本渲染，不注入 HTML。SQLite 在同一事务保存条目和 HTTP 验证器，避免成功缓存与 304 条件不一致。后台采集不依赖当前页面；UI 操作串行，避免刷新覆盖订阅偏好。

历史笔记审计：工作台架构笔记与本地工具笔记部分相关，沿用其真实桌面桥、SQLite、失败保留缓存原则，不修改其既定模块边界；Skills 系列和独立桌面构建提案与资讯业务无关。[工作台架构](../../implemented/architecture/2026-10-12-local-workbench.md)、[本地工具](../../implemented/feature/2026-10-16-local-tools.md)。

参考 [TrendRadar RSSFetcher](https://github.com/sansan0/TrendRadar/blob/master/trendradar/crawler/rss/fetcher.py) 的来源隔离与有界请求，以及 [Horizon](https://github.com/Thysrael/Horizon) 的多来源聚合与去重思路；不复制项目代码或引入其服务。

## Alternatives considered

- 部署 Horizon / TrendRadar：已有成熟采集、推送和 AI 简报管线，功能完整；但个人桌面专栏不需要独立 Python 服务、定时部署和 LLM 凭据，运维成本超出本次需求。
- 前端直接请求或公共 RSSHub 实例：实现轻、可覆盖无订阅站点；但浏览器 CORS 和公共实例可用性会影响读取，当前 Rust 已能直接联网，因此优先使用发布者自身订阅。
- GitHub REST Releases API：能准确区分预发布与草稿，结构化信息完整；但未认证限流与跨仓库轮询预算不利于持续获取，固定官方项目使用原生 Atom，并将条目视为发布记录而非保证稳定版本。

## Verification

- 侧栏和命令面板进入专栏，Playwright 验证搜索、分类、来源、日期筛选、原文打开、订阅暂停、失败保留和浏览器桌面能力提示；浅深主题及 1280×720、1440×900、1920×1080、853×480、375×812 布局均检查无横向溢出。
- Rust 测试覆盖 RSS/Atom 日期、链接安全、去重、304 请求验证器、503、异常 XML、响应大小、gzip、离线缓存、重启持久化、暂停自动更新、未到期和停用来源不联网。
- `npm test`、`npm run test:rust`、`npx playwright test`、`npm run build:desktop`、`npm run verify-notes` 验证构建与回归。新增界面测试用受控 IPC 响应验证 UI；真实网络另由 `inspect-agent-news` 调用生产 NewsService 写入新建临时目录验证，不改实际应用缓存。
- 2026-10-18 网络恢复后，九个真实来源均成功，缓存共 360 条；真实缓存页面默认最近 30 天显示 143 条，实际条数随上游更新变化。失败的初始探测定位为网络和未启用 gzip，最终验证均已通过。独立桌面程序输出到 `src-tauri/target/debug/my-toolbox.exe`。

## Consequences

获得无需账号、密钥和独立部署的自动资讯入口，沿用现有桌面网络桥与 SQLite。代价是固定来源白名单和关键词分类需要随上游变化维护，来源可能封锁自动请求或更改订阅；失败必须可见。订阅覆盖不等于全网，原文可能为英文，GitHub 含预发布版本；本次不引入付费翻译或摘要。每个来源最多保存 100 条，不承诺完整历史；若需要全文检索、更多来源或长期归档，再考虑独立条目表与来源管理。应用退出后不调度，系统休眠恢复后到期检查继续运行。Windows 实际 DPI、多屏及安装包不在本次验证范围。
