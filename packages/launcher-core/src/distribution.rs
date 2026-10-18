//! Client files only: fixed provider trust, resumable staging and journaled replacement.
use crate::{error,Result};
use fs2::FileExt;
use serde::{Deserialize,Serialize};
use sha2::{Digest,Sha256};
use std::{collections::{BTreeMap,BTreeSet},fs::{self,File},io::{Read,Write},path::{Path,PathBuf},sync::{Mutex,atomic::{AtomicBool,Ordering}},time::Duration};
const WORK:&str=".oo-client-update";
const STATE:&str=".oo-client-installed.json";
fn io_error()->crate::AppError{error("client_update_io","客户端文件操作失败；请确认目录可写及剩余空间。")}
fn invalid()->crate::AppError{error("client_manifest_invalid","客户端下载清单、路径或可信来源不符合要求。")}
#[derive(Clone)]
pub struct Source {edition:String,realm:String,adapter:String,manifest_url:String,artifact_prefix:String,profile:Option<crate::client::Profile>,fixture:bool}
impl Source {
 pub fn new(edition:&str,realm:&str,adapter:&str,manifest_url:&str,artifact_prefix:&str)->Result<Self>{
  for value in [edition,realm,adapter]{if value.is_empty()||value.len()>80||!value.bytes().all(|c|c.is_ascii_alphanumeric()||c==b'-'){return Err(invalid());}}
  for value in [manifest_url,artifact_prefix]{let u=reqwest::Url::parse(value).map_err(|_|invalid())?;if u.scheme()!="https"||u.host_str().is_none()||!u.username().is_empty()||u.password().is_some()||u.query().is_some()||u.fragment().is_some(){return Err(invalid());}}
  if !artifact_prefix.ends_with('/') {return Err(invalid());}
  let m=reqwest::Url::parse(manifest_url).map_err(|_|invalid())?;let a=reqwest::Url::parse(artifact_prefix).map_err(|_|invalid())?;if m.origin()!=a.origin(){return Err(invalid());}
  Ok(Self{edition:edition.into(),realm:realm.into(),adapter:adapter.into(),manifest_url:manifest_url.into(),artifact_prefix:artifact_prefix.into(),profile:None,fixture:false})
 }
 pub fn meowopia()->Self{Self::unconfigured()}
 /// 没有配置客户端下载源的发行版：清单与资产地址都为空的占位源，任何下载都会
 /// 明确报“尚未配置”，而不是退回到某个默认站点。
 pub fn unconfigured()->Self{Self{edition:String::new(),realm:String::new(),adapter:String::new(),manifest_url:String::new(),artifact_prefix:String::new(),profile:None,fixture:false}}
 pub fn with_profile(mut self, profile:crate::client::Profile)->Result<Self>{profile.validate()?;if profile.id!=self.adapter{return Err(invalid());}self.profile=Some(profile);Ok(self)}
 pub fn configured(&self)->bool{!self.manifest_url.is_empty()}
 fn client(&self)->Result<reqwest::Client>{reqwest::Client::builder().https_only(!self.fixture).no_proxy().redirect(reqwest::redirect::Policy::none()).connect_timeout(Duration::from_secs(10)).read_timeout(Duration::from_secs(30)).build().map_err(|_|io_error())}
 fn trusted(&self,value:&str)->bool{self.configured()&&value.starts_with(&self.artifact_prefix)&&reqwest::Url::parse(value).is_ok_and(|u|u.as_str()==value&&u.query().is_none()&&u.fragment().is_none()&&u.username().is_empty()&&u.password().is_none()&&!u.path().contains('%'))}
 #[cfg(test)] fn fixture(origin:&str,name:&str)->Self{Self{edition:"fixture".into(),realm:"s4a21".into(),adapter:"fixture-a21".into(),manifest_url:format!("{origin}/manifests/{name}.json"),artifact_prefix:format!("{origin}/files/"),profile:None,fixture:true}}
}
#[derive(Clone,Deserialize,Serialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
struct Asset {url:String,size:u64,sha256:String}
#[derive(Clone,Deserialize,Serialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
struct Entry {path:String,size:u64,sha256:String,#[serde(default)]url:Option<String>}
#[derive(Clone,Deserialize,Serialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
struct Manifest {schema_version:u32,edition:String,realm:String,adapter:String,version:String,files:Vec<Entry>,#[serde(default)]archive:Option<Asset>,#[serde(default)]archive_root:Option<String>,#[serde(default)]remove_files:Vec<String>}
fn safe_relative(value:&str)->Result<()> {
 if value.is_empty()||value.len()>512||value.contains(['\\',':','*','?','"','<','>','|'])||value.chars().any(|c|c.is_control()){return Err(invalid());}
 for p in value.split('/'){
  let upper=p.split('.').next().unwrap_or("").to_ascii_uppercase();
  if p.is_empty()||p=="."||p==".."||p.ends_with(['.',' '])||["CON","PRN","AUX","NUL"].contains(&upper.as_str())||["COM","LPT"].iter().any(|prefix|upper.strip_prefix(prefix).is_some_and(|n|n.len()==1&&matches!(n.as_bytes()[0],b'1'..=b'9'))){return Err(invalid());}
 }
 let first=value.split('/').next().unwrap().to_ascii_lowercase();
 if first.starts_with(".oo-")||["data","updates","screenshot","screenshots","capture","captures","logs","log","launcher.public.json","meowlauncher.exe","meowmanager.exe","meowgm.exe","meow-launcher.exe","meow-manager.exe","oolauncher.exe","oomanager.exe","oo launcher.exe","oo manager.exe","oo-launcher.exe","oo-manager.exe"].contains(&first.as_str()){return Err(invalid());}Ok(())
}
fn setting(path:&str)->bool{Path::new(path).extension().and_then(|e|e.to_str()).is_some_and(|e|["ini","cfg","xml","json"].contains(&e.to_ascii_lowercase().as_str()))}
fn needs_update(root:&Path,f:&Entry)->bool{let target=root.join(&f.path);if setting(&f.path)&&target.is_file(){return false;}!matches(&target,f.size,&f.sha256)}
fn hash_valid(value:&str)->bool{value.len()==64&&value.bytes().all(|b|b.is_ascii_hexdigit())}
fn validate(source:&Source,manifest:&Manifest)->Result<()> {
 if manifest.schema_version!=1||manifest.edition!=source.edition||manifest.realm!=source.realm||manifest.adapter!=source.adapter||manifest.version.is_empty()||manifest.version.len()>64||!manifest.version.bytes().all(|b|b.is_ascii_alphanumeric()||b==b'.'||b==b'-')||manifest.files.is_empty()||manifest.files.len()>50000{return Err(invalid());}
 let mut paths=BTreeSet::new();let mut total=0u64;
 for f in &manifest.files{safe_relative(&f.path)?;if !paths.insert(f.path.to_uppercase())||f.size>17179869184||!hash_valid(&f.sha256)||f.url.as_ref().is_some_and(|u|!source.trusted(u)){return Err(invalid());}total=total.checked_add(f.size).ok_or_else(invalid)?;}
 if total>137438953472{return Err(invalid());}
 for path in &paths{let mut p=path.as_str();while let Some((parent,_))=p.rsplit_once('/'){if paths.contains(parent){return Err(invalid());}p=parent;}}
 for f in &manifest.remove_files{safe_relative(f)?;if setting(f){return Err(error("client_remove_setting","更新清单试图删除玩家设置，已拒绝操作。"));}if !paths.insert(f.to_uppercase()){return Err(invalid());}}
 if let Some(a)=&manifest.archive{if !source.trusted(&a.url)||a.size==0||a.size>68719476736||!hash_valid(&a.sha256){return Err(invalid());}}
 if let Some(prefix)=&manifest.archive_root{safe_relative(prefix)?;if manifest.archive.is_none(){return Err(invalid());}}
 if manifest.archive.is_none()&&manifest.files.iter().any(|f|f.url.is_none()){return Err(invalid());}
 if !source.fixture {
    let p=source.profile.as_ref().ok_or(error("adapter_not_configured","下载源未绑定客户端适配器。"))?;
    if p.id!=source.adapter {return Err(invalid());}
    for required in &p.required_files {if !manifest.files.iter().any(|f|&f.path==required){return Err(invalid());}}
    for folder in &p.required_directories {if !manifest.files.iter().any(|f|f.path.starts_with(&format!("{folder}/"))){return Err(invalid());}}
    if !manifest.files.iter().any(|f|f.path==p.executable&&f.sha256.eq_ignore_ascii_case(&p.executable_sha256)){return Err(error("client_adapter_mismatch","下载清单与所选适配器不匹配。"));}
 } Ok(())
}
fn reparse(path:&Path)->bool {
 let Ok(meta)=fs::symlink_metadata(path)else{return false;};
 #[cfg(windows)] {use std::os::windows::fs::MetadataExt;if meta.file_attributes()&0x400!=0{return true;}}
 meta.file_type().is_symlink()
}
fn guarded(root:&Path,path:&str)->Result<PathBuf>{safe_relative(path)?;if reparse(root){return Err(invalid());}let mut result=root.to_path_buf();for piece in path.split('/'){result.push(piece);if reparse(&result){return Err(error("client_unsafe_path","客户端路径包含链接或重解析点。"));}}if result.exists()&&!result.is_file(){return Err(invalid());}Ok(result)}
fn hash(path:&Path)->Result<String>{let mut f=File::open(path).map_err(|_|io_error())?;let mut h=Sha256::new();let mut buf=[0u8;65536];loop{let n=f.read(&mut buf).map_err(|_|io_error())?;if n==0{break;}h.update(&buf[..n]);}Ok(format!("{:x}",h.finalize()))}
fn matches(path:&Path,size:u64,digest:&str)->bool{path.metadata().is_ok_and(|m|m.is_file()&&m.len()==size)&&hash(path).is_ok_and(|v|v.eq_ignore_ascii_case(digest))}
fn atomic(path:&Path,bytes:&[u8])->Result<()> {let parent=path.parent().ok_or_else(io_error)?;fs::create_dir_all(parent).map_err(|_|io_error())?;if reparse(path){return Err(invalid());}let mut temp=tempfile::NamedTempFile::new_in(parent).map_err(|_|io_error())?;temp.write_all(bytes).and_then(|_|temp.as_file().sync_all()).map_err(|_|io_error())?;temp.persist(path).map_err(|_|io_error())?;Ok(())}
fn json<T:Serialize>(path:&Path,value:&T)->Result<()>{atomic(path,&serde_json::to_vec(value).map_err(|_|invalid())?)}
fn read_limited(path:&Path,limit:u64)->Result<Vec<u8>>{let file=File::open(path).map_err(|_|io_error())?;let mut bytes=Vec::new();file.take(limit+1).read_to_end(&mut bytes).map_err(|_|io_error())?;if bytes.len() as u64>limit{return Err(invalid());}Ok(bytes)}
#[derive(Clone,Default,Serialize)]
#[serde(rename_all="camelCase")]
pub struct Progress {pub stage:String,pub file:String,pub received:u64,pub total:u64,pub files_done:usize,pub files_total:usize,pub message:String}
#[derive(Serialize)]
#[serde(rename_all="camelCase")]
pub struct Plan {pub version:String,pub edition:String,pub files:usize,pub changed_files:usize,pub download_bytes:u64,pub removed_files:usize,pub preserved_settings:usize,pub installed_version:Option<String>}
pub struct Engine {source:Source,http:Mutex<Option<reqwest::Client>>,busy:AtomicBool,cancel:AtomicBool,progress:Mutex<Progress>,#[cfg(test)]fail_after:std::sync::atomic::AtomicUsize}
struct Busy<'a>(&'a AtomicBool);impl Drop for Busy<'_>{fn drop(&mut self){self.0.store(false,Ordering::SeqCst);}}
impl Engine {
 pub fn new(source:Source)->Self{Self{source,http:Mutex::new(None),busy:AtomicBool::new(false),cancel:AtomicBool::new(false),progress:Mutex::new(Progress::default()),#[cfg(test)]fail_after:std::sync::atomic::AtomicUsize::new(0)}}
 pub fn busy(&self)->bool{self.busy.load(Ordering::SeqCst)}
 pub fn cancel(&self){self.cancel.store(true,Ordering::SeqCst);}
 pub fn progress(&self)->Progress{self.progress.lock().map(|p|p.clone()).unwrap_or_default()}
 fn client(&self)->Result<reqwest::Client>{let mut slot=self.http.lock().map_err(|_|io_error())?;if slot.is_none(){*slot=Some(self.source.client()?);}Ok(slot.as_ref().unwrap().clone())}
 fn status(&self,stage:&str,message:&str){if let Ok(mut p)=self.progress.lock(){p.stage=stage.into();p.message=message.into();}}
 async fn manifest(&self)->Result<(Manifest,String)> {
  if !self.source.configured(){return Err(error("client_source_unavailable","此发行版尚未配置客户端下载源；请使用手动下载入口，或联系服主提供下载地址。"));}
  self.status("checking","正在获取可信客户端下载清单…");let mut response=self.client()?.get(&self.source.manifest_url).send().await.map_err(|_|error("client_source_unavailable","暂时无法连接客户端下载源。"))?;
  if !response.status().is_success(){return Err(error("client_source_unavailable","客户端下载清单尚未发布，可先使用手动下载入口。"));}
  let mut bytes=Vec::new();while let Some(chunk)=response.chunk().await.map_err(|_|io_error())?{if bytes.len()+chunk.len()>8388608{return Err(invalid());}bytes.extend_from_slice(&chunk);}
  let manifest:Manifest=serde_json::from_slice(&bytes).map_err(|_|invalid())?;validate(&self.source,&manifest)?;Ok((manifest,format!("{:x}",Sha256::digest(&bytes))))
 }
 pub async fn check(&self,root:&Path)->Result<Plan>{let (manifest,_)=self.manifest().await?;plan(root,&manifest)}
 pub async fn install(&self,root:&Path,game_running:fn()->Result<bool>)->Result<Plan> {
  self.busy.compare_exchange(false,true,Ordering::SeqCst,Ordering::SeqCst).map_err(|_|error("client_update_busy","客户端下载或更新正在进行。"))?;let _busy=Busy(&self.busy);self.cancel.store(false,Ordering::SeqCst);if let Ok(mut p)=self.progress.lock(){*p=Progress{stage:"checking".into(),message:"正在准备客户端安装或更新…".into(),..Progress::default()};}
  let result=self.install_inner(root,game_running).await;
  if let Err(e)=&result{self.status(if e.code=="client_update_cancelled"{"paused"}else{"error"},e.message);}result
 }
 async fn install_inner(&self,root:&Path,game_running:fn()->Result<bool>)->Result<Plan> {
  if game_running()?{return Err(error("game_running","请先退出游戏，再安装或更新客户端。"));}
  if !root.is_absolute(){return Err(invalid());}let mut prefix=PathBuf::new();for piece in root.components(){prefix.push(piece.as_os_str());if reparse(&prefix){return Err(error("client_unsafe_path","安装目录包含链接或重解析点。"));}}
  fs::create_dir_all(root).map_err(|_|io_error())?;let root=root.canonicalize().map_err(|_|io_error())?;let work=root.join(WORK);if reparse(&work){return Err(invalid());}fs::create_dir_all(&work).map_err(|_|io_error())?;
  let lock_path=work.join("engine.lock");if reparse(&lock_path){return Err(invalid());}let lock=fs::OpenOptions::new().read(true).write(true).create(true).truncate(false).open(lock_path).map_err(|_|io_error())?;lock.try_lock_exclusive().map_err(|_|error("client_update_busy","另一启动器正在更新此客户端。"))?;
  recover(&root,&work,&self.source)?;
  let (manifest,id)=self.manifest().await?;let result=plan(&root,&manifest)?;
  let changed:Vec<Entry>=manifest.files.iter().filter(|f|needs_update(&root,f)).cloned().collect();
  let old=installed(&root)?;if old.as_ref().is_some_and(|m|m.edition!=self.source.edition||m.realm!=self.source.realm||m.adapter!=self.source.adapter){return Err(error("client_edition_mismatch","此客户端属于其他发行版或区服，拒绝混用更新。"));}for path in &manifest.remove_files{if let Ok(mut p)=self.progress.lock(){p.file=path.clone();p.message="正在检查旧资源删除条件…".into();}if !guarded(&root,path)?.exists(){continue;}if setting(path){return Err(error("client_remove_setting","更新清单试图删除玩家设置，已拒绝操作。"));}let Some(entry)=old.as_ref().and_then(|m|m.files.iter().find(|f|&f.path==path))else{return Err(error("client_remove_unmanaged","拒绝删除不属于旧版客户端下载清单的文件。"));};let p=guarded(&root,path)?;if p.exists()&&!matches(&p,entry.size,&entry.sha256){return Err(error("client_remove_modified","旧资源已被修改，拒绝自动删除。"));}}
  let use_archive=manifest.archive.is_some()&&(old.is_none()||changed.iter().any(|f|f.url.is_none()));
  let total=if use_archive{manifest.files.iter().map(|f|f.size).sum::<u64>()}else{changed.iter().map(|f|f.size).sum::<u64>()};let needed=total.checked_mul(2).and_then(|v|v.checked_add(if use_archive{manifest.archive.as_ref().map(|a|a.size).unwrap_or(0)}else{0})).ok_or_else(invalid)?;
  if fs2::available_space(&root).map_err(|_|io_error())?<needed{return Err(error("client_disk_space","磁盘剩余空间不足以安全暂存和备份本次更新。"));}
  if let Ok(mut p)=self.progress.lock(){*p=Progress{stage:"downloading".into(),files_total:changed.len(),total:result.download_bytes,..Progress::default()};}
  let tx=work.join(id);if reparse(&tx){return Err(invalid());}fs::create_dir_all(&tx).map_err(|_|io_error())?;let staged=tx.join("staged");if reparse(&staged){return Err(invalid());}fs::create_dir_all(&staged).map_err(|_|io_error())?;
  if !changed.is_empty(){
   if use_archive {let archive=manifest.archive.as_ref().unwrap();let file=self.download(&tx,archive,"客户端压缩包").await?;self.status("verifying","正在安全解压并逐文件校验…");extract(&file,&staged,&manifest)?;}
   else{for (index,f) in changed.iter().enumerate(){let asset=Asset{url:f.url.clone().ok_or_else(invalid)?,size:f.size,sha256:f.sha256.clone()};let file=self.download(&tx,&asset,&f.path).await?;let target=guarded(&staged,&f.path)?;if let Some(parent)=target.parent(){fs::create_dir_all(parent).map_err(|_|io_error())?;}fs::copy(file,&target).map_err(|_|io_error())?;fs::OpenOptions::new().write(true).open(&target).and_then(|f|f.sync_all()).map_err(|_|io_error())?;if !matches(&target,f.size,&f.sha256){return Err(error("client_checksum","客户端资源 SHA256 校验失败，现有客户端未修改。"));}if let Ok(mut p)=self.progress.lock(){p.files_done=index+1;}}}
  }
  if self.cancel.load(Ordering::SeqCst){return Err(error("client_update_cancelled","已暂停下载；再次安装时将验证并续传。"));}
  if game_running()?{return Err(error("game_running","检测到游戏正在运行，已保留下载；退出游戏后重试。"));}
  self.status("installing","正在安装已校验资源，失败时恢复原文件…");
  let mut journal=prepare(&root,&tx,&changed,&manifest.remove_files,&self.source)?;
  let outcome=apply(&root,&tx,&mut journal,|index|{#[cfg(test)]if self.fail_after.load(Ordering::SeqCst)==index{return Err(io_error());}let _=index;Ok(())});
  if let Err(e)=outcome{rollback(&root,&tx,&mut journal)?;return Err(e);}
  if let Err(e)=json(&root.join(STATE),&manifest){rollback(&root,&tx,&mut journal)?;return Err(e);}
  journal.status="committed".into();if let Err(e)=json(&tx.join("journal.json"),&journal){rollback(&root,&tx,&mut journal)?;return Err(e);}
  self.status("complete","客户端下载或更新完成；启动器仍会检查客户端，实际入场需在游戏内确认。");Ok(result)
 }
 async fn download(&self,tx:&Path,asset:&Asset,label:&str)->Result<PathBuf>{
  let cache=tx.join("cache");if reparse(&cache){return Err(invalid());}fs::create_dir_all(&cache).map_err(|_|io_error())?;
  let path=cache.join(format!("{}.part",asset.sha256.to_lowercase()));let meta_path=path.with_extension("resume.json");if reparse(&path)||reparse(&meta_path){return Err(invalid());}
  if matches(&path,asset.size,&asset.sha256){return Ok(path);}
  let mut received=path.metadata().map(|m|m.len()).unwrap_or(0);
  let old:Option<Resume>=read_limited(&meta_path,8192).ok().and_then(|b|serde_json::from_slice(&b).ok());
  let resume=old.filter(|m|m.url==asset.url&&m.size==asset.size&&m.sha256.eq_ignore_ascii_case(&asset.sha256)&&!m.etag.is_empty()&&!m.etag.starts_with("W/")&&received<asset.size);
  if resume.is_none(){received=0;}
  let mut request=self.client()?.get(&asset.url);if received>0{let m=resume.as_ref().unwrap();request=request.header("Range",format!("bytes={received}-")).header("If-Range",&m.etag);}
  let mut response=request.send().await.map_err(|_|error("client_download","客户端下载连接失败，可重试续传。"))?;
  let etag=response.headers().get("etag").and_then(|v|v.to_str().ok()).unwrap_or("").to_string();
  if response.status()==reqwest::StatusCode::PARTIAL_CONTENT {
   if received==0||resume.as_ref().is_none_or(|m|m.etag!=etag)||response.headers().get("content-range").and_then(|v|v.to_str().ok())!=Some(format!("bytes {}-{}/{}",received,asset.size-1,asset.size).as_str()){return Err(error("client_resume_invalid","下载源续传响应不一致，现有客户端未修改。"));}
  }else if response.status()==reqwest::StatusCode::OK{received=0;}else{return Err(error("client_download","客户端下载源返回失败，请稍后重试。"));}
  if response.content_length().is_some_and(|n|n!=asset.size-received){return Err(error("client_download_size","下载源文件大小与清单不一致。"));}
  json(&meta_path,&Resume{url:asset.url.clone(),size:asset.size,sha256:asset.sha256.clone(),etag})?;
  let mut out=fs::OpenOptions::new().write(true).create(true).truncate(received==0).append(received>0).open(&path).map_err(|_|io_error())?;
  if let Ok(mut p)=self.progress.lock(){p.file=label.into();p.received=received;p.total=asset.size;}
  loop{
   let next=tokio::select!{chunk=response.chunk()=>chunk.map_err(|_|error("client_download_interrupted","下载中断，已保留分段；重试会校验并续传。"))?,_=async{while !self.cancel.load(Ordering::SeqCst){tokio::time::sleep(Duration::from_millis(100)).await;}}=>{out.sync_all().map_err(|_|io_error())?;return Err(error("client_update_cancelled","已暂停下载；再次安装时将验证并续传。"));}};
   let Some(chunk)=next else{break;};received+=chunk.len() as u64;if received>asset.size{return Err(error("client_download_size","下载内容超过清单大小。"));}out.write_all(&chunk).map_err(|_|io_error())?;if let Ok(mut p)=self.progress.lock(){p.received=received;}
  }
  out.sync_all().map_err(|_|io_error())?;drop(out);
  if !matches(&path,asset.size,&asset.sha256){if path.exists(){fs::remove_file(&path).map_err(|_|io_error())?;}return Err(error("client_checksum","客户端资源 SHA256 校验失败，现有客户端未修改。"));}Ok(path)
 }
}
#[derive(Deserialize,Serialize)]struct Resume{url:String,size:u64,sha256:String,etag:String}
fn installed(root:&Path)->Result<Option<Manifest>>{let path=root.join(STATE);if !path.exists(){return Ok(None);}if reparse(&path){return Err(invalid());}Ok(Some(serde_json::from_slice(&read_limited(&path,8388608)?).map_err(|_|invalid())?))}
fn plan(root:&Path,manifest:&Manifest)->Result<Plan>{let old=installed(root)?;if old.as_ref().is_some_and(|m|m.edition!=manifest.edition||m.realm!=manifest.realm||m.adapter!=manifest.adapter){return Err(error("client_edition_mismatch","此客户端属于其他发行版或区服，拒绝混用更新。"));}let mut changed=0;let mut bytes=0;let mut missing_url=false;let mut preserved=0;for f in &manifest.files{let p=guarded(root,&f.path)?;if setting(&f.path)&&p.is_file(){if !matches(&p,f.size,&f.sha256){preserved+=1;}continue;}if !matches(&p,f.size,&f.sha256){changed+=1;bytes+=f.size;missing_url|=f.url.is_none();}}if changed>0&&(old.is_none()||missing_url){if let Some(a)=&manifest.archive{bytes=a.size;}}Ok(Plan{version:manifest.version.clone(),edition:manifest.edition.clone(),files:manifest.files.len(),changed_files:changed,download_bytes:bytes,removed_files:manifest.remove_files.iter().filter(|path|root.join(path).is_file()).count(),preserved_settings:preserved,installed_version:old.map(|m|m.version)})}
fn extract(archive:&Path,root:&Path,manifest:&Manifest)->Result<()> {
 let file=File::open(archive).map_err(|_|io_error())?;let mut zip=zip::ZipArchive::new(file).map_err(|_|error("client_archive_invalid","客户端压缩包不是支持的 ZIP 格式。"))?;
 if zip.len()>100000{return Err(invalid());}let expected:BTreeMap<String,&Entry>=manifest.files.iter().map(|f|(f.path.clone(),f)).collect();let mut seen=BTreeSet::new();
 for index in 0..zip.len(){let mut entry=zip.by_index(index).map_err(|_|invalid())?;let mut name=entry.name().to_string();let trimmed=name.trim_end_matches('/');safe_relative(trimmed)?;if entry.encrypted()||entry.unix_mode().is_some_and(|m|{let t=m&0o170000; t!=0&&t!=0o100000&&!(t==0o040000&&entry.is_dir())}){return Err(error("client_archive_unsafe","客户端压缩包包含链接、特殊文件或加密文件。"));}
  if let Some(prefix)=&manifest.archive_root{if entry.is_dir()&&name.trim_end_matches('/')==prefix{continue;}name=name.strip_prefix(&format!("{prefix}/")).ok_or_else(invalid)?.to_string();safe_relative(name.trim_end_matches('/'))?;}
  if entry.is_dir(){continue;}if !seen.insert(name.to_uppercase()){return Err(invalid());}let info=expected.get(&name).ok_or_else(invalid)?;if entry.size()!=info.size{return Err(error("client_archive_size","解压文件大小与清单不一致。"));}let target=guarded(root,&name)?;if let Some(parent)=target.parent(){fs::create_dir_all(parent).map_err(|_|io_error())?;}let mut out=File::create(&target).map_err(|_|io_error())?;let copied=std::io::copy(&mut entry.by_ref().take(info.size+1),&mut out).map_err(|_|invalid())?;out.sync_all().map_err(|_|io_error())?;drop(out);if copied!=info.size||!matches(&target,info.size,&info.sha256){return Err(error("client_checksum","解压资源 SHA256 校验失败，原客户端未修改。"));}}
 if seen.len()!=manifest.files.len(){return Err(error("client_archive_incomplete","压缩包缺少清单要求的客户端文件。"));}Ok(())
}
#[derive(Clone,Deserialize,Serialize)]#[serde(deny_unknown_fields)]struct Change{path:String,old:Option<String>,new:Option<String>}
#[derive(Deserialize,Serialize)]#[serde(deny_unknown_fields)]struct Journal{schema:u32,edition:String,realm:String,adapter:String,backup_dir:String,status:String,changes:Vec<Change>,previous_state:Option<String>}
fn prepare(root:&Path,tx:&Path,changed:&[Entry],removed:&[String],source:&Source)->Result<Journal>{let mut changes=Vec::new();for (path,new) in changed.iter().map(|f|(&f.path,Some(f.sha256.clone()))).chain(removed.iter().map(|p|(p,None))){let target=guarded(root,path)?;changes.push(Change{path:path.clone(),old:if target.exists(){Some(hash(&target)?)}else{None},new});}let state=root.join(STATE);let previous_state=if state.exists(){Some(String::from_utf8(read_limited(&state,8388608)?).map_err(|_|invalid())?)}else{None};let backup=tempfile::Builder::new().prefix("backup-").tempdir_in(tx).map_err(|_|io_error())?;let backup_dir=backup.path().file_name().unwrap().to_string_lossy().into_owned();let _backup=backup.keep();let journal=Journal{schema:1,backup_dir,edition:source.edition.clone(),realm:source.realm.clone(),adapter:source.adapter.clone(),status:"prepared".into(),changes,previous_state};json(&tx.join("journal.json"),&journal)?;Ok(journal)}
fn apply(root:&Path,tx:&Path,journal:&mut Journal,hook:impl Fn(usize)->Result<()>)->Result<()> {
 journal.status="committing".into();json(&tx.join("journal.json"),journal)?;
 for (index,c) in journal.changes.iter().enumerate(){let target=guarded(root,&c.path)?;let backup=guarded(&tx.join(&journal.backup_dir),&c.path)?;
  if let Some(old)=&c.old{if hash(&target)?!=*old{return Err(error("client_changed","安装前检测到原文件已变化，请重试。"));}if let Some(p)=backup.parent(){fs::create_dir_all(p).map_err(|_|io_error())?;}fs::rename(&target,&backup).map_err(|_|io_error())?;}else if target.exists(){return Err(error("client_changed","安装目标新增了同名文件，请重试。"));}
  if let Some(new)=&c.new{let staged=guarded(&tx.join("staged"),&c.path)?;if hash(&staged)?!=new.to_lowercase(){return Err(error("client_checksum","暂存资源校验失败。"));}if let Some(p)=target.parent(){fs::create_dir_all(p).map_err(|_|io_error())?;}fs::rename(staged,&target).map_err(|_|io_error())?;}
  hook(index+1)?;
 }Ok(())
}
fn rollback(root:&Path,tx:&Path,journal:&mut Journal)->Result<()> {
 for c in journal.changes.iter().rev(){let target=guarded(root,&c.path)?;let backup=guarded(&tx.join(&journal.backup_dir),&c.path)?;
  if backup.exists(){if c.old.as_ref().is_none_or(|old|!hash(&backup).is_ok_and(|h|h==*old)){return Err(error("client_rollback_failed","旧版备份校验失败，需要人工检查更新目录。"));}if target.exists(){fs::remove_file(&target).map_err(|_|error("client_rollback_failed","无法恢复旧文件，请退出占用程序后重试。"))?;}fs::rename(backup,target).map_err(|_|io_error())?;}
  else if c.old.is_none()&&target.exists(){if c.new.as_ref().is_some_and(|new|hash(&target).is_ok_and(|h|h.eq_ignore_ascii_case(new))){fs::remove_file(target).map_err(|_|io_error())?;}else{return Err(error("client_rollback_failed","更新后文件被修改，已停止回滚以保留数据。"));}}
 }
 let state=root.join(STATE);if let Some(previous)=&journal.previous_state{atomic(&state,previous.as_bytes())?;}else if state.exists(){if reparse(&state){return Err(invalid());}fs::remove_file(state).map_err(|_|io_error())?;}
 journal.status="rolledBack".into();json(&tx.join("journal.json"),journal)?;Ok(())
}
fn recover(root:&Path,work:&Path,source:&Source)->Result<()> {for entry in fs::read_dir(work).map_err(|_|io_error())?{let entry=entry.map_err(|_|io_error())?;let name=entry.file_name().to_string_lossy().into_owned();if !hash_valid(&name){continue;}if reparse(&entry.path()){return Err(invalid());}let path=entry.path().join("journal.json");if !path.exists(){continue;}if reparse(&path){return Err(invalid());}let mut journal:Journal=serde_json::from_slice(&read_limited(&path,33554432)?).map_err(|_|invalid())?;if journal.schema!=1||!journal.backup_dir.starts_with("backup-")||journal.backup_dir.len()>80||!journal.backup_dir.bytes().all(|b|b.is_ascii_alphanumeric()||b==b'-')||journal.edition!=source.edition||journal.realm!=source.realm||journal.adapter!=source.adapter||journal.changes.len()>50000{return Err(invalid());}for c in &journal.changes{safe_relative(&c.path)?;if c.old.as_ref().is_some_and(|v|!hash_valid(v))||c.new.as_ref().is_some_and(|v|!hash_valid(v)){return Err(invalid());}}if journal.status=="committing"{rollback(root,&entry.path(),&mut journal)?;}}Ok(())}

pub fn game_running()->Result<bool>{
 #[cfg(windows)]{use std::os::windows::process::CommandExt;let executable=crate::client::profile()?.executable;let filter=format!("IMAGENAME eq {executable}");let expected=format!("\"{}\"",executable.to_ascii_lowercase());let path=PathBuf::from(std::env::var_os("SystemRoot").ok_or_else(io_error)?).join("System32/tasklist.exe");let output=std::process::Command::new(path).args(["/FI",filter.as_str(),"/FO","CSV","/NH"]).creation_flags(0x08000000).output().map_err(|_|error("game_check_failed","无法检查游戏进程，请稍后重试。"))?;if !output.status.success(){return Err(error("game_check_failed","无法确认游戏是否已退出。"));}Ok(String::from_utf8_lossy(&output.stdout).to_ascii_lowercase().contains(&expected))}
 #[cfg(not(windows))]{Err(error("game_check_failed","当前平台不支持游戏进程核验。"))}
}
#[cfg(test)]#[path="distribution_tests.rs"]mod tests;
