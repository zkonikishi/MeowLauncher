//! Private stdin control channel; actual native codec remains the host owner's
//! existing Go test. Never starts host/DFO or prints credentials.
use meow_launcher_core::{generic::Target,generic_bridges::{LoggedIn,RunningBridges},generic_target::parse_local_artifact};
use serde::Deserialize;
use serde_json::json;
use std::io::{BufRead,Read,Write};
use zeroize::Zeroizing;
#[derive(Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
struct Input {target:Target,descriptor:serde_json::Value,username:String,password:String}
#[tokio::main]
async fn main(){
 let result=run().await;
 if let Err(phase)=result{println!("{}",json!({"passed":false,"phase":phase,"noGame":true}));std::process::exit(1);}
}
async fn run()->Result<(),&'static str>{
 let mut line=Zeroizing::new(String::new());
 std::io::stdin().lock().take(262145).read_line(&mut line).map_err(|_|"input")?;
 if line.len()>262144{return Err("input");}
 let input:Input=serde_json::from_str(&line).map_err(|_|"input")?;drop(line);
 let mapping=parse_local_artifact(&serde_json::to_vec(&input.descriptor).map_err(|_|"input")?,&input.target).map_err(|_|"descriptor")?;
 let account=LoggedIn::authenticate(input.target,input.username,Zeroizing::new(input.password)).await.map_err(|_|"login")?;
 let identity=account.identity.native_account_id.clone();
 let game_address=format!("127.0.0.4:{}",mapping.games.first().ok_or("descriptor")?.local_port);
 let directory_address=format!("127.0.0.4:{}",mapping.directory.local_port);
 let bridges=RunningBridges::start(account,mapping.games,7002,Some(mapping.directory)).await.map_err(|_|"bind")?;
 let addresses=bridges.bound_addresses().to_vec();
 println!("{}",json!({"phase":"ready","nativeAccountId":identity,"boundCount":addresses.len(),"gameAddress":game_address,"directoryAddress":directory_address,"directoryAliasAddress":"127.0.0.4:7002","noGame":true}));std::io::stdout().flush().map_err(|_|"stdout")?;
 // EOF, explicit done, or timeout all close only this process's bridges.
 let control=tokio::task::spawn_blocking(||{let mut line=String::new();let _=std::io::stdin().lock().read_line(&mut line);line});
 let done=tokio::time::timeout(std::time::Duration::from_secs(90),control).await;
 bridges.close().await;
 for address in addresses {let listener=tokio::net::TcpListener::bind(address).await.map_err(|_|"cleanup-port")?;drop(listener);}
 println!("{}",json!({"phase":"closed","passed":done.is_ok(),"allPortsReleased":true,"noGame":true}));
 // Blocking stdin worker must not prolong a timed-out acceptance process.
 if done.is_err(){std::process::exit(2);}
 Ok(())
}
