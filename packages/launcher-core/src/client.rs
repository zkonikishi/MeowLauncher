use crate::{error, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
};

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Profile {
    pub id: String,
    pub version_id: String,
    pub executable: String,
    pub machine: String,
    pub executable_sha256: String,
    pub arguments: Vec<String>,
    #[serde(default)] pub required_files: Vec<String>,
    #[serde(default)] pub required_directories: Vec<String>,
}
fn valid_name(s: &str) -> bool {
    !s.is_empty() && s.len() <= 240 && s != "." && s != ".."
        && !s.ends_with(['.', ' ']) && !s.chars().any(|c| c.is_control() || "\\/:*?\"<>|".contains(c))
}
impl Profile {
    pub fn validate(&self) -> Result<()> {
        let invalid = || error("adapter_invalid", "适配器配置无效，请联系服主。 ");
        if self.id.is_empty() || self.id.len()>80 || !self.id.bytes().all(|b| b.is_ascii_alphanumeric() || b==b'-')
            || self.version_id.is_empty() || self.version_id.len()>80
            || !valid_name(&self.executable) || !self.executable.to_ascii_lowercase().ends_with(".exe")
            || !matches!(self.machine.as_str(), "x86" | "x64")
            || self.executable_sha256.len()!=64 || !self.executable_sha256.bytes().all(|b| b.is_ascii_hexdigit())
            || self.arguments.is_empty() || self.arguments.len()>64
            || self.required_files.len()>128 || self.required_directories.len()>128
            || self.required_files.iter().chain(&self.required_directories).any(|s| !valid_name(s)) { return Err(invalid()); }
        for argument in &self.arguments {
            let mut literal=argument.clone();
            for key in ["{host}","{port}","{uid}","{token}"] { literal=literal.replace(key, ""); }
            if argument.len()>4096 || argument.chars().any(char::is_control) || literal.contains(['{','}']) {return Err(invalid());}
        }
        Ok(())
    }
}
pub fn profile() -> Result<Profile> {
    let exe=std::env::current_exe().map_err(|_|error("adapter_not_configured", "未配置客户端适配器。"))?;
    let config=crate::public_config::PublicConfig::load(exe.parent().ok_or(error("adapter_not_configured", "未配置客户端适配器。"))?)?;
    config.map(|c|c.client_adapter).ok_or(error("adapter_not_configured", "请在 launcher.public.json 中配置 clientAdapter。"))
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
fn pe_file(path: &Path, machine: &str) -> bool {
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
        && u16::from_le_bytes([pe[4], pe[5]]) == if machine == "x64" {0x8664} else {0x14c}
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
pub fn inspect(directory: &str) -> Result<ClientReport> { inspect_with_profile(directory, &profile()?) }
pub fn inspect_with_profile(directory: &str, profile: &Profile) -> Result<ClientReport> {
    profile.validate()?;
    let root=Path::new(directory.trim());
    if !root.is_absolute() || !root.is_dir() {return Err(error("invalid_directory", "请选择客户端目录。"));}
    let root=root.canonicalize().map_err(|_|error("invalid_directory", "无法访问客户端目录。"))?;
    let mut items=Vec::new();
    let exe=safe_file(&root, &profile.executable);
    let ok=exe.as_ref().is_ok_and(|p| pe_file(p, &profile.machine) && sha256_file(p).is_ok_and(|h| h.eq_ignore_ascii_case(&profile.executable_sha256)));
    items.push(CheckItem{name:profile.executable.clone(),ok,detail:if ok {format!("匹配适配器 {} / {}",profile.id,profile.machine)} else {"主程序缺失、架构或指纹不匹配".into()}});
    for name in &profile.required_files {
        let ok=safe_file(&root,name).is_ok_and(|p| p.metadata().is_ok_and(|m| m.len()>0));
        items.push(CheckItem{name:name.clone(),ok,detail:"适配器要求的非空文件".into()});
    }
    for name in &profile.required_directories {
        let ok=root.join(name).canonicalize().is_ok_and(|p|p.starts_with(&root)&&p.is_dir());
        items.push(CheckItem{name:name.clone(),ok,detail:"适配器要求的资源目录".into()});
    }
    Ok(ClientReport{directory:directory.trim().into(),ready:items.iter().all(|i|i.ok),items,note:NOTE,warnings:Vec::new()})
}
pub fn executable_with_profile(directory:&str, profile:&Profile)->Result<PathBuf> {
    profile.validate()?;
    let root=Path::new(directory).canonicalize().map_err(|_|error("invalid_directory", "无法访问客户端目录。"))?;
    safe_file(&root, &profile.executable)
}

#[cfg(test)]
mod tests {
 use super::*;
 #[test] fn arbitrary_x64_client_and_required_resources() {
    let d=tempfile::tempdir().unwrap(); let mut exe=vec![0u8;256]; exe[..2].copy_from_slice(b"MZ"); exe[60..64].copy_from_slice(&128u32.to_le_bytes()); exe[128..134].copy_from_slice(b"PE\0\0\x64\x86");
    std::fs::write(d.path().join("OtherGame.exe"), &exe).unwrap();
    let mut p=Profile{id:"other-client".into(),version_id:"115".into(),executable:"OtherGame.exe".into(),machine:"x64".into(),executable_sha256:sha256_file(&d.path().join("OtherGame.exe")).unwrap(),arguments:vec!["{uid}".into()],required_files:vec![],required_directories:vec![]};
    assert!(inspect_with_profile(d.path().to_str().unwrap(), &p).unwrap().ready);
    p.machine="x86".into(); assert!(!inspect_with_profile(d.path().to_str().unwrap(), &p).unwrap().ready);
    p.machine="x64".into();p.required_files.push("payload.pvf".into());assert!(!inspect_with_profile(d.path().to_str().unwrap(), &p).unwrap().ready);
    p.executable="../escape.exe".into();assert!(p.validate().is_err());
 }
 #[test] fn rejects_unknown_placeholders() {let mut p=crate::public_config::PublicConfig::parse(include_bytes!("../fixtures/public-config.json")).unwrap().client_adapter;p.arguments=vec!["{password}".into()];assert!(p.validate().is_err());}
}
