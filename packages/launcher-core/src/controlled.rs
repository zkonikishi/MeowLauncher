//! Max's controlled host gate. A website-session exchange callback supplies a
//! one-use assertion; no password/simple-mode fallback or native ID inference.
use crate::{error,Result};
use serde::{Deserialize,Serialize,de::DeserializeOwned};
use std::{future::Future,time::Duration};
use tokio::io::{AsyncRead,AsyncReadExt,AsyncWrite,AsyncWriteExt};
use zeroize::Zeroizing;

/// Independently provisioned transport trust, never read from a host challenge
/// or from a descriptor's diagnostic certificate digest.
#[derive(Clone,PartialEq,Eq,Serialize,Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
pub struct Endpoint{pub host:String,pub port:u16,pub server_name:String,#[serde(default)]pub ca_certificate:Option<String>,#[serde(skip)]pub embedded_ca_der:Option<Vec<u8>>}
impl Endpoint{
 pub fn validate(&self)->Result<()>{
  if self.port==0||!crate::public_config::valid_host(&self.host)||rustls::pki_types::ServerName::try_from(self.server_name.clone()).is_err(){return Err(rejected());}
  if self.ca_certificate.as_ref().is_some_and(|p|p.len()>4096||!std::path::Path::new(p).is_absolute()){return Err(rejected());}Ok(())
 }
 pub async fn connect(&self)->Result<tokio_rustls::client::TlsStream<tokio::net::TcpStream>>{
  self.validate()?;
  let mut roots=rustls::RootCertStore::from_iter(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
  if let Some(raw)=&self.embedded_ca_der{if raw.is_empty()||raw.len()>65536{return Err(rejected());}roots.add(rustls::pki_types::CertificateDer::from(raw.clone())).map_err(|_|rejected())?;}
  if let Some(path)=&self.ca_certificate{let file=std::fs::File::open(path).map_err(|_|rejected())?;let mut raw=Vec::new();std::io::Read::read_to_end(&mut std::io::Read::take(file,65537),&mut raw).map_err(|_|rejected())?;if raw.is_empty()||raw.len()>65536{return Err(rejected());}roots.add(rustls::pki_types::CertificateDer::from(raw)).map_err(|_|rejected())?;}
  let config=rustls::ClientConfig::builder_with_provider(std::sync::Arc::new(rustls::crypto::ring::default_provider())).with_protocol_versions(&[&rustls::version::TLS13]).map_err(|_|rejected())?.with_root_certificates(roots).with_no_client_auth();
  tokio::time::timeout(Duration::from_secs(10),async{let stream=tokio::net::TcpStream::connect((self.host.as_str(),self.port)).await.map_err(|_|rejected())?;let name=rustls::pki_types::ServerName::try_from(self.server_name.clone()).map_err(|_|rejected())?;tokio_rustls::TlsConnector::from(std::sync::Arc::new(config)).connect(name,stream).await.map_err(|_|rejected())}).await.map_err(|_|rejected())?
 }
}

const MAGIC:&[u8]=b"MEOW115/1\n";
const LIMIT:usize=16384;
fn rejected()->crate::AppError{error("controlled_rejected","平台游戏准入未完成，请重新登录后重试。")}
#[derive(Clone,PartialEq,Eq,Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Scope{pub host_id:String,pub issuer:String,pub audience:String,pub realm_id:String,pub version_id:String}
#[derive(Clone,Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Challenge{pub host_id:String,pub issuer:String,pub audience:String,pub realm_id:String,pub version_id:String,pub connection_id:String,pub nonce:String,pub proof:String}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope{r#type:String,challenge:Challenge}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Ready{r#type:String,connection_id:String,version_id:String,ready:bool}
#[derive(Serialize)]
struct Assertion<'a>{r#type:&'static str,assertion:&'a str}
fn field(s:&str)->bool{!s.is_empty()&&s.len()<=256&&!s.chars().any(char::is_control)}
impl Scope{
 pub fn validate(&self)->Result<()>{if self.version_id!="115us"||![&self.host_id,&self.issuer,&self.audience,&self.realm_id].into_iter().all(|s|field(s)){return Err(rejected());}Ok(())}
 pub(crate) fn accepts(&self,c:&Challenge)->bool{c.host_id==self.host_id&&c.issuer==self.issuer&&c.audience==self.audience&&c.realm_id==self.realm_id&&c.version_id==self.version_id&&field(&c.connection_id)&&c.nonce.len()==64&&c.nonce.bytes().all(|b|b.is_ascii_digit()||(b'a'..=b'f').contains(&b))&&!c.proof.is_empty()&&c.proof.len()<=LIMIT}
}
async fn frame<S:AsyncRead+Unpin,T:DeserializeOwned>(s:&mut S)->Result<T>{
 let n=s.read_u32().await.map_err(|_|rejected())? as usize;
 if n==0||n>LIMIT{return Err(rejected());}
 let mut raw=Zeroizing::new(vec![0;n]);s.read_exact(&mut raw).await.map_err(|_|rejected())?;
 serde_json::from_slice(&raw).map_err(|_|rejected())
}
/// Caller must supply an already CA+hostname-verified TLS1.3 stream. This
/// protocol gate deliberately does not choose endpoint trust or website URLs.
/// Exchange receives the complete validated socket challenge, not a UID.
async fn authenticate<S,F,Fut>(stream:&mut S,scope:&Scope,exchange:F)->Result<String>
where S:AsyncRead+AsyncWrite+Unpin,F:FnOnce(Challenge)->Fut,Fut:Future<Output=Result<Zeroizing<String>>>{
 scope.validate()?;
 tokio::time::timeout(Duration::from_secs(40),async{
  let mut magic=[0;10];stream.read_exact(&mut magic).await.map_err(|_|rejected())?;
  if magic.as_slice()!=MAGIC{return Err(rejected());}
  let envelope:Envelope=frame(stream).await?;
  if envelope.r#type!="host-challenge-v1"||!scope.accepts(&envelope.challenge){return Err(rejected());}
  let connection=envelope.challenge.connection_id.clone();
  let assertion=exchange(envelope.challenge).await?;
  if assertion.len()!=46||!assertion.starts_with("a1.")||!assertion.as_bytes()[3..].iter().all(|b|b.is_ascii_alphanumeric()||*b==b'-'||*b==b'_'){return Err(rejected());}
  let raw=Zeroizing::new(serde_json::to_vec(&Assertion{r#type:"native-assertion-v1",assertion:&assertion}).map_err(|_|rejected())?);
  stream.write_u32(raw.len() as u32).await.map_err(|_|rejected())?;stream.write_all(&raw).await.map_err(|_|rejected())?;stream.flush().await.map_err(|_|rejected())?;drop(raw);drop(assertion);
  let ready:Ready=frame(stream).await?;
  if ready.r#type!="native-ready-v1"||ready.connection_id!=connection||ready.version_id!=scope.version_id||!ready.ready{return Err(rejected());}
  Ok(connection)
 }).await.map_err(|_|rejected())?
}

/// Public gate accepts only a TLS1.3 client stream. Endpoint/CA construction
/// remains the trusted configuration owner's responsibility, never descriptor
/// certificateSha256 or the dynamic challenge's responsibility.
pub async fn authenticate_tls<F,Fut>(stream:&mut tokio_rustls::client::TlsStream<tokio::net::TcpStream>,scope:&Scope,exchange:F)->Result<String>
where F:FnOnce(Challenge)->Fut,Fut:Future<Output=Result<Zeroizing<String>>>{
 if stream.get_ref().1.protocol_version()!=Some(rustls::ProtocolVersion::TLSv1_3){return Err(rejected());}
 authenticate(stream,scope,exchange).await
}

#[cfg(test)]
mod tests{
 use super::*;
 use serde_json::json;
 fn scope()->Scope{Scope{host_id:"host".into(),issuer:"issuer".into(),audience:"aud".into(),realm_id:"realm".into(),version_id:"115us".into()}}
 fn challenge()->serde_json::Value{json!({"type":"host-challenge-v1","challenge":{"host_id":"host","issuer":"issuer","audience":"aud","realm_id":"realm","version_id":"115us","connection_id":"socket","nonce":"a".repeat(64),"proof":"opaque-proof"}})}
 async fn send(s:&mut tokio::io::DuplexStream,v:serde_json::Value){let raw=serde_json::to_vec(&v).unwrap();s.write_u32(raw.len() as u32).await.unwrap();s.write_all(&raw).await.unwrap();}
 #[tokio::test]async fn same_stream_ready_preserves_native_and_full_challenge(){
  let(mut c,mut s)=tokio::io::duplex(32768);
  let peer=tokio::spawn(async move{s.write_all(MAGIC).await.unwrap();send(&mut s,challenge()).await;let request:serde_json::Value=frame(&mut s).await.unwrap();assert_eq!(request["type"],"native-assertion-v1");assert_eq!(request["assertion"],format!("a1.{}","x".repeat(43)));send(&mut s,json!({"type":"native-ready-v1","connection_id":"socket","version_id":"115us","ready":true})).await;s.write_all(b"native").await.unwrap();});
  let id=authenticate(&mut c,&scope(),|ch|async move{assert_eq!(ch.proof,"opaque-proof");assert_eq!(ch.connection_id,"socket");Ok(Zeroizing::new(format!("a1.{}","x".repeat(43))))}).await.unwrap();assert_eq!(id,"socket");let mut bytes=[0;6];c.read_exact(&mut bytes).await.unwrap();assert_eq!(&bytes,b"native");peer.await.unwrap();
 }
 #[tokio::test]async fn foreign_scope_rejected_before_exchange(){
  for key in ["host_id","issuer","audience","realm_id","version_id","nonce"]{let(mut c,mut s)=tokio::io::duplex(32768);let mut value=challenge();value["challenge"][key]="foreign".into();s.write_all(MAGIC).await.unwrap();send(&mut s,value).await;assert!(authenticate(&mut c,&scope(),|_|async{panic!("untrusted challenge must not reach website session");#[allow(unreachable_code)]Ok(Zeroizing::new(String::new()))}).await.is_err());}
 }
 #[tokio::test]async fn no_simple_fallback_duplicate_fields_or_wrong_ready(){
  let(mut c,mut s)=tokio::io::duplex(32768);s.write_all(b"MEOWSIMPLE/1\n").await.unwrap();assert!(authenticate(&mut c,&scope(),|_|async{Ok(Zeroizing::new(String::new()))}).await.is_err());
  let(mut c,mut s)=tokio::io::duplex(32768);s.write_all(MAGIC).await.unwrap();send(&mut s,challenge()).await;
  let peer=tokio::spawn(async move{let _:serde_json::Value=frame(&mut s).await.unwrap();send(&mut s,json!({"type":"native-ready-v1","connection_id":"other","version_id":"115us","ready":true})).await;});
  assert!(authenticate(&mut c,&scope(),|_|async{Ok(Zeroizing::new(format!("a1.{}","x".repeat(43))))}).await.is_err());peer.await.unwrap();
  let(mut c,mut s)=tokio::io::duplex(32768);let raw=br#"{"type":"a","type":"b"}"#;s.write_u32(raw.len() as u32).await.unwrap();s.write_all(raw).await.unwrap();assert!(frame::<_,Envelope>(&mut c).await.is_err());
 }
}
