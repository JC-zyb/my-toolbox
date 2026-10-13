# Agent Note: 独立桌面构建不依赖开发服务器

Status: proposed

## Problem

交付的普通 cargo build 程序未启用 Tauri custom-protocol，仍加载 devUrl，用户直接启动时因 Vite 未运行出现 127.0.0.1 无法访问。

## Proposal

声明 custom-protocol Cargo feature，使用 Tauri CLI build --debug --no-bundle 构建内嵌 frontendDist 的可执行文件，提供 build:desktop 脚本并更正启动说明。开发继续使用 tauri dev。

## Alternatives considered

- 启动 Vite 可立即让开发程序工作，适合开发，但用户要求直接打开程序，因此独立交付不能依赖该服务。
- 删除 devUrl 可让普通 Cargo 构建嵌入文件，但破坏开发热更新约定；保留开发配置，通过官方构建流程区分用途。

## Acceptance criteria

独立程序构建成功，在 1420 无监听时启动，验证窗口加载内嵌页面与 Skills 导航；笔记门禁通过。

## Risks

调试构建仍不是安装包；后续普通 cargo build 会覆盖同路径开发程序，使用说明须明确交付构建入口。
