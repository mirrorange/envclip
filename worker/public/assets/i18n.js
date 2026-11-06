const DICT = {
  en: {
    "nav.lang": "中文",
    "hero.title": "Hand secrets to your agent.<br>Not to the chat.",
    "hero.sub":
      "envclip seals environment variables in your browser, lets an agent sandbox open them exactly once, and keeps the plaintext out of every log, transcript and server along the way.",
    "compose.title": "new clip",
    "compose.hint": "encrypted in this tab",
    "compose.name": "NAME",
    "compose.value": "value",
    "compose.add": "+ variable",
    "compose.import": "import .env",
    "compose.parse": "parse",
    "compose.paste": "# paste a .env file\nAPI_TOKEN=…\nDATABASE_URL=…",
    "compose.ttl": "expires in",
    "compose.seal": "seal",
    "compose.sealing": "sealing…",
    "result.title": "sealed",
    "result.link": "link",
    "result.key": "key",
    "result.prompt": "paste this to your agent",
    "result.copy": "copy",
    "result.copied": "copied",
    "result.show": "show",
    "result.hide": "hide",
    "result.revoke": "revoke",
    "result.new": "new clip",
    "result.note": "The key is shown once and never leaves this tab. Close it and it is gone.",
    "status.sealed": "sealed · waiting for the agent",
    "status.gone": "burned · opened, revoked or expired",
    "status.revoked": "revoked",
    "how.title": "how it works",
    "how.1.t": "seal",
    "how.1.d": "A random 256-bit key is created here. Variables are encrypted with AES-256-GCM before anything is sent.",
    "how.2.t": "share",
    "how.2.d": "The server stores ciphertext and a hash of an access token. You give the agent a link and the key.",
    "how.3.t": "open",
    "how.3.d": "The agent runs one command. envclip fetches the ciphertext, the server deletes it in the same statement.",
    "how.4.t": "use",
    "how.4.d": "Plaintext stays in envclip's memory. <code>envclip run -- cmd</code> injects it without echo.",
    "cli.title": "cli",
    "cli.d": "Same envelope, from your terminal. Read variables straight from your shell:",
    "drop.title": "This link is for an agent.",
    "drop.sub":
      "Someone sealed environment variables for an AI agent. The agent runs the command below with the key it was given; the ciphertext is burned on first open.",
    "drop.check": "status",
    "foot.tag": "zero-knowledge · burn after reading · no echo",
    "err.empty": "Add at least one variable.",
    "err.name": "Invalid name: ",
    "err.dup": "Duplicate name: ",
    "err.network": "Network error. Try again.",
    "err.server": "Server refused: ",
    "ttl.10m": "10m",
    "ttl.1h": "1h",
    "ttl.1d": "1d",
    "ttl.7d": "7d",
    prompt: (cmd) =>
      `I shared some environment variables with you through envclip (end-to-end encrypted, burn after reading). Load them with:

  ${cmd}

envclip keeps them in memory. Never print, log or commit their values. Use them like this:

  envclip run -- <command>
  eval "$(envclip env)" && <command>

\`envclip ls\` lists the names; \`envclip clear\` wipes them when you are done.`,
  },
  zh: {
    "nav.lang": "EN",
    "hero.title": "把机密交给 Agent，<br>而不是交给聊天记录。",
    "hero.sub":
      "envclip 在浏览器中加密环境变量，只允许 Agent 沙盒打开一次，明文不会出现在任何日志、对话记录或服务器上。",
    "compose.title": "新建",
    "compose.hint": "在此标签页内加密",
    "compose.name": "变量名",
    "compose.value": "值",
    "compose.add": "+ 变量",
    "compose.import": "导入 .env",
    "compose.parse": "解析",
    "compose.paste": "# 粘贴 .env 文件\nAPI_TOKEN=…\nDATABASE_URL=…",
    "compose.ttl": "有效期",
    "compose.seal": "封存",
    "compose.sealing": "加密中…",
    "result.title": "已封存",
    "result.link": "链接",
    "result.key": "密钥",
    "result.prompt": "发送给 Agent",
    "result.copy": "复制",
    "result.copied": "已复制",
    "result.show": "显示",
    "result.hide": "隐藏",
    "result.revoke": "撤销",
    "result.new": "再建一个",
    "result.note": "密钥只显示这一次，且从未离开此标签页。关闭后即无法找回。",
    "status.sealed": "已封存 · 等待 Agent 打开",
    "status.gone": "已销毁 · 已被打开、撤销或过期",
    "status.revoked": "已撤销",
    "how.title": "工作原理",
    "how.1.t": "封存",
    "how.1.d": "在本地生成 256 位随机密钥，变量经 AES-256-GCM 加密后才会发出。",
    "how.2.t": "分享",
    "how.2.d": "服务端只保存密文与访问令牌的哈希。你把链接和密钥交给 Agent。",
    "how.3.t": "打开",
    "how.3.d": "Agent 运行一条命令。envclip 取回密文，服务端在同一条语句中将其删除。",
    "how.4.t": "使用",
    "how.4.d": "明文只保存在 envclip 进程内存中，<code>envclip run -- cmd</code> 注入时不回显。",
    "cli.title": "命令行",
    "cli.d": "同样的加密格式，也可以在终端里使用，直接读取当前 shell 中的变量：",
    "drop.title": "这个链接是给 Agent 的。",
    "drop.sub": "有人为 AI Agent 封存了一组环境变量。Agent 使用收到的密钥运行下面的命令即可打开；首次打开后密文即被销毁。",
    "drop.check": "状态",
    "foot.tag": "零知识 · 阅后即焚 · 无回显",
    "err.empty": "请至少添加一个变量。",
    "err.name": "变量名无效：",
    "err.dup": "变量名重复：",
    "err.network": "网络错误，请重试。",
    "err.server": "服务端拒绝：",
    "ttl.10m": "10 分钟",
    "ttl.1h": "1 小时",
    "ttl.1d": "1 天",
    "ttl.7d": "7 天",
    prompt: (cmd) =>
      `我通过 envclip 向你分享了一些环境变量（端到端加密，阅后即焚）。请运行以下命令加载：

  ${cmd}

envclip 会把它们保存在内存中。不要打印、记录或提交这些值。这样使用：

  envclip run -- <命令>
  eval "$(envclip env)" && <命令>

\`envclip ls\` 列出变量名；用完后运行 \`envclip clear\` 清除。`,
  },
};

const KEY = "envclip.lang";

export let lang =
  localStorage.getItem(KEY) || ((navigator.language || "").toLowerCase().startsWith("zh") ? "zh" : "en");

export const t = (k) => DICT[lang][k] ?? DICT.en[k] ?? k;

export function apply(root = document) {
  document.documentElement.lang = lang === "zh" ? "zh-CN" : "en";
  for (const el of root.querySelectorAll("[data-i18n]")) el.innerHTML = t(el.dataset.i18n);
  for (const el of root.querySelectorAll("[data-i18n-ph]")) el.placeholder = t(el.dataset.i18nPh);
}

export function toggle() {
  lang = lang === "zh" ? "en" : "zh";
  localStorage.setItem(KEY, lang);
  apply();
  document.dispatchEvent(new Event("langchange"));
}
