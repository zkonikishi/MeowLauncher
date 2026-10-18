//! Strict Max descriptor parser; descriptor never supplies trust or identity.
use crate::{controlled::Scope,error,Result};
use serde::Deserialize;
use sha2::{Digest,Sha256};
use std::{collections::HashSet,net::{IpAddr,Ipv4Addr,SocketAddr}};
#[derive(Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
struct Artifact{schema:String,mode:String,identity_scope:Scope,game_transport:String,server_name:String,certificate_sha256:String,routes:Vec<Route>,directory:Directory}
#[derive(Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
pub struct Route{pub channel_id:u32,pub listen_address:String,pub advertise_address:String}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Directory{address:String,port:u16,transport:String}
pub struct Mapping{pub routes:Vec<Route>,pub directory_port:u16,pub artifact_sha256:String}
fn invalid()->crate::AppError{error("controlled_target","平台服务器频道配置与可信目标不一致。")}
pub fn parse(raw:&[u8],trusted_scope:&Scope,trusted_server_name:&str,primary_port:u16)->Result<Mapping>{
 trusted_scope.validate()?;
 if raw.len()>65536||trusted_server_name.is_empty()||primary_port==0{return Err(invalid());}
 let a:Artifact=serde_json::from_slice(raw).map_err(|_|invalid())?;
 if a.schema!="meow115-custom-host-target-v1"||a.mode!="custom"||a.game_transport!="tls1.3-controlled-v1"||a.identity_scope!=*trusted_scope||a.server_name!=trusted_server_name||a.routes.len()!=51||a.certificate_sha256.len()!=64||!a.certificate_sha256.bytes().all(|b|b.is_ascii_hexdigit()){return Err(invalid());}
 let mut ids=HashSet::new();let mut ports=HashSet::new();
 for route in &a.routes{
  let listen:SocketAddr=route.listen_address.parse().map_err(|_|invalid())?;let advertise:SocketAddr=route.advertise_address.parse().map_err(|_|invalid())?;
  if listen.port()==0||listen.port()==7002||listen.port()!=advertise.port()||advertise.ip()!=IpAddr::V4(Ipv4Addr::new(127,0,0,4))||!ids.insert(route.channel_id)||!ports.insert(listen.port()){return Err(invalid());}
 }
 let first:SocketAddr=a.routes[0].listen_address.parse().map_err(|_|invalid())?;let directory:SocketAddr=a.directory.address.parse().map_err(|_|invalid())?;
 if first.port()!=primary_port||a.directory.transport!="native-tls1.3-v1"||a.directory.port==0||a.directory.port==7002||directory.port()!=a.directory.port||ports.contains(&a.directory.port){return Err(invalid());}
 Ok(Mapping{routes:a.routes,directory_port:a.directory.port,artifact_sha256:format!("{:x}",Sha256::digest(raw))})
}
#[cfg(test)]mod tests{
 use super::*;
 fn fixture()->(serde_json::Value,Scope){let s=Scope{host_id:"host".into(),issuer:"platform".into(),audience:"game".into(),realm_id:"realm".into(),version_id:"115us".into()};let mut v:serde_json::Value=serde_json::from_slice(include_bytes!("../fixtures/generic-local-target-stopped.json")).unwrap();v["schema"]="meow115-custom-host-target-v1".into();v["mode"]="custom".into();v["gameTransport"]="tls1.3-controlled-v1".into();for key in ["authorityId","realmId","saveProfileId"]{v.as_object_mut().unwrap().remove(key);}v["identityScope"]=serde_json::to_value(&s).unwrap();v["directory"]["transport"]="native-tls1.3-v1".into();v["directory"]["port"]=61921.into();(v,s)}
 #[test]fn distinct_custom_mapping_keeps_order_and_requires_independent_scope(){let(v,s)=fixture();let m=parse(&serde_json::to_vec(&v).unwrap(),&s,"127.0.0.1",61869).unwrap();assert_eq!(m.routes[1].channel_id,6);assert_eq!(m.directory_port,61921);let mut other=s.clone();other.realm_id="other".into();assert!(parse(&serde_json::to_vec(&v).unwrap(),&other,"127.0.0.1",61869).is_err());}
 #[test]fn simple_plaintext_and_identity_injection_rejected(){let(v,s)=fixture();for (key,value) in [("mode","simple"),("gameTransport","tls1.3-simple-v1"),("uid","10000001"),("saveProfileId","native"),("caCertificate","untrusted")]{let mut bad=v.clone();bad[key]=value.into();assert!(parse(&serde_json::to_vec(&bad).unwrap(),&s,"127.0.0.1",61869).is_err());}let mut bad=v;bad["directory"]["transport"]="native-loopback-tcp-v1".into();assert!(parse(&serde_json::to_vec(&bad).unwrap(),&s,"127.0.0.1",61869).is_err());}
}
