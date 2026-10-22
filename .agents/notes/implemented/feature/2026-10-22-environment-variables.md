# Agent Note: Windows 环境变量编辑

Status: implemented

## Problem

工具箱缺少电脑环境变量编辑入口；开发工具的安装目录与 Path 仍需切换系统设置维护。读取当前进程环境无法区分持久化用户变量与系统变量，也会混入启动器注入的临时值。

## Decision

新增独立环境变量页和 Rust 模块。Windows 直接读取 HKCU\\Environment 与 HKLM\\SYSTEM\\CurrentControlSet\\Control\\Session Manager\\Environment，保留未展开值和 REG_SZ / REG_EXPAND_SZ 类型。复用锁文件中已有 windows-sys 的官方绑定，不引入运行时程序依赖。其他平台和浏览器明确提示不支持。

用户与系统作用域分开，支持搜索、新增、编辑、确认删除；现有变量名固定，避免重命名产生两次写入。Path 在原始值与每行一个条目之间可逆转换，不自动排序、去重、展开或修剪。系统键按当前进程访问权限判断，不自动提权。

列表突出变量名并将类型放在名称下方，给变量值保留主要宽度。Path 只在展示层按分号分行编号，默认显示前三项，其余可展开；空条目明确标记，展开字符串中的 `%变量名%` 引用高亮。普通长值支持查看完整内容，避免单元格内滚动。显示拆分与折叠不参与编辑状态或 IPC，不改变原始保存值。

每次写入校验变量名、UTF-16 长度、NUL 和类型；用读取时原始类型与字节的 SHA-256 核对当前值，拒绝过期覆盖与大小写重复新增。应用内写入串行，外部程序在核对与写入之间仍有小时间窗口。写后广播 WM_SETTINGCHANGE，通知失败报告为已保存但通知失败；既有进程需要重启才可能使用新环境。值只经本地 IPC 传递，不写应用配置、日志或笔记。

## 历史检索

按“环境变量 / environment variable / Windows”检索活跃笔记。工作台架构与本地工具笔记仅部分重叠：分别约定 Tauri / 浏览器边界、工具安装来源，均保留；当前没有同主题决定可替代。[工作台架构](../../implemented/architecture/2026-10-12-local-workbench.md)与[本地工具](../../implemented/feature/2026-10-16-local-tools.md)继续适用。

## Alternatives considered

- PowerShell / .NET Environment API 能避免 Win32 接口代码，但进程启动、字符串展开、空值与注册表类型保留不利于精确编辑；选择官方 Win32 绑定直接操作原值。
- 只打开系统环境变量窗口实现最简单且复用系统权限交互，但不能在工具箱内搜索、编辑和展示错误，不满足编辑入口的完整工作流。
- PowerToys 的 Profiles 可组织成套配置并恢复覆盖值，适合环境切换；当前仅需要单变量编辑，引入配置覆盖与激活状态会扩大行为范围。
- 原始值单行展示最紧凑，便于直接对照注册表，但长 Path 难以区分条目；采用分行预览与按需展开，以列表长度换取可读性，编辑时仍使用完整原始值。

## Verification

- `cargo test --manifest-path src-tauri/Cargo.toml --no-default-features --lib environment::`：4 项通过，使用 HKCU 下独立 UUID 测试键，覆盖真实注册表中的空值、中文、引用类型、长值、大小写重复、过期更新、删除及不支持类型识别。
- `npx playwright test tests/environment.spec.ts`：8 项通过；隔离 IPC 页面覆盖新增/编辑/确认删除、Path 可逆转换、系统/用户作用域分离、写权限、错误保留输入、读取失败保留上次数据、搜索、名称验证、主题、键盘和 1440 / 853 / 375 像素窗口。可读性回归覆盖 Path 编号、空条目、引用高亮、长值键盘展开、折叠阅读后原始值不变。浅深主题与编辑弹窗截图已目视检查。
- `npm run build`、桌面 command 的 `cargo check --manifest-path src-tauri/Cargo.toml --features desktop --bin my-toolbox` 通过。Vite 仍提示主包超过 500 kB，当前没有扩展到全局拆包。
- `npm run build:desktop` 与 `npm run verify-notes` 通过，可执行文件生成于 `src-tauri/target/debug/my-toolbox.exe`。
- 未修改真实用户或系统 Environment 键；真实管理员写入、广播到其他应用后的环境继承，以及安装包后的系统集成不属于隔离测试已证实范围。

## Consequences

编辑保留持久化原值、类型及 Path 顺序，单变量写入减少整表覆盖风险；代价是维护 Win32 接口与 UTF-16 解码，以及对外部写入做版本核对。本次仅支持 Windows，系统变量编辑需要现有进程有写权限。广播不能改变已经运行的进程环境；无自动提权或自动修改工具箱自身进程环境。读写核对不是跨进程事务。若需要跨平台编辑或成套配置切换，须重新定义 Shell 配置归属和覆盖恢复语义。

## References

- [PowerToys 环境变量开发文档](https://github.com/microsoft/PowerToys/blob/main/doc/devdocs/modules/environmentvariables.md)：直接注册表读取避免自动展开，将广播与写入分开。
- [PowerToys EnvironmentVariablesHelper](https://github.com/microsoft/PowerToys/blob/main/src/modules/EnvironmentVariables/EnvironmentVariablesUILib/Helpers/EnvironmentVariablesHelper.cs)：用户/系统键、未展开值与通知机制参考，独立实现 Rust 代码。
- [Microsoft Environment Variables](https://learn.microsoft.com/en-us/windows/win32/procthread/environment-variables) 与 [WM_SETTINGCHANGE](https://learn.microsoft.com/en-us/windows/win32/winmsg/wm-settingchange)：持久化位置、大小限制及广播语义。
