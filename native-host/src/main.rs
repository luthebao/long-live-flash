//! Chrome Native Messaging host for Llflash's web extension.
//!
//! Chrome / Chromium / Firefox spawn this binary on demand (via
//! `runtime.connectNative("com.longliveflash.rtmp_host")`) and bind its
//! stdin / stdout to the extension as length-prefixed JSON. We sit between
//! that pipe and the native [`llflash_rtmp`] worker — the same TCP-based
//! RTMP/RTMPE client the desktop player uses — and forward commands /
//! events in both directions.
//!
//! Wire format (per Chrome Native Messaging spec):
//!   - little-endian u32 length, followed by exactly that many bytes of
//!     UTF-8 JSON. Inbound and outbound use the same framing.
//!   - Max message size on either side is 1 MB; we hard-cap reads to that.
//!
//! JSON protocol:
//!
//!   Extension -> Host:
//!     { "op": "connect", "handle": <u64>, "url": "rtmp://...", "swfUrl": "...", "pageUrl": "...", "argsAmf": "<base64>" }
//!     { "op": "call",    "handle": <u64>, "txid": <u32>, "payloadAmf": "<base64>" }
//!     { "op": "close",   "handle": <u64> }
//!     { "op": "ping" }
//!
//!   Host -> Extension:
//!     { "ev": "ready", "version": "<crate version>" }       sent once at startup
//!     { "ev": "status",     "handle": <u64>, "code": "...", "level": "..." }
//!     { "ev": "callResult", "handle": <u64>, "txid": <u32>, "isError": <bool>, "bodyAmf": "<base64>" }
//!     { "ev": "serverCall", "handle": <u64>, "method": "...", "argsAmf": "<base64>" }
//!     { "ev": "pong" }
//!     { "ev": "log", "level": "warn|error", "msg": "..." }
//!
//! Handle management: the extension allocates handles client-side (a simple
//! monotonic counter) and passes them in every command. The host keeps a
//! `client_handle -> rtmp_handle` map; `llflash_rtmp::connect` returns its
//! own internal handle, and we translate. This lets the extension treat
//! handles as synchronous return values from the wasm hook even though the
//! native dial completes async.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use serde::Deserialize;
use serde_json::json;

/// Cap on a single Native Messaging frame, per the spec. We refuse to
/// allocate larger reads — a confused peer could otherwise OOM the host.
const MAX_FRAME_BYTES: u32 = 1024 * 1024;

fn main() {
    init_logging();
    tracing::info!(
        "llflash-rtmp-host starting (pid {})",
        std::process::id()
    );

    // Map<client_handle, rtmp_handle>. Populated synchronously on every
    // connect — the rtmp crate's NEXT_HANDLE is its own counter, separate
    // from whatever the extension allocated.
    let handles: Arc<Mutex<HashMap<u64, u64>>> = Arc::new(Mutex::new(HashMap::new()));
    // Reverse map for inbound events, which arrive keyed by rtmp_handle.
    let rev_handles: Arc<Mutex<HashMap<u64, u64>>> = Arc::new(Mutex::new(HashMap::new()));

    // Single shared stdout handle protected by a Mutex so the reader and
    // the event-pump thread can't interleave bytes mid-frame. We hold the
    // unlocked `Stdout` (which is Send+Sync) and call `.lock()` on each
    // write to get exclusive access to the underlying writer.
    let stdout = Arc::new(Mutex::new(std::io::stdout()));

    write_frame(&stdout, &json!({ "ev": "ready", "version": env!("CARGO_PKG_VERSION") }));

    // Event pump: drains llflash_rtmp's inbound queue and writes one
    // outbound frame per event. ~120Hz polling — about the upper bound at
    // which Flash content cares; cheaper than spawning a select-style
    // notifier just for this.
    {
        let stdout = stdout.clone();
        let rev_handles = rev_handles.clone();
        thread::Builder::new()
            .name("llflash-rtmp-host-pump".into())
            .spawn(move || event_pump(stdout, rev_handles))
            .expect("spawn event pump");
    }

    // Main thread reads stdin until the extension disconnects or stdin EOFs.
    let stdin = std::io::stdin();
    let mut stdin = stdin.lock();
    if let Err(e) = read_loop(&mut stdin, &stdout, &handles, &rev_handles) {
        tracing::warn!("read loop exited: {e}");
    }

    tracing::info!("llflash-rtmp-host shutting down");
    llflash_rtmp::shutdown();
}

/// Logging goes to stderr so it can't corrupt the stdout wire protocol.
/// Chrome captures stderr from native messaging hosts to the browser's
/// debug log on most platforms.
fn init_logging() {
    let filter = tracing_subscriber::EnvFilter::try_from_env("LLFLASH_RTMP_HOST_LOG")
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(filter)
        .init();
}

type Stdout = Arc<Mutex<std::io::Stdout>>;

fn read_loop<R: Read>(
    r: &mut R,
    stdout: &Stdout,
    handles: &Arc<Mutex<HashMap<u64, u64>>>,
    rev_handles: &Arc<Mutex<HashMap<u64, u64>>>,
) -> std::io::Result<()> {
    loop {
        let mut len_buf = [0u8; 4];
        if let Err(e) = r.read_exact(&mut len_buf) {
            if e.kind() == std::io::ErrorKind::UnexpectedEof {
                // Browser closed the port. Clean exit.
                return Ok(());
            }
            return Err(e);
        }
        let len = u32::from_le_bytes(len_buf);
        if len == 0 {
            continue;
        }
        if len > MAX_FRAME_BYTES {
            tracing::warn!("inbound frame too large ({len} bytes), dropping connection");
            return Ok(());
        }
        let mut buf = vec![0u8; len as usize];
        r.read_exact(&mut buf)?;
        match serde_json::from_slice::<InboundMsg>(&buf) {
            Ok(msg) => handle_message(msg, stdout, handles, rev_handles),
            Err(e) => {
                tracing::warn!("ignoring malformed JSON frame: {e}");
                send_log(stdout, "error", &format!("bad json from extension: {e}"));
            }
        }
    }
}

/// Translates JSON commands from the extension into [`llflash_rtmp`] calls.
/// The function is intentionally tolerant — bad input is logged and dropped
/// rather than killing the host, since restarting the native host loses
/// every live connection at once.
fn handle_message(
    msg: InboundMsg,
    stdout: &Stdout,
    handles: &Arc<Mutex<HashMap<u64, u64>>>,
    rev_handles: &Arc<Mutex<HashMap<u64, u64>>>,
) {
    match msg {
        InboundMsg::Connect {
            handle,
            url,
            swf_url,
            page_url,
            args_amf,
        } => {
            let args = decode_b64(&args_amf);
            let rtmp_handle =
                llflash_rtmp::connect(&url, &swf_url, &page_url, &args);
            if rtmp_handle == 0 {
                // Bad URL or unsupported scheme. llflash_rtmp already
                // logged; surface a synthetic failure status to the
                // extension so the AS3 NetConnection sees an event.
                send_status(stdout, handle, "NetConnection.Connect.Failed", "error");
            } else {
                handles.lock().unwrap().insert(handle, rtmp_handle);
                rev_handles.lock().unwrap().insert(rtmp_handle, handle);
            }
        }
        InboundMsg::Call { handle, txid, payload_amf } => {
            let payload = decode_b64(&payload_amf);
            if let Some(rtmp_handle) = handles.lock().unwrap().get(&handle).copied() {
                llflash_rtmp::call(rtmp_handle, txid, &payload);
            } else {
                send_log(stdout, "warn", &format!("call on unknown handle {handle}"));
            }
        }
        InboundMsg::Close { handle } => {
            let rtmp_handle = handles.lock().unwrap().remove(&handle);
            if let Some(rtmp_handle) = rtmp_handle {
                rev_handles.lock().unwrap().remove(&rtmp_handle);
                llflash_rtmp::close(rtmp_handle);
            }
        }
        InboundMsg::Ping {} => {
            write_frame(stdout, &json!({ "ev": "pong" }));
        }
    }
}

fn event_pump(stdout: Stdout, rev_handles: Arc<Mutex<HashMap<u64, u64>>>) {
    // 8ms polling = 125Hz, well above 60fps where Flash content tops out.
    // The drain is a mutex-lock + len-check so empty polls are cheap.
    let tick = Duration::from_millis(8);
    loop {
        let events = llflash_rtmp::drain();
        if events.is_empty() {
            thread::sleep(tick);
            continue;
        }
        let rev = rev_handles.lock().unwrap();
        for ev in events {
            match ev {
                llflash_rtmp::InboundEvent::Status { handle, code, level } => {
                    if let Some(&client_handle) = rev.get(&handle) {
                        send_status(&stdout, client_handle, &code, &level);
                    }
                }
                llflash_rtmp::InboundEvent::CallResult { handle, txid, is_error, body_amf } => {
                    if let Some(&client_handle) = rev.get(&handle) {
                        write_frame(
                            &stdout,
                            &json!({
                                "ev": "callResult",
                                "handle": client_handle,
                                "txid": txid,
                                "isError": is_error,
                                "bodyAmf": B64.encode(&body_amf),
                            }),
                        );
                    }
                }
                llflash_rtmp::InboundEvent::ServerCall { handle, method, args_amf } => {
                    if let Some(&client_handle) = rev.get(&handle) {
                        write_frame(
                            &stdout,
                            &json!({
                                "ev": "serverCall",
                                "handle": client_handle,
                                "method": method,
                                "argsAmf": B64.encode(&args_amf),
                            }),
                        );
                    }
                }
            }
        }
    }
}

fn send_status(stdout: &Stdout, client_handle: u64, code: &str, level: &str) {
    write_frame(
        stdout,
        &json!({
            "ev": "status",
            "handle": client_handle,
            "code": code,
            "level": level,
        }),
    );
}

fn send_log(stdout: &Stdout, level: &str, msg: &str) {
    write_frame(stdout, &json!({ "ev": "log", "level": level, "msg": msg }));
}

/// Length-prefixed write of a JSON value. The serialize step is fallible
/// only on `Value::Map` with non-string keys, which we never construct, so
/// `unwrap` is fine.
fn write_frame(stdout: &Stdout, msg: &serde_json::Value) {
    let bytes = serde_json::to_vec(msg).expect("serialize outbound");
    if bytes.len() > MAX_FRAME_BYTES as usize {
        tracing::error!(
            "refusing to emit oversized frame ({} bytes); event dropped",
            bytes.len()
        );
        return;
    }
    let len = (bytes.len() as u32).to_le_bytes();
    let guard = stdout.lock().unwrap();
    let mut out = guard.lock();
    if out.write_all(&len).is_err() || out.write_all(&bytes).is_err() {
        // Browser disconnected. Nothing useful we can do — the process
        // will exit on the next stdin read.
        return;
    }
    let _ = out.flush();
}

fn decode_b64(s: &str) -> Vec<u8> {
    B64.decode(s).unwrap_or_else(|e| {
        tracing::warn!("bad base64 payload, treating as empty: {e}");
        Vec::new()
    })
}

#[derive(Debug, Deserialize)]
#[serde(tag = "op", rename_all = "lowercase")]
enum InboundMsg {
    Connect {
        handle: u64,
        url: String,
        #[serde(default, rename = "swfUrl")]
        swf_url: String,
        #[serde(default, rename = "pageUrl")]
        page_url: String,
        #[serde(default, rename = "argsAmf")]
        args_amf: String,
    },
    Call {
        handle: u64,
        txid: u32,
        #[serde(rename = "payloadAmf")]
        payload_amf: String,
    },
    Close {
        handle: u64,
    },
    Ping {},
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_connect() {
        let raw = r#"{"op":"connect","handle":42,"url":"rtmp://a/b","swfUrl":"","pageUrl":"","argsAmf":""}"#;
        let msg: InboundMsg = serde_json::from_str(raw).unwrap();
        match msg {
            InboundMsg::Connect { handle, url, .. } => {
                assert_eq!(handle, 42);
                assert_eq!(url, "rtmp://a/b");
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn parses_call() {
        let raw = r#"{"op":"call","handle":7,"txid":3,"payloadAmf":"aGVsbG8="}"#;
        let msg: InboundMsg = serde_json::from_str(raw).unwrap();
        match msg {
            InboundMsg::Call { handle, txid, payload_amf } => {
                assert_eq!(handle, 7);
                assert_eq!(txid, 3);
                assert_eq!(decode_b64(&payload_amf), b"hello");
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn parses_close() {
        let raw = r#"{"op":"close","handle":99}"#;
        let msg: InboundMsg = serde_json::from_str(raw).unwrap();
        matches!(msg, InboundMsg::Close { handle: 99 });
    }

    #[test]
    fn parses_ping() {
        let raw = r#"{"op":"ping"}"#;
        let msg: InboundMsg = serde_json::from_str(raw).unwrap();
        matches!(msg, InboundMsg::Ping {});
    }
}
