//! Two-player native TCP session. The host alone advances authoritative state.
//! Clients receive only their fog-filtered view, never hidden units or orders.
use crate::{runtime::Snapshot, simulation::NativeGame, terrain::Terrain, world::World};
use bevy::prelude::Resource;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::VecDeque,
    io::{self, Read, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    path::{Path, PathBuf},
    sync::{Mutex, mpsc},
    time::{Duration, Instant},
};

const PROTOCOL: u32 = 1;
const MAX_STATE: usize = 64 * 1024 * 1024;
const MAX_COMMAND: usize = 16 * 1024;

#[derive(Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
enum Wire {
    Hello {
        protocol: u32,
        token: String,
    },
    Welcome {
        token: String,
    },
    Command {
        command: Value,
    },
    State {
        snapshot: Box<Snapshot>,
        terrain: Option<Box<Terrain>>,
        world: Box<World>,
        config: Value,
    },
    Ping,
}

struct Socket {
    stream: TcpStream,
    input: Vec<u8>,
    output: VecDeque<Vec<u8>>,
    offset: usize,
    max_input: usize,
    received: Instant,
    connected: Instant,
}
impl Socket {
    fn new(stream: TcpStream, max_input: usize) -> io::Result<Self> {
        stream.set_nonblocking(true)?;
        stream.set_nodelay(true)?;
        Ok(Self {
            stream,
            input: vec![],
            output: VecDeque::new(),
            offset: 0,
            max_input,
            received: Instant::now(),
            connected: Instant::now(),
        })
    }
    fn send(&mut self, message: &Wire) -> Result<(), String> {
        let body = serde_json::to_vec(message).map_err(|e| e.to_string())?;
        if body.len() > MAX_STATE
            || self.output.iter().map(Vec::len).sum::<usize>() + body.len() > MAX_STATE * 2
        {
            return Err("Network output exceeds limit".into());
        }
        let mut frame = (body.len() as u32).to_be_bytes().to_vec();
        frame.extend(body);
        self.output.push_back(frame);
        Ok(())
    }
    fn flush(&mut self) -> Result<(), String> {
        let mut budget = 256 * 1024;
        while let Some(frame) = self.output.front() {
            let end = frame.len().min(self.offset + budget);
            match self.stream.write(&frame[self.offset..end]) {
                Ok(0) => return Err("Connection closed".into()),
                Ok(n) => {
                    self.offset += n;
                    budget -= n;
                    if self.offset == frame.len() {
                        self.output.pop_front();
                        self.offset = 0;
                    }
                    if budget == 0 {
                        break;
                    }
                }
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(e.to_string()),
            }
        }
        Ok(())
    }
    fn poll(&mut self) -> Result<Vec<Wire>, String> {
        self.flush()?;
        let mut buffer = [0u8; 8192];
        for _ in 0..32 {
            match self.stream.read(&mut buffer) {
                Ok(0) => return Err("Connection closed".into()),
                Ok(n) => {
                    self.received = Instant::now();
                    self.input.extend_from_slice(&buffer[..n]);
                }
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(e.to_string()),
            }
            if self.input.len() > self.max_input + 256 * 1024 {
                return Err("Network input exceeds limit".into());
            }
        }
        let mut messages = vec![];
        let mut consumed = 0;
        while self.input.len() >= consumed + 4 && messages.len() < 64 {
            let size =
                u32::from_be_bytes(self.input[consumed..consumed + 4].try_into().unwrap()) as usize;
            if size == 0 || size > self.max_input {
                return Err("Invalid network frame length".into());
            }
            if self.input.len() < consumed + 4 + size {
                break;
            }
            messages.push(
                serde_json::from_slice(&self.input[consumed + 4..consumed + 4 + size])
                    .map_err(|e| e.to_string())?,
            );
            consumed += 4 + size;
        }
        self.input.drain(..consumed);
        if self.received.elapsed() > Duration::from_secs(10) {
            return Err("Connection timed out".into());
        }
        Ok(messages)
    }
}

#[derive(Serialize, Deserialize)]
struct SavedHost {
    protocol: u32,
    token: Option<String>,
    resume_paused: bool,
    game: NativeGame,
}

struct Host {
    listener: TcpListener,
    socket: Option<Socket>,
    authorized: bool,
    token: Option<String>,
    path: PathBuf,
    resume_paused: bool,
    terrain_key: String,
    sent: Instant,
    saved: Instant,
}
impl Host {
    fn persist(&mut self, game: &NativeGame) -> Result<(), String> {
        atomic_json(
            &self.path,
            &SavedHost {
                protocol: PROTOCOL,
                token: self.token.clone(),
                resume_paused: self.resume_paused,
                game: game.clone(),
            },
        )?;
        self.saved = Instant::now();
        Ok(())
    }
    fn disconnect(&mut self, game: &mut NativeGame) {
        if self.authorized {
            self.resume_paused = game.paused;
        }
        self.socket = None;
        self.authorized = false;
        game.paused = true;
        game.notice = "Guest disconnected; operation paused until reconnect.".into();
        if let Err(e) = self.persist(game) {
            game.notice = format!("Disconnect save failed: {e}");
        }
    }
    fn poll(&mut self, game: &mut NativeGame) -> Result<(), String> {
        if !self.authorized
            && self
                .socket
                .as_ref()
                .is_some_and(|s| s.connected.elapsed() > Duration::from_secs(5))
        {
            self.disconnect(game);
        }
        // Bounded accepts: an idle or invalid handshake cannot hold the lobby forever.
        for _ in 0..8 {
            match self.listener.accept() {
                Ok((stream, _)) if self.socket.is_none() => {
                    self.socket =
                        Some(Socket::new(stream, MAX_COMMAND).map_err(|e| e.to_string())?);
                    self.terrain_key.clear();
                }
                Ok(_) => {}
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
                Err(e) => return Err(e.to_string()),
            }
        }
        let messages = match self.socket.as_mut().map(Socket::poll).transpose() {
            Ok(messages) => messages.unwrap_or_default(),
            Err(_) => {
                self.disconnect(game);
                return Ok(());
            }
        };
        for message in messages {
            match message {
                Wire::Hello { protocol, token } if !self.authorized => {
                    if protocol != PROTOCOL
                        || !match &self.token {
                            Some(expected) => expected == &token,
                            None => {
                                token.len() == 64 && token.bytes().all(|c| c.is_ascii_hexdigit())
                            }
                        }
                    {
                        self.disconnect(game);
                        return Ok(());
                    }
                    if self.token.is_none() {
                        self.token = Some(token);
                    }
                    self.authorized = true;
                    game.paused = self.resume_paused;
                    game.notice = "Guest connected. Native multiplayer active.".into();
                    self.persist(game)?;
                    self.socket.as_mut().unwrap().send(&Wire::Welcome {
                        token: self.token.clone().unwrap(),
                    })?;
                }
                Wire::Command { command } if self.authorized => {
                    if matches!(
                        command["type"].as_str(),
                        Some("command" | "build" | "recruit")
                    ) {
                        game.command_as(1, command);
                    }
                }
                Wire::Ping if self.authorized => {}
                _ => {
                    self.disconnect(game);
                    return Ok(());
                }
            }
        }
        game.human_opponent = true;
        if !self.authorized {
            game.paused = true;
        }
        if self.authorized
            && self.sent.elapsed() >= Duration::from_millis(100)
            && self.socket.as_ref().is_some_and(|s| s.output.is_empty())
        {
            let key = format!(
                "{}:{}:{}:{}:{}",
                game.terrain.id,
                game.terrain.cols,
                game.terrain.rows,
                game.terrain.seed,
                game.terrain.revision
            );
            let terrain = (key != self.terrain_key).then(|| Box::new(game.terrain.clone()));
            self.socket.as_mut().unwrap().send(&Wire::State {
                snapshot: Box::new(game.snapshot_for(1)),
                terrain,
                world: Box::new(game.world.clone()),
                config: game.config.clone(),
            })?;
            self.terrain_key = key;
            self.sent = Instant::now();
        }
        if self.saved.elapsed() >= Duration::from_secs(5) {
            if self.authorized {
                self.resume_paused = game.paused;
            }
            self.persist(game)?;
        }
        Ok(())
    }
}

#[derive(Serialize, Deserialize)]
struct SavedClient {
    address: SocketAddr,
    token: String,
}
struct Client {
    address: SocketAddr,
    token: String,
    path: PathBuf,
    socket: Option<Socket>,
    connecting: Option<mpsc::Receiver<io::Result<TcpStream>>>,
    retry: Instant,
    ping: Instant,
    ready: bool,
    snapshot: Option<Snapshot>,
}
impl Client {
    fn poll(&mut self, game: &mut NativeGame) -> Result<(), String> {
        if self.socket.is_none()
            && self.connecting.is_none()
            && self.retry.elapsed() >= Duration::from_secs(2)
        {
            let (send, receive) = mpsc::channel();
            let address = self.address;
            std::thread::spawn(move || {
                let _ = send.send(TcpStream::connect_timeout(&address, Duration::from_secs(2)));
            });
            self.connecting = Some(receive);
            self.retry = Instant::now();
        }
        if let Some(receive) = &self.connecting {
            match receive.try_recv() {
                Ok(result) => {
                    self.connecting = None;
                    match result {
                        Ok(stream) => {
                            let mut socket =
                                Socket::new(stream, MAX_STATE).map_err(|e| e.to_string())?;
                            socket.send(&Wire::Hello {
                                protocol: PROTOCOL,
                                token: self.token.clone(),
                            })?;
                            self.socket = Some(socket);
                        }
                        Err(e) => {
                            return Err(format!(
                                "Connecting to {}: {e}. Retrying...",
                                self.address
                            ));
                        }
                    }
                }
                Err(mpsc::TryRecvError::Empty) => {}
                Err(_) => {
                    self.connecting = None;
                    return Err("Connection worker stopped".into());
                }
            }
        }
        let messages = match self.socket.as_mut().map(Socket::poll).transpose() {
            Ok(messages) => messages.unwrap_or_default(),
            Err(e) => {
                self.socket = None;
                self.ready = false;
                game.paused = true;
                if let Some(s) = &mut self.snapshot {
                    s.paused = true;
                }
                return Err(format!("Disconnected: {e}. Reconnecting..."));
            }
        };
        for message in messages {
            match message {
                Wire::Welcome { token } => {
                    self.token = token;
                    atomic_json(
                        &self.path,
                        &SavedClient {
                            address: self.address,
                            token: self.token.clone(),
                        },
                    )?;
                    self.ready = true;
                }
                Wire::State {
                    snapshot,
                    terrain,
                    world,
                    config,
                } if self.ready => {
                    if let Some(mut terrain) = terrain {
                        terrain.hydrate();
                        game.terrain = *terrain;
                    }
                    game.world = *world;
                    game.config = config;
                    game.tick = snapshot.tick;
                    game.paused = snapshot.paused;
                    game.winner = snapshot.winner.as_i64().map(|n| n as i32);
                    // Client never simulates or receives authoritative hidden entities.
                    game.entities.clear();
                    game.notice = snapshot.notice.clone();
                    self.snapshot = Some(*snapshot);
                }
                _ => {
                    self.socket = None;
                    self.ready = false;
                    return Err("Unexpected server message".into());
                }
            }
        }
        if self.ready && self.ping.elapsed() >= Duration::from_secs(1) {
            if let Some(s) = &mut self.socket {
                s.send(&Wire::Ping)?;
            }
            self.ping = Instant::now();
        }
        Ok(())
    }
}

enum Mode {
    Solo,
    Host(Box<Host>),
    Client(Box<Client>),
}
#[derive(Resource)]
pub struct Session {
    mode: Mutex<Mode>,
    pub owner: i32,
    pub status: String,
}
impl Default for Session {
    fn default() -> Self {
        Self {
            mode: Mutex::new(Mode::Solo),
            owner: 0,
            status: "SOLO".into(),
        }
    }
}
impl Session {
    pub fn host(address: &str, path: PathBuf, game: &mut NativeGame) -> Result<Self, String> {
        let listener = TcpListener::bind(address).map_err(|e| e.to_string())?;
        listener.set_nonblocking(true).map_err(|e| e.to_string())?;
        let mut token = None;
        let mut resume_paused = game.paused;
        if path.exists() {
            let data = std::fs::read(&path).map_err(|e| e.to_string())?;
            if data.len() > 128 * 1024 * 1024 {
                return Err("Host save exceeds limit".into());
            }
            let mut saved: SavedHost = serde_json::from_slice(&data).map_err(|e| e.to_string())?;
            if saved.protocol != PROTOCOL {
                return Err("Unsupported host session version".into());
            }
            saved.game.validate()?;
            saved.game.terrain.hydrate();
            saved.game.refresh_towns();
            saved.game.ground_roster_cleanup();
            *game = saved.game;
            token = saved.token;
            resume_paused = saved.resume_paused;
        }
        game.human_opponent = true;
        game.paused = true;
        game.notice = "Waiting for guest. Share the host IP address and port.".into();
        let status = format!(
            "HOST {} | Field Command",
            listener.local_addr().map_err(|e| e.to_string())?
        );
        let mut host = Host {
            listener,
            socket: None,
            authorized: false,
            token,
            path,
            resume_paused,
            terrain_key: String::new(),
            sent: Instant::now(),
            saved: Instant::now(),
        };
        host.persist(game)?;
        Ok(Self {
            mode: Mutex::new(Mode::Host(Box::new(host))),
            owner: 0,
            status,
        })
    }
    pub fn join(address: &str, path: PathBuf) -> Result<Self, String> {
        let address = address.parse::<SocketAddr>().map_err(|_| {
            "Join address must be IP:port (for example 192.168.1.10:4174)".to_string()
        })?;
        let saved = std::fs::read(&path)
            .ok()
            .and_then(|data| serde_json::from_slice::<SavedClient>(&data).ok());
        let token = match saved.filter(|s| s.address == address).map(|s| s.token) {
            Some(token) => token,
            None => new_token()?,
        };
        // Persist before hello so a lost welcome cannot strand a claimed slot.
        atomic_json(
            &path,
            &SavedClient {
                address,
                token: token.clone(),
            },
        )?;
        Ok(Self {
            owner: 1,
            status: format!("JOIN {address} | Iron Front"),
            mode: Mutex::new(Mode::Client(Box::new(Client {
                address,
                token,
                path,
                socket: None,
                connecting: None,
                retry: Instant::now() - Duration::from_secs(2),
                ping: Instant::now(),
                ready: false,
                snapshot: None,
            }))),
        })
    }
    pub fn multiplayer(&self) -> bool {
        !matches!(*self.mode.lock().unwrap(), Mode::Solo)
    }
    pub fn can_step(&self) -> bool {
        match &*self.mode.lock().unwrap() {
            Mode::Solo => true,
            Mode::Host(h) => h.authorized,
            Mode::Client(_) => false,
        }
    }
    pub fn receive(&mut self, game: &mut NativeGame, commands: Vec<Value>) -> Snapshot {
        let mut mode = self.mode.lock().unwrap();
        match &mut *mode {
            Mode::Solo => {
                game.human_opponent = crate::simulation::mock_battle_enabled();
                for command in commands {
                    game.command(command);
                }
                game.snapshot()
            }
            Mode::Host(host) => {
                for command in commands {
                    // Save/load target the shared session. Guest never controls host files.
                    match command["type"].as_str() {
                        Some("save") => {
                            if let Err(e) = host.persist(game) {
                                game.notice = e;
                            }
                        }
                        Some("load") => {
                            game.notice="Restart the host with the same --session path to restore multiplayer.".into();
                        }
                        _ => game.command(command),
                    }
                }
                if let Err(e) = host.poll(game) {
                    host.disconnect(game);
                    game.notice = format!("Network error: {e}");
                }
                game.snapshot()
            }
            Mode::Client(client) => {
                if let Err(e) = client.poll(game) {
                    self.status = e;
                } else if client.ready {
                    self.status = format!("JOIN {} | Iron Front", client.address);
                }
                for command in commands {
                    if client.ready
                        && matches!(
                            command["type"].as_str(),
                            Some("command" | "build" | "recruit")
                        )
                        && let Some(socket) = &mut client.socket
                        && let Err(e) = socket.send(&Wire::Command { command })
                    {
                        self.status = e;
                    }
                }
                let mut snapshot = client.snapshot.clone().unwrap_or_default();
                if !client.ready {
                    snapshot.paused = true;
                    snapshot.notice = self.status.clone();
                }
                snapshot
            }
        }
    }
    pub fn persist(&self, game: &NativeGame) -> Result<(), String> {
        if let Mode::Host(host) = &mut *self.mode.lock().unwrap() {
            if host.authorized {
                host.resume_paused = game.paused;
            }
            host.persist(game)?;
        }
        Ok(())
    }
}
fn new_token() -> Result<String, String> {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).map_err(|e| e.to_string())?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}
fn atomic_json(path: &Path, value: &impl Serialize) -> Result<(), String> {
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let temp = path.with_extension("tmp");
    std::fs::write(&temp, serde_json::to_vec(value).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    std::fs::rename(temp, path).map_err(|e| e.to_string())
}
pub fn save_on_exit(
    exit: bevy::prelude::MessageReader<bevy::app::AppExit>,
    game: bevy::prelude::Res<NativeGame>,
    session: bevy::prelude::Res<Session>,
) {
    let mut exit = exit;
    if exit.read().next().is_some()
        && let Err(e) = session.persist(&game)
    {
        eprintln!("Session save failed: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn pump(
        host: &mut Session,
        game: &mut NativeGame,
        client: &mut Session,
        view: &mut NativeGame,
        done: impl Fn(&Snapshot) -> bool,
    ) -> Snapshot {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            host.receive(game, vec![]);
            let snapshot = client.receive(view, vec![]);
            if done(&snapshot) {
                return snapshot;
            }
            assert!(
                Instant::now() < deadline,
                "Network did not settle: {}",
                client.status
            );
            std::thread::sleep(Duration::from_millis(2));
        }
    }
    fn address(session: &Session) -> String {
        let mode = session.mode.lock().unwrap();
        let Mode::Host(host) = &*mode else { panic!() };
        host.listener.local_addr().unwrap().to_string()
    }
    #[test]
    fn host_guest_authority_fog_disconnect_and_restart() {
        let dir = std::env::temp_dir().join(format!("row-network-{}", new_token().unwrap()));
        let host_path = dir.join("host.json");
        let guest_path = dir.join("guest.json");
        let mut game = NativeGame::new();
        game.entities.clear();
        let hidden = game.spawn("short", 0, [10., 10.]);
        let guest = game.spawn("scout", 1, [140., 140.]);
        game.supply = [41., 19.];
        let mut host = Session::host("127.0.0.1:0", host_path.clone(), &mut game).unwrap();
        let addr = address(&host);
        let mut client = Session::join(&addr, guest_path.clone()).unwrap();
        let mut view = NativeGame::new();
        assert!(!host.can_step() && !client.can_step());
        let snapshot = pump(&mut host, &mut game, &mut client, &mut view, |s| {
            s.entities.iter().any(|p| p.id == guest)
        });
        assert!(host.can_step() && !client.can_step());
        assert!(!game.paused && game.human_opponent);
        assert!(snapshot.entities.iter().all(|p| p.id != hidden));
        assert_eq!(snapshot.supply, 19.);
        assert_eq!(view.terrain.id, game.terrain.id);
        assert!(view.entities.is_empty());
        client.receive(&mut view,vec![json!({"type":"command","owner":0,"unitIds":[hidden,guest],"commandType":"move","destination":{"x":145.,"y":140.}})]);
        pump(&mut host, &mut game, &mut client, &mut view, |s| {
            s.notice.contains("1 unit(s)")
        });
        for _ in 0..40 {
            if host.can_step() {
                game.step();
            }
        }
        assert_eq!(
            game.entities
                .iter()
                .find(|e| e.view.id == hidden)
                .unwrap()
                .view
                .x,
            10.
        );
        assert!(
            game.entities
                .iter()
                .find(|e| e.view.id == guest)
                .unwrap()
                .view
                .x
                > 140.
        );
        let tick = game.tick;
        drop(client);
        let deadline = Instant::now() + Duration::from_secs(2);
        while host.can_step() {
            host.receive(&mut game, vec![]);
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(2));
        }
        assert!(game.paused);
        host.persist(&game).unwrap();
        drop(host);
        let mut restored = NativeGame::default();
        let mut host = Session::host(&addr, host_path.clone(), &mut restored).unwrap();
        assert_eq!(restored.tick, tick);
        let mut client = Session::join(&addr, guest_path.clone()).unwrap();
        pump(&mut host, &mut restored, &mut client, &mut view, |s| {
            s.tick == tick && !s.paused
        });
        assert!(host.can_step());
        assert_eq!(restored.tick, tick);
        drop(client);
        drop(host);
        std::fs::remove_file(host_path).unwrap();
        std::fs::remove_file(guest_path).unwrap();
        std::fs::remove_dir(dir).unwrap();
    }
    #[test]
    fn frames_handle_fragmentation_and_reject_oversize() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let mut stream = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let (peer, _) = listener.accept().unwrap();
        let mut socket = Socket::new(peer, MAX_COMMAND).unwrap();
        let body = serde_json::to_vec(&Wire::Ping).unwrap();
        let size = (body.len() as u32).to_be_bytes();
        stream.write_all(&size[..2]).unwrap();
        assert!(socket.poll().unwrap().is_empty());
        stream.write_all(&size[2..]).unwrap();
        stream.write_all(&body[..2]).unwrap();
        assert!(socket.poll().unwrap().is_empty());
        stream.write_all(&body[2..]).unwrap();
        let deadline = Instant::now() + Duration::from_secs(1);
        loop {
            if !socket.poll().unwrap().is_empty() {
                break;
            }
            assert!(Instant::now() < deadline);
            std::thread::yield_now();
        }
        stream
            .write_all(&((MAX_COMMAND + 1) as u32).to_be_bytes())
            .unwrap();
        loop {
            if socket.poll().is_err() {
                break;
            }
            assert!(Instant::now() < deadline);
            std::thread::yield_now();
        }
    }
    #[test]
    fn reconnect_rejects_wrong_token_and_preserves_pause() {
        let dir = std::env::temp_dir().join(format!("row-auth-{}", new_token().unwrap()));
        let path = dir.join("host.json");
        let mut game = NativeGame::new();
        let mut host = Session::host("127.0.0.1:0", path.clone(), &mut game).unwrap();
        {
            let mut mode = host.mode.lock().unwrap();
            let Mode::Host(h) = &mut *mode else { panic!() };
            h.token = Some(new_token().unwrap());
        }
        let stream = TcpStream::connect(address(&host)).unwrap();
        let mut socket = Socket::new(stream, MAX_STATE).unwrap();
        socket
            .send(&Wire::Hello {
                protocol: PROTOCOL,
                token: "wrong".into(),
            })
            .unwrap();
        socket.flush().unwrap();
        for _ in 0..20 {
            host.receive(&mut game, vec![]);
            std::thread::sleep(Duration::from_millis(2));
        }
        assert!(!host.can_step() && game.paused);
        drop(host);
        drop(socket);
        std::fs::remove_file(path).unwrap();
        std::fs::remove_dir(dir).unwrap();
    }
}
