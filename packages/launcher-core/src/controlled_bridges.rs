//! Max game streams require a fresh same-socket platform assertion per
//! connection. Directory TLS is a separate transport, never an auth fallback.
use crate::{controlled::{self,Endpoint,Scope},controlled_exchange::ExchangeClient,controlled_target::Mapping,error,Result};
use std::{collections::HashSet,net::{Ipv4Addr,SocketAddr},sync::Arc};
use tokio::{net::{TcpListener,TcpStream},sync::{watch,Mutex},task::{JoinHandle,JoinSet}};
use tokio_rustls::client::TlsStream;
use zeroize::Zeroizing;
use crate::bridge_observation::{copy_observed,Event as FlowEvent};
fn rejected()->crate::AppError{error("controlled_bridge","平台游戏连接未完成，请重新登录后重试。")}
pub struct Account{endpoint:Endpoint,scope:Scope,exchange:Arc<ExchangeClient>,session:Zeroizing<String>,deadline:std::time::Instant,initial:Mutex<Option<TlsStream<TcpStream>>>}
impl Account{
 pub async fn new(endpoint:Endpoint,scope:Scope,exchange:Arc<ExchangeClient>,session:Zeroizing<String>,valid_for:std::time::Duration)->Result<Arc<Self>>{
  endpoint.validate()?;scope.validate()?;
  if valid_for.as_secs()<=5||valid_for.as_secs()>86400{return Err(rejected());}
  let deadline=std::time::Instant::now().checked_add(valid_for).ok_or_else(rejected)?;
  let account=Arc::new(Self{endpoint,scope,exchange,session,deadline,initial:Mutex::new(None)});
  let stream=account.authenticate(account.endpoint.port).await?;*account.initial.lock().await=Some(stream);Ok(account)
 }
 async fn authenticate(&self,port:u16)->Result<TlsStream<TcpStream>>{
  if self.deadline.saturating_duration_since(std::time::Instant::now()).as_secs()<=5{return Err(error("login_required","登录已过期，请重新登录。"));}
  let mut target=self.endpoint.clone();target.port=port;let mut stream=target.connect().await?;
  controlled::authenticate_tls(&mut stream,&self.scope,|challenge|async move{self.exchange.exchange(&challenge,&self.session).await}).await?;
  Ok(stream)
 }
 async fn stream(&self,port:u16)->Result<TlsStream<TcpStream>>{
  if self.deadline.saturating_duration_since(std::time::Instant::now()).as_secs()<=5{return Err(error("login_required","登录已过期，请重新登录。"));}
  // Never hand an idle preflight connection to the game. Each actual game socket
  // obtains and consumes its own same-socket assertion, including primary.
  self.initial.lock().await.take();
  self.authenticate(port).await
 }
}
#[derive(Clone,Copy)]struct Route{port:u16,upstream_port:u16,directory:bool}
fn address(port:u16)->SocketAddr{SocketAddr::from((Ipv4Addr::new(127,0,0,4),port))}
fn plan(mapping:&Mapping,primary:u16)->Result<Vec<Route>>{
 if mapping.routes.len()!=51||mapping.directory_port==0||mapping.directory_port==7002{return Err(rejected());}
 let mut ids=HashSet::new();let mut ports=HashSet::new();let mut result=Vec::new();
 for route in &mapping.routes{
  let local:SocketAddr=route.advertise_address.parse().map_err(|_|rejected())?;
  let listen:SocketAddr=route.listen_address.parse().map_err(|_|rejected())?;
  if local!=address(local.port())||local.port()==0||local.port()==7002||listen.port()!=local.port()||!ids.insert(route.channel_id)||!ports.insert(local.port()){return Err(rejected());}
  result.push(Route{port:local.port(),upstream_port:local.port(),directory:false});
 }
 if result[0].port!=primary||ports.contains(&mapping.directory_port){return Err(rejected());}
 result.push(Route{port:7002,upstream_port:mapping.directory_port,directory:true});result.push(Route{port:mapping.directory_port,upstream_port:mapping.directory_port,directory:true});Ok(result)
}
fn bridge_event(dir:Option<&std::path::Path>,route:Route,phase:&'static str,outgoing:u64,incoming:u64){
 use std::io::Write;
 let Some(dir)=dir else{return;};
 let path=dir.join("bridge-events.log");
 if path.metadata().is_ok_and(|m|m.len()>262144){let previous=dir.join("bridge-events.previous.log");if previous.exists()&&std::fs::remove_file(&previous).is_err(){return;}if std::fs::rename(&path,previous).is_err(){return;}}
 if let Ok(mut file)=std::fs::OpenOptions::new().create(true).append(true).open(path){let _=writeln!(file,"{} phase={} local={} upstream={} directory={} sent={} received={}",crate::unix_now(),phase,route.port,route.upstream_port,route.directory,outgoing,incoming);}
}
pub struct RunningBridges{cancel:watch::Sender<bool>,workers:Vec<JoinHandle<()>>,addresses:Vec<SocketAddr>}
impl RunningBridges{
 pub async fn start(account:Arc<Account>,mapping:Mapping)->Result<Self>{Self::start_logged(account,mapping,None).await}
 pub async fn start_logged(account:Arc<Account>,mapping:Mapping,diagnostics:Option<std::path::PathBuf>)->Result<Self>{
  let routes=plan(&mapping,account.endpoint.port)?;let mut bound=Vec::new();
  // Binding all routes before spawning prevents a partial running set.
  for route in routes{let listener=TcpListener::bind(address(route.port)).await.map_err(|_|rejected())?;bound.push((listener,route));}
  let addresses=bound.iter().map(|(l,_)|l.local_addr().map_err(|_|rejected())).collect::<Result<Vec<_>>>()?;
  let(cancel,rx)=watch::channel(false);let mut workers=Vec::new();
  for(listener,route)in bound{let diagnostics=diagnostics.clone();let account=account.clone();let mut rx=rx.clone();let cancel=cancel.clone();workers.push(tokio::spawn(async move{
   let mut connections=JoinSet::new();
   loop{if *rx.borrow(){break;}
    tokio::select!{
     _=rx.changed()=>break,
     _=connections.join_next(),if !connections.is_empty()=>{},
     accepted=listener.accept(),if connections.len()<128=>{
      let Ok((mut local,_))=accepted else{let _=cancel.send(true);break;};let account=account.clone();let diagnostics=diagnostics.clone();let mut stop=rx.clone();
      connections.spawn(async move{tokio::select!{
       _=stop.changed()=>{},
       _=async{
        let port=route.upstream_port;
        bridge_event(diagnostics.as_deref(),route,"accepted",0,0);
        let connected=if route.directory{let mut endpoint=account.endpoint.clone();endpoint.port=port;endpoint.connect().await}else{account.stream(port).await};
        match connected{Ok(mut upstream)=>{bridge_event(diagnostics.as_deref(),route,"connected",0,0);let flow_dir=diagnostics.clone(); let flow_id=NEXT_FLOW.fetch_add(1,std::sync::atomic::Ordering::Relaxed); let flow_start=std::time::Instant::now(); let observer=Arc::new(move|event|bridge_flow_event(flow_dir.as_deref(),route,flow_id,flow_start,event)); match copy_observed(&mut local,&mut upstream,observer).await{Ok((outgoing,incoming))=>bridge_event(diagnostics.as_deref(),route,"closed",outgoing,incoming),Err(_)=>bridge_event(diagnostics.as_deref(),route,"copy_failed",0,0)}},Err(_)=>bridge_event(diagnostics.as_deref(),route,"connect_failed",0,0)}
       }=>{}
      }});
     }
    }
   }
   connections.abort_all();while connections.join_next().await.is_some(){}
  }));}
  Ok(Self{cancel,workers,addresses})
 }
 pub fn bound_addresses(&self)->&[SocketAddr]{&self.addresses}
 pub fn is_running(&self)->bool{!*self.cancel.borrow()&&self.workers.iter().all(|w|!w.is_finished())}
 pub async fn close(mut self){let _=self.cancel.send(true);for worker in self.workers.drain(..){let _=worker.await;}}
}
impl Drop for RunningBridges{fn drop(&mut self){let _=self.cancel.send(true);for worker in &self.workers{worker.abort();}}}

#[cfg(test)]mod tests{
 use super::*;use crate::controlled_target::Route as MappedRoute;
 fn mapping()->Mapping{Mapping{routes:(0..51).map(|i|MappedRoute{channel_id:1000-i,listen_address:format!("0.0.0.0:{}",30000+i),advertise_address:format!("127.0.0.4:{}",30000+i)}).collect(),directory_port:30051,artifact_sha256:"test".into()}}
 #[test]fn fifty_one_explicit_routes_alias_and_directory_without_channel_arithmetic(){let m=mapping();let routes=plan(&m,30000).unwrap();assert_eq!(routes.len(),53);assert_eq!(routes[1].port,30001);assert!(routes[51].directory);assert_eq!(routes[51].upstream_port,m.directory_port);assert!(routes[52].directory);assert_eq!(routes[52].upstream_port,m.directory_port);for route in &routes[..51]{assert!(!route.directory);assert_eq!(route.port,route.upstream_port);}assert!(routes[52].directory);assert_eq!(routes[51].port,7002);}
 #[test]fn repeated_metadata_rotation_does_not_grow_after_first_backup(){
  let root=std::env::temp_dir().join(format!("bridge-rotation-qa-{}-{}",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));std::fs::create_dir(&root).unwrap();
  let route=Route{port:7002,upstream_port:27001,directory:true};
  for _ in 0..2{std::fs::write(root.join("bridge-events.log"),vec![0;262145]).unwrap();bridge_event(Some(&root),route,"closed",1,2);assert!(std::fs::metadata(root.join("bridge-events.log")).unwrap().len()<1024);}
  assert_eq!(std::fs::read_dir(&root).unwrap().count(),2);std::fs::remove_dir_all(root).unwrap();
 }
 #[test]fn duplicate_ports_foreign_advertisements_and_directory_conflicts_rejected(){let mut m=mapping();m.directory_port=30000;assert!(plan(&m,30000).is_err());let mut m=mapping();m.routes[1].channel_id=m.routes[0].channel_id;assert!(plan(&m,30000).is_err());let mut m=mapping();m.routes[0].advertise_address="203.0.113.1:30000".into();assert!(plan(&m,30000).is_err());assert!(plan(&mapping(),30001).is_err());}
}

// Metadata only: no packet bytes, identities, tokens or raw error strings.
static NEXT_FLOW: std::sync::atomic::AtomicU64=std::sync::atomic::AtomicU64::new(1);
fn bridge_flow_event(dir:Option<&std::path::Path>,route:Route,id:u64,start:std::time::Instant,event:FlowEvent){
 use std::io::Write;
 static LOG_LOCK:std::sync::Mutex<()>=std::sync::Mutex::new(());
 let Some(dir)=dir else{return;};let Ok(_guard)=LOG_LOCK.lock()else{return;};
 let path=dir.join("bridge-flow-events.log");
 if path.metadata().is_ok_and(|m|m.len()>262144){let previous=dir.join("bridge-flow-events.previous.log");if previous.exists()&&std::fs::remove_file(&previous).is_err(){return;}if std::fs::rename(&path,previous).is_err(){return;}}
 let utc_ms=std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|v|v.as_millis()).unwrap_or(0);
 if let Ok(mut file)=std::fs::OpenOptions::new().create(true).append(true).open(path){
  let _=writeln!(file,"utc_ms={} connection={} elapsed_ms={} local={} upstream={} directory={} side={:?} event={:?} category={} read_bytes={} written_bytes={}",utc_ms,id,start.elapsed().as_millis(),route.port,route.upstream_port,route.directory,event.side,event.terminal,event.category,event.bytes_read,event.bytes_written);
 }
}
