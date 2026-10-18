# MeowLauncher Beta

通用版 Windows 游戏启动器。作者：**zkonikishi**。

官网与项目主页：[GitHub 项目](https://github.com/zkonikishi/MeowLauncher)。自定义版 **MeowLauncher Max** 的官网为 https://mrad.meowopia.com。

## 分支

- `main`：Electron / React 主线。现有源码包含早期26.1.0历史基线，不代表最新本地候选已同步。
- `Tauri`：保留此前已脱敏公开的通用版 Rust / Tauri 2 源码，不能当作Max自定义版。

## 开发版本

下一共同开发目标：**MeowLauncher Beta · 26.3-snapshot-1**，尚未发布。

命名顺序：`26.3-snapshot-1` → `26.3-pre-1` → `26.3-rc-1` → `26.3`；各阶段编号递增，正式补丁如 `26.3.1`。Beta是产品名，不是发布阶段。旧版本记录原样保留。

## 构建与范围

Electron主线使用Node.js 20+，渲染层在`ui`目录安装锁定依赖并构建；Tauri分支使用Rust/Tauri/WebView2，请以该分支Cargo工作区及构建脚本为准。运行环境版本与产品版本分别记录。

通用版由服主配置服务器与账号服务。尚未完成的新框架共享、真实游戏下载/启动及自动更新应用，不因源码上传而视为验收通过。多开开发保持暂停。

本仓库不包含游戏客户端、服务端程序、账号数据或私钥。作者说明与使用范围见 NOTICE.txt；腾讯相关游戏内容及知识产权归原权利人，所有解释权归腾讯，此表述不表示腾讯授权。
