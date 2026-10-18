//! Production routing configuration for the custom ("Max") edition.
//!
//! 公开仓库中的本文件只保留不可路由的占位值，不含任何真实服务器端点。
//! 定制版构建会在私有构建环境中注入真实端点（参见私有仓库的 overlays/）。
//! These are configuration targets, not a statement the services are ready.
use crate::{controlled::{Endpoint,Scope},controlled_target,error,Result};
pub const PLATFORM:&str="https://launcher.invalid";
pub const EXCHANGE:&str="https://launcher.invalid/v1/exchange";
pub const DESCRIPTOR:&str="https://launcher.invalid/launcher/custom-target.json";
pub const DIRECTORY_PORT:u16=27001;
pub fn endpoint()->Endpoint{Endpoint{host:"203.0.113.10".into(),port:27002,server_name:"launcher.invalid".into(),ca_certificate:None,embedded_ca_der:None}}
pub fn scope()->Scope{Scope{host_id:"custom-host-01".into(),issuer:"launcher.invalid".into(),audience:"custom-game".into(),realm_id:"hk".into(),version_id:"115us".into()}}
fn failed()->crate::AppError{error("game_provisioning","服务器连接准备中，请稍后重试。")}
pub async fn descriptor()->Result<Vec<u8>>{
 let client=reqwest::Client::builder().user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) MeowLauncherMax/0.7.1").https_only(true).no_proxy().redirect(reqwest::redirect::Policy::none()).timeout(std::time::Duration::from_secs(12)).build().map_err(|_|failed())?;
 let mut response=client.get(DESCRIPTOR).send().await.map_err(|_|failed())?;
 if response.status()!=reqwest::StatusCode::OK||response.content_length().is_some_and(|n|n>65536){return Err(failed());}
 let mut raw=Vec::new();while let Some(chunk)=response.chunk().await.map_err(|_|failed())?{if raw.len()+chunk.len()>65536{return Err(failed());}raw.extend_from_slice(&chunk);}
 let target=endpoint();let mapping=controlled_target::parse(&raw,&scope(),&target.server_name,target.port)?;
 if mapping.directory_port!=DIRECTORY_PORT{return Err(failed());}Ok(raw)
}
#[cfg(test)]mod tests{
 use super::*;
 #[test]fn endpoints_are_placeholders_and_consistent(){let target=endpoint();assert_eq!(target.host,"203.0.113.10");assert_eq!(target.server_name,"launcher.invalid");assert!(target.embedded_ca_der.is_none());assert_eq!(scope().issuer,"launcher.invalid");assert!(DESCRIPTOR.starts_with(PLATFORM));assert!(EXCHANGE.starts_with(PLATFORM));assert!(!PLATFORM.contains("meowopia")&&!PLATFORM.contains("arad"));}
}
