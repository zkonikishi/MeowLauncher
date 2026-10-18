use crate::{error, Result};
use serde::{Deserialize, Serialize};
use std::{collections::HashSet, time::Duration};
const SOURCE: &str="https://launcher.invalid/launcher/content.json";
#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all="camelCase")]
pub struct Content {pub schema_version:u32,pub updated_at:String,pub items:Vec<Article>,pub maintenance:Maintenance}
#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all="camelCase")]
pub struct Article {pub id:String,pub category:String,pub title:String,pub summary:String,pub published_at:String,pub url:String,pub realms:Vec<String>}
#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all="camelCase")]
pub struct Maintenance {pub status:String,pub updated_at:String,pub realms:Vec<String>,pub planned_start_at:Option<String>,pub planned_end_at:Option<String>,pub actual_start_at:Option<String>,pub actual_end_at:Option<String>,pub notice_id:Option<String>}
pub fn news_url(value:&str)->Result<reqwest::Url> {
    news_url_for(value,"https://launcher.invalid")
}
pub fn news_url_for(value:&str,website:&str)->Result<reqwest::Url> {
    let origin=crate::public_config::https_url(website)?;
    let url=reqwest::Url::parse(value).map_err(|_|error("invalid_url","资讯地址无效。"))?;
    if url.scheme()!="https" || url.origin()!=origin.origin() || !url.username().is_empty() || url.password().is_some() || url.path()!="/news/" {
        return Err(error("invalid_url","仅允许打开官网资讯页面。"));
    }
    Ok(url)
}
pub fn parse(raw:&[u8])->Result<Content> {
    parse_for(raw,"https://launcher.invalid")
}
pub fn parse_for(raw:&[u8],website:&str)->Result<Content> {
    let value:Content=serde_json::from_slice(raw).map_err(|_|error("content_invalid","官网资讯格式无效。"))?;
    if value.schema_version!=1 || value.updated_at.len()>64 || value.items.len()>200 || !matches!(value.maintenance.status.as_str(),"unknown"|"scheduled"|"active"|"resolved"|"cancelled") {return Err(error("content_invalid","官网资讯版本或状态无效。"));}
    let mut ids=HashSet::new();
    for article in &value.items {
        if article.id.is_empty() || article.id.len()>100 || !ids.insert(article.id.clone()) || article.title.is_empty() || article.title.len()>400 || article.summary.len()>4000 || !matches!(article.category.as_str(),"notice"|"news"|"activity"|"maintenance"|"update") || article.published_at.len()>64 || article.realms.len()>20 {return Err(error("content_invalid","官网资讯条目无效。"));}
        news_url_for(&article.url,website)?;
    }
    Ok(value)
}
/// 只用编译期固定的 Meowopia 资讯源。通用版必须通过发行配置提供 `contentUrl`。
pub async fn fetch()->Result<Content> {
    if crate::edition::is_sponsor(){return Err(error("public_config_required","通用版未配置官网资讯地址，请在发行配置中填写。"))}
    fetch_from(SOURCE,"https://launcher.invalid").await
}
pub async fn fetch_from(source:&str,website:&str)->Result<Content> {
    crate::public_config::https_url(source)?;
    let client=reqwest::Client::builder().https_only(true).no_proxy().redirect(reqwest::redirect::Policy::none()).connect_timeout(Duration::from_secs(5)).timeout(Duration::from_secs(10)).build().map_err(|_|error("content_unavailable","无法连接官网资讯。"))?;
    let mut response=client.get(source).send().await.map_err(|_|error("content_unavailable","官网资讯加载失败，请稍后重试。"))?;
    if !response.status().is_success(){return Err(error("content_unavailable","官网资讯接口暂不可用。"));}
    let mut bytes=Vec::new();
    while let Some(chunk)=response.chunk().await.map_err(|_|error("content_unavailable","官网资讯读取失败。"))? {
        if bytes.len()+chunk.len()>524288{return Err(error("content_invalid","官网资讯过大。"));}bytes.extend_from_slice(&chunk);
    }
    parse_for(&bytes,website)
}
#[cfg(test)] mod tests {
    use super::*;
    #[test] fn configured_website_controls_news_allowlist() {
        let raw=String::from_utf8(include_bytes!("../fixtures/content.json").to_vec()).unwrap().replace("https://launcher.invalid/news/","https://example.com/news/");
        assert!(parse_for(raw.as_bytes(),"https://example.com").is_ok());
        assert!(parse(raw.as_bytes()).is_err());
        assert!(news_url_for("https://example.com.evil.test/news/","https://example.com").is_err());
    }
    #[test] fn only_official_news_links(){
        assert!(news_url("https://launcher.invalid/news/?article=one").is_ok());
        for value in ["javascript:alert(1)","https://launcher.invalid.evil.test/news/","https://evil@launcher.invalid/news/","https://launcher.invalid/account/register/","http://launcher.invalid/news/"]{assert!(news_url(value).is_err());}
    }
    #[test] fn activity_category_is_accepted() {
        let mut value:serde_json::Value=serde_json::from_slice(include_bytes!("../fixtures/content.json")).unwrap();
        value["items"][0]["category"]=serde_json::json!("activity");
        assert!(parse(&serde_json::to_vec(&value).unwrap()).is_ok());
        value["items"][0]["category"]=serde_json::json!("unsupported-category");
        assert!(parse(&serde_json::to_vec(&value).unwrap()).is_err());
    }
    #[test] fn actual_website_contract(){
        let raw=include_bytes!("../fixtures/content.json");
        let value=parse(raw).unwrap();assert_eq!(value.schema_version,1);assert!(!value.items.is_empty());
    }
}
