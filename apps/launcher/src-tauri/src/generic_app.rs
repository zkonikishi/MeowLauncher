//! Generic edition entry point: never constructs a platform AccountClient.
use meow_launcher_core::{error,generic::{self,Target},generic_bridges::LoggedIn,settings::{self,Settings},Result};
use serde::{Deserialize,Serialize};
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
struct GenericState {data:PathBuf,profile:Mutex<Option<Profile>>,session:tokio::sync::Mutex<Option<Arc<LoggedIn>>>,running:tokio::sync::Mutex<Option<super::generic_review::Running>>,operation:tokio::sync::Mutex<()>}
fn target(state:&GenericState)->Result<Target> {
    state.profile.lock().map_err(|_|error("internal","服务器配置不可用。"))?.as_ref().map(|p|p.target.clone()).ok_or_else(||error("simple_config","请先在设置中配置服主提供的通用服务器。"))
}
#[tauri::command]
fn bootstrap(state:State<GenericState>)->serde_json::Value {
    let profile=state.profile.lock().ok().and_then(|p|p.clone());
    let settings=settings::load(&state.data.join("settings.json")).unwrap_or_default();
    let identity=state.session.try_lock().ok().and_then(|s|s.as_ref().map(|s|s.identity.clone()));
    serde_json::json!({"settings":settings,"version":env!("CARGO_PKG_VERSION"),"product":"MeowLauncher","edition":"sponsor","releaseNotes":meow_launcher_core::edition::NOTES,"accountMode":"simple","genericProfile":profile,"publicConfig":null,"warning":null,"authenticated":identity.is_some(),"identity":identity})
}
#[tauri::command]
async fn save_generic_profile(state:State<'_,GenericState>,profile:Profile)->Result<()> {
    let _operation=state.operation.lock().await;
    if state.running.lock().await.as_ref().is_some_and(|r|r.active()||r.quarantined()){return Err(error("game_running","请先退出当前游戏实例，再切换服务器。"));}
    profile.validate()?;
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
async fn login_account(state:State<'_,GenericState>,identifier:String,password:String)->Result<generic::Identity> {
    let password=Zeroizing::new(password);let _operation=state.operation.lock().await;
    if state.running.lock().await.as_ref().is_some_and(|r|r.active()||r.quarantined()){return Err(error("game_running","请先退出当前游戏实例，再切换账号。"));}
    // A failed new login cannot leave the old account authenticated.
    state.session.lock().await.take();
    let session=LoggedIn::authenticate(target(&state)?,identifier,password).await?;
    let identity=session.identity.clone();*state.session.lock().await=Some(session);Ok(identity)
}
#[tauri::command]
async fn register_generic(state:State<'_,GenericState>,username:String,password:String)->Result<()> {
    let password=Zeroizing::new(password);let _operation=state.operation.lock().await;
    generic::register(&target(&state)?,&username,password).await
}
#[tauri::command]
async fn logout_account(state:State<'_,GenericState>)->Result<()> {let _operation=state.operation.lock().await;if state.running.lock().await.as_ref().is_some_and(|r|r.active()||r.quarantined()){return Err(error("game_running","请先退出当前游戏实例，再退出账号。"));}state.session.lock().await.take();Ok(())}
#[tauri::command]
async fn start_authenticated(state:State<'_,GenericState>,window:tauri::WebviewWindow,directory:String)->Result<super::generic_review::Started> {
    let _operation=state.operation.lock().await;
    let mut running=state.running.lock().await;
    if running.as_ref().is_some_and(|r|r.quarantined()){return Err(error("client_cleanup_failed","此前实例清理状态不确定，当前候选不复用其资源；请检查实例日志。"));}
    if running.as_ref().is_some_and(|r|r.active()||r.quarantined()){return Err(error("game_running","当前已有游戏实例；多开适配尚未验收，请先退出该实例。"));}
    running.take();
    let account=state.session.lock().await.clone().ok_or_else(||error("login_required","请先登录目标服务器。"))?;
    let profile=state.profile.lock().map_err(|_|error("internal","服务器配置不可用。"))?.clone().ok_or_else(||error("simple_config","请先配置服务器。"))?;
    let review=profile.local_review.as_ref().ok_or_else(||error("client_host_unavailable","客户端宿主尚未配置；不会自动安装或启动未知组件。"))?;
    let instance=super::generic_review::start(review,&profile.target,account,std::path::Path::new(&directory),&state.data,window.clone()).await?;
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
            if let Some(w)=app.get_webview_window("main"){w.set_title("MeowLauncher · 通用版")?;w.center()?;super::window_shape::apply_webview(&w);}
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
        .on_window_event(|window,event|{if matches!(event,tauri::WindowEvent::Resized(_)|tauri::WindowEvent::ScaleFactorChanged{..}){super::window_shape::apply(window);}if let tauri::WindowEvent::CloseRequested{api,..}=event{let state=window.app_handle().state::<GenericState>();if settings::load(&state.data.join("settings.json")).is_ok_and(|s|s.close_to_tray){api.prevent_close();let _=window.hide();}}})
        .manage(GenericState{data,profile:Mutex::new(profile),session:tokio::sync::Mutex::new(None),running:tokio::sync::Mutex::new(None),operation:tokio::sync::Mutex::new(())})
        .invoke_handler(tauri::generate_handler![bootstrap,save_generic_profile,save_settings,login_account,register_generic,logout_account,start_authenticated,stop_instance,current_instance,choose_directory,window_action])
        .run(context);
    if result.is_err(){rfd::MessageDialog::new().set_title("MeowLauncher").set_description("通用版窗口未能启动。").show();}
}
