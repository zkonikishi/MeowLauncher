//! Owner-coordinated LIVE local channel artifact acceptance. No DFO or host
//! startup. The owner wrapper verifies the artifact's current PID/listeners.
use meow_launcher_core::{generic::Target,generic_bridges::{LoggedIn,RunningBridges},generic_target::parse_local_artifact};
use serde::Deserialize;
use serde_json::json;
use sha2::{Digest,Sha256};
use std::{io::Read,net::SocketAddr,time::Duration};
use tokio::{io::AsyncReadExt,net::{TcpListener,TcpStream}};
use zeroize::{Zeroize,Zeroizing};
#[derive(Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
struct Input {target:Target,descriptor:serde_json::Value,username:String,password:String,expected_native_sha256:String,native_bytes:usize,route_indexes:Vec<usize>}
impl Drop for Input {fn drop(&mut self){self.password.zeroize();}}
async fn run(input:&Input)->Result<serde_json::Value,&'static str> {
    if input.native_bytes==0 || input.native_bytes>1048576 || input.expected_native_sha256.len()!=64 || !input.expected_native_sha256.bytes().all(|b|b.is_ascii_hexdigit()) || input.route_indexes.len()>4 {return Err("input");}
    let raw=serde_json::to_vec(&input.descriptor).map_err(|_|"input")?;
    let mapping=parse_local_artifact(&raw,&input.target).map_err(|_|"descriptor")?;
    if input.route_indexes.iter().any(|i|*i>=mapping.games.len()){return Err("route-index");}
    let mut probes=vec![("alias".to_owned(),"127.0.0.4:7002".parse::<SocketAddr>().unwrap())];
    for index in &input.route_indexes {let game=&mapping.games[*index];probes.push((format!("source-index-{index}-channel-{}",game.channel_id),format!("127.0.0.4:{}",game.local_port).parse().map_err(|_|"route-index")?));}
    let account=LoggedIn::authenticate(input.target.clone(),input.username.clone(),Zeroizing::new(input.password.clone())).await.map_err(|_|"login")?;
    let identity=account.identity.clone();
    let bridges=RunningBridges::start(account,mapping.games,7002,Some(mapping.directory)).await.map_err(|_|"bind")?;
    let bound=bridges.bound_addresses().to_vec();
    let mut rows=Vec::new();let mut failure=None;
    for (label,address) in probes {
        let result=tokio::time::timeout(Duration::from_secs(15),async {
            let mut stream=TcpStream::connect(address).await.map_err(|_|"bridge-connect")?;
            let mut bytes=vec![0u8;input.native_bytes];stream.read_exact(&mut bytes).await.map_err(|_|"bridge-native-read")?;
            let digest=format!("{:x}",Sha256::digest(&bytes));
            if digest!=input.expected_native_sha256.to_ascii_lowercase(){return Err("bridge-native-digest");}
            Ok(json!({"route":label,"bytes":bytes.len(),"sha256":digest,"nativeAccountId":identity.native_account_id,"matched":true}))
        }).await;
        match result {Ok(Ok(row))=>rows.push(row),Ok(Err(phase))=>{failure=Some(phase);break;},Err(_)=>{failure=Some("bridge-timeout");break;}}
    }
    bridges.close().await;
    // Rebinding verifies release of every listener, not just a completed task.
    let mut rebound=Vec::new();
    for address in &bound {rebound.push(TcpListener::bind(address).await.map_err(|_|"bridge-close-port-leak")?);}
    drop(rebound);
    if let Some(phase)=failure{return Err(phase);}
    Ok(json!({"passed":true,"scope":"Live Rust bridge bindings, selected authenticated native streams and complete listener cleanup; not all51 logins or directory packet/GUI/DFO acceptance","pid":std::process::id(),"boundAddresses":bound,"boundCount":bound.len(),"selectedRoutes":rows,"nativeAccountId":identity.native_account_id,"allPortsReleased":true,"directory":"raw-loopback bound only; packet protocol not probed","noGame":true,"noBackendStart":true}))
}
#[tokio::main]
async fn main() {
    let mut raw=Zeroizing::new(Vec::new());let parsed=match std::io::stdin().take(262145).read_to_end(&mut raw){Ok(_) if raw.len()<=262144=>serde_json::from_slice::<Input>(&raw).ok(),_=>None};drop(raw);
    let Some(input)=parsed else {println!("{}",json!({"passed":false,"phase":"input","noGame":true}));std::process::exit(1);};
    let result=run(&input).await;drop(input);
    match result {Ok(report)=>println!("{report}"),Err(phase)=>{println!("{}",json!({"passed":false,"phase":phase,"noGame":true,"noBackendStart":true}));std::process::exit(1);}}
}
