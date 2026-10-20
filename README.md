# OneKVM RustDesk Extension

English | [简体中文](README.zh-CN.md)

Connect a RustDesk client to the computer captured and controlled by OneKVM,
not to the operating system running on the OneKVM device. The protocol runtime
consumes OneKVM's encoded video and routes input through its authenticated
extension control socket.

> **Build note:** This repository contains extension sources only. Release
> packages are built by the OpenEmbedded recipes in `onekvm-distro`; this
> repository does not provide a standalone release build.

## Features

- Stable 10-digit RustDesk ID and persistent Ed25519 identity
- hbbs registration and keep-alive over UDP port 21116
- hbbr relay sessions over TCP port 21117, including relay fallback after a
  failed hole-punch request
- Signed identity exchange and XSalsa20Poly1305 encrypted ID/relay sessions
- Optional headless direct-IP endpoint on TCP port 21118
- H.264 or H.265 passthrough without decoding or re-encoding
- Keyboard and absolute or relative pointer input
- Configurable frame rate, quality budget, bitrate, QP, GOP, and client limit

The extension does not provide file transfer, clipboard, terminal, port
forwarding, or device power operations.

## Install and enable

The package name is `onekvm-extension-rustdesk`. Install it through your
OneKVM distribution package or image, then manage it with the plugin manager:

```sh
onekvm-extension enable rustdesk
onekvm-extension disable rustdesk
```

The RustDesk service is off by default. Enable it in the extension settings
after selecting an ID server configuration.

## Server configuration

Choose one of these profiles in the **RustDesk** extension settings:

- **Official ID server:** uses RustDesk's upstream rendezvous endpoint and
  public key.
- **Custom server:** configure the hbbs address, normally on port `21116`, its
  public key, and optionally the hbbr address, normally on port `21117`. If the
  relay address is empty, the extension uses the address supplied by hbbs or
  the hbbs port plus one.

The API server field is retained to describe a complete custom-server profile,
but this headless controlled endpoint does not contact it. The generated
RustDesk ID is shown as a read-only value after the first service start.

The optional direct-IP listener defaults to `0.0.0.0:21118`. Direct-IP
handshakes do not encrypt the session and are intended only for trusted LANs.
ID and relay connections use RustDesk's signed public-key handshake and
encrypted framing. Configure a password before exposing either connection
method to other users.

## Identity and media

The RustDesk ID, UUID, and signing seed are generated on first start and stored
with mode `0600` in `ONEKVM_EXTENSION_DATA/identity.json`. The default client
limit is one and can be set from 1 to 8.

The extension requests hardware-encoded H.264 or H.265 from OneKVM and forwards
it without a decode/re-encode step. Encoder settings on the extension page
control frame rate, resource budget, bitrate, QP, and keyframe interval.

## Development and interoperability testing

The runtime is a Rust 2021 project and requires Rust 1.75 or newer. Run its
normal test suite with:

```sh
cargo test
```

The ignored `official_hbbs_hbbr_secure_interop` test exercises a live official
hbbs/hbbr pair. It requires `RUSTDESK_TEST_HBBS`, `RUSTDESK_TEST_RELAY`,
`RUSTDESK_TEST_ID`, `RUSTDESK_TEST_KEY`, and `RUSTDESK_TEST_DATA`.
`RUSTDESK_TEST_PUBLIC_KEY` may be supplied instead of the target data path.

## License

This project contains RustDesk-derived work and is licensed under the GNU
Affero General Public License v3.0 only. See [`LICENSE`](LICENSE).

The packaged [`web/rustdesk.svg`](web/rustdesk.svg) is RustDesk's upstream
Flutter icon from commit `807e05ea9a7e298ed2deb438195faaafce19cdd2` and is
distributed under the same AGPL-3.0 license.
