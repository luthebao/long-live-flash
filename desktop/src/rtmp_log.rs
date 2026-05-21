//! Audit logging of RTMP method invocations.
//!
//! Lines are emitted at `INFO` on the `rtmp_audit` target. The default
//! `RUST_LOG` keeps them silent; `main.rs` injects `rtmp_audit=info` into
//! any explicit `RUST_LOG` the user sets, so `RUST_LOG=error` surfaces the
//! audit trail without flipping the severity of the lines themselves
//! (these are not errors — they're a transcript).
//!
//! `llflash_core` calls into RTMP through three function-pointer hooks
//! (`connect` / `close` / `call`) — outbound traffic. Inbound traffic
//! (status, call results, server invokes) is drained from a global queue
//! once per tick in `app.rs`.
//!
//! Wiring: these wrappers are registered as the hooks (instead of the raw
//! `llflash_rtmp::*` fns), and `app.rs` calls [`log_inbound`] on each
//! drained event before dispatching it into AVM2.
//!
//! Filter policy: log the session bookends and every method invocation —
//! outbound `connect`, outbound `call` (with decoded args), inbound
//! `status`, and inbound `server_call` (with decoded args). Outbound
//! `close` and inbound `call_result` run silently.
//!
//! Payloads are decoded from AMF0 into a compact one-line representation
//! (`{key: "value", n: 42}`). When decoding fails partway, the rendered
//! prefix is shown plus a `…(+N raw bytes)` tail, never raw hex — readers
//! can crack open the rotating log file for forensic detail if needed.

use llflash_rtmp::amf0::{self, Cursor, Value};
use llflash_rtmp::InboundEvent;

/// Hard cap on how many AMF0 values from a single payload we'll render
/// before truncating with `…`. Stops a chatty 50-arg server call from
/// blowing up the log line.
const MAX_ARGS_RENDERED: usize = 12;

/// Hard cap on the rendered length of a single string value. Long auth
/// tokens or HTML blobs become e.g. `"abc…(+312)"`.
const MAX_STRING_LEN: usize = 96;

pub fn connect(url: &str, swf_url: &str, page_url: &str, args_amf: &[u8]) -> u64 {
    let handle = llflash_rtmp::connect(url, swf_url, page_url, args_amf);
    tracing::info!(
        target: "rtmp_audit",
        "OUTBOUND rtmp connect: handle={handle} url={url:?} swf_url={swf_url:?} \
         page_url={page_url:?} extra_args={}",
        render_arg_stream(args_amf),
    );
    handle
}

pub fn close(handle: u64) {
    llflash_rtmp::close(handle);
}

pub fn call(handle: u64, txid: u32, payload_amf: &[u8]) {
    // Outbound call payload = [method_str, txid_num, command_obj, args...].
    // Decode head separately to surface the method even if a later arg
    // breaks, then render whatever args parse.
    let mut cur = Cursor::new(payload_amf);
    let method = match amf0::decode(&mut cur) {
        Ok(Value::String(s)) => s,
        _ => return, // not a well-formed call; drop silently
    };
    let _ = amf0::decode(&mut cur); // txid
    let _ = amf0::decode(&mut cur); // command object (usually null)
    let args = render_arg_stream(cur.rest());

    tracing::info!(
        target: "rtmp_audit",
        "OUTBOUND rtmp call: handle={handle} txid={txid} method={method:?} args={args}",
    );

    llflash_rtmp::call(handle, txid, payload_amf);
}

pub fn log_inbound(ev: &InboundEvent) {
    match ev {
        InboundEvent::Status { handle, code, level } => {
            tracing::info!(
                target: "rtmp_audit",
                "INBOUND  rtmp status: handle={handle} code={code:?} level={level:?}",
            );
        }
        InboundEvent::ServerCall { handle, method, args_amf } => {
            tracing::info!(
                target: "rtmp_audit",
                "INBOUND  rtmp server_call: handle={handle} method={method:?} args={}",
                render_arg_stream(args_amf),
            );
        }
        InboundEvent::CallResult { .. } => {}
    }
}

/// Decode a stream of consecutive AMF0 values and render as `[v1, v2, …]`.
/// Empty payloads render as `[]`. Decode errors render the values that
/// parsed plus a tail of how many raw bytes were left over.
fn render_arg_stream(buf: &[u8]) -> String {
    if buf.is_empty() {
        return "[]".into();
    }
    let mut cur = Cursor::new(buf);
    let mut out = String::from("[");
    let mut n = 0;
    loop {
        if cur.remaining() == 0 {
            break;
        }
        if n == MAX_ARGS_RENDERED {
            out.push_str(", …");
            break;
        }
        if n > 0 {
            out.push_str(", ");
        }
        match amf0::decode(&mut cur) {
            Ok(v) => render_value(&v, &mut out),
            Err(_) => {
                out.push_str(&format!("…(+{} raw bytes)", cur.remaining()));
                break;
            }
        }
        n += 1;
    }
    out.push(']');
    out
}

fn render_value(v: &Value, out: &mut String) {
    match v {
        Value::Null => out.push_str("null"),
        Value::Undefined => out.push_str("undefined"),
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Number(n) => render_number(*n, out),
        Value::String(s) => render_string(s, out),
        Value::Object(kv) | Value::EcmaArray(kv) => {
            out.push('{');
            for (i, (k, val)) in kv.iter().take(MAX_ARGS_RENDERED).enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                out.push_str(k);
                out.push_str(": ");
                render_value(val, out);
            }
            if kv.len() > MAX_ARGS_RENDERED {
                out.push_str(", …");
            }
            out.push('}');
        }
        Value::StrictArray(items) => {
            out.push('[');
            for (i, val) in items.iter().take(MAX_ARGS_RENDERED).enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                render_value(val, out);
            }
            if items.len() > MAX_ARGS_RENDERED {
                out.push_str(", …");
            }
            out.push(']');
        }
    }
}

fn render_number(n: f64, out: &mut String) {
    // Integer-valued doubles print without `.0` for readability — most AMF0
    // numbers are conceptually ints (txids, counters, enum codes).
    if n.is_finite() && n == n.trunc() && n.abs() < 1e16 {
        let as_i = n as i64;
        out.push_str(&as_i.to_string());
        // Surface Unix-epoch-ms numbers with an ISO date hint — these turn
        // up constantly in time-sync RPCs (e.g. getServerTime replies).
        // Range: 2001 (1e12) through 2128 (5e12).
        if (1_000_000_000_000..5_000_000_000_000).contains(&as_i) {
            if let Some(ts) = chrono::DateTime::from_timestamp_millis(as_i) {
                out.push_str(&format!(" /* {} */", ts.format("%Y-%m-%dT%H:%M:%S%.3fZ")));
            }
        }
    } else {
        out.push_str(&n.to_string());
    }
}

fn render_string(s: &str, out: &mut String) {
    if s.len() <= MAX_STRING_LEN {
        out.push_str(&format!("{s:?}"));
    } else {
        let head: String = s.chars().take(MAX_STRING_LEN).collect();
        let extra = s.len() - head.len();
        out.push_str(&format!("{head:?}").trim_end_matches('"'));
        out.push_str(&format!("…(+{extra})\""));
    }
}
