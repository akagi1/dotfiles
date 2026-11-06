//! GlazeWM IPC client: a minimal WebSocket connection to 127.0.0.1:6123 that
//! subscribes to workspace events and re-queries the monitors after each one.

use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::sync::Mutex;
use std::thread;
use std::time::Duration;

use serde_json::Value;

#[derive(Clone)]
pub struct Workspace {
    pub name: String,
    pub label: String,
    pub focused: bool,
    pub displayed: bool,
}

pub struct Monitor {
    /// The monitor's HMONITOR.
    pub handle: isize,
    pub x: i32,
    pub y: i32,
    pub workspaces: Vec<Workspace>,
}

/// Latest monitors and their workspaces; empty while GlazeWM is not running.
pub static MONITORS: Mutex<Vec<Monitor>> = Mutex::new(Vec::new());
static WRITER: Mutex<Option<TcpStream>> = Mutex::new(None);

const ADDR: &str = "127.0.0.1:6123";
const SUBSCRIBE: &str = "sub -e focus_changed workspace_activated workspace_deactivated \
                         workspace_updated monitor_added monitor_updated monitor_removed";
const QUERY: &str = "query monitors";

const OP_TEXT: u8 = 0x1;
const OP_CLOSE: u8 = 0x8;
const OP_PING: u8 = 0x9;
const OP_PONG: u8 = 0xA;

/// Keeps a connection open on a background thread, reconnecting every 2 s
/// while GlazeWM is not running. `changed` runs after every update.
pub fn spawn(changed: impl Fn() + Send + 'static) {
    thread::spawn(move || loop {
        let _ = run(&changed);
        *WRITER.lock().unwrap() = None;
        if !std::mem::take(&mut *MONITORS.lock().unwrap()).is_empty() {
            changed();
        }
        thread::sleep(Duration::from_secs(2));
    });
}

/// Runs a GlazeWM command such as `focus --workspace 2`.
pub fn command(cmd: &str) {
    let _ = send(OP_TEXT, format!("command {cmd}").as_bytes());
}

fn run(changed: &impl Fn()) -> io::Result<()> {
    let stream = TcpStream::connect(ADDR)?;
    stream.set_nodelay(true)?;
    let mut writer = stream.try_clone()?;
    writer.write_all(
        b"GET / HTTP/1.1\r\nHost: 127.0.0.1:6123\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n\
          Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\n\r\n",
    )?;
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader.read_line(&mut line)?;
    if !line.contains(" 101 ") {
        return Err(io::Error::other("websocket handshake refused"));
    }
    loop {
        line.clear();
        if reader.read_line(&mut line)? == 0 {
            return Err(io::ErrorKind::UnexpectedEof.into());
        }
        if line.trim().is_empty() {
            break;
        }
    }
    *WRITER.lock().unwrap() = Some(writer);

    send(OP_TEXT, SUBSCRIBE.as_bytes())?;
    send(OP_TEXT, QUERY.as_bytes())?;
    // Events come in bursts; keep at most one query in flight.
    let mut pending = true;
    let mut dirty = false;
    loop {
        let Ok(msg) = serde_json::from_slice::<Value>(&read_message(&mut reader)?) else { continue };
        match msg["messageType"].as_str() {
            Some("event_subscription") if pending => dirty = true,
            Some("event_subscription") => {
                send(OP_TEXT, QUERY.as_bytes())?;
                pending = true;
            }
            Some("client_response") if msg["clientMessage"] == QUERY => {
                *MONITORS.lock().unwrap() = parse(&msg["data"]["monitors"]);
                changed();
                pending = dirty;
                if dirty {
                    send(OP_TEXT, QUERY.as_bytes())?;
                    dirty = false;
                }
            }
            _ => {}
        }
    }
}

fn parse(monitors: &Value) -> Vec<Monitor> {
    let list = |v: &Value| v.as_array().cloned().unwrap_or_default();
    list(monitors)
        .iter()
        .map(|m| Monitor {
            handle: m["handle"].as_i64().unwrap_or(0) as isize,
            x: m["x"].as_i64().unwrap_or(0) as i32,
            y: m["y"].as_i64().unwrap_or(0) as i32,
            workspaces: list(&m["children"])
                .iter()
                .map(|w| {
                    let name = w["name"].as_str().unwrap_or_default().to_string();
                    Workspace {
                        label: w["displayName"].as_str().unwrap_or(&name).to_string(),
                        name,
                        focused: w["hasFocus"].as_bool().unwrap_or(false),
                        displayed: w["isDisplayed"].as_bool().unwrap_or(false),
                    }
                })
                .collect(),
        })
        .collect()
}

/// Reads one complete (possibly fragmented) data message, answering pings.
fn read_message(r: &mut impl Read) -> io::Result<Vec<u8>> {
    let mut msg = Vec::new();
    loop {
        let mut head = [0u8; 2];
        r.read_exact(&mut head)?;
        let fin = head[0] & 0x80 != 0;
        let op = head[0] & 0x0F;
        let mut len = (head[1] & 0x7F) as u64;
        if len == 126 {
            let mut b = [0u8; 2];
            r.read_exact(&mut b)?;
            len = u16::from_be_bytes(b) as u64;
        } else if len == 127 {
            let mut b = [0u8; 8];
            r.read_exact(&mut b)?;
            len = u64::from_be_bytes(b);
        }
        let mut mask = [0u8; 4];
        if head[1] & 0x80 != 0 {
            r.read_exact(&mut mask)?;
        }
        let mut payload = vec![0u8; len as usize];
        r.read_exact(&mut payload)?;
        payload.iter_mut().enumerate().for_each(|(i, b)| *b ^= mask[i % 4]);
        match op {
            OP_CLOSE => return Err(io::ErrorKind::ConnectionAborted.into()),
            OP_PING => send(OP_PONG, &payload)?,
            OP_PONG => {}
            _ => {
                msg.extend(payload);
                if fin {
                    return Ok(msg);
                }
            }
        }
    }
}

/// Sends one frame. Client frames must be masked; an all-zero mask leaves the
/// payload as is, which is fine for a local connection.
fn send(op: u8, payload: &[u8]) -> io::Result<()> {
    let mut frame = vec![0x80 | op];
    match payload.len() {
        n @ 0..=125 => frame.push(0x80 | n as u8),
        n @ 126..=0xFFFF => {
            frame.push(0x80 | 126);
            frame.extend((n as u16).to_be_bytes());
        }
        n => {
            frame.push(0x80 | 127);
            frame.extend((n as u64).to_be_bytes());
        }
    }
    frame.extend([0u8; 4]);
    frame.extend(payload);
    match WRITER.lock().unwrap().as_mut() {
        Some(w) => w.write_all(&frame),
        None => Err(io::ErrorKind::NotConnected.into()),
    }
}
