//! Read-only observation of the already verified game process, never its memory.
use meow_launcher_core::{error,Result};
use std::{path::Path,sync::{Arc,atomic::{AtomicBool,Ordering}}};
fn unavailable()->meow_launcher_core::AppError{error("exit_observer","无法建立游戏退出诊断。")}
#[derive(serde::Deserialize,serde::Serialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
struct Fault{time_utc:String,module:String,exception_code:String,module_offset:String}
#[derive(serde::Deserialize,serde::Serialize)]
#[serde(deny_unknown_fields)]
struct Wer{attempts:u8,faults:Vec<Fault>}
fn parse_wer(raw:&[u8])->Result<Wer>{
    if raw.len()>16384{return Err(unavailable());}
    let value:Wer=serde_json::from_slice(raw).map_err(|_|unavailable())?;
    if !(1..=5).contains(&value.attempts)||value.faults.len()>4{return Err(unavailable());}
    for fault in &value.faults{
        let hex=|s:&str|{let digits=s.strip_prefix("0x").unwrap_or(s);!digits.is_empty()&&digits.len()<=16&&digits.bytes().all(|b|b.is_ascii_hexdigit())};
        if fault.module.is_empty()||fault.module.len()>128||!fault.module.bytes().all(|b|b.is_ascii_alphanumeric()||b"_.-".contains(&b))||!hex(&fault.exception_code)||!hex(&fault.module_offset)||fault.time_utc.len()>40||!fault.time_utc.bytes().all(|b|b.is_ascii_digit()||b"-:T.Z+".contains(&b)){return Err(unavailable());}
    }Ok(value)
}
#[cfg(windows)]
async fn wer(pid:u32,after:u64)->Result<Wer>{
    use std::os::windows::process::CommandExt;
    let system=std::env::var_os("SystemRoot").ok_or_else(unavailable)?;
    let mut command=std::process::Command::new(Path::new(&system).join("System32/WindowsPowerShell/v1.0/powershell.exe"));
    command.args(["-NoProfile","-NonInteractive","-Command",include_str!("client_exit_wer.ps1")]).env("MEOW_OBSERVER_PID",pid.to_string()).env("MEOW_OBSERVER_AFTER",after.to_string()).creation_flags(0x08000000);
    for(key,_)in std::env::vars_os(){if key.to_string_lossy().eq_ignore_ascii_case("PSModulePath"){command.env_remove(key);}}
    let mut command=tokio::process::Command::from(command);command.kill_on_drop(true);
    let output=tokio::time::timeout(std::time::Duration::from_secs(10),command.output()).await.map_err(|_|unavailable())?.map_err(|_|unavailable())?;
    if !output.status.success(){return Err(unavailable());}parse_wer(&output.stdout)
}
#[cfg(windows)]
mod win{
    use super::*;
    use std::ffi::c_void;
    #[repr(C)]#[derive(Default)]struct FileTime{low:u32,high:u32}
    impl FileTime{fn ticks(&self)->u64{u64::from(self.low)|(u64::from(self.high)<<32)}}
    #[link(name="kernel32")]
    unsafe extern "system"{
        fn OpenProcess(access:u32,inherit:i32,pid:u32)->*mut c_void;
        fn CloseHandle(handle:*mut c_void)->i32;
        fn GetProcessId(handle:*mut c_void)->u32;
        fn QueryFullProcessImageNameW(handle:*mut c_void,flags:u32,name:*mut u16,size:*mut u32)->i32;
        fn GetProcessTimes(handle:*mut c_void,created:*mut FileTime,exited:*mut FileTime,kernel:*mut FileTime,user:*mut FileTime)->i32;
        fn WaitForSingleObject(handle:*mut c_void,millis:u32)->u32;
        fn GetExitCodeProcess(handle:*mut c_void,code:*mut u32)->i32;
    }
    pub struct Process{handle:isize,pub pid:u32,pub created_ticks:u64}
    impl Drop for Process{fn drop(&mut self){unsafe{CloseHandle(self.handle as _);}}}
    fn normalize(path:&str)->String{path.trim_start_matches("\\\\?\\").replace('/',"\\").to_lowercase()}
    impl Process{
        pub fn open(pid:u32,image:&Path,before:u64,now:u64)->Result<Self>{
            if pid==0||before>now{return Err(unavailable());}
            // SYNCHRONIZE | QUERY_LIMITED_INFORMATION. No VM_READ/WRITE,
            // debugging, injection or termination access is requested.
            let handle=unsafe{OpenProcess(0x00101000,0,pid)};
            if handle.is_null(){return Err(unavailable());}
            let mut process=Self{handle:handle as isize,pid,created_ticks:0};
            let mut name=vec![0u16;32768];let mut length=name.len() as u32;
            if unsafe{GetProcessId(handle)}!=pid||unsafe{QueryFullProcessImageNameW(handle,0,name.as_mut_ptr(),&mut length)}==0{return Err(unavailable());}
            let actual=String::from_utf16(&name[..length as usize]).map_err(|_|unavailable())?;
            if normalize(&actual)!=normalize(image.to_str().ok_or_else(unavailable)?){return Err(unavailable());}
            let(created,_)=process.times()?;
            if created<before||created>now{return Err(unavailable());}
            process.created_ticks=created;Ok(process)
        }
        fn times(&self)->Result<(u64,u64)>{
            let(mut created,mut exited,mut kernel,mut user)=(FileTime::default(),FileTime::default(),FileTime::default(),FileTime::default());
            if unsafe{GetProcessTimes(self.handle as _,&mut created,&mut exited,&mut kernel,&mut user)}==0{return Err(unavailable());}
            Ok((created.ticks(),exited.ticks()))
        }
        pub fn exit(&self)->Result<Option<(u32,u64)>>{
            match unsafe{WaitForSingleObject(self.handle as _,0)}{
                258=>Ok(None),
                0=>{let mut code=0;if unsafe{GetExitCodeProcess(self.handle as _,&mut code)}==0{return Err(unavailable());}Ok(Some((code,self.times()?.1)))},
                _=>Err(unavailable())
            }
        }
    }
}
#[cfg(windows)]pub use win::Process;

#[cfg(windows)]
pub fn observe(process:Process,logs:&Path,verified_image_sha:&'static str,launcher_cancel:Arc<AtomicBool>)->tokio::task::JoinHandle<()>{
    observe_inner(process,logs,verified_image_sha,launcher_cancel,true)
}
#[cfg(windows)]
fn observe_inner(process:Process,logs:&Path,verified_image_sha:&'static str,launcher_cancel:Arc<AtomicBool>,query_wer:bool)->tokio::task::JoinHandle<()>{
    let output=logs.join("native-exit.json");
    let _=std::fs::write(logs.join("native-observer.json"),serde_json::json!({"schema":"native-observer-v1","armed":true,"pid":process.pid,"readOnlyHandle":true}).to_string());
    tokio::spawn(async move{
        let observed=meow_launcher_core::unix_now();
        let result=loop{match process.exit(){Ok(Some(exit))=>break Ok(exit),Err(e)=>break Err(e),Ok(None)=>tokio::time::sleep(std::time::Duration::from_millis(100)).await}};
        let mut record=match result{
            Ok((code,ended))=>serde_json::json!({"schema":"native-exit-v1","pid":process.pid,"verifiedImageSha256":verified_image_sha,"processCreatedFileTimeUtc":process.created_ticks.to_string(),"processExitedFileTimeUtc":ended.to_string(),"observedAtUnixSeconds":observed,"recordedAtUnixSeconds":meow_launcher_core::unix_now(),"exitCodeUnsigned":code,"exitCodeSigned":code as i32,"exitCodeHex":format!("0x{code:08X}"),"launcherCancelRequested":launcher_cancel.load(Ordering::SeqCst),"memoryOrPayloadRecorded":false}),
            Err(_)=>serde_json::json!({"schema":"native-exit-v1","pid":process.pid,"status":"observation_failed","recordedAtUnixSeconds":meow_launcher_core::unix_now(),"memoryOrPayloadRecorded":false})
        };
        // Persist the actual exit immediately. Delayed WER collection must not
        // lose that code if the player closes the launcher in the meantime.
        use std::io::{Write,Seek,SeekFrom};
        record["werStatus"]=serde_json::json!("pending");
        let mut file=match std::fs::OpenOptions::new().create_new(true).write(true).open(&output){Ok(file)=>file,Err(_)=>return};
        let initial=match serde_json::to_vec_pretty(&record){Ok(bytes)=>bytes,Err(_)=>return};
        if file.write_all(&initial).and_then(|_|file.sync_all()).is_err(){return;}
        if result.is_ok()&&query_wer{
            match wer(process.pid,observed).await{
                Ok(value)=>{record["werQueryAttempts"]=serde_json::json!(value.attempts);record["faultEvents"]=serde_json::json!(value.faults);record["werStatus"]=serde_json::json!(if value.faults.is_empty(){"no_matching_event"}else{"matched"});},
                Err(_)=>{record["werStatus"]=serde_json::json!("unavailable");}
            }
        }
        if !query_wer{record["werStatus"]=serde_json::json!("not_requested");}
        record["collectionEndUnixSeconds"]=serde_json::json!(meow_launcher_core::unix_now());
        record["limitation"]=serde_json::json!("No matching WER event does not prove a normal exit.");
        if let Ok(bytes)=serde_json::to_vec_pretty(&record){let _=file.seek(SeekFrom::Start(0)).and_then(|_|file.write_all(&bytes)).and_then(|_|file.set_len(bytes.len() as u64)).and_then(|_|file.sync_all());}
    })
}

#[cfg(all(test,windows))]mod tests{
    use super::*;
    #[test]fn exit_fixture(){if std::env::var_os("MEOW_EXIT_OBSERVER_FIXTURE").is_some(){std::thread::sleep(std::time::Duration::from_millis(700));std::process::exit(37);}}
    #[test]fn held_handle_observes_actual_code_and_rejects_wrong_image(){
        use std::os::windows::process::CommandExt;
        let image=std::env::current_exe().unwrap();
        let mut child=std::process::Command::new(&image).args(["--exact","client_exit::tests::exit_fixture","--nocapture"]).env("MEOW_EXIT_OBSERVER_FIXTURE","1").creation_flags(0x08000000).spawn().unwrap();
        assert!(Process::open(child.id(),Path::new("D:/not-the-fixture.exe"),0,u64::MAX).is_err());
        let process=Process::open(child.id(),&image,0,u64::MAX).unwrap();assert!(process.exit().unwrap().is_none());
        let status=child.wait().unwrap();assert_eq!(status.code(),Some(37));assert_eq!(process.exit().unwrap().unwrap().0,37);
        assert!(process.created_ticks>0);
    }
    #[test]fn wer_accepts_only_bounded_fault_metadata(){
        assert!(parse_wer(br#"{"attempts":5,"faults":[]}"#).is_ok());
        assert!(parse_wer(br#"{"attempts":1,"faults":[{"timeUtc":"2026-10-16T07:14:19.3270000Z","module":"example.dll","exceptionCode":"c0000005","moduleOffset":"0x123"}]}"#).is_ok());
        assert!(parse_wer(br#"{"attempts":6,"faults":[]}"#).is_err());
        assert!(parse_wer(br#"{"attempts":1,"faults":[],"payload":"not allowed"}"#).is_err());
        assert!(parse_wer(br#"{"attempts":1,"faults":[{"timeUtc":"2026-10-16T07:14:19Z","module":"C:/private/path.dll","exceptionCode":"c0000005","moduleOffset":"123"}]}"#).is_err());
    }
    #[tokio::test]async fn actual_fixture_exit_writes_separate_native_record(){
        use std::os::windows::process::CommandExt;
        let image=std::env::current_exe().unwrap();
        let sha: &'static str=Box::leak(meow_launcher_core::client::sha256_file(&image).unwrap().into_boxed_str());
        let mut child=std::process::Command::new(&image).args(["--exact","client_exit::tests::exit_fixture"]).env("MEOW_EXIT_OBSERVER_FIXTURE","1").stdout(std::process::Stdio::null()).creation_flags(0x08000000).spawn().unwrap();
        let process=Process::open(child.id(),&image,0,u64::MAX).unwrap();
        let root=std::env::temp_dir().join(format!("native-exit-qa-{}-{}",std::process::id(),meow_launcher_core::unix_now()));std::fs::create_dir(&root).unwrap();
        let observer=observe_inner(process,&root,sha,Arc::new(AtomicBool::new(false)),false);
        assert_eq!(child.wait().unwrap().code(),Some(37));observer.await.unwrap();
        let record:serde_json::Value=serde_json::from_slice(&std::fs::read(root.join("native-exit.json")).unwrap()).unwrap();
        assert_eq!(record["exitCodeUnsigned"],37);assert_eq!(record["exitCodeHex"],"0x00000025");assert_eq!(record["launcherCancelRequested"],false);assert_eq!(record["memoryOrPayloadRecorded"],false);assert!(!root.join("host-exit.json").exists());
        std::fs::remove_dir_all(root).unwrap();
    }
}
