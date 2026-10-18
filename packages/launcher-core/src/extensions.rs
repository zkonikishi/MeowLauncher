//! Declarative UI boundary for future MeowCore integration; not a plugin loader.
use crate::{error,Result};
use serde::{Deserialize,Serialize};
#[derive(Debug,Deserialize,Serialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
pub struct UiExtensions {pub schema_version:u32,pub edition:String,pub version_id:String,pub items:Vec<UiItem>}
#[derive(Debug,Deserialize,Serialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
pub struct UiItem {pub plugin_id:String,pub placement:String,pub kind:String,pub text:String,pub url:Option<String>}
/// Grants come from the trusted host registry, never from the incoming content document.
pub fn parse(raw:&[u8],edition:&str,version_id:&str,origin:&str,granted_plugins:&[&str])->Result<UiExtensions>{
 let invalid=||error("extension_invalid","UI 扩展内容、身份或权限不匹配。");
 if raw.len()>16384{return Err(invalid());}
 let origin=reqwest::Url::parse(origin).map_err(|_|invalid())?;
 if origin.scheme()!="https"||origin.host_str().is_none()||!origin.username().is_empty()||origin.password().is_some()||origin.path()!="/"||origin.query().is_some()||origin.fragment().is_some(){return Err(invalid());}
 let content:UiExtensions=serde_json::from_slice(raw).map_err(|_|invalid())?;
 if content.schema_version!=1||content.edition!=edition||content.version_id!=version_id||content.items.len()>12{return Err(invalid());}
 for item in &content.items {
  if !granted_plugins.contains(&item.plugin_id.as_str())||item.plugin_id.is_empty()||item.plugin_id.len()>64||!item.plugin_id.bytes().all(|b|b.is_ascii_alphanumeric()||b"-_.+".contains(&b))||!["launcher.login","launcher.lobby","manager.overview"].contains(&item.placement.as_str())||!["text","link","image"].contains(&item.kind.as_str())||item.text.trim().is_empty()||item.text.chars().count()>500||item.text.chars().any(|c|c.is_control()){return Err(invalid());}
  if item.kind=="text"&&item.url.is_some()||item.kind!="text"&&item.url.is_none(){return Err(invalid());}
  if let Some(url)=&item.url {
   if url.len()>2048||url.chars().any(|c|c.is_control())||url.contains('\\'){return Err(invalid());}
   let parsed=reqwest::Url::parse(url).map_err(|_|invalid())?;
   if parsed.origin()!=origin.origin()||!parsed.username().is_empty()||parsed.password().is_some()||parsed.fragment().is_some(){return Err(invalid());}
  }
 }
 Ok(content)
}
#[cfg(test)] mod tests {
 use super::*;
 fn value()->serde_json::Value{serde_json::json!({"schemaVersion":1,"edition":"public","versionId":"86s4a21","items":[{"pluginId":"meowlink+","placement":"launcher.lobby","kind":"link","text":"服务器活动","url":"https://owner.example.invalid/news/"}]})}
 fn check(v:serde_json::Value)->Result<UiExtensions>{parse(&serde_json::to_vec(&v).unwrap(),"public","86s4a21","https://owner.example.invalid/",&["meowlink+"])}
 #[test]fn respects_host_identity_and_grants(){assert!(check(value()).is_ok());for (field,bad) in [("edition","custom"),("versionId","95s5a9")]{let mut v=value();v[field]=bad.into();assert!(check(v).is_err());}assert!(parse(&serde_json::to_vec(&value()).unwrap(),"public","86s4a21","https://owner.example.invalid/",&[]).is_err());}
 #[test]fn rejects_code_and_external_content(){for url in ["javascript:alert(1)","https://evil.invalid/","file:///C:/x"]{let mut v=value();v["items"][0]["url"]=url.into();assert!(check(v).is_err());}let mut v=value();v["items"][0]["script"]="execute".into();assert!(check(v).is_err());let mut v=value();v["items"][0]["placement"]="game.client".into();assert!(check(v).is_err());}
}
