//! The network-audio wire format (decision 1 of the spec). Our own design:
//! nothing here comes from SonoBus or any other GPL code.
//!
//! One UDP datagram, little-endian:
//!
//! | bytes  | field                                            |
//! |--------|--------------------------------------------------|
//! | 0..4   | magic `TWNA`                                     |
//! | 4      | version (1)                                      |
//! | 5      | kind: 1 audio, 2 ping, 3 pong, 4 bye             |
//! | 6      | channels (audio: 1 or 2, else 0)                 |
//! | 7      | codec (audio: 1 = PCM16, else 0)                 |
//! | 8..12  | session id (random per sender run)               |
//! | 12..16 | sequence (audio packet number, or ping number)   |
//! | 16..24 | stamp (audio: sample rate in Hz; ping: sender  |
//! |        | clock in µs; pong echoes the ping's stamp)       |
//! | 24..26 | payload length                                   |
//! | 26..   | payload (audio samples, or the peer name in pings)|
//! | last 16| HMAC-SHA256 tag over everything before it        |
//!
//! Packets are signed with a key derived from the room passphrase, not
//! encrypted (decision 3). `encode` and `decode` never allocate.
use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};

pub const MAGIC: [u8; 4] = *b"TWNA";
pub const VERSION: u8 = 1;
pub const HEADER_LEN: usize = 26;
pub const TAG_LEN: usize = 16;
/// Frames per audio packet: 5 ms at 48 kHz.
pub const PACKET_FRAMES: usize = 240;
pub const MAX_CHANNELS: usize = 2;
pub const CODEC_PCM16: u8 = 1;
const PCM16_BYTES: usize = 2;
pub const MAX_PAYLOAD: usize = PACKET_FRAMES * MAX_CHANNELS * PCM16_BYTES;
pub const MAX_PACKET: usize = HEADER_LEN + MAX_PAYLOAD + TAG_LEN;
/// Longest peer name carried in a ping, in bytes.
pub const MAX_NAME_BYTES: usize = 64;
const KEY_CONTEXT: &[u8] = b"tiwaton-netaudio-v1";

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Audio = 1,
    Ping = 2,
    Pong = 3,
    Bye = 4,
}

impl Kind {
    fn from_byte(byte: u8) -> Option<Kind> {
        match byte {
            1 => Some(Kind::Audio),
            2 => Some(Kind::Ping),
            3 => Some(Kind::Pong),
            4 => Some(Kind::Bye),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Header {
    pub kind: Kind,
    pub channels: u8,
    pub codec: u8,
    pub session: u32,
    pub seq: u32,
    pub stamp: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProtocolError {
    TooShort,
    BadMagic,
    BadVersion,
    BadKind,
    BadLength,
    BadTag,
    BadAudio,
}

impl std::fmt::Display for ProtocolError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

/// The signing key for one room.
#[derive(Clone)]
pub struct RoomKey {
    key: [u8; 32],
}

impl RoomKey {
    /// SHA-256 of a fixed context string and the passphrase.
    pub fn from_passphrase(passphrase: &str) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(KEY_CONTEXT);
        hasher.update(passphrase.as_bytes());
        RoomKey {
            key: hasher.finalize().into(),
        }
    }

    fn mac(&self) -> HmacSha256 {
        // HMAC takes a key of any length; a 32-byte key never fails.
        <HmacSha256 as Mac>::new_from_slice(&self.key).expect("HMAC accepts any key length")
    }
}

impl std::fmt::Debug for RoomKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("RoomKey(..)")
    }
}

/// The 16-byte tag of `body`.
pub fn sign(key: &RoomKey, body: &[u8]) -> [u8; TAG_LEN] {
    let mut mac = key.mac();
    mac.update(body);
    let full = mac.finalize().into_bytes();
    let mut tag = [0u8; TAG_LEN];
    tag.copy_from_slice(&full[..TAG_LEN]);
    tag
}

fn verify(key: &RoomKey, body: &[u8], tag: &[u8]) -> bool {
    let mut mac = key.mac();
    mac.update(body);
    mac.verify_truncated_left(tag).is_ok()
}

/// An audio header must name a known codec, 1–2 channels, and carry whole
/// frames, at most `PACKET_FRAMES` of them.
fn audio_is_valid(header: &Header, payload_len: usize) -> bool {
    if header.kind != Kind::Audio {
        return true;
    }
    let channels = header.channels as usize;
    if header.codec != CODEC_PCM16 || channels == 0 || channels > MAX_CHANNELS {
        return false;
    }
    let frame_bytes = channels * PCM16_BYTES;
    payload_len > 0 && payload_len % frame_bytes == 0 && payload_len / frame_bytes <= PACKET_FRAMES
}

/// Write a signed packet into `out`; returns its length.
pub fn encode(
    key: &RoomKey,
    header: &Header,
    payload: &[u8],
    out: &mut [u8],
) -> Result<usize, ProtocolError> {
    if !audio_is_valid(header, payload.len()) {
        return Err(ProtocolError::BadAudio);
    }
    let body = HEADER_LEN + payload.len();
    if payload.len() > MAX_PAYLOAD || out.len() < body + TAG_LEN {
        return Err(ProtocolError::BadLength);
    }
    out[0..4].copy_from_slice(&MAGIC);
    out[4] = VERSION;
    out[5] = header.kind as u8;
    out[6] = header.channels;
    out[7] = header.codec;
    out[8..12].copy_from_slice(&header.session.to_le_bytes());
    out[12..16].copy_from_slice(&header.seq.to_le_bytes());
    out[16..24].copy_from_slice(&header.stamp.to_le_bytes());
    out[24..26].copy_from_slice(&(payload.len() as u16).to_le_bytes());
    out[HEADER_LEN..body].copy_from_slice(payload);
    let tag = sign(key, &out[..body]);
    out[body..body + TAG_LEN].copy_from_slice(&tag);
    Ok(body + TAG_LEN)
}

/// Check and parse one datagram: the header and a view of its payload.
/// The tag is checked before any field is trusted beyond the length.
pub fn decode<'a>(key: &RoomKey, packet: &'a [u8]) -> Result<(Header, &'a [u8]), ProtocolError> {
    if packet.len() < HEADER_LEN + TAG_LEN {
        return Err(ProtocolError::TooShort);
    }
    if packet[0..4] != MAGIC {
        return Err(ProtocolError::BadMagic);
    }
    if packet[4] != VERSION {
        return Err(ProtocolError::BadVersion);
    }
    let payload_len = u16::from_le_bytes([packet[24], packet[25]]) as usize;
    let body = HEADER_LEN + payload_len;
    if payload_len > MAX_PAYLOAD || packet.len() != body + TAG_LEN {
        return Err(ProtocolError::BadLength);
    }
    if !verify(key, &packet[..body], &packet[body..]) {
        return Err(ProtocolError::BadTag);
    }
    let kind = Kind::from_byte(packet[5]).ok_or(ProtocolError::BadKind)?;
    let header = Header {
        kind,
        channels: packet[6],
        codec: packet[7],
        session: u32::from_le_bytes([packet[8], packet[9], packet[10], packet[11]]),
        seq: u32::from_le_bytes([packet[12], packet[13], packet[14], packet[15]]),
        stamp: u64::from_le_bytes(packet[16..24].try_into().map_err(|_| ProtocolError::TooShort)?),
    };
    if !audio_is_valid(&header, payload_len) {
        return Err(ProtocolError::BadAudio);
    }
    Ok((header, &packet[HEADER_LEN..body]))
}

/// Samples → little-endian 16-bit PCM (clamped; NaN → 0). Returns the
/// bytes written: `min(samples, out / 2)` samples.
pub fn pcm16_encode(samples: &[f32], out: &mut [u8]) -> usize {
    let mut written = 0;
    for (sample, bytes) in samples.iter().zip(out.chunks_exact_mut(PCM16_BYTES)) {
        let clean = if sample.is_finite() { sample.clamp(-1.0, 1.0) } else { 0.0 };
        let value = (clean * 32_767.0).round() as i16;
        bytes.copy_from_slice(&value.to_le_bytes());
        written += PCM16_BYTES;
    }
    written
}

/// Little-endian 16-bit PCM → samples. Returns the samples written.
pub fn pcm16_decode(bytes: &[u8], out: &mut [f32]) -> usize {
    let mut written = 0;
    for (pair, sample) in bytes.chunks_exact(PCM16_BYTES).zip(out.iter_mut()) {
        *sample = (i16::from_le_bytes([pair[0], pair[1]]) as f32 / 32_767.0).max(-1.0);
        written += 1;
    }
    written
}

/// How far `to` is ahead of `from` in a wrapping sequence (negative when
/// behind).
pub fn seq_distance(from: u32, to: u32) -> i64 {
    to.wrapping_sub(from) as i32 as i64
}
