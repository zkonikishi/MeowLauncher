@echo off
chcp 65001 >nul
setlocal
cd /d "%~dp0"

set "EXE="
if exist "%~dp0MeowLauncher.exe" set "EXE=%~dp0MeowLauncher.exe"
if exist "%~dp0meow-launcher.exe" set "EXE=%~dp0meow-launcher.exe"
if "%EXE%"=="" (
  echo [X] 没有找到 MeowLauncher.exe / meow-launcher.exe。
  echo     请把本脚本和启动器 exe 放在同一个文件夹里再运行。
  pause
  exit /b 1
)

echo ==================================================
echo  MeowLauncher WebView2 诊断（只读，不改系统设置）
echo  启动器：%EXE%
echo ==================================================
echo.
echo [1/3] 安装/升级之前的运行库状态：
"%EXE%" --check-webview2 --output "%~dp0webview2-before.json"
type "%~dp0webview2-before.json"
echo.
echo.
echo [2/3] 请现在双击启动器 MeowLauncher.exe：
echo       - 若弹出中文“缺少/版本过低”提示，选“是”让它自动安装或升级；
echo       - 等界面出现、或等它报错结束，然后关闭启动器；
echo       - 回到这里按任意键继续。
pause >nul
"%EXE%" --check-webview2 --output "%~dp0webview2-after.json"
type "%~dp0webview2-after.json"
echo.
echo.
echo [3/3] 本地诊断日志（只有受控事件码，不含账号信息）：
if exist "%~dp0data\launcher.log" (
  type "%~dp0data\launcher.log"
) else (
  echo （没有 data\launcher.log）
)
echo.
echo ==================================================
echo  请把下面文件发回：
echo    webview2-before.json、webview2-after.json、data\launcher.log
echo  （本机 Windows 版本、以及是否断网/能否访问微软下载站也请一并说明）
echo ==================================================
pause
