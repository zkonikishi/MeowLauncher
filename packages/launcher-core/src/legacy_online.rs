//! Explicit compatibility with the currently deployed one-use-token service.
//! Never selected as a fallback from platform-v1.
use crate::{account::Ticket,error,Result};
use base64::{Engine,engine::general_purpose::URL_SAFE_NO_PAD};
use serde::Deserialize;
use zeroize::{Zeroize,Zeroizing};
#[derive(Deserialize)]
struct Response {ok:bool,uid:Option<String>,account_id:i32,token:String}
impl Drop for Response {fn drop(&mut self){self.token.zeroize();}}
pub fn parse(raw:&[u8],now:u64,identifier:&str)->Result<Ticket> {
    crate::account::validate_input(identifier,"response-validation")?;
    let mut r:Response=serde_json::from_slice(raw).map_err(|_|error("legacy_response","线上兼容接口响应无效。"))?;
    let identifier=identifier.trim();
    let uid=if identifier.len()==8&&identifier.bytes().all(|b|b.is_ascii_digit()) {
        if r.uid.as_ref().is_some_and(|u|u!=identifier){return Err(error("legacy_response","返回账号与登录 UID 不匹配。"));}
        identifier.to_owned()
    } else {
        r.uid.clone().ok_or_else(||error("email_login_unavailable","账号服务尚未返回邮箱对应的 UID，请使用 UID 登录或稍后重试。"))?
    };
    if !r.ok || r.account_id<1 || uid.len()!=8 || uid=="00000000" || !uid.bytes().all(|b|b.is_ascii_digit()) || !(32..=2048).contains(&r.token.len()) || !r.token.bytes().all(|b|b.is_ascii_alphanumeric()||b==b'-'||b==b'_') {return Err(error("legacy_response","账号或一次性凭据无效。"));}
    let payload=Zeroizing::new(URL_SAFE_NO_PAD.decode(&r.token).map_err(|_|error("legacy_response","一次性凭据格式无效。"))?);
    let text=std::str::from_utf8(&payload).map_err(|_|error("legacy_response","一次性凭据格式无效。"))?;
    let parts:Vec<_>=text.split('.').collect();
    if parts.len()!=4 || parts[0].parse::<i32>().ok()!=Some(r.account_id) || parts[2].len()!=32 || parts[3].len()!=64 || !parts[2].bytes().chain(parts[3].bytes()).all(|b|b.is_ascii_hexdigit()) {return Err(error("legacy_response","一次性凭据与账号不匹配。"));}
    let expires=parts[1].parse::<u64>().map_err(|_|error("legacy_response","凭据有效期无效。"))?;
    if expires<=now+5 || expires>now+310 {return Err(error("token_expired","一次性凭据已过期或服务器时间异常，请重新登录。"));}
    // Email resolution belongs to the authenticated service; game arguments always use its UID.
    // Only UID requests may read the historical response without a UID field.
    Ok(Ticket{uid,token:Zeroizing::new(std::mem::take(&mut r.token)),expires})
}
#[cfg(test)] mod tests {
    use super::*;
    fn response(id:i32,expiry:u64)->Vec<u8>{let token=URL_SAFE_NO_PAD.encode(format!("2.{expiry}.{}.{}","A".repeat(32),"B".repeat(64)));serde_json::to_vec(&serde_json::json!({"ok":true,"uid":"10000002","account_id":id,"token":token})).unwrap()}
    #[test] fn validates_binding_format_and_short_expiry(){assert_eq!(parse(&response(2,1300),1000,"10000002").unwrap().uid,"10000002");for (id,expiry) in [(3,1300),(2,1001),(2,1400)]{assert!(parse(&response(id,expiry),1000,"10000002").is_err());}assert!(parse(br#"{"ok":true,"sessionToken":"platform"}"#,1000,"10000002").is_err());assert!(parse(&response(2,1300),1000,"10000003").is_err());let mut old:serde_json::Value=serde_json::from_slice(&response(2,1300)).unwrap();old.as_object_mut().unwrap().remove("uid");assert_eq!(parse(&serde_json::to_vec(&old).unwrap(),1000,"10000002").unwrap().uid,"10000002");assert!(parse(&serde_json::to_vec(&old).unwrap(),1000,"x@example.com").is_err());}
    #[test] fn email_requires_server_uid_and_same_account_token(){
        assert_eq!(parse(&response(2,1300),1000," X@Example.com ").unwrap().uid,"10000002");
        assert!(parse(&response(3,1300),1000,"x@example.com").is_err());
        for uid in ["", "2", "00000000", "abcdefgh"]{let mut r:serde_json::Value=serde_json::from_slice(&response(2,1300)).unwrap();r["uid"]=uid.into();assert!(parse(&serde_json::to_vec(&r).unwrap(),1000,"x@example.com").is_err());}
    }
}
