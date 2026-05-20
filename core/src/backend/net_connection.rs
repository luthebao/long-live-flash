//! Static hook for routing `flash.net.NetConnection.connect("rtmp*://...")`
//! to a host-provided handler.
//!
//! This is intentionally a function-pointer hook rather than a full backend
//! trait — the only consumer right now is the `long-live-flash` Odin shell
//! and threading a `&mut dyn NetConnectionBackend` through `UpdateContext`,
//! `Player`, and `PlayerBuilder` is more invasive than this stop-gap.
//!
//! The host (capi crate) installs both halves at startup with `set_hooks`.
//! AVM2 / AVM1 `connect()` then calls `connect()` to launch a session and
//! gets back an opaque `u64` handle which is stored on the
//! `NetConnectionProtocol::Rtmp` variant. Status events arrive back via
//! `NetConnections::dispatch_rtmp_status`, driven from the host's tick.

use std::sync::Mutex;

/// Called on AVM `NetConnection.connect("rtmp*://...")`. The hook receives
/// the full original URL, the URL the loaded SWF came from, the URL of the
/// embedding page (empty if none), and the AMF0-serialised extra arguments
/// (everything after the URL in the AS3 call). The swf/page URLs are
/// forwarded into the RTMP `connect` command as `swfUrl` / `pageUrl` —
/// game servers commonly hotlink-check these. Returns an opaque handle
/// uniquely identifying the session; subsequent status/result events
/// reference it. Returning 0 means "host has no handler" — the caller
/// falls back to the pre-existing stub warning.
pub type ConnectFn = fn(url: &str, swf_url: &str, page_url: &str, args_amf: &[u8]) -> u64;

/// Called when the AVM-level NetConnection closes. The host should tear
/// down the session keyed by `handle`. Safe to drop on the floor if the
/// session has already been cleaned up.
pub type CloseFn = fn(handle: u64);

/// Called when AS3 invokes `NetConnection.call(command, responder, ...args)`
/// on an open RTMP connection. `payload_amf` is the full AMF0-encoded body:
/// command string, transaction id, null command object, then arguments.
pub type CallFn = fn(handle: u64, txid: u32, payload_amf: &[u8]);

#[derive(Copy, Clone, Default)]
struct Hooks {
    connect: Option<ConnectFn>,
    close:   Option<CloseFn>,
    call:    Option<CallFn>,
}

static HOOKS: Mutex<Hooks> = Mutex::new(Hooks { connect: None, close: None, call: None });

/// Install hooks. Any field set to `Some(_)` overwrites; `None` leaves it
/// untouched. The host typically installs all three at once.
pub fn set_hooks(connect: Option<ConnectFn>, close: Option<CloseFn>, call: Option<CallFn>) {
    let mut h = HOOKS.lock().unwrap();
    if let Some(c) = connect { h.connect = Some(c); }
    if let Some(c) = close   { h.close   = Some(c); }
    if let Some(c) = call    { h.call    = Some(c); }
}

pub fn connect(url: &str, swf_url: &str, page_url: &str, args_amf: &[u8]) -> u64 {
    let hook = HOOKS.lock().unwrap().connect;
    match hook {
        Some(f) => f(url, swf_url, page_url, args_amf),
        None => 0,
    }
}

pub fn close(handle: u64) {
    let hook = HOOKS.lock().unwrap().close;
    if let Some(f) = hook { f(handle) }
}

pub fn call(handle: u64, txid: u32, payload_amf: &[u8]) {
    let hook = HOOKS.lock().unwrap().call;
    if let Some(f) = hook { f(handle, txid, payload_amf) }
}

pub fn has_connect_hook() -> bool {
    HOOKS.lock().unwrap().connect.is_some()
}
