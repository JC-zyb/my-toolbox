# Agent Note: 本地开发工具发现与管理器聚合

Status: implemented

## Problem

快速入口只登记启动目标，无法表达工具的多份安装、真实来源、运行时管理关系及项目版本。Node、fnm、uv 等工具需要本机发现和远程资料补全，并通过既有管理器执行明确授权的操作。

## Decision

新增独立本地工具模块，工具定义、安装实例、安装来源与管理关系分离。Windows 对接 WinGet/Scoop，macOS/Linux 对接已有 Homebrew；fnm 管 Node、uv 管 Python、rustup 管 Rust，其余运行时使用 mise。已核实的 Cargo fnm 安装和 uv 独立安装使用原来源操作；未知来源不猜测、不自动接管。

静态目录保存官方身份和受支持映射，GitHub 仅补全资料，未知搜索候选需选择后登记。SQLite 独立保存扫描结果、登记、元数据缓存、项目与操作记录。后端保存预览，确认仅提交 ID；执行前核对来源和文件指纹，项目修改备份且保留无关配置。安装命令在受控目录运行，不执行项目 hook，不修改 PATH 或 shell 启动文件。管理器能力决定可用操作。

## Research

- [UniGetUI](https://github.com/Devolutions/UniGetUI)：参考管理器聚合、来源和操作记录。
- [mise registry](https://mise.jdx.dev/registry.html)：参考工具身份和后端分离，接入其版本能力。
- [nvm-desktop](https://github.com/1111mp/nvm-desktop)：参考 Tauri 中版本与项目交互。
- [proto](https://github.com/moonrepo/proto)：参考适配能力划分，不引入插件运行时。
- [fnm](https://github.com/Schniz/fnm/blob/master/docs/commands.md)、[uv](https://docs.astral.sh/uv/)、[rustup overrides](https://rust-lang.github.io/rustup/overrides.html)：遵循原 CLI 的作用范围。

已有[本地工作台架构](../../implemented/architecture/2026-10-12-local-workbench.md)部分重叠：保留原分层和真实桌面边界，本笔记补充工具管理，不替代用量与 Skills 决策。其余活跃笔记不涉及工具来源与版本管理，无需归档。

## Alternatives considered

- 工具箱自己下载与维护所有工具可以完全控制安装目录和更新，但会重写校验、解压、版本目录及 shell 接管，扩大维护与环境风险；采用原管理器。
- 全部统一到 mise 可以减少适配器，但会改变已有 fnm/uv/rustup 的管理归属；保持已有管理链，其余使用 mise。
- 仅管理资料最简单，但无法满足已确认的安装、多版本与项目固定需求；按适配器能力提供操作，未知来源仍只展示。

## Consequences

工具页面独立于用量、Skills 和脚本模块。后端 `local_tools` 拆分目录、进程、解析器、资料、项目配置、更新及操作任务，SQLite 保存资料缓存与任务记录。工具源和管理关系分离后，未知来源不会被猜测为 WinGet；uv 的卸载使用清单的解释器 key，避免同版本不同实现被混淆。Windows 读取注册表中的安装目录用于发现新工具，不把目录本身当成来源证据。

项目操作保存前后内容和相邻备份；15 分钟预览在提交及排队后均检查，执行前复核文件内容和管理器指纹。不同项目配置任务也串行，避免 Node/Rust/mise 同时修改配置的竞争。普通安装仍以管理器串行排队，退出码和重新探测共同判定成功。后台日志持续追加，应用重启后未结束任务标为中断。

CLI 格式会变化；解析器遇到无法识别输出显式报错。第三方安装操作无统一事务，失败重新扫描，不承诺回滚系统包。管理器默认值、工具箱继承的 PATH 和项目声明相互独立；其他管理器的项目实际解析结果未核实，Rust 显式查询 override。Scoop 全局安装不接管；多个入口无法唯一核实的 WinGet 操作阻止执行。uv 独立安装只支持官方自更新，不伪造卸载支持。

Windows 真实只读扫描确认 fnm 的 Cargo 来源、uv 独立收据、fnm/uv 管理的运行时、Java stderr、Maven/pnpm 批处理版本。真实 fnm 在全新临时 FNM_DIR 完成 Node 安装、默认值、项目固定和实际 node 版本核验，不改用户工具环境。自动测试覆盖 CLI 样例、非零退出、假成功、同管理器排队、过期预览、错误来源、默认值、配置保留/备份/冲突、离线缓存、重启恢复，以及 UI 的搜索添加、浅深主题和窗口尺寸。

三平台 CI 配置构建、核心测试、隔离 UI 和真实 fnm 闭环。当前本机仅验收 Windows；macOS/Linux 必须等待 CI 实际运行，WinGet/Scoop/Homebrew/uv/rustup/mise 的真实安装还需分别隔离验证。不能把接口实现和样例测试当作三平台安装验收完成。
