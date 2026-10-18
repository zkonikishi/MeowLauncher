//! Player-session website exchange, distinct from host Authority service token.
use crate::{controlled::{Challenge,Scope},error,Result};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD,Engine};
use serde::{Deserialize,Serialize};
use std::time::Duration;
use zeroize::Zeroizing;
fn rejected()->crate::AppError{error("controlled_exchange","平台游戏准入未完成，请稍后重试。")}
fn encode_session(session:&str)->Result<Zeroizing<String>>{
 // IdentityApi hashes this exact ASCII token string. Hex-decoding it would
 // select different session bytes and must never be done here.
 if session.len()!=64||!session.bytes().all(|b|b.is_ascii_hexdigit()){return Err(rejected());}
 Ok(Zeroizing::new(URL_SAFE_NO_PAD.encode(session.as_bytes())))
}
pub struct ExchangeClient{client:reqwest::Client,url:reqwest::Url,scope:Scope}
#[derive(Serialize)]struct Request<'a>{challenge:&'a Challenge}
#[derive(Deserialize)]#[serde(deny_unknown_fields)]struct Response{assertion:String}
fn parse(raw:&[u8])->Result<Zeroizing<String>>{
 if raw.len()>65536{return Err(rejected());}
 let r:Response=serde_json::from_slice(raw).map_err(|_|rejected())?;
 let assertion=Zeroizing::new(r.assertion);
 if assertion.len()!=46||!assertion.starts_with("a1.")||!assertion.as_bytes()[3..].iter().all(|b|b.is_ascii_alphanumeric()||*b==b'-'||*b==b'_'){return Err(rejected());}
 Ok(assertion)
}
impl ExchangeClient{
 /// Exact endpoint comes from separately trusted platform configuration. No
 /// descriptor/challenge URL, redirects, ambient proxy or HTTP fallback.
 pub fn new(endpoint:&str,scope:Scope,ca:Option<reqwest::Certificate>)->Result<Self>{
  scope.validate()?;let url=reqwest::Url::parse(endpoint).map_err(|_|rejected())?;
  if url.scheme()!="https"||url.host_str().is_none()||!url.username().is_empty()||url.password().is_some()||url.query().is_some()||url.fragment().is_some()||url.path()!="/v1/exchange"{return Err(rejected());}
  let mut builder=reqwest::Client::builder().user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) MeowLauncherMax/0.7.1").https_only(true).redirect(reqwest::redirect::Policy::none()).no_proxy().connect_timeout(Duration::from_secs(3)).timeout(Duration::from_secs(20)).min_tls_version(reqwest::tls::Version::TLS_1_3);
  if let Some(ca)=ca{builder=builder.add_root_certificate(ca);}
  let client=builder.build().map_err(|_|rejected())?;Ok(Self{client,url,scope})
 }
 /// Production IdentityApi sessionToken is exactly 64 ASCII hex characters.
 /// Encode its UTF-8 string once, preserving case; never hex-decode the token.
 pub async fn exchange(&self,challenge:&Challenge,session:&str)->Result<Zeroizing<String>>{
  if !self.scope.accepts(challenge){return Err(rejected());}
  let encoded=encode_session(session)?;
  let mut header=reqwest::header::HeaderValue::from_str(&encoded).map_err(|_|rejected())?;header.set_sensitive(true);
  let body=Zeroizing::new(serde_json::to_vec(&Request{challenge}).map_err(|_|rejected())?);
  if body.len()>65536{return Err(rejected());}
  let mut response=self.client.post(self.url.clone()).header("X-Launcher-Session",header).header(reqwest::header::CONTENT_TYPE,"application/json").body(body.to_vec()).send().await.map_err(|e|if e.is_timeout(){error("controlled_exchange_timeout","游戏准入响应超时，请稍后重试，无需重新登录。")}else{error("controlled_exchange_network","无法连接游戏准入服务，请检查网络后重试。")})?;
  if response.status()==reqwest::StatusCode::UNAUTHORIZED{return Err(error("login_required","登录已失效，请重新登录。"));}
  if response.status()!=reqwest::StatusCode::OK{return Err(error("controlled_exchange_http","游戏准入服务暂不可用，请稍后重试。"));}
  if response.content_length().is_some_and(|n|n>65536){return Err(rejected());}
  let mut raw=Zeroizing::new(Vec::new());
  while let Some(chunk)=response.chunk().await.map_err(|_|rejected())?{if raw.len()+chunk.len()>65536{return Err(rejected());}raw.extend_from_slice(&chunk);}
  parse(&raw)
 }
}
#[cfg(test)]mod tests{
 use super::*;
 fn scope()->Scope{Scope{host_id:"host".into(),issuer:"issuer".into(),audience:"game".into(),realm_id:"realm".into(),version_id:"115us".into()}}
 #[test]fn rejects_http_credentials_redirect_parameters_and_wrong_routes(){for u in ["http://127.0.0.1/v1/exchange","https://user:pass@example.invalid/v1/exchange","https://example.invalid/v1/exchange?q=x","https://example.invalid/v1/exchange#x","https://example.invalid/v1/consume"]{assert!(ExchangeClient::new(u,scope(),None).is_err());}assert!(ExchangeClient::new("https://account.example.invalid/v1/exchange",scope(),None).is_ok());}
 #[test]fn strict_bounded_assertion_response(){let raw=format!(r#"{{"assertion":"a1.{}"}}"#,"x".repeat(43));assert!(parse(raw.as_bytes()).is_ok());for r in [r#"{"assertion":"a","assertion":"b"}"#,r#"{"assertion":"bad"}"#,r#"{"assertion":"bad","uid":"10000001"}"#]{assert!(parse(r.as_bytes()).is_err());}assert!(parse(&vec![b' ';65537]).is_err());}
 #[test]fn production_session_encodes_original_64_character_string_not_decoded_hex(){let token="aB12".repeat(16);let encoded=encode_session(&token).unwrap();let decoded=URL_SAFE_NO_PAD.decode(encoded.as_bytes()).unwrap();assert_eq!(decoded,token.as_bytes());assert_eq!(decoded.len(),64);assert!(!encoded.contains('='));for bad in ["ab".repeat(16),"g".repeat(64),"a".repeat(63),format!("{}\n","a".repeat(63))]{assert!(encode_session(&bad).is_err());}}
}
