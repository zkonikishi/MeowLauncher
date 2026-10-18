#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use meow_launcher_core::{
    account::{AccountClient, Challenge, CodePurpose},
    client, diagnostic as launcher_log, error,
    launch::LaunchCoordinator,
    settings::{self, Settings},
    Result,
};
use serde::Serialize;
use std::{collections::HashMap, path::PathBuf, sync::Mutex};
use tauri::{Emitter, Manager, State};
use zeroize::Zeroizing;

#[cfg(windows)]
mod webview2;
mod credential;
mod cn115_cli;
mod generic_app;
mod generic_review;
mod client_host_bundle;
mod window_shape;
mod game_window;
mod client_exit;

struct AppState {
    data: PathBuf,
    updater: meow_launcher_core::update::Updater,
    client_updates:meow_launcher_core::distribution::Engine,
    game_operation:tokio::sync::Mutex<()>,
    settings_lock: Mutex<()>,
    account: AccountClient,
    platform: Option<meow_launcher_core::platform_identity::Client>,
    platform_session: tokio::sync::Mutex<Option<meow_launcher_core::platform_identity::Session>>,
    native_target:tokio::sync::Mutex<Option<Vec<u8>>>,
    controlled_running:tokio::sync::Mutex<Option<generic_review::Running>>,
    launcher: LaunchCoordinator,
    /// 邮箱验证码挑战只留在本进程里：challenge_id 与密码同级敏感，不回界面、
    /// 不落盘、不进日志。按用途分开保存，注册与重设不会互相顶掉。
    challenges: Mutex<HashMap<&'static str, (Challenge, u64, String)>>,
    account_operation:tokio::sync::Mutex<()>,
    /// 已投入使用的发行配置（自定义品牌等界面文案要用）。
    brand: String,
    theme_override: Option<String>,
    public_config: Option<meow_launcher_core::public_config::PublicConfig>,
    package_directory: PathBuf,
}
fn resolve_directory(state:&AppState,fallback:String)->Result<String> {
    match &state.public_config {Some(c)=>Ok(c.client_path(&state.package_directory)?.display().to_string()),None=>Ok(fallback)}
}
/// 命令行自检用的账号客户端：优先用启动器同目录的发行配置，其次才是编译期默认值。
/// 通用版没有配置时 `AccountClient::new()` 会直接失败，不会连到我们自己的服务。
fn cli_account()->Result<AccountClient> {
    if cfg!(feature="legacy-online"){return AccountClient::legacy_online();}
    let root=std::env::current_exe().ok().and_then(|path|path.parent().map(|parent|parent.to_path_buf()));
    if let Some(directory)=root {
        if let Some(config)=meow_launcher_core::public_config::PublicConfig::load(&directory)? {
            return AccountClient::configured(&config.account_base_url,Some(config.realm_id.clone()));
        }
    }
    AccountClient::new()
}
#[tauri::command]
fn window_action(window: tauri::WebviewWindow, action: String) -> Result<()> {
    let outcome = match action.as_str() {
        "minimize" => window.minimize(),
        "restore" => window.show().and_then(|_|window.unminimize()).and_then(|_|window.set_focus()),
        "close" => window.close(),
        _ => return Err(error("invalid_action", "不支持的窗口操作。")),
    };
    outcome.map_err(|_| error("window_failed", "窗口操作未完成。"))
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Bootstrap {
    settings: Settings,
    warning: Option<meow_launcher_core::AppError>,
    version: &'static str,
    profile_id: String,
    data_directory: String,
    public_config: Option<meow_launcher_core::public_config::PublicConfig>,
    account_mode: &'static str,
    product: &'static str,
    edition: &'static str,
    release_notes: &'static str,
    product_marker: &'static str,
    brand: String,
    native_target_ready: bool,
}
#[tauri::command]
fn bootstrap(state: State<AppState>) -> Bootstrap {
    let (mut settings, warning) = match settings::load(&state.data.join("settings.json")) {
        Ok(s) => (s, None),
        Err(e) => (Settings::default(), Some(e)),
    };
    if let Some(theme) = &state.theme_override {
        settings.theme = theme.clone();
    }
    if settings.client_directory.is_empty() && state.account.is_legacy() && state.package_directory.join("DNF.exe").is_file() {settings.client_directory=state.package_directory.display().to_string();}
    if let Some(config)=&state.public_config {
        match config.client_path(&state.package_directory) {
            Ok(path)=>settings.client_directory=path.display().to_string(),
            Err(_)=>settings.client_directory=String::new(),
        }
    }
    Bootstrap {
        product: meow_launcher_core::edition::display_name(),
        edition: meow_launcher_core::update::EDITION,
        release_notes: meow_launcher_core::update::NOTES,
        product_marker: meow_launcher_core::update::Program::Launcher.marker(),
        brand: state.brand.clone(),
        account_mode: if state.platform.is_some(){"platform-login-id-v1"}else if state.account.is_legacy(){"legacy-online"}else{"platform-v1"},
        native_target_ready: state.native_target.try_lock().ok().is_some_and(|v|v.is_some()),
        public_config: if state.platform.is_some(){None}else{state.public_config.clone()},
        settings,
        warning,
        version: env!("CARGO_PKG_VERSION"),
        profile_id: if state.platform.is_some(){"115us".into()}else{client::profile().id},
        data_directory: state.data.display().to_string(),
    }
}
#[tauri::command]
fn save_settings(state: State<AppState>, mut settings: Settings) -> Result<Settings> {
    let _lock = state
        .settings_lock
        .lock()
        .map_err(|_| error("internal", "设置正在使用。"))?;
    settings.client_directory=resolve_directory(&state,settings.client_directory)?;
    let result = settings::save(&state.data.join("settings.json"), settings);
    let _ = launcher_log::event(
        &state.data,
        if result.is_ok() {
            "settings_saved"
        } else {
            "settings_write_failed"
        },
    );
    result
}
#[tauri::command]
async fn game_window_preferences(state:State<'_,AppState>)->Result<game_window::Preference>{
    if state.platform.is_none(){return Err(error("protocol_unavailable","此版本暂不提供该设置。"));}
    game_window::load(&state.data)
}
#[tauri::command]
async fn save_game_window_preferences(state:State<'_,AppState>,preference:game_window::Preference)->Result<game_window::Preference>{
    if state.platform.is_none(){return Err(error("protocol_unavailable","此版本暂不提供该设置。"));}
    let _game=state.game_operation.lock().await;
    game_window::save(&state.data,preference)
}
#[tauri::command]
async fn choose_directory() -> Result<Option<String>> {
    let path = rfd::AsyncFileDialog::new()
        .set_title("选择包含 DNF.exe 的客户端目录")
        .pick_folder()
        .await;
    Ok(path.map(|p| p.path().display().to_string()))
}
#[tauri::command]
async fn inspect_client(state: State<'_, AppState>, directory: String) -> Result<client::ClientReport> {
    if state.platform.is_some(){return tauri::async_runtime::spawn_blocking(move||{let root=std::path::Path::new(&directory);let exe=root.join("DFO.exe");let ready=exe.is_file()&&!exe.is_symlink()&&client::sha256_file(&exe).is_ok_and(|sha|sha=="b8235efe5dbf140ea4c535a026e137c4b34cfaecd7c97c78ed5997c377327c95");Ok(client::ClientReport{directory,ready,items:vec![client::CheckItem{name:"115US 客户端".into(),ok:ready,detail:if ready{"主程序版本匹配"}else{"请选择匹配版本的 DFO.exe 所在目录"}.into()}],note:"已检查主程序版本，游戏运行结果以实际启动为准。",warnings:vec![]})}).await.map_err(|_|error("inspection_failed","检查任务失败。"))?;}
    let directory=resolve_directory(&state,directory)?;
    tauri::async_runtime::spawn_blocking(move || client::inspect(&directory))
        .await
        .map_err(|_| error("inspection_failed", "检查任务失败。"))?
}
#[tauri::command]
async fn check_service(
    state: State<'_, AppState>,
) -> Result<meow_launcher_core::account::ServiceStatus> {
    if let Some(platform)=&state.platform{platform.available().await?;return Ok(meow_launcher_core::account::ServiceStatus{available:true,message:"账号服务已连接。"});}
    let result = state.account.status().await;
    let _ = launcher_log::event(
        &state.data,
        match &result {
            Ok(_) => "service_available",
            Err(e) => e.code,
        },
    );
    result
}
#[tauri::command]
async fn launch_game(
    state: State<'_, AppState>,
    window: tauri::WebviewWindow,
    directory: String,
    identifier: String,
    password: String,
) -> Result<meow_launcher_core::launch::LaunchResult> {
    if state.platform.is_some(){return Err(error("game_provisioning","账号准备中，请稍后重试。"));}
    if state.client_updates.busy(){return Err(error("client_update_busy","请等待客户端下载或更新完成。"));}
    let _game=state.game_operation.lock().await;
    let _ = launcher_log::event(&state.data, "launch_requested");
    let directory=resolve_directory(&state,directory)?;
    let result = state
        .launcher
        .launch(
            &state.account,
            directory,
            identifier,
            Zeroizing::new(password),
        )
        .await;
    let _ = launcher_log::event(
        &state.data,
        match &result {
            Ok(_) => "process_created",
            Err(e) => e.code,
        },
    );
    if result.is_ok() { let _ = window.hide(); watch_game(window.app_handle().clone()); }
    result
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SavedCredentialView {
    identifier: String,
    has_password: bool,
}
/// 0.6.0 第 7 项：读取本机加密保存的凭据。密码**不**回传界面，只回账号；
/// 界面用掩码表示“已保存”，点登录时由主程序用密文换票（README §13.1 第 7 项）。
#[tauri::command]
fn load_credential(state: State<AppState>) -> Result<Option<SavedCredentialView>> {
    Ok(credential::load(&state.data).map(|saved| SavedCredentialView {
        identifier: saved.identifier,
        has_password: true,
    }))
}
#[tauri::command]
fn save_credential(state: State<AppState>, identifier: String, password: String) -> Result<()> {
    let identifier=identifier.trim().to_string();
    if identifier.len()!=8||!identifier.bytes().all(|b|b.is_ascii_digit()){return Err(error("invalid_identifier","请使用 8 位 UID 登录；忘记 UID 可先找回。"));}
    let password = Zeroizing::new(password);
    // 细节写诊断日志（只记事件名，diagnostic::event 只接受静态码），界面给稳定文案。
    credential::save(&state.data, &identifier, password.as_str()).map_err(|_detail| {
        let _ = launcher_log::event(&state.data, "credential_save_failed");
        error("credential_save", "密码未能保存到本机；请确认启动器数据目录可写后重试。")
    })
}
#[tauri::command]
fn clear_credential(state: State<AppState>) -> Result<()> {
    credential::clear(&state.data).map_err(|_detail| {
        let _ = launcher_log::event(&state.data, "credential_clear_failed");
        error("credential_clear", "清除已保存密码失败，请手动删除 credential.bin。")
    })
}
#[tauri::command]
async fn platform_login(state:State<'_,AppState>,identifier_type:String,identifier:String,password:String)->Result<meow_launcher_core::controlled_coordinator::AccountView>{
    use meow_launcher_core::platform_identity::IdentifierType;
    let _operation=state.account_operation.lock().await;
    let _game=state.game_operation.lock().await;
    if state.controlled_running.lock().await.as_ref().is_some_and(|r|r.active()||r.quarantined()){return Err(error("game_running","请先退出当前游戏，再切换账号。"));}
    state.platform_session.lock().await.take();
    let client=state.platform.as_ref().ok_or_else(||error("protocol_unavailable","请使用当前版本的 Max 启动器。"))?;
    let kind=match identifier_type.as_str(){"auto"=>IdentifierType::Auto,"id"=>IdentifierType::Id,"uid"=>IdentifierType::Uid,"email"=>IdentifierType::Email,_=>return Err(error("login_method_unavailable","该登录方式暂未开放。"))};
    let session=client.login(kind,&identifier,Zeroizing::new(password)).await?;
    let view=meow_launcher_core::controlled_coordinator::AccountView{uid:session.uid.clone(),nickname:session.nickname.clone(),game_ready:session.game_ready};
    *state.platform_session.lock().await=Some(session);Ok(view)
}
#[tauri::command]
async fn refresh_native_target(state:State<'_,AppState>)->Result<bool>{
    if state.platform.is_none(){return Err(error("protocol_unavailable","此版本不使用该连接方式。"));}
    state.native_target.lock().await.take();let raw=meow_launcher_core::max_production::descriptor().await?;*state.native_target.lock().await=Some(raw);Ok(true)
}
#[tauri::command]
async fn login_account(state: State<'_, AppState>, identifier: String, password: String) -> Result<String> {
    if state.platform.is_some(){return Err(error("protocol_unavailable","请使用 ID、UID 或邮箱登录。"));}
    let identifier=identifier.trim().to_string();
    if identifier.len()!=8||!identifier.bytes().all(|b|b.is_ascii_digit()){return Err(error("invalid_identifier","请使用 8 位 UID 登录；忘记 UID 可先找回。"));}
    // 密码留空且本机存有该账号的密文时用密文换票：密码不经过界面，只在本进程内存里。
    let password = if password.is_empty() {
        match credential::load(&state.data) {
            Some(saved) if saved.identifier == identifier => saved.password,
            Some(_) => return Err(error("credential_mismatch","本机保存的密码属于另一个 UID，请重新输入密码。")),
            None => return Err(error("password_required","请输入密码。")),
        }
    } else {
        Zeroizing::new(password)
    };
    let result = state.launcher.authenticate(&state.account, identifier, password).await;
    if let Err(failure) = &result {
        // 账号或密码已失效时，本机密文一并作废（README §13.1 第 7 项验收 ③）。
        if failure.code == "auth_failed" || failure.code == "invalid_identifier" {
            let _ = credential::clear(&state.data);
        }
    }
    result
}
#[tauri::command]
async fn recover_uid(state:State<'_,AppState>,email:String,password:String)->Result<String>{
    state.account.recover_uid(&email,Zeroizing::new(password)).await
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CodeSent { retry_after: u64 }
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Registered { uid: String, provisioning_pending: bool }
fn code_purpose(value:&str)->Result<CodePurpose> {
    match value {"register"=>Ok(CodePurpose::Register),"reset"=>Ok(CodePurpose::Reset),_=>Err(error("invalid_action","不支持的验证码用途。"))}
}
/// 验证码 15 分钟有效（与账号服务一致）。超时后本机不再保留挑战，只能重新获取。
const CHALLENGE_TTL_SECONDS:u64=900;
fn remember_challenge(state:&AppState,purpose:&'static str,email:String,challenge:Challenge)->Result<()> {
    let expires=state.account.trusted_now()+CHALLENGE_TTL_SECONDS;
    state.challenges.lock().map_err(|_|error("internal","验证码状态不可用。"))?.insert(purpose,(challenge,expires,email));
    Ok(())
}
/// Keep a challenge through validation errors and transport failures; the server
/// owns consumption. Bind it to the normalized email used to request it.
fn take_challenge(state:&AppState,purpose:&'static str,email:&str)->Result<Challenge> {
    let email=meow_launcher_core::account::validate_email(email)?;
    let challenges=state.challenges.lock().map_err(|_|error("internal","验证码状态不可用。"))?;
    let (challenge,expires,target)=challenges.get(purpose).ok_or_else(||error("code_not_requested","请先获取邮箱验证码。"))?;
    if *expires<=state.account.trusted_now(){return Err(error("code_expired","验证码已过期，请重新获取。"));}
    if target!=&email{return Err(error("code_email_mismatch","邮箱已变更，请为当前邮箱重新获取验证码。"));}
    Ok(challenge.clone())
}
/// 第一步：向账号服务索取验证码。challenge_id 留在本进程，不回界面。
#[tauri::command]
async fn request_code(state:State<'_,AppState>,email:String,purpose:String)->Result<CodeSent>{
    let _operation=state.account_operation.lock().await;
    let email=meow_launcher_core::account::validate_email(&email)?;
    let purpose=code_purpose(&purpose)?;
    if purpose==CodePurpose::Register {if let Some(platform)=&state.platform{let sent=platform.registration_code(&email).await?;let retry_after=sent.retry_after;remember_challenge(&state,"register",email,Challenge{id:sent.id,retry_after})?;return Ok(CodeSent{retry_after});}}
    let challenge=state.account.send_code(&email,purpose).await?;
    let retry_after=challenge.retry_after;
    remember_challenge(&state,purpose.as_str(),email,challenge)?;
    let _=launcher_log::event(&state.data,"code_requested");
    Ok(CodeSent{retry_after})
}
/// 第二步：在登陆器内直接注册。成功后只回 UID，不自动登录、不签发任何凭据。
#[tauri::command]
async fn platform_register(state:State<'_,AppState>,login_id:String,nickname:Option<String>,email:String,code:String,password:String)->Result<Registered>{
    let _operation=state.account_operation.lock().await;
    let platform=state.platform.as_ref().ok_or_else(||error("protocol_unavailable","请使用当前版本的 Max 启动器。"))?;
    let email=meow_launcher_core::account::validate_email(&email)?;
    let challenge=take_challenge(&state,"register",&email)?;
    let uid=platform.register(&login_id,nickname.as_deref(),&email,&code,Zeroizing::new(password),&challenge.id).await?;
    state.challenges.lock().map_err(|_|error("internal","验证码状态不可用。"))?.remove("register");
    Ok(Registered{uid,provisioning_pending:true})
}
#[tauri::command]
async fn complete_registration(state:State<'_,AppState>,email:String,code:String,password:String)->Result<Registered>{
    if state.platform.is_some(){return Err(error("invalid_login_id","注册时请填写登录 ID。"));}
    let _operation=state.account_operation.lock().await;
    let password=Zeroizing::new(password);
    let challenge=take_challenge(&state,CodePurpose::Register.as_str(),&email)?;
    let result=state.account.register(&email,&code,password,&challenge).await;
    let _=launcher_log::event(&state.data,match &result{Ok(_)=>"registered",Err(e)=>e.code});
    let registered=result?;
    state.challenges.lock().map_err(|_|error("internal","验证码状态不可用。"))?.remove("register");
    Ok(Registered{uid:registered.uid,provisioning_pending:registered.provisioning_pending})
}
/// 忘记密码：验证码换一次性令牌后立刻改密，令牌不经过界面，也不建立会话。
#[tauri::command]
async fn complete_password_reset(state:State<'_,AppState>,email:String,code:String,new_password:String)->Result<()>{
    let _operation=state.account_operation.lock().await;
    let new_password=Zeroizing::new(new_password);
    meow_launcher_core::account::validate_new_password(&new_password)?;
    let challenge=take_challenge(&state,CodePurpose::Reset.as_str(),&email)?;
    let token=state.account.request_password_reset(&email,&code,&challenge).await?;
    state.account.reset_password(&token.token,new_password).await?;
    // 改密后本机保存的旧密码必然失效，直接作废，避免玩家拿着旧密文反复失败。
    let _=credential::clear(&state.data);
    let _=launcher_log::event(&state.data,"password_reset");
    Ok(())
}
#[tauri::command]
async fn start_authenticated(window: tauri::WebviewWindow, state: State<'_, AppState>, directory: String) -> Result<meow_launcher_core::launch::LaunchResult> {
    if let Some(platform)=&state.platform{
        let _game=state.game_operation.lock().await;
        let mut running=state.controlled_running.lock().await;
        if running.as_ref().is_some_and(|r|r.active()||r.quarantined()){return Err(error("game_running","当前游戏尚未退出。"));}running.take();
        let session=state.platform_session.lock().await.clone().ok_or_else(||error("login_required","请先登录。"))?;
        let uid=session.uid.clone();if !session.game_ready{return Err(error("game_provisioning","账号准备中，请稍后重试。"));}
        let raw=meow_launcher_core::max_production::descriptor().await?;*state.native_target.lock().await=Some(raw.clone());
        let scope=meow_launcher_core::max_production::scope();let exchange=std::sync::Arc::new(meow_launcher_core::controlled_exchange::ExchangeClient::new(meow_launcher_core::max_production::EXCHANGE,scope.clone(),None)?);
        let coordinator=meow_launcher_core::controlled_coordinator::Coordinator::new(platform.clone(),meow_launcher_core::max_production::endpoint(),scope,exchange,raw)?;coordinator.adopt_session(session).await?;
        let client_root=std::path::Path::new(&directory).canonicalize().map_err(|_|error("client_host_integrity","请选择完整的 DFO 客户端目录。"))?;
        let launcher=std::env::current_exe().and_then(|p|p.canonicalize()).map_err(|_|error("client_host_integrity","启动器位置无法确认。"))?;meow_launcher_core::client_host::require_bridge_outside_client_root(&launcher,&client_root)?;
        let host_path=client_host_bundle::materialize(&state.data)?;
        let instance=match generic_review::start_controlled(&host_path,&coordinator,&client_root,&state.data,window.clone()).await{Ok(instance)=>instance,Err(e)=>{if e.code=="login_required"{state.platform_session.lock().await.take();}return Err(e);}};
        let result=meow_launcher_core::launch::LaunchResult{pid:instance.started.process_id,uid,message:instance.started.message};*running=Some(instance);let _=window.hide();return Ok(result);
    }
    let directory=resolve_directory(&state,directory)?;
    if state.client_updates.busy(){return Err(error("client_update_busy","请等待客户端下载或更新完成。"));}
    let _game=state.game_operation.lock().await;
    let result = state.launcher.start_authenticated(&state.account, directory).await?;
    let _ = window.hide();
    watch_game(window.app_handle().clone());
    Ok(result)
}
#[tauri::command]
async fn logout_account(state: State<'_, AppState>) -> Result<()> {
    if let Some(platform)=&state.platform{let _operation=state.account_operation.lock().await;let _game=state.game_operation.lock().await;if state.controlled_running.lock().await.as_ref().is_some_and(|r|r.active()||r.quarantined()){return Err(error("game_running","请先退出当前游戏，再退出账号。"));}let session=state.platform_session.lock().await.take();state.challenges.lock().map_err(|_|error("internal","登录状态不可用。"))?.clear();if let Some(session)=session{platform.logout(&session).await?;}return Ok(());}
    state.launcher.logout(&state.account).await
}
#[tauri::command]
fn open_account_page(state: State<AppState>, page: String) -> Result<()> {
    // 注册与找回密码都已在登陆器内完成；这里只保留“去官网处理”的可选入口，
    // 且只打开配置里声明的 HTTPS 地址，不拼 URL、不接受调用方给的地址。
    let configured=match (state.public_config.as_ref(),page.as_str()) {
        (Some(c),"register") => c.register_url.as_deref(),
        (Some(c),"forgot") => c.forgot_url.as_deref(),
        (None,"register") if meow_launcher_core::edition::is_custom() => Some("https://launcher.invalid/account/register/"),
        (None,"forgot") if meow_launcher_core::edition::is_custom() => Some("https://launcher.invalid/account/forgot/"),
        (_, "register"|"forgot") => None,
        _ => return Err(error("invalid_action", "不支持的页面。")),
    };
    let url=configured.ok_or_else(||error("account_page_unavailable","此发行版未配置网页账号入口，请直接在本启动器内注册或重设密码。"))?;
    open::that(url).map_err(|_| error("browser_open", "无法打开浏览器。"))
}
#[tauri::command]
async fn load_content(state: State<'_, AppState>) -> Result<meow_launcher_core::content::Content> {
    if let Some(c)=&state.public_config {meow_launcher_core::content::fetch_from(&c.content_url,&c.website_url).await}
    else {meow_launcher_core::content::fetch().await}
}
#[tauri::command]
fn open_news(state: State<AppState>, url: String) -> Result<()> {
    let url=meow_launcher_core::content::news_url_for(&url,state.public_config.as_ref().map(|c|c.website_url.as_str()).unwrap_or("https://launcher.invalid"))?;
    open::that(url.as_str()).map_err(|_| error("browser_open","无法打开浏览器。"))
}
#[tauri::command]
fn open_website(state: State<AppState>) -> Result<()> {
    // No caller-provided URLs or shell commands. Registration/reset routes are
    // intentionally unavailable until website delivery is verified.
    open::that(state.public_config.as_ref().map(|c|c.website_url.as_str()).unwrap_or("https://launcher.invalid"))
        .map_err(|_| error("browser_open", "无法打开默认浏览器。"))
}
#[tauri::command]
fn open_diagnostics(state: State<AppState>) -> Result<()> {
    std::fs::create_dir_all(&state.data).map_err(|_| error("log_write", "无法打开诊断目录。"))?;
    open::that(&state.data).map_err(|_| error("folder_open", "无法打开诊断目录。"))
}

#[tauri::command]
async fn check_update(state:State<'_,AppState>) -> Result<meow_launcher_core::update::UpdateStatus> {
    if state.updater.base().is_empty(){return Err(error("update_unavailable","服主尚未配置更新来源，无法在线检查更新。"));}
    meow_launcher_core::update::check(state.updater.base(),env!("CARGO_PKG_VERSION"), if cfg!(feature="legacy-online"){"legacy-online"}else{"platform-v1"},meow_launcher_core::update::Program::Launcher).await
}
#[tauri::command]
async fn download_update(state:State<'_,AppState>)->Result<meow_launcher_core::update::Downloaded> {
    state.updater.download(&state.data.join("updates")).await
}
#[tauri::command]
fn open_update(state:State<AppState>)->Result<()> {
    open::that(state.updater.downloaded_directory()?).map_err(|_|error("folder_open","无法打开更新目录。"))
}
#[tauri::command]
async fn client_distribution_check(state:State<'_,AppState>,directory:String)->Result<meow_launcher_core::distribution::Plan>{
    let directory=resolve_directory(&state,directory)?;
    if directory.trim().is_empty(){return Err(error("missing_directory","请先选择客户端下载或更新目录。"));}
    state.client_updates.check(std::path::Path::new(&directory)).await
}
#[tauri::command]
async fn client_distribution_install(state:State<'_,AppState>,directory:String)->Result<meow_launcher_core::distribution::Plan>{
    let directory=resolve_directory(&state,directory)?;
    if directory.trim().is_empty(){return Err(error("missing_directory","请先选择客户端下载或更新目录。"));}
    let _game=state.game_operation.lock().await;
    state.client_updates.install(std::path::Path::new(&directory),meow_launcher_core::distribution::game_running).await
}
#[tauri::command]
fn client_distribution_progress(state:State<AppState>)->meow_launcher_core::distribution::Progress{state.client_updates.progress()}
#[tauri::command]
fn client_distribution_cancel(state:State<AppState>){state.client_updates.cancel();}
#[tauri::command]
fn open_client_share()->Result<()>{open::that("https://pan.baidu.com/s/1Li5Z7lpiULNaT4lbbY1KQw?pwd=khug").map_err(|_|error("browser_open","无法打开客户端手动下载页面。"))}
fn watch_game(app:tauri::AppHandle){
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            let state=app.state::<AppState>();
            match state.launcher.running(){
                Ok(true)=>continue,
                Ok(false)=>{let _=app.emit("game-exited",());if settings::load(&state.data.join("settings.json")).map(|s|s.restore_after_game).unwrap_or(false){restore(&app);}break;},
                Err(_)=>break,
            }
        }
    });
}
fn restore(app:&tauri::AppHandle) {
    if let Some(w)=app.get_webview_window("main") {let _=w.show();let _=w.unminimize();let _=w.set_focus();}
}
/// 命令行出口：`--output <文件>` 供 Windows GUI 子系统的 release 版本使用
/// （没有控制台可读）。返回值即进程退出码：0 成功、1 业务失败、2 回执写失败。
fn emit_cli(args: &[String], output: &serde_json::Value) -> i32 {
    let text = serde_json::to_string_pretty(output).unwrap_or_else(|_| "{\"Err\":\"serialize\"}".into());
    if let Some(index) = args.iter().position(|value| value == "--output") {
        if let Some(path) = args.get(index + 1) {
            if std::fs::write(path, &text).is_err() {
                return 2;
            }
        }
    }
    println!("{text}");
    if output.get("Err").is_some() { 1 } else { 0 }
}
fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).is_some_and(|a| {
        a == "--test-launch" || a == "--check-public-config" || a == "--inspect" || a == "--probe" || a == "--probe-contract" || a == "--check-settings" || a == "--check-webview2" || a == "--check-credential" || a == "--credential-selftest" || a == "--credential-clear"
    }) {
        let output = match args[1].as_str() {
            "--test-launch" => {
                use std::io::Read;
                #[derive(serde::Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Input {identifier:String,password:String,directory:String}
                impl Drop for Input {fn drop(&mut self){use zeroize::Zeroize;self.password.zeroize();}}
                let result=if !cfg!(feature="legacy-online") {Err(error("protocol_unavailable","本测试入口仅用于显式线上兼容首版。"))} else {
                    let mut raw=Zeroizing::new(Vec::new());
                    let read=std::io::stdin().take(16385).read_to_end(&mut raw);
                    match read {
                        Ok(_) if raw.len()<=16384=>match serde_json::from_slice::<Input>(&raw) {
                            Ok(mut input)=>{let rt=tokio::runtime::Runtime::new().unwrap();rt.block_on(async {
                                let account=AccountClient::legacy_online()?;
                                LaunchCoordinator::default().launch(&account,input.directory.clone(),input.identifier.clone(),Zeroizing::new(std::mem::take(&mut input.password))).await
                            })},
                            _=>Err(error("invalid_request","私密输入格式无效。")),
                        },
                        _=>Err(error("invalid_request","私密输入不可读取或过大。")),
                    }
                };
                serde_json::to_value(result).unwrap()
            }
            "--check-public-config" => {
                let root=std::env::current_exe().unwrap().parent().unwrap().to_path_buf();
                let result=meow_launcher_core::public_config::PublicConfig::load(&root).and_then(|value| {
                    match value {
                        Some(c)=>{let path=c.client_path(&root)?;Ok(serde_json::json!({"config":c,"clientDirectory":path,"source":"executable-directory"}))},
                        None=>Ok(serde_json::json!({"source":"built-in"})),
                    }
                });
                serde_json::to_value(result).unwrap()
            }
            "--inspect" => serde_json::to_value(client::inspect(
                args.get(2).map(String::as_str).unwrap_or(""),
            ))
            .unwrap(),
            "--probe" => {
                let rt = tokio::runtime::Runtime::new().unwrap();
                serde_json::to_value(rt.block_on(async { cli_account()?.status().await }))
                    .unwrap()
            }
            "--probe-contract" => {
                let rt = tokio::runtime::Runtime::new().unwrap();
                serde_json::to_value(
                    rt.block_on(async { cli_account()?.probe_login_route().await }),
                )
                .unwrap()
            }
            "--check-webview2" => {
                #[cfg(windows)]
                { serde_json::to_value(webview2::availability().detail()).unwrap() }
                #[cfg(not(windows))]
                { serde_json::to_value("non-windows").unwrap() }
            }
            "--check-credential" => {
                // 支持用：只报密文文件是否存在、当前账号能否解密，绝不输出密码。
                let directory = std::env::current_exe()
                    .ok()
                    .and_then(|p| p.parent().map(|p| p.join("data")))
                    .expect("executable directory");
                let file = credential::path(&directory);
                let saved = credential::load(&directory);
                serde_json::to_value(serde_json::json!({
                    "file": file.display().to_string(),
                    "exists": credential::exists(&directory),
                    "decryptable": saved.is_some(),
                    "identifier": saved.as_ref().map(|c| c.identifier.clone()),
                }))
                .unwrap()
            }
            "--credential-selftest" => {
                // 跨进程 DPAPI 自检：密码由本进程随机生成（绝不来自命令行），保存后留给
                // `--check-credential` 在另一个进程里验证能否解密；已存在密文时拒绝覆盖。
                let directory = std::env::current_exe().ok().and_then(|p| p.parent().map(|p| p.join("data"))).expect("executable directory");
                if credential::exists(&directory) {
                    serde_json::to_value(serde_json::json!({
                        "saved": false,
                        "reason": "已存在 credential.bin，自检拒绝覆盖；如需清空请跑 --credential-clear",
                        "file": credential::path(&directory).display().to_string(),
                    })).unwrap()
                } else {
                    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.subsec_nanos()).unwrap_or(0);
                    let probe = 0u8;
                    let mut state = (nanos as u64) ^ ((&probe as *const u8 as u64) >> 3) | 1;
                    let mut password = String::with_capacity(24);
                    for _ in 0..24 {
                        state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                        password.push((b'a' + ((state >> 33) % 26) as u8) as char);
                    }
                    let saved = credential::save(&directory, "10000001", &password);
                    let reloaded = credential::load(&directory).map(|c| c.identifier == "10000001" && c.password.as_str() == password).unwrap_or(false);
                    serde_json::to_value(serde_json::json!({
                        "saved": saved.is_ok(),
                        "reloadedInThisProcess": reloaded,
                        "file": credential::path(&directory).display().to_string(),
                        "error": saved.err(),
                    })).unwrap()
                }
            }
            "--credential-clear" => {
                let directory = std::env::current_exe().ok().and_then(|p| p.parent().map(|p| p.join("data"))).expect("executable directory");
                let cleared = credential::clear(&directory);
                serde_json::to_value(serde_json::json!({
                    "cleared": cleared.is_ok(),
                    "exists": credential::exists(&directory),
                    "error": cleared.err(),
                })).unwrap()
            }
            _ => {
                // Isolated config smoke test, writes only into the requested suite folder.
                let p = PathBuf::from(
                    args.get(2)
                        .cloned()
                        .unwrap_or_else(|| "settings-smoke.json".into()),
                );
                let sample = Settings {
                    client_directory: "D:\\客户端 测试".into(),
                    theme: "retro".into(),
                    ..Default::default()
                };
                serde_json::to_value(settings::save(&p, sample).and_then(|_| settings::load(&p)))
                    .unwrap()
            }
        };
        std::process::exit(emit_cli(&args, &output));
    }
    // 115CN 局域网中继：在启动器里内置桥，按该包自己的环回地址表转发到主机。
    if args.get(1).is_some_and(|a| a == "--cn115-bridge" || a == "--cn115-check") {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        std::process::exit(runtime.block_on(cn115_cli::run(&args)));
    }
    let data = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.join("data")))
        .expect("executable directory");
    // WebView2 运行库必须在创建窗口之前检查：配置里声明的窗口由 Tauri 在 setup 之前创建，
    // 缺失时只会弹出 Tauri 自带的英文对话框，Rust 侧拿不到可处理的错误（README §13.2）。
    // 同目录的固定版本运行库（兼容版）要先写进环境变量——Tauri 在创建 WebView2 运行时之前读它，
    // 否则“检测通过”与“实际用了哪份运行时”会不一致（webview2::apply_runtime_override）。
    #[cfg(windows)]
    webview2::apply_runtime_override();
    #[cfg(windows)]
    let webview2_state = webview2::availability();
    #[cfg(windows)]
    if !webview2_state.is_ready() {
        let reason = webview2_state.user_reason();
        // 本地诊断只记受控的固定事件码（diagnostic.rs:8 的约定），版本号等细节留给第 6 条匿名上报。
        let _ = launcher_log::event(
            &data,
            match webview2_state {
                webview2::Availability::TooOld(_) => "webview2_too_old",
                _ => "webview2_missing",
            },
        );
        let install = rfd::MessageDialog::new()
            .set_title(format!("{} · WebView2 运行库需要安装或升级", meow_launcher_core::edition::display_name()))
            .set_description(format!(
                "{reason}\n\n启动器需要 Microsoft Edge WebView2 运行库才能显示界面。\n\n是否现在自动安装/升级？\n（不需要管理员权限，会装到当前 Windows 账号下；安装需联网，约 1–2 分钟。）\n\n若选“取消”，可自行下载安装：\n{}",
                webview2::DOWNLOAD_URL
            ))
            .set_buttons(rfd::MessageButtons::OkCancel)
            .show();
        if !matches!(install, rfd::MessageDialogResult::Ok) {
            rfd::MessageDialog::new()
                .set_title(format!("{} · WebView2 运行库未就绪", meow_launcher_core::edition::display_name()))
                .set_description(format!(
                    "{reason}\n\n请下载安装最新版后重新打开：\n{}\n\n提示：直接双击运行安装程序会装成“当前用户级”，可解决“已装在别的 Windows 账号下”的问题；若提示已安装，请选“修复”或先卸载再装。",
                    webview2::DOWNLOAD_URL
                ))
                .show();
            return;
        }
        if let Err(failure) = webview2::ensure_runtime() {
            rfd::MessageDialog::new()
                .set_title(format!("{} · WebView2 安装未完成", meow_launcher_core::edition::display_name()))
                .set_description(format!(
                    "{failure}\n\n请手动下载安装后重试：\n{}",
                    webview2::DOWNLOAD_URL
                ))
                .show();
            return;
        }
        let _ = launcher_log::event(&data, "webview2_installed");
    }
    if meow_launcher_core::edition::is_sponsor() { generic_app::run(data); return; }
    let theme_override = Some("modern".into()); // Legacy settings remain readable.
    let package_directory=data.parent().expect("package root").to_path_buf();
    let public_config=match meow_launcher_core::public_config::PublicConfig::load(&package_directory) {
        Ok(c)=>c,
        Err(e)=>{rfd::MessageDialog::new().set_title(format!("{} · 发行配置错误", meow_launcher_core::edition::display_name())).set_description(e.message).show();return;}
    };
    if cfg!(feature="legacy-online") && public_config.is_some() {
        rfd::MessageDialog::new().set_title(format!("{} · 配置冲突", meow_launcher_core::edition::display_name())).set_description("线上兼容首版使用固定线上账号协议，请勿混用平台发行配置。请在单独目录使用此首版。").show();return;
    }
    // 通用版没有编译期端点：没有服主配置就没有账号服务、没有官网、也没有更新源。
    // 这里直接停下并说明，而不是退回到我们自己的服务器。
    if meow_launcher_core::edition::is_sponsor() && public_config.is_none() {
        rfd::MessageDialog::new()
            .set_title(format!("{} · 缺少发行配置", meow_launcher_core::edition::display_name()))
            .set_description("此版本需要和启动器放在同一目录的 launcher.public.json（由服主生成），其中包含账号服务、官网与更新来源。\n\n请向服主索取完整客户端包，不要把本程序单独复制到别处运行。")
            .show();
        return;
    }
    let launcher=match &public_config {Some(c)=>LaunchCoordinator::configured(c).expect("validated config"),None=>LaunchCoordinator::default()};
    let account = match if cfg!(feature="legacy-online"){AccountClient::legacy_online()}else{public_config.as_ref().map(|c|AccountClient::configured(&c.account_base_url,Some(c.realm_id.clone()))).unwrap_or_else(AccountClient::new)} {
        Ok(c) => c,
        Err(_) => {
            rfd::MessageDialog::new()
                .set_title(meow_launcher_core::edition::display_name())
                .set_description("安全连接初始化失败，请重新获取启动器。")
                .show();
            return;
        }
    };
    let brand=public_config.as_ref().map(|c|c.brand.clone()).unwrap_or_else(||"Meowopia".into());
    let platform=if !cfg!(feature="legacy-online"){Some(meow_launcher_core::platform_identity::Client::new("https://launcher.invalid").expect("fixed HTTPS platform origin"))}else{None};
    // 更新源：服主显式配置优先；定制版没有配置时用编译期固定的官网地址；
    // 通用版没有配置就留空，检查更新会明确提示未配置。
    let update_base=match public_config.as_ref().and_then(|c|c.update_base()) {
        Some(base)=>base.to_owned(),
        None if meow_launcher_core::edition::is_custom()=>meow_launcher_core::update::BASE_URL.to_owned(),
        None=>String::new(),
    };
    let client_updates=match public_config.as_ref().map(|c|c.client_source()) {
        Some(Ok(Some(source)))=>meow_launcher_core::distribution::Engine::new(source),
        Some(Err(_))=>{rfd::MessageDialog::new().set_title(format!("{} · 发行配置错误", meow_launcher_core::edition::display_name())).set_description("发行配置里的客户端下载源无效，请让服主重新生成。").show();return;}
        _ if meow_launcher_core::edition::is_custom()=>meow_launcher_core::distribution::Engine::new(meow_launcher_core::distribution::Source::meowopia()),
        _=>meow_launcher_core::distribution::Engine::new(meow_launcher_core::distribution::Source::unconfigured()),
    };
    let _ = launcher_log::event(&data, "app_started");
    let mut context = tauri::generate_context!();
    // Preserve the existing main launcher's profile and single-instance identity.
    context.config_mut().identifier = if meow_launcher_core::edition::is_sponsor(){"com.meowopia.meow-launcher.sponsor"}else{"com.meowopia.oo-launcher.modern"}.into();
    if let Ok(arguments)=std::env::var("WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS") {
        if let Some(window)=context.config_mut().app.windows.first_mut(){window.additional_browser_args=Some(arguments);}
    }
    let window_title=format!("{} · {brand}", meow_launcher_core::edition::display_name());
    let result = tauri::Builder::default()
        .setup(move |app| {
            if let Some(w) = app.get_webview_window("main") {
                w.set_icon(tauri::image::Image::new_owned(
                    include_bytes!("../icons/site-favicon.rgba").to_vec(), 400, 400,
                ))?;
                w.set_title(&window_title)?;
                w.center()?;
                window_shape::apply_webview(&w);
            }
            let show=tauri::menu::MenuItem::with_id(app,"show","打开启动器",true,None::<&str>)?;
            let exit=tauri::menu::MenuItem::with_id(app,"exit","退出启动器",true,None::<&str>)?;
            let menu=tauri::menu::Menu::with_items(app,&[&show,&exit])?;
            tauri::tray::TrayIconBuilder::with_id("launcher-tray")
                .icon(tauri::image::Image::new_owned(include_bytes!("../icons/site-favicon.rgba").to_vec(),400,400))
                .tooltip(format!("{window_title} · 点击恢复窗口"))
                .menu(&menu).show_menu_on_left_click(false)
                .on_menu_event(|app,event|match event.id.as_ref(){"show"=>restore(app),"exit"=>app.exit(0),_=>{}})
                .on_tray_icon_event(|tray,event| {
                    if matches!(event,tauri::tray::TrayIconEvent::Click{button:tauri::tray::MouseButton::Left,button_state:tauri::tray::MouseButtonState::Up,..}) {restore(tray.app_handle());}
                }).build(app)?;
            Ok(())
        })
        .on_window_event(|window,event|{
            if matches!(event,tauri::WindowEvent::Resized(_)|tauri::WindowEvent::ScaleFactorChanged{..}){window_shape::apply(window);}
            if let tauri::WindowEvent::CloseRequested{api,..}=event {
                let state=window.app_handle().state::<AppState>();
                let tray=settings::load(&state.data.join("settings.json")).map(|s|s.close_to_tray).unwrap_or(false);
                if tray {api.prevent_close();let _=window.hide();}
            }
        })
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.show();
                let _ = w.unminimize();
                let _ = w.set_focus();
            }
        }))
        .manage(AppState {
            data,
            updater:meow_launcher_core::update::Updater::new(&update_base,meow_launcher_core::update::Program::Launcher,if cfg!(feature="legacy-online"){"legacy-online"}else{"platform-v1"},env!("CARGO_PKG_VERSION")),
            client_updates,
            game_operation:tokio::sync::Mutex::new(()),
            settings_lock: Mutex::new(()),
            account,
            platform,
            platform_session:tokio::sync::Mutex::new(None),
            native_target:tokio::sync::Mutex::new(None),
            controlled_running:tokio::sync::Mutex::new(None),
            launcher,
            challenges:Mutex::new(HashMap::new()),
            account_operation:tokio::sync::Mutex::new(()),
            brand,
            public_config,
            package_directory,
            theme_override,
        })
        .invoke_handler(tauri::generate_handler![
            client_distribution_check,
            client_distribution_install,
            client_distribution_progress,
            client_distribution_cancel,
            open_client_share,
            download_update,
            check_update,
            open_update,
            window_action,
            bootstrap,
            save_settings,
            choose_directory,
            game_window_preferences,
            save_game_window_preferences,
            inspect_client,
            check_service,
            launch_game,
            login_account,
            platform_login,
            refresh_native_target,
            load_credential,
            save_credential,
            clear_credential,
            recover_uid,
            request_code,
            complete_registration,
            platform_register,
            complete_password_reset,
            logout_account,
            start_authenticated,
            open_account_page,
            open_website,
            load_content,
            open_news,
            open_diagnostics
        ])
        .run(context);
    if result.is_err() {
        rfd::MessageDialog::new()
            .set_title(meow_launcher_core::edition::display_name())
            .set_description("窗口初始化失败，请检查 WebView2 Runtime 是否已安装。")
            .show();
    }
}
