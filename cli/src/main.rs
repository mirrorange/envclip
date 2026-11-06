//! envclip — hand secrets to an AI agent's sandbox without pasting them into chat.

mod agent;
mod api;
mod args;
mod crypto;
mod dotenv;
mod i18n;
mod ui;

use args::{Parsed, Spec};
use crypto::Secret;
use dotenv::Var;
use i18n::{tr, zh};
use serde_json::{Value, json};
use std::io::{IsTerminal, Read, Write};
use std::time::Duration;
use zeroize::Zeroizing;

type Res = Result<(), String>;

const DEFAULT_TTL: Duration = Duration::from_secs(24 * 3600);
const MAX_TTL: Duration = Duration::from_secs(7 * 24 * 3600);
const DEFAULT_HOLD: Duration = Duration::from_secs(12 * 3600);

fn main() {
    let mut argv: Vec<String> = std::env::args().skip(1).collect();
    let cmd = if argv.is_empty() { "help".to_string() } else { argv.remove(0) };
    let r = match cmd.as_str() {
        "send" | "s" => send(argv),
        "claim" | "c" => claim(argv),
        "run" | "r" => run(argv),
        "env" | "export" => env(argv),
        "get" => get(argv),
        "ls" | "list" => ls(),
        "clear" | "rm" => clear(argv),
        "status" => status(argv),
        "revoke" => revoke(argv),
        "__agent" => agent::serve(),
        "version" | "-V" | "--version" => {
            println!("envclip {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        "help" | "-h" | "--help" => {
            help();
            Ok(())
        }
        other => Err(l!(
            "unknown command `{}` — try `envclip help`",
            "未知命令 `{}` —— 试试 `envclip help`",
            other
        )),
    };
    if let Err(e) = r {
        ui::err(&e);
        std::process::exit(1);
    }
}

fn help() {
    let b = |s: &str| ui::bold(s);
    if zh() {
        println!(
            r#"{} — 把机密安全地交给 AI Agent 的云端沙盒。端到端加密 · 阅后即焚 · 无回显

{}
  envclip send [变量名...] [-f .env] [--ttl 24h]   加密并上传，输出链接与密钥
  envclip status <链接> [--key K]                 查看投递是否仍待领取
  envclip revoke <链接> [--key K]                 撤销（销毁）尚未领取的投递

{}
  envclip claim <链接> [--key K] [--hold 12h]     领取并解密到内存（服务端随即销毁）
  envclip run [变量名...] -- <命令>               以注入的环境变量运行命令
  eval "$(envclip env [变量名...])"               导出到当前 shell（输出中不含明文）
  envclip ls                                    列出已加载的变量名（不显示值）
  envclip get <变量名>                           输出单个值的明文（用于管道）
  envclip clear [变量名...]                      从内存中清除；不带参数则停止 agent

{}
  ENVCLIP_SERVER  服务地址      ENVCLIP_KEY  密钥      ENVCLIP_LANG  en | zh"#,
            b("envclip"),
            b("发送端"),
            b("Agent 端"),
            b("环境变量")
        );
    } else {
        println!(
            r#"{} — hand secrets to an AI agent's sandbox. End-to-end encrypted · burn after reading · no echo

{}
  envclip send [NAME...] [-f .env] [--ttl 24h]   encrypt, upload, print link + key
  envclip status <link> [--key K]                is the drop still waiting?
  envclip revoke <link> [--key K]                destroy an unclaimed drop

{}
  envclip claim <link> [--key K] [--hold 12h]    fetch + decrypt into memory (server copy is burned)
  envclip run [NAME...] -- <cmd>                 run a command with the variables injected
  eval "$(envclip env [NAME...])"                export into this shell (output holds no plaintext)
  envclip ls                                     list loaded names (never values)
  envclip get <NAME>                             print one raw value (for pipes)
  envclip clear [NAME...]                        forget variables; no args stops the agent

{}
  ENVCLIP_SERVER  server URL     ENVCLIP_KEY  key     ENVCLIP_LANG  en | zh"#,
            b("envclip"),
            b("Sender"),
            b("Agent side"),
            b("Environment")
        );
    }
}

// ───────────────────────────── sender ─────────────────────────────

fn send(argv: Vec<String>) -> Res {
    let p = args::parse(
        argv,
        &Spec { values: &[("file", 'f'), ("ttl", 't'), ("server", 's')], flags: &[("json", 'j')] },
    )?;
    let ttl = match p.opt("ttl") {
        Some(t) => ui::parse_duration(t).ok_or_else(|| l!("invalid --ttl `{}`", "无效的 --ttl `{}`", t))?,
        None => DEFAULT_TTL,
    };
    if ttl > MAX_TTL || ttl < Duration::from_secs(60) {
        return Err(tr("--ttl must be between 1m and 7d", "--ttl 必须介于 1m 与 7d 之间").into());
    }
    let server = p.opt("server").map(str::to_string).or_else(api::default_server).ok_or(tr(
        "no server configured — pass --server or set ENVCLIP_SERVER",
        "未配置服务地址 —— 请使用 --server 或设置 ENVCLIP_SERVER",
    ))?;

    let vars = collect_vars(&p)?;
    if vars.is_empty() {
        return Err(tr("nothing to send", "没有要发送的变量").into());
    }

    let env: Vec<Value> = vars.iter().map(|(k, v)| json!([k, v.as_str()])).collect();
    let plaintext =
        Zeroizing::new(serde_json::to_vec(&json!({ "v": 1, "env": env })).map_err(|e| e.to_string())?);
    drop(env);

    let secret = Secret::generate();
    let keys = secret.keys();
    let sealed = keys.seal(&plaintext);
    let client = api::Client::new(&server);
    let created = client.create(&sealed, &keys.claim_hash(), ttl).map_err(|e| e.to_string())?;
    let url = client.drop_url(&created.id);
    let key = secret.encode();

    if p.flag("json") {
        let names: Vec<&str> = vars.iter().map(|(k, _)| k.as_str()).collect();
        println!(
            "{}",
            json!({ "id": created.id, "url": url, "key": key, "expires_at": created.expires_at, "names": names })
        );
        return Ok(());
    }

    let names: Vec<&str> = vars.iter().map(|(k, _)| k.as_str()).collect();
    ui::ok(&l!(
        "sealed {} variable(s) · AES-256-GCM · expires in {}",
        "已加密 {} 个变量 · AES-256-GCM · {} 后过期",
        vars.len(),
        ui::human(ttl)
    ));
    eprintln!("  {}", ui::dim(&names.join("  ")));
    eprintln!();
    eprintln!("  {}  {}", ui::dim(tr("link", "链接")), ui::cyan(&url));
    eprintln!("  {}  {}", ui::dim(tr("key ", "密钥")), ui::bold(&key));
    eprintln!();
    eprintln!(
        "{}",
        ui::dim(tr(
            "── paste to your agent ─────────────────────────────────────────",
            "── 粘贴给你的 Agent ────────────────────────────────────────────",
        ))
    );
    println!("{}", handoff(&url, &key));
    eprintln!("{}", ui::dim("────────────────────────────────────────────────────────────────"));
    Ok(())
}

pub fn handoff(url: &str, key: &str) -> String {
    l!(
        r#"I'm sharing secrets with you via envclip (end-to-end encrypted, one-time).
Install envclip and load them into memory (run once):

  curl -fsSL {} | sh -s -- {}

Then use them without ever printing them:
  envclip run -- <command>              # inject into a single command
  eval "$(envclip env)" && <command>    # or export into the current shell
`envclip ls` lists the names. Never echo, log or commit the values."#,
        r#"我通过 envclip 与你分享了一些机密（端到端加密，阅后即焚）。
运行一次以下命令，安装 envclip 并将它们载入内存：

  curl -fsSL {} | sh -s -- {}

之后在不打印明文的前提下使用它们：
  envclip run -- <命令>                 # 注入到单条命令
  eval "$(envclip env)" && <命令>       # 或导出到当前 shell
`envclip ls` 可列出变量名。切勿回显、记录或提交这些值。"#,
        url,
        key
    )
}

fn collect_vars(p: &Parsed) -> Result<Vec<Var>, String> {
    let mut vars: Vec<Var> = Vec::new();
    for f in p.all("file") {
        let src = Zeroizing::new(if f == "-" {
            let mut s = String::new();
            std::io::stdin().read_to_string(&mut s).map_err(|e| e.to_string())?;
            s
        } else {
            std::fs::read_to_string(f).map_err(|e| format!("{f}: {e}"))?
        });
        for (k, v) in dotenv::parse(&src).map_err(|e| format!("{f}: {e}"))? {
            dotenv::upsert(&mut vars, k, v);
        }
    }
    for name in &p.pos {
        if !dotenv::valid_name(name) {
            return Err(l!("invalid variable name `{}`", "无效的变量名 `{}`", name));
        }
        let v = match std::env::var(name) {
            Ok(v) => Zeroizing::new(v),
            Err(_) if ui::stdin_tty() => {
                ui::hidden(&format!("{} {} ", ui::cyan(name), ui::dim("›"))).map_err(|e| e.to_string())?
            }
            Err(_) => return Err(l!("${} is not set", "${} 未设置", name)),
        };
        dotenv::upsert(&mut vars, name.clone(), v);
    }
    if p.pos.is_empty() && p.all("file").is_empty() {
        if ui::stdin_tty() {
            interactive(&mut vars)?;
        } else {
            let mut s = Zeroizing::new(String::new());
            std::io::stdin().read_to_string(&mut s).map_err(|e| e.to_string())?;
            vars = dotenv::parse(&s)?;
        }
    }
    Ok(vars)
}

fn interactive(vars: &mut Vec<Var>) -> Res {
    ui::info(tr(
        "enter variables; values are hidden. Empty name to finish.",
        "逐个输入变量，值不会回显。名称留空以结束。",
    ));
    loop {
        let name = ui::line(&format!("{} ", ui::dim(tr("name  ›", "名称  ›")))).map_err(|e| e.to_string())?;
        if name.is_empty() {
            return Ok(());
        }
        if !dotenv::valid_name(&name) {
            ui::err(&l!("invalid name `{}`", "无效的变量名 `{}`", name));
            continue;
        }
        let v = ui::hidden(&format!("{} ", ui::dim(tr("value ›", "值    ›")))).map_err(|e| e.to_string())?;
        dotenv::upsert(vars, name, v);
    }
}

// ───────────────────────────── key + link ─────────────────────────────

fn read_key(p: &Parsed) -> Result<Secret, String> {
    let raw: Zeroizing<String> = if let Some(k) = p.opt("key") {
        Zeroizing::new(k.to_string())
    } else if let Some(k) = std::env::var("ENVCLIP_KEY").ok().filter(|k| !k.is_empty()) {
        Zeroizing::new(k)
    } else if !std::io::stdin().is_terminal() {
        let mut s = Zeroizing::new(String::new());
        std::io::stdin().take(1024).read_to_string(&mut s).map_err(|e| e.to_string())?;
        s
    } else {
        ui::hidden(&format!("{} ", ui::dim(tr("key ›", "密钥 ›")))).map_err(|e| e.to_string())?
    };
    Secret::parse(&raw).ok_or_else(|| {
        tr("malformed key (expect 26 base32 characters)", "密钥格式错误（应为 26 位 base32 字符）").into()
    })
}

fn link(p: &Parsed) -> Result<(api::Client, String), String> {
    let r = p.pos.first().ok_or(tr("missing <link>", "缺少 <链接>"))?;
    let (base, id) =
        api::parse_ref(r).ok_or_else(|| l!("not an envclip link: {}", "不是有效的 envclip 链接：{}", r))?;
    Ok((api::Client::new(&base), id))
}

const KEY_SPEC: Spec = Spec { values: &[("key", 'k'), ("hold", 'H')], flags: &[] };

// ───────────────────────────── agent side ─────────────────────────────

fn claim(argv: Vec<String>) -> Res {
    let p = args::parse(argv, &KEY_SPEC)?;
    let hold = match p.opt("hold") {
        Some(h) => ui::parse_duration(h).ok_or_else(|| l!("invalid --hold `{}`", "无效的 --hold `{}`", h))?,
        None => DEFAULT_HOLD,
    };
    let (client, id) = link(&p)?;
    let secret = read_key(&p)?;
    let keys = secret.keys();
    let sealed = client.claim(&id, &keys.claim_token()).map_err(|e| e.to_string())?;
    let pt = keys
        .open(&sealed)
        .ok_or(tr("decryption failed — the drop was corrupted", "解密失败 —— 数据已损坏"))?;
    let payload: Value = serde_json::from_slice(&pt).map_err(|e| e.to_string())?;
    let mut vars: Vec<Var> = Vec::new();
    for pair in payload["env"].as_array().into_iter().flatten() {
        if let (Some(k), Some(v)) = (pair[0].as_str(), pair[1].as_str())
            && dotenv::valid_name(k)
        {
            dotenv::upsert(&mut vars, k.to_string(), Zeroizing::new(v.to_string()));
        }
    }
    drop(payload);
    agent::put(&vars, &client.drop_url(&id), hold)?;

    ui::ok(&l!(
        "claimed {} variable(s) · server copy burned · held in memory for {}",
        "已领取 {} 个变量 · 服务端副本已销毁 · 在内存中保留 {}",
        vars.len(),
        ui::human(hold)
    ));
    let names: Vec<&str> = vars.iter().map(|(k, _)| k.as_str()).collect();
    eprintln!("  {}", names.join("  "));
    ui::info(tr(
        "use: envclip run -- <cmd>   or   eval \"$(envclip env)\" && <cmd>",
        "用法：envclip run -- <命令>   或   eval \"$(envclip env)\" && <命令>",
    ));
    Ok(())
}

fn fetch(names: &[String]) -> Result<Vec<Var>, String> {
    let resp = agent::request(&json!({ "op": "get", "names": names }))?;
    Ok(resp["env"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|p| Some((p[0].as_str()?.to_string(), Zeroizing::new(p[1].as_str()?.to_string()))))
        .collect())
}

fn run(argv: Vec<String>) -> Res {
    let p = args::parse(argv, &Spec { values: &[], flags: &[] })?;
    let cmd = p.rest.filter(|r| !r.is_empty()).ok_or(tr(
        "usage: envclip run [NAME...] -- <command> [args...]",
        "用法：envclip run [变量名...] -- <命令> [参数...]",
    ))?;
    let vars = fetch(&p.pos)?;
    let mut c = std::process::Command::new(&cmd[0]);
    c.args(&cmd[1..]);
    for (k, v) in &vars {
        c.env(k, v.as_str());
    }
    use std::os::unix::process::CommandExt;
    let e = c.exec();
    Err(format!("{}: {e}", cmd[0]))
}

/// Prints shell code that pulls each value from the agent at eval time, so
/// the printed text itself never contains a secret.
fn env(argv: Vec<String>) -> Res {
    let p = args::parse(argv, &Spec { values: &[("shell", 's')], flags: &[] })?;
    let resp = agent::request(&json!({ "op": "list" }))?;
    let loaded: Vec<String> = resp["vars"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|v| v["name"].as_str().map(str::to_string))
        .collect();
    let names: Vec<String> = if p.pos.is_empty() { loaded.clone() } else { p.pos.clone() };
    if let Some(missing) = names.iter().find(|n| !loaded.contains(n)) {
        return Err(l!("{} is not loaded", "{} 未加载", missing));
    }
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let exe = ui::sh_quote(&exe.to_string_lossy());
    let fish = match p.opt("shell") {
        None | Some("sh" | "bash" | "zsh" | "posix") => false,
        Some("fish") => true,
        Some(s) => return Err(l!("unsupported shell `{}`", "不支持的 shell `{}`", s)),
    };
    let mut out = String::new();
    out.push_str(&format!(
        "# envclip: {} variable(s); values are read from memory at eval time\n",
        names.len()
    ));
    for n in &names {
        if fish {
            out.push_str(&format!("set -gx {n} ({exe} get {n} | string collect -N)\n"));
        } else {
            out.push_str(&format!("{n}=\"$({exe} get {n} && echo .)\" && export {n}=\"${{{n}%.}}\"\n"));
        }
    }
    std::io::stdout().write_all(out.as_bytes()).map_err(|e| e.to_string())
}

fn get(argv: Vec<String>) -> Res {
    let [name] = argv.as_slice() else {
        return Err(tr("usage: envclip get <NAME>", "用法：envclip get <变量名>").into());
    };
    let vars = fetch(std::slice::from_ref(name))?;
    let (_, v) = vars.first().ok_or_else(|| l!("{} is not loaded", "{} 未加载", name))?;
    std::io::stdout().write_all(v.as_bytes()).map_err(|e| e.to_string())
}

fn ls() -> Res {
    let resp = agent::request(&json!({ "op": "list" }))?;
    let vars = resp["vars"].as_array().cloned().unwrap_or_default();
    let left = Duration::from_secs(resp["expires_in"].as_u64().unwrap_or(0));
    for v in &vars {
        println!("{}", v["name"].as_str().unwrap_or(""));
    }
    ui::info(&l!(
        "{} variable(s) in memory · agent exits in {}",
        "内存中有 {} 个变量 · agent 将在 {} 后退出",
        vars.len(),
        ui::human(left)
    ));
    Ok(())
}

fn clear(argv: Vec<String>) -> Res {
    if argv.is_empty() {
        agent::request(&json!({ "op": "stop" }))?;
        ui::ok(tr("memory wiped, agent stopped", "内存已清除，agent 已停止"));
    } else {
        agent::request(&json!({ "op": "rm", "names": argv }))?;
        ui::ok(&l!("forgot {}", "已清除 {}", argv.join(", ")));
    }
    Ok(())
}

// ───────────────────────────── sender follow-up ─────────────────────────────

fn status(argv: Vec<String>) -> Res {
    let p = args::parse(argv, &KEY_SPEC)?;
    let (client, id) = link(&p)?;
    let keys = read_key(&p)?.keys();
    match client.status(&id, &keys.claim_token()) {
        Ok(s) => {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            let left = Duration::from_secs(s.expires_at.saturating_sub(now));
            ui::ok(&l!("waiting to be claimed · expires in {}", "等待领取 · {} 后过期", ui::human(left)));
            Ok(())
        }
        Err(api::Error::NotFound) => {
            ui::info(tr(
                "gone — claimed, revoked or expired (or wrong key)",
                "已不存在 —— 已被领取、撤销或过期（或密钥错误）",
            ));
            Ok(())
        }
        Err(e) => Err(e.to_string()),
    }
}

fn revoke(argv: Vec<String>) -> Res {
    let p = args::parse(argv, &KEY_SPEC)?;
    let (client, id) = link(&p)?;
    let keys = read_key(&p)?.keys();
    client.revoke(&id, &keys.claim_token()).map_err(|e| e.to_string())?;
    ui::ok(tr("revoked — the ciphertext has been destroyed", "已撤销 —— 密文已销毁"));
    Ok(())
}
