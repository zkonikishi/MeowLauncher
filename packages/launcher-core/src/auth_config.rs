//! Public account integration configuration. Provider selection never bypasses
//! the target gateway. Cloud/OAuth are reserved interfaces, not implemented wire.
use crate::{error,generic::Target,Result};
use serde::{Deserialize,Serialize};
use std::collections::HashSet;
#[derive(Clone,Deserialize,Serialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
pub struct AuthConfig {pub schema_version:u32,pub active_provider:String,pub providers:Vec<Provider>}
#[derive(Clone,Deserialize,Serialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
pub struct Provider {pub id:String,pub label:String,pub connection:Connection}
#[derive(Clone,Deserialize,Serialize)]
#[serde(tag="kind",rename_all="kebab-case",deny_unknown_fields)]
pub enum Connection {
    LocalGateway {target:Target},
    CloudAccount {#[serde(rename="baseUrl")]base_url:String,#[serde(rename="gatewayProviderId")]gateway_provider_id:String,methods:Vec<Method>},
}
#[derive(Clone,Copy,Deserialize,Serialize,PartialEq,Eq,Hash)]
#[serde(rename_all="kebab-case")]
pub enum Method {UsernamePassword,EmailCode,PhoneCode,ThirdParty}
fn invalid()->crate::AppError {error("auth_config_invalid","账号接入配置无效；不会切换到默认账号服务。")}
impl AuthConfig {
 pub fn parse(raw:&[u8])->Result<Self>{if raw.len()>32768{return Err(invalid());}let c:Self=serde_json::from_slice(raw).map_err(|_|invalid())?;c.validate()?;Ok(c)}
 pub fn validate(&self)->Result<()> {
    if self.schema_version!=1||self.providers.is_empty()||self.providers.len()>16{return Err(invalid());}
    let mut ids=HashSet::new();
    for p in &self.providers {
        if p.id.is_empty()||p.id.len()>64||!p.id.bytes().all(|b|b.is_ascii_alphanumeric()||b==b'-')||!ids.insert(&p.id)||p.label.trim().is_empty()||p.label.len()>128||p.label.chars().any(char::is_control){return Err(invalid());}
        match &p.connection {
            Connection::LocalGateway{target}=>target.validate()?,
            Connection::CloudAccount{base_url,methods,..}=>{let u=crate::public_config::https_url(base_url)?;if u.path()!="/"||methods.is_empty()||methods.iter().collect::<HashSet<_>>().len()!=methods.len(){return Err(invalid());}}
        }
    }
    if !ids.contains(&self.active_provider){return Err(invalid());}
    for p in &self.providers {if let Connection::CloudAccount{gateway_provider_id,..}=&p.connection {
        if !self.providers.iter().any(|g|&g.id==gateway_provider_id&&matches!(g.connection,Connection::LocalGateway{..})){return Err(invalid());}
    }}
    Ok(())
 }
 pub fn gateway(&self)->Result<Target>{
    let p=self.selected()?;match &p.connection {Connection::LocalGateway{target}=>Ok(target.clone()),Connection::CloudAccount{gateway_provider_id,..}=>self.providers.iter().find_map(|g|{if &g.id==gateway_provider_id{if let Connection::LocalGateway{target}=&g.connection{return Some(target.clone());}}None}).ok_or_else(invalid)}
 }
 pub fn load(directory:&std::path::Path)->Result<Option<Self>>{
    let path=directory.join("auth-providers.json");match std::fs::symlink_metadata(&path){Err(e)if e.kind()==std::io::ErrorKind::NotFound=>Ok(None),Ok(m)if m.is_file()&&!m.file_type().is_symlink()&&m.len()<=32768=>Self::parse(&std::fs::read(path).map_err(|_|invalid())?).map(Some),_=>Err(invalid())}
 }
 pub fn save(&self,directory:&std::path::Path)->Result<()>{
    use std::io::Write;self.validate()?;let bytes=serde_json::to_vec_pretty(self).map_err(|_|invalid())?;if bytes.len()>32768{return Err(invalid());}std::fs::create_dir_all(directory).map_err(|_|invalid())?;
    let mut temp=tempfile::NamedTempFile::new_in(directory).map_err(|_|invalid())?;temp.write_all(&bytes).and_then(|_|temp.as_file().sync_all()).map_err(|_|invalid())?;temp.persist(directory.join("auth-providers.json")).map_err(|_|invalid())?;Ok(())
 }
 pub fn selected(&self)->Result<&Provider>{self.validate()?;self.providers.iter().find(|p|p.id==self.active_provider).ok_or_else(invalid)}
}
#[cfg(test)]mod tests {
 use super::*;
 fn sample()->serde_json::Value {serde_json::json!({"schemaVersion":1,"activeProvider":"cloud","providers":[{"id":"gateway","label":"服主网关","connection":{"kind":"local-gateway","target":{"host":"127.0.0.1","port":27002,"serverName":"gateway.example.com","authorityId":"owner","realmId":"realm","saveProfileId":"save"}}},{"id":"cloud","label":"云端账号","connection":{"kind":"cloud-account","baseUrl":"https://accounts.example.com","gatewayProviderId":"gateway","methods":["username-password","email-code","phone-code","third-party"]}}]})}
 fn parse(v:&serde_json::Value)->Result<AuthConfig>{AuthConfig::parse(&serde_json::to_vec(v).unwrap())}
 #[test]fn explicit_selection_and_gateway_binding(){let c=parse(&sample()).unwrap();assert_eq!(c.selected().unwrap().id,"cloud");}
 #[test]fn no_missing_or_duplicate_provider(){let mut v=sample();v["activeProvider"]="missing".into();assert!(parse(&v).is_err());let mut v=sample();v["providers"][1]["id"]="gateway".into();assert!(parse(&v).is_err());}
 #[test]fn cloud_cannot_skip_gateway(){let mut v=sample();v["providers"][1]["connection"]["gatewayProviderId"]="cloud".into();assert!(parse(&v).is_err());}
 #[test]fn no_private_secrets_in_public_config(){for field in ["clientSecret","databasePassword","signingPrivateKey","sshPassword"]{let mut v=sample();v["providers"][1][field]="SECRET".into();assert!(parse(&v).is_err(),"{field}");}}
 #[test]fn no_plaintext_or_url_credential(){for url in ["http://accounts.example.com","https://user:pass@accounts.example.com","https://accounts.example.com?token=x","https://accounts.example.com/api"]{let mut v=sample();v["providers"][1]["connection"]["baseUrl"]=url.into();assert!(parse(&v).is_err());}}
}
