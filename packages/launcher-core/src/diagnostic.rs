use crate::{error, Result};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::Path,
};

// Only controlled static event codes are logged, never errors/HTTP bodies/args.
pub fn event(dir: &Path, code: &'static str) -> Result<()> {
    fs::create_dir_all(dir).map_err(|_| error("log_write", "诊断目录不可写。"))?;
    let p = dir.join("launcher.log");
    if p.metadata().is_ok_and(|m| m.len() > 262144) {
        let old = dir.join("launcher.previous.log");
        if old.exists() {
            fs::remove_file(&old).map_err(|_| error("log_write", "无法轮换诊断日志。"))?;
        }
        fs::rename(&p, old).map_err(|_| error("log_write", "无法轮换诊断日志。"))?;
    }
    let mut f = OpenOptions::new()
        .create(true)
        .append(true)
        .open(p)
        .map_err(|_| error("log_write", "无法写入诊断日志。"))?;
    writeln!(f, "{} {}", crate::unix_now(), code)
        .map_err(|_| error("log_write", "无法写入诊断日志。"))
}
