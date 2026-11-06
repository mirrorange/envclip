//! The envclip agent: a background process that holds opened variables in
//! memory and serves them over a private Unix socket. Nothing touches disk.

use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
use std::os::unix::net::{UnixListener, UnixStream};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use zeroize::Zeroize;

use crate::crypto::Vars;
use crate::ui::now;

const MAX_LINE: u64 = 4 << 20;

#[derive(Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "lowercase")]
pub enum Req {
    Put { vars: Vars },
    Get {
        #[serde(default)]
        names: Vec<String>,
    },
    List,
    Clear,
}

#[derive(Serialize, Deserialize, Default)]
pub struct Resp {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vars: Option<Vars>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub names: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pid: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<u64>,
}

pub fn socket_path() -> PathBuf {
    if let Some(p) = std::env::var_os("ENVCLIP_SOCK").filter(|p| !p.is_empty()) {
        return p.into();
    }
    let dir = std::env::var_os("XDG_RUNTIME_DIR")
        .filter(|p| !p.is_empty())
        .map(|p| PathBuf::from(p).join("envclip"))
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".envclip")))
        .unwrap_or_else(|| std::env::temp_dir().join("envclip"));
    dir.join("agent.sock")
}

// ── client side ────────────────────────────────────────────────────────────

pub fn request(sock: &Path, req: &Req) -> std::io::Result<Resp> {
    let mut s = UnixStream::connect(sock)?;
    s.set_read_timeout(Some(Duration::from_secs(10)))?;
    let mut line = serde_json::to_vec(req)?;
    line.push(b'\n');
    s.write_all(&line)?;
    line.zeroize();
    let mut buf = String::new();
    BufReader::new(s.take(MAX_LINE)).read_line(&mut buf)?;
    let resp = serde_json::from_str(&buf).map_err(std::io::Error::other);
    buf.zeroize();
    resp
}

pub fn is_running(sock: &Path) -> bool {
    UnixStream::connect(sock).is_ok()
}

/// Start a detached agent unless one is already listening.
pub fn ensure(sock: &Path, hold: u64) -> std::io::Result<()> {
    if is_running(sock) {
        return Ok(());
    }
    let exe = std::env::current_exe()?;
    Command::new(exe)
        .arg("__agent")
        .arg(hold.to_string())
        .env("ENVCLIP_SOCK", sock)
        .current_dir("/")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn()?;
    for _ in 0..100 {
        if is_running(sock) {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(30));
    }
    Err(std::io::Error::other("agent did not start"))
}

// ── server side ────────────────────────────────────────────────────────────

struct Store {
    vars: Vars,
}

impl Store {
    fn wipe(&mut self) {
        for (n, v) in self.vars.iter_mut() {
            n.zeroize();
            v.zeroize();
        }
        self.vars.clear();
    }
}

fn shutdown(sock: &Path, store: &Mutex<Store>) -> ! {
    if let Ok(mut s) = store.lock() {
        s.wipe();
    }
    let _ = fs::remove_file(sock);
    std::process::exit(0)
}

fn bind(sock: &Path) -> std::io::Result<UnixListener> {
    if let Some(dir) = sock.parent() {
        fs::DirBuilder::new().recursive(true).mode(0o700).create(dir)?;
        fs::set_permissions(dir, fs::Permissions::from_mode(0o700))?;
    }
    if sock.exists() {
        if is_running(sock) {
            std::process::exit(0); // someone else won the race
        }
        fs::remove_file(sock)?;
    }
    let l = UnixListener::bind(sock)?;
    fs::set_permissions(sock, fs::Permissions::from_mode(0o600))?;
    Ok(l)
}

pub fn serve(hold: u64) -> ! {
    let sock = socket_path();
    let Ok(listener) = bind(&sock) else { std::process::exit(1) };
    let expires_at = now() + hold;
    let store = Arc::new(Mutex::new(Store { vars: Vars::new() }));

    {
        let (sock, store) = (sock.clone(), store.clone());
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_secs(hold));
            shutdown(&sock, &store);
        });
    }

    for conn in listener.incoming() {
        let Ok(mut conn) = conn else { continue };
        let _ = conn.set_read_timeout(Some(Duration::from_secs(5)));
        let mut line = String::new();
        if BufReader::new((&conn).take(MAX_LINE)).read_line(&mut line).is_err() {
            continue;
        }
        let parsed = serde_json::from_str::<Req>(&line);
        line.zeroize();
        let mut clear = false;
        let resp = match parsed {
            Err(e) => Resp { error: Some(e.to_string()), ..Default::default() },
            Ok(req) => {
                let mut st = store.lock().unwrap_or_else(|e| e.into_inner());
                match req {
                    Req::Put { vars } => {
                        for (n, v) in vars {
                            match st.vars.iter_mut().find(|(k, _)| *k == n) {
                                Some(slot) => {
                                    slot.1.zeroize();
                                    slot.1 = v;
                                }
                                None => st.vars.push((n, v)),
                            }
                        }
                        Resp { ok: true, ..Default::default() }
                    }
                    Req::Get { names } => {
                        let missing: Vec<_> =
                            names.iter().filter(|n| !st.vars.iter().any(|(k, _)| k == *n)).cloned().collect();
                        if missing.is_empty() {
                            let vars = st
                                .vars
                                .iter()
                                .filter(|(k, _)| names.is_empty() || names.contains(k))
                                .cloned()
                                .collect();
                            Resp { ok: true, vars: Some(vars), ..Default::default() }
                        } else {
                            Resp { names: Some(missing), error: Some("missing".into()), ..Default::default() }
                        }
                    }
                    Req::List => Resp {
                        ok: true,
                        names: Some(st.vars.iter().map(|(k, _)| k.clone()).collect()),
                        pid: Some(std::process::id()),
                        expires_at: Some(expires_at),
                        ..Default::default()
                    },
                    Req::Clear => {
                        clear = true;
                        Resp { ok: true, ..Default::default() }
                    }
                }
            }
        };
        if let Ok(mut out) = serde_json::to_vec(&resp) {
            out.push(b'\n');
            let _ = conn.write_all(&out);
            out.zeroize();
        }
        if let Some(mut vars) = resp.vars {
            for (_, v) in vars.iter_mut() {
                v.zeroize();
            }
        }
        if clear {
            drop(conn);
            shutdown(&sock, &store);
        }
    }
    shutdown(&sock, &store)
}
