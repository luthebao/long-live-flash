//! Wasm-side hooks that route `flash.net.NetConnection.connect("rtmp...")`
//! calls out to a JS-supplied bridge. In the web extension that bridge
//! forwards messages over `chrome.runtime.connectNative` to the
//! `llflash-rtmp-host` native messaging host, which owns the real TCP /
//! RTMPE socket. Outside the extension (selfhosted, demo) the bridge can
//! be left unset and the player falls back to the no-backend warning in
//! [`llflash_core::avm2::globals::flash::net::net_connection`].
//!
//! Threading model: wasm32 is single-threaded, so the bridge lives in a
//! `thread_local!` cell. The `fn` pointers installed via
//! `llflash_core::backend::net_connection::set_hooks` read that cell on
//! every call. Handles are allocated client-side from an atomic counter —
//! the native host stores a `client_handle -> internal rtmp_handle` map
//! so the AVM hook can stay synchronous even though the upstream dial is
//! async.

use base64::{Engine as _, engine::general_purpose::STANDARD as B64};
use js_sys::{Function, Reflect};
use std::cell::RefCell;
use std::sync::atomic::{AtomicU64, Ordering};
use wasm_bindgen::JsValue;

thread_local! {
    /// The JS callback registered via
    /// `RuffleInstanceBuilder::setRtmpBridge`. Invoked on every
    /// AVM `NetConnection` op against an RTMP scheme. When `None`, the
    /// hooks return 0 / no-op and `has_connect_hook` reports false from
    /// the wasm side (we never call `set_hooks` unless the bridge is set).
    static BRIDGE: RefCell<Option<Function>> = const { RefCell::new(None) };
}

/// Per-process monotonic handle allocator. Skips 0 because
/// `llflash_core::backend::net_connection::connect` treats 0 as "host
/// declined the URL".
static NEXT_HANDLE: AtomicU64 = AtomicU64::new(1);

/// Install the JS bridge function. Pass `None` (or omit / pass null from
/// JS) to clear. Idempotent — the most recent registration wins.
pub fn set_bridge(cb: Option<Function>) {
    BRIDGE.with(|b| *b.borrow_mut() = cb);
}

/// True when a bridge is currently installed. Used by
/// [`crate::builder`] to decide whether to wire the core net_connection
/// hooks at all; without a bridge they'd just return 0.
pub fn has_bridge() -> bool {
    BRIDGE.with(|b| b.borrow().is_some())
}

/// Hook installed into `llflash_core::backend::net_connection`. Allocates
/// a client handle, marshals the connect args into a JS object, and
/// dispatches to the registered bridge. Returning 0 here would tell the
/// AVM stub branch to fire, so we only do that if there's no bridge —
/// otherwise we always return a fresh handle and rely on the native
/// host to emit a NetStatus event if the actual dial fails.
pub fn web_connect(url: &str, swf_url: &str, page_url: &str, args_amf: &[u8]) -> u64 {
    if !has_bridge() {
        return 0;
    }
    let handle = NEXT_HANDLE.fetch_add(1, Ordering::Relaxed);
    let msg = match build_object(&[
        ("op", JsValue::from_str("connect")),
        ("handle", handle_to_jsvalue(handle)),
        ("url", JsValue::from_str(url)),
        ("swfUrl", JsValue::from_str(swf_url)),
        ("pageUrl", JsValue::from_str(page_url)),
        ("argsAmf", JsValue::from_str(&B64.encode(args_amf))),
    ]) {
        Some(m) => m,
        None => {
            tracing::error!("native_rtmp: failed to build connect message object");
            return 0;
        }
    };
    dispatch(&msg);
    handle
}

pub fn web_close(handle: u64) {
    if !has_bridge() {
        return;
    }
    let Some(msg) = build_object(&[
        ("op", JsValue::from_str("close")),
        ("handle", handle_to_jsvalue(handle)),
    ]) else {
        return;
    };
    dispatch(&msg);
}

pub fn web_call(handle: u64, txid: u32, payload_amf: &[u8]) {
    if !has_bridge() {
        return;
    }
    let Some(msg) = build_object(&[
        ("op", JsValue::from_str("call")),
        ("handle", handle_to_jsvalue(handle)),
        ("txid", JsValue::from_f64(txid as f64)),
        ("payloadAmf", JsValue::from_str(&B64.encode(payload_amf))),
    ]) else {
        return;
    };
    dispatch(&msg);
}

fn dispatch(msg: &JsValue) {
    BRIDGE.with(|b| {
        if let Some(cb) = b.borrow().as_ref() {
            // Pass `null` as `this` — bridge functions registered from JS
            // are expected to be free functions (or pre-bound methods).
            if let Err(e) = cb.call1(&JsValue::NULL, msg) {
                tracing::warn!("native_rtmp: bridge threw: {:?}", e);
            }
        }
    });
}

fn build_object(pairs: &[(&str, JsValue)]) -> Option<JsValue> {
    let obj = js_sys::Object::new();
    for (k, v) in pairs {
        if Reflect::set(&obj, &JsValue::from_str(k), v).is_err() {
            return None;
        }
    }
    Some(obj.into())
}

/// 64-bit handles don't fit in JS numbers safely beyond 2^53. The
/// extension JS deals in JS numbers (postMessage friendliness) so we
/// keep handles within the safe integer range — `NEXT_HANDLE` would have
/// to be hit a quintillion times before this matters.
fn handle_to_jsvalue(h: u64) -> JsValue {
    JsValue::from_f64(h as f64)
}
