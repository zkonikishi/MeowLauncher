//! Private repository authorization stays on the website, never in the EXE.
use crate::{error, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest,Sha256};
use std::{io::{Read,Write},path::{Path,PathBuf},sync::{Mutex,atomic::{AtomicBool,Ordering}},time::Duration};
pub const EDITION:&str=crate::edition::ID;
pub const NOTES:&str=crate::edition::NOTES;
const LEGACY_NOTES:&str="OOArad为Meowopia服务器定制版";
/// Notes accepted from a release manifest. The custom edition keeps its own
/// string plus the historical one; the generic edition accepts its own label or
/// the bare product name so a server owner does not have to copy a magic string.
fn accepts_notes(value:&str)->bool {
 if [NOTES,LEGACY_NOTES].contains(&value){return true;}
 // The generic edition also accepts the bare product name so a server owner
 // does not have to copy an exact Chinese label into his own feed.
 !crate::edition::is_custom() && ["MeowLauncher","MeowLauncher 赞助版"].contains(&value)
}
/// Update feed base for the custom edition. The generic edition must supply
/// `updateBaseUrl` in `launcher.public.json` instead.
pub const BASE_URL:&str=crate::edition::MEOWOPIA_ORIGIN;
pub fn manifest_url(base:&str)->String {format!("{base}/launcher/meow-update.json")}
#[derive(Clone,Copy,Debug,PartialEq)]
pub enum Program {Launcher,Manager}
impl Program {
 pub fn id(self)->&'static str {match self{Self::Launcher=>"MeowLauncher",Self::Manager=>"MeowGM"}}
 pub fn filename(self)->&'static str {match self{Self::Launcher=>if crate::edition::is_custom(){"MeowLauncher Max.exe"}else{"MeowLauncher.exe"},Self::Manager=>"MeowGM.exe"}}
 fn legacy_id(self)->&'static str {match self{Self::Launcher=>"OOLauncher",Self::Manager=>"OOManager"}}
 fn recognizes(self,id:&str)->bool {id==self.id()||id==self.legacy_id()}
 /// Identity embedded in the EXE, used to tell the two editions apart in
 /// diagnostics. Never used as a trust decision on its own.
 pub fn marker(self)->&'static str {if crate::edition::is_custom(){match self{Self::Launcher=>"MEOW-PRODUCT:MeowLauncher;EDITION:meowopia-custom",Self::Manager=>"MEOW-PRODUCT:MeowGM;EDITION:meowopia-custom"}}else{match self{Self::Launcher=>"MEOW-PRODUCT:MeowLauncher;EDITION:sponsor",Self::Manager=>"MEOW-PRODUCT:MeowGM;EDITION:sponsor"}}}
}
#[derive(Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
struct Manifest {schema_version:u32,version:String,edition:String,notes:String,programs:Vec<Artifact>}
#[derive(Clone,Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
struct Artifact {id:String,channel:String,file_name:String,url:String,sha256:String}
#[derive(Serialize)]
#[serde(rename_all="camelCase")]
pub struct UpdateStatus {pub available:bool,pub version:String,pub program:&'static str,pub edition:&'static str,pub notes:String}
struct Selected {status:UpdateStatus,artifact:Artifact}
fn version(s:&str)->Option<[u32;3]> {if s.len()>32{return None;}s.split('.').map(|p|if p.is_empty()||!p.bytes().all(|b|b.is_ascii_digit())||p.len()>1&&p.starts_with('0'){None}else{p.parse().ok()}).collect::<Option<Vec<u32>>>()?.try_into().ok()}
fn parse(raw:&[u8],base:&str,current:&str,channel:&str,program:Program)->Result<Selected> {
 let invalid=||error("update_invalid","官网更新清单无效，或不适用于此程序和发行版。");
 if raw.len()>16384{return Err(invalid());}
 let m:Manifest=serde_json::from_slice(raw).map_err(|_|invalid())?;
 let release=version(&m.version).ok_or_else(invalid)?;let current=version(current).ok_or_else(invalid)?;
 if m.schema_version!=1||m.edition!=EDITION||!accepts_notes(&m.notes)||!(1..=2).contains(&m.programs.len()){return Err(invalid());}
 for product in [Program::Launcher,Program::Manager] {if m.programs.iter().filter(|a|product.recognizes(&a.id)).count()>1{return Err(invalid());}}
 for a in &m.programs {
  let product=[Program::Launcher,Program::Manager].into_iter().find(|p|p.recognizes(&a.id)).ok_or_else(invalid)?;
  let filename=format!("{}.exe",a.id);
  let expected=format!("{base}/launcher/releases/{}/{}/{}",EDITION,m.version,filename);
  let valid_channel=match product{Program::Launcher=>["legacy-online","platform-v1"].contains(&a.channel.as_str()),Program::Manager=>a.channel=="manager-v1"};
  if !valid_channel||a.file_name!=filename||a.url!=expected||a.sha256.len()!=64||!a.sha256.bytes().all(|c|c.is_ascii_hexdigit()){return Err(invalid());}
 }
 let artifact=m.programs.into_iter().find(|a|program.recognizes(&a.id)).ok_or_else(invalid)?;
 if artifact.channel!=channel{return Err(invalid());}
 Ok(Selected{status:UpdateStatus{available:release>current,version:m.version,program:program.id(),edition:EDITION,notes:NOTES.into()},artifact})
}
fn http(timeout:u64)->Result<reqwest::Client> {reqwest::Client::builder().https_only(true).no_proxy().redirect(reqwest::redirect::Policy::none()).connect_timeout(Duration::from_secs(5)).timeout(Duration::from_secs(timeout)).build().map_err(|_|error("update_unavailable","更新连接初始化失败。"))}
async fn fetch(base:&str,current:&str,channel:&str,program:Program)->Result<Selected> {
 let mut response=http(12)?.get(manifest_url(base)).send().await.map_err(|_|error("update_unavailable","暂时无法连接官网更新服务，请稍后重试。"))?;
 if !response.status().is_success(){return Err(error("update_unavailable","官网更新清单尚不可用，请稍后重试。"));}
 let mut bytes=Vec::new();while let Some(chunk)=response.chunk().await.map_err(|_|error("update_unavailable","更新清单读取失败。"))? {if bytes.len()+chunk.len()>16384{return Err(error("update_invalid","更新清单过大。"));}bytes.extend_from_slice(&chunk);}
 parse(&bytes,base,current,channel,program)
}
/// `base` is the HTTPS origin that serves this edition's feed, for example
/// `https://launcher.invalid`. It is never derived from the manifest itself.
pub fn validate_base(base:&str)->Result<()> {
 if base.is_empty()||base.len()>2048||base.ends_with('/'){return Err(error("update_unavailable","更新源地址无效，请检查发行配置。"));}
 let u=reqwest::Url::parse(base).map_err(|_|error("update_unavailable","更新源地址无效，请检查发行配置。"))?;
 if u.scheme()!="https"||u.host_str().is_none()||u.path()!="/"||!u.username().is_empty()||u.password().is_some()||u.query().is_some()||u.fragment().is_some(){return Err(error("update_unavailable","更新源地址无效，请检查发行配置。"));}
 Ok(())
}
pub async fn check(base:&str,current:&str,channel:&str,program:Program)->Result<UpdateStatus>{validate_base(base)?;Ok(fetch(base,current,channel,program).await?.status)}
fn product_resource(data:&[u8],product_id:&str)->bool {
 let key:Vec<u8>="ProductName\0".encode_utf16().flat_map(u16::to_le_bytes).collect();
 let value:Vec<u8>=format!("{}\0",product_id).encode_utf16().flat_map(u16::to_le_bytes).collect();
 data.windows(key.len()).enumerate().any(|(offset,bytes)|{let start=(offset+key.len()+3)&!3;bytes==key&&data.get(start..start+value.len())==Some(value.as_slice())})
}
fn verify(path:&Path,artifact:&Artifact,program:Program)->Result<()> {
 let invalid=||error("update_integrity","更新文件校验失败，请重新下载。现有程序未修改。");
 let mut file=std::fs::File::open(path).map_err(|_|invalid())?;let size=file.metadata().map_err(|_|invalid())?.len();if !(1024..=134217728).contains(&size){return Err(invalid());}
 let mut data=Vec::with_capacity(size as usize);file.read_to_end(&mut data).map_err(|_|invalid())?;
 if format!("{:x}",Sha256::digest(&data))!=artifact.sha256.to_lowercase()||data.get(..2)!=Some(b"MZ"){return Err(invalid());}
 let pe=u32::from_le_bytes(data.get(60..64).ok_or_else(invalid)?.try_into().map_err(|_|invalid())?) as usize;
 if !program.recognizes(&artifact.id)||data.get(pe..pe+6)!=Some(b"PE\0\0\x64\x86")||!(product_resource(&data,&artifact.id) || program == Program::Launcher && crate::edition::is_custom() && artifact.id == program.id() && product_resource(&data,"MeowLauncher Max")){return Err(invalid());}Ok(())
}
pub struct Updater {base:String,program:Program,channel:&'static str,current:&'static str,busy:AtomicBool,downloaded:Mutex<Option<PathBuf>>}
struct Busy<'a>(&'a AtomicBool);impl Drop for Busy<'_>{fn drop(&mut self){self.0.store(false,Ordering::SeqCst);}}
#[derive(Serialize)]
#[serde(rename_all="camelCase")]
pub struct Downloaded {pub version:String,pub program:&'static str,pub verified:bool,pub directory:String}
impl Updater {
 /// `base` must already pass `validate_base`; callers pass the compiled
 /// Meowopia origin or the operator's `updateBaseUrl`.
 pub fn new(base:&str,program:Program,channel:&'static str,current:&'static str)->Self{Self{base:base.to_owned(),program,channel,current,busy:AtomicBool::new(false),downloaded:Mutex::new(None)}}
 pub fn base(&self)->&str{&self.base}
 pub async fn download(&self,root:&Path)->Result<Downloaded> {
  self.busy.compare_exchange(false,true,Ordering::SeqCst,Ordering::SeqCst).map_err(|_|error("update_busy","更新任务正在进行。"))?;let _busy=Busy(&self.busy);
  self.downloaded.lock().map_err(|_|error("internal","更新状态不可用。"))?.take();
  validate_base(&self.base)?;
  let selected=fetch(&self.base,self.current,self.channel,self.program).await?;if !selected.status.available{return Err(error("no_update","当前已经是最新版本。"));}
  std::fs::create_dir_all(root).map_err(|_|error("update_write","更新目录不可写。"))?;
  let directory=tempfile::Builder::new().prefix("verified-update-").tempdir_in(root).map_err(|_|error("update_write","无法创建更新目录。"))?;
  let file=directory.path().join(self.program.filename());let mut out=std::fs::OpenOptions::new().write(true).create_new(true).open(&file).map_err(|_|error("update_write","无法写入更新文件。"))?;
  let mut response=http(180)?.get(&selected.artifact.url).send().await.map_err(|_|error("update_unavailable","新版下载失败，请稍后重试。"))?;
  if !response.status().is_success(){return Err(error("update_unavailable","新版下载地址尚不可用。"));}
  let mut size=0usize;while let Some(chunk)=response.chunk().await.map_err(|_|error("update_unavailable","新版下载中断，请重新下载。"))? {size+=chunk.len();if size>134217728{return Err(error("update_integrity","更新文件过大。"));}out.write_all(&chunk).map_err(|_|error("update_write","更新文件写入失败。"))?;}
  out.sync_all().map_err(|_|error("update_write","更新文件保存失败。"))?;drop(out);verify(&file,&selected.artifact,self.program)?;
  let saved=directory.keep();*self.downloaded.lock().map_err(|_|error("internal","更新状态不可用。"))?=Some(saved.clone());
  Ok(Downloaded{version:selected.status.version,program:self.program.id(),verified:true,directory:saved.display().to_string()})
 }
 pub fn downloaded_directory(&self)->Result<PathBuf>{self.downloaded.lock().map_err(|_|error("internal","更新状态不可用。"))?.clone().ok_or_else(||error("update_not_ready","请先下载并校验新版。"))}
}
#[cfg(test)]
mod tests {
 use super::*;
 const BASE:&str="https://launcher.invalid";
 /// The edition this build is NOT, used to prove cross-edition feeds are rejected.
 fn other_edition()->&'static str {if crate::edition::is_custom(){"sponsor"}else{"meowopia-custom"}}
 fn fixture(v:&str)->serde_json::Value {serde_json::json!({"schemaVersion":1,"version":v,"edition":EDITION,"notes":NOTES,"programs":[{"id":"MeowLauncher","channel":"legacy-online","fileName":"MeowLauncher.exe","url":format!("{BASE}/launcher/releases/{EDITION}/{v}/MeowLauncher.exe"),"sha256":"a".repeat(64)},{"id":"MeowGM","channel":"manager-v1","fileName":"MeowGM.exe","url":format!("{BASE}/launcher/releases/{EDITION}/{v}/MeowGM.exe"),"sha256":"b".repeat(64)}]})}
 fn read(v:&serde_json::Value,p:Program,c:&str)->Result<Selected>{parse(&serde_json::to_vec(v).unwrap(),BASE,"0.5.0",c,p)}
 #[test] fn checks_version_without_downgrading(){for (v,expected) in [("0.5.1",true),("0.5.0",false),("0.4.9",false)]{assert_eq!(read(&fixture(v),Program::Launcher,"legacy-online").unwrap().status.available,expected);}}
 #[test] fn legacy_names_and_independent_manifests_remain_bound(){
  let mut v=fixture("0.5.1");v["notes"]=LEGACY_NOTES.into();
  for (i,p) in [Program::Launcher,Program::Manager].into_iter().enumerate(){v["programs"][i]["id"]=p.legacy_id().into();v["programs"][i]["fileName"]=format!("{}.exe",p.legacy_id()).into();v["programs"][i]["url"]=format!("{BASE}/launcher/releases/{EDITION}/0.5.1/{}.exe",p.legacy_id()).into();}
  assert_eq!(read(&v,Program::Launcher,"legacy-online").unwrap().status.program,"MeowLauncher");
  let mut one=fixture("0.5.1");one["programs"].as_array_mut().unwrap().remove(0);assert!(read(&one,Program::Manager,"manager-v1").is_ok());assert!(read(&one,Program::Launcher,"legacy-online").is_err());
  let duplicate=v["programs"][0].clone();let mut mixed=fixture("0.5.1");mixed["programs"][1]=duplicate;assert!(read(&mixed,Program::Launcher,"legacy-online").is_err());
 }
 #[test] fn old_pe_product_cannot_masquerade_as_new_product(){
  let d=tempfile::tempdir().unwrap();let path=d.path().join("legacy.exe");let mut data=vec![0u8;2048];data[..2].copy_from_slice(b"MZ");data[60..64].copy_from_slice(&128u32.to_le_bytes());data[128..134].copy_from_slice(b"PE\0\0\x64\x86");
  let key:Vec<u8>="ProductName\0".encode_utf16().flat_map(u16::to_le_bytes).collect();let value:Vec<u8>="OOLauncher\0".encode_utf16().flat_map(u16::to_le_bytes).collect();data[768..768+key.len()].copy_from_slice(&key);let start=(768+key.len()+3)&!3;data[start..start+value.len()].copy_from_slice(&value);std::fs::write(&path,&data).unwrap();
  let mut artifact=read(&fixture("0.5.1"),Program::Launcher,"legacy-online").unwrap().artifact;artifact.sha256=format!("{:x}",Sha256::digest(&data));assert!(verify(&path,&artifact,Program::Launcher).is_err());artifact.id="OOLauncher".into();assert!(verify(&path,&artifact,Program::Launcher).is_ok());assert!(verify(&path,&artifact,Program::Manager).is_err());
 }
 #[test] fn max_product_resource_is_custom_only(){
  let d=tempfile::tempdir().unwrap();let path=d.path().join("max.exe");let mut data=vec![0u8;2048];data[..2].copy_from_slice(b"MZ");data[60..64].copy_from_slice(&128u32.to_le_bytes());data[128..134].copy_from_slice(b"PE\0\0\x64\x86");
  let key:Vec<u8>="ProductName\0".encode_utf16().flat_map(u16::to_le_bytes).collect();let value:Vec<u8>="MeowLauncher Max\0".encode_utf16().flat_map(u16::to_le_bytes).collect();data[768..768+key.len()].copy_from_slice(&key);let start=(768+key.len()+3)&!3;data[start..start+value.len()].copy_from_slice(&value);std::fs::write(&path,&data).unwrap();
  let mut a=read(&fixture("0.5.1"),Program::Launcher,"legacy-online").unwrap().artifact;a.sha256=format!("{:x}",Sha256::digest(&data));assert_eq!(verify(&path,&a,Program::Launcher).is_ok(),crate::edition::is_custom());assert!(verify(&path,&a,Program::Manager).is_err());
 }
 #[test] fn separates_programs_and_protocols(){let v=fixture("0.5.1");assert_eq!(read(&v,Program::Manager,"manager-v1").unwrap().artifact.file_name,"MeowGM.exe");assert!(read(&v,Program::Launcher,"manager-v1").is_err());assert!(read(&v,Program::Launcher,"platform-v1").is_err());}
 #[test] fn rejects_other_editions_urls_hashes_and_duplicate_programs(){
  for (field,value) in [("edition",other_edition()),("version","latest"),("notes","other")] {let mut v=fixture("0.5.1");v[field]=value.into();assert!(read(&v,Program::Launcher,"legacy-online").is_err(),"{field}");}
  let foreign=format!("{BASE}/launcher/releases/{}/0.5.1/MeowLauncher.exe",other_edition());
  let foreign_origin="https://github.com/example/MeowLauncher/releases/download/v0.5.1/MeowLauncher.exe";
  for (field,value) in [("url",foreign_origin),("url",foreign.as_str()),("fileName","MeowGM.exe"),("sha256","bad"),("id","MeowGM")] {let mut v=fixture("0.5.1");v["programs"][0][field]=value.into();assert!(read(&v,Program::Launcher,"legacy-online").is_err(),"{field}");}
 }
 #[test] fn feed_base_is_validated_and_drives_the_artifact_path(){
  for bad in ["http://launcher.invalid","https://launcher.invalid/","https://launcher.invalid/launcher","https://user@launcher.invalid","https://launcher.invalid?x=1",""] {assert!(validate_base(bad).is_err(),"{bad}");}
  assert!(validate_base(BASE).is_ok());
  // The same manifest is rejected when the operator configured another origin.
  assert!(parse(&serde_json::to_vec(&fixture("0.5.1")).unwrap(),"https://other.example.com","0.5.0","legacy-online",Program::Launcher).is_err());
  assert_eq!(manifest_url(BASE),format!("{BASE}/launcher/meow-update.json"));
 }
 #[test] fn download_verifies_hash_and_embedded_product(){let d=tempfile::tempdir().unwrap();let path=d.path().join("test.exe");let mut data=vec![0u8;2048];data[..2].copy_from_slice(b"MZ");data[60..64].copy_from_slice(&128u32.to_le_bytes());data[128..134].copy_from_slice(b"PE\0\0\x64\x86");let marker=Program::Launcher.marker().as_bytes();data[512..512+marker.len()].copy_from_slice(marker);let key:Vec<u8>="ProductName\0".encode_utf16().flat_map(u16::to_le_bytes).collect();let value:Vec<u8>="MeowLauncher\0".encode_utf16().flat_map(u16::to_le_bytes).collect();data[768..768+key.len()].copy_from_slice(&key);let start=(768+key.len()+3)&!3;data[start..start+value.len()].copy_from_slice(&value);std::fs::write(&path,&data).unwrap();let mut a=read(&fixture("0.5.1"),Program::Launcher,"legacy-online").unwrap().artifact;a.sha256=format!("{:x}",Sha256::digest(&data));assert!(verify(&path,&a,Program::Launcher).is_ok());assert!(verify(&path,&a,Program::Manager).is_err());data[1024]=1;std::fs::write(&path,data).unwrap();assert!(verify(&path,&a,Program::Launcher).is_err());}
}
