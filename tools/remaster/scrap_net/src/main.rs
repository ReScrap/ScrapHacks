use anyhow::{bail, ensure, Result};
use binrw::BinReaderExt;
use binrw::{BinRead, NullString};
use chacha20::cipher::KeyInit;
use chacha20::cipher::{KeyIvInit, StreamCipher, StreamCipherSeek};
use chacha20::ChaCha20;
use clap::Parser;
use dialoguer::theme::ColorfulTheme;
use dialoguer::Select;
use futures_util::FutureExt;
use poly1305::Poly1305;
use rand::{thread_rng, Rng};
use rhexdump::hexdump;
use rustyline_async::{Readline, ReadlineError, SharedWriter};
use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::Display;
use std::io::Cursor;
use std::io::Write;
use std::iter;
use std::net::SocketAddr;
use std::net::ToSocketAddrs;
use std::net::{IpAddr, Ipv4Addr};
use std::path::PathBuf;
use std::time::{Duration, Instant};
use tokio::io::AsyncBufReadExt;
use tokio::net::UdpSocket;
use tokio::time;

mod hex_ii;
mod parser;

const KEY: &[u8; 32] = b"\x02\x04\x06\x08\x0a\x0c\x0e\x10\x12\x14\x16\x18\x1a\x1c\x1e\x20\x22\x24\x26\x28\x2a\x2c\x2e\x30\x32\x34\x36\x38\x3a\x3c\x3e\x40";
const INFO_PACKET: &[u8] = b"\x7f\x01\x00\x00\x07";

#[derive(Debug, Clone)]
struct ServerFlags {
    dedicated: bool,
    force_vehicle: bool,
    _rest: u8,
}

impl Display for ServerFlags {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let force_vehicle = if self.force_vehicle { "F" } else { " " };
        let dedicated = if self.dedicated { "D" } else { " " };
        write!(f, "{}{}", force_vehicle, dedicated)?;
        Ok(())
    }
}

impl From<u8> for ServerFlags {
    fn from(v: u8) -> Self {
        ServerFlags {
            dedicated: v & 0b1 != 0,
            force_vehicle: v & 0b10 != 0,
            _rest: (v & 0b11111100) >> 2,
        }
    }
}

#[derive(Debug, Clone, BinRead)]
#[br(little, magic = b"\xba\xce", import(rtt: Duration, addr: SocketAddr))]
pub struct Server {
    #[br(calc=addr)]
    addr: SocketAddr,
    #[br(calc=rtt)]
    rtt: Duration,
    #[br(map = |v: (u8,u8)| format!("{}.{}",v.0,v.1))]
    version: String,
    port: u16,
    max_players: u16,
    cur_players: u16,
    #[br(map = u8::into)]
    flags: ServerFlags,
    #[br(pad_size_to(0x20), map = |s :NullString| s.to_string())]
    name: String,
    #[br(pad_size_to(0x10), map = |s :NullString| s.to_string())]
    mode: String,
    #[br(pad_size_to(0x20), map = |s :NullString| s.to_string())]
    map: String,
    _pad: u8,
}

fn pad_copy(d: &[u8], l: usize) -> Vec<u8> {
    let diff = d.len() % l;
    if diff != 0 {
        d.iter()
            .copied()
            .chain(iter::repeat(0).take(l - diff))
            .collect()
    } else {
        d.to_vec()
    }
}

fn pad(d: &mut Vec<u8>, l: usize) {
    let diff = d.len() % l;
    if diff != 0 {
        d.extend(iter::repeat(0).take(l - diff))
    }
}

struct Packet {
    nonce: Vec<u8>,
    data: Vec<u8>,
}

impl Packet {
    fn encrypt(data: &[u8]) -> Packet {
        let mut data: Vec<u8> = data.to_vec();
        let mut rng = thread_rng();
        let mut nonce = vec![0u8; 12];
        rng.fill(nonce.as_mut_slice());
        let mut cipher = ChaCha20::new(KEY.into(), nonce.as_slice().into());
        cipher.seek(KEY.len() + 32);
        cipher.apply_keystream(&mut data);
        Packet { nonce, data }
    }

    fn get_tag(&self) -> Vec<u8> {
        let mut sign_data = vec![];
        sign_data.extend(pad_copy(&self.nonce, 16).iter());
        sign_data.extend(pad_copy(&self.data, 16).iter());
        sign_data.extend((self.nonce.len() as u64).to_le_bytes().iter());
        sign_data.extend((self.data.len() as u64).to_le_bytes().iter());
        let mut cipher = ChaCha20::new(KEY.into(), self.nonce.as_slice().into());
        let mut poly_key = *KEY;
        cipher.apply_keystream(&mut poly_key);
        let signer = Poly1305::new(&poly_key.into());
        signer.compute_unpadded(&sign_data).into_iter().collect()
    }

    fn bytes(&self) -> Vec<u8> {
        let mut data = vec![];
        data.extend(pad_copy(&self.nonce, 16).iter());
        data.extend(pad_copy(&self.data, 16).iter());
        data.extend((self.nonce.len() as u64).to_le_bytes().iter());
        data.extend((self.data.len() as u64).to_le_bytes().iter());
        data.extend(self.get_tag().iter());
        data
    }

    fn decrypt(&self) -> Result<Vec<u8>> {
        let mut data = self.data.clone();
        let mut sign_data = data.clone();
        pad(&mut sign_data, 16);
        let mut nonce = self.nonce.clone();
        pad(&mut nonce, 16);
        let sign_data = nonce
            .iter()
            .chain(sign_data.iter())
            .chain((self.nonce.len() as u64).to_le_bytes().iter())
            .chain((self.data.len() as u64).to_le_bytes().iter())
            .copied()
            .collect::<Vec<u8>>();
        let mut poly_key = *KEY;
        let mut cipher = ChaCha20::new(KEY.into(), self.nonce.as_slice().into());
        cipher.apply_keystream(&mut poly_key);
        let signer = Poly1305::new(&poly_key.into());
        let signature: Vec<u8> = signer.compute_unpadded(&sign_data).into_iter().collect();

        if signature != self.get_tag() {
            bail!("Invalid signature!");
        };
        cipher.seek(poly_key.len() + 32);
        cipher.apply_keystream(&mut data);
        Ok(data)
    }
}

impl TryFrom<&[u8]> for Packet {
    type Error = anyhow::Error;
    fn try_from(data: &[u8]) -> Result<Self> {
        let (mut nonce, data) = data.split_at(16);
        let (mut data, tag) = data.split_at(data.len() - 16);
        let nonce_len = u64::from_le_bytes(data[data.len() - 16..][..8].try_into()?) as usize;
        let data_len = u64::from_le_bytes(data[data.len() - 8..].try_into()?) as usize;
        data = &data[..data_len];
        nonce = &nonce[..nonce_len];
        let pkt = Packet {
            nonce: nonce.into(),
            data: data.into(),
        };
        if pkt.get_tag() != tag {
            bail!("Invalid signature!");
        }
        Ok(pkt)
    }
}

#[derive(Debug, Clone)]
pub enum ServerEntry {
    Alive(Server),
    Dead { addr: SocketAddr, reason: String },
}

impl Display for ServerEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ServerEntry::Alive(srv) => write!(
                f,
                "[{}] {} ({} {}/{} Players on {}) version {} [{}] RTT: {:?}",
                srv.addr,
                srv.name,
                srv.mode,
                srv.cur_players,
                srv.max_players,
                srv.map,
                srv.version,
                srv.flags,
                srv.rtt
            ),
            ServerEntry::Dead { addr, reason } => write!(f, "[{}] (error: {})", addr, reason),
        }
    }
}

fn encrypt(data: &[u8]) -> Vec<u8> {
    Packet::encrypt(data).bytes()
}

fn decrypt(data: &[u8]) -> Result<Vec<u8>> {
    Packet::try_from(data)?.decrypt()
}

async fn recv_from_timeout(
    sock: &UdpSocket,
    buf: &mut [u8],
    timeout: f64,
) -> Result<(usize, SocketAddr)> {
    Ok(time::timeout(Duration::from_secs_f64(timeout), sock.recv_from(buf)).await??)
}

async fn query_server<'a>(addr: SocketAddr) -> Result<Server> {
    let mut buf = [0; 32 * 1024];
    let socket = UdpSocket::bind("0.0.0.0:0").await?;
    socket.connect(addr).await?;
    let msg = encrypt(INFO_PACKET);
    let t_start = Instant::now();
    socket.send(&msg).await?;
    let size = recv_from_timeout(&socket, &mut buf, 5.0).await?.0;
    let rtt = t_start.elapsed();
    let data = decrypt(&buf[..size])?;
    if !data.starts_with(&[0xba, 0xce]) {
        // Server Info
        bail!("Invalid response");
    }
    let mut cur = Cursor::new(&data);
    let info: Server = cur.read_le_args((rtt, addr))?;
    if info.port != addr.port() {
        eprint!("[WARN] Port differs for {}: {}", addr, info.port);
    }
    if cur.position() != (data.len() as u64) {
        bail!("Leftover data");
    }
    Ok(info)
}

async fn get_servers(master_addr: &str) -> Result<(Duration, Vec<ServerEntry>)> {
    let master_addr: SocketAddr = master_addr.to_socket_addrs()?.next().unwrap();
    let mut rtt = std::time::Duration::from_secs_f32(0.0);
    let mut servers = vec![];
    let mut buf = [0; 32 * 1024];
    let master = UdpSocket::bind("0.0.0.0:0").await?;
    master.connect(master_addr).await?;
    for n in 0..(256 / 32) {
        let data = format!("Brw={},{}\0", n * 32, (n + 1) * 32);
        let data = &encrypt(data.as_bytes());
        let t_start = Instant::now();
        master.send(data).await?;
        let size = master.recv(&mut buf).await?;
        rtt += t_start.elapsed();
        let data = decrypt(&buf[..size])?;
        if data.starts_with(b"\0\0\0\0}") {
            for chunk in data[5..].chunks(6) {
                if chunk.iter().all(|v| *v == 0) {
                    break;
                }
                let port = u16::from_le_bytes(chunk[chunk.len() - 2..].try_into()?);
                let addr = SocketAddr::from(([chunk[0], chunk[1], chunk[2], chunk[3]], port));
                let server = match query_server(addr).await {
                    Ok(server) => ServerEntry::Alive(server),
                    Err(err) => ServerEntry::Dead {
                        addr,
                        reason: err.to_string(),
                    },
                };
                servers.push(server);
            }
        }
    }
    rtt = Duration::from_secs_f64(rtt.as_secs_f64() / ((256 / 32) as f64));
    Ok((rtt, servers))
}

fn indent_hexdump(data: &[u8], indentation: usize, label: &str) -> String {
    let mut out = String::new();
    let indent = " ".repeat(indentation);
    out.push_str(&indent);
    out.push_str(label);
    out.push('\n');
    for line in rhexdump::hexdump(data).lines() {
        out.push_str(&indent);
        out.push_str(line);
        out.push('\n');
    }
    out.trim_end().to_owned()
}

#[derive(Default, Debug)]
struct State {
    client: BTreeMap<usize, BTreeMap<u8, usize>>,
    server: BTreeMap<usize, BTreeMap<u8, usize>>,
}

impl State {
    fn update_client(&mut self, data: &[u8]) {
        data.iter().enumerate().for_each(|(pos, b)| {
            *self.client.entry(pos).or_default().entry(*b).or_default() += 1;
        });
    }
    fn update_server(&mut self, data: &[u8]) {
        data.iter().enumerate().for_each(|(pos, b)| {
            *self.server.entry(pos).or_default().entry(*b).or_default() += 1;
        });
    }
}

#[derive(Debug, PartialEq, Eq, Copy, Clone)]
enum Direction {
    Client,
    Server,
    Both,
}

#[derive(Debug)]
enum CmdResult {
    Exit,
    Packet {
        data: Vec<u8>,
        direction: Direction,
    },
    Fuzz {
        direction: Direction,
        start: usize,
        end: usize,
        chance: (u32, u32),
    },
    NoFuzz,
    Log(bool),
}

async fn handle_line(
    line: &str,
    state: &State,
    stdout: &mut SharedWriter,
) -> Result<Option<CmdResult>> {
    use CmdResult::*;
    let cmd: Vec<&str> = line.trim().split_ascii_whitespace().collect();
    match cmd[..] {
        ["log", "off"] => Ok(Some(Log(false))),
        ["log", "on"] => Ok(Some(Log(true))),
        ["state", pos] => {
            let pos = pos.parse()?;
            writeln!(stdout, "Client: {:?}", state.client.get(&pos))?;
            writeln!(stdout, "Server: {:?}", state.server.get(&pos))?;
            Ok(None)
        }
        [dir @ ("client" | "server"), ref args @ ..] => {
            let mut data: Vec<u8> = vec![];
            for args in args.iter() {
                let args = hex::decode(args)?;
                data.extend(args);
            }
            Ok(Some(CmdResult::Packet {
                data,
                direction: match dir {
                    "client" => Direction::Client,
                    "server" => Direction::Server,
                    _ => unreachable!(),
                },
            }))
        }
        ["fuzz", dir @ ("client" | "server" | "both"), start, end, chance_num, chance_den] => {
            let direction = match dir {
                "client" => Direction::Client,
                "server" => Direction::Server,
                "both" => Direction::Both,
                _ => unreachable!(),
            };
            let start = start.parse()?;
            let end = end.parse()?;
            if start > end {
                bail!("Fuzz start>end");
            }
            let res = CmdResult::Fuzz {
                direction,
                start,
                end,
                chance: (chance_num.parse()?, chance_den.parse()?),
            };
            Ok(Some(res))
        }
        ["fuzz", "off"] => Ok(Some(CmdResult::NoFuzz)),
        ["exit"] => Ok(Some(CmdResult::Exit)),
        [""] => Ok(None),
        _ => bail!("Unknown command: {:?}", line),
    }
}

async fn run_proxy(
    remote_addr: &SocketAddr,
    local_addr: &SocketAddr,
    logfile: &Option<PathBuf>,
) -> Result<()> {
    let mut print_log = false;
    let mut state = State::default();
    let mut logfile = match logfile {
        Some(path) => Some(std::fs::File::create(path)?),
        None => None,
    };
    let mut fuzz = None;
    let mut rng = thread_rng();
    let mut client_addr: Option<SocketAddr> = None;
    let local = UdpSocket::bind(local_addr).await?;
    let remote = UdpSocket::bind("0.0.0.0:0").await?;
    remote.connect(remote_addr).await?;
    let mut local_buf = vec![0; 32 * 1024];
    let mut remote_buf = vec![0; 32 * 1024];
    println!("Proxy listening on {}", local_addr);
    let (mut rl, mut stdout) = Readline::new(format!("{}> ", remote_addr)).unwrap();
    loop {
        tokio::select! {
            line = rl.readline().fuse() => {
                match line {
                    Ok(line) => {
                        let line=line.trim();
                        rl.add_history_entry(line.to_owned());
                        match  handle_line(line, &state, &mut stdout).await {
                            Ok(Some(result)) => {
                                match result {
                                        CmdResult::Packet{data,direction} => {
                                            let data=encrypt(&data);
                                            match direction {
                                                Direction::Client => {
                                                    if client_addr.is_some() {
                                                        local
                                                            .send_to(&data, client_addr.unwrap())
                                                            .await?;
                                                    } else {
                                                        writeln!(stdout,"Error: No client address")?;
                                                    }
                                                },
                                                Direction::Server => {
                                                    remote.send(&data).await?;
                                                }
                                                Direction::Both => unreachable!()
                                            };
                                        }
                                        CmdResult::Log(log) => {
                                            print_log=log;
                                        }
                                        CmdResult::Exit => break Ok(()),
                                        CmdResult::NoFuzz => {
                                            fuzz=None;
                                        }
                                        CmdResult::Fuzz { .. } => {
                                            fuzz=Some(result)
                                        },
                                    }
                            },
                            Ok(None) => (),
                            Err(msg) => {
                                writeln!(stdout, "Error: {}", msg)?;
                            }
                        }
                    },
                    Err(ReadlineError::Eof) =>{ writeln!(stdout, "Exiting...")?; break Ok(()) },
                    Err(ReadlineError::Interrupted) => {
                        writeln!(stdout, "^C")?;
                        break Ok(());
                    },
                    Err(err) => {
                        writeln!(stdout, "Received err: {:?}", err)?;
                        writeln!(stdout, "Exiting...")?;
                        break Ok(());
                    }
                }
            }
            local_res = local.recv_from(&mut local_buf) => {
                let (size, addr) = local_res?;
                client_addr.get_or_insert(addr);
                let mut data = Packet::try_from(&local_buf[..size])?.decrypt()?;
                state.update_client(&data);
                if print_log {
                    writeln!(stdout,"{}", indent_hexdump(&data, 0, &format!("OUT: {}", addr)))?;
                }
                if let Some(lf) = logfile.as_mut() {
                    writeln!(lf, ">{:?} {} {}", addr, data.len(), hex::encode(&data))?;
                };
                if let Some(CmdResult::Fuzz{direction,start,end,chance}) = fuzz {
                    if (direction==Direction::Server || direction==Direction::Both) && rng.gen_ratio(chance.0,chance.1) {
                        rng.fill(&mut data[start..end]);
                    }
                }
                remote.send(&encrypt(&data)).await?;
            }
            remote_res = remote.recv_from(&mut remote_buf) => {
                let (size, addr) = remote_res?;
                let mut data = Packet::try_from(&remote_buf[..size])?.decrypt()?;
                state.update_server(&data);
                if print_log {
                    writeln!(stdout,"\r{}", indent_hexdump(&data, 5, &format!("IN: {}", addr)))?;
                }
                if let Some(lf) = logfile.as_mut() {
                    writeln!(lf, "<{:?} {} {}", addr, data.len(), hex::encode(&data))?;
                };
                if client_addr.is_some() {
                    if let Some(CmdResult::Fuzz{direction,start,end,chance}) = &fuzz {
                        if (*direction==Direction::Client || *direction==Direction::Both) && rng.gen_ratio(chance.0,chance.1) {
                            rng.fill(&mut data[*start..*end]);
                        }
                    }
                    local
                        .send_to(&encrypt(&data), client_addr.unwrap())
                        .await?;
                }
            }
        }
    }
}

async fn send_master_cmd(sock: &UdpSocket, cmd: &str) -> Result<Vec<u8>> {
    let mut buf = [0; 32 * 1024];
    let mut data: Vec<u8> = cmd.as_bytes().to_vec();
    data.push(0);
    let data = &encrypt(&data);
    sock.send(data).await?;
    let size = recv_from_timeout(sock, &mut buf, 5.0).await?.0;
    decrypt(&buf[..size])
}

async fn run_master_shell(master_addr: &str) -> Result<()> {
    let master = UdpSocket::bind("0.0.0.0:0").await?;
    master.connect(master_addr).await?;
    let (mut rl, mut stdout) = Readline::new(format!("{}> ", master_addr)).unwrap();
    loop {
        tokio::select! {
            line = rl.readline().fuse() => {
                match line {
                    Ok(line) => {
                        let line=line.trim();
                        rl.add_history_entry(line.to_owned());
                        writeln!(stdout,"[CMD] {line}")?;
                        match send_master_cmd(&master,line).await {
                            Ok(data) => writeln!(stdout,"{}",hexdump(&data))?,
                            Err(e) => writeln!(stdout,"Error: {e}")?
                        }
                    }
                    Err(ReadlineError::Eof) =>{ writeln!(stdout, "Exiting...")?; break Ok(()) },
                    Err(ReadlineError::Interrupted) => {
                        writeln!(stdout, "^C")?;
                        break Ok(());
                    },
                    Err(err) => {
                        writeln!(stdout, "Receive error: {err}")?;
                        break Err(err.into());
                    }
                }
            }
        }
    }
}

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    /// Server to connect to (if unspecified will query the master server)
    server: Option<SocketAddr>,
    /// Only list servers without starting proxy
    #[clap(short, long, action)]
    list: bool,
    /// Local Address to bind to
    #[clap(short,long, default_value_t = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 28086))]
    addr: SocketAddr,
    /// Master server to query for running games
    #[clap(short, long, default_value = "scrapland.mercurysteam.com:5000")]
    master: String,
    /// Path of file to log decrypted packets to
    #[clap(short = 'f', long)]
    logfile: Option<PathBuf>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    if args.list && args.server.is_some() {
        let addr = args.server.unwrap();
        let server = match query_server(addr).await {
            Ok(server) => ServerEntry::Alive(server),
            Err(msg) => ServerEntry::Dead {
                addr,
                reason: msg.to_string(),
            },
        };
        println!("{}", server);
        return Ok(());
    }
    if let Some(server) = args.server {
        run_proxy(&server, &args.addr, &args.logfile).await?;
        return Ok(());
    }
    loop {
        let (rtt, servers) = get_servers(&args.master).await?;
        println!("Master RTT: {:?}", rtt);
        if args.list {
            for server in servers {
                println!("{}", server);
            }
            return Ok(());
        }
        let selection = Select::with_theme(&ColorfulTheme::default())
            .items(&servers)
            .with_prompt("Select server (press Esc to drop into master server command shell)")
            .interact_opt()?
            .map(|v| &servers[v]);
        match selection {
            Some(ServerEntry::Dead { addr, reason }) => {
                eprintln!("{:?} returned an error: {}", addr, reason)
            }
            Some(ServerEntry::Alive(srv)) => {
                return run_proxy(&srv.addr, &args.addr, &args.logfile).await;
            }
            None => {
                return run_master_shell(&args.master).await;
            }
        }
    }
}
