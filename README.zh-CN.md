# OneKVM RustDesk 扩展

[English](README.md) | 简体中文

> **构建说明：** 本仓库只包含扩展源码。扩展统一由 `onekvm-distro` 中的
> OpenEmbedded（OE）配方构建和打包；本仓库不提供独立的正式构建流程。

这是 OneKVM 的进程外 RustDesk 协议支持。它用于远程访问 OneKVM 所采集
的目标计算机，而不是运行 OneKVM 的设备操作系统。扩展消费 OneKVM 编码
后的视频流，并通过已鉴权的扩展控制 Socket 转发键盘和鼠标事件。

## 功能

- 稳定的 10 位 RustDesk ID 和持久化 Ed25519 身份
- 通过 UDP 21116 向 hbbs 注册并保活
- 通过 TCP 21117 连接 hbbr 中继，支持打洞失败后的中继回退
- RustDesk 签名身份交换和 XSalsa20Poly1305 加密会话
- 可选的 TCP 21118 无界面直连端点
- H.264 直接转发，不解码也不重新编码
- 键盘及绝对/相对指针输入
- 不支持文件传输、剪贴板、终端、端口转发和设备电源操作

直连 IP 握手不会加密会话，因此只适合可信局域网。通过 ID 和中继建立的
连接会使用 RustDesk 的公钥签名握手及加密帧。首次启动时生成的 ID、UUID
和签名种子会以 0600 权限保存到
`ONEKVM_EXTENSION_DATA/identity.json`。

被忽略的 `official_hbbs_hbbr_secure_interop` 测试用于连接真实 hbbs/hbbr，
需要 README 英文版中列出的 `RUSTDESK_TEST_*` 环境变量。

## 许可

RustDesk 衍生代码及其打包产物须遵守 GNU Affero General Public License
3.0。`web/rustdesk.svg` 来自 RustDesk 上游 Flutter 图标，同样按
AGPL-3.0 分发。
