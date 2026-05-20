//! High-level RTMP/RTMPE client: dial, handshake, send `connect`, await
//! `_result`, then drive subsequent messages on a per-connection reader
//! thread. The single worker thread serialises connect/call/close commands
//! posted by the host. Inbound events flow back through the global queue in
//! `crate::events`.
//!
//! Mirrors the worker + reader threads in `src/rtmp.odin`.

use std::collections::HashMap;
use std::net::TcpStream;
use std::sync::{Mutex, OnceLock};
use std::thread;

fn hex_block(buf: &[u8]) -> String {
    let mut s = String::with_capacity(buf.len() * 3);
    for (i, b) in buf.iter().enumerate() {
        if i > 0 && i % 16 == 0 {
            s.push('\n');
        }
        s.push_str(&format!("{b:02x} "));
    }
    s
}

use crate::amf0::{self, Value};
use crate::chunks::{self, Conn, ReadHalf, SharedWriter};
use crate::events::{self, InboundEvent};
use crate::handshake;
use crate::url::{self, RtmpUrl, Scheme};
use crate::RtmpError;

// --- Worker command channel ----------------------------------------------

enum Cmd {
    Connect {
        handle: u64,
        url: String,
        swf_url: String,
        page_url: String,
        extra_args_amf: Vec<u8>,
    },
    Call {
        handle: u64,
        txid: u32,
        payload: Vec<u8>,
    },
    Close {
        handle: u64,
    },
    Stop,
}

struct CmdQueue {
    queue: Mutex<Vec<Cmd>>,
    condvar: std::sync::Condvar,
}

impl CmdQueue {
    fn new() -> Self {
        Self {
            queue: Mutex::new(Vec::new()),
            condvar: std::sync::Condvar::new(),
        }
    }
    fn push(&self, cmd: Cmd) {
        let mut q = self.queue.lock().unwrap();
        q.push(cmd);
        self.condvar.notify_one();
    }
    fn pop(&self) -> Cmd {
        let mut q = self.queue.lock().unwrap();
        while q.is_empty() {
            q = self.condvar.wait(q).unwrap();
        }
        q.remove(0)
    }
}

static QUEUE: OnceLock<CmdQueue> = OnceLock::new();

fn queue() -> &'static CmdQueue {
    QUEUE.get_or_init(|| {
        let q = CmdQueue::new();
        thread::Builder::new()
            .name("ruffle-rtmp-worker".into())
            .spawn(worker_main)
            .expect("spawn rtmp worker");
        q
    })
}

pub fn post_connect(
    handle: u64,
    url: String,
    swf_url: String,
    page_url: String,
    extra_args_amf: Vec<u8>,
) {
    queue().push(Cmd::Connect {
        handle,
        url,
        swf_url,
        page_url,
        extra_args_amf,
    });
}

pub fn post_call(handle: u64, txid: u32, payload: Vec<u8>) {
    queue().push(Cmd::Call {
        handle,
        txid,
        payload,
    });
}

pub fn post_close(handle: u64) {
    queue().push(Cmd::Close { handle });
}

pub fn shutdown_worker() {
    if QUEUE.get().is_some() {
        queue().push(Cmd::Stop);
    }
}

// --- Worker connection registry ------------------------------------------

struct WorkerState {
    /// One writer per open handle; shared with the reader thread so it can
    /// bounce acks / pings back. The worker uses it to send call payloads.
    writers: HashMap<u64, SharedWriter>,
}

fn worker_main() {
    let mut state = WorkerState {
        writers: HashMap::new(),
    };
    loop {
        let cmd = queue().pop();
        match cmd {
            Cmd::Stop => return,
            Cmd::Connect {
                handle,
                url,
                swf_url,
                page_url,
                extra_args_amf,
            } => do_connect(&mut state, handle, &url, &swf_url, &page_url, &extra_args_amf),
            Cmd::Call {
                handle,
                txid,
                payload,
            } => do_call(&mut state, handle, txid, &payload),
            Cmd::Close { handle } => do_close(&mut state, handle),
        }
    }
}

fn do_connect(
    state: &mut WorkerState,
    handle: u64,
    url_str: &str,
    swf_url: &str,
    page_url: &str,
    extra_args_amf: &[u8],
) {
    match dial_and_connect(url_str, swf_url, page_url, extra_args_amf) {
        Ok((conn, code, level)) => {
            let (read_half, writer) = conn.into_halves();
            state.writers.insert(handle, writer);
            events::push(InboundEvent::Status {
                handle,
                code,
                level,
            });
            // Spawn the per-connection reader. It owns the read half; the
            // writer Arc lives on in state.writers (and inside the reader
            // for ack/ping replies).
            thread::Builder::new()
                .name(format!("ruffle-rtmp-reader-{handle}"))
                .spawn(move || reader_main(handle, read_half))
                .expect("spawn rtmp reader");
        }
        Err(e) => {
            tracing::error!("rtmp connect to {url_str} failed: {e}");
            events::push(InboundEvent::Status {
                handle,
                code: "NetConnection.Connect.Failed".into(),
                level: "error".into(),
            });
        }
    }
}

fn do_call(state: &mut WorkerState, handle: u64, txid: u32, payload: &[u8]) {
    let Some(writer) = state.writers.get(&handle).cloned() else {
        tracing::warn!("rtmp call on unknown handle {handle} (txid={txid})");
        return;
    };
    let mut w = writer.lock().unwrap();
    tracing::debug!("rtmp do_call handle={handle} txid={txid} {} bytes", payload.len());
    if let Err(e) = w.send_message(3, chunks::MSG_AMF0_COMMAND, 0, payload) {
        tracing::error!("rtmp call send failed (handle={handle} txid={txid}): {e}");
    }
}

fn do_close(state: &mut WorkerState, handle: u64) {
    if let Some(writer) = state.writers.remove(&handle) {
        writer.lock().unwrap().shutdown();
        events::push(InboundEvent::Status {
            handle,
            code: "NetConnection.Connect.Closed".into(),
            level: "status".into(),
        });
    }
}

// --- Dial + handshake + connect command ----------------------------------

fn dial_and_connect(
    url_str: &str,
    swf_url: &str,
    page_url: &str,
    extra_args_amf: &[u8],
) -> Result<(Conn, String, String), RtmpError> {
    let url = url::parse(url_str)?;
    match url.scheme {
        Scheme::Rtmp | Scheme::Rtmpe => {}
        s => return Err(RtmpError::UnsupportedScheme(s.as_str().into())),
    }

    let mut sock = TcpStream::connect((url.host.as_str(), url.port))?;
    sock.set_nodelay(true).ok();

    let rtmpe_keys = match url.scheme {
        Scheme::Rtmpe => {
            tracing::debug!("rtmp dial ok; starting RTMPE handshake (DH-1024 + RC4)");
            Some(handshake::rtmpe_handshake(&mut sock)?)
        }
        _ => {
            tracing::debug!("rtmp dial ok; starting plain handshake");
            handshake::plain_handshake(&mut sock)?;
            None
        }
    };

    let mut conn = Conn::new(sock)?;
    if let Some(k) = rtmpe_keys {
        conn.enable_rc4(k.rc4_in, k.rc4_out);
    }

    conn.send_window_ack_size(chunks::WINDOW_ACK_SIZE)?;
    conn.send_set_chunk_size(4096)?;

    let connect_payload = build_connect_payload(&url, swf_url, page_url, extra_args_amf);
    let effective_page_url = if page_url.is_empty() { swf_url } else { page_url };
    tracing::info!(
        "rtmp sending connect(app={:?}, tcUrl={:?}, swfUrl={:?}, pageUrl={:?}, \
         flashver={:?}, payload={} bytes, extra_args={} bytes)",
        url.app,
        url.tc_url,
        swf_url,
        effective_page_url,
        host_flashver(),
        connect_payload.len(),
        extra_args_amf.len()
    );
    if !extra_args_amf.is_empty() {
        tracing::debug!("rtmp extra_args_amf bytes: {}", hex_block(extra_args_amf));
    }
    conn.send_message(3, chunks::MSG_AMF0_COMMAND, 0, &connect_payload)?;

    let result = await_connect_result(&mut conn)?;
    let code = if !result.code.is_empty() {
        result.code
    } else if result.success {
        "NetConnection.Connect.Success".into()
    } else {
        "NetConnection.Connect.Failed".into()
    };
    let level = if !result.level.is_empty() {
        result.level
    } else if result.success {
        "status".into()
    } else {
        "error".into()
    };
    tracing::info!("rtmp connect result: code={code:?} level={level:?}");
    Ok((conn, code, level))
}

fn build_connect_payload(
    url: &RtmpUrl,
    swf_url: &str,
    page_url: &str,
    extra_args_amf: &[u8],
) -> Vec<u8> {
    let mut buf: Vec<u8> = Vec::with_capacity(256);

    amf0::encode(&mut buf, &amf0::s("connect"));
    amf0::encode(&mut buf, &amf0::n(1.0));

    // pageUrl defaults to swfUrl when the host has no embedding page (desktop
    // standalone), matching the projector behavior. Several game servers
    // hotlink-check pageUrl and reject an empty value.
    let effective_page_url = if page_url.is_empty() {
        swf_url
    } else {
        page_url
    };

    let cmd_obj = amf0::object_kv([
        ("app", amf0::s(&url.app)),
        ("flashver", amf0::s(host_flashver())),
        ("swfUrl", amf0::s(swf_url)),
        ("tcUrl", amf0::s(&url.tc_url)),
        ("fpad", amf0::b(false)),
        ("capabilities", amf0::n(239.0)),
        ("audioCodecs", amf0::n(3575.0)),
        ("videoCodecs", amf0::n(252.0)),
        ("videoFunction", amf0::n(1.0)),
        ("pageUrl", amf0::s(effective_page_url)),
        ("objectEncoding", amf0::n(0.0)),
    ]);
    amf0::encode(&mut buf, &cmd_obj);

    if !extra_args_amf.is_empty() {
        buf.extend_from_slice(extra_args_amf);
    }
    buf
}

/// Flash Player version string the RTMP layer impersonates in the `flashver`
/// connect-command field. Format is `"<OS> <maj>,<min>,<rev>,<build>"`
/// where OS is `WIN`, `MAC`, or `LNX`. The numbers match a recent Flash
/// Player 32 release; some servers parse the OS prefix and reject obvious
/// outliers.
fn host_flashver() -> &'static str {
    #[cfg(target_os = "windows")]
    {
        "WIN 32,0,0,114"
    }
    #[cfg(target_os = "macos")]
    {
        "MAC 32,0,0,114"
    }
    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    {
        "LNX 32,0,0,114"
    }
}

struct ConnectResult {
    success: bool,
    code: String,
    level: String,
}

fn await_connect_result(conn: &mut Conn) -> Result<ConnectResult, RtmpError> {
    loop {
        let msg = conn.read_message()?;
        if msg.msg_type != chunks::MSG_AMF0_COMMAND && msg.msg_type != chunks::MSG_AMF3_COMMAND {
            continue;
        }
        let payload: &[u8] = if msg.msg_type == chunks::MSG_AMF3_COMMAND && !msg.payload.is_empty()
        {
            &msg.payload[1..]
        } else {
            &msg.payload
        };
        tracing::debug!(
            "await_connect_result: msg type={} len={} head: {}",
            msg.msg_type,
            payload.len(),
            hex_block(&payload[..payload.len().min(48)])
        );

        let mut cur = amf0::Cursor::new(payload);
        let cmd = match amf0::decode(&mut cur) {
            Ok(Value::String(s)) => s,
            other => {
                tracing::debug!("await_connect_result: first value not a String: {other:?}");
                continue;
            }
        };
        tracing::debug!("await_connect_result: cmd={cmd:?}");
        if cmd != "_result" && cmd != "_error" {
            continue;
        }
        let _txid = amf0::decode(&mut cur).ok();
        let _props = amf0::decode(&mut cur).ok();
        let info = amf0::decode(&mut cur).ok();
        tracing::debug!("await_connect_result: info={info:#?}");

        let mut code = String::new();
        let mut level = String::new();
        if let Some(info) = info {
            if let Some(c) = info.get("code").and_then(|v| v.as_str()) {
                code = c.into();
            }
            if let Some(l) = info.get("level").and_then(|v| v.as_str()) {
                level = l.into();
            }
        }
        return Ok(ConnectResult {
            success: cmd == "_result",
            code,
            level,
        });
    }
}

// --- Per-connection reader thread ----------------------------------------

fn reader_main(handle: u64, mut read_half: ReadHalf) {
    loop {
        let msg = match read_half.read_message() {
            Ok(m) => m,
            Err(e) => {
                tracing::debug!("rtmp reader: handle={handle} ended: {e}");
                events::push(InboundEvent::Status {
                    handle,
                    code: "NetConnection.Connect.Closed".into(),
                    level: "status".into(),
                });
                return;
            }
        };
        handle_inbound(handle, msg);
    }
}

fn handle_inbound(handle: u64, msg: chunks::Message) {
    if msg.msg_type != chunks::MSG_AMF0_COMMAND && msg.msg_type != chunks::MSG_AMF3_COMMAND {
        // Audio/video/data — not dispatched in this MVP slice.
        return;
    }
    let payload: &[u8] = if msg.msg_type == chunks::MSG_AMF3_COMMAND && !msg.payload.is_empty() {
        &msg.payload[1..]
    } else {
        &msg.payload
    };

    let mut cur = amf0::Cursor::new(payload);
    let cmd_name = match amf0::decode(&mut cur) {
        Ok(Value::String(s)) => s,
        _ => return,
    };
    let txid_f = match amf0::decode(&mut cur) {
        Ok(Value::Number(n)) => n,
        _ => return,
    };
    let txid = txid_f as u32;

    if amf0::decode(&mut cur).is_err() {
        return;
    }
    let body = cur.rest().to_vec();

    match cmd_name.as_str() {
        "_result" | "_error" => {
            events::push(InboundEvent::CallResult {
                handle,
                txid,
                is_error: cmd_name == "_error",
                body_amf: body,
            });
        }
        _ => {
            events::push(InboundEvent::ServerCall {
                handle,
                method: cmd_name,
                args_amf: body,
            });
        }
    }
}
