//! Bounded, owner-coordinated real-host acceptance probe. Reads private JSON
//! from stdin only; never starts a server, client host, DFO or database.
use meow_launcher_core::generic::{self,Target};
use serde::Deserialize;
use serde_json::{json,Value};
use sha2::{Digest,Sha256};
use std::{io::Read,time::Duration};
use tokio::io::{AsyncReadExt,AsyncWriteExt};
use zeroize::{Zeroize,Zeroizing};

#[derive(Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
struct Input {target:Target,accounts:Vec<Account>}
#[derive(Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
struct Account {
    username:String,password:String,
    #[serde(default)]register_first:bool,
    #[serde(default)]wrong_password:Option<String>,
    #[serde(default)]check_wrong_hostname:bool,
    #[serde(default)]check_wrong_scope:bool,
    #[serde(default)]native_steps:Vec<NativeStep>,
}
impl Drop for Account {fn drop(&mut self){self.password.zeroize();if let Some(p)=&mut self.wrong_password{p.zeroize();}}}
#[derive(Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
struct NativeStep {write_hex:String,read_length:usize,expected_sha256:String}
fn decode(value:&str)->Option<Vec<u8>> {
    if value.len()>131072 || value.len()%2!=0 || !value.bytes().all(|b|b.is_ascii_hexdigit()){return None;}
    (0..value.len()).step_by(2).map(|i|u8::from_str_radix(&value[i..i+2],16).ok()).collect()
}
fn digest(bytes:&[u8])->String {format!("{:x}",Sha256::digest(bytes))}
async fn account(target:&Target,a:&Account)->Result<Value,&'static str> {
    generic::validate_credentials(&a.username,&a.password).map_err(|_|"input")?;
    if a.native_steps.len()>32 || a.native_steps.iter().any(|s|s.read_length==0||s.read_length>1048576||s.expected_sha256.len()!=64||!s.expected_sha256.bytes().all(|b|b.is_ascii_hexdigit())||decode(&s.write_hex).is_none()){return Err("native-vector-input");}
    if a.register_first {generic::register(target,&a.username,Zeroizing::new(a.password.clone())).await.map_err(|_|"register")?;}
    let mut negatives=Vec::new();
    if let Some(wrong)=&a.wrong_password {
        if wrong==&a.password || generic::validate_credentials(&a.username,wrong).is_err(){return Err("wrong-password-vector");}
        if generic::login(target,&a.username,Zeroizing::new(wrong.clone())).await.is_ok(){return Err("wrong-password-accepted");}
        negatives.push("wrong-password-rejected");
    }
    if a.check_wrong_hostname {
        let mut wrong=target.clone();wrong.server_name="hostname-mismatch.invalid".into();
        if generic::login(&wrong,&a.username,Zeroizing::new(a.password.clone())).await.is_ok(){return Err("wrong-hostname-accepted");}
        negatives.push("wrong-hostname-rejected");
    }
    if a.check_wrong_scope {
        let mut wrong=target.clone();wrong.authority_id.push_str("-wrong");
        if generic::login(&wrong,&a.username,Zeroizing::new(a.password.clone())).await.is_ok(){return Err("wrong-scope-accepted");}
        negatives.push("wrong-scope-rejected-before-credentials");
    }
    let mut session=generic::login(target,&a.username,Zeroizing::new(a.password.clone())).await.map_err(|_|"login")?;
    let protocol=session.stream.get_ref().1.protocol_version();
    if protocol!=Some(rustls::ProtocolVersion::TLSv1_3){return Err("tls-version");}
    let mut steps=Vec::new();
    for vector in &a.native_steps {
        let bytes=decode(&vector.write_hex).ok_or("native-vector-input")?;
        session.stream.write_all(&bytes).await.map_err(|_|"native-write")?;
        session.stream.flush().await.map_err(|_|"native-write")?;
        let mut response=vec![0;vector.read_length];
        tokio::time::timeout(Duration::from_secs(10),session.stream.read_exact(&mut response)).await.map_err(|_|"native-timeout")?.map_err(|_|"native-read")?;
        let hash=digest(&response);
        if hash!=vector.expected_sha256.to_ascii_lowercase(){return Err("native-digest");}
        steps.push(json!({"written":bytes.len(),"read":response.len(),"sha256":hash,"matched":true}));
    }
    let identity=&session.identity;
    let result=json!({"registered":a.register_first,"reconnectedLogin":true,"tls":"TLS1.3","usernameSha256":digest(a.username.as_bytes()),"nativeAccountId":identity.native_account_id,"connectionId":identity.connection_id,"negativeChecks":negatives,"nativeSteps":steps,"sameTlsStream":true});
    session.stream.shutdown().await.map_err(|_|"tls-shutdown")?;
    Ok(result)
}
#[tokio::main]
async fn main() {
    let mut raw=Zeroizing::new(Vec::new());
    let input=match std::io::stdin().take(262145).read_to_end(&mut raw){Ok(_) if raw.len()<=262144=>serde_json::from_slice::<Input>(&raw).ok(),_=>None};
    drop(raw);
    let Some(input)=input else {println!("{}",json!({"passed":false,"phase":"input","noGame":true}));std::process::exit(1);};
    if input.target.validate().is_err() || input.accounts.is_empty() || input.accounts.len()>4 {println!("{}",json!({"passed":false,"phase":"input","noGame":true}));std::process::exit(1);}
    let mut reports=Vec::new();
    for a in &input.accounts {
        match account(&input.target,a).await {
            Ok(report)=>reports.push(report),
            Err(phase)=>{println!("{}",json!({"passed":false,"phase":phase,"completedAccounts":reports,"noGame":true,"noBackendStart":true}));std::process::exit(1);}
        }
    }
    println!("{}",json!({"passed":true,"scope":"Rust native generic adapter against an owner-managed real TLS target; only supplied native vectors","accounts":reports,"noGame":true,"noBackendStart":true}));
}
