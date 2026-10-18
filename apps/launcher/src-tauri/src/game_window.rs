//! Edit only the two verified ASCII dimension values; preserve the cfg's bytes.
use meow_launcher_core::{error,Result};
use serde::{Deserialize,Serialize};
use std::{fs,io::{Read,Seek,SeekFrom,Write},path::Path};

#[derive(Clone,Copy,Deserialize,Serialize,Default)]
#[serde(deny_unknown_fields)]
pub struct Preference { pub size:Option<Size> }
#[derive(Clone,Copy,Deserialize,Serialize)]
#[serde(deny_unknown_fields)]
pub struct Size { pub width:u32,pub height:u32 }
fn failure()->meow_launcher_core::AppError{error("game_resolution","窗口尺寸未应用，请退出游戏并检查游戏配置。")}
fn validate(size:Size)->Result<()>{
    if [(800,600),(1024,768),(1280,720),(1280,960),(1600,900),(1920,1080)].contains(&(size.width,size.height)){Ok(())}
    else{Err(error("game_resolution","请选择列表中的窗口尺寸。"))}
}
pub fn load(data:&Path)->Result<Preference>{
    let path=data.join("game-window.json");
    if !path.exists(){return Ok(Preference::default());}
    let raw=fs::read(path).map_err(|_|failure())?;
    if raw.len()>1024{return Err(failure());}
    let preference:Preference=serde_json::from_slice(&raw).map_err(|_|failure())?;
    if let Some(size)=preference.size{validate(size)?;}Ok(preference)
}
pub fn save(data:&Path,preference:Preference)->Result<Preference>{
    if let Some(size)=preference.size{validate(size)?;}
    fs::create_dir_all(data).map_err(|_|failure())?;
    fs::write(data.join("game-window.json"),serde_json::to_vec(&preference).map_err(|_|failure())?).map_err(|_|failure())?;
    Ok(preference)
}
fn rewrite(raw:&[u8],size:Size)->Result<Vec<u8>>{
    validate(size)?;
    if raw.len()>65536{return Err(failure());}
    let keys:[&[u8];2]=[b"CONFIG_COMMON_RESIZE_WIDTH=",b"CONFIG_COMMON_RESIZE_HEIGHT="];
    let values=[size.width,size.height];let mut counts=[0,0];let mut inside=false;let mut output=Vec::with_capacity(raw.len()+16);
    for line in raw.split_inclusive(|b|*b==b'\n'){
        let end=line.len()-usize::from(line.ends_with(b"\n"));
        let end=end-usize::from(line[..end].ends_with(b"\r"));
        let body=&line[..end];let ending=&line[end..];
        if body.starts_with(b"[")&&body.ends_with(b"]"){inside=body==b"[DNF COMMON]";}
        let mut replaced=false;
        if inside{for i in 0..2{if let Some(value)=body.strip_prefix(keys[i]){
            if value.is_empty()||!value.iter().all(u8::is_ascii_digit){return Err(failure());}
            counts[i]+=1;output.extend_from_slice(keys[i]);output.extend_from_slice(values[i].to_string().as_bytes());output.extend_from_slice(ending);replaced=true;break;
        }}}
        if !replaced{output.extend_from_slice(line);}
    }
    if counts!=[1,1]{return Err(failure());}Ok(output)
}
#[cfg(windows)]
fn game_running()->Result<bool>{
    use std::ffi::c_void;
    #[repr(C)]struct Entry{size:u32,usage:u32,pid:u32,heap:usize,module:u32,threads:u32,parent:u32,priority:i32,flags:u32,name:[u16;260]}
    #[link(name="kernel32")]unsafe extern "system"{
        fn CreateToolhelp32Snapshot(flags:u32,pid:u32)->*mut c_void;
        fn Process32FirstW(snapshot:*mut c_void,entry:*mut Entry)->i32;
        fn Process32NextW(snapshot:*mut c_void,entry:*mut Entry)->i32;
        fn CloseHandle(handle:*mut c_void)->i32;
    }
    unsafe{
        let snapshot=CreateToolhelp32Snapshot(2,0);
        if snapshot as isize == -1{return Err(failure());}
        let mut entry:Entry=std::mem::zeroed();entry.size=std::mem::size_of::<Entry>() as u32;
        if Process32FirstW(snapshot,&mut entry)==0{CloseHandle(snapshot);return Err(failure());}
        loop{
            let n=entry.name.iter().position(|c|*c==0).unwrap_or(260);
            if String::from_utf16_lossy(&entry.name[..n]).eq_ignore_ascii_case("DFO.exe"){CloseHandle(snapshot);return Ok(true);}
            if Process32NextW(snapshot,&mut entry)==0{break;}
        }
        CloseHandle(snapshot);Ok(false)
    }
}
/// Called only immediately before a manually requested game launch.
pub fn apply_saved(data:&Path)->Result<()>{
    let Some(size)=load(data)?.size else{return Ok(());};
    #[cfg(windows)]if game_running()?{return Err(error("game_running","请先退出游戏，再应用窗口尺寸。"));}
    let local=std::env::var_os("LOCALAPPDATA").ok_or_else(failure)?;
    let local=Path::new(&local).parent().ok_or_else(failure)?;
    let path=local.join("LocalLow/DNF/DNF.cfg");
    write_config(&path,data,size)
}
fn write_config(path:&Path,data:&Path,size:Size)->Result<()>{
    if fs::symlink_metadata(path).map_err(|_|failure())?.file_type().is_symlink(){return Err(failure());}
    let mut options=fs::OpenOptions::new();options.read(true).write(true);
    #[cfg(windows)]{use std::os::windows::fs::OpenOptionsExt;options.share_mode(0);}
    let mut file=options.open(path).map_err(|_|failure())?;
    if file.metadata().map_err(|_|failure())?.len()>65536{return Err(failure());}
    let mut original=Vec::new();file.read_to_end(&mut original).map_err(|_|failure())?;
    let replacement=rewrite(&original,size)?;if original==replacement{return Ok(());}
    use sha2::{Digest,Sha256};let hash=format!("{:x}",Sha256::digest(&original));
    let backups=data.join("game-config-backups");fs::create_dir_all(&backups).map_err(|_|failure())?;
    let mut backup=fs::OpenOptions::new().write(true).create_new(true).open(backups.join(format!("DNF-{}-{}.cfg",meow_launcher_core::unix_now(),&hash[..16]))).map_err(|_|failure())?;
    backup.write_all(&original).and_then(|_|backup.sync_all()).map_err(|_|failure())?;
    file.seek(SeekFrom::Start(0)).map_err(|_|failure())?;let mut current=Vec::new();file.read_to_end(&mut current).map_err(|_|failure())?;
    if current!=original{return Err(failure());}
    let written=file.seek(SeekFrom::Start(0)).and_then(|_|file.write_all(&replacement)).and_then(|_|file.set_len(replacement.len() as u64)).and_then(|_|file.sync_all());
    if written.is_err(){let _=file.seek(SeekFrom::Start(0));let _=file.write_all(&original);let _=file.set_len(original.len() as u64);let _=file.sync_all();return Err(failure());}Ok(())
}
#[cfg(test)]mod tests{
    use super::*;
    const RAW:&[u8]=b"[DNF COMMON]\r\nCONFIG_COMMON_SCREEN_MODE=3\r\nCONFIG_COMMON_RESIZE_WIDTH=800\r\nCONFIG_COMMON_RESIZE_HEIGHT=600\r\n[\xff\x81]\r\nOTHER=keep\r\n";
    #[test]fn preserves_mode_unknown_encoding_and_line_endings(){
        let edited=rewrite(RAW,Size{width:1024,height:768}).unwrap();
        assert_eq!(edited,b"[DNF COMMON]\r\nCONFIG_COMMON_SCREEN_MODE=3\r\nCONFIG_COMMON_RESIZE_WIDTH=1024\r\nCONFIG_COMMON_RESIZE_HEIGHT=768\r\n[\xff\x81]\r\nOTHER=keep\r\n");
    }
    #[test]fn rejects_duplicate_missing_or_non_numeric_dimensions(){for bad in [b"[DNF COMMON]\nCONFIG_COMMON_RESIZE_WIDTH=x\nCONFIG_COMMON_RESIZE_HEIGHT=600\n".as_slice(),b"[DNF COMMON]\nCONFIG_COMMON_RESIZE_HEIGHT=600\n",b"[DNF COMMON]\nCONFIG_COMMON_RESIZE_WIDTH=800\nCONFIG_COMMON_RESIZE_WIDTH=800\nCONFIG_COMMON_RESIZE_HEIGHT=600\n"]{assert!(rewrite(bad,Size{width:1024,height:768}).is_err());}}
    #[test]fn fixture_write_keeps_original_backup_and_failure_keeps_cfg(){
        let root=std::env::temp_dir().join(format!("game-size-qa-{}-{}",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        fs::create_dir_all(&root).unwrap();let cfg=root.join("DNF.cfg");fs::write(&cfg,RAW).unwrap();
        let data=root.join("data");write_config(&cfg,&data,Size{width:1024,height:768}).unwrap();
        assert_eq!(fs::read(&cfg).unwrap(),rewrite(RAW,Size{width:1024,height:768}).unwrap());
        let backup=fs::read_dir(data.join("game-config-backups")).unwrap().next().unwrap().unwrap().path();assert_eq!(fs::read(backup).unwrap(),RAW);
        fs::write(&cfg,RAW).unwrap();let invalid=root.join("not-a-directory");fs::write(&invalid,b"fixture").unwrap();
        assert!(write_config(&cfg,&invalid,Size{width:1024,height:768}).is_err());assert_eq!(fs::read(&cfg).unwrap(),RAW);
        fs::remove_dir_all(root).unwrap();
    }

}
