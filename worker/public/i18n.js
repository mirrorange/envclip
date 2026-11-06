// UI strings. Keys are referenced from index.html via data-i18n / data-i18n-ph.

export const STRINGS = {
  en: {
    tagline: "Hand secrets to your AI agent. Not to the chat.",
    sub: "Encrypted in your browser · burn after reading · never echoed",
    compose: "new drop",
    name_ph: "NAME",
    value_ph: "value",
    add: "+ variable",
    paste: "paste .env",
    paste_ph: "# paste KEY=value lines here\nOPENAI_API_KEY=sk-...\nGIT_TOKEN=...",
    import: "import",
    cancel: "cancel",
    ttl: "expires in",
    seal: "seal & upload",
    sealing: "sealing…",
    show: "show",
    hide: "hide",
    remove: "remove",
    sealed: (n, ttl) => `sealed ${n} variable${n === 1 ? "" : "s"} · AES-256-GCM · expires in ${ttl}`,
    link: "link",
    key: "key",
    copy: "copy",
    copied: "copied",
    handoff: "paste to your agent",
    copy_handoff: "copy message",
    key_warn: "The key exists only in this tab. envclip never saw it — close the tab and it's gone.",
    status: "status",
    revoke: "revoke",
    again: "new drop",
    waiting: (ttl) => `waiting to be claimed · expires in ${ttl}`,
    gone: "gone — claimed, revoked or expired",
    revoked: "revoked — ciphertext destroyed",
    how: "how it works",
    s1t: "seal", s1: "Your browser (or the CLI) encrypts the variables with a fresh random key. Only ciphertext leaves your machine.",
    s2t: "hand off", s2: "You give the agent a link and the key. The link alone is useless; the server never sees the key.",
    s3t: "claim", s3: "The agent runs one command. envclip downloads the ciphertext and the server deletes it — one read, ever.",
    s4t: "use", s4: "Plaintext lives only in envclip's memory. `envclip run` injects it into commands; nothing is printed.",
    cli: "cli",
    cli_note: "Single static binary. Reads straight from your shell, so secrets never touch the clipboard.",
    c1: "install",
    c2: "send variables straight from your shell",
    c3: "inside the agent sandbox",
    drop: "drop",
    drop_intro: "This link is meant for an AI agent's sandbox. The agent should run:",
    drop_sender: "Sent this? Check or revoke it with your key:",
    key_ph: "26-character key",
    bad_key: "malformed key",
    empty: "add at least one variable",
    bad_name: (n) => `invalid name: ${n || "(empty)"}`,
    dupe: (n) => `duplicate name: ${n}`,
    too_large: "too large (max ≈ 64 KiB)",
    net: (e) => `request failed: ${e}`,
    agent_msg: (url, key) => `I'm sharing secrets with you via envclip (end-to-end encrypted, one-time).
Install envclip and load them into memory (run once):

  curl -fsSL ${url} | sh -s -- ${key}

Then use them without ever printing them:
  envclip run -- <command>              # inject into a single command
  eval "$(envclip env)" && <command>    # or export into the current shell
\`envclip ls\` lists the names. Never echo, log or commit the values.`,
    foot: "zero-knowledge · AES-256-GCM · HKDF-SHA256 · Cloudflare Workers + D1",
    source: "source",
  },
  zh: {
    tagline: "把机密交给你的 AI Agent，而不是聊天记录。",
    sub: "浏览器内加密 · 阅后即焚 · 全程无回显",
    compose: "新建投递",
    name_ph: "变量名",
    value_ph: "值",
    add: "+ 变量",
    paste: "粘贴 .env",
    paste_ph: "# 在此粘贴 KEY=value 格式的内容\nOPENAI_API_KEY=sk-...\nGIT_TOKEN=...",
    import: "导入",
    cancel: "取消",
    ttl: "有效期",
    seal: "加密并上传",
    sealing: "加密中…",
    show: "显示",
    hide: "隐藏",
    remove: "移除",
    sealed: (n, ttl) => `已加密 ${n} 个变量 · AES-256-GCM · ${ttl} 后过期`,
    link: "链接",
    key: "密钥",
    copy: "复制",
    copied: "已复制",
    handoff: "粘贴给你的 Agent",
    copy_handoff: "复制消息",
    key_warn: "密钥只存在于此标签页中，envclip 从未见过它 —— 关闭页面后即无法找回。",
    status: "状态",
    revoke: "撤销",
    again: "再来一个",
    waiting: (ttl) => `等待领取 · ${ttl} 后过期`,
    gone: "已不存在 —— 已被领取、撤销或过期",
    revoked: "已撤销 —— 密文已销毁",
    how: "工作原理",
    s1t: "加密", s1: "浏览器（或 CLI）使用全新的随机密钥加密变量，离开你设备的只有密文。",
    s2t: "交付", s2: "把链接和密钥交给 Agent。只有链接毫无用处；服务端永远看不到密钥。",
    s3t: "领取", s3: "Agent 运行一条命令。envclip 下载密文，服务端随即删除 —— 仅可读取一次。",
    s4t: "使用", s4: "明文只存在于 envclip 的内存中。`envclip run` 把它注入命令，全程不打印。",
    cli: "命令行",
    cli_note: "单个静态二进制。直接从 shell 读取变量，机密不经过剪贴板。",
    c1: "安装",
    c2: "直接从当前 shell 发送变量",
    c3: "在 Agent 沙盒中",
    drop: "投递",
    drop_intro: "此链接是给 AI Agent 云端沙盒使用的。Agent 应运行：",
    drop_sender: "是你发送的？使用密钥查看状态或撤销：",
    key_ph: "26 位密钥",
    bad_key: "密钥格式错误",
    empty: "请至少添加一个变量",
    bad_name: (n) => `无效的变量名：${n || "（空）"}`,
    dupe: (n) => `变量名重复：${n}`,
    too_large: "内容过大（上限约 64 KiB）",
    net: (e) => `请求失败：${e}`,
    agent_msg: (url, key) => `我通过 envclip 与你分享了一些机密（端到端加密，阅后即焚）。
运行一次以下命令，安装 envclip 并将它们载入内存：

  curl -fsSL ${url} | sh -s -- ${key}

之后在不打印明文的前提下使用它们：
  envclip run -- <命令>                 # 注入到单条命令
  eval "$(envclip env)" && <命令>       # 或导出到当前 shell
\`envclip ls\` 可列出变量名。切勿回显、记录或提交这些值。`,
    foot: "零知识 · AES-256-GCM · HKDF-SHA256 · Cloudflare Workers + D1",
    source: "源码",
  },
};

export function detectLang() {
  const saved = localStorage.getItem("envclip.lang");
  if (saved === "en" || saved === "zh") return saved;
  return (navigator.language || "en").toLowerCase().startsWith("zh") ? "zh" : "en";
}
