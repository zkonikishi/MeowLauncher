# MeowLauncher 26.X

MeowLauncher 26.X 是面向 **115 版本客户端** 的启动器主线：Electron 主进程 + React/Vite 渲染层。界面结构参考官方启动器解包结果，应用层完全自研；不加载、不执行任何官方客户端代码。

当前版本：**26.1.0 Beta**（line `26.1`，snapshot `26w42a`，codename `Dolphin`，Electron `31.7.5`）。

> 状态：Beta 主线首版，版本管理 / 启动 / 设置 / 日志骨架已就绪；账号、更新、服务端卡片仍在迭代。本仓库不包含游戏客户端、服务端程序或游戏资源。

## 结构

| 路径 | 内容 |
| --- | --- |
| `src/main/` | Electron 主进程：`index.mjs` + `modules/{config,game,ipc,logger,updater,version,window}.mjs` |
| `src/preload/` | 预加载脚本（contextBridge，`preload.cjs`） |
| `src/renderer/` | 备用渲染层（`ui/` 未构建时回退） |
| `ui/` | React + Vite 渲染层（开发用 `npm run dev`，构建输出 `dist/ui`） |
| `version.json` | 版本清单（line / version / channel / snapshot / codename / electron） |
| `tools/extract-asar.ps1` | 官方启动器 asar 解包辅助脚本（仅调研用，不分发官方代码） |

## 运行与构建

要求 Node.js 20+。

```bash
cd ui
npm install
npm run dev      # 开发：Vite 静态服务（默认 5273），主进程用 MEOW_DEV_SERVER 或 --dev-server 指向它
npm run build    # 构建渲染层到 dist/ui
```

主进程：`node src/main/index.mjs`（或在 Electron 运行时内启动）。渲染层解析顺序：`MEOW_DEV_SERVER` / `--dev-server=` → `ui/index.html` → `src/renderer/index.html`。

## 版本方案

- 正式版 `YY.minor.patch`（当前 `26.1.0`），预发布 `-rc.N` / `-beta.N`。
- 快照版本 `YYwWWx`（ISO 周，当前 `26w42a`）；通道：`dev` / `snapshot` / `beta` / `stable`。
- 游戏目录按 `<game.root>/versions/<name>/version.json` 扫描；配置与日志由主进程维护。

## 许可

本仓库不含游戏客户端、服务端程序或游戏资源；游戏内容及相关知识产权归原权利人所有。使用声明见 [`NOTICE.txt`](NOTICE.txt)。