use std::{
    env, io,
    os::fd::{AsRawFd, FromRawFd, OwnedFd},
    path::PathBuf,
    ptr::NonNull,
    slice,
    sync::{
        atomic::{AtomicU32, Ordering},
        Arc,
    },
};

use bytes::Bytes;
use serde::Serialize;
use tokio::{
    io::unix::AsyncFd,
    io::{AsyncRead, AsyncReadExt, AsyncWriteExt},
    net::UnixStream,
};

const MEDIA_MAGIC: &[u8; 4] = b"OKVF";
const MEDIA_VERSION: u8 = 1;
const MEDIA_HEADER_LENGTH: usize = 40;
const MAX_MEDIA_PAYLOAD: usize = 8 << 20;
const RING_MAGIC: &[u8; 4] = b"OKVR";
const RING_HANDSHAKE: u8 = b'R';
const RING_VERSION: u32 = 1;
const RING_HEADER_LENGTH: usize = 64;
const RING_SLOT_HEADER_LENGTH: usize = 64;
const RING_STATE_FREE: u32 = 0;
const RING_STATE_READY: u32 = 2;
const RING_STATE_READING: u32 = 3;

#[derive(Clone, Debug)]
pub struct Identity {
    pub extension_id: String,
    pub token: String,
}

impl Identity {
    pub fn load() -> io::Result<Self> {
        let extension_id = env::var("ONEKVM_EXTENSION_ID").map_err(|_| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "ONEKVM_EXTENSION_ID is unavailable",
            )
        })?;
        let credentials = env::var_os("CREDENTIALS_DIRECTORY")
            .map(PathBuf::from)
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::NotFound,
                    "CREDENTIALS_DIRECTORY is unavailable",
                )
            })?;
        let token = std::fs::read_to_string(credentials.join("onekvm-token"))?
            .trim()
            .to_owned();
        if token.len() != 64 || !token.bytes().all(|value| value.is_ascii_hexdigit()) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "extension credential must contain 64 hexadecimal characters",
            ));
        }
        Ok(Self {
            extension_id,
            token,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VideoInfo {
    pub codec: Codec,
    pub width: u16,
    pub height: u16,
    pub fps: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Codec {
    H264,
    H265,
    Mjpeg,
}

#[derive(Clone, Debug)]
pub struct VideoProfile {
    pub codec: Codec,
    pub fps: u32,
    pub quality_factor: f64,
    pub bitrate_kbps: u32,
    pub initial_qp: u32,
    pub min_qp: u32,
    pub max_qp: u32,
    pub gop: u32,
}

#[derive(Debug)]
pub struct MediaFrame {
    pub sequence: u64,
    pub pts_usec: i64,
    pub duration_usec: u32,
    pub width: u16,
    pub height: u16,
    pub codec: Codec,
    pub keyframe: bool,
    pub payload: Bytes,
}

#[derive(Serialize)]
struct Subscription<'a> {
    version: u8,
    extension_id: &'a str,
    token: &'a str,
    video: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    fps: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    codec: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    quality_factor: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    bitrate_kbps: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    initial_qp: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    min_qp: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_qp: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    gop: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    transport: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    client_ip: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    client_user: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    client_transport: Option<&'a str>,
}

pub struct ClientMetadata<'a> {
    pub ip: &'a str,
    pub user: Option<&'a str>,
    pub transport: &'a str,
}

pub struct MediaSubscriber {
    stream: UnixStream,
    ring: Option<Arc<SharedRing>>,
    legacy_prefix: Option<u8>,
    last_sequence: u64,
    waiting_for_keyframe: bool,
}

impl MediaSubscriber {
    pub async fn connect(
        socket_path: &str,
        identity: &Identity,
        video: &str,
        profile: Option<&VideoProfile>,
        client: Option<&ClientMetadata<'_>>,
    ) -> io::Result<Self> {
        if video != "info" {
            if let Ok(subscriber) =
                Self::connect_transport(socket_path, identity, video, profile, client, true).await
            {
                return Ok(subscriber);
            }
        }
        Self::connect_transport(socket_path, identity, video, profile, client, false).await
    }

    async fn connect_transport(
        socket_path: &str,
        identity: &Identity,
        video: &str,
        profile: Option<&VideoProfile>,
        client: Option<&ClientMetadata<'_>>,
        shared_ring: bool,
    ) -> io::Result<Self> {
        let mut stream = UnixStream::connect(socket_path).await?;
        let metadata_only = video == "info";
        let mut request = serde_json::to_vec(&Subscription {
            version: 1,
            extension_id: &identity.extension_id,
            token: &identity.token,
            video,
            fps: (!metadata_only)
                .then(|| profile.map(|value| value.fps))
                .flatten(),
            codec: profile.map(|value| codec_name(value.codec)),
            quality_factor: (!metadata_only)
                .then(|| profile.map(|value| value.quality_factor))
                .flatten(),
            bitrate_kbps: (!metadata_only)
                .then(|| {
                    profile
                        .and_then(|value| (value.bitrate_kbps != 0).then_some(value.bitrate_kbps))
                })
                .flatten(),
            initial_qp: (!metadata_only)
                .then(|| {
                    profile.and_then(|value| (value.initial_qp != 0).then_some(value.initial_qp))
                })
                .flatten(),
            min_qp: (!metadata_only)
                .then(|| profile.and_then(|value| (value.min_qp != 0).then_some(value.min_qp)))
                .flatten(),
            max_qp: (!metadata_only)
                .then(|| profile.and_then(|value| (value.max_qp != 0).then_some(value.max_qp)))
                .flatten(),
            gop: (!metadata_only)
                .then(|| profile.and_then(|value| (value.gop != 0).then_some(value.gop)))
                .flatten(),
            transport: shared_ring.then_some("shm-ring-v1"),
            client_ip: client.map(|value| value.ip),
            client_user: client.and_then(|value| value.user),
            client_transport: client.map(|value| value.transport),
        })
        .map_err(io::Error::other)?;
        request.push(b'\n');
        stream.write_all(&request).await?;
        let (ring, legacy_prefix) = if shared_ring {
            match receive_shared_ring(&stream).await? {
                RingHandshake::Shared(ring) => (Some(ring), None),
                RingHandshake::Legacy(prefix) => (None, Some(prefix)),
            }
        } else {
            (None, None)
        };
        Ok(Self {
            stream,
            ring,
            legacy_prefix,
            last_sequence: 0,
            waiting_for_keyframe: shared_ring,
        })
    }

    pub async fn read_frame(&mut self) -> io::Result<MediaFrame> {
        if self.ring.is_none() {
            return read_media_frame_prefixed(&mut self.stream, self.legacy_prefix.take()).await;
        }
        loop {
            let ring = Arc::clone(self.ring.as_ref().expect("ring checked above"));
            let Some(frame) = take_next_frame(Arc::clone(&ring))? else {
                wait_for_ring(&self.stream, &ring).await?;
                continue;
            };
            if self.last_sequence != 0 && frame.sequence != self.last_sequence + 1 {
                self.waiting_for_keyframe = true;
                if !frame.keyframe {
                    self.request_key_frame().await?;
                }
            }
            self.last_sequence = frame.sequence;
            if self.waiting_for_keyframe {
                if !frame.keyframe {
                    continue;
                }
                self.waiting_for_keyframe = false;
            }
            return Ok(frame);
        }
    }

    async fn request_key_frame(&mut self) -> io::Result<()> {
        self.stream.write_all(b"{\"request\":\"keyframe\"}\n").await
    }
}

struct SharedRing {
    _memory_fd: OwnedFd,
    event_fd: AsyncFd<OwnedFd>,
    memory: NonNull<u8>,
    size: usize,
    slot_count: usize,
    slot_capacity: usize,
    stride: usize,
}

// The mapping and its descriptors are immutable after construction. Slot
// ownership is coordinated with cross-process atomics in the shared mapping.
unsafe impl Send for SharedRing {}
unsafe impl Sync for SharedRing {}

impl Drop for SharedRing {
    fn drop(&mut self) {
        unsafe {
            libc::munmap(self.memory.as_ptr().cast(), self.size);
        }
    }
}

struct RingLease {
    _ring: Arc<SharedRing>,
    state: NonNull<AtomicU32>,
    data: NonNull<u8>,
    size: usize,
}

unsafe impl Send for RingLease {}
unsafe impl Sync for RingLease {}

impl AsRef<[u8]> for RingLease {
    fn as_ref(&self) -> &[u8] {
        // The READING state prevents the producer from reusing this slot until
        // the final Bytes clone drops the lease.
        unsafe { slice::from_raw_parts(self.data.as_ptr(), self.size) }
    }
}

impl Drop for RingLease {
    fn drop(&mut self) {
        unsafe { self.state.as_ref() }.store(RING_STATE_FREE, Ordering::Release);
    }
}

enum RingHandshake {
    Shared(Arc<SharedRing>),
    Legacy(u8),
}

async fn receive_shared_ring(stream: &UnixStream) -> io::Result<RingHandshake> {
    let descriptors = loop {
        stream.readable().await?;
        match try_receive_ring_descriptors(stream.as_raw_fd()) {
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => continue,
            result => break result?,
        }
    };
    let descriptors = match descriptors {
        RingDescriptorHandshake::Shared(descriptors) => descriptors,
        RingDescriptorHandshake::Legacy(prefix) => {
            return Ok(RingHandshake::Legacy(prefix));
        }
    };
    let memory_fd = unsafe { OwnedFd::from_raw_fd(descriptors[0]) };
    let event_fd = unsafe { OwnedFd::from_raw_fd(descriptors[1]) };
    let mut status = unsafe { std::mem::zeroed::<libc::stat>() };
    if unsafe { libc::fstat(memory_fd.as_raw_fd(), &mut status) } != 0
        || status.st_size < RING_HEADER_LENGTH as libc::off_t
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid shared media ring size",
        ));
    }
    let size = status.st_size as usize;
    let mapped = unsafe {
        libc::mmap(
            std::ptr::null_mut(),
            size,
            libc::PROT_READ | libc::PROT_WRITE,
            libc::MAP_SHARED,
            memory_fd.as_raw_fd(),
            0,
        )
    };
    if mapped == libc::MAP_FAILED {
        return Err(io::Error::last_os_error());
    }
    let memory = NonNull::new(mapped.cast::<u8>()).expect("mmap returned a non-null mapping");
    let bytes = unsafe { slice::from_raw_parts(memory.as_ptr(), size) };
    let slot_count = read_le_u32(bytes, 8)? as usize;
    let slot_capacity = read_le_u32(bytes, 12)? as usize;
    let stride = read_le_u32(bytes, 16)? as usize;
    let header_length = read_le_u32(bytes, 20)? as usize;
    let valid = &bytes[..4] == RING_MAGIC
        && read_le_u32(bytes, 4)? == RING_VERSION
        && header_length == RING_HEADER_LENGTH
        && (2..=16).contains(&slot_count)
        && slot_capacity > 0
        && slot_capacity <= MAX_MEDIA_PAYLOAD
        && stride >= RING_SLOT_HEADER_LENGTH + slot_capacity
        && slot_count <= (size - header_length) / stride;
    if !valid {
        unsafe {
            libc::munmap(memory.as_ptr().cast(), size);
        }
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid shared media ring layout",
        ));
    }
    Ok(RingHandshake::Shared(Arc::new(SharedRing {
        _memory_fd: memory_fd,
        event_fd: AsyncFd::new(event_fd)?,
        memory,
        size,
        slot_count,
        slot_capacity,
        stride,
    })))
}

enum RingDescriptorHandshake {
    Shared([libc::c_int; 2]),
    Legacy(u8),
}

fn try_receive_ring_descriptors(fd: libc::c_int) -> io::Result<RingDescriptorHandshake> {
    let mut marker = 0_u8;
    let mut vector = libc::iovec {
        iov_base: (&mut marker as *mut u8).cast(),
        iov_len: 1,
    };
    let control_size = unsafe { libc::CMSG_SPACE((2 * std::mem::size_of::<libc::c_int>()) as _) };
    let mut control = vec![0_u8; control_size as usize];
    let mut message = unsafe { std::mem::zeroed::<libc::msghdr>() };
    message.msg_iov = &mut vector;
    message.msg_iovlen = 1;
    message.msg_control = control.as_mut_ptr().cast();
    message.msg_controllen = control.len() as _;
    let received = unsafe {
        libc::recvmsg(
            fd,
            &mut message,
            libc::MSG_CMSG_CLOEXEC | libc::MSG_DONTWAIT,
        )
    };
    if received < 0 {
        return Err(io::Error::last_os_error());
    }
    if received != 1 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid shared media ring handshake",
        ));
    }
    if marker != RING_HANDSHAKE {
        return Ok(RingDescriptorHandshake::Legacy(marker));
    }
    let header = unsafe { libc::CMSG_FIRSTHDR(&message) };
    if header.is_null()
        || unsafe { (*header).cmsg_level } != libc::SOL_SOCKET
        || unsafe { (*header).cmsg_type } != libc::SCM_RIGHTS
        || unsafe { (*header).cmsg_len }
            < unsafe { libc::CMSG_LEN((2 * std::mem::size_of::<libc::c_int>()) as _) } as _
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "shared media ring descriptors are missing",
        ));
    }
    let mut descriptors = [-1; 2];
    unsafe {
        std::ptr::copy_nonoverlapping(
            libc::CMSG_DATA(header).cast::<libc::c_int>(),
            descriptors.as_mut_ptr(),
            descriptors.len(),
        );
    }
    Ok(RingDescriptorHandshake::Shared(descriptors))
}

async fn wait_for_ring(stream: &UnixStream, ring: &SharedRing) -> io::Result<()> {
    loop {
        tokio::select! {
            ready = ring.event_fd.readable() => {
                let mut guard = ready?;
                match guard.try_io(|descriptor| {
                    let mut notifications = 0_u64;
                    let read = unsafe {
                        libc::read(
                            descriptor.get_ref().as_raw_fd(),
                            (&mut notifications as *mut u64).cast(),
                            std::mem::size_of::<u64>(),
                        )
                    };
                    if read < 0 {
                        Err(io::Error::last_os_error())
                    } else if read as usize == std::mem::size_of::<u64>() {
                        Ok(())
                    } else {
                        Err(io::Error::new(io::ErrorKind::UnexpectedEof, "short media ring notification"))
                    }
                }) {
                    Ok(result) => return result,
                    Err(_) => continue,
                }
            }
            ready = stream.readable() => {
                ready?;
                let mut byte = [0_u8; 1];
                match stream.try_read(&mut byte) {
                    Ok(0) => return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "media subscription closed")),
                    Ok(_) => return Err(io::Error::new(io::ErrorKind::InvalidData, "unexpected shared media control data")),
                    Err(error) if error.kind() == io::ErrorKind::WouldBlock => continue,
                    Err(error) => return Err(error),
                }
            }
        }
    }
}

fn take_next_frame(ring: Arc<SharedRing>) -> io::Result<Option<MediaFrame>> {
    // H.264/H.265 frames are interdependent. Drain ready slots from oldest to
    // newest and leave the remaining slots intact when one eventfd wakeup has
    // coalesced several producer notifications.
    for attempt in 0..2 {
        let mut selected = None;
        let mut oldest_sequence = u64::MAX;
        for index in 0..ring.slot_count {
            let offset = RING_HEADER_LENGTH + index * ring.stride;
            if atomic_at(&ring, offset).load(Ordering::Acquire) != RING_STATE_READY {
                continue;
            }
            let sequence = read_ring_u64(&ring, offset + 8)?;
            if selected.is_none() || sequence < oldest_sequence {
                selected = Some(index);
                oldest_sequence = sequence;
            }
        }
        let Some(selected) = selected else {
            if attempt == 0 {
                // Clear notification ownership, then rescan to close the race
                // with a producer that observed the previous value of one.
                atomic_at(&ring, 24).store(0, Ordering::Release);
                continue;
            }
            return Ok(None);
        };
        let selected_offset = RING_HEADER_LENGTH + selected * ring.stride;
        let state = atomic_at(&ring, selected_offset);
        if state
            .compare_exchange(
                RING_STATE_READY,
                RING_STATE_READING,
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .is_err()
        {
            continue;
        }
        let bytes = ring_bytes(&ring);
        let payload_size = read_le_u32(bytes, selected_offset + 4)? as usize;
        let sequence = read_le_u64(bytes, selected_offset + 8)?;
        let pts_usec = read_le_u64(bytes, selected_offset + 16)? as i64;
        let duration_usec = read_le_u32(bytes, selected_offset + 24)?;
        let width = read_le_u16(bytes, selected_offset + 28)?;
        let height = read_le_u16(bytes, selected_offset + 30)?;
        let codec = decode_codec(bytes[selected_offset + 32])?;
        let keyframe = bytes[selected_offset + 33] != 0;
        if payload_size == 0 || payload_size > ring.slot_capacity || width == 0 || height == 0 {
            state.store(RING_STATE_FREE, Ordering::Release);
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "invalid shared media ring frame",
            ));
        }
        let state = NonNull::from(state);
        let data = NonNull::new(unsafe {
            ring.memory
                .as_ptr()
                .add(selected_offset + RING_SLOT_HEADER_LENGTH)
        })
        .expect("ring payload pointer is non-null");
        let payload = Bytes::from_owner(RingLease {
            _ring: ring,
            state,
            data,
            size: payload_size,
        });
        return Ok(Some(MediaFrame {
            sequence,
            pts_usec,
            duration_usec,
            width,
            height,
            codec,
            keyframe,
            payload,
        }));
    }
    Ok(None)
}

fn ring_bytes(ring: &SharedRing) -> &[u8] {
    unsafe { slice::from_raw_parts(ring.memory.as_ptr(), ring.size) }
}

fn atomic_at(ring: &SharedRing, offset: usize) -> &AtomicU32 {
    assert_eq!(offset % std::mem::align_of::<AtomicU32>(), 0);
    unsafe { &*ring.memory.as_ptr().add(offset).cast::<AtomicU32>() }
}

fn read_ring_u64(ring: &SharedRing, offset: usize) -> io::Result<u64> {
    read_le_u64(ring_bytes(ring), offset)
}

fn read_le_u16(bytes: &[u8], offset: usize) -> io::Result<u16> {
    bytes
        .get(offset..offset + 2)
        .and_then(|value| value.try_into().ok())
        .map(u16::from_le_bytes)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "truncated media ring"))
}

fn read_le_u32(bytes: &[u8], offset: usize) -> io::Result<u32> {
    bytes
        .get(offset..offset + 4)
        .and_then(|value| value.try_into().ok())
        .map(u32::from_le_bytes)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "truncated media ring"))
}

fn read_le_u64(bytes: &[u8], offset: usize) -> io::Result<u64> {
    bytes
        .get(offset..offset + 8)
        .and_then(|value| value.try_into().ok())
        .map(u64::from_le_bytes)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "truncated media ring"))
}

fn decode_codec(value: u8) -> io::Result<Codec> {
    match value {
        1 => Ok(Codec::H264),
        2 => Ok(Codec::H265),
        3 => Ok(Codec::Mjpeg),
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid media codec",
        )),
    }
}

async fn read_media_frame<R>(reader: &mut R) -> io::Result<MediaFrame>
where
    R: AsyncRead + Unpin,
{
    read_media_frame_prefixed(reader, None).await
}

async fn read_media_frame_prefixed<R>(reader: &mut R, prefix: Option<u8>) -> io::Result<MediaFrame>
where
    R: AsyncRead + Unpin,
{
    let mut header = [0_u8; MEDIA_HEADER_LENGTH];
    let offset = if let Some(prefix) = prefix {
        header[0] = prefix;
        1
    } else {
        0
    };
    reader.read_exact(&mut header[offset..]).await?;
    if &header[..4] != MEDIA_MAGIC || header[4] != MEDIA_VERSION {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid OneKVM media frame header",
        ));
    }
    let codec = match header[5] {
        1 => Codec::H264,
        2 => Codec::H265,
        3 => Codec::Mjpeg,
        value => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("unsupported OneKVM codec id {value}"),
            ));
        }
    };
    let payload_length = u32::from_be_bytes(header[32..36].try_into().unwrap()) as usize;
    if payload_length > MAX_MEDIA_PAYLOAD {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "OneKVM media frame is too large",
        ));
    }
    let mut payload = vec![0; payload_length];
    reader.read_exact(&mut payload).await?;
    Ok(MediaFrame {
        sequence: u64::from_be_bytes(header[8..16].try_into().unwrap()),
        pts_usec: i64::from_be_bytes(header[16..24].try_into().unwrap()),
        duration_usec: u32::from_be_bytes(header[24..28].try_into().unwrap()),
        width: u16::from_be_bytes(header[28..30].try_into().unwrap()),
        height: u16::from_be_bytes(header[30..32].try_into().unwrap()),
        codec,
        keyframe: header[6] & 1 != 0,
        payload: payload.into(),
    })
}

pub async fn query_video_info(
    socket_path: &str,
    identity: &Identity,
    profile: &VideoProfile,
) -> io::Result<VideoInfo> {
    let mut subscriber =
        MediaSubscriber::connect(socket_path, identity, "info", Some(profile), None).await?;
    let frame = subscriber.read_frame().await?;
    if !frame.payload.is_empty()
        || frame.width == 0
        || frame.height == 0
        || frame.duration_usec == 0
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid OneKVM video metadata",
        ));
    }
    Ok(VideoInfo {
        codec: frame.codec,
        width: frame.width,
        height: frame.height,
        fps: (1_000_000_u32 / frame.duration_usec).max(1),
    })
}

pub fn parse_codec(value: &str) -> io::Result<Codec> {
    match value {
        "h264" => Ok(Codec::H264),
        "h265" => Ok(Codec::H265),
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("unsupported independent video codec {value}"),
        )),
    }
}

fn codec_name(codec: Codec) -> &'static str {
    match codec {
        Codec::H264 => "h264",
        Codec::H265 => "h265",
        Codec::Mjpeg => "mjpeg",
    }
}

#[derive(Clone, Debug)]
pub struct HidClient {
    socket_path: String,
    identity: Identity,
}

impl HidClient {
    pub fn new(socket_path: String, identity: Identity) -> Self {
        Self {
            socket_path,
            identity,
        }
    }

    pub async fn keyboard(&self, modifiers: u8, keys: &[u8]) -> io::Result<()> {
        self.post(
            "/api/hid/keyboard",
            &serde_json::json!({"modifiers": modifiers, "keys": keys}),
        )
        .await
    }

    pub async fn mouse(&self, buttons: u8, x: i8, y: i8, wheel: i8) -> io::Result<()> {
        self.post(
            "/api/hid/mouse",
            &serde_json::json!({"buttons": buttons, "x": x, "y": y, "wheel": wheel}),
        )
        .await
    }

    pub async fn absolute_mouse(&self, buttons: u16, x: u16, y: u16) -> io::Result<()> {
        self.post(
            "/api/hid/mouse/absolute",
            &serde_json::json!({"buttons": buttons, "x": x, "y": y}),
        )
        .await
    }

    async fn post(&self, path: &str, value: &serde_json::Value) -> io::Result<()> {
        let body = serde_json::to_vec(value).map_err(io::Error::other)?;
        let mut stream = UnixStream::connect(&self.socket_path).await?;
        let headers = format!(
            "POST {path} HTTP/1.1\r\nHost: unix\r\nContent-Type: application/json\r\nContent-Length: {}\r\nAuthorization: Bearer {}\r\nX-OneKVM-Extension: {}\r\nConnection: close\r\n\r\n",
            body.len(), self.identity.token, self.identity.extension_id
        );
        stream.write_all(headers.as_bytes()).await?;
        stream.write_all(&body).await?;
        stream.shutdown().await?;
        let mut response = Vec::with_capacity(512);
        stream.read_to_end(&mut response).await?;
        let status_line = response
            .split(|value| *value == b'\n')
            .next()
            .unwrap_or_default();
        if status_line.starts_with(b"HTTP/1.1 2") || status_line.starts_with(b"HTTP/1.0 2") {
            return Ok(());
        }
        Err(io::Error::other(format!(
            "OneKVM HID request failed: {}",
            String::from_utf8_lossy(status_line).trim()
        )))
    }
}

#[cfg(test)]
mod tests {
    use tokio::io::{duplex, AsyncWriteExt};

    use super::{read_media_frame, Codec, MEDIA_HEADER_LENGTH};

    #[tokio::test]
    async fn parses_media_frame() {
        let (mut writer, mut reader) = duplex(1024);
        let mut header = [0_u8; MEDIA_HEADER_LENGTH];
        header[..4].copy_from_slice(b"OKVF");
        header[4] = 1;
        header[5] = 1;
        header[6] = 1;
        header[16..24].copy_from_slice(&123_i64.to_be_bytes());
        header[24..28].copy_from_slice(&16_667_u32.to_be_bytes());
        header[28..30].copy_from_slice(&1920_u16.to_be_bytes());
        header[30..32].copy_from_slice(&1080_u16.to_be_bytes());
        header[32..36].copy_from_slice(&4_u32.to_be_bytes());
        writer.write_all(&header).await.unwrap();
        writer.write_all(&[0, 0, 0, 1]).await.unwrap();
        let frame = read_media_frame(&mut reader).await.unwrap();
        assert_eq!(frame.codec, Codec::H264);
        assert_eq!((frame.width, frame.height), (1920, 1080));
        assert!(frame.keyframe);
        assert_eq!(frame.duration_usec, 16_667);
    }
}
