//! 启动前的 WebView2 Runtime 检测与引导安装（README §13.1 第 6 项、§13.2）。
//!
//! 单文件发行（`bundle.active=false`）下，配置里声明的窗口由 Tauri 在 `setup` 之前创建，
//! 运行库缺失时先弹出 Tauri 自带的英文对话框，Rust 侧拿不到可处理的错误，
//! 所以这里在**创建任何窗口之前**主动检测：
//!
//! 1. 环境变量 `WEBVIEW2_BROWSER_EXECUTABLE_FOLDER` 指向有效运行库 → 放行；
//! 2. 与 exe 同目录存在 Fixed Version（`WebView2Runtime\msedgewebview2.exe`）→ 放行；
//!    注意 Tauri 2.12 在 `Builder::build()` 里读的就是上面这个环境变量，所以放行前后必须由
//!    `apply_runtime_override()` 把它写进去，否则检测通过、实际创建 WebView 仍会去找系统运行库；
//! 3. 注册表 `pv`（HKLM 32/64 位视图、HKCU）大于 `0.0.0.0`、**且运行库文件确实存在**，
//!    并且版本 ≥ `MINIMUM_VERSION` → 放行；登记在册但文件被删、或版本过低都不算可用；
//! 4. 都不满足 → 用内置的官方 Evergreen 引导程序静默安装/升级（不提权 ⇒ 装成当前用户级，
//!    正好绕开“装在别的 Windows 账号下”的情形），安装期间轮询，装好即放行。
//!
//! 这里落地 README §13.2“检测方式”的两条口径：**别只信 `pv`**（精简版常见“注册表还在、
//! `msedgewebview2.exe` 已被删”，所以登记值必须与文件存在性双查）、**存在不等于能用**
//! （低于 `MINIMUM_VERSION` 的运行时界面会走形，按不可用处理）。权威判定仍然是
//! “实际创建 WebView 是否成功”，版本常量只用于提前提示与自愈。
//!
//! 已知边界：版本下限只比对注册表/目录报告出来的 Evergreen（`registry:`）；第 1、2 条的
//! `env:`／`fixed:` 路径不比对版本——固定版目录被改名后拿不到版本号，读
//! `msedgewebview2.exe` 的 FileVersion 还要再引 `version.dll`，而这两条路径的运行时是我们
//! （或冒烟测试）自己指定的，真要做时在这里补即可。
//!
//! 注意：不提供把运行库整包内置的方案——官方 Fixed Version 本身体积就超过 250 MB，
//! 塞进单文件 exe 不现实（README §13.2 已定“在线优先”）。

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// 官方 WebView2 Evergreen 引导程序（Microsoft Edge Update，签名者 Microsoft Corporation，
/// 文件版本 1.3.287.9）。运行前重新校验摘要，避免发行包里的这份被替换。
const BOOTSTRAPPER: &[u8] = include_bytes!("../assets/MicrosoftEdgeWebview2Setup.exe");
const BOOTSTRAPPER_SHA256: &str = "baa78c71c5615533a1ee0c4d2ca2ea3fd24dae01965f9033ac530a61b5aa93a0";
/// 运行库客户端注册表键（`pv` 为版本字符串；微软《分发应用和 WebView2 运行时》§检测是否已安装）。
const CLIENT_KEY: &str = r"SOFTWARE\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}";
const RUNTIME_EXE: &str = "msedgewebview2.exe";
const INSTALL_TIMEOUT: Duration = Duration::from_secs(600);
pub const DOWNLOAD_URL: &str = "https://developer.microsoft.com/zh-CN/microsoft-edge/webview2/consumer/";
/// 最低可用版本：Chromium 93。UI 用到 flex `gap`（84+）、`inset`（87+）、`accent-color`（93+），
/// 低于此界面会走形（README §13.2“版本过低”）。低于下限按“不可用”处理并走升级链。
pub const MINIMUM_VERSION: &str = "93.0.961.0";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Availability {
    /// 已可用；字符串是来源说明（便于日志定位）。
    Ready(String),
    /// 已安装但版本低于 `MINIMUM_VERSION`：界面不可用，需要升级；字符串是登记版本号。
    TooOld(String),
    /// 未找到可用运行库；字符串说明原因（如 `runtime not found`）。
    Missing(String),
}

impl Availability {
    pub fn is_ready(&self) -> bool {
        matches!(self, Availability::Ready(_))
    }
    /// `--check-webview2` 与日志用的机器可读标记（README §13.2“检测方式”）：
    /// `env:<目录>` / `fixed:<目录>` / `registry:<版本>` / `too-old:<版本>` / `missing:<原因>`。
    pub fn detail(&self) -> String {
        match self {
            Availability::Ready(detail) => detail.clone(),
            Availability::TooOld(version) => format!("too-old:{version}"),
            Availability::Missing(reason) => format!("missing:{reason}"),
        }
    }
    /// 原生对话框用的中文说明（缺失与版本过低要给出不同的话）。
    pub fn user_reason(&self) -> String {
        match self {
            Availability::Ready(detail) => format!("已检测到 WebView2 运行库（{detail}）。"),
            Availability::TooOld(version) => format!(
                "当前 WebView2 运行库版本过低（{version}），启动器界面需要 {MINIMUM_VERSION} 或更高版本。"
            ),
            Availability::Missing(_) => "未检测到可用的 Microsoft Edge WebView2 运行库。".into(),
        }
    }
}

/// 双查通过的运行库：登记/目录报告的版本号，以及实际可执行文件位置。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Installed {
    pub version: String,
    pub executable: PathBuf,
}

/// 只读检测，不写注册表、不装任何东西。
pub fn availability() -> Availability {
    if let Some(directory) = std::env::var_os("WEBVIEW2_BROWSER_EXECUTABLE_FOLDER") {
        let directory = PathBuf::from(directory);
        if directory.join(RUNTIME_EXE).is_file() {
            return Availability::Ready(format!("env:{}", directory.display()));
        }
    }
    if let Some(directory) = fixed_runtime_beside_executable() {
        return Availability::Ready(format!("fixed:{}", directory.display()));
    }
    if let Some(installed) = installed_runtime() {
        if meets_minimum(&installed.version) {
            Availability::Ready(format!("registry:{}", installed.version))
        } else {
            Availability::TooOld(installed.version)
        }
    } else if let Some(version) = installed_version() {
        Availability::Missing(format!("registry:{version} without {RUNTIME_EXE}"))
    } else {
        Availability::Missing("runtime not found".into())
    }
}

/// 与当前 exe 同目录的 Fixed Version 运行库（用户手工放入时可免安装）。
pub fn fixed_runtime_beside_executable() -> Option<PathBuf> {
    let directory = std::env::current_exe()
        .ok()?
        .parent()?
        .join("WebView2Runtime");
    directory.join(RUNTIME_EXE).is_file().then_some(directory)
}

/// 把“与 exe 同目录的固定版本运行库”真正交给 WebView2：Tauri 2.12 在创建运行时之前读
/// `WEBVIEW2_BROWSER_EXECUTABLE_FOLDER`（`tauri/src/app.rs` 的 `Builder::build()`），
/// 所以必须在 `tauri::Builder` 之前调用。已有有效变量时不覆盖（打包器/用户设置优先）。
pub fn apply_runtime_override() -> Option<PathBuf> {
    if let Some(directory) = std::env::var_os("WEBVIEW2_BROWSER_EXECUTABLE_FOLDER") {
        if PathBuf::from(&directory).join(RUNTIME_EXE).is_file() {
            return None;
        }
    }
    let directory = fixed_runtime_beside_executable()?;
    std::env::set_var("WEBVIEW2_BROWSER_EXECUTABLE_FOLDER", &directory);
    Some(directory)
}

/// 让运行库就绪：先复用已有安装；缺失或版本过低时用内置引导程序静默安装/升级。
/// 返回 Ok(来源说明)；失败返回可展示给用户的中文说明。
pub fn ensure_runtime() -> Result<String, String> {
    match availability() {
        Availability::Ready(detail) => Ok(detail),
        Availability::TooOld(_) => {
            install_bundled()?;
            match availability() {
                Availability::Ready(detail) => Ok(detail),
                Availability::TooOld(still) => Err(format!(
                    "自动升级后运行库仍是 {still}，低于要求的 {MINIMUM_VERSION}。"
                )),
                Availability::Missing(reason) => Err(format!("升级后未检测到运行库：{reason}")),
            }
        }
        Availability::Missing(_) => {
            install_bundled()?;
            match availability() {
                Availability::Ready(detail) => Ok(detail),
                Availability::TooOld(version) => Err(format!(
                    "安装后的运行库为 {version}，仍低于要求的 {MINIMUM_VERSION}。"
                )),
                Availability::Missing(reason) => Err(format!("WebView2 运行库安装后仍未检测到：{reason}")),
            }
        }
    }
}

/// 运行内置的官方引导程序（`/silent /install`，不提权 ⇒ 当前用户级安装）。
/// 已有的低版本 Evergreen 会被它升级到最新，因此“版本过低”与“缺失”共用同一条链。
pub fn install_bundled() -> Result<String, String> {
    if !sha256_hex(BOOTSTRAPPER).eq_ignore_ascii_case(BOOTSTRAPPER_SHA256) {
        return Err("内置的 WebView2 引导程序校验值不符，已中止安装。".into());
    }
    let directory = std::env::temp_dir().join("meowlauncher-webview2");
    std::fs::create_dir_all(&directory).map_err(|e| format!("无法创建临时目录：{e}"))?;
    let installer = directory.join("MicrosoftEdgeWebview2Setup.exe");
    std::fs::write(&installer, BOOTSTRAPPER).map_err(|e| format!("无法写入安装程序：{e}"))?;
    let mut child = std::process::Command::new(&installer)
        .args(["/silent", "/install"])
        .spawn()
        .map_err(|e| format!("无法启动安装程序：{e}"))?;
    let started = Instant::now();
    loop {
        if let Some(installed) = usable_installed() {
            let _ = std::fs::remove_file(&installer);
            return Ok(format!(
                "installed:{} ({})",
                installed.version,
                installed.executable.display()
            ));
        }
        match child.try_wait() {
            Ok(Some(status)) => {
                let _ = std::fs::remove_file(&installer);
                return match usable_installed() {
                    Some(installed) => Ok(format!(
                        "installed:{} ({}, status={status})",
                        installed.version,
                        installed.executable.display()
                    )),
                    None => match installed_runtime() {
                        // 装完仍低于下限：不能放行，否则界面会走形（README §13.2 验收 6）。
                        Some(installed) => Err(format!(
                            "安装程序退出（{status}），运行库仍是 {}，低于要求的 {MINIMUM_VERSION}。",
                            installed.version
                        )),
                        None => Err(format!("安装程序退出（{status}），但未检测到运行库。")),
                    },
                };
            }
            Ok(None) => {}
            Err(e) => return Err(format!("等待安装程序失败：{e}")),
        }
        if started.elapsed() > INSTALL_TIMEOUT {
            let _ = child.kill();
            return Err("安装超时（10 分钟）。".into());
        }
        std::thread::sleep(Duration::from_millis(500));
    }
}

/// 注册表登记 + 运行库文件双查后的结果；多处登记时取版本最高的那个。
pub fn installed_runtime() -> Option<Installed> {
    let mut versions = registered_versions();
    versions.sort_by(|a, b| version_key(b).cmp(&version_key(a)));
    versions.into_iter().find_map(|reported| {
        locate_binary(&reported).map(|(version, executable)| Installed {
            version,
            executable,
        })
    })
}

/// `availability()` 的判定口径：已安装且版本达标。
pub fn usable_installed() -> Option<Installed> {
    installed_runtime().filter(|installed| meets_minimum(&installed.version))
}

/// 注册表里登记的版本号（**不校验文件是否存在**，供日志定位与诊断）。
pub fn installed_version() -> Option<String> {
    registered_versions()
        .into_iter()
        .max_by(|a, b| version_key(a).cmp(&version_key(b)))
}

/// 三处注册表位置的 `pv`（去重；HKLM 32 位视图 → HKLM 64 位视图 → HKCU）。
fn registered_versions() -> Vec<String> {
    let mut versions = Vec::new();
    for (root, view) in [
        (HKEY_LOCAL_MACHINE, KEY_WOW64_32KEY),
        (HKEY_LOCAL_MACHINE, KEY_WOW64_64KEY),
        (HKEY_CURRENT_USER, 0),
    ] {
        if let Some(version) = read_pv(root, view) {
            let version = version.trim().to_string();
            if is_installed_value(&version) && !versions.contains(&version) {
                versions.push(version);
            }
        }
    }
    versions
}

/// 微软文档：`pv` 缺失、空串或 `0.0.0.0` 都表示未安装。
fn is_installed_value(version: &str) -> bool {
    let version = version.trim();
    !version.is_empty() && version != "0.0.0.0"
}

/// 版本是否达到最低可用版本；解析不了时放行（宁可让可用机器过关，也不把它挡在门外，
/// 权威判定始终是“实际创建 WebView 是否成功”）。
fn meets_minimum(version: &str) -> bool {
    match (version_key(version), version_key(MINIMUM_VERSION)) {
        (Some(current), Some(minimum)) => current >= minimum,
        _ => true,
    }
}

/// 把 `126.0.2592.87` 这类点分版本号转成可比较的数字序列；出现非数字段时返回 None。
fn version_key(text: &str) -> Option<Vec<u32>> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    let mut parts = Vec::new();
    for part in text.split('.') {
        let digits: String = part.chars().take_while(|c| c.is_ascii_digit()).collect();
        if digits.is_empty() {
            return None;
        }
        parts.push(digits.parse().ok()?);
    }
    Some(parts)
}

/// Evergreen 的官方安装位置：机器级默认落在 32 位视图的 `Program Files (x86)`，
/// 按用户级落在 `%LOCALAPPDATA%`（机器级与用户级可以并存，取实际存在且版本最高的）。
fn runtime_roots() -> Vec<PathBuf> {
    ["ProgramFiles(x86)", "ProgramFiles", "LOCALAPPDATA"]
        .iter()
        .filter_map(|name| std::env::var_os(name))
        .map(|base| {
            PathBuf::from(base)
                .join("Microsoft")
                .join("EdgeWebView")
                .join("Application")
        })
        .collect()
}

/// 按注册表报告的版本号在安装根目录里找可执行文件；找不到目录时退化为“该根目录下最高的
/// 版本目录”（精简版常见 `pv` 与实际目录不一致）。
fn locate_binary(reported: &str) -> Option<(String, PathBuf)> {
    locate_binary_in(&runtime_roots(), reported)
}

fn locate_binary_in(roots: &[PathBuf], reported: &str) -> Option<(String, PathBuf)> {
    for root in roots {
        let exact = root.join(reported).join(RUNTIME_EXE);
        if exact.is_file() {
            return Some((reported.to_string(), exact));
        }
    }
    roots.iter().find_map(|root| highest_runtime(root))
}

/// 某个安装根目录下版本最高的运行库目录（目录名必须是点分版本号且含可执行文件）。
fn highest_runtime(root: &Path) -> Option<(String, PathBuf)> {
    let mut best: Option<(Vec<u32>, String, PathBuf)> = None;
    for entry in std::fs::read_dir(root).ok()?.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        let Some(key) = version_key(&name) else { continue };
        let executable = entry.path().join(RUNTIME_EXE);
        if !executable.is_file() {
            continue;
        }
        if best.as_ref().is_none_or(|(best_key, _, _)| key > *best_key) {
            best = Some((key, name, executable));
        }
    }
    best.map(|(_, version, executable)| (version, executable))
}

#[cfg(windows)]
fn read_pv(root: Hkey, view: u32) -> Option<String> {
    let subkey = wide(CLIENT_KEY);
    let name = wide("pv");
    unsafe {
        let mut handle: Hkey = 0;
        if RegOpenKeyExW(root, subkey.as_ptr(), 0, KEY_READ | view, &mut handle) != 0 {
            return None;
        }
        let mut buffer = [0u8; 128];
        let mut length = buffer.len() as u32;
        let mut kind = 0u32;
        let status = RegQueryValueExW(
            handle,
            name.as_ptr(),
            std::ptr::null_mut(),
            &mut kind,
            buffer.as_mut_ptr(),
            &mut length,
        );
        RegCloseKey(handle);
        if status != 0 || length == 0 {
            return None;
        }
        let bytes = &buffer[..(length as usize).min(buffer.len())];
        let units: Vec<u16> = bytes
            .chunks_exact(2)
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
            .take_while(|unit| *unit != 0)
            .collect();
        String::from_utf16(&units).ok()
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(windows)]
fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

// ---- Win32：advapi32 注册表读取（不引入新依赖，风格与 main.rs 的 dwmapi 调用一致） ----

#[cfg(windows)]
type Hkey = isize;
#[cfg(windows)]
const HKEY_CURRENT_USER: Hkey = 0x8000_0001u32 as Hkey;
#[cfg(windows)]
const HKEY_LOCAL_MACHINE: Hkey = 0x8000_0002u32 as Hkey;
#[cfg(windows)]
const KEY_READ: u32 = 0x0002_0019;
#[cfg(windows)]
const KEY_WOW64_32KEY: u32 = 0x0200;
#[cfg(windows)]
const KEY_WOW64_64KEY: u32 = 0x0100;

#[cfg(windows)]
#[link(name = "advapi32")]
extern "system" {
    fn RegOpenKeyExW(
        key: Hkey,
        sub_key: *const u16,
        options: u32,
        desired: u32,
        result: *mut Hkey,
    ) -> i32;
    fn RegQueryValueExW(
        key: Hkey,
        value_name: *const u16,
        reserved: *mut u32,
        kind: *mut u32,
        data: *mut u8,
        length: *mut u32,
    ) -> i32;
    fn RegCloseKey(key: Hkey) -> i32;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_bootstrapper_matches_pinned_digest() {
        assert!(sha256_hex(BOOTSTRAPPER).eq_ignore_ascii_case(BOOTSTRAPPER_SHA256));
    }

    #[test]
    fn zero_or_empty_version_counts_as_missing() {
        assert!(!is_installed_value(""));
        assert!(!is_installed_value("   "));
        assert!(!is_installed_value("0.0.0.0"));
        assert!(is_installed_value("126.0.2592.87"));
    }

    #[test]
    fn version_floor_follows_the_ui_baseline() {
        assert!(meets_minimum("93.0.961.0"));
        assert!(meets_minimum("126.0.2592.87"));
        assert!(!meets_minimum("92.0.902.0"));
        assert!(!meets_minimum("86.0.622.0"));
        // 解析不了的登记值不拦（权威判定是实际创建 WebView 是否成功）。
        assert!(meets_minimum("unknown"));
    }

    #[test]
    fn runtime_is_found_by_file_not_by_registry() {
        let root = std::env::temp_dir().join(format!("meow-webview2-test-{}", std::process::id()));
        let older = root.join("92.0.902.0");
        let newer = root.join("126.0.2592.87");
        std::fs::create_dir_all(&older).unwrap();
        std::fs::create_dir_all(&newer).unwrap();
        // 只有目录、没有可执行文件：不算安装（精简版“注册表还在、文件被删”）。
        assert!(locate_binary_in(&[root.clone()], "126.0.2592.87").is_none());
        std::fs::write(newer.join(RUNTIME_EXE), b"stub").unwrap();
        std::fs::write(older.join(RUNTIME_EXE), b"stub").unwrap();
        assert_eq!(
            locate_binary_in(&[root.clone()], "126.0.2592.87").unwrap().1,
            newer.join(RUNTIME_EXE)
        );
        // 登记值与实际目录不一致时，退化为取版本最高的那个目录。
        assert_eq!(
            locate_binary_in(&[root.clone()], "124.0.2478.0").unwrap().0,
            "126.0.2592.87"
        );
        assert_eq!(highest_runtime(&root).unwrap().0, "126.0.2592.87");
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn availability_reports_a_source() {
        let state = availability();
        let detail = state.detail();
        assert!(!detail.is_empty());
        assert!(!state.user_reason().is_empty());
        assert_eq!(
            state.is_ready(),
            detail.starts_with("env:")
                || detail.starts_with("fixed:")
                || detail.starts_with("registry:")
        );
        assert!(
            ["env:", "fixed:", "registry:", "too-old:", "missing:"]
                .iter()
                .any(|prefix| detail.starts_with(prefix)),
            "unexpected detail: {detail}"
        );
        if let Availability::Missing(reason) = &state {
            assert!(!reason.is_empty());
        }
    }
}
