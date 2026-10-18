//! Native GUI-facing state machine. Tokens/challenges stay in this module;
//! preparing bridges is NOT strict client readiness or a game-start success.
use crate::{controlled::{Endpoint,Scope},controlled_bridges,controlled_exchange::ExchangeClient,controlled_target,platform_identity::{self,IdentifierType},error,Result};
use serde::Serialize;
use std::{sync::Arc,time::{Duration,Instant}};
use tokio::sync::Mutex;
use zeroize::Zeroizing;
#[derive(Clone,Serialize)]#[serde(rename_all="camelCase")]
pub struct AccountView{pub uid:String,pub nickname:String,pub game_ready:bool}
pub struct Coordinator{client:platform_identity::Client,endpoint:Endpoint,scope:Scope,exchange:Arc<ExchangeClient>,mapping:Vec<u8>,session:Mutex<Option<Arc<platform_identity::Session>>>,challenge:Mutex<Option<PendingCode>>,operation:Mutex<()>}
struct PendingCode{email:String,id:Zeroizing<String>,deadline:Instant}
pub struct Prepared{pub bridges:controlled_bridges::RunningBridges}
impl Coordinator{
 pub fn new(client:platform_identity::Client,endpoint:Endpoint,scope:Scope,exchange:Arc<ExchangeClient>,mapping:Vec<u8>)->Result<Self>{
  endpoint.validate()?;controlled_target::parse(&mapping,&scope,&endpoint.server_name,endpoint.port)?;
  Ok(Self{client,endpoint,scope,exchange,mapping,session:Mutex::new(None),challenge:Mutex::new(None),operation:Mutex::new(())})
 }
 /// IPC callers must clear any displayed old account before awaiting login.
 /// Typed ID/UID is forwarded unchanged, never locally resolved by nickname.
 pub async fn login(&self,kind:IdentifierType,identifier:&str,password:Zeroizing<String>)->Result<AccountView>{
  let _operation=self.operation.lock().await;self.session.lock().await.take();
  let session=self.client.login(kind,identifier,password).await?;let view=AccountView{uid:session.uid.clone(),nickname:session.nickname.clone(),game_ready:session.game_ready};*self.session.lock().await=Some(Arc::new(session));Ok(view)
 }
 pub async fn registration_code(&self,email:&str)->Result<u64>{
  let _operation=self.operation.lock().await;let email=crate::account::validate_email(email)?;let sent=self.client.registration_code(&email).await?;let retry=sent.retry_after;
  *self.challenge.lock().await=Some(PendingCode{email,id:sent.id,deadline:Instant::now()+Duration::from_secs(300)});Ok(retry)
 }
 pub async fn register(&self,id:&str,nickname:Option<&str>,email:&str,code:&str,password:Zeroizing<String>)->Result<String>{
  let _operation=self.operation.lock().await;let email=crate::account::validate_email(email)?;
  let challenge=self.challenge.lock().await;let pending=challenge.as_ref().filter(|p|p.email==email&&p.deadline>Instant::now()).ok_or_else(||error("code_not_requested","请先获取当前邮箱的验证码。"))?;
  let result=self.client.register(id,nickname,&email,code,password,&pending.id).await;drop(challenge);
  if result.is_ok(){self.challenge.lock().await.take();}result
 }
 pub async fn logout(&self)->Result<()>{let _operation=self.operation.lock().await;let session=self.session.lock().await.take();self.challenge.lock().await.take();if let Some(session)=session{self.client.logout(&session).await?;}Ok(())}
 pub async fn adopt_session(&self,session:platform_identity::Session)->Result<()>{let _operation=self.operation.lock().await;if session.valid_for().as_secs()<=5{return Err(error("login_required","登录已失效，请重新登录。"));}*self.session.lock().await=Some(Arc::new(session));Ok(())}
 pub async fn prepare_game(&self)->Result<Prepared>{self.prepare_game_logged(None).await}
 pub async fn prepare_game_logged(&self,diagnostics:Option<std::path::PathBuf>)->Result<Prepared>{
  let _operation=self.operation.lock().await;let session=self.session.lock().await.clone().ok_or_else(||error("login_required","请先登录。"))?;
  if session.valid_for().as_secs()<=5{self.session.lock().await.take();return Err(error("login_required","登录已失效，请重新登录。"));}
  if !session.game_ready{return Err(error("game_provisioning","游戏账号正在开通，请稍后重试。"));}
  let account=match controlled_bridges::Account::new(self.endpoint.clone(),self.scope.clone(),self.exchange.clone(),session.token.clone(),session.valid_for()).await{Ok(account)=>account,Err(e)=>{if e.code=="login_required"{self.session.lock().await.take();}return Err(e);}};
  let mapping=controlled_target::parse(&self.mapping,&self.scope,&self.endpoint.server_name,self.endpoint.port)?;
  let bridges=controlled_bridges::RunningBridges::start_logged(account,mapping,diagnostics).await?;Ok(Prepared{bridges})
 }
}
#[cfg(test)]mod tests{
 use super::*;
 fn coordinator()->Coordinator{
  let scope=Scope{host_id:"host".into(),issuer:"issuer".into(),audience:"game".into(),realm_id:"realm".into(),version_id:"115us".into()};
  let endpoint=Endpoint{host:"server.example.invalid".into(),port:30000,server_name:"server.example.invalid".into(),ca_certificate:None,embedded_ca_der:None};
  let routes:Vec<_>=(0..51).map(|i|serde_json::json!({"channelId":i+1,"listenAddress":format!("0.0.0.0:{}",30000+i),"advertiseAddress":format!("127.0.0.4:{}",30000+i)})).collect();
  let raw=serde_json::to_vec(&serde_json::json!({"schema":"meow115-custom-host-target-v1","mode":"custom","identityScope":scope,"gameTransport":"tls1.3-controlled-v1","serverName":"server.example.invalid","certificateSha256":"a".repeat(64),"routes":routes,"directory":{"address":"0.0.0.0:30051","port":30051,"transport":"native-tls1.3-v1"}})).unwrap();
  Coordinator::new(platform_identity::Client::new("https://account.example.invalid").unwrap(),endpoint,scope.clone(),Arc::new(ExchangeClient::new("https://account.example.invalid/v1/exchange",scope,None).unwrap()),raw).unwrap()
 }
 #[tokio::test]async fn unauthenticated_prepare_and_unrequested_registration_never_connect(){let c=coordinator();assert_eq!(c.prepare_game().await.err().unwrap().code,"login_required");assert_eq!(c.register("Ab1234",None,"test@example.invalid","123456",Zeroizing::new("fixture-only".into())).await.err().unwrap().code,"code_not_requested");assert!(c.logout().await.is_ok());}
}
