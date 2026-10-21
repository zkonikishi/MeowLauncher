//! Explicit LOCAL, unsigned-r1 client validation path. Not an installer or a
//! production descriptor; nothing launches unless the user clicks Start.
use meow_launcher_core::{client_host,generic::Target,generic_bridges::{LoggedIn,RunningBridges,DirectoryTransport,probe_directory_tls},generic_target::{parse_local_artifact,parse_remote_artifact,LocalMapping},error,Result};
use serde::{Deserialize,Serialize};
use sha2::{Digest,Sha256};
use std::{fs::File,io::Read,net::{IpAddr,SocketAddr},path::{Path,PathBuf},process::Stdio,sync::{Arc,atomic::{AtomicBool,Ordering}},time::{Duration,SystemTime,UNIX_EPOCH}};
use tauri::Emitter;
use tokio::{io::{AsyncReadExt,BufReader},process::{Child,ChildStdout},sync::watch,task::JoinHandle};

const HOST_SHA:&str="dd6ac277206e5f29e24e0d5e1f332842b540d74949b73d1af3a4e7226982b1f8";
const DFO_SHA:&str="b8235efe5dbf140ea4c535a026e137c4b34cfaecd7c97c78ed5997c377327c95";
#[derive(Clone,Deserialize,Serialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
pub struct Config {#[serde(default)]pub use_bundled_host:bool,#[serde(default)]pub host_executable:String,pub player_target_file:String,#[serde(default)]pub ready_file:String,pub allow_unsigned_candidate:bool,#[serde(default)]pub mode:Mode}
#[derive(Clone,Copy,Default,PartialEq,Deserialize,Serialize)]
#[serde(rename_all="lowercase")]
pub enum Mode {#[default]Local,Remote}
impl Config {pub fn validate(&self)->Result<()> {
    if self.use_bundled_host && !self.host_executable.is_empty(){return Err(error("client_review_config","内置宿主模式不能同时指定其他宿主。"));}
    let required=if self.use_bundled_host{vec![&self.player_target_file]}else{vec![&self.host_executable,&self.player_target_file]};
    for value in required{if value.len()>4096||value.contains('\0')||!Path::new(value).is_absolute(){return Err(error("client_review_config","候选验证路径必须为完整绝对路径。"));}}
    if self.mode==Mode::Local && (self.ready_file.len()>4096||self.ready_file.contains('\0')||!Path::new(&self.ready_file).is_absolute()){return Err(error("client_review_config","本机验证需要本轮 ready.json 的完整路径。"));}
    if self.mode==Mode::Remote && !self.ready_file.is_empty(){return Err(error("client_review_config","远端验证不读取本机 ready/PID 文件，请清空 ready.json 路径。"));}
    Ok(())
}}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct HostReady {pid:u32,address:String,advertise:String,channels:usize,fixture_bytes:usize}
#[derive(Clone,Serialize)]
#[serde(rename_all="camelCase")]
pub struct Started {pub instance_id:String,pub host_pid:u32,pub process_id:u32,pub status:&'static str,pub message:&'static str}
pub struct Running {pub started:Started,cancel:watch::Sender<bool>,task:Option<JoinHandle<()>>,cleanup_failed:Arc<AtomicBool>}
impl Running {
    pub fn active(&self)->bool{self.task.as_ref().is_some_and(|t|!t.is_finished())}
    pub fn quarantined(&self)->bool{self.cleanup_failed.load(Ordering::SeqCst)}
    pub async fn stop(&mut self)->Result<()> {
        let _=self.cancel.send(true);
        if let Some(mut task)=self.task.take(){if tokio::time::timeout(Duration::from_secs(10),&mut task).await.is_err(){task.abort();let _=task.await;self.cleanup_failed.store(true,Ordering::SeqCst);}}
        if self.quarantined(){Err(error("client_cleanup_failed","实例清理状态不确定，保留隔离状态；请检查本次日志。"))}else{Ok(())}
    }
}
impl Drop for Running {fn drop(&mut self){let _=self.cancel.send(true);if let Some(task)=self.task.take(){task.abort();}}}
fn failure()->meow_launcher_core::AppError{error("client_review_failed","本地客户端验证未完成，未报告游戏启动。请查看本次实例日志。")}
fn file_time(time:SystemTime)->Result<u64>{let ticks=time.duration_since(UNIX_EPOCH).map_err(|_|failure())?.as_nanos()/100;u64::try_from(ticks).ok().and_then(|v|v.checked_add(116444736000000000)).ok_or_else(failure)}
fn bytes(path:&Path,limit:u64)->Result<Vec<u8>> {
    if path.is_symlink(){return Err(failure());}
    let mut data=Vec::new();File::open(path).map_err(|_|failure())?.take(limit+1).read_to_end(&mut data).map_err(|_|failure())?;
    if data.len() as u64>limit{return Err(failure());}Ok(data)
}
fn pinned(path:&Path,sha:&str)->Result<File> {
    if !path.is_file()||path.is_symlink(){return Err(error("client_host_integrity","客户端或严格宿主文件不存在。"));}
    let mut options=std::fs::OpenOptions::new();options.read(true);
    #[cfg(windows)] {use std::os::windows::fs::OpenOptionsExt;options.share_mode(1);}
    let mut file=options.open(path).map_err(|_|error("client_host_integrity","无法锁定并核验客户端文件。"))?;
    let mut hash=Sha256::new();let mut buffer=[0u8;65536];loop {let n=file.read(&mut buffer).map_err(|_|failure())?;if n==0{break;}hash.update(&buffer[..n]);}
    if format!("{:x}",hash.finalize())!=sha{return Err(error("client_host_integrity","文件摘要与当前候选版本不符；未执行或替换任何程序。"));}Ok(file)
}
async fn owner(config:&Config,target:&Target,mapping:&LocalMapping)->Result<()> {
    let ready_path=Path::new(&config.ready_file);let target_path=Path::new(&config.player_target_file);
    if ready_path.file_name().and_then(|s|s.to_str())!=Some("ready.json")||target_path.file_name().and_then(|s|s.to_str())!=Some("player-target.json")||ready_path.parent()!=target_path.parent(){return Err(error("client_review_config","请选择同一运行目录的 ready.json 与 player-target.json。"));}
    let ready:HostReady=serde_json::from_slice(&bytes(ready_path,16384)?).map_err(|_|failure())?;
    let host:IpAddr=target.host.parse().map_err(|_|failure())?;
    let expected=SocketAddr::new(host,target.port);
    if ready.pid==0||ready.channels!=51||ready.fixture_bytes==0||ready.address!=expected.to_string()||ready.advertise!=format!("127.0.0.4:{}",target.port){return Err(failure());}
    #[cfg(windows)] {
        let observation=client_host::observe_local_owner(ready.pid).await?;
        let modified=file_time(std::fs::metadata(ready_path).and_then(|m|m.modified()).map_err(|_|failure())?)?;
        let created=observation.process.created_file_time_utc.parse::<u64>().map_err(|_|failure())?;
        if observation.process.pid!=ready.pid||created>modified||modified>file_time(SystemTime::now())?{return Err(error("client_review_stale","宿主已退出、PID已改变或就绪文件过期，请重新获取本轮配置。"));}
        let contains=|address:&str,port:u16|observation.listeners.iter().any(|p|p.address==address&&p.port==port);
        if mapping.games.iter().any(|g|!contains(&g.upstream.host,g.upstream.port))||!contains(&mapping.directory.upstream.ip().to_string(),mapping.directory.upstream.port()){return Err(error("client_review_stale","频道监听不属于本轮宿主，未启动客户端。"));}
    }
    #[cfg(not(windows))] {return Err(failure());}
    Ok(())
}
async fn line(reader:&mut BufReader<ChildStdout>)->Result<String> {
    let mut bytes=Vec::new();loop {let byte=reader.read_u8().await.map_err(|_|failure())?;if byte==b'\n'{break;}bytes.push(byte);if bytes.len()>16384{return Err(failure());}}
    String::from_utf8(bytes).map_err(|_|failure())
}
async fn ready(reader:&mut BufReader<ChildStdout>,pid:u32,client:&Path)->Result<client_host::Ready> {
    loop {let line=line(reader).await?;if line.contains("CLIENTHOST_READY_V1"){return client_host::parse_ready(&line,pid,client);}}
}
enum OwnedBridges {Simple(RunningBridges),Controlled(meow_launcher_core::controlled_bridges::RunningBridges)}
impl OwnedBridges {async fn close(self){match self{Self::Simple(b)=>b.close().await,Self::Controlled(b)=>b.close().await}}}
async fn stop_failed(child:&mut Child,bridges:OwnedBridges){let _=child.kill().await;let _=child.wait().await;bridges.close().await;}

pub async fn start(config:&Config,target:&Target,account:Arc<LoggedIn>,directory:&Path,data:&Path,window:tauri::WebviewWindow)->Result<Running> {
    config.validate()?;
    if !config.use_bundled_host && !config.allow_unsigned_candidate{return Err(error("client_review_unsigned","此宿主是未签名候选，默认不执行；仅在本地候选验证设置中显式启用。"));}
    #[cfg(windows)] {
        #[link(name="shell32")] extern "system" {fn IsUserAnAdmin()->i32;}
        if unsafe{IsUserAnAdmin()}==0{return Err(error("client_host_admin","严格客户端网络隔离需要管理员权限，请手动以管理员身份运行启动器。"));}
    }
    #[cfg(not(windows))] {return Err(failure());}
    let raw=bytes(Path::new(&config.player_target_file),65536)?;
    let (games,directory_route)=match config.mode {
        Mode::Local=>{let mapping=parse_local_artifact(&raw,target)?;owner(config,target,&mapping).await?;(mapping.games,DirectoryTransport::Loopback(mapping.directory))},
        Mode::Remote=>{let mapping=parse_remote_artifact(&raw,target)?;(mapping.games,DirectoryTransport::Tls(mapping.directory))},
    };
    let host=if config.use_bundled_host{super::client_host_bundle::materialize(data)?}else{Path::new(&config.host_executable).canonicalize().map_err(|_|failure())?};
    let client_directory=directory.canonicalize().map_err(|_|error("client_host_integrity","请选择完整的 DFO 客户端目录。"))?;
    let launcher=std::env::current_exe().and_then(|p|p.canonicalize()).map_err(|_|failure())?;
    client_host::require_bridge_outside_client_root(&launcher,&client_directory)?;
    let client=client_directory.join("DFO.exe");
    let guards=vec![pinned(&host,HOST_SHA)?,pinned(&client,DFO_SHA)?];
    account.refresh().await?;
    // Recheck owner after file hashing/authentication, before acquiring routes.
    if config.mode==Mode::Local {let mapping=parse_local_artifact(&raw,target)?;owner(config,target,&mapping).await?;}
    if let DirectoryTransport::Tls(directory)=&directory_route {probe_directory_tls(&directory.upstream).await?;}
    let bridges=RunningBridges::start_transport(account,games,7002,Some(directory_route)).await?;
    launch(OwnedBridges::Simple(bridges),guards,host,client_directory,client,data,window).await
}

pub async fn start_controlled(host_path:&Path,coordinator:&meow_launcher_core::controlled_coordinator::Coordinator,directory:&Path,data:&Path,window:tauri::WebviewWindow)->Result<Running>{
    #[cfg(windows)] {#[link(name="shell32")] extern "system"{fn IsUserAnAdmin()->i32;}if unsafe{IsUserAnAdmin()}==0{return Err(error("client_host_admin","请以管理员身份运行启动器后开始游戏。"));}}
    #[cfg(not(windows))] {return Err(failure());}
    let host=host_path.canonicalize().map_err(|_|error("client_host_integrity","缺少客户端连接组件，请重新获取完整启动器。"))?;
    let client_directory=directory.canonicalize().map_err(|_|error("client_host_integrity","请选择完整的 DFO 客户端目录。"))?;
    let launcher=std::env::current_exe().and_then(|p|p.canonicalize()).map_err(|_|failure())?;
    client_host::require_bridge_outside_client_root(&launcher,&client_directory)?;
    let client=client_directory.join("DFO.exe");let guards=vec![pinned(&host,HOST_SHA)?,pinned(&client,DFO_SHA)?];
    super::game_window::apply_saved(data)?;
    let mut target=meow_launcher_core::max_production::endpoint();target.port=meow_launcher_core::max_production::DIRECTORY_PORT;
    let mut stream=target.connect().await?;tokio::io::AsyncWriteExt::shutdown(&mut stream).await.map_err(|_|failure())?;drop(stream);
    let prepared=coordinator.prepare_game_logged(Some(data.to_path_buf())).await?;
    launch(OwnedBridges::Controlled(prepared.bridges),guards,host,client_directory,client,data,window).await
}
async fn launch(bridges:OwnedBridges,guards:Vec<File>,host:PathBuf,client_directory:PathBuf,client:PathBuf,data:&Path,window:tauri::WebviewWindow)->Result<Running>{
    let id=format!("instance-{}-{}",std::process::id(),SystemTime::now().duration_since(UNIX_EPOCH).map_err(|_|failure())?.as_nanos());
    let logs=data.join("instances").join(&id);std::fs::create_dir_all(logs.parent().ok_or_else(failure)?).map_err(|_|failure())?;std::fs::create_dir(&logs).map_err(|_|failure())?;
    let plan=client_host::strict_plan(&host,&client_directory,&logs.join("client.log"),&logs.join("breakpoints.txt"),7002)?;
    let stderr=File::create(logs.join("host.stderr.log")).map_err(|_|failure())?;
    let mut command=std::process::Command::new(&plan.executable);command.args(&plan.arguments).current_dir(&plan.working_directory).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::from(stderr));
    command.env_remove("DFO_ACCOUNT");
    #[cfg(windows)] {use std::os::windows::process::CommandExt;command.creation_flags(0x08000000);}
    let before=file_time(SystemTime::now())?;
    let mut command=tokio::process::Command::from(command);command.kill_on_drop(true);
    let mut child=command.spawn().map_err(|_|failure())?;let host_pid=child.id().ok_or_else(failure)?;
    let stdout=child.stdout.take().ok_or_else(failure)?;let mut reader=BufReader::new(stdout);
    let announced=tokio::time::timeout(Duration::from_secs(35),ready(&mut reader,host_pid,&client)).await;
    let announced=match announced {Ok(Ok(value))=>value,_=>{stop_failed(&mut child,bridges).await;return Err(failure());}};
    // Retain a read-only OS handle before the second identity observation so
    // the observer cannot accidentally attach to a subsequently reused PID.
    #[cfg(windows)]
    let native_process=super::client_exit::Process::open(announced.client_pid,&client,before,file_time(SystemTime::now())?).ok();
    #[cfg(windows)] {
        let observed=client_host::observe_processes(host_pid,announced.client_pid).await;
        let verified=observed.and_then(|rows|client_host::verify_observations(&announced,&host,&client,before,file_time(SystemTime::now())?,&rows));
        if let Err(e)=verified{stop_failed(&mut child,bridges).await;return Err(e);}
    }
    let started=Started{instance_id:id,host_pid,process_id:announced.client_pid,status:"running",message:"客户端进程已创建并通过严格隔离就绪核验；进入游戏仍需客户端验证。"};
    let exited=started.clone();let (cancel,mut cancellation)=watch::channel(false);
    let cleanup_failed=Arc::new(AtomicBool::new(false));let cleanup=cleanup_failed.clone();
    let requested_cancel=Arc::new(AtomicBool::new(false));let observer_cancel=requested_cancel.clone();
    #[cfg(windows)]
    let native_observer=native_process.map(|process|super::client_exit::observe(process,&logs,DFO_SHA,observer_cancel));
    #[cfg(windows)]
    if native_observer.is_none(){let _=std::fs::write(logs.join("native-observer.json"),b"{\"schema\":\"native-observer-v1\",\"armed\":false}");}
    let host_exit_path=logs.join("host-exit.json");
    let settings_file=data.join("settings.json");
    let task=tokio::spawn(async move {
        let _guards=guards;
        let drain=tokio::spawn(async move {let _=tokio::io::copy(&mut reader,&mut tokio::io::sink()).await;});
        let result=tokio::select!{result=child.wait()=>result,_=cancellation.changed()=>{requested_cancel.store(true,Ordering::SeqCst);let _=child.kill().await;child.wait().await}};
        drain.abort();let _=drain.await;bridges.close().await;
        let (exit_code,wait_failed)=match result{Ok(status)=>(status.code(),false),Err(_)=>(None,true)};
        // Host status is never presented as the native game's crash code.
        let host_record=serde_json::json!({"schema":"host-exit-v1","recordedAtUnixSeconds":meow_launcher_core::unix_now(),"hostExitCode":exit_code,"waitFailed":wait_failed,"launcherCancelRequested":requested_cancel.load(Ordering::SeqCst)});
        if let Ok(bytes)=serde_json::to_vec_pretty(&host_record){let _=std::fs::write(host_exit_path,bytes);}
        #[cfg(windows)]
        // Detached read-only observation owns its process handle. Delayed WER
        // lookup must not delay cleanup, audio restoration or the exit event.
        if let Some(observer)=native_observer{drop(observer);}
        let failed=wait_failed||exit_code==Some(13);cleanup.store(failed,Ordering::SeqCst);
        let _=window.emit("game-exited",serde_json::json!({"instanceId":exited.instance_id,"processId":exited.process_id,"exitCode":exit_code,"cleanupFailed":failed}));
        if meow_launcher_core::settings::load(&settings_file).map(|s|s.restore_after_game).unwrap_or(true){let _=window.show();let _=window.unminimize();let _=window.set_focus();}
    });
    Ok(Running{started,cancel,task:Some(task),cleanup_failed})
}
