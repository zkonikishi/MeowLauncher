//! 115CN 局域网中继的启动器命令行入口（`--cn115-bridge` / `--cn115-check`）。
//!
//! 115CN 自带服务端，但它的客户端把登录与频道地址钉死在环回地址上：服务端监听
//! `127.0.0.1`（登录/HTTP）与 `127.0.0.2`（频道与中转），并向客户端广播
//! `127.0.0.2`。这里在每台玩家机上按同一张环回地址表监听，再转发到真正运行
//! `DNF115CN.Server.exe` 的那台机器；客户端与服务端都不需要任何改动。
//!
//! 只做字节转发：不解析、不修改、不落盘任何游戏数据，不碰客户端文件，不处理账号。
//! 仅限局域网使用；公网需要一个带鉴权的转发层，本模块不承担。
use crate::emit_cli;
use meow_launcher_core::{
    cn115_bridges::{self, Cn115Profile, RunningCn115},
    error, Result,
};
use std::{
    path::PathBuf,
    process::{Child, Command},
    time::Duration,
};

/// 桥在等待退出信号时轮询子进程的间隔。
const POLL: Duration = Duration::from_millis(500);

fn flag<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    args.iter().position(|value| value == name).and_then(|index| args.get(index + 1)).map(String::as_str)
}

fn profile_path(args: &[String]) -> Result<PathBuf> {
    let requested = flag(args, "--profile").or_else(|| args.get(2).map(String::as_str));
    match requested {
        Some(path) if !path.starts_with("--") => Ok(PathBuf::from(path)),
        // 没有显式路径时用启动器同目录的默认档，方便双击/快捷方式直接跑。
        _ => std::env::current_exe()
            .ok()
            .and_then(|path| path.parent().map(|parent| parent.join("cn115-bridge.json")))
            .ok_or_else(|| error("cn115_config", "找不到 115CN 中继配置文件，请用 --profile <文件> 指定。")),
    }
}

fn receipt(mode: &str, args: &[String], profile: &Cn115Profile, listeners: Vec<String>) -> serde_json::Value {
    let plan = profile
        .plan()
        .map(|entries| {
            entries
                .iter()
                .map(|entry| {
                    serde_json::json!({
                        "transport": entry.transport,
                        "listen": entry.listen.to_string(),
                        "upstream": entry.upstream.to_string(),
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    serde_json::json!({
        "mode": mode,
        "profile": profile_path(args).map(|path| path.display().to_string()).unwrap_or_default(),
        "host": profile.host.to_string(),
        "routes": profile.routes.len(),
        "listeners": listeners,
        "plan": plan,
        "clientAliases": [cn115_bridges::LOGIN_ALIAS.to_string(), cn115_bridges::GAME_ALIAS.to_string()],
        "scope": "仅转发 115CN 客户端的环回地址；不解析、不修改游戏数据；仅限局域网。",
    })
}

/// 等待退出：优先跟随 `--launch` 拉起的 115CN 启动器，否则等 Ctrl+C。
async fn wait_for_shutdown(launched: &mut Option<Child>) {
    if let Some(child) = launched.as_mut() {
        loop {
            if !matches!(child.try_wait(), Ok(None)) {
                return;
            }
            tokio::select! {
                _ = tokio::signal::ctrl_c() => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return;
                }
                _ = tokio::time::sleep(POLL) => {}
            }
        }
    }
    if tokio::signal::ctrl_c().await.is_ok() {
        return;
    }
    // 没有控制台（GUI 子系统下由启动器拉起）时收不到 Ctrl+C：一直保持桥在跑，
    // 由父进程结束本进程；这里绝不主动退出，否则桥会在启动后立刻消失。
    loop {
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

async fn run_inner(args: &[String]) -> Result<serde_json::Value> {
    let path = profile_path(args)?;
    let profile = Cn115Profile::load(&path)?;
    let mode = args.get(1).map(String::as_str).unwrap_or("");
    if mode == "--cn115-check" {
        // 校验并试绑全表，然后立刻释放：玩家能在开服前看到端口冲突。
        let listeners = cn115_bridges::check(&profile).await?;
        return Ok(receipt("check", args, &profile, listeners));
    }
    let bridge = RunningCn115::start(&profile).await?;
    let listeners = bridge.bound_addresses().iter().map(|address| address.to_string()).collect::<Vec<_>>();
    let mut launched = match flag(args, "--launch") {
        Some(program) => Some(
            Command::new(program)
                .spawn()
                .map_err(|_| error("cn115_launch", "无法启动 115CN 启动器，请检查 --launch 指向的程序路径。"))?,
        ),
        None => None,
    };
    // 先给出“桥已就绪”的回执，再进入等待；玩家据此确认可以进游戏。
    let _ = emit_cli(args, &receipt("bridge", args, &profile, listeners));
    wait_for_shutdown(&mut launched).await;
    let was_running = bridge.is_running();
    bridge.close().await;
    Ok(serde_json::json!({
        "mode": "bridge",
        "profile": path.display().to_string(),
        "host": profile.host.to_string(),
        "stopped": true,
        "wasRunning": was_running,
    }))
}

/// `--cn115-bridge` / `--cn115-check` 主入口，返回进程退出码。
pub async fn run(args: &[String]) -> i32 {
    match run_inner(args).await {
        Ok(value) => emit_cli(args, &value),
        Err(failure) => {
            let value = serde_json::to_value(failure).unwrap_or_else(|_| serde_json::json!({"code":"cn115","message":"115CN 中继失败。"}));
            emit_cli(args, &value)
        }
    }
}
