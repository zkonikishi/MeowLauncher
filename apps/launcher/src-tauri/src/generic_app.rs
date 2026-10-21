//! Generic edition entry point: never constructs a platform AccountClient.
use meow_launcher_core::{account_provider::{AccountProvider,SimpleGatewayProvider},error,generic::Target,generic_bridges::LoggedIn,settings::{self,Settings},Result};
use serde::{Deserialize,Serialize};
use meow_launcher_core::{auth_config::{AuthConfig,Connection,Method},cloud_account};
use std::{path::PathBuf,sync::{Mutex,Arc}};
use tauri::{Manager,State};
use zeroize::Zeroizing;

#[derive(Clone,Deserialize,Serialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
struct Profile {schema_version:u32,brand:String,target:Target,#[serde(default)]local_review:Option<super::generic_review::Config>}
impl Profile {
    fn validate(&self)->Result<()> {
        if self.schema_version!=1 || self.brand.trim().is_empty() || self.brand.chars().count()>80 || self.brand.chars().any(char::is_control) {return Err(error("simple_config","服务器名称或配置版本无效。"));}
        if let Some(review)=&self.local_review {review.validate()?;}
        self.target.validate()
    }
}
#[derive(Clone)]
struct CloudSession {client:Arc<cloud_account::Client>,account:Arc<cloud_account::Session>,browser:Option<u64>,request:Option<String>,confirmed:bool}
#[derive(Clone)]
enum AccountSession {Simple(Arc<LoggedIn>),Cloud(CloudSession)}
impl AccountSession {
    fn valid(&self)->bool{match self{Self::Simple(_)=>true,Self::Cloud(s)=>s.account.valid()&&s.confirmed}}
    fn view(&self)->serde_json::Value{match self{Self::Simple(s)=>serde_json::to_value(&s.identity).unwrap_or(serde_json::Value::Null),Self::Cloud(s)=>serde_json::json!({"username":s.account.username,"nativeAccountId":s.account.native_account_id,"providerKind":"cloud-account"})}}
}
struct GenericState {cloud_request:tokio::sync::Mutex<Option<String>>,next_browser:std::sync::atomic::AtomicU64,data:PathBuf,profile:Mutex<Option<Profile>>,auth:Mutex<Option<AuthConfig>>,challenge:tokio::sync::Mutex<Option<cloud_account::Challenge>>,browser:tokio::sync::Mutex<Option<(Arc<cloud_account::Client>,cloud_account::BrowserFlow,u64)>>,session:tokio::sync::Mutex<Option<AccountSession>>,running:tokio::sync::Mutex<Option<super::generic_review::Running>>,operation:tokio::sync::Mutex<()>}
fn cloud(state:&GenericState)->Result<Option<Arc<cloud_account::Client>>>{
    let auth=state.auth.lock().map_err(|_|error("internal","账号配置不可用。"))?;let Some(c)=auth.as_ref()else{return Ok(None)};
    match &c.selected()?.connection{Connection::LocalGateway{..}=>Ok(None),Connection::CloudAccount{base_url,methods,..}=>Ok(Some(Arc::new(cloud_account::Client::new(base_url,c.gateway()?,methods.clone())?)))}
}
fn target(state:&GenericState)->Result<Target> {
    if let Some(config)=state.auth.lock().map_err(|_|error("internal","账号配置不可用。"))?.as_ref(){return config.gateway();}
    state.profile.lock().map_err(|_|error("internal","服务器配置不可用。"))?.as_ref().map(|p|p.target.clone()).ok_or_else(||error("simple_config","请先在设置中配置服主提供的通用服务器。"))
}
#[tauri::command]
fn bootstrap(state:State<GenericState>)->serde_json::Value {
    let mut profile=state.profile.lock().ok().and_then(|p|p.clone());
    let auth=state.auth.lock().ok().and_then(|p|p.clone());
    if let (Some(p),Some(c))=(profile.as_mut(),auth.as_ref()){if let Ok(target)=c.gateway(){p.target=target;}}
    let settings=settings::load(&state.data.join("settings.json")).unwrap_or_default();
    let identity=state.session.try_lock().ok().and_then(|s|s.as_ref().filter(|s|s.valid()).map(|s|s.view()));
    serde_json::json!({"settings":settings,"version":env!("CARGO_PKG_VERSION"),"product":"MeowLauncher","edition":"sponsor","releaseNotes":meow_launcher_core::edition::NOTES,"accountMode":"simple","genericProfile":profile,"authConfig":auth,"effectiveTarget":target(&state).ok(),"publicConfig":null,"warning":null,"authenticated":identity.is_some(),"identity":identity})
}
#[tauri::command]
fn admission_capabilities()->serde_json::Value {
    serde_json::json!({"contract":meow_launcher_core::launch_contract::CONTRACT,"edition":"sponsor","provider":"simple-gateway-v1","nativeAdapterReady":false,"ticketTtlSeconds":meow_launcher_core::launch_contract::TICKET_TTL_SECONDS,"requiresGateway":true,"code":"native_adapter_pending"})
}
async fn ensure_stopped(state:&GenericState)->Result<()>{if state.running.lock().await.as_ref().is_some_and(|r|r.active()||r.quarantined()){return Err(error("game_running","请先退出当前游戏实例，再修改账号或服务器。"));}Ok(())}
#[tauri::command]
async fn save_auth_config(state:State<'_,GenericState>,config:AuthConfig)->Result<()> {
    let _operation=state.operation.lock().await;ensure_stopped(&state).await?;config.save(&state.data)?;
    state.cloud_request.lock().await.take();state.session.lock().await.take();state.challenge.lock().await.take();state.browser.lock().await.take();
    let target=config.gateway()?;if let Some(p)=state.profile.lock().map_err(|_|error("internal","配置不可用。"))?.as_mut(){p.target=target;}
    *state.auth.lock().map_err(|_|error("internal","账号配置不可用。"))?=Some(config);Ok(())
}
#[tauri::command]
async fn request_cloud_code(state:State<'_,GenericState>,method:Method,identifier:String,purpose:String)->Result<cloud_account::ChallengeView>{
    let _operation=state.operation.lock().await;ensure_stopped(&state).await?;
    let client=cloud(&state)?.ok_or(error("auth_method_unavailable","当前账号接入不支持验证码。"))?;
    let (challenge,view)=client.challenge(method,&identifier,&purpose).await?;*state.challenge.lock().await=Some(challenge);Ok(view)
}
#[tauri::command]
async fn login_cloud(state:State<'_,GenericState>,method:Method,identifier:String,password:String,code:String,request_id:String)->Result<serde_json::Value>{
    let password=Zeroizing::new(password);let _operation=state.operation.lock().await;ensure_stopped(&state).await?;state.session.lock().await.take();state.browser.lock().await.take();
    if request_id.is_empty()||request_id.len()>128||!request_id.bytes().all(|b|b.is_ascii_alphanumeric()||b==b'-'){return Err(error("cloud_request","账号请求标识无效。"));}
    *state.cloud_request.lock().await=Some(request_id.clone());
    let client=cloud(&state)?.ok_or(error("auth_method_unavailable","尚未配置云端账号服务。"))?;
    let challenge=state.challenge.lock().await;let result=client.login(method,&identifier,password,&code,challenge.as_ref()).await;drop(challenge);
    let session=Arc::new(result?);
    let current=state.cloud_request.lock().await;
    if current.as_deref()!=Some(request_id.as_str()){return Err(error("cloud_cancelled","此账号请求已取消。"));}
    state.challenge.lock().await.take();let result=AccountSession::Cloud(CloudSession{client,account:session,browser:None,request:Some(request_id.clone()),confirmed:false});let view=result.view();*state.session.lock().await=Some(result);drop(current);Ok(view)
}
#[tauri::command]
async fn cancel_cloud_request(state:State<'_,GenericState>,request_id:String)->Result<()>{
    let _operation=state.operation.lock().await;ensure_stopped(&state).await?;let mut current=state.cloud_request.lock().await;
    if current.as_deref()==Some(request_id.as_str()){current.take();}
    let mut session=state.session.lock().await;if session.as_ref().is_some_and(|s|matches!(s,AccountSession::Cloud(c)if c.request.as_deref()==Some(request_id.as_str()))){session.take();}Ok(())
}
#[tauri::command]
async fn accept_cloud_request(state:State<'_,GenericState>,request_id:String)->Result<()>{
    let _operation=state.operation.lock().await;let mut current=state.cloud_request.lock().await;
    if current.as_deref()!=Some(request_id.as_str()){return Err(error("cloud_cancelled","此账号请求已取消。"));}
    let mut session=state.session.lock().await;match session.as_mut(){Some(AccountSession::Cloud(c))if c.request.as_deref()==Some(request_id.as_str())=>{c.confirmed=true;current.take();Ok(())},_=>Err(error("cloud_cancelled","此账号请求已取消。"))}
}
#[tauri::command]
async fn accept_cloud_browser(state:State<'_,GenericState>,handle:u64)->Result<()>{
    let _operation=state.operation.lock().await;let mut current=state.session.lock().await;
    match current.as_mut(){Some(AccountSession::Cloud(s))if s.browser==Some(handle)=>{s.confirmed=true;Ok(())},_=>Err(error("cloud_cancelled","此浏览器请求已取消。"))}
}
#[tauri::command]
async fn register_cloud(state:State<'_,GenericState>,method:Method,username:String,identifier:String,password:String,code:String)->Result<()> {
    let password=Zeroizing::new(password);let _operation=state.operation.lock().await;ensure_stopped(&state).await?;
    let client=cloud(&state)?.ok_or(error("auth_method_unavailable","尚未配置云端账号服务。"))?;
    let challenge=state.challenge.lock().await;client.register(method,&username,&identifier,password,&code,challenge.as_ref()).await?;drop(challenge);state.challenge.lock().await.take();Ok(())
}
#[tauri::command]
async fn reset_cloud_password(state:State<'_,GenericState>,method:Method,identifier:String,password:String,code:String)->Result<()> {
    let password=Zeroizing::new(password);let _operation=state.operation.lock().await;ensure_stopped(&state).await?;
    let client=cloud(&state)?.ok_or(error("auth_method_unavailable","尚未配置找回密码服务。"))?;
    let challenge=state.challenge.lock().await;client.reset(method,&identifier,password,&code,challenge.as_ref().ok_or(error("cloud_challenge_required","请先获取本次验证码。"))?).await?;drop(challenge);state.challenge.lock().await.take();state.session.lock().await.take();Ok(())
}
#[tauri::command]
async fn begin_cloud_login(state:State<'_,GenericState>)->Result<serde_json::Value>{
    let _operation=state.operation.lock().await;ensure_stopped(&state).await?;state.cloud_request.lock().await.take();state.session.lock().await.take();state.browser.lock().await.take();
    let client=cloud(&state)?.ok_or(error("auth_method_unavailable","尚未配置第三方账号服务。"))?;let (flow,view)=client.begin_browser().await?;
    open::that(&flow.url).map_err(|_|error("browser_open","无法打开系统浏览器。"))?;let handle=state.next_browser.fetch_add(1,std::sync::atomic::Ordering::SeqCst)+1;*state.browser.lock().await=Some((client,flow,handle));let mut result=serde_json::to_value(view).map_err(|_|error("internal","登录状态不可用。"))?;result["handle"]=handle.into();Ok(result)
}
#[tauri::command]
async fn poll_cloud_login(state:State<'_,GenericState>,handle:u64)->Result<Option<serde_json::Value>>{
    let _operation=state.operation.lock().await;ensure_stopped(&state).await?;
    let mut browser=state.browser.lock().await;let (client,flow,current)=browser.as_mut().ok_or(error("cloud_flow_expired","浏览器登录已取消，请重新开始。"))?;
    if *current!=handle{return Err(error("cloud_flow_expired","此浏览器登录已被新的操作替代。"));}
    if let Some(session)=client.poll_browser(flow).await?{let result=AccountSession::Cloud(CloudSession{client:client.clone(),account:Arc::new(session),browser:Some(handle),request:None,confirmed:false});let view=result.view();*browser=None;*state.session.lock().await=Some(result);Ok(Some(view))}else{Ok(None)}
}
#[tauri::command]
async fn cancel_cloud_login(state:State<'_,GenericState>,handle:u64)->Result<()>{let _operation=state.operation.lock().await;ensure_stopped(&state).await?;let mut browser=state.browser.lock().await;if browser.as_ref().is_some_and(|(_,_,id)|*id==handle){browser.take();}let mut session=state.session.lock().await;if session.as_ref().is_some_and(|s|matches!(s,AccountSession::Cloud(c)if c.browser==Some(handle))){session.take();}Ok(())}
#[tauri::command]
async fn save_generic_profile(state:State<'_,GenericState>,profile:Profile)->Result<()> {
    let _operation=state.operation.lock().await;
    if state.running.lock().await.as_ref().is_some_and(|r|r.active()||r.quarantined()){return Err(error("game_running","请先退出当前游戏实例，再切换服务器。"));}
    profile.validate()?;
    if let Some(c)=state.auth.lock().map_err(|_|error("internal","账号配置不可用。"))?.as_ref(){if c.gateway()?!=profile.target{return Err(error("auth_target_conflict","请在账号接入配置中修改目标网关。"));}}
    let bytes=serde_json::to_vec_pretty(&profile).map_err(|_|error("simple_config","服务器配置无效。"))?;
    if bytes.len()>16384{return Err(error("simple_config","服务器配置过大，未保存。"));}
    std::fs::create_dir_all(&state.data).map_err(|_|error("settings_write","无法保存服务器配置。"))?;
    let path=state.data.join("generic-target.json");
    // Atomic replace using the same tested settings writer's staging convention.
    let mut temp=std::fs::OpenOptions::new().write(true).create_new(true).open(state.data.join("generic-target.new")).map_err(|_|error("settings_write","配置暂存文件已存在或不可写，请检查 data。"))?;
    use std::io::Write;
    let staged=state.data.join("generic-target.new");
    let result=temp.write_all(&bytes).and_then(|_|temp.sync_all());drop(temp);
    if result.is_err(){let _=std::fs::remove_file(&staged);return Err(error("settings_write","保存配置失败，原配置未修改。"));}
    // Windows rename cannot replace an existing file. Use the Win32 atomic API.
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        #[link(name="kernel32")] extern "system" {fn MoveFileExW(from:*const u16,to:*const u16,flags:u32)->i32;}
        let from:Vec<u16>=staged.as_os_str().encode_wide().chain(Some(0)).collect();let to:Vec<u16>=path.as_os_str().encode_wide().chain(Some(0)).collect();
        if unsafe {MoveFileExW(from.as_ptr(),to.as_ptr(),1|8)}==0 {let _=std::fs::remove_file(&staged);return Err(error("settings_write","保存配置失败，原配置未修改。"));}
    }
    #[cfg(not(windows))]
    std::fs::rename(&staged,&path).map_err(|_|error("settings_write","无法保存配置。"))?;
    state.session.lock().await.take();
    *state.profile.lock().map_err(|_|error("internal","配置不可用。"))?=Some(profile);
    Ok(())
}
#[tauri::command]
fn save_settings(state:State<GenericState>,settings:Settings)->Result<Settings> {settings::save(&state.data.join("settings.json"),settings)}
#[tauri::command]
async fn login_account(state:State<'_,GenericState>,identifier:String,password:String)->Result<serde_json::Value> {
    let password=Zeroizing::new(password);let _operation=state.operation.lock().await;
    if state.running.lock().await.as_ref().is_some_and(|r|r.active()||r.quarantined()){return Err(error("game_running","请先退出当前游戏实例，再切换账号。"));}
    // A failed new login cannot leave the old account authenticated.
    state.session.lock().await.take();
    state.browser.lock().await.take();
    state.cloud_request.lock().await.take();
    if let Some(client)=cloud(&state)?{
        let session=Arc::new(client.login(Method::UsernamePassword,&identifier,password,"",None).await?);let result=AccountSession::Cloud(CloudSession{client,account:session,browser:None,request:None,confirmed:true});let view=result.view();*state.session.lock().await=Some(result);return Ok(view);
    }
    let provider=SimpleGatewayProvider::new(target(&state)?)?;
    let session=provider.authenticate(identifier,password).await?;
    let result=AccountSession::Simple(session);let view=result.view();*state.session.lock().await=Some(result);Ok(view)
}
#[tauri::command]
async fn register_generic(state:State<'_,GenericState>,username:String,password:String)->Result<()> {
    let password=Zeroizing::new(password);let _operation=state.operation.lock().await;
    if let Some(client)=cloud(&state)?{return client.register(Method::UsernamePassword,&username,&username,password,"",None).await;}
    let provider=SimpleGatewayProvider::new(target(&state)?)?;
    provider.register(username,password).await
}
#[tauri::command]
async fn logout_account(state:State<'_,GenericState>)->Result<serde_json::Value> {
    let _operation=state.operation.lock().await;ensure_stopped(&state).await?;
    state.cloud_request.lock().await.take();let previous=state.session.lock().await.take();state.challenge.lock().await.take();state.browser.lock().await.take();
    let revoked=match previous{Some(AccountSession::Cloud(s))=>s.client.logout(&s.account).await.is_ok(),_=>true};
    Ok(serde_json::json!({"loggedOut":true,"revocationConfirmed":revoked}))
}
#[tauri::command]
async fn start_authenticated(state:State<'_,GenericState>,window:tauri::WebviewWindow,directory:String)->Result<super::generic_review::Started> {
    let _operation=state.operation.lock().await;
    let mut running=state.running.lock().await;
    if running.as_ref().is_some_and(|r|r.quarantined()){return Err(error("client_cleanup_failed","此前实例清理状态不确定，当前候选不复用其资源；请检查实例日志。"));}
    if running.as_ref().is_some_and(|r|r.active()||r.quarantined()){return Err(error("game_running","当前已有游戏实例；多开适配尚未验收，请先退出该实例。"));}
    running.take();
    let account=state.session.lock().await.clone().ok_or_else(||error("login_required","请先登录目标服务器。"))?;
    let trusted=target(&state)?;
    let account=match account{
        AccountSession::Simple(account)=>account,
        AccountSession::Cloud(session)=>{
            if !session.confirmed{return Err(error("cloud_confirmation_pending","账号结果尚未确认，请重新登录。"));}
            LoggedIn::authenticate_cloud(trusted.clone(),session.client,session.account).await?
        }
    };
    let profile=state.profile.lock().map_err(|_|error("internal","服务器配置不可用。"))?.clone().ok_or_else(||error("simple_config","请先配置服务器。"))?;
    let review=profile.local_review.as_ref().ok_or_else(||error("client_host_unavailable","客户端宿主尚未配置；不会自动安装或启动未知组件。"))?;
    let instance=super::generic_review::start(review,&trusted,account,std::path::Path::new(&directory),&state.data,window.clone()).await?;
    let started=instance.started.clone();*running=Some(instance);let _=window.hide();Ok(started)
}
#[tauri::command]
async fn stop_instance(state:State<'_,GenericState>,instance_id:String)->Result<()> {
    let _operation=state.operation.lock().await;
    let mut current=state.running.lock().await;
    if current.as_ref().is_some_and(|r|r.started.instance_id!=instance_id){return Err(error("instance_mismatch","实例已改变，未关闭其他实例。"));}
    let instance=current.take();drop(current);if let Some(mut instance)=instance{if let Err(e)=instance.stop().await{*state.running.lock().await=Some(instance);return Err(e);}}Ok(())
}
#[tauri::command]
async fn current_instance(state:State<'_,GenericState>)->Result<Option<super::generic_review::Started>> {Ok(state.running.lock().await.as_ref().filter(|r|r.active()||r.quarantined()).map(|r|{let mut info=r.started.clone();if r.quarantined(){info.status="cleanup_failed";}info}))}
#[tauri::command]
async fn choose_directory()->Option<String> {rfd::AsyncFileDialog::new().set_title("选择包含 DFO.exe 的客户端目录").pick_folder().await.map(|p|p.path().display().to_string())}
#[tauri::command]
async fn window_action(window:tauri::WebviewWindow,action:String)->Result<()> {
    let result=match action.as_str(){"minimize"=>window.minimize(),"close"=>window.close(),_=>return Err(error("invalid_action","窗口操作无效。"))};result.map_err(|_|error("window_failed","窗口操作未完成。"))
}
pub fn run(data:PathBuf) {
    let auth=match AuthConfig::load(&data){Ok(c)=>c,Err(e)=>{rfd::MessageDialog::new().set_title("MeowLauncher Beta · 账号配置错误").set_description(e.message).show();return;}};

    let profile=match std::fs::read(data.join("generic-target.json")) {
        Ok(bytes) if bytes.len()<=16384=>match serde_json::from_slice::<Profile>(&bytes).ok().filter(|p|p.validate().is_ok()).ok_or(()) {Ok(p)=>Some(p),Err(_)=>{rfd::MessageDialog::new().set_title("MeowLauncher · 配置无效").set_description("通用服务器配置损坏或不适用，原文件已保留。请检查 data/generic-target.json。").show();return;}},
        Ok(_)=>{rfd::MessageDialog::new().set_description("通用服务器配置过大，未加载。").show();return;},
        Err(e) if e.kind()==std::io::ErrorKind::NotFound=>None,
        Err(_)=>{rfd::MessageDialog::new().set_description("无法读取通用服务器配置，未连接服务器。").show();return;},
    };
    let mut context=tauri::generate_context!();context.config_mut().identifier="com.meowopia.meow-launcher.sponsor".into();
    // Same local WebView2 diagnostic hook as the custom entry; no default port.
    if let Ok(arguments)=std::env::var("WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS") {
        if let Some(window)=context.config_mut().app.windows.first_mut(){window.additional_browser_args=Some(arguments);}
    }
    let result=tauri::Builder::default().plugin(tauri_plugin_single_instance::init(|app,_,_|{if let Some(w)=app.get_webview_window("main"){let _=w.show();let _=w.unminimize();let _=w.set_focus();}}))
        .setup(|app|{
            if let Some(w)=app.get_webview_window("main"){w.set_title("MeowLauncher · 通用版")?;super::window_layout::apply(&w)?;super::window_shape::apply_webview(&w);}
            let show=tauri::menu::MenuItem::with_id(app,"show","打开启动器",true,None::<&str>)?;
            let exit=tauri::menu::MenuItem::with_id(app,"exit","退出启动器",true,None::<&str>)?;
            let menu=tauri::menu::Menu::with_items(app,&[&show,&exit])?;
            tauri::tray::TrayIconBuilder::with_id("generic-launcher-tray")
                .icon(tauri::image::Image::new_owned(include_bytes!("../icons/site-favicon.rgba").to_vec(),400,400))
                .tooltip("MeowLauncher · 通用版").menu(&menu).show_menu_on_left_click(false)
                .on_menu_event(|app,event|match event.id.as_ref(){"show"=>{if let Some(w)=app.get_webview_window("main"){let _=w.show();let _=w.unminimize();let _=w.set_focus();}},"exit"=>app.exit(0),_=>{}})
                .on_tray_icon_event(|tray,event|{if matches!(event,tauri::tray::TrayIconEvent::Click{button:tauri::tray::MouseButton::Left,button_state:tauri::tray::MouseButtonState::Up,..}){if let Some(w)=tray.app_handle().get_webview_window("main"){let _=w.show();let _=w.unminimize();let _=w.set_focus();}}}).build(app)?;
            Ok(())
        })
        .on_window_event(|window,event|{if matches!(event,tauri::WindowEvent::ScaleFactorChanged{..}){if let Some(w)=window.app_handle().get_webview_window("main"){let _=super::window_layout::apply(&w);}}if matches!(event,tauri::WindowEvent::Resized(_)|tauri::WindowEvent::ScaleFactorChanged{..}){super::window_shape::apply(window);}if let tauri::WindowEvent::CloseRequested{api,..}=event{let state=window.app_handle().state::<GenericState>();if settings::load(&state.data.join("settings.json")).is_ok_and(|s|s.close_to_tray){api.prevent_close();let _=window.hide();}}})
        .manage(GenericState{cloud_request:tokio::sync::Mutex::new(None),next_browser:std::sync::atomic::AtomicU64::new(0),data,profile:Mutex::new(profile),auth:Mutex::new(auth),challenge:tokio::sync::Mutex::new(None),browser:tokio::sync::Mutex::new(None),session:tokio::sync::Mutex::new(None),running:tokio::sync::Mutex::new(None),operation:tokio::sync::Mutex::new(())})
        .invoke_handler(tauri::generate_handler![bootstrap,admission_capabilities,save_auth_config,request_cloud_code,login_cloud,register_cloud,reset_cloud_password,begin_cloud_login,poll_cloud_login,cancel_cloud_login,cancel_cloud_request,accept_cloud_request,accept_cloud_browser,save_generic_profile,save_settings,login_account,register_generic,logout_account,start_authenticated,stop_instance,current_instance,choose_directory,window_action])
        .run(context);
    if result.is_err(){rfd::MessageDialog::new().set_title("MeowLauncher").set_description("通用版窗口未能启动。").show();}
}
