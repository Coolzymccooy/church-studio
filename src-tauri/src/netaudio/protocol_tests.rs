use super::protocol::*;

fn key() -> RoomKey {
    RoomKey::from_passphrase("church-room-1")
}

fn audio_header(seq: u32, channels: u8) -> Header {
    Header {
        kind: Kind::Audio,
        channels,
        codec: CODEC_PCM16,
        session: 0xDEAD_BEEF,
        seq,
        stamp: 123_456,
    }
}

fn encode_audio(seq: u32, samples: &[f32], channels: u8) -> Vec<u8> {
    let mut payload = [0u8; MAX_PAYLOAD];
    let len = pcm16_encode(samples, &mut payload);
    let mut out = [0u8; MAX_PACKET];
    let n = encode(&key(), &audio_header(seq, channels), &payload[..len], &mut out).unwrap();
    out[..n].to_vec()
}

#[test]
fn audio_packet_round_trips() {
    let samples: Vec<f32> = (0..PACKET_FRAMES).map(|i| (i as f32 / 240.0) - 0.5).collect();
    let packet = encode_audio(7, &samples, 1);
    assert_eq!(packet.len(), HEADER_LEN + PACKET_FRAMES * 2 + TAG_LEN);
    let (header, payload) = decode(&key(), &packet).unwrap();
    assert_eq!(header, audio_header(7, 1));
    let mut decoded = [0.0f32; PACKET_FRAMES];
    assert_eq!(pcm16_decode(payload, &mut decoded), PACKET_FRAMES);
    for (a, b) in samples.iter().zip(decoded.iter()) {
        assert!((a - b).abs() < 1.0 / 16_000.0, "{a} vs {b}");
    }
}

#[test]
fn stereo_packet_fits_the_largest_datagram() {
    let samples = vec![0.25f32; PACKET_FRAMES * 2];
    let packet = encode_audio(1, &samples, 2);
    assert_eq!(packet.len(), MAX_PACKET);
    assert!(MAX_PACKET <= 1_200, "stays under a safe MTU");
    assert!(decode(&key(), &packet).is_ok());
}

#[test]
fn pcm16_clamps_and_keeps_full_scale() {
    let mut bytes = [0u8; 8];
    assert_eq!(pcm16_encode(&[2.0, -2.0, 1.0, -1.0], &mut bytes), 8);
    let mut back = [0.0f32; 4];
    pcm16_decode(&bytes, &mut back);
    assert_eq!(back, [1.0, -1.0, 1.0, -1.0]);
    // NaN never reaches the wire as noise.
    pcm16_encode(&[f32::NAN], &mut bytes);
    pcm16_decode(&bytes[..2], &mut back[..1]);
    assert_eq!(back[0], 0.0);
}

#[test]
fn ping_carries_a_name_and_pong_echoes_the_stamp() {
    let ping = Header {
        kind: Kind::Ping,
        channels: 0,
        codec: 0,
        session: 9,
        seq: 3,
        stamp: 42,
    };
    let mut out = [0u8; MAX_PACKET];
    let n = encode(&key(), &ping, "Pastor laptop".as_bytes(), &mut out).unwrap();
    let (header, payload) = decode(&key(), &out[..n]).unwrap();
    assert_eq!(header, ping);
    assert_eq!(payload, b"Pastor laptop");
}

#[test]
fn wrong_passphrase_is_rejected() {
    let packet = encode_audio(1, &[0.1; PACKET_FRAMES], 1);
    let other = RoomKey::from_passphrase("another-room");
    assert_eq!(decode(&other, &packet), Err(ProtocolError::BadTag));
}

#[test]
fn any_flipped_bit_is_rejected() {
    let packet = encode_audio(1, &[0.1; PACKET_FRAMES], 1);
    for byte in [0, 5, 9, 14, 20, 30, packet.len() - 1] {
        let mut bad = packet.clone();
        bad[byte] ^= 0x01;
        assert!(decode(&key(), &bad).is_err(), "byte {byte}");
    }
}

#[test]
fn malformed_packets_are_rejected() {
    let packet = encode_audio(1, &[0.1; PACKET_FRAMES], 1);
    assert_eq!(decode(&key(), &packet[..10]), Err(ProtocolError::TooShort));
    let mut magic = packet.clone();
    magic[0] = b'X';
    assert_eq!(decode(&key(), &magic), Err(ProtocolError::BadMagic));
    let mut version = packet.clone();
    version[4] = 99;
    assert_eq!(decode(&key(), &version), Err(ProtocolError::BadVersion));
    // Declared length longer than the datagram.
    assert_eq!(
        decode(&key(), &packet[..packet.len() - 4]),
        Err(ProtocolError::BadLength)
    );
    // Trailing bytes after the tag.
    let mut long = packet.clone();
    long.push(0);
    assert_eq!(decode(&key(), &long), Err(ProtocolError::BadLength));
}

#[test]
fn encode_refuses_bad_audio_headers() {
    let mut out = [0u8; MAX_PACKET];
    let payload = [0u8; PACKET_FRAMES * 2];
    let mut header = audio_header(1, 3);
    assert_eq!(
        encode(&key(), &header, &payload, &mut out),
        Err(ProtocolError::BadAudio)
    );
    header.channels = 1;
    header.codec = 9;
    assert_eq!(
        encode(&key(), &header, &payload, &mut out),
        Err(ProtocolError::BadAudio)
    );
    // Odd byte count is not whole 16-bit frames.
    header.codec = CODEC_PCM16;
    assert_eq!(
        encode(&key(), &header, &payload[..3], &mut out),
        Err(ProtocolError::BadAudio)
    );
    // Too much payload for the buffer.
    let mut small = [0u8; 40];
    assert_eq!(
        encode(&key(), &header, &payload, &mut small),
        Err(ProtocolError::BadLength)
    );
}

#[test]
fn signed_but_invalid_audio_is_still_rejected() {
    // A sender with the right key but a bad channel count is not trusted.
    let mut bytes = encode_audio(1, &[0.0; PACKET_FRAMES], 1);
    bytes[6] = 5; // channels
    let body = bytes.len() - TAG_LEN;
    let tag = sign(&key(), &bytes[..body]);
    bytes[body..].copy_from_slice(&tag);
    assert_eq!(decode(&key(), &bytes), Err(ProtocolError::BadAudio));
}

#[test]
fn sequence_distance_wraps() {
    assert_eq!(seq_distance(5, 5), 0);
    assert_eq!(seq_distance(5, 8), 3);
    assert_eq!(seq_distance(8, 5), -3);
    assert_eq!(seq_distance(u32::MAX, 1), 2);
    assert_eq!(seq_distance(1, u32::MAX), -2);
}
