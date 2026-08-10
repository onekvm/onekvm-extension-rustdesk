#!/usr/bin/env python3
"""Minimal RustDesk direct-IP interoperability probe for OneKVM."""

import argparse
import hashlib
import socket
import time


def varint(value: int) -> bytes:
    output = bytearray()
    while value >= 0x80:
        output.append((value & 0x7F) | 0x80)
        value >>= 7
    output.append(value)
    return bytes(output)


def field(number: int, value: bytes) -> bytes:
    return varint((number << 3) | 2) + varint(len(value)) + value


def integer(number: int, value: int) -> bytes:
    return varint(number << 3) + varint(value)


def signed_integer(number: int, value: int) -> bytes:
    return integer(number, (value << 1) ^ (value >> 31))


def parse_fields(payload: bytes) -> list[tuple[int, int, object]]:
    fields = []
    offset = 0
    while offset < len(payload):
        key, offset = read_varint(payload, offset)
        number, wire_type = key >> 3, key & 7
        if wire_type == 0:
            value, offset = read_varint(payload, offset)
        elif wire_type == 2:
            length, offset = read_varint(payload, offset)
            value = payload[offset : offset + length]
            offset += length
        elif wire_type == 1:
            value = payload[offset : offset + 8]
            offset += 8
        elif wire_type == 5:
            value = payload[offset : offset + 4]
            offset += 4
        else:
            raise ValueError(f"unsupported protobuf wire type {wire_type}")
        fields.append((number, wire_type, value))
    return fields


def read_varint(payload: bytes, offset: int = 0) -> tuple[int, int]:
    result = 0
    shift = 0
    while offset < len(payload):
        byte = payload[offset]
        offset += 1
        result |= (byte & 0x7F) << shift
        if byte < 0x80:
            return result, offset
        shift += 7
    raise EOFError("truncated varint")


def only(payload: bytes, number: int, default=None):
    for field_number, _, value in parse_fields(payload):
        if field_number == number:
            return value
    return default


def read_exact(stream: socket.socket, length: int) -> bytes:
    output = bytearray()
    while len(output) < length:
        part = stream.recv(length - len(output))
        if not part:
            raise EOFError("connection closed")
        output.extend(part)
    return bytes(output)


def read_packet(stream: socket.socket) -> bytes:
    first = read_exact(stream, 1)[0]
    header_length = (first & 3) + 1
    header = bytes([first]) + read_exact(stream, header_length - 1)
    return read_exact(stream, int.from_bytes(header, "little") >> 2)


def write_packet(stream: socket.socket, payload: bytes) -> None:
    length = len(payload)
    if length <= 0x3F:
        encoded, size = length << 2, 1
    elif length <= 0x3FFF:
        encoded, size = (length << 2) | 1, 2
    elif length <= 0x3FFFFF:
        encoded, size = (length << 2) | 2, 3
    else:
        encoded, size = (length << 2) | 3, 4
    stream.sendall(encoded.to_bytes(size, "little") + payload)


def password_hash(password: str, salt: str, challenge: str) -> bytes:
    first = hashlib.sha256(password.encode() + salt.encode()).digest()
    return hashlib.sha256(first + challenge.encode()).digest()


def login_request(password: bytes) -> bytes:
    decoding = integer(2, 1) + integer(3, 1)
    option = field(10, decoding)
    request = b"".join(
        [
            field(2, password),
            field(4, b"onekvm-probe"),
            field(5, b"OneKVM probe"),
            field(6, option),
            field(11, b"1.4.9"),
            field(13, b"Linux"),
        ]
    )
    return field(7, request)


def key_press(character: str) -> bytes:
    event = integer(2, 1) + integer(4, ord(character))
    return field(15, event)


def mouse_move(x: int, y: int, relative: bool = False) -> bytes:
    event = integer(1, 5 if relative else 0)
    event += signed_integer(2, x) + signed_integer(3, y)
    return field(10, event)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("host")
    parser.add_argument("--port", type=int, default=21118)
    parser.add_argument("--password", required=True)
    parser.add_argument("--seconds", type=float, default=3.0)
    parser.add_argument("--min-fps", type=float, default=0)
    parser.add_argument("--key", help="send one printable key after login")
    parser.add_argument("--mouse", help="send one absolute mouse position as X,Y")
    parser.add_argument("--relative-mouse", help="send one relative mouse delta as DX,DY")
    args = parser.parse_args()

    with socket.create_connection((args.host, args.port), timeout=5) as stream:
        challenge_message = read_packet(stream)
        challenge = only(challenge_message, 9)
        if challenge is None:
            raise RuntimeError("server did not send RustDesk Hash")
        salt = only(challenge, 1, b"").decode()
        nonce = only(challenge, 2, b"").decode()
        write_packet(
            stream,
            login_request(password_hash(args.password, salt, nonce)),
        )
        response_message = read_packet(stream)
        response = only(response_message, 8)
        if response is None:
            raise RuntimeError("server did not send LoginResponse")
        error = only(response, 1)
        if error is not None:
            raise RuntimeError(f"login failed: {error.decode(errors='replace')}")
        peer = only(response, 2)
        if peer is None:
            raise RuntimeError("LoginResponse does not contain PeerInfo")
        display = only(peer, 4)
        width = only(display, 3, 0) if display else 0
        height = only(display, 4, 0) if display else 0
        cursor_embedded = bool(only(display, 7, 0)) if display else False
        platform = only(peer, 3, b"").decode(errors="replace")
        version = only(peer, 7, b"").decode(errors="replace")
        if args.key:
            if len(args.key) != 1 or not args.key.isprintable():
                raise ValueError("--key must contain one printable character")
            write_packet(stream, key_press(args.key))
        if args.mouse:
            try:
                x, y = (int(value) for value in args.mouse.split(",", 1))
            except ValueError as error:
                raise ValueError("--mouse must have the form X,Y") from error
            write_packet(stream, mouse_move(x, y))
        if args.relative_mouse:
            try:
                x, y = (int(value) for value in args.relative_mouse.split(",", 1))
            except ValueError as error:
                raise ValueError("--relative-mouse must have the form DX,DY") from error
            write_packet(stream, mouse_move(x, y, relative=True))

        deadline = time.monotonic() + args.seconds
        frames = 0
        keyframes = 0
        video_bytes = 0
        codec = "unknown"
        stream.settimeout(max(args.seconds, 1.0))
        started_at = time.monotonic()
        while time.monotonic() < deadline:
            message = read_packet(stream)
            video = only(message, 6)
            if video is None:
                continue
            encoded_frames = only(video, 10)
            if encoded_frames is not None:
                codec = "H.264"
            else:
                encoded_frames = only(video, 11)
                if encoded_frames is not None:
                    codec = "H.265"
            if encoded_frames is None:
                continue
            for number, wire_type, encoded in parse_fields(encoded_frames):
                if number != 1 or wire_type != 2:
                    continue
                frames += 1
                data = only(encoded, 1, b"")
                video_bytes += len(data)
                keyframes += int(bool(only(encoded, 2, 0)))

    elapsed = time.monotonic() - started_at
    fps = frames / elapsed
    print(
        f"login=ok platform={platform} version={version} display={width}x{height} "
        f"cursor_embedded={cursor_embedded} "
        f"codec={codec} frames={frames} fps={fps:.2f} "
        f"keyframes={keyframes} bytes={video_bytes}"
    )
    return 0 if frames > 0 and video_bytes > 0 and fps >= args.min_fps else 1


if __name__ == "__main__":
    raise SystemExit(main())
