//! Native-only generic provider seam. Max platform sessions use a separate provider.
use crate::{error,generic::{Identity,Target},generic_bridges::LoggedIn,launch_contract::{AdmissionTicket,Scope,Snapshot},Result};
use std::{future::Future,pin::Pin,sync::Arc};
use zeroize::Zeroizing;
pub type NativeFuture<'a,T> = Pin<Box<dyn Future<Output=Result<T>>+Send+'a>>;
pub trait AccountProvider: Send+Sync {
    type Session: Send+Sync;
    fn authenticate<'a>(&'a self, username:String, password:Zeroizing<String>)->NativeFuture<'a,Self::Session>;
    fn register<'a>(&'a self, username:String, password:Zeroizing<String>)->NativeFuture<'a,()>;
}
pub struct SimpleGatewayProvider { target:Target }
impl SimpleGatewayProvider {
    pub fn new(target:Target)->Result<Self> {target.validate()?;Ok(Self{target})}
}
impl AccountProvider for SimpleGatewayProvider {
    type Session=Arc<LoggedIn>;
    fn authenticate<'a>(&'a self,username:String,password:Zeroizing<String>)->NativeFuture<'a,Self::Session> {
        Box::pin(LoggedIn::authenticate(self.target.clone(),username,password))
    }
    fn register<'a>(&'a self,username:String,password:Zeroizing<String>)->NativeFuture<'a,()> {
        Box::pin(async move {crate::generic::register(&self.target,&username,password).await})
    }
}
/// Identity is obtained from the authenticated session, never renderer-supplied UID.
pub fn simple_scope(identity:&Identity)->Scope {
    Scope {edition:crate::launch_contract::Edition::Beta,authority:identity.authority_id.clone(),account:identity.native_account_id.clone(),target:format!("{}/{}",identity.realm_id,identity.save_profile_id)}
}
pub trait NativeLaunchAdapter: Send+Sync {
    fn bind<'a>(&'a self,scope:&'a Scope,request_id:&'a str,ticket:&'a AdmissionTicket)->NativeFuture<'a,Snapshot>;
    fn query<'a>(&'a self,scope:&'a Scope,request_id:&'a str)->NativeFuture<'a,Snapshot>;
    fn cancel<'a>(&'a self,scope:&'a Scope,request_id:&'a str)->NativeFuture<'a,Snapshot>;
}
/// Authentication session and game ticket are separate native types. A provider
/// implementation must issue/query/cancel using the same authenticated request.
/// HTTP paths and wire fields are intentionally not defined by this seam.
pub trait GameAdmissionProvider<S: Send+Sync>: Send+Sync {
    fn issue<'a>(&'a self,session:&'a S,scope:&'a Scope,request_id:&'a str)->NativeFuture<'a,AdmissionTicket>;
    fn query<'a>(&'a self,session:&'a S,scope:&'a Scope,request_id:&'a str)->NativeFuture<'a,Snapshot>;
    fn cancel<'a>(&'a self,session:&'a S,scope:&'a Scope,request_id:&'a str)->NativeFuture<'a,Snapshot>;
}
/// Production v2 cannot start until the host's versioned binding contract exists.
pub struct PendingNativeAdapter;
impl NativeLaunchAdapter for PendingNativeAdapter {
    fn bind<'a>(&'a self,_:&'a Scope,_:&'a str,_:&'a AdmissionTicket)->NativeFuture<'a,Snapshot>{Box::pin(async{Err(pending())})}
    fn query<'a>(&'a self,_:&'a Scope,_:&'a str)->NativeFuture<'a,Snapshot>{Box::pin(async{Err(pending())})}
    fn cancel<'a>(&'a self,_:&'a Scope,_:&'a str)->NativeFuture<'a,Snapshot>{Box::pin(async{Err(pending())})}
}
fn pending()->crate::AppError {error("native_adapter_pending","原生准入适配器尚未接通，未报告游戏启动。")}
#[cfg(test)] mod tests {
 use super::*;
 #[tokio::test] async fn pending_adapter_never_reports_success_or_fake_cancel(){let scope=Scope{edition:crate::launch_contract::Edition::Beta,authority:"owner".into(),account:"account".into(),target:"target".into()};let ticket=AdmissionTicket::received(Zeroizing::new("a".repeat(32)),scope.clone(),"request-1".into(),100,400,101).unwrap();let a=PendingNativeAdapter;for r in [a.bind(&scope,"request-1",&ticket).await,a.query(&scope,"request-1").await,a.cancel(&scope,"request-1").await]{assert_eq!(r.err().unwrap().code,"native_adapter_pending");}}
}
