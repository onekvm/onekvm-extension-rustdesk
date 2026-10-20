# OneKVM RustDesk 扩展

[English](README.md) | 简体中文

让 RustDesk 客户端连接到 OneKVM 所采集和控制的目标计算机，而不是运行
OneKVM 的设备操作系统。协议运行时消费 OneKVM 编码后的视频流，并通过
已鉴权的扩展控制 Socket 转发输入。

> **构建说明：** 本仓库只包含扩展源码。正式软件包由 `onekvm-distro` 中的
> OpenEmbedded（OE）配方统一构建；本仓库不提供独立的正式构建流程。

## 功能

- 稳定的 10 位 RustDesk ID 和持久化 Ed25519 身份
- 通过 UDP 21116 向 hbbs 注册并保活
- 通过 TCP 21117 建立 hbbr 中继会话，支持打洞失败后的中继回退
- 签名身份交换，以及采用 XSalsa20Poly1305 加密的 ID/中继会话
- 可选的 TCP 21118 无界面直连端点
- H.264 或 H.265 直接转发，不解码也不重新编码
- 键盘及绝对或相对指针输入
- 可配置帧率、画质预算、码率、QP、GOP 和客户端数量上限

扩展不提供文件传输、剪贴板、终端、端口转发或设备电源操作。

## 安装与启用

软件包名称是 `onekvm-extension-rustdesk`。请通过 OneKVM 发行版的软件包
或镜像安装，然后使用插件管理器控制扩展：

```sh
onekvm-extension enable rustdesk
onekvm-extension disable rustdesk
```

RustDesk 服务默认关闭。选择 ID 服务器配置后，再从扩展设置中开启服务。

## 服务器配置

在 **RustDesk** 扩展设置中选择一种配置：

- **官方 ID 服务器：** 使用 RustDesk 上游的 rendezvous 端点和公钥。
- **自定义服务器：** 填写 hbbs 地址（通常使用端口 `21116`）和服务器公钥；
  还可以填写 hbbr 地址（通常使用端口 `21117`）。中继地址留空时，扩展会
  使用 hbbs 返回的地址，或使用 hbbs 端口加一。

API 服务器字段用于完整描述自定义服务器配置，但这个无界面受控端点不会
访问它。首次启动服务后，页面会把生成的 RustDesk ID 显示为只读值。

可选的直连监听默认是 `0.0.0.0:21118`。直连 IP 握手不会加密会话，只适合
可信局域网；ID 和中继连接会使用 RustDesk 的公钥签名握手及加密帧。向其他
用户开放任何连接方式前，请先配置密码。

## 身份与视频

RustDesk ID、UUID 和签名种子会在首次启动时生成，并以 `0600` 权限保存到
`ONEKVM_EXTENSION_DATA/identity.json`。默认最多允许一个客户端，可以
调整为 1 到 8 个。

扩展向 OneKVM 请求硬件编码的 H.264 或 H.265 视频并直接转发，不经过解码
和重新编码。扩展页面中的编码选项用于控制帧率、资源预算、码率、QP 和关键
帧间隔。

## 开发与互操作测试

运行时使用 Rust 2021，要求 Rust 1.75 或更高版本。普通测试命令是：

```sh
cargo test
```

被忽略的 `official_hbbs_hbbr_secure_interop` 测试会连接真实的官方 hbbs/hbbr，
需要 `RUSTDESK_TEST_HBBS`、`RUSTDESK_TEST_RELAY`、`RUSTDESK_TEST_ID`、
`RUSTDESK_TEST_KEY` 和 `RUSTDESK_TEST_DATA`。也可以用
`RUSTDESK_TEST_PUBLIC_KEY` 代替目标数据路径。

## 许可证

本项目包含 RustDesk 衍生工作，仅按 GNU Affero General Public License
v3.0 授权，完整条款见 [`LICENSE`](LICENSE)。

打包的 [`web/rustdesk.svg`](web/rustdesk.svg) 是 RustDesk 上游提交
`807e05ea9a7e298ed2deb438195faaafce19cdd2` 中的 Flutter 图标，同样按
AGPL-3.0 分发。
