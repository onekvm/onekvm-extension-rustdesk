# OneKVM RustDesk Extension

English | [简体中文](README.zh-CN.md)

> **Build note:** This repository contains extension sources only. The
> extension is built and packaged by the OpenEmbedded recipes in
> `onekvm-distro`; this repository does not provide a standalone release build.

Out-of-process RustDesk protocol support for OneKVM.

The extension is intended to expose the target computer captured by OneKVM,
not the operating system running on the OneKVM device. It will consume the
encoded OneKVM media stream and route RustDesk keyboard and pointer events
through OneKVM's authenticated extension control socket.

## Features

- Stable 10-digit RustDesk ID and persistent Ed25519 identity
- hbbs registration and keep-alive over UDP 21116
- hbbr relay sessions over TCP 21117, including relay fallback for hole-punch requests
- RustDesk signed identity exchange and XSalsa20Poly1305 encrypted sessions
- Optional headless direct-IP endpoint on TCP 21118
- H.264 passthrough without decoding or re-encoding
- Keyboard and absolute/relative pointer input
- No file transfer, clipboard, terminal, port forwarding, or device power
  operations

Direct-IP transport is intentionally limited to trusted LANs because RustDesk's
direct-IP handshake does not encrypt the session. ID and relay connections use
RustDesk's signed public-key handshake and encrypted framing. The official ID
server option uses RustDesk's upstream rendezvous endpoint and public key. When
that option is disabled, configure an hbbs address (default port 21116), an
optional hbbr address (default port 21117), and the server public key shown by
hbbs. If the relay address is empty, the extension uses the address supplied by
hbbs or the hbbs port plus one. The API server field is retained for a complete
custom-server profile but is not contacted by this headless controlled endpoint.

The RustDesk ID, UUID, and signing seed are generated on first service start and
stored with mode 0600 in `ONEKVM_EXTENSION_DATA/identity.json`. The runtime logs
the generated ID and registration state. The layout exposes the generated ID as
a runtime-provided read-only value.

The ignored `official_hbbs_hbbr_secure_interop` test exercises a live official
hbbs/hbbr pair. It requires `RUSTDESK_TEST_HBBS`, `RUSTDESK_TEST_RELAY`,
`RUSTDESK_TEST_ID`, `RUSTDESK_TEST_KEY`, and `RUSTDESK_TEST_DATA`.
`RUSTDESK_TEST_PUBLIC_KEY` may be supplied instead of the target data path.

This repository is initialized independently from the OneKVM core and is
intended to remain a separately packaged extension. RustDesk-derived work must
comply with the GNU Affero General Public License version 3.

The packaged `web/rustdesk.svg` is RustDesk's upstream Flutter icon from
commit `807e05ea9a7e298ed2deb438195faaafce19cdd2` and is distributed under the
same AGPL-3.0 license.
