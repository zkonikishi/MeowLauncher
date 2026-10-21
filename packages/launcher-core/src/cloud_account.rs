//! Operator-hosted public account bridge. This is distinct from Max platform-v1.
//! External email/SMS/OAuth integrations live on the operator's backend.
use crate::{auth_config::Method,error,generic::Target,Result};
use serde::{Deserialize,Serialize};
use std::time::{Duration,Instant};
use zeroize::{Zeroize,Zeroizing};
pub const PROTOCOL:&str="meowlauncher-account-v1";
fn invalid()->crate::AppError{error("cloud_protocol","云端账号接口响应无效，未取得游戏准入。")}
#[derive(Clone)]
pub struct Client {http:reqwest::Client,base:String,target:Target,methods:Vec<Method>}
pub struct Session {pub username:String,pub native_account_id:String,token:Zeroizing<String>,expires:Instant}
impl Session {pub fn valid(&self)->bool{Instant::now()<self.expires}}
pub struct Challenge {id:Zeroizing<String>,pub method:Method,pub identifier:String,pub purpose:String,expires:Instant}
pub struct BrowserFlow {id:Zeroizing<String>,secret:Zeroizing<String>,expires:Instant,pub interval:u64,pub url:String,last_poll:Instant}
pub struct NativeGrant {pub username:String,pub password:Zeroizing<String>,pub native_account_id:String}
#[derive(Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
struct SessionReply {protocol:String,username:String,native_account_id:String,authority_id:String,realm_id:String,save_profile_id:String,session_token:String,expires_in:u64}
impl Drop for SessionReply {fn drop(&mut self){self.session_token.zeroize();}}
#[derive(Serialize)]
#[serde(rename_all="camelCase")]
pub struct ChallengeView {pub expires_in:u64,pub retry_after:u64}
#[derive(Serialize)]
#[serde(rename_all="camelCase")]
pub struct BrowserView {pub authorization_url:String,pub expires_in:u64,pub interval:u64}
impl Client {
 pub fn new(base:&str,target:Target,methods:Vec<Method>)->Result<Self>{
    target.validate()?;let u=crate::public_config::https_url(base)?;if u.path()!="/"||methods.is_empty(){return Err(invalid());}
    let http=reqwest::Client::builder().https_only(true).no_proxy().redirect(reqwest::redirect::Policy::none()).connect_timeout(Duration::from_secs(5)).timeout(Duration::from_secs(12)).build().map_err(|_|invalid())?;
    Ok(Self{http,base:base.trim_end_matches('/').into(),target,methods})
 }
 pub fn target(&self)->&Target{&self.target}
 fn method(&self,m:Method)->Result<()> {if !self.methods.contains(&m){return Err(error("auth_method_unavailable","服主尚未启用此账号方式。"));}Ok(())}
 async fn post<T:for<'a> Deserialize<'a>>(&self,route:&str,value:serde_json::Value,token:Option<&str>)->Result<T>{
    let mut request=self.http.post(format!("{}/meowlauncher/v1/{route}",self.base)).json(&value);
    if let Some(token)=token{request=request.bearer_auth(token);}
    let mut response=request.send().await.map_err(|_|error("cloud_unavailable","云端账号服务连接未完成，请稍后重试。"))?;
    if response.status()==reqwest::StatusCode::TOO_MANY_REQUESTS{return Err(error("cloud_rate_limit","请求过于频繁，请稍后重试。"));}
    if !response.status().is_success(){return Err(error("cloud_rejected","账号操作未完成，请检查输入或联系服主。"));}
    let mut bytes=Zeroizing::new(Vec::new());while let Some(chunk)=response.chunk().await.map_err(|_|invalid())?{if bytes.len()+chunk.len()>16384{return Err(invalid());}bytes.extend_from_slice(&chunk);}
    serde_json::from_slice(&bytes).map_err(|_|invalid())
 }
 fn session(&self,r:SessionReply)->Result<Session>{
    crate::generic::validate_credentials(&r.username,"x")?;
    if r.protocol!=PROTOCOL||r.authority_id!=self.target.authority_id||r.realm_id!=self.target.realm_id||r.save_profile_id!=self.target.save_profile_id||!positive_id(&r.native_account_id)||!opaque(&r.session_token)||r.expires_in==0||r.expires_in>3600{return Err(invalid());}
    Ok(Session{username:r.username.clone(),native_account_id:r.native_account_id.clone(),token:Zeroizing::new(r.session_token.clone()),expires:Instant::now()+Duration::from_secs(r.expires_in)})
 }
 pub async fn challenge(&self,method:Method,identifier:&str,purpose:&str)->Result<(Challenge,ChallengeView)>{
    self.method(method)?;if !matches!(method,Method::EmailCode|Method::PhoneCode)||!matches!(purpose,"login"|"register"|"reset")||identifier.is_empty()||identifier.len()>254||identifier.chars().any(char::is_control){return Err(invalid());}
    #[derive(Deserialize)]#[serde(rename_all="camelCase",deny_unknown_fields)]struct Reply{protocol:String,challenge_id:String,expires_in:u64,retry_after:u64}
    let r:Reply=self.post("challenge",serde_json::json!({"protocol":PROTOCOL,"method":method,"identifier":identifier,"purpose":purpose}),None).await?;
    if r.protocol!=PROTOCOL||!opaque(&r.challenge_id)||r.expires_in==0||r.expires_in>900||r.retry_after>900{return Err(invalid());}
    Ok((Challenge{id:Zeroizing::new(r.challenge_id),method,identifier:identifier.into(),purpose:purpose.into(),expires:Instant::now()+Duration::from_secs(r.expires_in)},ChallengeView{expires_in:r.expires_in,retry_after:r.retry_after}))
 }
 fn challenge_id<'a>(&self,c:Option<&'a Challenge>,method:Method,identifier:&str,purpose:&str)->Result<Option<&'a str>>{
    if method==Method::UsernamePassword{return Ok(None);}
    let c=c.ok_or(error("cloud_challenge_required","请先获取本次操作的验证码。"))?;
    if c.method!=method||c.identifier!=identifier||c.purpose!=purpose||Instant::now()>=c.expires{return Err(error("cloud_challenge_required","验证码挑战不匹配或已过期。"));}Ok(Some(c.id.as_str()))
 }
 pub async fn login(&self,method:Method,identifier:&str,password:Zeroizing<String>,code:&str,c:Option<&Challenge>)->Result<Session>{
    self.method(method)?;if method==Method::ThirdParty{return Err(invalid());}
    if method==Method::UsernamePassword{crate::generic::validate_credentials(identifier,&password)?;}
    let challenge=self.challenge_id(c,method,identifier,"login")?;
    if challenge.is_some()&&!valid_code(code){return Err(invalid());}
    let r=self.post("login",serde_json::json!({"protocol":PROTOCOL,"method":method,"identifier":identifier,"password":password.as_str(),"code":code,"challengeId":challenge}),None).await?;
    self.session(r)
 }
 pub async fn register(&self,method:Method,username:&str,identifier:&str,password:Zeroizing<String>,code:&str,c:Option<&Challenge>)->Result<()> {
    self.method(method)?;crate::generic::validate_credentials(username,&password)?;if method==Method::ThirdParty{return Err(invalid());}
    let challenge=self.challenge_id(c,method,identifier,"register")?;if challenge.is_some()&&!valid_code(code){return Err(invalid());}
    self.confirm("register",serde_json::json!({"protocol":PROTOCOL,"method":method,"username":username,"identifier":identifier,"password":password.as_str(),"code":code,"challengeId":challenge})).await
 }
 pub async fn reset(&self,method:Method,identifier:&str,password:Zeroizing<String>,code:&str,c:&Challenge)->Result<()> {
    self.method(method)?;crate::generic::validate_credentials("check",&password)?;if !valid_code(code){return Err(invalid());}
    let challenge=self.challenge_id(Some(c),method,identifier,"reset")?;
    self.confirm("reset",serde_json::json!({"protocol":PROTOCOL,"method":method,"identifier":identifier,"password":password.as_str(),"code":code,"challengeId":challenge})).await
 }
 async fn confirm(&self,route:&str,value:serde_json::Value)->Result<()> {
    #[derive(Deserialize)]#[serde(deny_unknown_fields)]struct Reply{protocol:String,ok:bool}
    let r:Reply=self.post(route,value,None).await?;if r.protocol!=PROTOCOL||!r.ok{return Err(invalid());}Ok(())
 }
 pub async fn begin_browser(&self)->Result<(BrowserFlow,BrowserView)>{
    self.method(Method::ThirdParty)?;
    #[derive(Deserialize)]#[serde(rename_all="camelCase",deny_unknown_fields)]struct Reply{protocol:String,flow_id:String,flow_secret:String,authorization_url:String,expires_in:u64,interval:u64}
    let r:Reply=self.post("browser/begin",serde_json::json!({"protocol":PROTOCOL}),None).await?;
    let url=crate::public_config::https_url(&r.authorization_url)?;let base=reqwest::Url::parse(&self.base).map_err(|_|invalid())?;
    if r.protocol!=PROTOCOL||!opaque(&r.flow_id)||!opaque(&r.flow_secret)||url.origin()!=base.origin()||r.expires_in==0||r.expires_in>300||!(2..=10).contains(&r.interval){return Err(invalid());}
    let view=BrowserView{authorization_url:r.authorization_url.clone(),expires_in:r.expires_in,interval:r.interval};
    Ok((BrowserFlow{id:Zeroizing::new(r.flow_id),secret:Zeroizing::new(r.flow_secret),expires:Instant::now()+Duration::from_secs(r.expires_in),interval:r.interval,url:r.authorization_url,last_poll:Instant::now()},view))
 }
 pub async fn poll_browser(&self,flow:&mut BrowserFlow)->Result<Option<Session>>{
    if Instant::now()>=flow.expires{return Err(error("cloud_flow_expired","浏览器登录已过期，请重新开始。"));}
    if flow.last_poll.elapsed()<Duration::from_secs(flow.interval){return Err(error("cloud_poll_wait","请等待浏览器登录确认。"));}flow.last_poll=Instant::now();
    #[derive(Deserialize)]#[serde(deny_unknown_fields)]struct Reply{protocol:String,pending:bool,session:Option<SessionReply>}
    let r:Reply=self.post("browser/poll",serde_json::json!({"protocol":PROTOCOL,"flowId":flow.id.as_str(),"flowSecret":flow.secret.as_str()}),None).await?;
    if r.protocol!=PROTOCOL{return Err(invalid());}match(r.pending,r.session){(true,None)=>Ok(None),(false,Some(s))=>self.session(s).map(Some),_=>Err(invalid())}
 }
 pub async fn native_grant(&self,session:&Session)->Result<NativeGrant>{
    if Instant::now()>=session.expires{return Err(error("login_required","云端会话已过期，请重新登录。"));}
    #[derive(Deserialize)]#[serde(rename_all="camelCase",deny_unknown_fields)]struct Reply{protocol:String,username:String,password:String,native_account_id:String,authority_id:String,realm_id:String,save_profile_id:String,expires_in:u64}
    impl Drop for Reply{fn drop(&mut self){self.password.zeroize();}}
    let r:Reply=self.post("native-grant",serde_json::json!({"protocol":PROTOCOL}),Some(session.token.as_str())).await?;
    if r.protocol!=PROTOCOL||r.native_account_id!=session.native_account_id||r.authority_id!=self.target.authority_id||r.realm_id!=self.target.realm_id||r.save_profile_id!=self.target.save_profile_id||r.expires_in==0||r.expires_in>300{return Err(invalid());}
    crate::generic::validate_credentials(&r.username,&r.password)?;
    Ok(NativeGrant{username:r.username.clone(),password:Zeroizing::new(r.password.clone()),native_account_id:r.native_account_id.clone()})
 }
 pub async fn logout(&self,session:&Session)->Result<()> {
    #[derive(Deserialize)]#[serde(deny_unknown_fields)]struct Reply{protocol:String,ok:bool}
    let r:Reply=self.post("logout",serde_json::json!({"protocol":PROTOCOL}),Some(session.token.as_str())).await?;
    if r.protocol!=PROTOCOL||!r.ok{return Err(invalid());}Ok(())
 }
}
fn positive_id(s:&str)->bool{!s.starts_with('0')&&s.parse::<i64>().is_ok_and(|n|n>0)&&s.bytes().all(|c|c.is_ascii_digit())}
fn opaque(s:&str)->bool{(16..=2048).contains(&s.len())&&s.bytes().all(|b|b.is_ascii_alphanumeric()||matches!(b,b'-'|b'_'|b'.'))}
fn valid_code(s:&str)->bool{(4..=12).contains(&s.len())&&s.bytes().all(|b|b.is_ascii_digit())}

#[cfg(test)]mod tests {
 use super::*;
 use std::{io::{Read,Write},sync::{Arc,Mutex,atomic::{AtomicBool,Ordering}},thread::JoinHandle};
 fn target()->Target{Target{host:"127.0.0.1".into(),port:27002,server_name:"fixture".into(),authority_id:"owner".into(),realm_id:"realm".into(),save_profile_id:"save".into(),ca_certificate:None}}
 fn client(base:String)->Client{Client{http:reqwest::Client::builder().no_proxy().timeout(Duration::from_secs(2)).build().unwrap(),base,target:target(),methods:vec![Method::UsernamePassword,Method::EmailCode,Method::PhoneCode,Method::ThirdParty]}}
 fn reply()->serde_json::Value{serde_json::json!({"protocol":PROTOCOL,"username":"Player","nativeAccountId":"42","authorityId":"owner","realmId":"realm","saveProfileId":"save","sessionToken":"fixture_session_0123456789","expiresIn":300})}
 struct Server {stop:Arc<AtomicBool>,thread:Option<JoinHandle<()>>}
 impl Drop for Server{fn drop(&mut self){self.stop.store(true,Ordering::SeqCst);if let Some(t)=self.thread.take(){t.join().unwrap();}}}
 fn server(responses:Vec<(u16,serde_json::Value)>)->(String,Arc<Mutex<Vec<(String,serde_json::Value)>>>,Server){
  let listener=std::net::TcpListener::bind("127.0.0.1:0").unwrap();let base=format!("http://{}",listener.local_addr().unwrap());listener.set_nonblocking(true).unwrap();
  let seen=Arc::new(Mutex::new(Vec::new()));let requests=seen.clone();let stop=Arc::new(AtomicBool::new(false));let stopping=stop.clone();
  let thread=std::thread::spawn(move||{for (status,response) in responses{let mut stream=loop{match listener.accept(){Ok((s,_))=>break s,Err(e)if e.kind()==std::io::ErrorKind::WouldBlock=>{if stopping.load(Ordering::SeqCst){return;}std::thread::sleep(Duration::from_millis(5));},Err(_)=>return}};stream.set_read_timeout(Some(Duration::from_secs(2))).unwrap();let mut data=Vec::new();let mut buffer=[0u8;1024];let (boundary,length)=loop{let n=match stream.read(&mut buffer){Ok(n)if n>0=>n,_=>return};data.extend_from_slice(&buffer[..n]);assert!(data.len()<32768);if let Some(end)=data.windows(4).position(|v|v==b"\r\n\r\n"){let headers=String::from_utf8_lossy(&data[..end]);let len=headers.lines().find_map(|line|line.to_ascii_lowercase().strip_prefix("content-length:").map(|s|s.trim().parse::<usize>().unwrap())).unwrap();break(end+4,len);}};
   while data.len()<boundary+length{let n=stream.read(&mut buffer).unwrap();if n==0{return;}data.extend_from_slice(&buffer[..n]);}
   requests.lock().unwrap().push((String::from_utf8_lossy(&data[..boundary]).into(),serde_json::from_slice(&data[boundary..boundary+length]).unwrap()));let bytes=serde_json::to_vec(&response).unwrap();write!(stream,"HTTP/1.1 {status} Fixture\r\nContent-Length: {}\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n",bytes.len()).unwrap();stream.write_all(&bytes).unwrap();
  }});(base,seen,Server{stop,thread:Some(thread)})
 }
 #[test]fn public_constructor_rejects_http_and_paths(){for url in ["http://localhost","https://service.example/api","https://user:pass@service.example"]{assert!(Client::new(url,target(),vec![Method::UsernamePassword]).is_err());}}
 #[test]fn session_scope_and_expiry_are_not_ui_claims(){let c=client("http://127.0.0.1:1".into());for (field,bad)in[("authorityId",serde_json::json!("foreign")),("realmId",serde_json::json!("foreign")),("nativeAccountId",serde_json::json!("01")),("expiresIn",serde_json::json!(3601)),("protocol",serde_json::json!("platform-v1"))]{let mut r=reply();r[field]=bad;assert!(c.session(serde_json::from_value(r).unwrap()).is_err());}}
 #[tokio::test]async fn actual_http_login_and_scoped_native_grant(){
  let grant=serde_json::json!({"protocol":PROTOCOL,"username":"Player","password":"fixture-native-password","nativeAccountId":"42","authorityId":"owner","realmId":"realm","saveProfileId":"save","expiresIn":30});let (base,seen,_server)=server(vec![(200,reply()),(200,grant)]);let c=client(base);
  let session=c.login(Method::UsernamePassword,"Player",Zeroizing::new("fixture-password".into()),"",None).await.unwrap();let grant=c.native_grant(&session).await.unwrap();assert_eq!(grant.native_account_id,"42");assert_eq!(grant.password.as_str(),"fixture-native-password");let seen=seen.lock().unwrap();assert!(seen[0].0.starts_with("POST /meowlauncher/v1/login "));assert!(seen[1].0.to_ascii_lowercase().contains("authorization: bearer fixture_session_0123456789"));assert_eq!(seen[0].1["identifier"],"Player");
 }
 #[tokio::test]async fn challenge_purpose_and_identity_cannot_be_swapped(){let response=serde_json::json!({"protocol":PROTOCOL,"challengeId":"fixture_challenge_0123456789","expiresIn":300,"retryAfter":30});let (base,seen,_server)=server(vec![(200,response)]);let c=client(base);let (challenge,_)=c.challenge(Method::EmailCode,"p@example.com","register").await.unwrap();assert!(c.login(Method::EmailCode,"p@example.com",Zeroizing::new(String::new()),"123456",Some(&challenge)).await.is_err());assert!(c.register(Method::EmailCode,"Player","other@example.com",Zeroizing::new("fixture-password".into()),"123456",Some(&challenge)).await.is_err());assert_eq!(seen.lock().unwrap().len(),1);}
 #[tokio::test]async fn register_and_reset_use_explicit_confirmation(){let challenge=serde_json::json!({"protocol":PROTOCOL,"challengeId":"fixture_challenge_0123456789","expiresIn":300,"retryAfter":30});let ok=serde_json::json!({"protocol":PROTOCOL,"ok":true});let (base,seen,_server)=server(vec![(200,challenge.clone()),(200,ok.clone()),(200,challenge),(200,ok)]);let c=client(base);let (registration,_)=c.challenge(Method::PhoneCode,"+12025550123","register").await.unwrap();c.register(Method::PhoneCode,"Player","+12025550123",Zeroizing::new("fixture-password".into()),"123456",Some(&registration)).await.unwrap();let (reset,_)=c.challenge(Method::PhoneCode,"+12025550123","reset").await.unwrap();c.reset(Method::PhoneCode,"+12025550123",Zeroizing::new("new-fixture-password".into()),"123456",&reset).await.unwrap();assert_eq!(seen.lock().unwrap().len(),4);}
 #[tokio::test]async fn rejected_http_does_not_echo_remote_secrets(){let (base,_,_server)=server(vec![(403,serde_json::json!({"message":"REMOTE_SECRET"}))]);let e=client(base).login(Method::UsernamePassword,"Player",Zeroizing::new("fixture-password".into()),"",None).await.err().unwrap();assert_eq!(e.code,"cloud_rejected");assert!(!e.message.contains("REMOTE_SECRET"));}
 #[tokio::test]async fn retired_or_foreign_native_identity_is_rejected(){let mut grant=reply();grant.as_object_mut().unwrap().remove("sessionToken");grant["password"]="fixture-native-password".into();grant["nativeAccountId"]="43".into();let (base,_,_server)=server(vec![(200,grant)]);let c=client(base);let session=c.session(serde_json::from_value(reply()).unwrap()).unwrap();assert!(c.native_grant(&session).await.is_err());}
 #[tokio::test]async fn browser_flow_poll_stays_native_and_respects_interval(){let (base,seen,_server)=server(vec![(200,serde_json::json!({"protocol":PROTOCOL,"pending":true,"session":null})),(200,serde_json::json!({"protocol":PROTOCOL,"pending":false,"session":reply()}))]);let c=client(base);let mut flow=BrowserFlow{id:Zeroizing::new("fixture_flow_0123456789".into()),secret:Zeroizing::new("fixture_secret_0123456789".into()),expires:Instant::now()+Duration::from_secs(30),interval:2,url:"https://service.example/flow".into(),last_poll:Instant::now()};assert!(c.poll_browser(&mut flow).await.is_err());flow.last_poll=Instant::now()-Duration::from_secs(3);assert!(c.poll_browser(&mut flow).await.unwrap().is_none());flow.last_poll=Instant::now()-Duration::from_secs(3);assert_eq!(c.poll_browser(&mut flow).await.unwrap().unwrap().native_account_id,"42");assert_eq!(seen.lock().unwrap().len(),2);}
}
