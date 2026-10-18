use crate::{error, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    pub id: String,
    pub version_id: String,
    pub executable_sha256: String,
    pub game_host: String,
    pub game_port: u16,
    pub argument_prefix: String,
    pub argument_suffix: String,
}
pub fn profile() -> Profile {
    serde_json::from_str(include_str!("../../../adapters/s4a21/profile.json"))
        .expect("compiled adapter profile")
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckItem {
    pub name: String,
    pub ok: bool,
    pub detail: String,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientReport {
    pub directory: String,
    pub ready: bool,
    pub items: Vec<CheckItem>,
    pub note: &'static str,
    pub warnings: Vec<String>,
}
const NOTE: &str = "本次检查确认文件存在、架构及主程序版本；插件功能与进入选角仍需实机验证。";

fn safe_file(root: &Path, relative: &str) -> Result<PathBuf> {
    if relative.is_empty()
        || relative.contains([':', '\\', '/'])
        || relative == "."
        || relative == ".."
        || relative.contains('\0')
    {
        return Err(error(
            "unsafe_plugin_path",
            "插件路径必须是客户端根目录内的文件名。",
        ));
    }
    let p = root
        .join(relative)
        .canonicalize()
        .map_err(|_| error("missing_file", "缺少客户端文件。"))?;
    if !p.starts_with(root) || !p.is_file() {
        return Err(error(
            "unsafe_plugin_path",
            "客户端文件指向目录外或不是文件。",
        ));
    }
    Ok(p)
}
fn x86_file(path: &Path) -> bool {
    let Ok(mut f) = File::open(path) else {
        return false;
    };
    let mut header = [0u8; 64];
    if f.read_exact(&mut header).is_err() || &header[0..2] != b"MZ" {
        return false;
    }
    let offset = u32::from_le_bytes(header[60..64].try_into().unwrap()) as u64;
    let mut pe = [0u8; 6];
    f.seek(SeekFrom::Start(offset)).is_ok()
        && f.read_exact(&mut pe).is_ok()
        && &pe[..4] == b"PE\0\0"
        && u16::from_le_bytes([pe[4], pe[5]]) == 0x14c
}
pub fn sha256_file(path: &Path) -> Result<String> {
    let mut f = File::open(path).map_err(|_| error("file_read", "无法读取客户端文件。"))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 65536];
    loop {
        let n = f
            .read(&mut buffer)
            .map_err(|_| error("file_read", "读取客户端文件失败。"))?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}
pub fn inspect(directory: &str) -> Result<ClientReport> {
    inspect_for_hash(directory,&profile().executable_sha256)
}
fn inspect_for_hash(directory:&str,expected_sha:&str)->Result<ClientReport>{
    let root = Path::new(directory.trim());
    if !root.is_absolute() || !root.is_dir() {
        return Err(error(
            "invalid_directory",
            "请选择包含 DNF.exe 的客户端目录。",
        ));
    }
    let root = root
        .canonicalize()
        .map_err(|_| error("invalid_directory", "无法访问客户端目录。"))?;
    let mut items = Vec::new();
    for name in ["DNF.exe", "az.dll", "ijl15.dll", "GameGaurd.dll"] {
        let ok = safe_file(&root, name).is_ok_and(|p| x86_file(&p));
        items.push(CheckItem {
            name: name.into(),
            ok,
            detail: if ok {
                "x86 文件可读取"
            } else {
                "缺失、路径异常或不是 x86 PE 文件"
            }
            .into(),
        });
    }
    let fingerprint = safe_file(&root, "DNF.exe").and_then(|p| sha256_file(&p));
    let matched = fingerprint
        .as_ref()
        .is_ok_and(|h| h == expected_sha);
    items.push(CheckItem {
        name: "客户端版本".into(),
        ok: matched,
        detail: if matched {
            "匹配已记录的 S4A21_CN 主程序"
        } else {
            "主程序指纹不匹配，当前启动模板不可用"
        }
        .into(),
    });
    let pvf = safe_file(&root, "Script.pvf").is_ok_and(|p| p.metadata().is_ok_and(|m| m.len() > 0));
    items.push(CheckItem {
        name: "Script.pvf".into(),
        ok: pvf,
        detail: if pvf {
            "存在且非空（未验证发布签名）"
        } else {
            "文件缺失或为空"
        }
        .into(),
    });
    for name in ["ImagePacks2", "SoundPacks"] {
        let ok = root
            .join(name)
            .canonicalize()
            .is_ok_and(|p| p.starts_with(&root) && p.is_dir());
        items.push(CheckItem {
            name: name.into(),
            ok,
            detail: if ok {
                "资源目录存在"
            } else {
                "资源目录缺失或指向目录外"
            }
            .into(),
        });
    }
    // GameGaurd.ini 属于玩家/客户端自有配置：启动器不检测、不读取、不解析、不展示，
    // 也不据其检查插件 DLL。文件是否存在都与启动条件无关。
    let warnings=Vec::new();
    Ok(ClientReport {
        directory: directory.trim().into(),
        ready: items.iter().all(|i| i.ok),
        items,
        note: NOTE,
        warnings,
    })
}
pub fn executable(directory: &str) -> Result<PathBuf> {
    let root = Path::new(directory)
        .canonicalize()
        .map_err(|_| error("invalid_directory", "无法访问客户端目录。"))?;
    safe_file(&root, "DNF.exe")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    #[test] fn original_archive_is_ready_but_required_integrity_is_enforced(){
        let d=tempfile::tempdir().unwrap();let root=d.path();let mut exe=vec![0u8;2048];exe[..2].copy_from_slice(b"MZ");exe[60..64].copy_from_slice(&128u32.to_le_bytes());exe[128..134].copy_from_slice(b"PE\0\0\x4c\x01");
        for name in ["DNF.exe","az.dll","ijl15.dll","GameGaurd.dll"]{fs::write(root.join(name),&exe).unwrap();}
        fs::write(root.join("Script.pvf"),b"fixture never executed").unwrap();for name in ["ImagePacks2","SoundPacks"]{fs::create_dir(root.join(name)).unwrap();}
        let sha=sha256_file(&root.join("DNF.exe")).unwrap();let report=inspect_for_hash(root.to_str().unwrap(),&sha).unwrap();assert!(report.ready);assert!(report.warnings.is_empty());assert!(!root.join("GameGaurd.ini").exists());
        fs::remove_file(root.join("GameGaurd.dll")).unwrap();assert!(!inspect_for_hash(root.to_str().unwrap(),&sha).unwrap().ready);fs::write(root.join("GameGaurd.dll"),&exe).unwrap();exe[1000]=1;fs::write(root.join("DNF.exe"),exe).unwrap();assert!(!inspect_for_hash(root.to_str().unwrap(),&sha).unwrap().ready);
    }
    #[test]
    fn empty_directory_is_not_ready() {
        let d = tempfile::tempdir().unwrap();
        let r = inspect(d.path().to_str().unwrap()).unwrap();
        assert!(!r.ready);
        assert!(r.items.iter().any(|i| i.name == "DNF.exe" && !i.ok));
    }
    #[test]
    fn file_escape_and_truncated_pe_rejected() {
        let d = tempfile::tempdir().unwrap();
        assert!(safe_file(d.path(), "..\\other.dll").is_err());
        let p = d.path().join("DNF.exe");
        fs::write(&p, b"MZ").unwrap();
        assert!(!x86_file(&p));
    }
}
