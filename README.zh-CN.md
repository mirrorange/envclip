<div align="center">

```
envclip_
```

**把机密交给你的 AI Agent，而不是聊天记录。**

端到端加密 · 阅后即焚 · 全程无回显

[English](README.md) · [简体中文](README.zh-CN.md)

</div>

---

云端 Agent（Claude Managed Agents、各类托管编码 Agent……）运行在你无法直接登录的沙盒里。
当任务需要凭据——测试私有 API 的 token、推送到自托管 Git 仓库的密码——唯一的通道往往是聊天框，
而机密会永久留在对话记录中。

envclip 是一个运行在边缘的、专为环境变量设计的"投递箱"：

- **零知识服务端**：在你的电脑（CLI）或浏览器（Web）中完成加密，Cloudflare Worker 只保存密文。
- **阅后即焚**：每份投递只能被领取一次，随即删除；未领取的投递按时过期（1 分钟 – 7 天）。
- **无回显**：在沙盒中，明文只存在于一个小小的 envclip agent 进程内存里；命令通过注入获得变量，不会被打印。
- **单一静态二进制**：Rust 编写，约 2 MB，无运行时依赖。

## 工作原理

```
 你                                  envclip 边缘服务 (Workers + D1)          Agent 沙盒
 ──                                  ──────────────────────────────          ─────────
 envclip send API_TOKEN
   ├─ 用全新密钥加密 ────────────▶    保存密文 + H(领取令牌)
   └─ 输出链接与密钥
 把链接与密钥发到对话中 ────────────────────────────────────────────────▶   curl -fsSL <链接> | sh -s -- <密钥>
                                     DELETE … RETURNING ciphertext  ◀──────  ├─ 安装 envclip 并领取
                                     （该行永久消失）                         └─ 解密到 agent 内存
                                                                             envclip run -- git push
                                                                             eval "$(envclip env)" && ./test.sh
```

1. **加密**：envclip 生成随机 128 位密钥，用 AES-256-GCM 加密变量，只上传密文；密钥不离开你手中。
2. **交付**：把链接和密钥粘贴到 Agent 对话（envclip 会生成现成的消息）。
3. **领取**：Agent 运行一条命令，安装 envclip、证明自己持有密钥、取回密文，服务端原子性地删除密文。
4. **使用**：解密后的值只保存在 envclip 内存中。`envclip run` 把它们注入命令；`envclip env` 输出的
   shell 代码会在 eval 时才去读取值，因此连它的输出里都没有机密。

## 快速开始

### 发送：网页端

打开你部署的 envclip 站点，添加变量（或粘贴 `.env`），点击 **加密并上传**，把生成的消息复制给 Agent。

### 发送：命令行

```sh
curl -fsSL https://<你的-envclip>/install.sh | sh

# 直接读取当前 shell 中的变量 —— 全程不需要复制粘贴明文
envclip send OPENAI_API_KEY GITEA_TOKEN --ttl 1h

# 也可以来自文件 / 标准输入 / 隐藏输入的交互提示
envclip send -f .env.agent
op read op://vault/db/password | sed 's/^/DB_PASSWORD=/' | envclip send
envclip send
```

`send` 会输出链接、密钥，以及一段给 Agent 的说明（设置 `ENVCLIP_LANG=zh` 或中文 `LANG` 时为中文）。

### 使用：在 Agent 沙盒中

```sh
envclip run -- git push origin main               # 单条命令，注入变量
envclip run GITEA_TOKEN -- ./deploy.sh            # 只注入部分变量
eval "$(envclip env)" && curl -H "Authorization: Bearer $API_TOKEN" …
envclip ls                                        # 只列出变量名
envclip clear                                     # 清除内存并停止 agent
```

> 许多 Agent 框架每条命令都会开启新的 shell，因此推荐在同一条命令里使用
> `envclip run -- …` 或 `eval "$(envclip env)" && …`。

## 命令参考

| 命令 | |
|---|---|
| `envclip send [变量名…] [-f 文件] [--ttl 24h] [--server URL] [--json]` | 加密并上传。`变量名` 从当前 shell 读取 `$变量名`（未设置时隐藏输入）。`-f -` 读取标准输入。 |
| `envclip status <链接> [--key K]` | 投递是否仍在等待领取 |
| `envclip revoke <链接> [--key K]` | 销毁尚未领取的投递 |
| `envclip claim <链接> [--key K] [--hold 12h]` | 领取、销毁服务端副本、解密到内存 agent |
| `envclip run [变量名…] -- <命令…>` | 以注入的环境变量执行命令 |
| `envclip env [变量名…] [--shell sh\|fish]` | 供 `eval` 使用的 shell 代码，不含明文 |
| `envclip get <变量名>` | 输出单个原始值（用于管道 —— 它*会*打印明文） |
| `envclip ls` | 列出已加载的变量名与 agent 剩余时间 |
| `envclip clear [变量名…]` | 清除部分变量；不带参数则全部清除并停止 agent |

密钥按顺序从 `--key`、`$ENVCLIP_KEY`、标准输入（管道）或隐藏提示中读取。时长支持 `90s`、`15m`、`12h`、`7d`。

| 环境变量 | |
|---|---|
| `ENVCLIP_SERVER` | `send` 及纯 ID 使用的服务地址 |
| `ENVCLIP_KEY` | `claim` / `status` / `revoke` 的密钥 |
| `ENVCLIP_LANG` | `en` 或 `zh`（默认跟随 `LANG`） |
| `ENVCLIP_SOCK` | agent 套接字路径 |
| `NO_COLOR` | 关闭颜色 |

## 安全性

| | 密文 | 密钥 | 明文 |
|---|---|---|---|
| envclip 服务端 | ✓（领取或过期前） | ✗ | ✗ |
| 网络观察者 | TLS | ✗ | ✗ |
| AI 服务商 / 对话记录 | ✗ | ✓ 链接 + 密钥 | ✗ —— 值从不被打印 |
| Agent 沙盒 | 一次 | ✓ | ✓ 位于 envclip 内存及被注入的命令中 |

- **服务端**只保存密文和 `SHA-256(领取令牌)`。领取令牌由 HKDF 从密钥派生，与加密密钥相互独立，
  服务端可以据此授权领取，却得不到任何可用于解密的信息。
- **错误的密钥不会销毁投递**。领取是一条受领取哈希保护的 `DELETE … RETURNING` 语句，
  因此只有一个持有密钥的请求能拿到密文。
- **对话记录**中必然包含链接与密钥 —— 这是沙盒获得它们的方式。它们是一次性凭证：沙盒领取后即作废。
  如果有人抢先使用，Agent 的领取会明确失败（`drop not found`），你便知道需要轮换机密。
- 在**沙盒**中，值保存在独立的 agent 进程里（不可 dump、禁用 core dump、仅限本用户的套接字并校验对端 uid、
  清空的环境变量、退出时清零）。envclip 防止的是意外泄露，无法阻止 Agent 故意执行 `echo $TOKEN`。
  请尽量授予最小权限、最短有效期的凭据。

完整的协议与威胁模型见 [docs/PROTOCOL.md](docs/PROTOCOL.md)。

## 在 Cloudflare 上自托管

```sh
cd worker
npm ci
npx wrangler d1 create envclip          # 把 database_id 填入 wrangler.jsonc
npm run migrate:remote
npm run deploy                          # → https://envclip.<you>.workers.dev
```

然后让 CLI 指向它：

- 在仓库中设置变量 `ENVCLIP_SERVER`（如 `https://envclip.example.com`），发布二进制将默认使用该服务；
  推送 `v*` 标签后，`release` 工作流会构建静态 Linux（musl）与 macOS 二进制，
  `/install.sh` 与 `/c/<id>` 会通过 `/dl/*` 下载它们；
- 或者在发送端机器上 `export ENVCLIP_SERVER=https://…`。

`wrangler.jsonc` 中的 `RELEASES_URL` 决定 `/dl/*` 的跳转地址。如需从 CI 部署，添加
`CLOUDFLARE_API_TOKEN`、`CLOUDFLARE_ACCOUNT_ID` 机密并设置变量 `DEPLOY_WORKER=true`。

## 开发

```sh
cd cli && cargo test                 # 加密向量、解析器
cd worker && npm test                # 浏览器端加密对照同一组向量
./scripts/e2e.sh                     # wrangler dev + 本地 D1 + CLI + 安装脚本 + Web 加密
```

## 许可证

MIT
