//! Offline v2 admission boundary. This is not a new production wire protocol.
//! A timeout means unknown, never cancelled or consumed. No secret is serializable.
use crate::{error, Result};
use serde::Serialize;
use zeroize::Zeroizing;

pub const CONTRACT: &str = "launcher-admission-v2";
pub const HOST_PROTOCOL: &str = "meow-admission-v2";
pub const AUTHORITY_CALL_SECONDS:u64=15;
pub const WHOLE_OPERATION_SECONDS:u64=45;
pub const TICKET_TTL_SECONDS: u64 = 300;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase { Idle, Authenticating, Preparing, Connecting, Binding, Ready, Cancelling, Cancelled, Failed }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edition { Beta, Max }

/// Constructed by native provider code after authentication, never from UI UID.
#[derive(Clone, PartialEq, Eq)]
pub struct Scope { pub edition: Edition, pub authority: String, pub account: String, pub target: String }
impl Scope {
    pub fn validate(&self) -> Result<()> {
        if [&self.authority, &self.account, &self.target].iter().any(|s| s.is_empty() || s.len()>256 || s.chars().any(char::is_control)) {
            return Err(error("admission_scope", "游戏准入身份或目标无效。"));
        }
        Ok(())
    }
}

/// Received from the server; random generation and TTL enforcement belong there.
/// No Debug/Serialize/Clone: do not put this in events, URLs, argv or files.
pub struct AdmissionTicket { value: Zeroizing<String>, scope: Scope, request_id: String, expires: u64 }
impl AdmissionTicket {
    pub fn received(value: Zeroizing<String>, scope: Scope, request_id: String, issued: u64, expires: u64, now: u64) -> Result<Self> {
        scope.validate()?; valid_request(&request_id)?;
        if value.len()!=32 || !value.bytes().all(|b| b.is_ascii_alphanumeric()) || expires.checked_sub(issued)!=Some(TICKET_TTL_SECONDS) || now<issued || now>=expires {
            return Err(error("admission_ticket", "游戏准入票据格式或有效期无效。"));
        }
        Ok(Self { value, scope, request_id, expires })
    }
    /// Only native adapters borrow the ticket. UI-facing types cannot contain it.
    pub fn borrow_for_binding(&self, scope: &Scope, request_id: &str, now: u64) -> Result<&str> {
        if scope!=&self.scope || request_id!=self.request_id || now>=self.expires {
            return Err(error("admission_scope", "票据不属于本次准入或已过期。"));
        }
        Ok(self.value.as_str())
    }
}
fn valid_request(id: &str) -> Result<()> {
    if id.is_empty() || id.len()>128 || !id.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b,b'-'|b'_')) {
        return Err(error("admission_request", "准入请求标识无效。"));
    }
    Ok(())
}

#[derive(Clone, Serialize)]
#[serde(rename_all="camelCase")]
pub struct Snapshot { pub contract: &'static str, pub request_id: String, pub phase: Phase, pub revision: u64, pub pending_confirmation: bool, pub code: Option<&'static str> }

/// Only an authenticated native adapter may submit these observations. The UI
/// cannot send serverBound/hostProof flags to manufacture admission success.
/// Cancelled means verified authority abort or exact committed-generation retirement.
/// A cancel request/HTTP timeout must never be normalized as Cancelled.
pub enum Observation { Pending, Bound { host_proof_verified: bool, session_active: bool, generation_verified: bool }, Cancelled, Rejected }
pub struct Attempt { scope: Scope, snapshot: Snapshot, server_bound: bool, activation_verified: bool }
impl Attempt {
    /// request_id must be issuer-assigned and authenticated by the native provider.
    /// A locally generated UI/fixture ID is not authority authorization.
    pub fn new(request_id: String, scope: Scope) -> Result<Self> {
        scope.validate()?;valid_request(&request_id)?;
        Ok(Self { scope, snapshot: Snapshot { contract: CONTRACT, request_id, phase: Phase::Preparing, revision:0, pending_confirmation:false, code:None }, server_bound:false, activation_verified:false })
    }
    pub fn snapshot(&self) -> Snapshot { self.snapshot.clone() }
    pub fn advance(&mut self, phase: Phase) -> Result<()> {
        if !matches!((self.snapshot.phase,phase),(Phase::Preparing,Phase::Connecting)|(Phase::Connecting,Phase::Binding)) {
            return Err(error("admission_transition", "准入阶段转换无效。"));
        }
        self.snapshot.phase=phase;Ok(())
    }
    pub fn timeout(&mut self) {
        if !self.terminal() { self.snapshot.pending_confirmation=true; self.snapshot.code=Some("admission_result_pending"); }
    }
    pub fn cancel(&mut self) {
        if !self.terminal() {self.snapshot.phase=Phase::Cancelling;self.snapshot.pending_confirmation=true;}
    }
    fn terminal(&self)->bool {matches!(self.snapshot.phase,Phase::Ready|Phase::Cancelled|Phase::Failed)}
    /// Local verified host observation can arrive after a server response. It
    /// does not require the server to invent a new revision for the same binding.
    pub fn confirm_host(&mut self,scope:&Scope,request_id:&str)->Result<()> {
        if scope!=&self.scope || request_id!=self.snapshot.request_id || !self.server_bound || !self.activation_verified || self.terminal() || self.snapshot.phase==Phase::Cancelling {
            return Err(error("host_proof_pending", "未取得本次请求的绑定结果与宿主证明。"));
        }
        self.snapshot.phase=Phase::Ready;self.snapshot.pending_confirmation=false;self.snapshot.code=None;Ok(())
    }
    pub fn observe(&mut self, scope:&Scope, request_id:&str, revision:u64, result:Observation)->Result<()> {
        if scope!=&self.scope || request_id!=self.snapshot.request_id {return Err(error("admission_scope", "准入结果与本次请求不匹配。"));}
        if revision==0 {return Err(error("admission_revision", "准入结果缺少有效序号。"));}
        if revision<=self.snapshot.revision {return Ok(());}
        if self.terminal() {return Err(error("admission_terminal", "准入终态已确认，拒绝冲突结果。"));}
        if !matches!(self.snapshot.phase,Phase::Binding|Phase::Cancelling) {return Err(error("admission_transition", "尚未进入绑定阶段，不能接收绑定结果。"));}
        match result {
            Observation::Pending=>{self.snapshot.pending_confirmation=true;}
            Observation::Bound{host_proof_verified,session_active,generation_verified}=>{
                self.server_bound=true;self.activation_verified=session_active&&generation_verified;
                if self.snapshot.phase==Phase::Cancelling {self.snapshot.pending_confirmation=true;self.snapshot.code=Some("admission_retirement_pending");}
                else if host_proof_verified&&self.activation_verified {self.snapshot.phase=Phase::Ready;self.snapshot.pending_confirmation=false;self.snapshot.code=None;}
                else {self.snapshot.pending_confirmation=true;self.snapshot.code=Some("host_activation_pending");}
            }
            Observation::Cancelled=>{self.snapshot.phase=Phase::Cancelled;self.snapshot.pending_confirmation=false;self.snapshot.code=None;}
            Observation::Rejected=>{self.snapshot.phase=Phase::Failed;self.snapshot.pending_confirmation=false;self.snapshot.code=Some("admission_rejected");}
        }
        self.snapshot.revision=revision;Ok(())
    }
}

#[cfg(test)] mod tests {
    use super::*;
    fn scope()->Scope {Scope{edition:Edition::Beta,authority:"owner".into(),account:"native-account".into(),target:"realm-115".into()}}
    fn binding()->Attempt {let mut a=Attempt::new("request-1".into(),scope()).unwrap();a.advance(Phase::Connecting).unwrap();a.advance(Phase::Binding).unwrap();a}
    #[test] fn timeout_and_cancel_do_not_guess_consumption_or_replace_request(){let mut a=binding();a.timeout();assert_eq!(a.snapshot().phase,Phase::Binding);a.cancel();assert_eq!(a.snapshot().request_id,"request-1");assert_eq!(a.snapshot().phase,Phase::Cancelling);a.observe(&scope(),"request-1",1,Observation::Bound{host_proof_verified:true,session_active:true,generation_verified:true}).unwrap();assert_eq!(a.snapshot().phase,Phase::Cancelling);assert_eq!(a.snapshot().code,Some("admission_retirement_pending"));assert!(a.confirm_host(&scope(),"request-1").is_err());a.observe(&scope(),"request-1",2,Observation::Cancelled).unwrap();assert_eq!(a.snapshot().phase,Phase::Cancelled);}
    #[test] fn cancellation_winner_and_stale_results_are_stable(){let mut a=binding();a.cancel();a.observe(&scope(),"request-1",2,Observation::Cancelled).unwrap();a.observe(&scope(),"request-1",1,Observation::Bound{host_proof_verified:true,session_active:true,generation_verified:true}).unwrap();assert_eq!(a.snapshot().phase,Phase::Cancelled);assert!(a.observe(&scope(),"request-1",3,Observation::Bound{host_proof_verified:true,session_active:true,generation_verified:true}).is_err());}
    #[test] fn host_proof_and_scope_are_required(){let mut a=binding();a.observe(&scope(),"request-1",1,Observation::Bound{host_proof_verified:false,session_active:true,generation_verified:true}).unwrap();assert_eq!(a.snapshot().phase,Phase::Binding);let mut foreign=scope();foreign.edition=Edition::Max;assert!(a.observe(&foreign,"request-1",2,Observation::Bound{host_proof_verified:true,session_active:true,generation_verified:true}).is_err());assert!(a.observe(&scope(),"another-request",2,Observation::Rejected).is_err());}
    #[test] fn ticket_is_bound_and_never_in_ui_snapshot(){let token="Ab3".repeat(10)+"Z9";let ticket=AdmissionTicket::received(Zeroizing::new(token.clone()),scope(),"request-1".into(),100,400,101).unwrap();assert_eq!(ticket.borrow_for_binding(&scope(),"request-1",399).unwrap(),token);assert!(ticket.borrow_for_binding(&scope(),"request-1",400).is_err());let mut foreign=scope();foreign.target="other".into();assert!(ticket.borrow_for_binding(&foreign,"request-1",102).is_err());assert!(!serde_json::to_string(&binding().snapshot()).unwrap().contains(&token));}
    #[test] fn malformed_ttl_and_tokens_are_rejected(){for (v,e) in [("a".repeat(31),400),("a".repeat(32),399),("!".repeat(32),400)]{assert!(AdmissionTicket::received(Zeroizing::new(v),scope(),"request-1".into(),100,e,101).is_err());}}
    #[test] fn cancellation_blocks_late_local_proof_even_after_commit(){let mut a=binding();a.observe(&scope(),"request-1",1,Observation::Bound{host_proof_verified:false,session_active:true,generation_verified:true}).unwrap();a.cancel();assert!(a.confirm_host(&scope(),"request-1").is_err());a.timeout();assert_eq!(a.snapshot().phase,Phase::Cancelling);assert!(a.snapshot().pending_confirmation);}
    #[test] fn authority_commit_without_active_fenced_host_is_not_ready(){for (active,generation) in [(false,true),(true,false),(false,false)]{let mut a=binding();a.observe(&scope(),"request-1",1,Observation::Bound{host_proof_verified:true,session_active:active,generation_verified:generation}).unwrap();assert_eq!(a.snapshot().phase,Phase::Binding);assert!(a.confirm_host(&scope(),"request-1").is_err());}}
    #[test] fn cannot_claim_ready_by_local_transition(){let mut a=binding();assert!(a.advance(Phase::Ready).is_err());a.cancel();a.cancel();assert_eq!(a.snapshot().phase,Phase::Cancelling);}
    #[test] fn host_proof_can_arrive_later_without_new_server_revision(){let mut a=binding();assert!(a.confirm_host(&scope(),"request-1").is_err());a.observe(&scope(),"request-1",1,Observation::Bound{host_proof_verified:false,session_active:true,generation_verified:true}).unwrap();a.confirm_host(&scope(),"request-1").unwrap();assert_eq!(a.snapshot().phase,Phase::Ready);assert_eq!(a.snapshot().revision,1);}
}
