# MeowLauncher Beta

面向不同模拟端适配的 Windows 通用游戏启动器，使用 Rust + Tauri 2 + WebView2 构建。
界面尺寸统一以 1920×1080 为设计基准：1280×720 画布占屏幕约三分之二；4K 屏幕整体放大为 2560×1440。窗口受工作区留边约束，WebView 按屏幕比例与系统 DPI 换算缩放；登录、大厅及所有弹窗共用此画布，不分别改变缩放。原生圆角同步按窗口比例调整。


本仓库是 **MeowLauncher Beta（sponsor 通用版）** 的源码，默认构建目标即普通版。
同一份源码在编译期通过 `MEOW_LAUNCHER_EDITION` 区分两种产品，编译期固定、运行时不可切换：

| 产品 | 编译开关 | 说明 |
| --- | --- | --- |
| **MeowLauncher Beta（通用版，本仓库）** | `-Edition sponsor`（本仓库默认） | 不内置任何服务端点；由服主填写自己的服务器连接配置后才能连接 |
| MeowLauncher Max（自定义版） | `-Edition meowopia-custom` | 面向特定服务器的定制构建；定制文档、端点配置与发布物由维护者私有仓库维护，不在本仓库分发 |

> **使用声明**：本普通版仅供免费技术交流与个人学习研究使用，严禁倒卖、收费传播或任何形式的商业牟利。
> 游戏内容及相关知识产权归原权利人所有。完整声明见 [NOTICE.txt](NOTICE.txt)。

## 功能

- 账号：用户名密码登录（通用版，需服主配置服务端）；8 位 UID 登录（定制版）
- 大厅、托盘常驻、启动游戏；登录票据一次性使用
- 自动更新：同源校验、SHA256 与 Windows 产品名校验，失败时不动现有程序
- 客户端安装/更新引擎：安全解压、ETag / HTTP Range 续传、事务替换与回滚、备份（`.oo-client-update`）
- 双主题界面：`modern`（现代版）与 `retro`（复古版），可在设置中切换
- 客户端宿主与隔离接口保留；多开开发与验收当前暂停
- GM 管理工具（MeowGM 管理器 / 管理后台 / 网关服务）为独立项目：[zkonikishi/MeowGM](https://github.com/zkonikishi/MeowGM)

本仓库**不包含**游戏客户端、服务端程序或 `Script.pvf` 等游戏资源，它们独立分发。

## 目录结构

```
apps/
  launcher/        启动器主程序（Tauri 2 / Rust + UI 静态页）
    src-tauri/     原生壳：登录、启动、更新、托盘、多实例、客户端宿主
    ui/            前端页面与素材（无构建步骤的 ES Module 页面）
packages/
  launcher-core/   启动器核心库（账号、更新、启动、适配层、CN115 桥）
adapters/examples/ 可选适配器示例，不编译进内核
tools/             构建与模型测试脚本（build.ps1、model.test.mjs 等）
docs/              普通版使用说明与主题素材文档
```

## 环境要求

- Windows 10/11 x64
- [Rust](https://rustup.rs) stable（MSVC toolchain）
- Node.js ≥ 20（仅用于界面模型测试 `tools/model.test.mjs`）
- Microsoft Edge WebView2 Runtime（运行期依赖，Win11 已内置）

## 构建

```powershell
# 普通版（通用版，本仓库默认）
pwsh -File tools\build.ps1 -Edition sponsor -ReleaseName 0.7.1-generic

# 跳过测试
pwsh -File tools\build.ps1 -SkipTests -Edition sponsor -ReleaseName 0.7.1-generic
```

产物输出到 `releases\<ReleaseName>\`：`MeowLauncher.exe` 与 `使用说明.txt`。

`tools/build.ps1` 参数：

| 参数 | 说明 |
| --- | --- |
| `-Edition` | `sponsor`（普通版，本仓库默认）或 `meowopia-custom`（定制版） |
| `-LegacyOnline` | 启用 `legacy-online` 特性（仅定制版使用，普通版不可用） |
| `-ReleaseName` | 输出目录名，默认 `0.7.0-review` |
| `-SkipTests` | 跳过核心库与界面模型测试 |

离线构建前请确认 `cargo` 已在 PATH 中（Rust 工具链安装后重开终端）。

如需自行分发，请勿在普通版中加入他人服务器的账号服务或更新源；普通版必须由分发者提供
自己的 HTTPS 服务配置（`launcher.public.json`），程序不会静默回退到任何内置服务。

## 测试

```powershell
cargo test -p meow-launcher-core --locked
node --test tools/model.test.mjs
```

## 内嵌二进制说明

`apps/launcher/src-tauri/runtime/clienthost-strict-r1.exe`（客户端宿主）在编译期通过 `include_bytes!` 嵌入主程序
（自释放 sidecar），属发行产物的一部分。其源码不在本仓库中；如需完整构建链请自行替换为等价实现或联系维护者。

## 脱敏说明

本开源仓库为**脱敏版本**：不含任何真实服务器地址、IP、域名、证书或私钥，所有与线上服务相关的端点
均为不可路由的占位符（`launcher.invalid`、`203.0.113.10`）。定制版（MeowLauncher Max）的真实端点等
构建输入由维护者私有仓库维护，不在本仓库分发。部分开发辅助脚本（`tools/` 下）使用 `C:/workspace`、
`upstream/` 等占位路径，请按本机环境调整。

## 版本

当前源码版本 0.7.1。历史发行见 Releases。

## 许可

本仓库未附开源许可证，默认保留所有权利（All rights reserved）。
在遵守 [NOTICE.txt](NOTICE.txt) 使用声明的前提下，可自由下载、编译、用于个人学习研究。


## Tauri 适配器接入（2026-10-18）

作者：zkonikishi。S4A21 不再作为核心的默认客户端、指纹、下载源或启动参数。原配置保存在 `adapters/examples/s4a21/profile.json`，仅供手动选用，不能据此认定该端已通过联机验收。

旧的 `launcher.public.json` 发行配置需要增加 `clientAdapter` 对象，并使 `adapterId` 与其 `id` 一致。字段包括 `id`、`versionId`、`executable`、`machine`（x86/x64）、`executableSha256`、`arguments`（字符串数组）、`requiredFiles` 和 `requiredDirectories`。示例结构见 `packages/launcher-core/fixtures/public-config.json`，其中零指纹是测试占位，实际发行必须填写目标主程序 SHA256。没有适配器时明确提示未配置，不回退到 S4A21。

参数支持 `{host}`、`{port}`、`{uid}`、`{token}`，每个数组元素作为独立进程参数直接传入，不执行 shell。可用一个元素兼容旧式问号分隔参数，也可使用多个命令行参数。资源要求由适配器声明；文件名限客户端根目录内，主程序架构与 SHA256 仍须匹配。适配器配置是服主选择并随发行包提供的受信配置，不从远程清单自动替换。

通用版当前使用 `generic-target.json` 的认证桥与客户端宿主接口；上述发行适配器改造不等于所有认证协议或宿主 ABI 已兼容。需要新认证协议、原生桥或特殊启动方式的模拟端，仍需实现对应适配并逐端验收。目标是开放接入各种版本，不宣称已支持全部版本；本次不恢复多开、不改 Max、不发布游戏资源。

## 归档候选状态（2026-10-21）

作者：zkonikishi。Tauri 是 Beta 的主要开发实现，Electron 独立保存在 Electron 分支。此次为源码归档候选，不是稳定可玩发行版；内部历史版本号尚未完成新发布元数据整理。

已实现 MEOWSIMPLE/1 用户名密码注册、登录和同流认证；账号配置支持 local-gateway 与可配置 HTTPS cloud-account provider。云端客户端已实现邮箱/手机挑战、注册、找回、登录以及系统浏览器第三方登录流程，秘密留在原生进程；必须由服主另行部署兼容账号桥和可信原生 grant 签发后端。本仓库没有附带可用云服务，也没有任何真实邮箱、短信、第三方平台配置或实服验收。

AuthConfig 包含 schemaVersion=1、activeProvider、providers；provider 包含 id、label、connection。cloud-account.connection 包含 kind、baseUrl（HTTPS origin）、gatewayProviderId（引用 local-gateway）、methods（username-password/email-code/phone-code/third-party）。账号桥协议为 meowlauncher-account-v1，路径 /meowlauncher/v1/ 下 challenge、login、register、reset、browser/begin、browser/poll、native-grant、logout；DTO 和范围检查以 packages/launcher-core/src/cloud_account.rs 为准。第三方登录需服主的同源授权页，不能直接填写第三方 SDK 密钥到启动器。

已有原生 v1 宿主链保留，v2 admission 接口仍为离线候选。账号登录不代表进入游戏成功，当前新增云端与布局未经过真实服务及 DFO 实机验证。核心测试126通过、3忽略；云端UI边界模拟10通过，不能作为实际第三方服务可用证明。

Beta 使用 data/beta 保存公开配置，data 不随源码分发。待完成：详细运营部署教程、真实云服务联调、极端分辨率 WebView 缩放上限补偿、原生界面验收、统一新版本元数据和正式打包。屏幕比例算法已经编译，尚不宣称全分辨率实机验收。
