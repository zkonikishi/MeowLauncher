//! Plain LAN bridge for the packaged DNF115CN local server (1.0.5).
//!
//! 115CN ships a real server, but its client is pinned to loopback: the server
//! binds `127.0.0.1`/`127.0.0.2` and advertises `127.0.0.2`, and the client
//! kernel carries an outbound Winsock guard. The bridge therefore listens on
//! the SAME loopback endpoints on each player machine and forwards them to the
//! machine that actually runs the server. The client is never told anything
//! new, and the host keeps advertising `127.0.0.2` unchanged.
//!
//! Scope: byte-level TCP/UDP forwarding only. No TLS, no authentication, no
//! packet inspection, no account handling, no game files touched. LAN only.
use crate::{error, Result};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    io::Read,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    path::Path,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio::{
    io::copy_bidirectional,
    net::{TcpListener, TcpStream, UdpSocket},
    sync::watch,
    task::JoinSet,
};

/// Loopback alias the 115CN client uses for login/HTTP.
pub const LOGIN_ALIAS: Ipv4Addr = Ipv4Addr::new(127, 0, 0, 1);
/// Loopback alias the 115CN server advertises for channels/relay.
pub const GAME_ALIAS: Ipv4Addr = Ipv4Addr::new(127, 0, 0, 2);
/// Channels the packaged server opens (server.json `gamePort`).
pub const CHANNEL_PORTS: std::ops::RangeInclusive<u16> = 10011..=10022;
const IDLE: Duration = Duration::from_secs(120);
const MAX_TCP_CLIENTS: usize = 128;
const MAX_UDP_SESSIONS: usize = 256;
const PROFILE_LIMIT: u64 = 64 * 1024;

fn invalid() -> crate::AppError {
    error("cn115_config", "115CN 中继配置无效，未启动任何转发。")
}
fn busy() -> crate::AppError {
    error("cn115_bind", "115CN 中继端口被占用，未启动任何转发；请先关闭本机 115CN 服务端。")
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Cn115Transport {
    Tcp,
    Udp,
}
impl Cn115Transport {
    fn label(self) -> &'static str {
        match self {
            Self::Tcp => "tcp",
            Self::Udp => "udp",
        }
    }
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Cn115Route {
    pub transport: Cn115Transport,
    /// Loopback address the client connects to, e.g. `127.0.0.2:10011`.
    pub listen: String,
    /// Port on the host machine; the host address comes from the profile.
    pub upstream_port: u16,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Cn115Profile {
    /// LAN address of the machine that runs `DNF115CN.Server.exe`.
    pub host: IpAddr,
    /// Empty means the packaged 115CN matrix.
    #[serde(default)]
    pub routes: Vec<Cn115Route>,
}

impl Cn115Profile {
    /// Read a bridge profile from disk. Bounded, no includes, no network.
    pub fn load(path: &Path) -> Result<Self> {
        let file = std::fs::File::open(path).map_err(|_| invalid())?;
        let mut bytes = Vec::new();
        std::io::Read::take(file, PROFILE_LIMIT + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| invalid())?;
        if bytes.is_empty() || bytes.len() as u64 > PROFILE_LIMIT {
            return Err(invalid());
        }
        let mut profile: Self = serde_json::from_slice(&bytes).map_err(|_| invalid())?;
        if profile.routes.is_empty() {
            profile.routes = Self::default_routes();
        }
        profile.validate()?;
        Ok(profile)
    }

    /// The endpoints the packaged 115CN client/server pair uses by default.
    pub fn default_routes() -> Vec<Cn115Route> {
        let mut routes = vec![
            route(Cn115Transport::Tcp, LOGIN_ALIAS, 7000, 7000),
            route(Cn115Transport::Tcp, LOGIN_ALIAS, 7001, 7001),
            route(Cn115Transport::Tcp, GAME_ALIAS, 7278, 7278),
        ];
        for port in CHANNEL_PORTS {
            routes.push(route(Cn115Transport::Tcp, GAME_ALIAS, port, port));
        }
        for alias in [LOGIN_ALIAS, GAME_ALIAS] {
            for port in [2311u16, 2312] {
                routes.push(route(Cn115Transport::Udp, alias, port, port));
            }
        }
        routes
    }

    pub fn template(host: IpAddr) -> Self {
        Self { host, routes: Self::default_routes() }
    }

    pub fn validate(&self) -> Result<()> {
        if self.host.is_multicast() || self.host.is_unspecified() {
            return Err(invalid());
        }
        if self.routes.is_empty() || self.routes.len() > 64 {
            return Err(invalid());
        }
        let mut seen = std::collections::HashSet::new();
        for entry in &self.routes {
            let listen: SocketAddr = entry.listen.parse().map_err(|_| invalid())?;
            // A bridge may only shadow loopback: anything else would hijack a
            // real interface on the player's machine.
            if !listen.ip().is_loopback() || listen.port() == 0 || entry.upstream_port == 0 {
                return Err(invalid());
            }
            if !seen.insert((entry.transport.label(), listen)) {
                return Err(invalid());
            }
        }
        Ok(())
    }

    /// Validated forwarding table. Nothing is bound here.
    pub fn plan(&self) -> Result<Vec<Planned>> {
        self.validate()?;
        Ok(self
            .routes
            .iter()
            .map(|entry| {
                let listen: SocketAddr = entry.listen.parse().expect("validated above");
                Planned {
                    transport: entry.transport,
                    listen,
                    upstream: SocketAddr::new(self.host, entry.upstream_port),
                }
            })
            .collect())
    }
}

fn route(transport: Cn115Transport, alias: Ipv4Addr, port: u16, upstream_port: u16) -> Cn115Route {
    Cn115Route { transport, listen: SocketAddr::new(IpAddr::V4(alias), port).to_string(), upstream_port }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Planned {
    pub transport: Cn115Transport,
    pub listen: SocketAddr,
    pub upstream: SocketAddr,
}
impl Planned {
    pub fn describe(self) -> String {
        format!("{} {} -> {}", self.transport.label(), self.listen, self.upstream)
    }
}

enum Bound {
    Tcp(TcpListener, SocketAddr),
    Udp(Arc<UdpSocket>, SocketAddr),
}

/// Bind every endpoint of the plan. All or nothing: a single conflict drops
/// the whole set, so a half-open routing table can never look healthy.
async fn bind_all(plan: &[Planned]) -> Result<Vec<Bound>> {
    let mut bound = Vec::new();
    for entry in plan {
        match entry.transport {
            Cn115Transport::Tcp => {
                let listener = TcpListener::bind(entry.listen).await.map_err(|_| busy())?;
                bound.push(Bound::Tcp(listener, entry.listen));
            }
            Cn115Transport::Udp => {
                let socket = UdpSocket::bind(entry.listen).await.map_err(|_| busy())?;
                bound.push(Bound::Udp(Arc::new(socket), entry.listen));
            }
        }
    }
    Ok(bound)
}

/// Validate and bind the complete plan, then release it. Used by the
/// `--cn115-check` entry point so a player can see conflicts before playing.
pub async fn check(profile: &Cn115Profile) -> Result<Vec<String>> {
    let plan = profile.plan()?;
    let bound = bind_all(&plan).await?;
    let addresses = bound
        .iter()
        .map(|entry| match entry {
            Bound::Tcp(_, address) | Bound::Udp(_, address) => address.to_string(),
        })
        .collect();
    drop(bound);
    Ok(addresses)
}

pub struct RunningCn115 {
    cancel: watch::Sender<bool>,
    task: Option<tokio::task::JoinHandle<()>>,
    bound: Vec<SocketAddr>,
}

impl RunningCn115 {
    pub async fn start(profile: &Cn115Profile) -> Result<Self> {
        let plan = profile.plan()?;
        let bound = bind_all(&plan).await?;
        let addresses = bound
            .iter()
            .map(|entry| match entry {
                Bound::Tcp(_, address) | Bound::Udp(_, address) => *address,
            })
            .collect::<Vec<_>>();
        let targets = plan
            .iter()
            .map(|entry| entry.upstream)
            .collect::<Vec<_>>();
        let (cancel, receiver) = watch::channel(false);
        let task = tokio::spawn(async move {
            let mut workers = JoinSet::new();
            let mut pending = targets.into_iter();
            for entry in bound {
                let upstream = pending.next().expect("plan and bound stay aligned");
                let mut receiver = receiver.clone();
                match entry {
                    Bound::Tcp(listener, _) => {
                        workers.spawn(async move { run_tcp(listener, upstream, &mut receiver).await });
                    }
                    Bound::Udp(socket, _) => {
                        workers.spawn(async move { run_udp(socket, upstream, &mut receiver).await });
                    }
                }
            }
            // Losing one listener invalidates the complete routing set.
            let _ = workers.join_next().await;
            workers.abort_all();
            while workers.join_next().await.is_some() {}
        });
        Ok(Self { cancel, task: Some(task), bound: addresses })
    }

    pub fn bound_addresses(&self) -> &[SocketAddr] {
        &self.bound
    }
    pub fn is_running(&self) -> bool {
        !*self.cancel.borrow() && self.task.as_ref().is_some_and(|task| !task.is_finished())
    }
    pub async fn close(mut self) {
        let _ = self.cancel.send(true);
        if let Some(mut task) = self.task.take() {
            if tokio::time::timeout(Duration::from_secs(5), &mut task).await.is_err() {
                task.abort();
                let _ = task.await;
            }
        }
    }
}
impl Drop for RunningCn115 {
    fn drop(&mut self) {
        let _ = self.cancel.send(true);
        if let Some(task) = self.task.take() {
            task.abort();
        }
    }
}

async fn run_tcp(listener: TcpListener, upstream: SocketAddr, receiver: &mut watch::Receiver<bool>) {
    let mut clients = JoinSet::new();
    loop {
        tokio::select! {
            biased;
            _ = receiver.changed() => break,
            _ = clients.join_next(), if !clients.is_empty() => {},
            accepted = listener.accept() => {
                let Ok((local, _peer)) = accepted else { break };
                if clients.len() >= MAX_TCP_CLIENTS { drop(local); continue; }
                let mut receiver = receiver.clone();
                clients.spawn(async move {
                    if *receiver.borrow() { return; }
                    let Ok(mut remote) = TcpStream::connect(upstream).await else { return };
                    let mut local = local;
                    tokio::select! {
                        biased;
                        _ = receiver.changed() => {},
                        _ = copy_bidirectional(&mut local, &mut remote) => {},
                    }
                });
            }
        }
    }
    clients.abort_all();
    while clients.join_next().await.is_some() {}
}

struct UdpSession {
    upstream: Arc<UdpSocket>,
    cancel: watch::Sender<bool>,
    last: Mutex<Instant>,
}

async fn run_udp(socket: Arc<UdpSocket>, upstream: SocketAddr, receiver: &mut watch::Receiver<bool>) {
    // One upstream socket per client keeps distinct source ports on the host.
    let bind_any = match socket.local_addr().map(|address| address.ip()) {
        Ok(IpAddr::V6(_)) => SocketAddr::new(IpAddr::V6(std::net::Ipv6Addr::UNSPECIFIED), 0),
        _ => SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0),
    };
    let mut sessions: HashMap<SocketAddr, UdpSession> = HashMap::new();
    let mut clients = JoinSet::new();
    let mut buffer = vec![0u8; 65536];
    let mut reaper = tokio::time::interval(Duration::from_secs(15));
    loop {
        tokio::select! {
            biased;
            _ = receiver.changed() => break,
            _ = clients.join_next(), if !clients.is_empty() => {},
            _ = reaper.tick() => {
                let expired: Vec<SocketAddr> = sessions
                    .iter()
                    .filter(|(_, session)| session.last.lock().map(|last| last.elapsed() > IDLE).unwrap_or(true))
                    .map(|(peer, _)| *peer)
                    .collect();
                for peer in expired {
                    if let Some(session) = sessions.remove(&peer) {
                        let _ = session.cancel.send(true);
                    }
                }
            }
            received = socket.recv_from(&mut buffer) => {
                let Ok((length, peer)) = received else { break };
                if !sessions.contains_key(&peer) {
                    if sessions.len() >= MAX_UDP_SESSIONS { continue; }
                    let Ok(created) = UdpSocket::bind(bind_any).await else { continue };
                    let upstream_socket = Arc::new(created);
                    let (cancel, mut cancelled) = watch::channel(false);
                    let reply = socket.clone();
                    let reader = upstream_socket.clone();
                    let mut scratch = vec![0u8; 65536];
                    clients.spawn(async move {
                        loop {
                            tokio::select! {
                                biased;
                                _ = cancelled.changed() => break,
                                received = reader.recv_from(&mut scratch) => {
                                    let Ok((length, _from)) = received else { break };
                                    if reply.send_to(&scratch[..length], peer).await.is_err() { break; }
                                }
                            }
                        }
                    });
                    sessions.insert(peer, UdpSession { upstream: upstream_socket, cancel, last: Mutex::new(Instant::now()) });
                }
                let Some(session) = sessions.get(&peer) else { continue };
                if let Ok(mut last) = session.last.lock() { *last = Instant::now(); }
                let _ = session.upstream.send_to(&buffer[..length], upstream).await;
            }
        }
    }
    for (_, session) in sessions.drain() {
        let _ = session.cancel.send(true);
    }
    clients.abort_all();
    while clients.join_next().await.is_some() {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    async fn free(alias: Ipv4Addr) -> u16 {
        let listener = TcpListener::bind(SocketAddr::new(IpAddr::V4(alias), 0)).await.unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        port
    }

    fn loopback_profile(routes: Vec<Cn115Route>) -> Cn115Profile {
        Cn115Profile { host: IpAddr::V4(Ipv4Addr::LOCALHOST), routes }
    }

    #[test]
    fn default_profile_covers_the_packaged_matrix() {
        let profile = Cn115Profile::template(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 6)));
        assert_eq!(profile.routes.len(), 19);
        let plan = profile.plan().unwrap();
        assert_eq!(plan.iter().filter(|entry| entry.transport == Cn115Transport::Tcp).count(), 15);
        assert_eq!(plan.iter().filter(|entry| entry.transport == Cn115Transport::Udp).count(), 4);
        assert!(plan.iter().all(|entry| entry.upstream.ip() == IpAddr::V4(Ipv4Addr::new(192, 168, 1, 6))));
    }

    #[test]
    fn rejects_non_loopback_duplicate_and_empty_plans() {
        let hijack = loopback_profile(vec![Cn115Route { transport: Cn115Transport::Tcp, listen: "0.0.0.0:7000".into(), upstream_port: 7000 }]);
        assert!(hijack.validate().is_err());
        let lan = loopback_profile(vec![Cn115Route { transport: Cn115Transport::Tcp, listen: "192.168.1.6:7000".into(), upstream_port: 7000 }]);
        assert!(lan.validate().is_err());
        let zero = loopback_profile(vec![Cn115Route { transport: Cn115Transport::Tcp, listen: "127.0.0.1:0".into(), upstream_port: 7000 }]);
        assert!(zero.validate().is_err());
        let duplicate = loopback_profile(vec![
            Cn115Route { transport: Cn115Transport::Tcp, listen: "127.0.0.1:7000".into(), upstream_port: 7000 },
            Cn115Route { transport: Cn115Transport::Tcp, listen: "127.0.0.1:7000".into(), upstream_port: 7001 },
        ]);
        assert!(duplicate.validate().is_err());
        // Same port on different aliases or transports is legitimate.
        let distinct = loopback_profile(vec![
            Cn115Route { transport: Cn115Transport::Tcp, listen: "127.0.0.1:2311".into(), upstream_port: 2311 },
            Cn115Route { transport: Cn115Transport::Udp, listen: "127.0.0.1:2311".into(), upstream_port: 2311 },
            Cn115Route { transport: Cn115Transport::Tcp, listen: "127.0.0.2:2311".into(), upstream_port: 2311 },
        ]);
        assert!(distinct.validate().is_ok());
        let mut unspecified = Cn115Profile::template(IpAddr::V4(Ipv4Addr::UNSPECIFIED));
        unspecified.routes = Cn115Profile::default_routes();
        assert!(unspecified.validate().is_err());
    }

    #[tokio::test]
    async fn tcp_forwards_and_stops_with_the_bridge() {
        let echo = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let echo_port = echo.local_addr().unwrap().port();
        tokio::spawn(async move {
            while let Ok((mut peer, _)) = echo.accept().await {
                tokio::spawn(async move {
                    let mut buffer = [0u8; 4096];
                    loop {
                        match peer.read(&mut buffer).await {
                            Ok(0) | Err(_) => break,
                            Ok(length) => {
                                if peer.write_all(&buffer[..length]).await.is_err() {
                                    break;
                                }
                            }
                        }
                    }
                });
            }
        });
        let listen = free(GAME_ALIAS).await;
        let profile = loopback_profile(vec![Cn115Route { transport: Cn115Transport::Tcp, listen: SocketAddr::new(IpAddr::V4(GAME_ALIAS), listen).to_string(), upstream_port: echo_port }]);
        let bridge = RunningCn115::start(&profile).await.unwrap();
        assert!(bridge.is_running());
        let mut client = TcpStream::connect(SocketAddr::new(IpAddr::V4(GAME_ALIAS), listen)).await.unwrap();
        client.write_all(b"ping").await.unwrap();
        let mut data = [0u8; 4];
        client.read_exact(&mut data).await.unwrap();
        assert_eq!(&data, b"ping");
        bridge.close().await;
        let mut end = [0u8; 1];
        let read = tokio::time::timeout(Duration::from_secs(2), client.read(&mut end)).await.unwrap().unwrap();
        assert_eq!(read, 0);
    }

    #[tokio::test]
    async fn udp_forwards_datagrams_in_both_directions() {
        let echo = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let echo_port = echo.local_addr().unwrap().port();
        tokio::spawn(async move {
            let mut buffer = [0u8; 2048];
            while let Ok((length, from)) = echo.recv_from(&mut buffer).await {
                let _ = echo.send_to(&buffer[..length], from).await;
            }
        });
        let listen = free(GAME_ALIAS).await;
        let profile = loopback_profile(vec![Cn115Route { transport: Cn115Transport::Udp, listen: SocketAddr::new(IpAddr::V4(GAME_ALIAS), listen).to_string(), upstream_port: echo_port }]);
        let bridge = RunningCn115::start(&profile).await.unwrap();
        let client = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        client.send_to(b"datagram", SocketAddr::new(IpAddr::V4(GAME_ALIAS), listen)).await.unwrap();
        let mut buffer = [0u8; 64];
        let (length, _) = tokio::time::timeout(Duration::from_secs(3), client.recv_from(&mut buffer)).await.unwrap().unwrap();
        assert_eq!(&buffer[..length], b"datagram");
        bridge.close().await;
    }

    #[tokio::test]
    async fn one_conflict_starts_nothing_and_leaves_no_listener() {
        let taken = TcpListener::bind(SocketAddr::new(IpAddr::V4(GAME_ALIAS), 0)).await.unwrap();
        let taken_port = taken.local_addr().unwrap().port();
        let other = free(LOGIN_ALIAS).await;
        let profile = loopback_profile(vec![
            Cn115Route { transport: Cn115Transport::Tcp, listen: SocketAddr::new(IpAddr::V4(LOGIN_ALIAS), other).to_string(), upstream_port: 7000 },
            Cn115Route { transport: Cn115Transport::Tcp, listen: SocketAddr::new(IpAddr::V4(GAME_ALIAS), taken_port).to_string(), upstream_port: 7001 },
        ]);
        assert!(RunningCn115::start(&profile).await.is_err());
        // The first endpoint must have been released again.
        let reclaimed = TcpListener::bind(SocketAddr::new(IpAddr::V4(LOGIN_ALIAS), other)).await;
        assert!(reclaimed.is_ok(), "failed start leaked a listener");
        assert!(check(&profile).await.is_err());
        drop(taken);
    }

    #[tokio::test]
    async fn profile_load_rejects_unknown_fields_and_oversize() {
        let directory = std::env::temp_dir().join(format!("cn115-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("profile.json");
        std::fs::write(&path, br#"{"host":"192.168.1.6","extra":1}"#).unwrap();
        assert!(Cn115Profile::load(&path).is_err());
        std::fs::write(&path, br#"{"host":"192.168.1.6"}"#).unwrap();
        let loaded = Cn115Profile::load(&path).unwrap();
        assert_eq!(loaded.routes.len(), 19);
        std::fs::write(&path, vec![b' '; (PROFILE_LIMIT + 1) as usize]).unwrap();
        assert!(Cn115Profile::load(&path).is_err());
        std::fs::remove_dir_all(&directory).ok();
    }
}
