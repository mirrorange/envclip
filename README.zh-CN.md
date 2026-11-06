# envclip_

**把机密交给 Agent，而不是交给聊天记录。**

[English](README.md) · [协议](docs/PROTOCOL.md) · [安全模型](docs/SECURITY.md)

云端 Agent（如 Claude Managed Agents）运行在你无法直接访问的沙盒里。当任务需要
令牌、数据库地址或部署密钥时，通常只能把它们粘贴进聊天，然后永远留在对话记录中。

envclip 在你的本地加密环境变量，只允许沙盒打开**一次**：

- **端到端加密。** 在浏览器或 CLI 中使用 AES-256-GCM 加密，服务端只保存它无法解密的密文。
- **阅后即焚。** 第一次成功打开时，返回密文的同一条 SQL 语句会将其删除。
- **无回显。** 明文只存在于一个小型 `envclip` 后台进程的内存中，`envclip run -- cmd` 注入变量而不打印。
- **边缘原生。** 一个 Cloudflare Worker、一张 D1 表、静态资源，无需运维服务器。
- **单个静态二进制。** 约 2 MB，无运行时依赖，支持 Linux 与 macOS。

```
 你                          envclip 边缘                Agent 沙盒
 ──                          ────────────                ──────────
 seal(vars) ── 密文 ───────▶ D1 行
 链接 + 密钥 ───────────────────────────────────────────▶ curl 链接 | sh -s -- 密钥
                             D1 行 ◀── open ───────────── envclip open
                             （已删除）── 密文 ─────────▶ 在内存中解密
                                                         envclip run -- npm test
```

## 发送

**浏览器。** 打开你的 envclip 站点，添加变量或粘贴 `.env`，选择有效期，点击 **封存**，
把生成的消息复制给 Agent。

**命令行。**

```sh
curl -fsSL https://<你的-envclip>/install.sh | sh

envclip push API_TOKEN DATABASE_URL     # 从当前 shell 读取
envclip push -f .env.staging --ttl 10m  # 或从 .env 文件读取
envclip push                            # 或手动输入，值不回显
```

`push` 将链接与密钥输出到 stderr，将可直接粘贴的消息输出到 stdout，
因此 `envclip push API_TOKEN | pbcopy` 会把消息直接放进剪贴板。

## 接收（Agent 执行）

```sh
curl -fsSL https://<你的-envclip>/s/<id> | sh -s -- <密钥>
```

这一行会下载 CLI（校验 SHA-256）、取回并销毁密文、解密，然后把变量交给后台进程。
`curl` 只请求 `/s/<id>`；密钥是本地 `sh` 的参数，永远不会发往服务端。

之后在任意 shell 中：

```sh
envclip run -- ./deploy.sh              # 注入变量，不打印任何内容
eval "$(envclip env)" && git push       # 或导出到当前 shell
envclip ls                              # 只列出变量名
envclip clear                           # 清除并停止后台进程
```

`envclip env` 拒绝向终端输出，它只用于 `eval`。

## 命令参考

| 命令 | |
| ---- | - |
| `envclip push [NAME…] [-f 文件] [--ttl 时长]` | 封存变量，输出链接、密钥和给 Agent 的消息 |
| `envclip open <链接\|ID> [--key 密钥] [--hold 时长]` | 取回、销毁、解密并保存在内存中 |
| `envclip run [NAME…] -- <命令>` | 使用已保存的变量执行命令 |
| `envclip env [NAME…]` | 输出供 `eval` 使用的 `export` 语句 |
| `envclip ls` | 列出已保存的变量名 |
| `envclip clear` | 清除内存并停止后台进程 |
| `envclip revoke <链接\|ID> [--key 密钥]` | 在被打开之前删除 clip |

| 选项 / 环境变量 | |
| --------------- | - |
| `--server URL` · `ENVCLIP_SERVER` · `~/.config/envclip/server` | 使用的 envclip 服务端 |
| `--ttl 时长` | 服务端保留时长，`1m` … `7d`，默认 `1h` |
| `--hold 时长` | 后台进程保存变量的时长，默认 `12h` |
| `--lang en\|zh` · `ENVCLIP_LANG` · `LANG` | 界面语言 |
| `ENVCLIP_SOCK` | 后台进程的 socket 路径 |

密钥依次从 `--key`、管道输入的 stdin、隐藏提示中读取。

## 在 Cloudflare 上自托管

```sh
cd worker
npm ci
npx wrangler d1 create envclip          # 把 database_id 填入 wrangler.jsonc
npm run db:migrate                      # 将 migrations/ 应用到远程 D1
npm run deploy
```

然后在 GitHub 仓库中：

1. 在 *Settings → Secrets and variables → Actions → Variables* 中设置
   `ENVCLIP_SERVER` 为部署后的地址，发布构建会把它作为 `envclip push` 的默认服务端。
2. 推送 `v0.1.0` 之类的标签。发布流程会构建 `x86_64`/`aarch64` Linux（musl）
   与 macOS 的静态二进制，并附带 `.sha256` 文件。

Worker 通过代理 GitHub Releases 在 `/dl/…` 下提供这些文件（见 `wrangler.jsonc`
中的 `RELEASE_BASE`），因此沙盒只需能访问你的 envclip 域名。

## 目录结构

```
cli/            Rust CLI：加密、API 客户端、内存后台进程、中英文消息
worker/src/     Worker：API、安装脚本、二进制代理
worker/public/  网页：原生 JS、WebCrypto、中英文，无需构建
worker/migrations/  D1 表结构
testdata/       两种实现共用的加密测试向量
docs/           协议与安全模型
```

## 开发

```sh
cargo test                              # CLI，包括共用测试向量
cd worker && npm ci && npm test         # 网页端加密与 .env 解析
npm run db:migrate:local && npm run dev # http://localhost:8787
ENVCLIP_SERVER=http://localhost:8787 cargo run -- push FOO
```
