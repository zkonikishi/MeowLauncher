//! Fixed known host, embedded in the launcher; never a downloaded executable.
use meow_launcher_core::{client,error,Result};
use std::{io::Write,path::{Path,PathBuf}};
const SHA:&str="dd6ac277206e5f29e24e0d5e1f332842b540d74949b73d1af3a4e7226982b1f8";
pub fn materialize(data:&Path)->Result<PathBuf>{
 let directory=data.join("runtime").join("clienthost-strict-r1");std::fs::create_dir_all(&directory).map_err(|_|error("client_host_integrity","无法准备客户端连接组件。"))?;
 let path=directory.join("MeowClientHost.exe");
 if path.exists(){if path.is_symlink()||client::sha256_file(&path)?!=SHA{return Err(error("client_host_integrity","客户端连接组件不匹配，请重新获取完整启动器。"));}return Ok(path);}
 let mut file=std::fs::OpenOptions::new().write(true).create_new(true).open(&path).map_err(|_|error("client_host_integrity","无法准备客户端连接组件。"))?;
 if file.write_all(include_bytes!("../runtime/clienthost-strict-r1.exe")).and_then(|_|file.sync_all()).is_err(){drop(file);let _=std::fs::remove_file(&path);return Err(error("client_host_integrity","客户端连接组件准备失败。"));}drop(file);
 if client::sha256_file(&path)?!=SHA{return Err(error("client_host_integrity","客户端连接组件校验失败。"));}Ok(path)
}
