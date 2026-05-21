//! Native Rust RTMP/RTMPE client for the standalone Llflash desktop build.
//!
//! Llflash's core delegates RTMP-family URLs to a function-pointer hook in
//! [`llflash_core::backend::net_connection`]. The Odin shell installs its own
//! hook there; when this crate is used instead (i.e. for the pure-Rust
//! `llflash_desktop` binary), it installs equivalent hooks backed by the
//! Odin protocol port in this crate.
//!
//! Architecture (mirrors the Odin worker model):
//! - One background `ruffle-rtmp-worker` thread owns the connection map and
//!   serialises connect/call/close commands.
//! - Each open connection gets its own `ruffle-rtmp-reader-<handle>` thread
//!   that blocks on the socket and pushes parsed inbound messages onto the
//!   global event queue.
//! - The host drains the event queue from its render/tick loop and dispatches
//!   events via [`llflash_core::net_connection::NetConnections`].
//!
//! Supported schemes today: `rtmp://`, `rtmpe://`. `rtmps://`, `rtmpt://`,
//! and `rtmpte://` return an error (TLS / HTTP-tunneling not yet ported).

pub mod amf0;
mod chunks;
mod client;
mod events;
mod handshake;
mod url;

pub use events::{drain, InboundEvent};

use std::sync::atomic::{AtomicU64, Ordering};

/// Function-pointer types matching `llflash_core::backend::net_connection`.
/// The host crate is responsible for calling
/// `llflash_core::backend::net_connection::set_hooks` with our exported `fn`s
/// — we keep zero compile-time dependency on `llflash_core` so this crate is
/// independently testable.
pub type ConnectFn = fn(url: &str, swf_url: &str, page_url: &str, args_amf: &[u8]) -> u64;
pub type CloseFn = fn(handle: u64);
pub type CallFn = fn(handle: u64, txid: u32, payload_amf: &[u8]);

/// Connect to an RTMP / RTMPE URL. `swf_url` / `page_url` are forwarded into
/// the RTMP `connect` command as `swfUrl` / `pageUrl` so game servers that
/// hotlink-check them see the real player metadata. Returns an opaque
/// handle the host stashes on the AS3 NetConnection; later
/// status/result/server-call events reference this handle. Returning 0
/// means the URL was rejected outright (parse error, unsupported scheme)
/// and the host should treat that as "no native handler" — i.e. fall back
/// to the stub warning. Successful connects always return a non-zero
/// handle; the actual TCP dial happens asynchronously and ships a
/// NetStatus event when done.
pub fn connect(url: &str, swf_url: &str, page_url: &str, args_amf: &[u8]) -> u64 {
    // Pre-validate URL on the caller's thread so a malformed URL surfaces
    // immediately rather than as an async status event a frame later.
    let Ok(parsed) = url::parse(url) else {
        tracing::warn!("llflash_rtmp: rejecting unparseable URL {url:?}");
        return 0;
    };
    match parsed.scheme {
        url::Scheme::Rtmp | url::Scheme::Rtmpe => {}
        s => {
            tracing::warn!("llflash_rtmp: {} not yet supported", s.as_str());
            return 0;
        }
    }
    let handle = next_handle();
    tracing::info!(
        "llflash_rtmp::connect url={url:?} swf_url={swf_url:?} page_url={page_url:?} \
         handle={handle} extra_args={} bytes: {}",
        args_amf.len(),
        hex_block_full(args_amf),
    );
    client::post_connect(
        handle,
        url.to_string(),
        swf_url.to_string(),
        page_url.to_string(),
        args_amf.to_vec(),
    );
    handle
}

fn hex_block_full(buf: &[u8]) -> String {
    let mut s = String::with_capacity(buf.len() * 3);
    for (i, b) in buf.iter().enumerate() {
        if i > 0 && i % 16 == 0 {
            s.push('\n');
        }
        s.push_str(&format!("{b:02x} "));
    }
    s
}

pub fn close(handle: u64) {
    client::post_close(handle);
}

pub fn call(handle: u64, txid: u32, payload_amf: &[u8]) {
    client::post_call(handle, txid, payload_amf.to_vec());
}

pub fn shutdown() {
    client::shutdown_worker();
}

static NEXT_HANDLE: AtomicU64 = AtomicU64::new(1);

fn next_handle() -> u64 {
    NEXT_HANDLE.fetch_add(1, Ordering::Relaxed)
}

#[derive(Debug, thiserror::Error)]
pub enum RtmpError {
    #[error("bad RTMP URL: {0}")]
    BadUrl(String),
    #[error("unsupported RTMP scheme: {0}")]
    UnsupportedScheme(String),
    #[error("unexpected EOF on RTMP socket")]
    Eof,
    #[error("RTMP handshake: bad S0 byte 0x{0:02x}")]
    HandshakeBadS0(u8),
    #[error("RTMPE handshake: S1 digest invalid under both schemas")]
    HandshakeBadDigest,
    #[error("AMF0 decode: unexpected EOF")]
    AmfEof,
    #[error("AMF0 decode: bad marker 0x{0:02x}")]
    AmfBadMarker(u8),
    #[error("AMF0 decode: non-UTF8 string")]
    AmfBadString,
    #[error(transparent)]
    Io(#[from] std::io::Error),
}
