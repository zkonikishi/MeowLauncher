use crate::{
    account::{AccountClient, Session, Ticket},
    client, error, Result,
};
use serde::Serialize;
use std::{
    path::Path,
    process::{Child, Command},
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
};
use zeroize::Zeroizing;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchResult {
    pub pid: u32,
    pub uid: String,
    pub message: &'static str,
}
/// 兼容协议（legacy）只发一次性票据、没有会话：票据被用掉或过期后想再次启动游戏，
/// 只能用同一份凭据再换一张新票。因此在**进程内存**里记住凭据，避免玩家每启动一次都要重新输密码。
/// 凭据用 `Zeroizing` 包裹，不实现 Debug/Serialize、不写盘、不进日志、不跨 IPC；
/// 退出登录、认证失败（改密或账号停用）或进程结束时即消失。
struct LoginCredential { identifier: String, password: Zeroizing<String> }
#[derive(Default)]
pub struct LaunchCoordinator {
    endpoint: Option<(String,u16)>,
    adapter: Option<client::Profile>,
    busy: AtomicBool,
    child: Mutex<Option<Child>>,
    pending: Mutex<Option<Session>>,
    pending_legacy: Mutex<Option<Ticket>>,
    credentials: Mutex<Option<LoginCredential>>,
}
struct ResetBusy<'a>(&'a AtomicBool);
impl Drop for ResetBusy<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}
fn arguments_for(ticket:&Ticket, now:u64, endpoint:Option<&(String,u16)>, profile:&client::Profile)->Result<Zeroizing<Vec<String>>> {
    profile.validate()?;
    if ticket.expires<=now+5 {return Err(error("token_expired", "登录凭据已过期，请重新登录。"));}
    if ticket.uid.is_empty() || ticket.uid.len()>128 || ticket.token.is_empty() || ticket.token.len()>8192 || [&ticket.uid,ticket.token.as_str()].iter().any(|s| !s.bytes().all(|b|b.is_ascii_alphanumeric() || b==b'-' || b==b'_')) {return Err(error("invalid_token", "登录凭据格式无效。"));}
    let (host,port)=endpoint.ok_or(error("adapter_not_configured", "未配置游戏连接地址。"))?;
    Ok(Zeroizing::new(profile.arguments.iter().map(|a|a.replace("{host}",host).replace("{port}",&port.to_string()).replace("{uid}",&ticket.uid).replace("{token}",ticket.token.as_str())).collect()))
}
impl LaunchCoordinator {
    pub fn running(&self)->Result<bool>{
        let mut slot=self.child.lock().map_err(|_|error("internal","游戏状态不可用。"))?;
        if let Some(child)=slot.as_mut(){if child.try_wait().map_err(|_|error("process_state","无法确认游戏状态。"))?.is_none(){return Ok(true);}}
        *slot=None;Ok(false)
    }

    pub fn configured(config:&crate::public_config::PublicConfig)->Result<Self> {
        config.validate()?;Ok(Self{endpoint:Some((config.game_host.clone(),config.game_port)),adapter:Some(config.client_adapter.clone()),..Self::default()})
    }
    pub async fn authenticate(&self, account: &AccountClient, identifier: String, password: Zeroizing<String>) -> Result<String> {
        crate::account::validate_login_uid(&identifier)?;
        if self.busy.swap(true, Ordering::SeqCst) { return Err(error("busy", "正在处理，请稍候。")); }
        let _reset = ResetBusy(&self.busy);
        *self.pending.lock().map_err(|_| error("internal", "登录状态不可用。"))? = None;
        *self.pending_legacy.lock().map_err(|_| error("internal", "登录状态不可用。"))? = None;
        *self.credentials.lock().map_err(|_| error("internal", "登录状态不可用。"))? = None;
        if account.is_legacy() {
            // 认证成功才记住凭据：兼容协议只发一次性票据，后续启动需要用它静默换新票。
            let remembered=LoginCredential{identifier:identifier.trim().to_owned(),password:password.clone()};
            let ticket=account.legacy_ticket(&identifier,password).await?;
            let uid=ticket.uid.clone();
            *self.credentials.lock().map_err(|_|error("internal","登录状态不可用。"))?=Some(remembered);
            *self.pending_legacy.lock().map_err(|_|error("internal","登录状态不可用。"))?=Some(ticket);
            return Ok(uid);
        }
        let ticket = account.login(&identifier, password).await?;
        let uid = ticket.uid.clone();
        *self.pending.lock().map_err(|_| error("internal", "登录状态不可用。"))? = Some(ticket);
        Ok(uid)
    }
    pub async fn start_authenticated(&self, account: &AccountClient, directory: String) -> Result<LaunchResult> {
        if self.busy.swap(true, Ordering::SeqCst) { return Err(error("busy", "正在启动，请稍候。")); }
        let _reset = ResetBusy(&self.busy);
        // 凭据有效期由账号服务按服务端时钟计算，比较也使用服务端时间，避免本机时钟偏差误判。
        let now = account.trusted_now();
        {
            if account.is_legacy() {
                let mut pending=self.pending_legacy.lock().map_err(|_|error("internal","登录状态不可用。"))?;
                let had=pending.is_some();
                let valid=pending.as_ref().map(|ticket|ticket.expires>now+5).unwrap_or(false);
                if !valid {*pending=None;}
                drop(pending);
                if !valid {
                    // 票据已被用掉或已过期：只要还记着凭据就稍后静默换新票，不再打断玩家。
                    let remembered=self.credentials.lock().map_err(|_|error("internal","登录状态不可用。"))?.is_some();
                    if !remembered {
                        return Err(if had {error("token_expired","登录凭据已过期，请重新登录。")} else {error("login_required","请重新登录。")});
                    }
                }
            } else {
                let mut pending = self.pending.lock().map_err(|_| error("internal", "登录状态不可用。"))?;
                let ticket = pending.as_ref().ok_or(error("login_required", "请重新登录。"))?;
                if ticket.expires <= now + 5 { *pending=None; return Err(error("token_expired", "登录凭据已过期，请重新登录。")); }
            }
        }
        {
            let mut existing=self.child.lock().map_err(|_| error("internal", "游戏状态不可用。"))?;
            if let Some(child)=existing.as_mut() {
                if child.try_wait().map_err(|_| error("process_state", "游戏状态不可用。"))?.is_none() { return Err(error("game_running", "游戏仍在运行。")); }
            }
        }
        let adapter=self.adapter.clone().ok_or(error("adapter_not_configured", "未配置客户端适配器。"))?;
        let dir=directory.clone();
        let report=tokio::task::spawn_blocking(move || client::inspect_with_profile(&dir, &adapter)).await.map_err(|_| error("inspection_failed", "检查失败。"))??;
        if !report.ready { return Err(error("client_not_ready", "客户端检查未通过，请在设置中查看。")); }
        if account.is_legacy() {
            self.ensure_legacy_ticket(account).await?;
            let now=account.trusted_now();
            let ticket=self.take_legacy_ticket(now)?;
            return self.spawn_ticket(&directory,ticket,now);
        }
        let session=self.pending.lock().map_err(|_| error("internal", "登录状态不可用。"))?.as_ref().cloned().ok_or(error("login_required", "请重新登录。"))?;
        // Issue a fresh one-use game ticket only after the client is ready.
        let ticket=match account.ticket(&session).await {
            Ok(ticket)=>ticket,
            Err(e)=>{if matches!(e.code,"login_required"|"auth_failed") {*self.pending.lock().map_err(|_| error("internal","登录状态不可用。"))?=None;}return Err(e);}
        };
        let now=account.trusted_now();
        self.spawn_ticket(&directory,ticket,now)
    }
    pub async fn launch(
        &self,
        account: &AccountClient,
        directory: String,
        identifier: String,
        password: Zeroizing<String>,
    ) -> Result<LaunchResult> {
        if self.busy.swap(true, Ordering::SeqCst) {
            return Err(error("busy", "正在启动，请勿重复操作。"));
        }
        let _reset = ResetBusy(&self.busy);
        {
            let mut existing = self
                .child
                .lock()
                .map_err(|_| error("internal", "无法读取游戏进程状态。"))?;
            if let Some(child) = existing.as_mut() {
                if child
                    .try_wait()
                    .map_err(|_| error("process_state", "无法确认游戏是否已退出，请稍后重试。"))?
                    .is_none()
                {
                    return Err(error("game_running", "本启动器启动的游戏仍在运行。"));
                }
            }
            *existing = None;
        }
        crate::account::validate_input(&identifier, &password)?;
        crate::account::validate_login_uid(&identifier)?;
        let adapter=self.adapter.clone().ok_or(error("adapter_not_configured", "未配置客户端适配器。"))?;
        let dir = directory.clone();
        let report = tokio::task::spawn_blocking(move || client::inspect_with_profile(&dir, &adapter))
            .await
            .map_err(|_| error("inspection_failed", "客户端检查失败。"))??;
        if !report.ready {
            return Err(error(
                "client_not_ready",
                "客户端检查未通过，请在设置中查看缺失或不匹配的文件。",
            ));
        }
        // Obtain one fresh ticket per explicit launch attempt. Never cache/replay it.
        let ticket = if account.is_legacy() {account.legacy_ticket(&identifier,password).await?} else {
            let session = account.login(&identifier, password).await?;
            account.ticket(&session).await?
        };
        let result = self.spawn_ticket(&directory, ticket, account.trusted_now());
        result
    }
    pub async fn logout(&self, account: &AccountClient) -> Result<()> {
        if self.busy.swap(true, Ordering::SeqCst) { return Err(error("busy","正在处理，请稍候。")); }
        let _reset=ResetBusy(&self.busy);
        *self.pending_legacy.lock().map_err(|_|error("internal","登录状态不可用。"))?=None;
        *self.credentials.lock().map_err(|_|error("internal","登录状态不可用。"))?=None;
        let session=self.pending.lock().map_err(|_| error("internal","登录状态不可用。"))?.take();
        if let Some(session)=session {account.logout(&session).await?;}
        Ok(())
    }
    /// 兼容协议（legacy）只有一次性票据：票据被用掉或过期后，用内存里记住的凭据静默再登录一次，
    /// 免得玩家每启动一次游戏都要重新输密码。凭据被服务端拒绝（改密或账号停用）时清空并如实报错。
    async fn ensure_legacy_ticket(&self,account:&AccountClient)->Result<()> {
        {
            let pending=self.pending_legacy.lock().map_err(|_|error("internal","登录状态不可用。"))?;
            if pending.as_ref().map(|ticket|ticket.expires>account.trusted_now()+5).unwrap_or(false) {return Ok(());}
        }
        let credential={
            let guard=self.credentials.lock().map_err(|_|error("internal","登录状态不可用。"))?;
            guard.as_ref().map(|c|(c.identifier.clone(),c.password.clone())).ok_or(error("login_required","请重新登录。"))?
        };
        match account.legacy_ticket(&credential.0,credential.1).await {
            Ok(ticket)=>{
                *self.pending_legacy.lock().map_err(|_|error("internal","登录状态不可用。"))?=Some(ticket);
                Ok(())
            }
            Err(e)=>{
                if matches!(e.code,"auth_failed"|"invalid_identifier") {
                    *self.credentials.lock().map_err(|_|error("internal","登录状态不可用。"))?=None;
                }
                Err(e)
            }
        }
    }
    fn take_legacy_ticket(&self,now:u64)->Result<Ticket> {
        let ticket=self.pending_legacy.lock().map_err(|_|error("internal","登录状态不可用。"))?.take().ok_or(error("login_required","请重新登录。"))?;
        if ticket.expires<=now+5 {return Err(error("token_expired","登录凭据已过期，请重新登录。"));}
        Ok(ticket)
    }
    fn spawn_ticket(&self, directory: &str, ticket: Ticket, now:u64) -> Result<LaunchResult> {
        let adapter=self.adapter.as_ref().ok_or(error("adapter_not_configured", "未配置客户端适配器。"))?;
        let exe = client::executable_with_profile(directory,adapter)?;
        let args = arguments_for(&ticket, now, self.endpoint.as_ref(),adapter)?;
        let child = spawn(&exe, &args)?;
        let pid = child.id();
        *self
            .child
            .lock()
            .map_err(|_| error("internal", "无法记录游戏进程状态。"))? = Some(child);
        Ok(LaunchResult {
            pid,
            uid: ticket.uid.clone(),
            message: "已创建游戏进程；连接成功与进入选角请在游戏内确认。",
        })
    }
}
fn spawn(exe: &Path, args: &[String]) -> Result<Child> {
    // Direct process invocation, one argument, no cmd.exe and no batch execution.
    Command::new(exe)
        .args(args)
        .current_dir(
            exe.parent()
                .ok_or(error("invalid_directory", "客户端目录无效。"))?,
        )
        .spawn()
        .map_err(|_| {
            error(
                "launch_failed",
                "游戏进程启动失败。请检查文件权限后重新登录；本次凭据不会复用。",
            )
        })
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::unix_now;
    #[tokio::test] async fn legacy_lobby_ticket_is_one_use_and_logout_clears_it() {
        let c=LaunchCoordinator::default();let account=AccountClient::legacy_for_test("http://127.0.0.1:1");
        assert_eq!(c.start_authenticated(&account,String::new()).await.err().unwrap().code,"login_required");
        *c.pending_legacy.lock().unwrap()=Some(Ticket{uid:"10000001".into(),token:Zeroizing::new("fixture_only".into()),expires:unix_now()+60});
        assert_eq!(c.take_legacy_ticket(unix_now()).unwrap().uid,"10000001");
        assert_eq!(c.take_legacy_ticket(unix_now()).err().unwrap().code,"login_required");
        *c.pending_legacy.lock().unwrap()=Some(Ticket{uid:"10000001".into(),token:Zeroizing::new("fixture_only".into()),expires:0});
        assert_eq!(c.start_authenticated(&account,String::new()).await.err().unwrap().code,"token_expired");
        assert!(c.pending_legacy.lock().unwrap().is_none());
        *c.pending_legacy.lock().unwrap()=Some(Ticket{uid:"10000001".into(),token:Zeroizing::new("fixture_only".into()),expires:unix_now()+60});
        c.logout(&account).await.unwrap();assert!(c.pending_legacy.lock().unwrap().is_none());assert!(c.child.lock().unwrap().is_none());
    }
    #[test] fn configured_adapter_renders_separate_arguments() {
        let c=crate::public_config::PublicConfig::parse(include_bytes!("../fixtures/public-config.json")).unwrap();
        let coordinator=LaunchCoordinator::configured(&c).unwrap();
        let ticket=Ticket{uid:"account_42".into(),token:Zeroizing::new("fixture_token".into()),expires:2000};
        let args=arguments_for(&ticket,1000,coordinator.endpoint.as_ref(),coordinator.adapter.as_ref().unwrap()).unwrap();
        assert_eq!(*args,vec!["--server=game.example.com:17001","--uid=account_42","--ticket=fixture_token"]);
        assert!(arguments_for(&ticket,2000,coordinator.endpoint.as_ref(),coordinator.adapter.as_ref().unwrap()).is_err());
    }
    #[tokio::test]
    async fn missing_client_fails_before_authentication_and_releases_busy_guard() {
        let d = tempfile::tempdir().unwrap();
        let account = AccountClient::configured("https://fixture.example.invalid",None).unwrap();
        let coordinator = LaunchCoordinator::default();
        for _ in 0..2 {
            let result = coordinator
                .launch(
                    &account,
                    d.path().display().to_string(),
                    "12345678".into(),
                    Zeroizing::new("fixture-only".into()),
                )
                .await;
            assert_eq!(result.err().unwrap().code, "adapter_not_configured");
        }
        coordinator.busy.store(true, Ordering::SeqCst);
        let result = coordinator
            .launch(
                &account,
                String::new(),
                "12345678".into(),
                Zeroizing::new("fixture-only".into()),
            )
            .await;
        assert_eq!(result.err().unwrap().code, "busy");
    }
    #[tokio::test]
    async fn lobby_start_requires_unexpired_native_ticket() {
        let c=LaunchCoordinator::default();
        let account=AccountClient::configured("https://fixture.example.invalid",None).unwrap();
        assert_eq!(c.start_authenticated(&account,String::new()).await.err().unwrap().code,"login_required");
        *c.pending.lock().unwrap()=Some(Session {uid:"12345678".into(),token:Zeroizing::new("YWJj".into()),expires:0});
        assert_eq!(c.start_authenticated(&account,String::new()).await.err().unwrap().code,"token_expired");
        assert!(c.pending.lock().unwrap().is_none());
    }
    #[tokio::test]
    async fn second_launch_reuses_the_remembered_credential_instead_of_asking_again() {
        use std::io::{Read,Write};
        use base64::Engine;
        use std::time::Duration;
        use std::sync::atomic::{AtomicUsize,Ordering as AtomicOrdering};
        let listener=std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let base=format!("http://{}",listener.local_addr().unwrap());
        let seen=std::sync::Arc::new(AtomicUsize::new(0));
        let counter=seen.clone();
        let server=std::thread::spawn(move||{
            for attempt in 0..3 {
                let (mut stream,_)=listener.accept().unwrap();stream.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
                let mut bytes=Vec::new();let mut buffer=[0u8;2048];
                let (boundary,length)=loop{let n=stream.read(&mut buffer).unwrap();assert!(n>0);bytes.extend_from_slice(&buffer[..n]);if let Some(end)=bytes.windows(4).position(|w|w==b"\r\n\r\n"){let headers=String::from_utf8_lossy(&bytes[..end]);let length=headers.lines().find_map(|line|line.to_lowercase().strip_prefix("content-length:").map(|v|v.trim().parse::<usize>().unwrap())).unwrap();break(end+4,length);}};
                while bytes.len()<boundary+length {let n=stream.read(&mut buffer).unwrap();assert!(n>0);bytes.extend_from_slice(&buffer[..n]);}
                counter.fetch_add(1,AtomicOrdering::SeqCst);
                let request:serde_json::Value=serde_json::from_slice(&bytes[boundary..boundary+length]).unwrap();
                assert_eq!(request["username"],"10000001");assert_eq!(request["password"],"isolated-password");
                let accepted=attempt<2;
                let body=if accepted {let token=base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(format!("2.{}.{}.{}",unix_now()+300,"A".repeat(32),"B".repeat(64)));serde_json::json!({"ok":true,"uid":"10000001","account_id":2,"token":token})}else{serde_json::json!({"ok":false,"error":"auth_failed"})};
                let body=serde_json::to_vec(&body).unwrap();
                write!(stream,"HTTP/1.1 {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",if accepted{"200 OK"}else{"401 Unauthorized"},body.len()).unwrap();stream.write_all(&body).unwrap();
            }
        });
        let account=AccountClient::legacy_for_test(&base);
        let coordinator=LaunchCoordinator::default();
        assert_eq!(coordinator.authenticate(&account,"10000001".into(),Zeroizing::new("isolated-password".into())).await.unwrap(),"10000001");
        assert_eq!(coordinator.take_legacy_ticket(account.trusted_now()).unwrap().uid,"10000001");
        // 第二次启动：一次性票据已经用掉，靠记住的凭据静默换票，不再要求重新登录。
        coordinator.ensure_legacy_ticket(&account).await.unwrap();
        assert_eq!(coordinator.take_legacy_ticket(account.trusted_now()).unwrap().uid,"10000001");
        assert_eq!(seen.load(AtomicOrdering::SeqCst),2);
        // 服务端拒绝（改密或账号停用）：清空凭据，下一次如实要求重新登录。
        assert_eq!(coordinator.ensure_legacy_ticket(&account).await.err().unwrap().code,"auth_failed");
        assert!(coordinator.credentials.lock().unwrap().is_none());
        assert_eq!(coordinator.ensure_legacy_ticket(&account).await.err().unwrap().code,"login_required");
        assert_eq!(seen.load(AtomicOrdering::SeqCst),3);
        server.join().unwrap();
    }
    #[test]
    fn spawn_failure_is_generic_and_has_no_credential() {
        let e = spawn(Path::new("Z:/__oo_missing__/DNF.exe"), &["secret-token".into()]).unwrap_err();
        assert_eq!(e.code, "launch_failed");
        assert!(!e.message.contains("secret-token"));
    }
    #[tokio::test]
    async fn logout_clears_the_remembered_credential() {
        let coordinator=LaunchCoordinator::default();
        let account=AccountClient::legacy_for_test("http://127.0.0.1:1");
        *coordinator.credentials.lock().unwrap()=Some(LoginCredential{identifier:"10000001".into(),password:Zeroizing::new("isolated-password".into())});
        coordinator.logout(&account).await.unwrap();
        assert!(coordinator.credentials.lock().unwrap().is_none());
    }
}
