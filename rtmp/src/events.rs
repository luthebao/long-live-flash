//! Inbound-event queue, drained by the host on each player tick.
//!
//! The RTMP worker/reader threads push events here; the host (desktop main
//! or capi) calls `drain()` to pull them out and translate into
//! `llflash_core::net_connection::NetConnections::dispatch_rtmp_*` calls.

use std::sync::Mutex;

#[derive(Debug, Clone)]
pub enum InboundEvent {
    /// A NetStatus event to deliver on the AS3 NetConnection object.
    Status {
        handle: u64,
        code: String,
        level: String,
    },
    /// Reply to a previous `NetConnection.call(...)`, dispatched to its
    /// Responder.
    CallResult {
        handle: u64,
        txid: u32,
        is_error: bool,
        body_amf: Vec<u8>,
    },
    /// Server-initiated invoke (e.g. `onBWDone`, `onMetaData`, app-specific
    /// callbacks) to deliver on `NetConnection.client`.
    ServerCall {
        handle: u64,
        method: String,
        args_amf: Vec<u8>,
    },
}

static QUEUE: Mutex<Vec<InboundEvent>> = Mutex::new(Vec::new());

pub fn push(ev: InboundEvent) {
    QUEUE.lock().unwrap().push(ev);
}

/// Drain every queued event. Cheap when empty (just a Mutex lock + len).
pub fn drain() -> Vec<InboundEvent> {
    let mut q = QUEUE.lock().unwrap();
    std::mem::take(&mut *q)
}
