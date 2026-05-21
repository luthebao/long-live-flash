//! ERROR-level audit logging for every RTMP event flowing through the
//! desktop build.
//!
//! `llflash_core` calls into RTMP through three function-pointer hooks
//! (`connect` / `close` / `call`) — outbound traffic. Inbound traffic
//! (status, call results, server invokes) is drained from a global queue
//! once per tick in `app.rs`. To get a full audit trail at ERROR level
//! without modifying the `llflash_rtmp` crate, we:
//!
//! 1. Register the wrappers below as the hooks (instead of the raw
//!    `llflash_rtmp::*` fns). Each wrapper logs at ERROR then delegates.
//! 2. Have the tick loop call [`log_inbound`] on each drained event before
//!    dispatching it into AVM2.
//!
//! The hook signatures are bare `fn` pointers — these wrappers therefore
//! must be free functions with no captured state.

use llflash_rtmp::InboundEvent;

/// Max number of payload bytes to render as a hex preview. AMF blobs can
/// be many KB (e.g. a long argument list to `NetConnection.call`); printing
/// them in full would flood the log. 64 bytes captures the AMF marker +
/// the first string/number argument, which is what we typically need to
/// identify the call.
const HEX_PREVIEW_BYTES: usize = 64;

pub fn connect(url: &str, swf_url: &str, page_url: &str, args_amf: &[u8]) -> u64 {
    let handle = llflash_rtmp::connect(url, swf_url, page_url, args_amf);
    tracing::error!(
        target: "rtmp_audit",
        "OUTBOUND rtmp connect: handle={handle} url={url:?} swf_url={swf_url:?} \
         page_url={page_url:?} extra_args={} bytes [{}]",
        args_amf.len(),
        hex_preview(args_amf),
    );
    handle
}

pub fn close(handle: u64) {
    tracing::error!(
        target: "rtmp_audit",
        "OUTBOUND rtmp close: handle={handle}",
    );
    llflash_rtmp::close(handle);
}

pub fn call(handle: u64, txid: u32, payload_amf: &[u8]) {
    tracing::error!(
        target: "rtmp_audit",
        "OUTBOUND rtmp call: handle={handle} txid={txid} payload={} bytes [{}]",
        payload_amf.len(),
        hex_preview(payload_amf),
    );
    llflash_rtmp::call(handle, txid, payload_amf);
}

pub fn log_inbound(ev: &InboundEvent) {
    match ev {
        InboundEvent::Status { handle, code, level } => {
            tracing::error!(
                target: "rtmp_audit",
                "INBOUND  rtmp status: handle={handle} code={code:?} level={level:?}",
            );
        }
        InboundEvent::CallResult { handle, txid, is_error, body_amf } => {
            tracing::error!(
                target: "rtmp_audit",
                "INBOUND  rtmp call_result: handle={handle} txid={txid} is_error={is_error} \
                 body={} bytes [{}]",
                body_amf.len(),
                hex_preview(body_amf),
            );
        }
        InboundEvent::ServerCall { handle, method, args_amf } => {
            tracing::error!(
                target: "rtmp_audit",
                "INBOUND  rtmp server_call: handle={handle} method={method:?} args={} bytes [{}]",
                args_amf.len(),
                hex_preview(args_amf),
            );
        }
    }
}

fn hex_preview(buf: &[u8]) -> String {
    let n = buf.len().min(HEX_PREVIEW_BYTES);
    let mut s = String::with_capacity(n * 3 + 16);
    for b in &buf[..n] {
        s.push_str(&format!("{b:02x} "));
    }
    if buf.len() > n {
        s.push_str(&format!("... +{} more", buf.len() - n));
    }
    s
}
