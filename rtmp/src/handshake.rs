//! RTMP handshakes — plain and RTMPE (Adobe FP9 encrypted variant).
//!
//! Plain: 1+1536 byte echo per RTMP spec §5.2.
//!
//! RTMPE: DH-1024 key exchange embedded in the C1/S1 random bytes, HMAC-SHA256
//! digests bound to Adobe's "Genuine FP" / "Genuine FMS" constants, RC4
//! keystream applied to every chunk byte once the handshake completes.
//!
//! References:
//!   - rtmpdump's `librtmp/handshake.c`
//!   - ffmpeg's `libavformat/rtmpcrypt.c`
//!   - `src/handshake_rtmpe.odin`
//!
//! RC4 is hand-rolled (~10 lines).

use std::io::{Read, Write};
use std::net::TcpStream;

use hmac::{Hmac, Mac};
use num_bigint::BigUint;
use rand::RngCore;
use sha2::Sha256;

use crate::RtmpError;

pub const HANDSHAKE_SIZE: usize = 1536;
const RTMP_DIGEST_LEN: usize = 32;
const DH_KEY_LEN: usize = 128;

type HmacSha256 = Hmac<Sha256>;

// 1024-bit MODP prime from RFC 3526 group 2 — Adobe's choice for RTMPE.
const DH1024_P: [u8; DH_KEY_LEN] = [
    0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xC9, 0x0F, 0xDA, 0xA2, 0x21, 0x68, 0xC2, 0x34,
    0xC4, 0xC6, 0x62, 0x8B, 0x80, 0xDC, 0x1C, 0xD1, 0x29, 0x02, 0x4E, 0x08, 0x8A, 0x67, 0xCC, 0x74,
    0x02, 0x0B, 0xBE, 0xA6, 0x3B, 0x13, 0x9B, 0x22, 0x51, 0x4A, 0x08, 0x79, 0x8E, 0x34, 0x04, 0xDD,
    0xEF, 0x95, 0x19, 0xB3, 0xCD, 0x3A, 0x43, 0x1B, 0x30, 0x2B, 0x0A, 0x6D, 0xF2, 0x5F, 0x14, 0x37,
    0x4F, 0xE1, 0x35, 0x6D, 0x6D, 0x51, 0xC2, 0x45, 0xE4, 0x85, 0xB5, 0x76, 0x62, 0x5E, 0x7E, 0xC6,
    0xF4, 0x4C, 0x42, 0xE9, 0xA6, 0x37, 0xED, 0x6B, 0x0B, 0xFF, 0x5C, 0xB6, 0xF4, 0x06, 0xB7, 0xED,
    0xEE, 0x38, 0x6B, 0xFB, 0x5A, 0x89, 0x9F, 0xA5, 0xAE, 0x9F, 0x24, 0x11, 0x7C, 0x4B, 0x1F, 0xE6,
    0x49, 0x28, 0x66, 0x51, 0xEC, 0xE6, 0x53, 0x81, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
];

// "Genuine Adobe Flash Player 001" + 32-byte tail. HMAC key for C1 digest
// (first 30 bytes only) and for the C2 outer HMAC (all 62 bytes).
const GENUINE_FP_KEY: [u8; 62] = [
    0x47, 0x65, 0x6E, 0x75, 0x69, 0x6E, 0x65, 0x20, 0x41, 0x64, 0x6F, 0x62, 0x65, 0x20, 0x46, 0x6C,
    0x61, 0x73, 0x68, 0x20, 0x50, 0x6C, 0x61, 0x79, 0x65, 0x72, 0x20, 0x30, 0x30, 0x31, 0xF0, 0xEE,
    0xC2, 0x4A, 0x80, 0x68, 0xBE, 0xE8, 0x2E, 0x00, 0xD0, 0xD1, 0x02, 0x9E, 0x7E, 0x57, 0x6E, 0xEC,
    0x5D, 0x2D, 0x29, 0x80, 0x6F, 0xAB, 0x93, 0xB8, 0xE6, 0x36, 0xCF, 0xEB, 0x31, 0xAE,
];

// "Genuine Adobe Flash Media Server 001" + 32-byte tail. HMAC key for S1
// digest (first 36 bytes only) and for the S2 outer HMAC (all 68 bytes).
const GENUINE_FMS_KEY: [u8; 68] = [
    0x47, 0x65, 0x6E, 0x75, 0x69, 0x6E, 0x65, 0x20, 0x41, 0x64, 0x6F, 0x62, 0x65, 0x20, 0x46, 0x6C,
    0x61, 0x73, 0x68, 0x20, 0x4D, 0x65, 0x64, 0x69, 0x61, 0x20, 0x53, 0x65, 0x72, 0x76, 0x65, 0x72,
    0x20, 0x30, 0x30, 0x31, 0xF0, 0xEE, 0xC2, 0x4A, 0x80, 0x68, 0xBE, 0xE8, 0x2E, 0x00, 0xD0, 0xD1,
    0x02, 0x9E, 0x7E, 0x57, 0x6E, 0xEC, 0x5D, 0x2D, 0x29, 0x80, 0x6F, 0xAB, 0x93, 0xB8, 0xE6, 0x36,
    0xCF, 0xEB, 0x31, 0xAE,
];

// --- RC4 ------------------------------------------------------------------

#[derive(Clone)]
pub struct Rc4 {
    s: [u8; 256],
    i: u32,
    j: u32,
}

impl Rc4 {
    pub fn new(key: &[u8]) -> Self {
        assert!(!key.is_empty(), "RC4 key must be non-empty");
        let mut s = [0u8; 256];
        for (k, item) in s.iter_mut().enumerate() {
            *item = k as u8;
        }
        let mut j: u32 = 0;
        for k in 0..256 {
            j = (j + s[k] as u32 + key[k % key.len()] as u32) & 0xff;
            s.swap(k, j as usize);
        }
        Self { s, i: 0, j: 0 }
    }

    pub fn crypt(&mut self, data: &mut [u8]) {
        for b in data {
            self.i = (self.i + 1) & 0xff;
            self.j = (self.j + self.s[self.i as usize] as u32) & 0xff;
            self.s.swap(self.i as usize, self.j as usize);
            let idx = (self.s[self.i as usize] as u32 + self.s[self.j as usize] as u32) & 0xff;
            *b ^= self.s[idx as usize];
        }
    }
}

// --- HMAC-SHA256 helper ---------------------------------------------------

fn hmac_sha256(key: &[u8], data: &[u8]) -> [u8; 32] {
    let mut mac = HmacSha256::new_from_slice(key).expect("HMAC key length is unrestricted");
    mac.update(data);
    let out = mac.finalize().into_bytes();
    let mut buf = [0u8; 32];
    buf.copy_from_slice(&out);
    buf
}

// --- Schema offset calculators -------------------------------------------
// Two schemas exist for where the digest and DH pubkey live inside the
// 1536-byte C1/S1 random block. Schema 1: digest first, DH at end.
// Schema 2: DH first, digest at end. Adobe FP9 in encrypted mode uses
// schema 2 for C1; servers reply in either, so we detect S1's schema by
// HMAC trial.

fn digest_offset_1(buf: &[u8]) -> usize {
    let s = buf[8] as usize + buf[9] as usize + buf[10] as usize + buf[11] as usize;
    (s % 728) + 12
}
fn dh_offset_1(buf: &[u8]) -> usize {
    let s = buf[1532] as usize + buf[1533] as usize + buf[1534] as usize + buf[1535] as usize;
    (s % 632) + 772
}
fn digest_offset_2(buf: &[u8]) -> usize {
    let s = buf[772] as usize + buf[773] as usize + buf[774] as usize + buf[775] as usize;
    (s % 728) + 776
}
fn dh_offset_2(buf: &[u8]) -> usize {
    let s = buf[768] as usize + buf[769] as usize + buf[770] as usize + buf[771] as usize;
    (s % 632) + 8
}

fn verify_digest(buf: &[u8; HANDSHAKE_SIZE], digest_offset: usize, key: &[u8]) -> bool {
    let mut no_dig = [0u8; HANDSHAKE_SIZE - RTMP_DIGEST_LEN];
    no_dig[..digest_offset].copy_from_slice(&buf[..digest_offset]);
    no_dig[digest_offset..].copy_from_slice(&buf[digest_offset + RTMP_DIGEST_LEN..]);
    let want = hmac_sha256(key, &no_dig);
    want[..] == buf[digest_offset..digest_offset + RTMP_DIGEST_LEN]
}

// --- Plain handshake -------------------------------------------------------

pub fn plain_handshake(stream: &mut TcpStream) -> Result<(), RtmpError> {
    let mut c0c1 = [0u8; 1 + HANDSHAKE_SIZE];
    c0c1[0] = 0x03;
    // Bytes 1..4: timestamp (left zero — every server we've tested ignores
    // it). Bytes 5..8: zero (simple-handshake marker). Bytes 9..1537: random.
    let mut rng = rand::rng();
    rng.fill_bytes(&mut c0c1[9..]);

    tracing::debug!("rtmp handshake: sending C0+C1 ({} bytes)", c0c1.len());
    stream.write_all(&c0c1)?;

    let mut s0 = [0u8; 1];
    stream.read_exact(&mut s0)?;
    if s0[0] != 0x03 {
        return Err(RtmpError::HandshakeBadS0(s0[0]));
    }

    let mut s1 = [0u8; HANDSHAKE_SIZE];
    stream.read_exact(&mut s1)?;
    let mut s2 = [0u8; HANDSHAKE_SIZE];
    stream.read_exact(&mut s2)?;

    // C2 = echo of S1 (we don't validate S2 against C1 — servers vary).
    stream.write_all(&s1)?;
    tracing::debug!("rtmp handshake: done");
    Ok(())
}

// --- RTMPE handshake -------------------------------------------------------

/// On success, returns the two RC4 keystreams (`out` encrypts client→server,
/// `in` decrypts server→client) plus the leading-1536-byte "kick" already
/// applied. Callers should treat all subsequent chunk-stream bytes as
/// RC4-protected with these states.
pub struct RtmpeKeys {
    pub rc4_out: Rc4,
    pub rc4_in: Rc4,
}

pub fn rtmpe_handshake(stream: &mut TcpStream) -> Result<RtmpeKeys, RtmpError> {
    let mut rng = rand::rng();

    // --- DH-1024 keypair generation ---------------------------------------
    let p = BigUint::from_bytes_be(&DH1024_P);
    let g = BigUint::from(2u32);
    let mut priv_bytes = [0u8; DH_KEY_LEN];
    rng.fill_bytes(&mut priv_bytes);
    let x = BigUint::from_bytes_be(&priv_bytes);
    let xpub = g.modpow(&x, &p);
    let our_pub = bn_to_fixed::<DH_KEY_LEN>(&xpub);

    // --- Build C0 + C1 (schema 2) ----------------------------------------
    let mut c0c1 = [0u8; 1 + HANDSHAKE_SIZE];
    c0c1[0] = 0x06; // 0x06 = encrypted
    rng.fill_bytes(&mut c0c1[1..]);
    // Bytes 0..3 of C1: timestamp (left random — server ignores).
    // Bytes 4..7 of C1: FP9-encrypted version stamp.
    c0c1[1 + 4] = 0x80;
    c0c1[1 + 5] = 0x00;
    c0c1[1 + 6] = 0x03;
    c0c1[1 + 7] = 0x02;

    let dh_off = dh_offset_2(&c0c1[1..]);
    c0c1[1 + dh_off..1 + dh_off + DH_KEY_LEN].copy_from_slice(&our_pub);

    let dig_off = digest_offset_2(&c0c1[1..]);
    let mut no_dig = [0u8; HANDSHAKE_SIZE - RTMP_DIGEST_LEN];
    no_dig[..dig_off].copy_from_slice(&c0c1[1..1 + dig_off]);
    no_dig[dig_off..].copy_from_slice(&c0c1[1 + dig_off + RTMP_DIGEST_LEN..]);
    let client_digest = hmac_sha256(&GENUINE_FP_KEY[..30], &no_dig);
    c0c1[1 + dig_off..1 + dig_off + RTMP_DIGEST_LEN].copy_from_slice(&client_digest);

    tracing::debug!(
        "rtmpe handshake: sending C0+C1 (schema 2, dh_off={dh_off} dig_off={dig_off})"
    );
    stream.write_all(&c0c1)?;

    // --- Receive S0 + S1 + S2 -------------------------------------------
    let mut s0 = [0u8; 1];
    stream.read_exact(&mut s0)?;
    if s0[0] != 0x06 {
        // 0x03 here means the server doesn't speak RTMPE (returned plain).
        return Err(RtmpError::HandshakeBadS0(s0[0]));
    }
    let mut s1 = [0u8; HANDSHAKE_SIZE];
    stream.read_exact(&mut s1)?;
    let mut s2 = [0u8; HANDSHAKE_SIZE];
    stream.read_exact(&mut s2)?;

    // Detect S1's schema by HMAC trial.
    let (s1_schema, s1_dig_off) = {
        let off2 = digest_offset_2(&s1);
        if verify_digest(&s1, off2, &GENUINE_FMS_KEY[..36]) {
            (2u8, off2)
        } else {
            let off1 = digest_offset_1(&s1);
            if verify_digest(&s1, off1, &GENUINE_FMS_KEY[..36]) {
                (1u8, off1)
            } else {
                return Err(RtmpError::HandshakeBadDigest);
            }
        }
    };
    let mut server_digest = [0u8; RTMP_DIGEST_LEN];
    server_digest.copy_from_slice(&s1[s1_dig_off..s1_dig_off + RTMP_DIGEST_LEN]);

    let s1_dh_off = if s1_schema == 2 {
        dh_offset_2(&s1)
    } else {
        dh_offset_1(&s1)
    };
    tracing::debug!(
        "rtmpe handshake: S1 ok (schema={s1_schema} dh_off={s1_dh_off} dig_off={s1_dig_off})"
    );

    // --- Derive shared secret -------------------------------------------
    let server_pub_bytes = &s1[s1_dh_off..s1_dh_off + DH_KEY_LEN];
    let y = BigUint::from_bytes_be(server_pub_bytes);
    let z = y.modpow(&x, &p);
    let shared = bn_to_fixed::<DH_KEY_LEN>(&z);

    // --- Derive RC4 keys ------------------------------------------------
    //   key_out (client → server encrypt) = HMAC-SHA256(server_pub, key=Z)[:16]
    //   key_in  (server → client decrypt) = HMAC-SHA256(our_pub,    key=Z)[:16]
    let key_out_full = hmac_sha256(&shared, server_pub_bytes);
    let key_in_full = hmac_sha256(&shared, &our_pub);
    let mut rc4_out = Rc4::new(&key_out_full[..16]);
    let mut rc4_in = Rc4::new(&key_in_full[..16]);

    // RC4 keystream kick: advance each side by 1536 bytes and discard. This
    // skips the biased early keystream — Adobe FMS expects it.
    let mut kick = [0u8; HANDSHAKE_SIZE];
    rc4_out.crypt(&mut kick);
    kick = [0u8; HANDSHAKE_SIZE];
    rc4_in.crypt(&mut kick);

    // --- Build and send C2 (signed echo) --------------------------------
    let mut c2 = [0u8; HANDSHAKE_SIZE];
    rng.fill_bytes(&mut c2[..HANDSHAKE_SIZE - RTMP_DIGEST_LEN]);
    let temp_key = hmac_sha256(&GENUINE_FP_KEY, &server_digest);
    let c2_hmac = hmac_sha256(&temp_key, &c2[..HANDSHAKE_SIZE - RTMP_DIGEST_LEN]);
    c2[HANDSHAKE_SIZE - RTMP_DIGEST_LEN..].copy_from_slice(&c2_hmac);
    stream.write_all(&c2)?;

    // Best-effort S2 verification — many servers don't sign S2 properly,
    // so a mismatch is logged but not fatal.
    {
        let temp_key2 = hmac_sha256(&GENUINE_FMS_KEY, &client_digest);
        let expected = hmac_sha256(&temp_key2, &s2[..HANDSHAKE_SIZE - RTMP_DIGEST_LEN]);
        if expected[..] != s2[HANDSHAKE_SIZE - RTMP_DIGEST_LEN..] {
            tracing::debug!("rtmpe S2 HMAC mismatch — tolerating (many servers don't sign S2)");
        }
    }

    tracing::debug!("rtmpe handshake: done; RC4 keystreams armed");
    Ok(RtmpeKeys { rc4_out, rc4_in })
}

fn bn_to_fixed<const N: usize>(n: &BigUint) -> [u8; N] {
    let mut out = [0u8; N];
    let bytes = n.to_bytes_be();
    // Left-pad with leading zeros to exactly N bytes, matching BN_bn2binpad.
    let pad = N.saturating_sub(bytes.len());
    let take = bytes.len().min(N);
    out[pad..pad + take].copy_from_slice(&bytes[bytes.len() - take..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rc4_known_vector() {
        // RFC 6229 §2 test vector: key 0x0102030405, plaintext zeros.
        let mut rc4 = Rc4::new(&[0x01, 0x02, 0x03, 0x04, 0x05]);
        let mut buf = [0u8; 16];
        rc4.crypt(&mut buf);
        assert_eq!(
            buf,
            [
                0xb2, 0x39, 0x63, 0x05, 0xf0, 0x3d, 0xc0, 0x27, 0xcc, 0xc3, 0x52, 0x4a, 0x0a, 0x11,
                0x18, 0xa8,
            ]
        );
    }

    #[test]
    fn bn_pad_left() {
        let n = BigUint::from(0x12u32);
        let out = bn_to_fixed::<4>(&n);
        assert_eq!(out, [0, 0, 0, 0x12]);
    }
}
