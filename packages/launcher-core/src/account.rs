use crate::{error, error_with_delay, unix_now, Result};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::Duration;
use zeroize::Zeroizing;

pub const HOST: &str = "launcher.invalid";
pub const GAME_HOST: &str = "203.0.113.10";
// 测试用自签公钥证书的 DER 摘要（仅用于证书校验单元测试，无私钥、无真实端点）。
pub const ROOT_SHA256: &str = "69d3b1ccf08161ae6bed3ed25f28ff12b11e74af4bfb9801bfa61df0b485f23c";

pub struct AccountClient { http: reqwest::Client, base: String, realm: Option<String>, legacy: bool, skew: AtomicI64 }
// Credentials never implement Debug or Serialize and never cross IPC.
#[derive(Clone)]
pub struct Session { pub uid: String, pub token: Zeroizing<String>, pub expires: u64 }
pub struct Ticket { pub uid: String, pub token: Zeroizing<String>, pub expires: u64 }
/// 注册/重设密码用的邮箱验证码用途。服务端校验 purpose，不允许在这里自由填字符串。
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum CodePurpose { Register, Reset }
impl CodePurpose { pub fn as_str(self)->&'static str {match self{Self::Register=>"register",Self::Reset=>"reset"}} }
/// 服务端签发的验证码挑战。`id` 与密码同级敏感：不实现 Debug/Serialize、不写日志。
#[derive(Clone)]
pub struct Challenge { pub id: Zeroizing<String>, pub retry_after: u64 }
/// 注册结果。服务端只回 UID 与开通状态，**不**回任何凭据，也不自动登录。
pub struct Registration { pub uid: String, pub provisioning_pending: bool }
/// 重设密码令牌，同样按凭据处理。
pub struct ResetToken { pub token: Zeroizing<String>, pub expires: u64 }
#[derive(Serialize)]
pub struct ServiceStatus { pub available: bool, pub message: &'static str }
#[derive(Deserialize)]
struct SessionResponse {
    ok: bool, uid: String,
    #[serde(rename="sessionToken")] token: String,
    expires_in: u64, protocol: String,
}
impl Drop for SessionResponse { fn drop(&mut self) { use zeroize::Zeroize; self.token.zeroize(); } }
#[derive(Deserialize)]
struct TicketResponse { ok: bool, uid: String, ticket: String, expires_in: u64, protocol: String }
impl Drop for TicketResponse { fn drop(&mut self) { use zeroize::Zeroize; self.ticket.zeroize(); } }
impl AccountClient {
    /// 只用编译期固定的 Meowopia 账号服务。通用版（sponsor）没有默认账号服务：
    /// 它必须拿到服主的 `launcher.public.json`，否则会在取到地址前就失败。
    pub fn new() -> Result<Self> {
        if crate::edition::is_sponsor() {return Err(error("public_config_required","通用版必须使用服主提供的发行配置，不会连接默认账号服务。"));}
        Self::configured(&format!("https://{HOST}"), None)
    }
    pub fn configured(base: &str, realm: Option<String>) -> Result<Self> {
        let url=crate::public_config::https_url(base)?;
        if url.path()!="/" {return Err(error("public_config_invalid","账号地址必须是 HTTPS 来源。"));}
        // The public same-origin platform endpoint uses normal Web PKI validation.
        // Never resolve the website to the old direct-origin IP or bypass TLS.
        let http=reqwest::Client::builder().https_only(true).no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(5)).timeout(Duration::from_secs(12))
            .user_agent("MeowLauncher/0.5.0").build()
            .map_err(|_| error("tls_config","无法初始化安全连接。"))?;
        Ok(Self{http,base:base.trim_end_matches('/').into(),realm,legacy:false,skew:AtomicI64::new(0)})
    }
    pub fn legacy_online()->Result<Self> {let mut c=Self::new()?;c.legacy=true;Ok(c)}
    #[cfg(test)]
    pub(crate) fn legacy_for_test(base:&str)->Self {
        Self{http:reqwest::Client::builder().no_proxy().timeout(Duration::from_secs(5)).build().unwrap(),base:base.trim_end_matches('/').into(),realm:None,legacy:true,skew:AtomicI64::new(0)}
    }
    pub fn is_legacy(&self)->bool {self.legacy}
    pub async fn legacy_ticket(&self,identifier:&str,password:Zeroizing<String>)->Result<Ticket> {
        if !self.legacy {return Err(error("protocol_unavailable","未启用线上兼容协议。"));}
        validate_input(identifier,&password)?;
        if !valid_uid(identifier.trim()){return Err(error("invalid_identifier","请使用 8 位 UID 登录；忘记 UID 可先找回。"));}
        let response=self.http.post(format!("{}/api/account/login",self.base)).json(&serde_json::json!({"username":identifier.trim(),"password":password.as_str()})).send().await.map_err(network_error)?;
        drop(password);
        self.note_server_time(&response);
        crate::legacy_online::parse(&checked_body(response).await?,self.trusted_now(),identifier.trim())
    }
    pub async fn recover_uid(&self,email:&str,password:Zeroizing<String>)->Result<String> {
        validate_input(email,&password)?;
        if !email.trim().contains('@'){return Err(error("invalid_email","请输入有效邮箱。"));}
        #[derive(Serialize)] struct Request<'a>{email:&'a str,password:&'a str}
        let response=self.http.post(format!("{}/api/account/recoverUid",self.base)).json(&Request{email:email.trim(),password:password.as_str()}).send().await.map_err(network_error)?;
        drop(password);
        let raw=Zeroizing::new(checked_body(response).await?);
        parse_recovered_uid(&raw)
    }
    /// 请求邮箱验证码。返回的 challenge_id 必须原样带回 register / 重设密码请求，
    /// 否则服务端会走“该邮箱最近一次未消费的挑战”，可能对不上玩家收到的那封邮件。
    pub async fn send_code(&self,email:&str,purpose:CodePurpose)->Result<Challenge> {
        let email=normalized_email(email)?;
        #[derive(Serialize)] struct Request<'a>{email:&'a str,purpose:&'a str}
        let response=self.http.post(format!("{}/api/account/sendCode",self.base))
            .json(&Request{email:&email,purpose:purpose.as_str()}).send().await.map_err(network_error)?;
        self.note_server_time(&response);
        let raw=checked_body(response).await?;
        if self.legacy {parse_legacy_challenge(&raw)} else {parse_challenge(&raw)}
    }
    /// 在登陆器里直接注册。成功后服务端只回 UID，开通是 pending，需要另行验证才能入场。
    pub async fn register(&self,email:&str,code:&str,password:Zeroizing<String>,challenge:&Challenge)->Result<Registration> {
        let email=normalized_email(email)?;
        validate_code(code)?;
        validate_new_password(&password)?;
        #[derive(Serialize)] struct Request<'a>{email:&'a str,code:&'a str,password:&'a str,challenge_id:&'a str}
        let response=self.http.post(format!("{}/api/account/register",self.base))
            .json(&Request{email:&email,code:code.trim(),password:&password,challenge_id:challenge.id.as_str()}).send().await.map_err(network_error)?;
        drop(password);
        let raw=checked_body(response).await?;
        if self.legacy {parse_legacy_registration(&raw)} else {parse_registration(&raw)}
    }
    /// 忘记密码第一步：邮箱验证码换一次性重设令牌，不改变密码、不签发会话。
    pub async fn request_password_reset(&self,email:&str,code:&str,challenge:&Challenge)->Result<ResetToken> {
        let email=normalized_email(email)?;
        validate_code(code)?;
        #[derive(Serialize)] struct Request<'a>{email:&'a str,code:&'a str,challenge_id:&'a str}
        let response=self.http.post(format!("{}/api/account/forgotPassword",self.base))
            .json(&Request{email:&email,code:code.trim(),challenge_id:challenge.id.as_str()}).send().await.map_err(network_error)?;
        let raw=checked_body(response).await?;
        if self.legacy {parse_legacy_reset(&raw)} else {parse_reset_token(&raw)}
    }
    /// 忘记密码第二步：用令牌设置新密码。服务端会撤销旧会话与旧挑战。
    pub async fn reset_password(&self,token:&Zeroizing<String>,password:Zeroizing<String>)->Result<()> {
        if self.legacy {if !valid_legacy_reset_token(token){return Err(error("invalid_token","重设凭据无效，请重新获取验证码。"));}} else {validate_reset_token(token)?;}
        validate_new_password(&password)?;
        #[derive(Serialize)] struct Request<'a>{#[serde(rename="resetToken")]reset_token:&'a str,#[serde(rename="newPassword")]new_password:&'a str}
        let response=self.http.post(format!("{}/api/account/resetPassword",self.base))
            .json(&Request{reset_token:token.as_str(),new_password:&password}).send().await.map_err(network_error)?;
        drop(password);
        let raw=checked_body(response).await?;
        #[derive(Deserialize)] #[serde(deny_unknown_fields)] struct Reset{ok:bool,#[serde(default,rename="message")]_message:Option<String>}
        let r:Reset=serde_json::from_slice(&raw).map_err(|_|error("invalid_response","重设密码响应无效；密码是否已修改未能确认。"))?;
        if !r.ok{return Err(error("invalid_response","重设密码响应无效；密码是否已修改未能确认。"));}
        Ok(())
    }
    pub async fn status(&self)->Result<ServiceStatus> {
        let response=self.http.get(format!("{}/api/account/status",self.base)).send().await.map_err(network_error)?;
        let data=checked_body(response).await?;
        let body:serde_json::Value=serde_json::from_slice(&data).map_err(|_| error("invalid_response","账号服务响应格式无效。"))?;
        if self.legacy {
            if body.get("ok").and_then(|v|v.as_bool())!=Some(true) || body.get("protocol").is_some() || body.get("accounts").and_then(|v|v.as_u64()).is_none() {return Err(error("protocol_unavailable","当前接口不匹配线上兼容协议。"));}
            return Ok(ServiceStatus{available:true,message:"线上兼容账号接口可访问，游戏入场需实际验证。"});
        }
        if body.get("ok").and_then(|v|v.as_bool())!=Some(true) || body.get("protocol").and_then(|v|v.as_str())!=Some("platform-v1") {
            return Err(error("protocol_unavailable","新版账号服务尚未就绪，请稍后重试。"));
        }
        Ok(ServiceStatus{available:true,message:"新版账号服务可连接，游戏入场需另行验证。"})
    }
    pub async fn probe_login_route(&self)->Result<u16> {
        let response=self.http.post(format!("{}/api/account/login",self.base))
            .json(&serde_json::json!({"identifier":"","password":"","intent":"platform"}))
            .send().await.map_err(network_error)?;
        Ok(response.status().as_u16())
    }
    pub async fn login(&self,identifier:&str,password:Zeroizing<String>)->Result<Session> {
        if self.legacy {return Err(error("legacy_direct_launch","线上兼容首版需要登录后直接进入游戏。"));}
        validate_input(identifier,&password)?;
        #[derive(Serialize)] struct Request<'a>{identifier:&'a str,password:&'a str,intent:&'a str}
        let response=self.http.post(format!("{}/api/account/login",self.base))
            .json(&Request{identifier:identifier.trim(),password:&password,intent:"platform"})
            .send().await.map_err(network_error)?;
        drop(password);
        self.note_server_time(&response);
        parse_session(&checked_body(response).await?,self.trusted_now())
    }
    pub async fn ticket(&self,session:&Session)->Result<Ticket> {
        if session.expires<=self.trusted_now()+5 {return Err(error("login_required","登录已过期，请重新登录。"));}
        let response=self.http.post(format!("{}/api/account/gameTickets",self.base))
            .bearer_auth(session.token.as_str()).json(&self.realm.as_ref().map(|r|serde_json::json!({"realmId":r})).unwrap_or(serde_json::json!({})))
            .send().await.map_err(network_error)?;
        self.note_server_time(&response);
        parse_ticket(&checked_body(response).await?,self.trusted_now(),&session.uid)
    }
    pub async fn logout(&self,session:&Session)->Result<()> {
        let response=self.http.post(format!("{}/api/account/logout",self.base))
            .bearer_auth(session.token.as_str()).json(&serde_json::json!({}))
            .send().await.map_err(network_error)?;
        checked_body(response).await?;Ok(())
    }
    /// 凭据由账号服务签发，有效期也按服务端时钟计算：记住本机与该响应的时差，
    /// 避免本机时钟偏慢/偏快时把刚签发的凭据误判为过期或异常。
    fn note_server_time(&self,response:&reqwest::Response){
        if let Some(seconds)=response.headers().get(reqwest::header::DATE).and_then(|v|v.to_str().ok()).and_then(http_date_seconds){
            self.skew.store(seconds-unix_now() as i64,Ordering::Relaxed);
        }
    }
    pub fn trusted_now(&self)->u64 {(unix_now() as i64+self.skew.load(Ordering::Relaxed)).max(0) as u64}
}
fn valid_uid(uid:&str)->bool {uid.len()==8 && uid != "00000000" && uid.bytes().all(|b|b.is_ascii_digit())}
/// `Date: Wed, 14 Oct 2026 09:20:07 GMT`（IMF-fixdate，HTTP 必需头）→ Unix 秒。
fn http_date_seconds(value:&str)->Option<i64> {
    let mut fields=value.trim().split_whitespace();
    let _weekday=fields.next()?;
    let day: i64=fields.next()?.trim_end_matches(',').parse().ok()?;
    let month: i64=match fields.next()? {"Jan"=>1,"Feb"=>2,"Mar"=>3,"Apr"=>4,"May"=>5,"Jun"=>6,"Jul"=>7,"Aug"=>8,"Sep"=>9,"Oct"=>10,"Nov"=>11,"Dec"=>12,_=>return None};
    let year: i64=fields.next()?.parse().ok()?;
    let mut clock=fields.next()?.split(':');
    let hour: i64=clock.next()?.parse().ok()?;
    let minute: i64=clock.next()?.parse().ok()?;
    let second: i64=clock.next()?.parse().ok()?;
    if clock.next().is_some() || !(1..=31).contains(&day) || !(1..=9999).contains(&year) || hour>23 || minute>59 || second>60 {return None;}
    // 公历日期转儒略日数（Howard Hinnant days_from_civil）。
    let y=year-(month<=2) as i64;
    let era=(if y>=0 {y} else {y-399})/400;
    let yoe=y-era*400;
    let doy=(153*((month+9)%12)+2)/5+day-1;
    let doe=yoe*365+yoe/4-yoe/100+doy;
    Some((era*146097+doe-719468)*86400+hour*3600+minute*60+second)
}
fn valid_token(token:&str)->bool {(32..=2048).contains(&token.len()) && token.bytes().all(|b|b.is_ascii_alphanumeric()||b==b'-'||b==b'_')}
fn parse_session(data:&[u8],now:u64)->Result<Session> {
    let mut r:SessionResponse=serde_json::from_slice(data).map_err(|_|error("protocol_unavailable","账号服务未返回新版登录会话。"))?;
    if !r.ok || r.protocol!="platform-session-v1" || !valid_uid(&r.uid) || !valid_token(&r.token) || !(6..=3600).contains(&r.expires_in) {
        return Err(error("invalid_response","登录会话响应无效。"));
    }
    Ok(Session{uid:r.uid.clone(),token:Zeroizing::new(std::mem::take(&mut r.token)),expires:now+r.expires_in})
}
fn parse_ticket(data:&[u8],now:u64,uid:&str)->Result<Ticket> {
    let mut r:TicketResponse=serde_json::from_slice(data).map_err(|_|error("invalid_response","游戏票据响应无效。"))?;
    if !r.ok || r.protocol!="platform-ticket-v1" || r.uid!=uid || !valid_uid(&r.uid) || !valid_token(&r.ticket) || !(6..=60).contains(&r.expires_in) {
        return Err(error("invalid_response","游戏票据与账号不匹配或已过期。"));
    }
    Ok(Ticket{uid:r.uid.clone(),token:Zeroizing::new(std::mem::take(&mut r.ticket)),expires:now+r.expires_in})
}
async fn checked_body(mut response:reqwest::Response)->Result<Zeroizing<Vec<u8>>> {
    let status=response.status().as_u16();
    let mut body=Zeroizing::new(Vec::new());
    while let Some(chunk)=response.chunk().await.map_err(network_error)? {
        if body.len()+chunk.len()>16384 {return Err(error("invalid_response","账号响应过大。"));}
        body.extend_from_slice(&chunk);
    }
    if !(200..300).contains(&status) {
        let parsed:serde_json::Value=serde_json::from_slice(&body).unwrap_or_default();
        let retry=parsed.get("retry_after").and_then(|v|v.as_u64()).filter(|v|*v<=3600);
        let mapped=match parsed.get("error").and_then(|v|v.as_str()) {
            Some("invalid_session")=>error("login_required","登录已过期或被撤销，请重新登录。"),
            Some("provisioning_pending"|"registration_not_completed")=>error("provisioning_pending","游戏账号正在开通，请稍后再试。"),
            Some("launcher_upgrade_required")=>error("protocol_unavailable","账号服务协议不匹配，请联系管理员。"),
            Some("invalid_code")=>error("invalid_code","验证码错误或已失效，请重新获取。"),
            Some("invalid_email")=>error("invalid_email","邮箱格式不被接受，请检查后重试。"),
            Some("invalid_password")=>error("invalid_password","密码需 6–128 个字符，且不能包含控制字符。"),
            Some("invalid_display_name")=>error("invalid_request","昵称不符合要求。"),
            Some("invalid_purpose")=>error("invalid_request","请求无效，请重新打开注册页面。"),
            Some("mail_unavailable")=>error("mail_unavailable","邮件服务暂不可用，请联系管理员。"),
            Some("cooldown"|"rate_limited")=>error_with_delay("rate_limited","请求过于频繁，请稍后再试。",retry),
            Some("uid_allocation_exhausted"|"uid_policy_version_mismatch")=>error("service_unavailable","账号服务暂时无法分配 UID，请稍后再试。"),
            Some("invalid_token")=>error("invalid_token","重设凭据已失效，请重新验证邮箱。"),
            Some("forbidden"|"tls_required")=>error("forbidden","账号服务拒绝了此连接，请联系管理员检查代理配置。"),
            Some("client_identity_required")=>error("forbidden","代理未提供客户端身份，请联系管理员。"),
            Some("json_required"|"invalid_json"|"invalid_request")=>error("invalid_request","请求无效，请重新打开注册页面。"),
            Some("temporarily_unavailable")=>error("service_unavailable","账号服务暂时不可用，请稍后重试。"),
            Some("not_found")=>error("api_unavailable","该账号接口尚未部署，请稍后重试或联系管理员。"),
            _=>status_error(status),
        };
        return Err(error_with_delay(mapped.code,mapped.message,retry.or(mapped.retry_after)));
    }
    Ok(body)
}
pub fn validate_input(identifier: &str, password: &str) -> Result<()> {
    let i = identifier.trim();
    let uid = valid_uid(i);
    let email = i.len() <= 254
        && i.split('@').count() == 2
        && i.split_once('@').is_some_and(|(a, b)| {
            !a.is_empty() && b.contains('.') && !b.starts_with('.') && !b.ends_with('.')
        });
    if (!uid && !email) || i.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(error("invalid_identifier", "请输入 8 位 UID 或有效邮箱。"));
    }
    if password.is_empty() || password.chars().count() > 128 {
        return Err(error("invalid_password", "请输入密码，最多 128 个字符。"));
    }
    Ok(())
}
pub fn validate_login_uid(identifier:&str)->Result<()> {
    if !valid_uid(identifier.trim()){return Err(error("invalid_identifier","请使用 8 位 UID 登录；忘记 UID 可先找回。"));}Ok(())
}
fn parse_recovered_uid(raw:&[u8])->Result<String>{
    #[derive(Deserialize)] #[serde(deny_unknown_fields)] struct Recovery{ok:bool,uid:String}
    let r:Recovery=serde_json::from_slice(raw).map_err(|_|error("recovery_response","找回 UID 接口响应无效；没有登录或签发游戏凭据。"))?;
    if !r.ok||!valid_uid(&r.uid){return Err(error("recovery_response","找回 UID 接口未返回有效 UID。"));}
    Ok(r.uid)
}
/// 本地预检只挡明显无效的输入，正式判定始终在账号服务。这里刻意比服务端宽松
/// （例如不实现 IDN 转 punycode），避免把服务端会接受的邮箱在本地先拒掉。
pub fn validate_email(value:&str)->Result<String>{
    let invalid=||error("invalid_email","请输入有效邮箱。");
    let normalized=value.trim().to_lowercase();
    if normalized.len()>254 || normalized.chars().count()>254 || normalized.chars().any(|c|c.is_control()||c.is_whitespace()) {return Err(invalid());}
    let mut parts=normalized.split('@');
    let (local,domain)=(parts.next().unwrap_or_default(),parts.next().unwrap_or_default());
    if parts.next().is_some() {return Err(invalid());}
    if local.is_empty() || local.chars().count()>64 || local.starts_with('.') || local.ends_with('.') || local.contains("..") {return Err(invalid());}
    if domain.is_empty() || domain.chars().count()>253 || !domain.contains('.') || domain.starts_with('.') || domain.ends_with('.') || domain.contains("..") || !domain.chars().any(|c|c.is_alphabetic()) {return Err(invalid());}
    Ok(normalized)
}
fn normalized_email(value:&str)->Result<String>{validate_email(value)}
/// 与服务端 Password() 同口径：6–128 个字符、无控制字符。密码不 trim、不改写。
pub fn validate_new_password(password:&str)->Result<()>{
    let length=password.encode_utf16().count();
    if !(6..=128).contains(&length) || password.chars().any(char::is_control) {
        return Err(error("invalid_password","密码需 6–128 个字符，且不能包含控制字符。"));
    }
    Ok(())
}
pub fn validate_code(code:&str)->Result<()>{
    let code=code.trim();
    if code.len()!=6 || !code.bytes().all(|b|b.is_ascii_digit()){return Err(error("invalid_code","请输入 6 位数字验证码。"));}
    Ok(())
}
fn valid_challenge_id(value:&str)->bool {
    value.len()==36 && value.bytes().all(|b|b.is_ascii_hexdigit()||b==b'-') && value.split('-').map(|p|p.len()).collect::<Vec<_>>()==[8,4,4,4,12]
}
fn valid_reset_token(value:&str)->bool {
    value.len()==101 && value.bytes().all(|b|b.is_ascii_alphanumeric()||b==b'-'||b==b'_'||b==b'.') && value.split('.').next().is_some_and(valid_challenge_id)
}
fn validate_reset_token(value:&str)->Result<()>{
    if !valid_reset_token(value){return Err(error("invalid_token","重设凭据已失效，请重新验证邮箱。"));}
    Ok(())
}
/// 服务端只回 `{ok,challenge_id,retry_after,delivery}`。challenge_id 视作凭据：不日志、不回界面。
fn parse_challenge(raw:&[u8])->Result<Challenge>{
    #[derive(Deserialize)] struct Response{ok:bool,challenge_id:String,retry_after:u64,delivery:String}
    let r:Response=serde_json::from_slice(raw).map_err(|_|error("invalid_response","验证码接口响应无效；未收到可用的验证码挑战。"))?;
    if !r.ok || r.delivery!="queued" || !valid_challenge_id(&r.challenge_id) {
        return Err(error("invalid_response","验证码接口未返回可用挑战，请重新发送。"));
    }
    Ok(Challenge{id:Zeroizing::new(r.challenge_id),retry_after:r.retry_after.min(3600)})
}
/// 注册成功响应**必须**只带 UID 与开通状态。`deny_unknown_fields` 让任何凭据字段
/// （token/password/session）直接变成协议错误，而不是被悄悄忽略。
fn parse_registration(raw:&[u8])->Result<Registration>{
    #[derive(Deserialize)] #[serde(deny_unknown_fields)] struct Response{ok:bool,uid:String,provisioning:String}
    let r:Response=serde_json::from_slice(raw).map_err(|_|error("invalid_response","注册响应无效；未确认 UID 是否已分配。"))?;
    if !r.ok || !valid_uid(&r.uid) || r.provisioning.is_empty() || r.provisioning.len()>32 {
        return Err(error("invalid_response","注册响应未返回有效 UID。"));
    }
    Ok(Registration{uid:r.uid.clone(),provisioning_pending:r.provisioning=="pending"})
}
fn parse_reset_token(raw:&[u8])->Result<ResetToken>{
    #[derive(Deserialize)] #[serde(deny_unknown_fields)] struct Response{ok:bool,#[serde(rename="resetToken")]token:String,expires_in:u64}
    let r:Response=serde_json::from_slice(raw).map_err(|_|error("invalid_response","重设密码响应无效；没有拿到重设凭据。"))?;
    if !r.ok || !valid_reset_token(&r.token) || !(60..=3600).contains(&r.expires_in) {
        return Err(error("invalid_response","重设密码响应未返回有效凭据。"));
    }
    Ok(ResetToken{token:Zeroizing::new(r.token),expires:r.expires_in})
}
// Explicit legacy contract: never accept these shapes for platform-v1.
fn parse_legacy_challenge(raw:&[u8])->Result<Challenge>{
    #[derive(Deserialize)] #[serde(deny_unknown_fields)] struct Response{ok:bool,#[serde(default,rename="message")]_message:Option<String>}
    let r:Response=serde_json::from_slice(raw).map_err(|_|error("invalid_response","验证码响应无效，请稍后重试。"))?;
    if !r.ok{return Err(error("invalid_response","验证码请求未确认。"));}
    Ok(Challenge{id:Zeroizing::new(String::new()),retry_after:60})
}
fn parse_legacy_registration(raw:&[u8])->Result<Registration>{
    #[derive(Deserialize)] #[serde(deny_unknown_fields)] struct Response{ok:bool,uid:String,account_id:u64,#[serde(default,rename="message")]_message:Option<String>}
    let r:Response=serde_json::from_slice(raw).map_err(|_|error("invalid_response","注册结果未确认，请先尝试找回 UID，勿重复提交。"))?;
    if !r.ok||!valid_uid(&r.uid)||r.account_id==0{return Err(error("invalid_response","注册结果未返回有效身份。"));}
    Ok(Registration{uid:r.uid,provisioning_pending:false})
}
fn valid_legacy_reset_token(token:&str)->bool {token.len()==43&&token.bytes().all(|b|b.is_ascii_alphanumeric()||b==b'-'||b==b'_')}
fn parse_legacy_reset(raw:&[u8])->Result<ResetToken>{
    #[derive(Deserialize)] #[serde(deny_unknown_fields)] struct Response{ok:bool,#[serde(rename="resetToken")]token:String,#[serde(default,rename="message")]_message:Option<String>}
    let r:Response=serde_json::from_slice(raw).map_err(|_|error("invalid_response","重设密码响应无效。"))?;
    if !r.ok||!valid_legacy_reset_token(&r.token){return Err(error("invalid_response","重设密码凭据无效。"));}
    Ok(ResetToken{token:Zeroizing::new(r.token),expires:900})
}
fn network_error(e:reqwest::Error)->crate::AppError {
    if e.is_timeout(){error("timeout","连接超时，请稍后重试。")}
    else{error("secure_connection","无法建立安全连接，请检查网络、系统时间和证书。")}
}
pub fn status_error(status:u16)->crate::AppError {
    match status {
        400=>error("invalid_request","请求无效，请检查输入。"),
        401|403=>error("auth_failed","账号或密码错误，或账号暂不可用。"),
        404=>error("api_unavailable","接口尚未部署，请稍后重试。"),
        409=>error("not_ready","账号或游戏区尚未就绪，请稍后重试。"),
        429=>error("rate_limited","请求过于频繁，请稍后再试。"),
        300..=399=>error("redirect_blocked","账号接口发生重定向，已停止请求。"),
        _=>error("service_unavailable","账号服务暂不可用，请稍后重试。"),
    }
}
#[cfg(test)] mod tests {
    use super::*;
    #[test] fn http_date_is_parsed_for_server_time(){
        assert_eq!(http_date_seconds("Thu, 01 Jan 1970 00:00:00 GMT"),Some(0));
        assert_eq!(http_date_seconds("Wed, 14 Oct 2026 09:20:07 GMT"),Some(1791969607));
        assert_eq!(http_date_seconds("Wed, 14 Oct 2026 09:20:08 GMT"),Some(1791969608));
        assert_eq!(http_date_seconds("Sat, 29 Feb 2020 12:00:00 GMT"),Some(1582977600));
        assert_eq!(http_date_seconds("Fri, 31 Dec 1999 23:59:59 GMT"),Some(946684799));
        for bad in ["","Wed, 14 Oct 2026","14/10/2026 09:20:07","Wed, 32 Oct 2026 09:20:07 GMT","Wed, 14 Xxx 2026 09:20:07 GMT","Wed, 14 Oct 2026 24:20:07 GMT","Wed, 14 Oct 2026 09:20:07:08 GMT"]{assert!(http_date_seconds(bad).is_none(),"{bad}");}
    }
    #[test] fn trusted_now_uses_the_server_offset(){
        let c=AccountClient{http:reqwest::Client::new(),base:"https://example.test".into(),realm:None,legacy:true,skew:AtomicI64::new(0)};
        assert_eq!(c.trusted_now(),unix_now());
        c.skew.store(37,Ordering::Relaxed);
        assert_eq!(c.trusted_now(),unix_now()+37);
    }
    #[test] fn recovered_uid_keeps_zeroes_and_rejects_credentials(){
        assert_eq!(parse_recovered_uid(br#"{"ok":true,"uid":"00000001"}"#).unwrap(),"00000001");
        for raw in [br#"{"ok":true,"uid":"00000000"}"#.as_slice(),br#"{"ok":true,"uid":"1"}"#,br#"{"ok":true,"uid":"10000001","token":"forbidden"}"#]{assert!(parse_recovered_uid(raw).is_err());}
    }
    #[tokio::test] async fn recovery_http_contract_has_no_login_ticket_and_rate_limit_is_clear(){
        use std::io::{Read,Write};
        let listener=std::net::TcpListener::bind("127.0.0.1:0").unwrap();let base=format!("http://{}",listener.local_addr().unwrap());
        let server=std::thread::spawn(move||{
            for i in 0..4 {
                let (mut stream,_)=listener.accept().unwrap();stream.set_read_timeout(Some(Duration::from_secs(5))).unwrap();let mut raw=Vec::new();let mut buffer=[0u8;2048];
                let (end,len)=loop{let n=stream.read(&mut buffer).unwrap();assert!(n>0);raw.extend_from_slice(&buffer[..n]);if let Some(e)=raw.windows(4).position(|w|w==b"\r\n\r\n"){let header=String::from_utf8_lossy(&raw[..e]);assert!(header.starts_with("POST /api/account/recoverUid "));let length=header.lines().find_map(|line|line.to_lowercase().strip_prefix("content-length:").map(|x|x.trim().parse::<usize>().unwrap())).unwrap();break(e+4,length);}};
                while raw.len()<end+len{let n=stream.read(&mut buffer).unwrap();assert!(n>0);raw.extend_from_slice(&buffer[..n]);}
                let request:serde_json::Value=serde_json::from_slice(&raw[end..end+len]).unwrap();assert_eq!(request.as_object().unwrap().len(),2);assert!(request.get("email").is_some());assert!(request.get("password").is_some());
                let good=request["email"]=="test10000001@example.invalid"&&request["password"]=="fixture-password";
                let (status,body)=if i==3{("429 Too Many Requests",br#"{"ok":false,"error":"rate_limited"}"#.as_slice())}else if good{("200 OK",br#"{"ok":true,"uid":"00000001"}"#.as_slice())}else{("401 Unauthorized",br#"{"ok":false,"error":"auth_failed"}"#.as_slice())};
                write!(stream,"HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",body.len()).unwrap();stream.write_all(body).unwrap();
            }
        });
        let client=AccountClient{http:reqwest::Client::builder().no_proxy().timeout(Duration::from_secs(5)).build().unwrap(),base,realm:None,legacy:true,skew:AtomicI64::new(0)};
        assert_eq!(client.recover_uid(" test10000001@example.invalid ",Zeroizing::new("fixture-password".into())).await.unwrap(),"00000001");
        for (email,password) in [("missing@example.invalid","fixture-password"),("test10000001@example.invalid","wrong")]{assert_eq!(client.recover_uid(email,Zeroizing::new(password.into())).await.err().unwrap().code,"auth_failed");}
        assert_eq!(client.recover_uid("test10000001@example.invalid",Zeroizing::new("fixture-password".into())).await.err().unwrap().code,"rate_limited");server.join().unwrap();
    }
    #[tokio::test]
    async fn legacy_uid_login_rejects_email_before_network() {
        use std::io::{Read,Write};
        use base64::Engine;
        let listener=std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let base=format!("http://{}",listener.local_addr().unwrap());
        let server=std::thread::spawn(move||{
            for _ in 0..3 {
                let (mut stream,_)=listener.accept().unwrap();stream.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
                let mut bytes=Vec::new();let mut buffer=[0u8;2048];
                let (boundary,length)=loop{let n=stream.read(&mut buffer).unwrap();assert!(n>0);bytes.extend_from_slice(&buffer[..n]);if let Some(end)=bytes.windows(4).position(|w|w==b"\r\n\r\n"){let headers=String::from_utf8_lossy(&bytes[..end]);let length=headers.lines().find_map(|line|line.to_lowercase().strip_prefix("content-length:").map(|v|v.trim().parse::<usize>().unwrap())).unwrap();break(end+4,length);}};
                while bytes.len()<boundary+length {let n=stream.read(&mut buffer).unwrap();assert!(n>0);bytes.extend_from_slice(&buffer[..n]);}
                let request:serde_json::Value=serde_json::from_slice(&bytes[boundary..boundary+length]).unwrap();
                let valid=request["password"]=="isolated-password"&&request["username"]=="10000001";
                let body=if valid {let token=base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(format!("2.{}.{}.{}",unix_now()+300,"A".repeat(32),"B".repeat(64)));serde_json::json!({"ok":true,"uid":"10000001","account_id":2,"token":token})}else{serde_json::json!({"ok":false,"error":"auth_failed"})};
                let body=serde_json::to_vec(&body).unwrap();write!(stream,"HTTP/1.1 {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",if valid{"200 OK"}else{"401 Unauthorized"},body.len()).unwrap();stream.write_all(&body).unwrap();
            }
        });
        let account=AccountClient{http:reqwest::Client::builder().no_proxy().timeout(Duration::from_secs(5)).build().unwrap(),base,realm:None,legacy:true,skew:AtomicI64::new(0)};
        let uid=account.legacy_ticket("10000001",Zeroizing::new("isolated-password".into())).await.unwrap();
        assert_eq!(account.legacy_ticket("test10000001@example.invalid",Zeroizing::new("isolated-password".into())).await.err().unwrap().code,"invalid_identifier");
        assert_eq!(uid.uid,"10000001");
        for (name,password) in [("10000002","isolated-password"),("10000001","wrong-password")]{assert_eq!(account.legacy_ticket(name,Zeroizing::new(password.into())).await.err().unwrap().code,"auth_failed");}
        server.join().unwrap();
    }
    fn http_date(seconds:u64)->String{
        const WEEK:[&str;7]=["Thu","Fri","Sat","Sun","Mon","Tue","Wed"];
        const MONTH:[&str;12]=["Jan","Feb","Mar","Apr","May","Jun","Jul","Aug","Sep","Oct","Nov","Dec"];
        let days=(seconds/86_400) as i64;let clock=seconds%86_400;
        let z=days+719_468;let era=z.div_euclid(146_097);let doe=z.rem_euclid(146_097);
        let yoe=(doe-doe/1460+doe/36_524-doe/146_096)/365;let year=yoe+era*400;
        let doy=doe-(365*yoe+yoe/4-yoe/100);let mp=(5*doy+2)/153;
        let day=doy-(153*mp+2)/5+1;let month=if mp<10{mp+3}else{mp-9};let year=if month<=2{year+1}else{year};
        format!("{}, {:02} {} {} {:02}:{:02}:{:02} GMT",WEEK[days.rem_euclid(7) as usize],day,MONTH[(month-1) as usize],year,clock/3600,(clock%3600)/60,clock%60)
    }
    #[tokio::test]
    async fn legacy_login_uses_server_date_when_local_clock_is_skewed(){
        use std::io::{Read,Write};
        use base64::Engine;
        for offset in [3600i64,-3600] {
            let listener=std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let base=format!("http://{}",listener.local_addr().unwrap());
            let server=std::thread::spawn(move||{
                let (mut stream,_)=listener.accept().unwrap();stream.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
                let mut bytes=Vec::new();let mut buffer=[0u8;2048];
                let (boundary,length)=loop{let n=stream.read(&mut buffer).unwrap();assert!(n>0);bytes.extend_from_slice(&buffer[..n]);if let Some(end)=bytes.windows(4).position(|w|w==b"\r\n\r\n"){let headers=String::from_utf8_lossy(&bytes[..end]);let length=headers.lines().find_map(|line|line.to_lowercase().strip_prefix("content-length:").map(|v|v.trim().parse::<usize>().unwrap())).unwrap();break(end+4,length);}};
                while bytes.len()<boundary+length {let n=stream.read(&mut buffer).unwrap();assert!(n>0);bytes.extend_from_slice(&buffer[..n]);}
                // The fixture answers as a server whose clock differs from this machine by `offset`
                // seconds, and mints a token that is valid for 300 seconds of ITS OWN time. Without
                // the Date header (offset 0) the token would be 3600s away from the local clock and
                // the 5..310s window in legacy_online::parse would reject it.
                let server_now=(unix_now() as i64+offset) as u64;
                let token=base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(format!("2.{}.{}.{}",server_now+300,"A".repeat(32),"B".repeat(64)));
                let body=serde_json::to_vec(&serde_json::json!({"ok":true,"uid":"10000001","account_id":2,"token":token})).unwrap();
                write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nDate: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",http_date(server_now),body.len()).unwrap();stream.write_all(&body).unwrap();
                server_now
            });
            let account=AccountClient{http:reqwest::Client::builder().no_proxy().timeout(Duration::from_secs(5)).build().unwrap(),base,realm:None,legacy:true,skew:AtomicI64::new(0)};
            let ticket=account.legacy_ticket("10000001",Zeroizing::new("isolated-password".into())).await.unwrap();
            assert_eq!(ticket.uid,"10000001");
            let server_now=server.join().unwrap();
            assert!((account.trusted_now() as i64-server_now as i64).abs()<=2,"trusted time must follow the server Date header, not the local clock");
            assert!((account.skew.load(Ordering::Relaxed)-offset).abs()<=2,"measured skew must match the fixture offset");
        }
    }
    #[tokio::test]
    #[ignore = "requires isolated HTTPS account fixture and fixture CA"]
    async fn configured_https_contract() {
        let raw=std::fs::read(std::env::var("OO_TEST_PUBLIC_CONFIG").unwrap()).unwrap();
        let config=crate::public_config::PublicConfig::parse(&raw).unwrap();
        assert_eq!(reqwest::Url::parse(&config.account_base_url).unwrap().host_str(),Some("127.0.0.1"));
        let mut account=AccountClient::configured(&config.account_base_url,Some(config.realm_id)).unwrap();
        let ca=std::fs::read(std::env::var("OO_TEST_CA").unwrap()).unwrap();
        account.http=reqwest::Client::builder().https_only(true).no_proxy().redirect(reqwest::redirect::Policy::none()).timeout(Duration::from_secs(10)).add_root_certificate(reqwest::Certificate::from_pem(&ca).unwrap()).build().unwrap();
        account.status().await.unwrap();
        let uid=std::env::var("OO_TEST_UID").unwrap();
        let session=account.login(&uid,Zeroizing::new(std::env::var("OO_TEST_PASSWORD").unwrap())).await.unwrap();
        assert_eq!(session.uid,uid);
        let ticket=account.ticket(&session).await.unwrap();assert_eq!(ticket.uid,uid);assert!(ticket.token.as_str()!=session.token.as_str());
        account.logout(&session).await.unwrap();
        assert!(account.ticket(&session).await.is_err());
    }
    fn session()->Vec<u8>{serde_json::to_vec(&serde_json::json!({"ok":true,"uid":"10000001","sessionToken":"A".repeat(43),"expires_in":3600,"protocol":"platform-session-v1"})).unwrap()}
    fn ticket(uid:&str,expires:u64)->Vec<u8>{serde_json::to_vec(&serde_json::json!({"ok":true,"uid":uid,"ticket":"B".repeat(43),"expires_in":expires,"protocol":"platform-ticket-v1"})).unwrap()}
    #[test] fn distinct_session_and_ticket_protocols(){
        assert_eq!(parse_session(&session(),10).unwrap().expires,3610);
        assert!(parse_ticket(&session(),10,"10000001").is_err());
        assert!(parse_session(&ticket("10000001",60),10).is_err());
        assert_eq!(parse_ticket(&ticket("10000001",60),10,"10000001").unwrap().expires,70);
    }
    #[test] fn rejects_mismatch_expiry_and_legacy(){
        assert!(parse_ticket(&ticket("10000002",60),10,"10000001").is_err());
        assert!(parse_ticket(&ticket("10000001",3600),10,"10000001").is_err());
        assert!(parse_ticket(&ticket("10000001",0),10,"10000001").is_err());
        assert!(parse_session(br#"{"ok":true,"token":"legacy","account_id":3,"uid":"10000001"}"#,10).is_err());
        assert!(!valid_token("abc?def"));
    }
    #[tokio::test]
    #[ignore = "requires isolated Platform HTTP fixture"]
    async fn platform_http_contract() {
        let base=std::env::var("OO_TEST_PLATFORM_URL").expect("isolated fixture URL");
        let url=reqwest::Url::parse(&base).unwrap();
        assert_eq!(url.host_str(),Some("127.0.0.1"));assert_eq!(url.scheme(),"http");
        let mut headers=reqwest::header::HeaderMap::new();
        let mut key=reqwest::header::HeaderValue::from_str(&std::env::var("OO_TEST_PLATFORM_KEY").unwrap()).unwrap();key.set_sensitive(true);
        headers.insert("X-Arad-Service-Key",key);headers.insert("X-Arad-Client-IP",reqwest::header::HeaderValue::from_static("203.0.113.188"));
        let http=reqwest::Client::builder().no_proxy().default_headers(headers).timeout(Duration::from_secs(10)).redirect(reqwest::redirect::Policy::none()).build().unwrap();
        let account=AccountClient{http,base,realm:None,legacy:false,skew:AtomicI64::new(0)};
        let uid=std::env::var("OO_TEST_UID").unwrap();
        let password=Zeroizing::new(std::env::var("OO_TEST_PASSWORD").unwrap());
        account.status().await.unwrap();
        let session=account.login(&uid,password).await.unwrap();assert_eq!(session.uid,uid);
        let first=account.ticket(&session).await.unwrap();let second=account.ticket(&session).await.unwrap();
        assert!(first.token.as_str()!=second.token.as_str());assert_eq!(first.uid,uid);
        account.logout(&session).await.unwrap();
        let error=account.ticket(&session).await.err().expect("revoked session rejected");assert_eq!(error.code,"login_required");
    }
    #[test] fn rejects_invalid_input(){assert!(validate_input("123?45678","p").is_err());assert!(validate_input("10000001","").is_err());}
    #[test] fn uid_preserves_leading_zeroes(){
        assert!(validate_input("00000001","p").is_ok());
        assert!(validate_input("00000000","p").is_err());
        assert_eq!(parse_ticket(&ticket("00000001",60),10,"00000001").unwrap().uid,"00000001");
    }
    #[test] fn legacy_registration_contract_is_explicit_and_never_accepts_tokens(){
        let code=br#"{"ok":true,"message":"sent"}"#;
        assert_eq!(parse_legacy_challenge(code).unwrap().retry_after,60);
        assert!(parse_challenge(code).is_err());
        let registration=br#"{"ok":true,"uid":"11111112","account_id":7,"message":"registered"}"#;
        assert_eq!(parse_legacy_registration(registration).unwrap().uid,"11111112");
        assert!(parse_registration(registration).is_err());
        assert!(parse_legacy_registration(br#"{"ok":true,"uid":"11111112","account_id":7,"token":"secret"}"#).is_err());
        let reset=format!(r#"{{"ok":true,"resetToken":"{}","message":"verified"}}"#,"A".repeat(43));
        assert!(parse_legacy_reset(reset.as_bytes()).is_ok());
        assert!(parse_reset_token(reset.as_bytes()).is_err());
    }
    const CHALLENGE_ID:&str="01234567-89ab-cdef-0123-456789abcdef";
    fn reset_token_fixture()->String {format!("{CHALLENGE_ID}.{}","Z".repeat(64))}
    #[test] fn challenge_registration_and_reset_responses_are_parsed_strictly(){
        let challenge=parse_challenge(format!(r#"{{"ok":true,"challenge_id":"{CHALLENGE_ID}","retry_after":60,"delivery":"queued"}}"#).as_bytes()).unwrap();
        assert_eq!(challenge.id.as_str(),CHALLENGE_ID);assert_eq!(challenge.retry_after,60);
        for raw in [r#"{"ok":true,"challenge_id":"short","retry_after":60,"delivery":"queued"}"#,r#"{"ok":true,"challenge_id":"01234567-89ab-cdef-0123-456789abcdef","retry_after":60,"delivery":"failed"}"#,r#"{"ok":false,"error":"invalid_email"}"#]{assert!(parse_challenge(raw.as_bytes()).is_err(),"{raw}");}
        let registered=parse_registration(br#"{"ok":true,"uid":"11111111","provisioning":"pending"}"#).unwrap();
        assert_eq!(registered.uid,"11111111");assert!(registered.provisioning_pending);
        // 注册响应里出现任何凭据字段都是协议错误，不能被静默忽略。
        for raw in [br#"{"ok":true,"uid":"11111111","provisioning":"pending","token":"leak"}"#.as_slice(),br#"{"ok":true,"uid":"00000000","provisioning":"pending"}"#,br#"{"ok":true,"uid":"1","provisioning":"pending"}"#]{assert!(parse_registration(raw).is_err());}
        let token=parse_reset_token(format!(r#"{{"ok":true,"resetToken":"{}","expires_in":900}}"#,reset_token_fixture()).as_bytes()).unwrap();
        assert_eq!(token.token.as_str(),reset_token_fixture());assert_eq!(token.expires,900);
        for raw in [br#"{"ok":true,"resetToken":"short","expires_in":900}"#.as_slice(),br#"{"ok":true,"resetToken":"01234567-89ab-cdef-0123-456789abcdef.ZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZ","expires_in":0}"#,br#"{"ok":true,"resetToken":"01234567-89ab-cdef-0123-456789abcdef.ZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZ","expires_in":900,"password":"leak"}"#]{assert!(parse_reset_token(raw).is_err());}
    }
    #[test] fn registration_input_is_pre_checked_before_the_network(){
        assert_eq!(validate_email(" Test10000001@Example.INVALID ").unwrap(),"test10000001@example.invalid");
        for bad in ["","plain","a@b","@example.com","a@.com","a@com","a b@example.com","a@exa mple.com","a@example..com",".a@example.com"]{assert!(validate_email(bad).is_err(),"{bad}");}
        assert!(validate_new_password("123456").is_ok());
        assert!(validate_new_password("12345").is_err());
        assert!(validate_new_password(&"a".repeat(129)).is_err());
        assert!(validate_new_password("12345\u{7}").is_err());
        assert!(validate_code("123456").is_ok());
        for bad in ["12345","1234567","12345a","","12 456"]{assert!(validate_code(bad).is_err(),"{bad}");}
        assert!(validate_reset_token(&reset_token_fixture()).is_ok());
        assert!(validate_reset_token("nope").is_err());
    }
    #[tokio::test]
    async fn registration_http_contract_returns_no_credentials_and_reports_cooldown(){
        use std::io::{Read,Write};
        let listener=std::net::TcpListener::bind("127.0.0.1:0").unwrap();let base=format!("http://{}",listener.local_addr().unwrap());
        let token=reset_token_fixture();
        let server=std::thread::spawn(move||{
            let expectations:Vec<(&str,Vec<&str>)>=vec![
                ("/api/account/sendCode",vec!["email","purpose"]),
                ("/api/account/register",vec!["email","code","password","challenge_id"]),
                ("/api/account/forgotPassword",vec!["email","code","challenge_id"]),
                ("/api/account/resetPassword",vec!["resetToken","newPassword"]),
                ("/api/account/sendCode",vec!["email","purpose"]),
            ];
            for (index,(path,fields)) in expectations.into_iter().enumerate(){
                let (mut stream,_)=listener.accept().unwrap();stream.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
                let mut raw=Vec::new();let mut buffer=[0u8;2048];
                let (end,length)=loop{let n=stream.read(&mut buffer).unwrap();assert!(n>0);raw.extend_from_slice(&buffer[..n]);if let Some(e)=raw.windows(4).position(|w|w==b"\r\n\r\n"){let header=String::from_utf8_lossy(&raw[..e]);let length=header.lines().find_map(|line|line.to_lowercase().strip_prefix("content-length:").map(|x|x.trim().parse::<usize>().unwrap())).unwrap();assert!(header.starts_with(&format!("POST {path} ")),"{header}");break(e+4,length);}};
                while raw.len()<end+length{let n=stream.read(&mut buffer).unwrap();assert!(n>0);raw.extend_from_slice(&buffer[..n]);}
                let request:serde_json::Value=serde_json::from_slice(&raw[end..end+length]).unwrap();
                let object=request.as_object().unwrap();
                assert_eq!(object.len(),fields.len(),"{path}");
                for field in fields.iter().copied(){assert!(object.contains_key(field),"{path} missing {field}");}
                if fields.contains(&"email"){assert_eq!(request["email"].as_str().map(|v|v.to_lowercase()),Some(String::from("test10000001@example.invalid")),"{path}");}
                if path=="/api/account/register"{assert_eq!(request["challenge_id"],CHALLENGE_ID);assert_eq!(request["code"],"123456");assert_eq!(request["password"],"fixture-password");assert_eq!(request["purpose"],serde_json::Value::Null);}
                if path=="/api/account/sendCode"{assert_eq!(request["purpose"],if index==0{"register"}else{"reset"});}
                if path=="/api/account/forgotPassword"{assert_eq!(request["challenge_id"],CHALLENGE_ID);}
                if path=="/api/account/resetPassword"{assert_eq!(request["newPassword"],"fixture-password");}
                let (status,reason,body)=match index {
                    0=>(202,"Accepted",format!(r#"{{"ok":true,"challenge_id":"{CHALLENGE_ID}","retry_after":60,"delivery":"queued"}}"#)),
                    1=>(201,"Created",String::from(r#"{"ok":true,"uid":"11111111","provisioning":"pending"}"#)),
                    2=>(200,"OK",format!(r#"{{"ok":true,"resetToken":"{token}","expires_in":900}}"#)),
                    3=>(200,"OK",String::from(r#"{"ok":true}"#)),
                    _=>(429,"Too Many Requests",String::from(r#"{"ok":false,"error":"cooldown","retry_after":42}"#)),
                };
                write!(stream,"HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",body.len()).unwrap();stream.write_all(body.as_bytes()).unwrap();
            }
        });
        let token=reset_token_fixture();
        let account=AccountClient{http:reqwest::Client::builder().no_proxy().timeout(Duration::from_secs(5)).build().unwrap(),base,realm:None,legacy:false,skew:AtomicI64::new(0)};
        let challenge=account.send_code(" Test10000001@Example.invalid ",CodePurpose::Register).await.unwrap();
        let registered=account.register("test10000001@example.invalid","123456",Zeroizing::new("fixture-password".into()),&challenge).await.unwrap();
        assert_eq!(registered.uid,"11111111");assert!(registered.provisioning_pending);
        let reset=account.request_password_reset("test10000001@example.invalid","123456",&challenge).await.unwrap();
        assert_eq!(reset.token.as_str(),token);assert_eq!(reset.expires,900);
        account.reset_password(&reset.token,Zeroizing::new("fixture-password".into())).await.unwrap();
        let limited=account.send_code("test10000001@example.invalid",CodePurpose::Reset).await.err().unwrap();
        assert_eq!(limited.code,"rate_limited");assert_eq!(limited.retry_after,Some(42));
        server.join().unwrap();
    }
}
