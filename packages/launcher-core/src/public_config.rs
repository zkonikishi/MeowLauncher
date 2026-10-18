//! Public distribution settings only. Executable hashes and command templates stay compiled in.
use crate::{error, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
pub const FILE_NAME: &str = "launcher.public.json";
#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all="camelCase", deny_unknown_fields)]
pub struct PublicConfig {
    pub schema_version: u32,
    pub brand: String,
    pub account_protocol: String,
    pub account_base_url: String,
    pub website_url: String,
    /// 网页注册/找回入口。登陆器自己就能注册，因此这里可以留空。
    #[serde(default)]
    pub register_url: Option<String>,
    #[serde(default)]
    pub forgot_url: Option<String>,
    pub content_url: String,
    /// 更新清单与发行件的 HTTPS 来源（只填 origin）。通用版靠它获取更新，
    /// 不填就表示服主没有自建更新通道，登陆器会明确说明而不是去连别家。
    #[serde(default)]
    pub update_base_url: Option<String>,
    /// 客户端资源清单与资产前缀。三项要么都填，要么都不填。
    #[serde(default)]
    pub client_manifest_url: Option<String>,
    #[serde(default)]
    pub client_artifact_prefix: Option<String>,
    #[serde(default)]
    pub client_edition: Option<String>,
    pub realm_id: String,
    pub realm_name: String,
    pub adapter_id: String,
    pub game_host: String,
    pub game_port: u16,
    pub client_directory: String,
}
fn invalid() -> crate::AppError { error("public_config_invalid", "发行配置无效，请联系服主重新生成；未连接默认服务器。") }
fn origin_only(value:&str)->Result<()> {
    let url=https_url(value)?;
    if url.path()!="/" {return Err(invalid());}
    Ok(())
}
pub fn https_url(value: &str) -> Result<reqwest::Url> {
    if value.len()>2048 || !value.starts_with("https://") || value.bytes().any(|b| b<=32 || b>=127 || b==b'\\') {return Err(invalid());}
    let u=reqwest::Url::parse(value).map_err(|_|invalid())?;
    if u.host_str().is_none() || u.port()==Some(0) || !u.username().is_empty() || u.password().is_some() || u.fragment().is_some() || u.query().is_some() {return Err(invalid());}
    Ok(u)
}

#[cfg(test)] mod tests {
    use super::*;
    fn fixture()->PublicConfig {PublicConfig::parse(include_bytes!("../fixtures/public-config.json")).unwrap()}
    #[test] fn config_survives_package_move_and_rejects_unknown_secrets() {
        let temp=tempfile::tempdir().unwrap();let old=temp.path().join("发行 包");std::fs::create_dir_all(old.join("客户端 测试")).unwrap();
        std::fs::write(old.join(FILE_NAME),include_bytes!("../fixtures/public-config.json")).unwrap();
        let new=temp.path().join("搬迁 后");std::fs::rename(old,&new).unwrap();
        let config=PublicConfig::load(&new).unwrap().unwrap();assert_eq!(config.client_path(&new).unwrap(),new.join("客户端 测试").canonicalize().unwrap());
        let mut value=serde_json::to_value(fixture()).unwrap();value["password"]=serde_json::json!("secret");assert!(PublicConfig::parse(&serde_json::to_vec(&value).unwrap()).is_err());
        std::fs::write(new.join(FILE_NAME),b"{").unwrap();assert!(PublicConfig::load(&new).is_err());assert!(PublicConfig::load(temp.path()).unwrap().is_none());
    }
    #[test] fn rejects_escape_and_unsafe_endpoints() {
        for path in ["../client","/client","C:/client","Client\\x","a/../x","a//x","a."] {let mut c=fixture();c.client_directory=path.into();assert!(c.validate().is_err(),"{path}");}
        for url in ["http://example.com","https://user@example.com","https://example.com?key=x","https://example.com/#x","https://example.com:0"] {let mut c=fixture();c.account_base_url=url.into();assert!(c.validate().is_err(),"{url}");}
        let mut c=fixture();c.game_host="1.2.3.4?payload".into();assert!(c.validate().is_err());
    }
    #[test] fn browser_account_links_are_optional_but_endpoints_stay_locked() {
        // 登陆器自己做注册与重设，服主没有官网注册页也能用。
        let mut c=fixture();c.register_url=None;c.forgot_url=None;assert!(c.validate().is_ok());
        for url in ["http://example.com","ftp://example.com/register"] {let mut c=fixture();c.register_url=Some(url.into());c.forgot_url=None;assert!(c.validate().is_err(),"{url}");}
        let mut c=fixture();c.forgot_url=Some("https://user@example.com/forgot/".into());assert!(c.validate().is_err());
    }
    #[test] fn update_and_client_sources_are_origin_locked_and_all_or_nothing() {
        let mut c=fixture();c.update_base_url=Some("https://cdn.example.com".into());assert!(c.validate().is_ok());assert_eq!(c.update_base(),Some("https://cdn.example.com"));
        for bad in ["http://cdn.example.com","https://cdn.example.com/","https://cdn.example.com/launcher","https://cdn.example.com?x=1"] {let mut c=fixture();c.update_base_url=Some(bad.into());assert!(c.validate().is_err(),"{bad}");}
        let mut c=fixture();
        c.client_manifest_url=Some("https://cdn.example.com/client/manifest.json".into());
        assert!(c.validate().is_err(),"half-configured client source must be rejected");
        c.client_artifact_prefix=Some("https://cdn.example.com/client/releases/".into());
        c.client_edition=Some("sponsor".into());
        assert!(c.validate().is_ok());
        assert!(c.client_source().unwrap().is_some());
        c.client_artifact_prefix=Some("https://other.example.com/client/releases/".into());
        assert!(c.validate().is_err(),"cross-origin artifact prefix must be rejected");
        c.client_artifact_prefix=Some("https://cdn.example.com/client/releases".into());
        assert!(c.validate().is_err(),"artifact prefix must end with a slash");
        let mut c=fixture();
        c.client_manifest_url=Some("https://cdn.example.com/client/manifest.json?token=x".into());
        c.client_artifact_prefix=Some("https://cdn.example.com/client/releases/".into());
        c.client_edition=Some("sponsor".into());
        assert!(c.validate().is_err());
    }
}
pub fn valid_host(s:&str)->bool {
    !s.is_empty() && s.len()<=253 && s.split('.').all(|p| !p.is_empty() && p.len()<=63 && p.as_bytes()[0].is_ascii_alphanumeric() && p.as_bytes()[p.len()-1].is_ascii_alphanumeric() && p.bytes().all(|b|b.is_ascii_alphanumeric()||b==b'-'))
}
impl PublicConfig {
    pub fn validate(&self)->Result<()> {
        if self.schema_version!=1 || self.account_protocol!="platform-v1" || self.adapter_id!=crate::client::profile().id || self.game_port==0 || !valid_host(&self.game_host) {return Err(invalid());}
        for s in [&self.brand,&self.realm_name] {if s.trim().is_empty() || s.chars().count()>80 || s.chars().any(char::is_control) {return Err(invalid());}}
        if self.realm_id.is_empty() || self.realm_id.len()>32 || !self.realm_id.bytes().all(|b|b.is_ascii_lowercase()||b.is_ascii_digit()||b==b'-') {return Err(invalid());}
        origin_only(&self.account_base_url)?;
        https_url(&self.website_url)?;
        https_url(&self.content_url)?;
        for s in [&self.register_url,&self.forgot_url].into_iter().flatten() {https_url(s)?;}
        // 更新源要拼 `/launcher/…`，尾斜杠会产生 `//`，在配置阶段就拦掉。
        if let Some(s)=&self.update_base_url {origin_only(s)?;if s.ends_with('/'){return Err(invalid());}}
        let client=[self.client_manifest_url.as_ref(),self.client_artifact_prefix.as_ref(),self.client_edition.as_ref()];
        let present=client.iter().filter(|value|value.is_some()).count();
        if present!=0&&present!=3 {return Err(invalid());}
        if present==3 {
            let manifest=https_url(self.client_manifest_url.as_ref().unwrap())?;
            let prefix=self.client_artifact_prefix.as_ref().unwrap();
            let prefix_url=https_url(prefix)?;
            let edition=self.client_edition.as_ref().unwrap();
            if edition.is_empty()||edition.len()>80||!edition.bytes().all(|b|b.is_ascii_alphanumeric()||b==b'-') {return Err(invalid());}
            if manifest.path()!="/"&&!manifest.path().ends_with(".json") {return Err(invalid());}
            if !prefix_url.path().ends_with('/')||manifest.origin()!=prefix_url.origin() {return Err(invalid());}
            // 资源地址是本机信任判断的前缀，必须在本地校验成规范形式，不能等到下载时再判断。
            if prefix_url.as_str()!=prefix {return Err(invalid());}
        }
        let p=&self.client_directory;
        if p!="." && (p.is_empty() || p.len()>512 || p.split('/').any(|c|c.is_empty()||c=="."||c==".."||c.ends_with('.')||c.ends_with(' ')) || p.chars().any(|c|c.is_control()||"\\:*?\"<>|".contains(c))) {return Err(invalid());}
        Ok(())
    }
    pub fn parse(raw:&[u8])->Result<Self> {
        if raw.len()>16384 {return Err(invalid());}
        let value:Self=serde_json::from_slice(raw).map_err(|_|invalid())?;
        value.validate()?;Ok(value)
    }
    pub fn load(directory:&Path)->Result<Option<Self>> {
        let path=directory.join(FILE_NAME);
        match std::fs::metadata(&path) {
            Err(e) if e.kind()==std::io::ErrorKind::NotFound=>Ok(None),
            Ok(m) if m.is_file() && m.len()<=16384=>Self::parse(&std::fs::read(path).map_err(|_|invalid())?).map(Some),
            _=>Err(invalid()),
        }
    }
    pub fn client_path(&self,root:&Path)->Result<PathBuf> {
        self.validate()?;
        let root=root.canonicalize().map_err(|_|invalid())?;
        let path=root.join(&self.client_directory).canonicalize().map_err(|_|invalid())?;
        if !path.is_dir() || !path.starts_with(&root) {return Err(invalid());}
        Ok(path)
    }
    /// 服主自建的更新通道，未配置时返回 None。调用方据此明确提示，而不是回退到别家地址。
    pub fn update_base(&self)->Option<&str> {self.update_base_url.as_deref()}
    /// 已配置的客户端分发源；未配置时返回 None。
    pub fn client_source(&self)->Result<Option<crate::distribution::Source>> {
        let (Some(manifest),Some(prefix),Some(edition))=(self.client_manifest_url.as_ref(),self.client_artifact_prefix.as_ref(),self.client_edition.as_ref()) else {return Ok(None)};
        Ok(Some(crate::distribution::Source::new(edition,self.realm_id.as_str(),self.adapter_id.as_str(),manifest,prefix)?))
    }
}
