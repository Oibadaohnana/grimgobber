//! The front door at `grim.buggly.de`: a small UDP service that knows the
//! lobbies by code. Each lobby is a game server of its own, a child process
//! on a port of its own (`--server PORT --code CODE`). SCARE always runs (and
//! is started again if it dies); a player may open more (`HOST`), each with a
//! random code, which close once empty for `IDLE` seconds.
//!
//! One line of text each way: `GRIM <protocol> JOIN <code>` or `GRIM
//! <protocol> HOST`, answered `LOBBY <code> <port>`, `NONE` (no such lobby),
//! `FULL` (no room for another) or `OLD` (the asker's game is another
//! version than the server's).

use crate::net::{PORT, PROTOCOL};
use crate::rng::Rng;
use std::net::{Ipv4Addr, SocketAddr, ToSocketAddrs, UdpSocket};
use std::process::{Child, Command};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub const HOST: &str = "grim.buggly.de";
pub const FIRST: &str = "SCARE";
pub const CODE_LEN: usize = 5;
/// Lobbies at once, SCARE among them, on the ports after the door's.
const LOBBIES: u16 = 8;
/// A lobby other than SCARE closes once empty this long, in seconds.
pub const IDLE: f32 = 600.0;
/// How long the client waits for an answer, asking again every `AGAIN`.
const WAIT: f32 = 3.0;
const AGAIN: f32 = 0.5;

struct Lobby {
    code: String,
    port: u16,
    child: Child,
}

fn open(code: &str, port: u16) -> std::io::Result<Child> {
    println!("opening lobby {code} on UDP port {port}");
    Command::new(std::env::current_exe()?)
        .args(["--server", &port.to_string(), "--code", code])
        .spawn()
}

fn random_code(rng: &mut Rng, taken: &[Lobby]) -> String {
    loop {
        let code: String = (0..CODE_LEN)
            .map(|_| (b'A' + rng.range(0, 26) as u8) as char)
            .collect();
        if taken.iter().all(|l| l.code != code) {
            return code;
        }
    }
}

/// `grimgobber --serve [port]`: the door on `port`, its lobbies above it. Never returns.
pub fn serve(port: u16) -> ! {
    let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, port))
        .unwrap_or_else(|e| panic!("cannot bind UDP port {port}: {e}"));
    socket
        .set_read_timeout(Some(Duration::from_secs(1)))
        .expect("a read timeout");
    println!("the door is open on UDP port {port}");
    let seed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(1, |d| d.as_nanos() as u64);
    let mut rng = Rng::new(seed);
    let mut lobbies: Vec<Lobby> = Vec::new();
    let mut buf = [0u8; 128];
    loop {
        // A lobby gone: SCARE comes back, any other is let go.
        lobbies.retain_mut(|l| match l.child.try_wait() {
            Ok(None) => true,
            _ => {
                println!("lobby {} closed", l.code);
                false
            }
        });
        if lobbies.iter().all(|l| l.code != FIRST) {
            match open(FIRST, port + 1) {
                Ok(child) => lobbies.push(Lobby {
                    code: FIRST.into(),
                    port: port + 1,
                    child,
                }),
                Err(e) => eprintln!("cannot open lobby {FIRST}: {e}"),
            }
        }
        let Ok((n, from)) = socket.recv_from(&mut buf) else {
            continue;
        };
        let ask = String::from_utf8_lossy(&buf[..n]);
        let words: Vec<&str> = ask.split_whitespace().collect();
        let answer = match words.as_slice() {
            ["GRIM", p, ..] if *p != PROTOCOL.to_string() => "OLD".to_string(),
            ["GRIM", _, "JOIN", code] => lobbies
                .iter()
                .find(|l| l.code.eq_ignore_ascii_case(code))
                .map_or("NONE".into(), |l| format!("LOBBY {} {}", l.code, l.port)),
            ["GRIM", _, "HOST"] => {
                let free =
                    (port + 1..=port + LOBBIES).find(|p| lobbies.iter().all(|l| l.port != *p));
                match free {
                    Some(p) => {
                        let code = random_code(&mut rng, &lobbies);
                        match open(&code, p) {
                            Ok(child) => {
                                let answer = format!("LOBBY {code} {p}");
                                lobbies.push(Lobby {
                                    code,
                                    port: p,
                                    child,
                                });
                                answer
                            }
                            Err(_) => "FULL".into(),
                        }
                    }
                    None => "FULL".into(),
                }
            }
            _ => continue,
        };
        let _ = socket.send_to(answer.as_bytes(), from);
    }
}

/// What the door said.
pub enum Answer {
    Lobby(String, SocketAddr),
    None,
    Full,
    Old,
}

/// A question to the door, waited on frame by frame.
pub struct Asking {
    socket: UdpSocket,
    door: SocketAddr,
    ask: String,
    waited: f32,
    sent: f32,
}

impl Asking {
    /// To join `code`, or with `None` to open a new lobby.
    pub fn new(door: &str, code: Option<&str>) -> Result<Self, String> {
        let door = door
            .to_socket_addrs()
            .ok()
            .and_then(|mut a| a.find(SocketAddr::is_ipv4))
            .ok_or_else(|| format!("cannot find {door}"))?;
        let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).map_err(|e| e.to_string())?;
        socket.set_nonblocking(true).map_err(|e| e.to_string())?;
        let ask = match code {
            Some(c) => format!("GRIM {PROTOCOL} JOIN {c}"),
            None => format!("GRIM {PROTOCOL} HOST"),
        };
        Ok(Asking {
            socket,
            door,
            ask,
            waited: 0.0,
            sent: f32::INFINITY,
        })
    }

    /// Asking again now and then; the answer once there is one, or
    /// `Err` once it has waited too long.
    pub fn poll(&mut self, dt: f32) -> Option<Result<Answer, String>> {
        self.waited += dt;
        self.sent += dt;
        if self.sent >= AGAIN {
            self.sent = 0.0;
            let _ = self.socket.send_to(self.ask.as_bytes(), self.door);
        }
        let mut buf = [0u8; 128];
        if let Ok((n, _)) = self.socket.recv_from(&mut buf) {
            let text = String::from_utf8_lossy(&buf[..n]);
            let words: Vec<&str> = text.split_whitespace().collect();
            return Some(Ok(match words.as_slice() {
                ["LOBBY", code, port] => {
                    let port = port.parse().unwrap_or(PORT + 1);
                    Answer::Lobby(code.to_string(), SocketAddr::new(self.door.ip(), port))
                }
                ["FULL"] => Answer::Full,
                ["OLD"] => Answer::Old,
                _ => Answer::None,
            }));
        }
        (self.waited >= WAIT).then(|| Err(format!("no answer from {}", self.door)))
    }
}
