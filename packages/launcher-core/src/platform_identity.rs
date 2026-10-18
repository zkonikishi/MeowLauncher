//! Current platform login-ID contract. Username is a public nickname, never a
//! login selector. Native account keys/mappings are owned by the service.
use crate::{error,Result};
use serde::{Deserialize,Serialize};
use std::time::{Duration,Instant};
use zeroize::Zeroizing;
fn failed()->crate::AppError{error("platform_unavailable","账号服务暂不可用，请稍后重试。")}
#[derive(Clone,Copy,Serialize)]#[serde(rename_all="lowercase")]
pub enum IdentifierType{Auto,Id,Uid,Email}
pub fn login_id(s:&str)->bool{(4..=32).contains(&s.len())&&s.bytes().all(|b|b.is_ascii_alphanumeric())}
pub fn uid(s:&str)->bool{s.len()==8&&s!="00000000"&&s.bytes().all(|b|b.is_ascii_digit())}
pub fn nickname(s:&str)->bool{!s.trim().is_empty()&&s.chars().count()<=64&&!s.chars().any(char::is_control)}
pub fn validate_identifier(kind:IdentifierType,s:&str)->Result<()>{
 let valid=match kind{IdentifierType::Id=>login_id(s),IdentifierType::Uid=>uid(s),IdentifierType::Email=>crate::account::validate_email(s).is_ok(),IdentifierType::Auto=>login_id(s)||uid(s)||crate::account::validate_email(s).is_ok()};
 if !valid{return Err(error("invalid_identifier","请输入有效的登录 ID、UID 或邮箱。"));}Ok(())
}
#[derive(Clone)]
pub struct Session{pub uid:String,pub nickname:String,pub game_ready:bool,pub token:Zeroizing<String>,deadline:Instant}
impl Session{pub fn valid_for(&self)->Duration{self.deadline.saturating_duration_since(Instant::now())}}
#[derive(Deserialize)]#[serde(deny_unknown_fields)]
struct LoginResponse{ok:bool,uid:String,username:String,#[serde(rename="sessionToken")]token:String,expires_in:u64,#[serde(rename="gameReady")]game_ready:bool,protocol:String}
impl Drop for LoginResponse{fn drop(&mut self){use zeroize::Zeroize;self.token.zeroize();}}
fn parse_login(raw:&[u8])->Result<Session>{
 let mut r:LoginResponse=serde_json::from_slice(raw).map_err(|_|failed())?;
 if !r.ok||!uid(&r.uid)||!nickname(&r.username)||r.protocol!="platform-session-v1"||r.token.len()!=64||!r.token.bytes().all(|b|b.is_ascii_digit()||(b'a'..=b'f').contains(&b))||r.expires_in<=5||r.expires_in>86400{return Err(failed());}
 let deadline=Instant::now().checked_add(Duration::from_secs(r.expires_in)).ok_or_else(failed)?;
 Ok(Session{uid:r.uid.clone(),nickname:r.username.clone(),game_ready:r.game_ready,token:Zeroizing::new(std::mem::take(&mut r.token)),deadline})
}
#[derive(Clone)]
pub struct Client{http:reqwest::Client,base:String}
pub struct EmailChallenge{pub id:Zeroizing<String>,pub retry_after:u64}
impl Client{
 pub fn new(origin:&str)->Result<Self>{let url=crate::public_config::https_url(origin)?;if url.path()!="/"{return Err(failed());}let http=reqwest::Client::builder().user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) MeowLauncherMax/0.7.1").https_only(true).no_proxy().redirect(reqwest::redirect::Policy::none()).timeout(Duration::from_secs(12)).build().map_err(|_|failed())?;Ok(Self{http,base:origin.trim_end_matches('/').into()})}
 pub async fn available(&self)->Result<()>{let response=self.http.get(format!("{}/api/account/status",self.base)).send().await.map_err(|_|failed())?;let status=response.status();let raw=body(response).await?;if status!=reqwest::StatusCode::OK{return Err(service_error(status,&raw));}let value:serde_json::Value=serde_json::from_slice(&raw).map_err(|_|failed())?;if value.get("ok").and_then(|v|v.as_bool())!=Some(true){return Err(failed());}Ok(())}
 pub async fn registration_code(&self,email:&str)->Result<EmailChallenge>{
  let email=crate::account::validate_email(email)?;
  let response=self.http.post(format!("{}/api/account/sendCode",self.base)).json(&serde_json::json!({"email":email,"purpose":"register"})).send().await.map_err(|_|failed())?;let status=response.status();let raw=body(response).await?;
  if status!=reqwest::StatusCode::ACCEPTED{return Err(service_error(status,&raw));}
  #[derive(Deserialize)]#[serde(deny_unknown_fields)]struct Sent{ok:bool,challenge_id:String,retry_after:u64,delivery:String}
  let r:Sent=serde_json::from_slice(&raw).map_err(|_|failed())?;if !r.ok||r.challenge_id.is_empty()||r.challenge_id.len()>256||r.challenge_id.chars().any(char::is_control)||r.retry_after>3600||r.delivery!="queued"{return Err(failed());}Ok(EmailChallenge{id:Zeroizing::new(r.challenge_id),retry_after:r.retry_after})
 }
 pub async fn login(&self,kind:IdentifierType,identifier:&str,password:Zeroizing<String>)->Result<Session>{
  validate_identifier(kind,identifier)?;if password.chars().count()<6||password.chars().count()>128||password.chars().any(char::is_control){return Err(error("invalid_password","请输入 6–128 位密码。"));}
  #[derive(Serialize)]struct Request<'a>{identifier_type:IdentifierType,identifier:&'a str,password:&'a str,intent:&'static str}
  let response=self.http.post(format!("{}/api/account/login",self.base)).json(&Request{identifier_type:kind,identifier,password:&password,intent:"platform"}).send().await.map_err(|_|failed())?;drop(password);
  let status=response.status();let raw=body(response).await?;
  if status!=reqwest::StatusCode::OK{return Err(service_error(status,&raw));}parse_login(&raw)
 }
 pub async fn register(&self,id:&str,display_name:Option<&str>,email:&str,code:&str,password:Zeroizing<String>,challenge:&str)->Result<String>{
  if !login_id(id){return Err(error("invalid_login_id","登录 ID 须为 4–32 位英文字母或数字。"));}
  if display_name.is_some_and(|n|!n.is_empty()&&!nickname(n)){return Err(error("invalid_display_nickname","昵称须为 1–64 个字符。"));}
  crate::account::validate_email(email)?;
  if code.len()!=6||!code.bytes().all(|b|b.is_ascii_digit())||challenge.is_empty()||challenge.len()>256{return Err(error("invalid_code","请填写邮箱验证码。"));}
  if !(6..=128).contains(&password.chars().count())||password.chars().any(char::is_control){return Err(error("invalid_password","请输入 6–128 位密码。"));}
  #[derive(Serialize)]struct Request<'a>{login_id:&'a str,#[serde(skip_serializing_if="Option::is_none")]username:Option<&'a str>,email:&'a str,code:&'a str,password:&'a str,challenge_id:&'a str}
  let response=self.http.post(format!("{}/api/account/register",self.base)).json(&Request{login_id:id,username:display_name.filter(|n|!n.is_empty()),email,code,password:&password,challenge_id:challenge}).send().await.map_err(|_|failed())?;drop(password);let status=response.status();let raw=body(response).await?;
  if status!=reqwest::StatusCode::CREATED{return Err(service_error(status,&raw));}
  #[derive(Deserialize)]#[serde(deny_unknown_fields)]struct Registered{ok:bool,uid:String,username:String,provisioning:String,#[serde(rename="gameReady")]game_ready:bool}
  let r:Registered=serde_json::from_slice(&raw).map_err(|_|failed())?;if !r.ok||!uid(&r.uid)||!nickname(&r.username)||r.provisioning!="pending"||r.game_ready{return Err(failed());}Ok(r.uid)
 }
 pub async fn logout(&self,session:&Session)->Result<()>{let response=self.http.post(format!("{}/api/account/logout",self.base)).bearer_auth(session.token.as_str()).send().await.map_err(|_|failed())?;let status=response.status();let raw=body(response).await?;if status!=reqwest::StatusCode::OK{return Err(service_error(status,&raw));}Ok(())}
}
async fn body(mut response:reqwest::Response)->Result<Zeroizing<Vec<u8>>>{if response.content_length().is_some_and(|n|n>65536){return Err(failed());}let mut raw=Zeroizing::new(Vec::new());while let Some(chunk)=response.chunk().await.map_err(|_|failed())?{if raw.len()+chunk.len()>65536{return Err(failed());}raw.extend_from_slice(&chunk);}Ok(raw)}
fn service_error(status:reqwest::StatusCode,raw:&[u8])->crate::AppError{
 let value:serde_json::Value=serde_json::from_slice(raw).unwrap_or_default();let code=value.get("error").or_else(||value.get("code")).and_then(|v|v.as_str()).unwrap_or("");
 match code{"ambiguous_identifier" if status==reqwest::StatusCode::BAD_REQUEST=>error("ambiguous_identifier","该标识对应多个账号，请明确选择 ID 或 UID 登录。"),"invalid_session" if status==reqwest::StatusCode::UNAUTHORIZED=>error("login_required","登录已失效，请重新登录。"),"invalid_credentials" if status==reqwest::StatusCode::UNAUTHORIZED=>error("auth_failed","账号或密码不正确。"),"maintenance"=>error("maintenance","服务器维护中，请稍后再试。"),"mail_unavailable"=>error("mail_unavailable","邮件服务暂不可用，请稍后再试。"),"login_method_unavailable"=>error("login_method_unavailable","该登录方式暂未开放。"),_ if status==reqwest::StatusCode::TOO_MANY_REQUESTS=>error("rate_limited","操作频繁，请稍后再试。"),_=>failed()}
}
#[cfg(test)]mod tests{
 use super::*;
 #[test]fn private_id_not_nickname_and_ambiguous_numeric_selector_stays_explicit(){assert!(login_id("Ab1234"));for bad in ["abc","ab_cd","中文id","abcd-123","a b c"]{assert!(!login_id(bad));}assert!(nickname("新的昵称"));assert!(validate_identifier(IdentifierType::Id,"10000001").is_ok());assert!(validate_identifier(IdentifierType::Uid,"10000001").is_ok());assert!(!uid("00000000"));assert_eq!(serde_json::to_string(&IdentifierType::Id).unwrap(),"\"id\"");assert_eq!(serde_json::to_string(&IdentifierType::Uid).unwrap(),"\"uid\"");}
 #[test]fn token_is_original_string_uid_keeps_zeroes_and_game_ready_is_not_login(){let value=serde_json::json!({"ok":true,"uid":"00123456","username":"显示昵称","sessionToken":"ab".repeat(32),"expires_in":3600,"gameReady":false,"protocol":"platform-session-v1"});let session=parse_login(&serde_json::to_vec(&value).unwrap()).unwrap();assert_eq!(session.uid,"00123456");assert!(!session.game_ready);assert_eq!(session.token.len(),64);assert!(session.valid_for().as_secs()>3500);let mut bad=value;bad["sessionToken"]="a1.not-a-platform-session".into();assert!(parse_login(&serde_json::to_vec(&bad).unwrap()).is_err());}
 #[test]fn backend_messages_are_never_echoed(){let raw=br#"{"error":"unknown","message":"private password should never be shown"}"#;assert_eq!(service_error(reqwest::StatusCode::BAD_REQUEST,raw).message,"账号服务暂不可用，请稍后重试。");assert_eq!(service_error(reqwest::StatusCode::BAD_REQUEST,br#"{"error":"ambiguous_identifier"}"#).code,"ambiguous_identifier");assert_eq!(service_error(reqwest::StatusCode::FORBIDDEN,br#"{"error":"invalid_credentials"}"#).code,"platform_unavailable");}
}
