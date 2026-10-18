use crate::{error, Result};
use serde::{Deserialize, Serialize};
use std::{fs, io::Write, path::Path};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Settings {
    pub version: u32,
    pub client_directory: String,
    pub theme: String,
    pub remember_identifier: bool,
    pub identifier: String,
    #[serde(default="enabled")]
    pub close_to_tray:bool,
    #[serde(default="enabled")]
    pub restore_after_game:bool,
}
fn enabled()->bool{true}
impl Default for Settings {
    fn default() -> Self {
        Self {
            version: 1,
            client_directory: String::new(),
            theme: "modern".into(),
            remember_identifier: false,
            identifier: String::new(),
            close_to_tray:true,
            restore_after_game:true,
        }
    }
}
impl Settings {
    pub fn normalize(mut self) -> Result<Self> {
        if self.version != 1 || !matches!(self.theme.as_str(), "modern" | "retro") {
            return Err(error("invalid_settings", "设置版本或界面类型无效。"));
        }
        self.client_directory = self.client_directory.trim().to_owned();
        if self.client_directory.len() > 4096 || self.client_directory.contains('\0') {
            return Err(error("invalid_path", "客户端路径无效。"));
        }
        self.identifier = if self.remember_identifier {
            if crate::edition::is_sponsor() { self.identifier.clone() } else { self.identifier.trim().to_owned() }
        } else {
            String::new()
        };
        if self.identifier.len() > 254 {
            return Err(error("invalid_identifier", "账号长度超出限制。"));
        }
        // Migrate historical remembered emails without deleting the existing data directory.
        let valid = if crate::edition::is_sponsor() { !self.identifier.is_empty() && self.identifier.len()<=32 && self.identifier.bytes().all(|b| b.is_ascii_alphanumeric() || b==b'_' || b==b'-') } else { crate::platform_identity::login_id(&self.identifier) && self.identifier!="00000000" };
        if !valid {
            self.identifier.clear();
        }
        Ok(self)
    }
}
pub fn load(path: &Path) -> Result<Settings> {
    if !path.exists() {
        return Ok(Settings::default());
    }
    let data = fs::read(path).map_err(|_| error("settings_read", "无法读取设置文件。"))?;
    if data.len() > 16384 {
        return Err(error("settings_invalid", "设置文件过大，请检查设置文件。"));
    }
    serde_json::from_slice::<Settings>(&data)
        .map_err(|_| error("settings_invalid", "设置文件损坏，原文件已保留。"))?
        .normalize()
}
pub fn save(path: &Path, settings: Settings) -> Result<Settings> {
    let settings = settings.normalize()?;
    let parent = path
        .parent()
        .ok_or(error("settings_write", "无法定位设置目录。"))?;
    fs::create_dir_all(parent).map_err(|_| {
        error(
            "settings_write",
            "无法创建设置目录，请将启动器放到可写目录。",
        )
    })?;
    let bytes = serde_json::to_vec_pretty(&settings)
        .map_err(|_| error("settings_write", "设置序列化失败。"))?;
    let mut temp = tempfile::NamedTempFile::new_in(parent)
        .map_err(|_| error("settings_write", "无法创建临时设置文件。"))?;
    temp.write_all(&bytes)
        .and_then(|_| temp.as_file().sync_all())
        .map_err(|_| error("settings_write", "设置写入失败，原设置已保留。"))?;
    temp.persist(path)
        .map_err(|_| error("settings_write", "无法替换设置文件，原设置已保留。"))?;
    Ok(settings)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn remembered_identifier_respects_edition_without_case_folding() {
        let name=Settings{remember_identifier:true,identifier:"Player_A-1".into(),..Default::default()}.normalize().unwrap();
        assert_eq!(name.identifier,if crate::edition::is_sponsor(){"Player_A-1"}else{""});
        let spaces=Settings{remember_identifier:true,identifier:" Player_A-1 ".into(),..Default::default()}.normalize().unwrap();
        assert!(spaces.identifier.is_empty());
    }
    #[test] fn old_settings_receive_window_defaults(){
        let old=serde_json::from_str::<Settings>(r#"{"version":1,"clientDirectory":"","theme":"modern","rememberIdentifier":false,"identifier":""}"#).unwrap();
        assert!(old.close_to_tray&&old.restore_after_game);
        let d=tempfile::tempdir().unwrap();let p=d.path().join("settings.json");let mut s=old;s.close_to_tray=false;s.restore_after_game=false;save(&p,s).unwrap();let loaded=load(&p).unwrap();assert!(!loaded.close_to_tray&&!loaded.restore_after_game);
    }
    #[test] fn old_email_memory_is_discarded_but_uid_zeroes_survive(){
        let old=Settings{remember_identifier:true,identifier:"test@example.invalid".into(),..Default::default()}.normalize().unwrap();assert!(old.identifier.is_empty());
        let uid=Settings{remember_identifier:true,identifier:"00000001".into(),..Default::default()}.normalize().unwrap();assert_eq!(uid.identifier,"00000001");
    }
    #[test]
    fn roundtrip_and_forget_identifier() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("settings.json");
        let s = Settings {
            client_directory: "D:\\游戏\\客户端".into(),
            theme: "retro".into(),
            remember_identifier: true,
            identifier: "12345678".into(),
            ..Default::default()
        };
        save(&p, s.clone()).unwrap();
        assert_eq!(load(&p).unwrap(), s);
        save(
            &p,
            Settings {
                remember_identifier: false,
                ..s
            },
        )
        .unwrap();
        assert_eq!(load(&p).unwrap().identifier, "");
        assert!(!fs::read_to_string(p).unwrap().contains("12345678"));
    }
    #[test]
    fn corrupt_file_is_preserved_and_secrets_rejected() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("settings.json");
        fs::write(&p, "broken").unwrap();
        assert_eq!(load(&p).unwrap_err().code, "settings_invalid");
        assert_eq!(fs::read_to_string(p).unwrap(), "broken");
        assert!(serde_json::from_str::<Settings>(r#"{"version":1,"clientDirectory":"","theme":"modern","rememberIdentifier":false,"identifier":"","password":"secret"}"#).is_err());
    }
}
