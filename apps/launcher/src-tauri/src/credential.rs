//! 0.6.0 第 7 项「记住密码」的本机凭据存储。
//!
//! 约定见 README §13.1 第 7 项：密文文件独立于 `settings.json`
//! （`<启动器数据目录>/credential.bin`），只用 Windows DPAPI
//! `CryptProtectData`（CurrentUser 范围）加密——与 `main.rs` 里 `dwmapi`
//! 同一套裸 FFI 风格，不新增依赖。禁止明文、禁止自研可逆加密、禁止写
//! 注册表明文或日志。密码在内存里以 `Zeroizing<String>` 流转，只用于换取
//! 300 秒一次性凭据。

use std::path::{Path, PathBuf};
use zeroize::Zeroizing;

/// 容器魔数（版本 1），用于把损坏文件与外来文件区分开。
const MAGIC: &[u8] = b"MEOWLAUNCHER-CREDENTIAL\x01";
/// DPAPI 不弹任何提示框（启动器无人值守）。
#[allow(dead_code)]
const CRYPTPROTECT_UI_FORBIDDEN: u32 = 0x01;
/// 标识符长度上限，避免损坏文件造成超大分配。
const MAX_IDENTIFIER: usize = 1024;
/// 密文长度上限（DPAPI 输出通常是明文长度加数百字节）。
const MAX_BLOB: usize = 65536;

/// 密文文件名；始终与 `settings.json` 同目录、但独立存放。
pub const FILE_NAME: &str = "credential.bin";

/// 一份已保存的凭据。密码只在内存中，落盘时必为 DPAPI 密文。
pub struct SavedCredential {
    pub identifier: String,
    pub password: Zeroizing<String>,
}

pub fn path(data_directory: &Path) -> PathBuf {
    data_directory.join(FILE_NAME)
}

/// 加密保存（先写临时文件再改名，避免中途断电留下半截密文）。
pub fn save(data_directory: &Path, identifier: &str, password: &str) -> Result<(), String> {
    let identifier = identifier.trim();
    if identifier.len() != 8 || identifier == "00000000" || !identifier.bytes().all(|b| b.is_ascii_digit()) {
        return Err("账号为空，未保存密码。".into());
    }
    if password.is_empty() || password.chars().count() > 128 {
        return Err("密码为空，未保存。".into());
    }
    let blob = platform::protect(password.as_bytes())?;
    let bytes = encode(identifier, &blob);
    let target = path(data_directory);
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("无法创建启动器数据目录：{e}"))?;
    }
    let temp = target.with_extension("bin.tmp");
    std::fs::write(&temp, &bytes).map_err(|e| format!("保存密码失败：{e}"))?;
    if let Err(e) = std::fs::rename(&temp, &target) {
        let _ = std::fs::remove_file(&temp);
        return Err(format!("保存密码失败：{e}"));
    }
    Ok(())
}

/// 读取并解密；文件缺失、格式损坏、或换了 Windows 账号/机器（DPAPI 解不开）
/// 都返回 `None`，由调用方回退到手动输入密码。
pub fn load(data_directory: &Path) -> Option<SavedCredential> {
    use std::io::Read;
    let file = std::fs::File::open(path(data_directory)).ok()?;
    let limit = MAGIC.len() + 8 + MAX_IDENTIFIER + MAX_BLOB;
    let mut bytes = Vec::new();
    file.take((limit + 1) as u64).read_to_end(&mut bytes).ok()?;
    if bytes.len() > limit { return None; }
    let (identifier, blob) = decode(&bytes)?;
    let plain = platform::unprotect(&blob).ok()?;
    let password = std::str::from_utf8(&plain).ok()?.to_owned();

    Some(SavedCredential {
        identifier,
        password: Zeroizing::new(password),
    })
}

/// 清除本机保存的凭据；文件本来就不存在也算成功。
pub fn clear(data_directory: &Path) -> Result<(), String> {
    match std::fs::remove_file(path(data_directory)) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!("清除已保存密码失败：{e}")),
    }
}

pub fn exists(data_directory: &Path) -> bool {
    path(data_directory).is_file()
}

fn encode(identifier: &str, blob: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(MAGIC.len() + 8 + identifier.len() + blob.len());
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&(identifier.len() as u32).to_le_bytes());
    out.extend_from_slice(identifier.as_bytes());
    out.extend_from_slice(&(blob.len() as u32).to_le_bytes());
    out.extend_from_slice(blob);
    out
}

fn decode(bytes: &[u8]) -> Option<(String, Vec<u8>)> {
    if bytes.len() < MAGIC.len() + 8 || !bytes.starts_with(MAGIC) {
        return None;
    }
    let mut at = MAGIC.len();
    let identifier_length = read_length(bytes, &mut at)?;
    if identifier_length == 0 || identifier_length > MAX_IDENTIFIER || at + identifier_length > bytes.len() {
        return None;
    }
    let identifier = std::str::from_utf8(&bytes[at..at + identifier_length]).ok()?.to_string();
    at += identifier_length;
    let blob_length = read_length(bytes, &mut at)?;
    if blob_length == 0 || blob_length > MAX_BLOB || at + blob_length != bytes.len() {
        return None;
    }
    Some((identifier, bytes[at..at + blob_length].to_vec()))
}

fn read_length(bytes: &[u8], at: &mut usize) -> Option<usize> {
    let end = at.checked_add(4)?;
    if end > bytes.len() {
        return None;
    }
    let value = u32::from_le_bytes(bytes[*at..end].try_into().ok()?) as usize;
    *at = end;
    Some(value)
}

#[cfg(windows)]
mod platform {
    use std::ffi::c_void;
    use zeroize::Zeroizing;

    use super::CRYPTPROTECT_UI_FORBIDDEN;

    #[repr(C)]
    struct DataBlob {
        cb_data: u32,
        pb_data: *mut u8,
    }

    #[link(name = "crypt32")]
    unsafe extern "system" {
        fn CryptProtectData(
            p_data_in: *const DataBlob,
            sz_data_descr: *const u16,
            p_optional_entropy: *const DataBlob,
            pv_reserved: *mut c_void,
            p_prompt_struct: *mut c_void,
            dw_flags: u32,
            p_data_out: *mut DataBlob,
        ) -> i32;
        fn CryptUnprotectData(
            p_data_in: *const DataBlob,
            ppsz_data_descr: *mut *mut u16,
            p_optional_entropy: *const DataBlob,
            pv_reserved: *mut c_void,
            p_prompt_struct: *mut c_void,
            dw_flags: u32,
            p_data_out: *mut DataBlob,
        ) -> i32;
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn LocalFree(memory: *mut c_void) -> *mut c_void;
    }

    pub fn protect(plain: &[u8]) -> Result<Vec<u8>, String> {
        if plain.len() > u32::MAX as usize {
            return Err("密码过长，无法保存。".into());
        }
        let input = DataBlob {
            cb_data: plain.len() as u32,
            pb_data: plain.as_ptr() as *mut u8,
        };
        let mut output = DataBlob {
            cb_data: 0,
            pb_data: std::ptr::null_mut(),
        };
        let ok = unsafe {
            CryptProtectData(
                &input,
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        };
        if ok == 0 {
            return Err(format!("Windows DPAPI 加密失败（{}）。", std::io::Error::last_os_error()));
        }
        let bytes = unsafe { std::slice::from_raw_parts(output.pb_data, output.cb_data as usize) }.to_vec();
        unsafe { LocalFree(output.pb_data as *mut c_void) };
        Ok(bytes)
    }

    pub fn unprotect(blob: &[u8]) -> Result<Zeroizing<Vec<u8>>, String> {
        if blob.is_empty() || blob.len() > u32::MAX as usize {
            return Err("密文无效。".into());
        }
        let input = DataBlob {
            cb_data: blob.len() as u32,
            pb_data: blob.as_ptr() as *mut u8,
        };
        let mut output = DataBlob {
            cb_data: 0,
            pb_data: std::ptr::null_mut(),
        };
        let ok = unsafe {
            CryptUnprotectData(
                &input,
                std::ptr::null_mut(),
                std::ptr::null(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        };
        if ok == 0 {
            return Err("Windows DPAPI 解密失败：该凭据只能由保存它的 Windows 账号在同一台机器上解密。".into());
        }
        let bytes = unsafe { std::slice::from_raw_parts(output.pb_data, output.cb_data as usize) }.to_vec();
        use zeroize::Zeroize;
        unsafe { std::slice::from_raw_parts_mut(output.pb_data, output.cb_data as usize) }.zeroize();
        unsafe { LocalFree(output.pb_data as *mut c_void) };
        Ok(Zeroizing::new(bytes))
    }
}

#[cfg(not(windows))]
mod platform {
    use zeroize::Zeroizing;

    pub fn protect(_plain: &[u8]) -> Result<Vec<u8>, String> {
        Err("仅 Windows 支持本机凭据保存。".into())
    }

    pub fn unprotect(_blob: &[u8]) -> Result<Zeroizing<Vec<u8>>, String> {
        Err("仅 Windows 支持本机凭据解密。".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    fn temp_directory(tag: &str) -> PathBuf {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let directory = std::env::temp_dir().join(format!(
            "meow-credential-{}-{tag}-{unique}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).expect("temp directory");
        directory
    }

    #[test]
    fn container_round_trip_and_bounds() {
        let blob = vec![1u8, 2, 3, 4, 5];
        assert_eq!(decode(&encode("12345678", &blob)), Some(("12345678".to_string(), blob)));
        let mut trailing = encode("12345678", &[1, 2, 3]);
        trailing.push(0);
        assert!(decode(&trailing).is_none());
        assert!(decode(&[]).is_none());
        assert!(decode(MAGIC).is_none());
        let full = encode("12345678", &[9u8; 16]);
        let mut truncated = full.clone();
        truncated.truncate(full.len() - 2);
        assert!(decode(&truncated).is_none());
        let mut wrong_magic = full.clone();
        wrong_magic[0] = b'X';
        assert!(decode(&wrong_magic).is_none());
        let mut oversized = Vec::new();
        oversized.extend_from_slice(MAGIC);
        oversized.extend_from_slice(&u32::MAX.to_le_bytes());
        assert!(decode(&oversized).is_none());
    }

    #[test]
    fn dpapi_ciphertext_hides_the_password_and_round_trips() {
        let plain = b"BanTest12345";
        let blob = platform::protect(plain).expect("protect");
        assert!(
            !blob.windows(plain.len()).any(|window| window == plain.as_slice()),
            "DPAPI 密文里不应出现明文密码"
        );
        let restored = platform::unprotect(&blob).expect("unprotect");
        assert_eq!(restored.as_slice(), plain.as_slice());
    }

    #[test]
    fn tampered_ciphertext_is_rejected() {
        let mut blob = platform::protect(b"secret-password").expect("protect");
        let last = blob.len() - 1;
        blob[last] ^= 0xff;
        assert!(platform::unprotect(&blob).is_err());
    }

    #[test]
    fn saved_credential_is_encrypted_on_disk_and_cleared() {
        let directory = temp_directory("save");
        save(&directory, "10000001", "s3cret-password").expect("save");
        assert!(exists(&directory));
        let raw = std::fs::read(path(&directory)).expect("read");
        assert!(raw.starts_with(MAGIC), "密文文件应带本启动器魔数");
        assert!(
            !raw.windows(15).any(|window| window == b"s3cret-password"),
            "磁盘上不应出现明文密码"
        );
        let loaded = load(&directory).expect("load");
        assert_eq!(loaded.identifier, "10000001");
        assert_eq!(loaded.password.as_str(), "s3cret-password");
        clear(&directory).expect("clear");
        assert!(!exists(&directory), "取消记住密码后不应再留下密文");
        assert!(load(&directory).is_none());
        clear(&directory).expect("clear is idempotent");
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn corrupted_or_foreign_file_is_ignored() {
        let directory = temp_directory("corrupt");
        std::fs::write(path(&directory), b"not a credential file").expect("write");
        assert!(load(&directory).is_none());
        let _ = std::fs::remove_dir_all(&directory);
    }
}
