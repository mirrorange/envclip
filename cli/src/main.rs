#[macro_use]
mod i18n;
mod agent;
mod api;
mod crypto;
mod dotenv;
mod ui;

use std::io::{BufRead, IsTerminal, Read, Write};
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Command, ExitCode};

use zeroize::Zeroize;

use agent::{Req, Resp};
use api::{ApiError, Client};
use crypto::{Key, Vars};
use ui::{human_secs, now, parse_duration, sh_quote, Paint};

const VERSION: &str = env!("CARGO_PKG_VERSION");
const DEFAULT_TTL: u64 = 3600;
const DEFAULT_HOLD: u64 = 12 * 3600;
const MIN_TTL: u64 = 60;
const MAX_TTL: u64 = 7 * 86400;

type R<T = ()> = Result<T, String>;

// ── arguments ──────────────────────────────────────────────────────────────

const VALUE_FLAGS: &[&str] = &["--server", "--lang", "--ttl", "--key", "--hold", "--file", "-f"];

#[derive(Default)]
struct Args {
    cmd: String,
    pos: Vec<String>,
    flags: Vec<(String, String)>,
    help: bool,
    rest: Option<Vec<String>>,
}

impl Args {
    fn parse(argv: Vec<String>) -> R<Self> {
        let mut a = Args::default();
        let mut it = argv.into_iter();
        while let Some(arg) = it.next() {
            if arg == "--" {
                a.rest = Some(it.by_ref().collect());
                break;
            }
            if arg == "-h" || arg == "--help" {
                a.help = true;
            } else if arg == "-V" || arg == "--version" {
                a.cmd = "version".into();
            } else if arg.starts_with('-') && arg.len() > 1 {
                let (name, inline) = match arg.split_once('=') {
                    Some((n, v)) => (n.to_string(), Some(v.to_string())),
                    None => (arg.clone(), None),
                };
                if !VALUE_FLAGS.contains(&name.as_str()) {
                    return Err(t!("unknown option {}", "未知选项 {}", name));
                }
                let name = if name == "-f" { "--file".to_string() } else { name };
                let value = match inline {
                    Some(v) => v,
                    None => it.next().ok_or_else(|| t!("{} needs a value", "{} 需要一个值", name))?,
                };
                a.flags.push((name, value));
            } else if a.cmd.is_empty() {
                a.cmd = arg;
            } else {
                a.pos.push(arg);
            }
        }
        Ok(a)
    }

    fn flag(&self, name: &str) -> Option<&str> {
        self.flags.iter().rev().find(|(n, _)| n == name).map(|(_, v)| v.as_str())
    }

    fn only(&self, allowed: &[&str]) -> R {
        for (n, _) in &self.flags {
            if n != "--lang" && n != "--server" && !allowed.contains(&n.as_str()) {
                return Err(t!("{} does not take {}", "{} 不接受 {}", self.cmd, n));
            }
        }
        Ok(())
    }

    fn duration(&self, name: &str, default: u64) -> R<u64> {
        match self.flag(name) {
            None => Ok(default),
            Some(s) => parse_duration(s).ok_or_else(|| t!("bad duration {}", "时长无效 {}", s)),
        }
    }
}

// ── server and link resolution ─────────────────────────────────────────────

fn config_dir() -> Option<PathBuf> {
    std::env::var_os("XDG_CONFIG_HOME")
        .filter(|p| !p.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .map(|p| p.join("envclip"))
}

fn server(a: &Args) -> R<String> {
    let from_file = || {
        let s = std::fs::read_to_string(config_dir()?.join("server")).ok()?;
        Some(s.trim().to_string()).filter(|s| !s.is_empty())
    };
    a.flag("--server")
        .map(str::to_string)
        .or_else(|| std::env::var("ENVCLIP_SERVER").ok().filter(|s| !s.is_empty()))
        .or_else(from_file)
        .or_else(|| option_env!("ENVCLIP_DEFAULT_SERVER").map(str::to_string))
        .map(|s| s.trim_end_matches('/').to_string())
        .ok_or_else(|| {
            t!(
                "no server configured. use --server URL or set ENVCLIP_SERVER",
                "未配置服务端。请使用 --server URL 或设置 ENVCLIP_SERVER"
            )
        })
}

/// `https://host/s/<id>` or a bare `<id>` → (server, id)
fn target(a: &Args) -> R<(String, String)> {
    let raw = a.pos.first().ok_or_else(|| t!("missing link or id", "缺少链接或 id"))?;
    let raw = raw.split(['#', '?']).next().unwrap_or_default().trim_end_matches('/');
    let (base, id) = match raw.rsplit_once("/s/") {
        Some((base, id)) if base.starts_with("https://") || base.starts_with("http://") => {
            (a.flag("--server").map_or_else(|| base.to_string(), str::to_string), id)
        }
        _ => (server(a)?, raw),
    };
    let id = id.to_ascii_lowercase();
    if !crypto::is_id(&id) {
        return Err(t!("bad clip id {}", "clip id 无效 {}", id));
    }
    Ok((base, id))
}

fn read_key(a: &Args) -> R<Key> {
    let mut raw = match a.flag("--key") {
        Some(k) => k.to_string(),
        None if !std::io::stdin().is_terminal() => {
            let mut s = String::new();
            std::io::stdin().lock().read_line(&mut s).map_err(|e| e.to_string())?;
            s
        }
        None => rpassword::prompt_password(t!("key › ", "密钥 › ")).map_err(|e| e.to_string())?,
    };
    let key = Key::parse(&raw).map_err(|_| t!("bad key", "密钥格式错误"));
    raw.zeroize();
    key
}

// ── push ───────────────────────────────────────────────────────────────────

fn prompt_vars() -> R<Vars> {
    let p = Paint::stderr();
    eprintln!(
        "{}",
        p.dim(&t!("enter variables · values are hidden · empty name to finish", "输入变量 · 值不回显 · 变量名留空结束"))
    );
    let mut vars = Vars::new();
    loop {
        eprint!("  {} ", p.green(&t!("name  ›", "变量名 ›")));
        let _ = std::io::stderr().flush();
        let mut name = String::new();
        if std::io::stdin().lock().read_line(&mut name).map_err(|e| e.to_string())? == 0 {
            break;
        }
        let name = name.trim().to_string();
        if name.is_empty() {
            break;
        }
        if !crypto::is_name(&name) {
            eprintln!("  {}", p.red(&t!("bad name {}", "变量名无效 {}", name)));
            continue;
        }
        let value = rpassword::prompt_password(format!("  {} ", p.green(&t!("value ›", "值     ›"))))
            .map_err(|e| e.to_string())?;
        vars.push((name, value));
    }
    Ok(vars)
}

fn agent_prompt(cmd: &str) -> String {
    t!(
        "I shared some environment variables with you through envclip (end-to-end encrypted, burn after reading). Load them with:

  {}

envclip keeps them in memory. Never print, log or commit their values. Use them like this:

  envclip run -- <command>
  eval \"$(envclip env)\" && <command>

`envclip ls` lists the names; `envclip clear` wipes them when you are done.",
        "我通过 envclip 向你分享了一些环境变量（端到端加密，阅后即焚）。请运行以下命令加载：

  {}

envclip 会把它们保存在内存中。不要打印、记录或提交这些值。这样使用：

  envclip run -- <命令>
  eval \"$(envclip env)\" && <命令>

`envclip ls` 列出变量名；用完后运行 `envclip clear` 清除。",
        cmd
    )
}

fn push(a: &Args) -> R {
    a.only(&["--ttl", "--file"])?;
    let ttl = a.duration("--ttl", DEFAULT_TTL)?;
    if !(MIN_TTL..=MAX_TTL).contains(&ttl) {
        return Err(t!("--ttl must be between 1m and 7d", "--ttl 须在 1m 到 7d 之间"));
    }
    let base = server(a)?;

    let mut vars = Vars::new();
    for name in &a.pos {
        if !crypto::is_name(name) {
            return Err(t!("bad name {}", "变量名无效 {}", name));
        }
        match std::env::var(name) {
            Ok(v) => vars.push((name.clone(), v)),
            Err(std::env::VarError::NotPresent) => {
                return Err(t!("{} is not set in this shell", "当前 shell 中未设置 {}", name))
            }
            Err(_) => return Err(t!("{} is not valid UTF-8", "{} 不是有效的 UTF-8", name)),
        }
    }
    let read_dotenv = |src: &str| -> R<Vars> {
        let mut text = String::new();
        if src == "-" {
            std::io::stdin().read_to_string(&mut text).map_err(|e| e.to_string())?;
        } else {
            text = std::fs::read_to_string(src).map_err(|e| format!("{src}: {e}"))?;
        }
        let parsed = dotenv::parse(&text);
        text.zeroize();
        parsed
    };
    if let Some(f) = a.flag("--file") {
        vars.extend(read_dotenv(f)?);
    }
    if a.pos.is_empty() && a.flag("--file").is_none() {
        vars = if std::io::stdin().is_terminal() { prompt_vars()? } else { read_dotenv("-")? };
    }
    if vars.is_empty() {
        return Err(t!("nothing to send", "没有要发送的变量"));
    }
    for (i, (n, v)) in vars.iter().enumerate() {
        if vars[..i].iter().any(|(m, _)| m == n) {
            return Err(t!("duplicate name {}", "变量名重复 {}", n));
        }
        if v.contains('\0') {
            return Err(t!("{} contains a NUL byte", "{} 含有 NUL 字节", n));
        }
    }

    let client = Client::new(&base);
    let (id, key, expires_at) = loop {
        let (id, key) = (crypto::new_id(), Key::generate());
        let blob = crypto::seal(&id, &key, &vars);
        match client.create(&id, &key.auth_hash(), &blob, ttl) {
            Ok(exp) => break (id, key, exp),
            Err(ApiError::Conflict) => continue,
            Err(e) => return Err(e.describe(client.base())),
        }
    };
    for (_, v) in vars.iter_mut() {
        v.zeroize();
    }

    let link = format!("{}/s/{}", client.base(), id);
    let key = key.encode();
    let p = Paint::stderr();
    eprintln!(
        "{} {}",
        p.green("✓"),
        t!(
            "sealed {} var(s) · expires in {}",
            "已封存 {} 个变量 · {} 后过期",
            vars.len(),
            human_secs(expires_at.saturating_sub(now()))
        )
    );
    eprintln!();
    eprintln!("  {}  {}", p.dim(&t!("link", "链接")), link);
    eprintln!("  {}  {}", p.dim(&t!("key ", "密钥")), key);
    eprintln!();
    eprintln!("{}", p.dim(&t!("── paste this to your agent ──", "── 发送给 Agent ──")));
    eprintln!();
    println!("{}", agent_prompt(&format!("curl -fsSL {link} | sh -s -- {key}")));
    Ok(())
}

// ── open / revoke ──────────────────────────────────────────────────────────

fn agent_err(e: std::io::Error) -> String {
    t!("envclip agent: {}", "envclip agent：{}", e)
}

fn open(a: &Args) -> R {
    a.only(&["--key", "--hold"])?;
    let hold = a.duration("--hold", DEFAULT_HOLD)?;
    if hold < 60 {
        return Err(t!("--hold must be at least 1m", "--hold 至少为 1m"));
    }
    let (base, id) = target(a)?;
    let key = read_key(a)?;
    let client = Client::new(&base);
    let blob = client.open(&id, &key.auth_token()).map_err(|e| e.describe(client.base()))?;
    let vars = crypto::unseal(&id, &key, &blob).map_err(|_| {
        t!(
            "decryption failed; the clip is burned. ask the sender for a new one",
            "解密失败，该 clip 已销毁。请让发送者重新创建"
        )
    })?;
    let names: Vec<String> = vars.iter().map(|(n, _)| n.clone()).collect();

    let sock = agent::socket_path();
    agent::ensure(&sock, hold).map_err(agent_err)?;
    let r = agent::request(&sock, &Req::Put { vars }).map_err(agent_err)?;
    if !r.ok {
        return Err(agent_err(std::io::Error::other(r.error.unwrap_or_default())));
    }
    let info = agent::request(&sock, &Req::List).map_err(agent_err)?;

    let p = Paint::stderr();
    eprintln!(
        "{} {}",
        p.green("✓"),
        t!("opened {} · burned on the server", "已打开 {} · 服务端已销毁", id)
    );
    let width = names.iter().map(String::len).max().unwrap_or(0);
    for n in &names {
        eprintln!("  {}{}  {}", p.bold(n), " ".repeat(width - n.len()), p.dim("••••••••"));
    }
    eprintln!(
        "{}",
        p.dim(&t!(
            "held in memory by envclip (pid {}) for {} · values are never printed",
            "由 envclip 保存在内存中（pid {}），有效期 {} · 值不会被打印",
            info.pid.unwrap_or_default(),
            human_secs(info.expires_at.unwrap_or_default().saturating_sub(now()))
        ))
    );
    eprintln!();
    eprintln!("  {}", t!("envclip run -- <command>", "envclip run -- <命令>"));
    eprintln!("  eval \"$(envclip env)\"");
    Ok(())
}

fn revoke(a: &Args) -> R {
    a.only(&["--key"])?;
    let (base, id) = target(a)?;
    let key = read_key(a)?;
    let client = Client::new(&base);
    client.revoke(&id, &key.auth_token()).map_err(|e| e.describe(client.base()))?;
    eprintln!("{} {}", Paint::stderr().green("✓"), t!("revoked {}", "已撤销 {}", id));
    Ok(())
}

// ── using held variables ───────────────────────────────────────────────────

fn held(names: &[String]) -> R<Vars> {
    let sock = agent::socket_path();
    let r: Resp = agent::request(&sock, &Req::Get { names: names.to_vec() })
        .map_err(|_| t!("nothing held. run `envclip open` first", "当前没有保存的变量。请先运行 `envclip open`"))?;
    if !r.ok {
        let missing = r.names.unwrap_or_default().join(", ");
        return Err(t!("not held: {}", "未保存：{}", missing));
    }
    Ok(r.vars.unwrap_or_default())
}

fn env(a: &Args) -> R {
    a.only(&[])?;
    if std::io::stdout().is_terminal() {
        return Err(t!(
            "refusing to print secrets to a terminal. use: eval \"$(envclip env)\"",
            "拒绝向终端打印机密。请使用：eval \"$(envclip env)\""
        ));
    }
    let mut vars = held(&a.pos)?;
    let mut out = String::new();
    for (n, v) in &vars {
        out.push_str(&format!("export {}={}\n", n, sh_quote(v)));
    }
    let res = std::io::stdout().write_all(out.as_bytes()).map_err(|e| e.to_string());
    out.zeroize();
    for (_, v) in vars.iter_mut() {
        v.zeroize();
    }
    res
}

fn run(a: &Args) -> R {
    a.only(&[])?;
    let cmd = a.rest.as_deref().unwrap_or_default();
    let Some((prog, args)) = cmd.split_first() else {
        return Err(t!("usage: envclip run [NAME…] -- <command>", "用法：envclip run [NAME…] -- <命令>"));
    };
    let vars = held(&a.pos)?;
    let err = Command::new(prog).args(args).envs(vars).exec();
    Err(format!("{prog}: {err}"))
}

fn ls(a: &Args) -> R {
    a.only(&[])?;
    let p = Paint::stdout();
    let Ok(r) = agent::request(&agent::socket_path(), &Req::List) else {
        eprintln!("{}", t!("nothing held", "当前没有保存的变量"));
        return Ok(());
    };
    for n in r.names.unwrap_or_default() {
        println!("{}", n);
    }
    eprintln!(
        "{}",
        p.dim(&t!(
            "pid {} · wiped in {}",
            "pid {} · {} 后清除",
            r.pid.unwrap_or_default(),
            human_secs(r.expires_at.unwrap_or_default().saturating_sub(now()))
        ))
    );
    Ok(())
}

fn clear(a: &Args) -> R {
    a.only(&[])?;
    match agent::request(&agent::socket_path(), &Req::Clear) {
        Ok(_) => eprintln!("{} {}", Paint::stderr().green("✓"), t!("wiped from memory", "已从内存中清除")),
        Err(_) => eprintln!("{}", t!("nothing held", "当前没有保存的变量")),
    }
    Ok(())
}

// ── help ───────────────────────────────────────────────────────────────────

fn help() -> String {
    t!(
        "envclip {} — hand secrets to your agent, not to the chat

usage
  envclip push [NAME…] [-f FILE] [--ttl 1h]   seal variables, print a link and a key
  envclip open <LINK|ID> [--key KEY]          fetch, burn and hold variables in memory
  envclip run [NAME…] -- <command>            run a command with held variables
  envclip env [NAME…]                         emit export lines, for eval \"$(envclip env)\"
  envclip ls                                  list held names
  envclip clear                               wipe held variables and stop the agent
  envclip revoke <LINK|ID> [--key KEY]        delete a clip before it is opened

push reads NAMEs from the current shell, -f from a .env file (- for stdin),
or prompts interactively without echo. open reads the key from --key, stdin,
or a hidden prompt.

options
  --server URL    envclip server (env ENVCLIP_SERVER, or ~/.config/envclip/server)
  --ttl DUR       how long the server keeps the clip: 1m … 7d (default 1h)
  --hold DUR      how long the agent holds opened variables (default 12h)
  --lang en|zh    interface language (env ENVCLIP_LANG)

examples
  envclip push API_TOKEN DATABASE_URL
  envclip push -f .env.production --ttl 10m
  curl -fsSL https://<host>/s/<id> | sh -s -- <key>
  envclip run -- npm test",
        "envclip {} — 把机密交给 Agent，而不是交给聊天记录

用法
  envclip push [NAME…] [-f 文件] [--ttl 1h]   封存变量，输出链接与密钥
  envclip open <链接|ID> [--key 密钥]         取回、销毁密文，并在内存中保存变量
  envclip run [NAME…] -- <命令>               使用已保存的变量运行命令
  envclip env [NAME…]                         输出 export 语句，用于 eval \"$(envclip env)\"
  envclip ls                                  列出已保存的变量名
  envclip clear                               清除变量并停止后台进程
  envclip revoke <链接|ID> [--key 密钥]       在被打开前删除 clip

push 从当前 shell 读取 NAME，用 -f 读取 .env 文件（- 表示标准输入），
或以不回显的方式交互输入。open 从 --key、标准输入或隐藏提示读取密钥。

选项
  --server URL    envclip 服务端（环境变量 ENVCLIP_SERVER，或 ~/.config/envclip/server）
  --ttl 时长      服务端保留时长：1m … 7d（默认 1h）
  --hold 时长     后台进程保存变量的时长（默认 12h）
  --lang en|zh    界面语言（环境变量 ENVCLIP_LANG）

示例
  envclip push API_TOKEN DATABASE_URL
  envclip push -f .env.production --ttl 10m
  curl -fsSL https://<host>/s/<id> | sh -s -- <key>
  envclip run -- npm test",
        VERSION
    )
}

fn main() -> ExitCode {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let lang = argv
        .iter()
        .position(|a| a == "--lang")
        .and_then(|i| argv.get(i + 1).cloned())
        .or_else(|| argv.iter().find_map(|a| a.strip_prefix("--lang=").map(str::to_string)));
    i18n::init(lang.as_deref());

    let result = Args::parse(argv).and_then(|a| {
        if a.help || a.cmd.is_empty() || a.cmd == "help" {
            println!("{}", help());
            return Ok(());
        }
        match a.cmd.as_str() {
            "push" | "send" => push(&a),
            "open" | "get" => open(&a),
            "run" => run(&a),
            "env" | "export" => env(&a),
            "ls" | "list" => ls(&a),
            "clear" | "stop" => clear(&a),
            "revoke" => revoke(&a),
            "version" => {
                println!("envclip {VERSION}");
                Ok(())
            }
            "__agent" => agent::serve(a.pos.first().and_then(|s| s.parse().ok()).unwrap_or(DEFAULT_HOLD)),
            other => Err(t!("unknown command {}. see envclip --help", "未知命令 {}。参见 envclip --help", other)),
        }
    });

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{}{}", Paint::stderr().red(&t!("error: ", "错误：")), e);
            ExitCode::FAILURE
        }
    }
}
