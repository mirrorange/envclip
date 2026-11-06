//! The in-memory agent: a tiny background process that holds decrypted
//! variables and serves them over a user-private Unix socket.
//!
//! Nothing is ever written to disk. The agent exits (and wipes memory) when
//! its hold time elapses or `envclip clear` is run.

use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt};
use std::os::unix::net::{UnixListener, UnixStream};
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use zeroize::Zeroizing;

use crate::dotenv::Var;
use crate::i18n::tr;

const MAX_MSG: u64 = 4 << 20;

struct Entry {
    name: String,
    value: Zeroizing<String>,
    src: String,
    at: u64,
}

struct State {
    entries: Vec<Entry>,
    deadline: Instant,
}

impl State {
    fn wipe(&mut self) {
        // `Zeroizing` scrubs each value as it is dropped.
        self.entries.clear();
    }
}

fn uid() -> u32 {
    unsafe { libc::getuid() }
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// `$ENVCLIP_SOCK`, else `$XDG_RUNTIME_DIR/envclip/agent.sock`,
/// else `$TMPDIR/envclip-<uid>/agent.sock`.
pub fn socket_path() -> Result<PathBuf, String> {
    if let Some(p) = std::env::var_os("ENVCLIP_SOCK").filter(|p| !p.is_empty()) {
        return Ok(PathBuf::from(p));
    }
    let dir = match std::env::var_os("XDG_RUNTIME_DIR").filter(|p| !p.is_empty()) {
        Some(d) => PathBuf::from(d).join("envclip"),
        None => std::env::temp_dir().join(format!("envclip-{}", uid())),
    };
    if !dir.exists() {
        std::fs::DirBuilder::new()
            .mode(0o700)
            .recursive(true)
            .create(&dir)
            .map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    let md = std::fs::symlink_metadata(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    if !md.is_dir() || md.uid() != uid() || md.mode() & 0o077 != 0 {
        return Err(crate::l!(
            "refusing to use {}: must be a directory owned by you with mode 0700",
            "拒绝使用 {}：必须是归你所有且权限为 0700 的目录",
            dir.display()
        ));
    }
    Ok(dir.join("agent.sock"))
}

/// Uid of the process at the other end of the socket.
fn peer_uid(s: &UnixStream) -> Option<u32> {
    use std::os::fd::AsRawFd;
    let fd = s.as_raw_fd();
    #[cfg(any(target_os = "linux", target_os = "android"))]
    unsafe {
        let mut cred: libc::ucred = std::mem::zeroed();
        let mut len = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
        let r = libc::getsockopt(
            fd,
            libc::SOL_SOCKET,
            libc::SO_PEERCRED,
            &mut cred as *mut _ as *mut libc::c_void,
            &mut len,
        );
        (r == 0).then_some(cred.uid)
    }
    #[cfg(not(any(target_os = "linux", target_os = "android")))]
    unsafe {
        let (mut u, mut g) = (0, 0);
        (libc::getpeereid(fd, &mut u, &mut g) == 0).then_some(u)
    }
}

// ───────────────────────────── client side ─────────────────────────────

pub fn request(req: &Value) -> Result<Value, String> {
    let path = socket_path()?;
    let mut s = UnixStream::connect(&path).map_err(|_| {
        tr(
            "no envclip agent is running — claim a drop first: envclip claim <link>",
            "envclip agent 未运行 —— 请先领取：envclip claim <链接>",
        )
        .to_string()
    })?;
    s.set_read_timeout(Some(Duration::from_secs(10))).ok();
    send(&mut s, req)?;
    let resp = recv(&mut s)?.ok_or("agent closed the connection")?;
    if resp["ok"].as_bool() == Some(true) {
        Ok(resp)
    } else {
        Err(resp["error"].as_str().unwrap_or("agent error").to_string())
    }
}

fn send(s: &mut UnixStream, v: &Value) -> Result<(), String> {
    let mut buf = Zeroizing::new(serde_json::to_vec(v).map_err(|e| e.to_string())?);
    buf.push(b'\n');
    s.write_all(&buf).map_err(|e| e.to_string())
}

fn recv(s: &mut UnixStream) -> Result<Option<Value>, String> {
    let mut line = Zeroizing::new(Vec::new());
    BufReader::new(s.take(MAX_MSG)).read_until(b'\n', &mut line).map_err(|e| e.to_string())?;
    if line.is_empty() {
        return Ok(None);
    }
    serde_json::from_slice(&line).map(Some).map_err(|e| e.to_string())
}

fn put_request(vars: &[Var], src: &str, hold: Duration) -> Value {
    let env: Vec<Value> = vars.iter().map(|(k, v)| json!([k, v.as_str()])).collect();
    json!({ "op": "put", "env": env, "src": src, "hold": hold.as_secs() })
}

/// Hands variables to the running agent, starting one if needed.
pub fn put(vars: &[Var], src: &str, hold: Duration) -> Result<(), String> {
    let req = put_request(vars, src, hold);
    let path = socket_path()?;
    if UnixStream::connect(&path).is_ok() {
        return request(&req).map(|_| ());
    }
    let _ = std::fs::remove_file(&path); // stale socket from a killed agent

    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let mut cmd = Command::new(exe);
    cmd.arg("__agent").stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null());
    unsafe {
        cmd.pre_exec(|| {
            libc::setsid();
            Ok(())
        });
    }
    let mut child = cmd.spawn().map_err(|e| e.to_string())?;
    {
        let mut stdin = child.stdin.take().expect("piped stdin");
        let mut buf = Zeroizing::new(serde_json::to_vec(&req).map_err(|e| e.to_string())?);
        buf.push(b'\n');
        stdin.write_all(&buf).map_err(|e| e.to_string())?;
    }
    let mut ready = String::new();
    BufReader::new(child.stdout.take().expect("piped stdout"))
        .read_line(&mut ready)
        .map_err(|e| e.to_string())?;
    match ready.trim() {
        "ok" => Ok(()),
        "" => Err(tr("agent failed to start", "agent 启动失败").to_string()),
        e => Err(e.to_string()),
    }
}

// ───────────────────────────── agent side ─────────────────────────────

fn harden() {
    unsafe {
        let zero = libc::rlimit { rlim_cur: 0, rlim_max: 0 };
        libc::setrlimit(libc::RLIMIT_CORE, &zero);
        #[cfg(any(target_os = "linux", target_os = "android"))]
        libc::prctl(libc::PR_SET_DUMPABLE, 0, 0, 0, 0);
    }
}

fn detach_stdio() {
    unsafe {
        let fd = libc::open(c"/dev/null".as_ptr(), libc::O_RDWR);
        if fd >= 0 {
            libc::dup2(fd, 0);
            libc::dup2(fd, 1);
            libc::dup2(fd, 2);
            if fd > 2 {
                libc::close(fd);
            }
        }
    }
}

/// Entry point of the hidden `__agent` subcommand.
pub fn serve() -> ! {
    harden();
    let fail = |msg: String| -> ! {
        println!("{msg}");
        std::process::exit(1)
    };
    let path = socket_path().unwrap_or_else(|e| fail(e));
    let listener = UnixListener::bind(&path).unwrap_or_else(|e| fail(format!("{}: {e}", path.display())));
    let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));

    let state = Arc::new(Mutex::new(State { entries: Vec::new(), deadline: Instant::now() }));

    // The first request arrives on stdin from the spawning `claim`.
    let mut first = Zeroizing::new(String::new());
    std::io::stdin().read_line(&mut first).ok();
    match serde_json::from_str::<Value>(&first) {
        Ok(req) => {
            handle(&state, &req);
        }
        Err(e) => {
            let _ = std::fs::remove_file(&path);
            fail(e.to_string());
        }
    }
    println!("ok");
    let _ = std::io::stdout().flush();
    detach_stdio();

    let timer_state = state.clone();
    let timer_path = path.clone();
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(Duration::from_secs(1));
            let mut st = timer_state.lock().unwrap_or_else(|p| p.into_inner());
            if Instant::now() >= st.deadline || st.entries.is_empty() {
                st.wipe();
                let _ = std::fs::remove_file(&timer_path);
                std::process::exit(0);
            }
        }
    });

    for conn in listener.incoming() {
        let Ok(mut conn) = conn else { continue };
        if peer_uid(&conn) != Some(uid()) {
            continue;
        }
        conn.set_read_timeout(Some(Duration::from_secs(5))).ok();
        let req = match recv(&mut conn) {
            Ok(Some(r)) => r,
            _ => continue,
        };
        let resp = handle(&state, &req);
        let _ = send(&mut conn, &resp);
        if req["op"] == "stop" {
            let _ = std::fs::remove_file(&path);
            std::process::exit(0);
        }
    }
    std::process::exit(0)
}

fn handle(state: &Mutex<State>, req: &Value) -> Value {
    let mut st = state.lock().unwrap_or_else(|p| p.into_inner());
    let names = |key: &str| -> Vec<String> {
        req[key]
            .as_array()
            .map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_string)).collect())
            .unwrap_or_default()
    };
    match req["op"].as_str().unwrap_or("") {
        "put" => {
            let src = req["src"].as_str().unwrap_or("").to_string();
            let hold = Duration::from_secs(req["hold"].as_u64().unwrap_or(43_200));
            let at = now();
            for pair in req["env"].as_array().into_iter().flatten() {
                let (Some(k), Some(v)) = (pair[0].as_str(), pair[1].as_str()) else { continue };
                let value = Zeroizing::new(v.to_string());
                match st.entries.iter_mut().find(|e| e.name == k) {
                    Some(e) => {
                        e.value = value;
                        e.src = src.clone();
                        e.at = at;
                    }
                    None => st.entries.push(Entry { name: k.to_string(), value, src: src.clone(), at }),
                }
            }
            st.deadline = st.deadline.max(Instant::now() + hold);
            json!({ "ok": true })
        }
        "list" => {
            let vars: Vec<Value> =
                st.entries.iter().map(|e| json!({ "name": e.name, "src": e.src, "at": e.at })).collect();
            let left = st.deadline.saturating_duration_since(Instant::now()).as_secs();
            json!({ "ok": true, "vars": vars, "expires_in": left })
        }
        "get" => {
            let want = names("names");
            let mut env = Vec::new();
            for n in &want {
                match st.entries.iter().find(|e| &e.name == n) {
                    Some(e) => env.push(json!([e.name, e.value.as_str()])),
                    None => {
                        return json!({ "ok": false, "error": crate::l!("{} is not loaded", "{} 未加载", n) });
                    }
                }
            }
            if want.is_empty() {
                env = st.entries.iter().map(|e| json!([e.name, e.value.as_str()])).collect();
            }
            json!({ "ok": true, "env": env })
        }
        "rm" => {
            let want = names("names");
            st.entries.retain(|e| !want.contains(&e.name));
            json!({ "ok": true })
        }
        "stop" => {
            st.wipe();
            json!({ "ok": true })
        }
        _ => json!({ "ok": false, "error": "unknown op" }),
    }
}
