use crate::Player;
use crate::avm1::Object as Avm1Object;
use crate::avm1::globals::netconnection::NetConnection as Avm1NetConnectionObject;
use crate::avm2::object::{
    NetConnectionObject as Avm2NetConnectionObject, ResponderObject as Avm2ResponderObject,
};
use crate::avm2::{Activation as Avm2Activation, Avm2, EventObject as Avm2EventObject};
use crate::backend::navigator::{
    ErrorResponse, FetchReason, NavigatorBackend, OwnedFuture, Request,
};
use crate::backend::net_connection as net_connection_backend;
use crate::context::UpdateContext;
use crate::loader::Error;
use flash_lso::packet::{Header, Message, Packet};
use flash_lso::types::{AMFVersion, Element, Value as AmfValue};
use gc_arena::{Collect, DynamicRoot, Gc, Rootable};
use slotmap::{SlotMap, new_key_type};
use std::fmt::{Debug, Formatter};
use std::rc::Rc;
use std::sync::{Arc, Mutex};

new_key_type! {
    pub struct NetConnectionHandle;
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub enum ResponderCallback {
    Result,
    Status,
}

#[derive(Clone)]
pub enum ResponderHandle {
    Avm2(DynamicRoot<Rootable![Avm2ResponderObject<'_>]>),
    Avm1(DynamicRoot<Rootable![Avm1Object<'_>]>),
}

impl Debug for ResponderHandle {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            ResponderHandle::Avm2(_) => write!(f, "ResponderHandle::Avm2"),
            ResponderHandle::Avm1(_) => write!(f, "ResponderHandle::Avm1"),
        }
    }
}

impl ResponderHandle {
    pub fn call(
        &self,
        context: &mut UpdateContext<'_>,
        callback: ResponderCallback,
        message: Rc<AmfValue>,
    ) {
        match self {
            ResponderHandle::Avm2(handle) => {
                let object = context.dynamic_root.fetch(handle);
                let mut activation = Avm2Activation::from_nothing(context);

                if let Err(err) = object.send_callback(&mut activation, callback, &message) {
                    Avm2::uncaught_error(
                        &mut activation,
                        None, // TODO we need to set this, but how?
                        err,
                        "Error running AVM2 NetConnection callback",
                    );
                }
            }
            ResponderHandle::Avm1(handle) => {
                let object = context.dynamic_root.fetch(handle);
                if let Err(e) =
                    Avm1NetConnectionObject::send_callback(context, *object, callback, &message)
                {
                    tracing::error!("Unhandled error sending {callback:?} callback: {e}");
                }
            }
        }
    }
}

#[derive(Copy, Clone, Collect)]
#[collect(no_drop)]
pub enum NetConnectionObject<'gc> {
    Avm2(Avm2NetConnectionObject<'gc>),
    Avm1(Avm1Object<'gc>),
}

impl NetConnectionObject<'_> {
    pub fn set_handle(&self, handle: Option<NetConnectionHandle>) -> Option<NetConnectionHandle> {
        match self {
            NetConnectionObject::Avm2(object) => object.set_handle(handle),
            NetConnectionObject::Avm1(object) => {
                if let Some(net_connection) = Avm1NetConnectionObject::cast((*object).into()) {
                    net_connection.set_handle(handle)
                } else {
                    None
                }
            }
        }
    }
}

impl<'gc> From<Avm2NetConnectionObject<'gc>> for NetConnectionObject<'gc> {
    fn from(value: Avm2NetConnectionObject<'gc>) -> Self {
        NetConnectionObject::Avm2(value)
    }
}

impl<'gc> From<Avm1Object<'gc>> for NetConnectionObject<'gc> {
    fn from(value: Avm1Object<'gc>) -> Self {
        NetConnectionObject::Avm1(value)
    }
}

/// Manages the collection of NetConnections.
#[derive(Collect)]
#[collect(no_drop)]
pub struct NetConnections<'gc> {
    connections: SlotMap<NetConnectionHandle, NetConnection<'gc>>,
}

impl Default for NetConnections<'_> {
    fn default() -> Self {
        Self {
            connections: SlotMap::with_key(),
        }
    }
}

impl<'gc> NetConnections<'gc> {
    pub fn connect_to_local<O: Into<NetConnectionObject<'gc>>>(
        context: &mut UpdateContext<'gc>,
        target: O,
    ) {
        let target = target.into();
        let connection = NetConnection {
            object: target,
            protocol: NetConnectionProtocol::Local,
        };
        let handle = context.net_connections.connections.insert(connection);

        if let Some(existing_handle) = target.set_handle(Some(handle)) {
            NetConnections::close(context, existing_handle, false);
        }

        match target {
            NetConnectionObject::Avm2(object) => {
                let mut activation = Avm2Activation::from_nothing(context);
                let event = Avm2EventObject::net_status_event(
                    &mut activation,
                    [
                        ("code", "NetConnection.Connect.Success"),
                        ("level", "status"),
                    ],
                );
                Avm2::dispatch_event(activation.context, event, object.into());
            }
            NetConnectionObject::Avm1(object) => {
                if let Err(e) = Avm1NetConnectionObject::on_status_event(
                    context,
                    object,
                    "NetConnection.Connect.Success",
                ) {
                    tracing::error!("Unhandled error sending connection callback: {e}");
                }
            }
        }
    }

    /// Open a connection to an RTMP-family server. The actual socket lives
    /// in the Odin RTMP worker; this side just stores the opaque handle the
    /// worker returned and stays around to receive status/result events.
    /// Status events arrive via `dispatch_rtmp_status`, called from the
    /// capi tick before `Player::tick`.
    pub fn connect_to_rtmp<O: Into<NetConnectionObject<'gc>>>(
        context: &mut UpdateContext<'gc>,
        target: O,
        url: String,
        odin_handle: u64,
    ) {
        let target = target.into();
        let connection = NetConnection {
            object: target,
            protocol: NetConnectionProtocol::Rtmp(RtmpConnection {
                url,
                odin_handle,
                next_txid: 2, // connect() was txid=1 on the Odin side
                pending: std::collections::HashMap::new(),
            }),
        };
        let handle = context.net_connections.connections.insert(connection);

        if let Some(existing_handle) = target.set_handle(Some(handle)) {
            NetConnections::close(context, existing_handle, false);
        }
        // No open event yet — wait for the Odin worker to ship a
        // NetConnection.Connect.Success status via dispatch_rtmp_status.
    }

    /// Look up the RTMP connection with the given Odin handle, take the
    /// responder pending under `txid`, decode the AMF0 body, and invoke the
    /// AS3 Responder's `onResult` (or `onStatus` for `_error`).
    pub fn dispatch_rtmp_call_result(
        context: &mut UpdateContext<'gc>,
        odin_handle: u64,
        txid: u32,
        is_error: bool,
        body_amf: &[u8],
    ) {
        let mut responder_handle: Option<ResponderHandle> = None;
        for (_, conn) in context.net_connections.connections.iter_mut() {
            if let NetConnectionProtocol::Rtmp(rtmp) = &mut conn.protocol
                && rtmp.odin_handle == odin_handle
            {
                responder_handle = rtmp.take_pending(txid);
                break;
            }
        }
        let Some(responder_handle) = responder_handle else {
            // No responder bound (call() invoked without one, or already
            // dispatched). Nothing more to do.
            return;
        };

        // Decode the AMF0 body into a Value. Empty body → Undefined; decode
        // failure → log and skip rather than panicking inside the player.
        let value: Rc<AmfValue> = if body_amf.is_empty() {
            Rc::new(AmfValue::Undefined)
        } else {
            use flash_lso::amf0::read::AMF0Decoder;
            let mut dec = AMF0Decoder::default();
            match dec.parse_single_element(body_amf) {
                Ok((_, v)) => v,
                Err(e) => {
                    tracing::error!("Failed to decode RTMP _result body for txid {txid}: {e:?}");
                    return;
                }
            }
        };

        let callback = if is_error { ResponderCallback::Status } else { ResponderCallback::Result };
        responder_handle.call(context, callback, value);
    }

    /// Dispatch a server-initiated callback (`onLineList`, `onStatus`,
    /// `onBWDone`, custom RPCs) on the AS3 NetConnection's `client`
    /// property. `args_amf` is a concatenation of AMF0-encoded top-level
    /// values — the same shape an AS3 caller would pass to a function.
    pub fn dispatch_rtmp_server_call(
        context: &mut UpdateContext<'gc>,
        odin_handle: u64,
        method: &str,
        args_amf: &[u8],
    ) {
        use crate::avm2::FunctionArgs;
        use crate::avm2::Value as Avm2Value;
        use crate::string::AvmString;
        use flash_lso::amf0::read::AMF0Decoder;

        let mut target_object: Option<NetConnectionObject<'gc>> = None;
        for (_, conn) in context.net_connections.connections.iter() {
            if let NetConnectionProtocol::Rtmp(rtmp) = &conn.protocol
                && rtmp.odin_handle == odin_handle
            {
                target_object = Some(conn.object);
                break;
            }
        }
        let Some(target) = target_object else { return };
        let nc_object = match target {
            NetConnectionObject::Avm2(o) => o,
            NetConnectionObject::Avm1(_) => {
                tracing::warn!(
                    "RTMP server call {method} on AVM1 NetConnection — not routed (would need a parallel dispatch path)"
                );
                return;
            }
        };

        let mut activation = Avm2Activation::from_nothing(context);

        // Decode every AMF0 value in `args_amf`. The remaining-byte loop
        // works because the AMF0 decoder consumes exactly one top-level
        // value per call and returns the unread suffix.
        let mut dec = AMF0Decoder::default();
        let mut args: Vec<Avm2Value<'gc>> = Vec::new();
        let mut remaining = args_amf;
        loop {
            if remaining.is_empty() { break; }
            match dec.parse_single_element(remaining) {
                Ok((rest, v)) => {
                    let val = match crate::avm2::amf::deserialize_value(&mut activation, &v) {
                        Ok(v) => v,
                        Err(e) => {
                            tracing::warn!("Failed to deserialize RTMP arg for {method}: {e:?}");
                            return;
                        }
                    };
                    args.push(val);
                    if rest.len() == remaining.len() {
                        // Decoder didn't advance — protect against an
                        // infinite loop on a malformed value.
                        break;
                    }
                    remaining = rest;
                }
                Err(e) => {
                    tracing::warn!("Failed to parse RTMP arg for {method}: {e:?}");
                    break;
                }
            }
        }

        // nc.client → AS3 object that owns the named callback method.
        let nc_value: Avm2Value<'gc> = Avm2Value::Object(nc_object.into());
        let client_value = match nc_value.get_public_property(
            AvmString::new_utf8(activation.gc(), "client"),
            &mut activation,
        ) {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!("Cannot read NetConnection.client for RTMP {method}: {e:?}");
                return;
            }
        };

        let method_name = AvmString::new_utf8(activation.gc(), method);
        match client_value.call_public_property(
            method_name,
            FunctionArgs::from_slice(&args),
            &mut activation,
        ) {
            Ok(_) => {}
            Err(e) => {
                // Stringify via Error::log so AS3 stack traces and the
                // actual thrown value end up in stderr instead of an
                // opaque "AvmError" Debug print.
                e.log(&mut activation, &format!("RTMP nc.client.{method}() raised"));
            }
        }
    }

    /// Look up the RTMP connection with the given Odin-side handle and
    /// dispatch a `NetStatusEvent` on its AS3 object.
    pub fn dispatch_rtmp_status(
        context: &mut UpdateContext<'gc>,
        odin_handle: u64,
        code: &str,
        level: &str,
    ) {
        let mut target_object: Option<NetConnectionObject<'gc>> = None;
        for (_, conn) in context.net_connections.connections.iter() {
            if let NetConnectionProtocol::Rtmp(rtmp) = &conn.protocol
                && rtmp.odin_handle == odin_handle
            {
                target_object = Some(conn.object);
                break;
            }
        }
        let Some(target) = target_object else { return };

        match target {
            NetConnectionObject::Avm2(object) => {
                let mut activation = Avm2Activation::from_nothing(context);
                let event = Avm2EventObject::net_status_event(
                    &mut activation,
                    [("code", code), ("level", level)],
                );
                Avm2::dispatch_event(activation.context, event, object.into());
            }
            NetConnectionObject::Avm1(_) => {
                // AVM1 NetConnection RTMP dispatch goes through a different
                // status-event helper; routing it is not needed for the
                // current AS3 game and would require deeper plumbing.
                tracing::warn!(
                    "RTMP status {code}/{level} dispatched on an AVM1 NetConnection — ignored"
                );
            }
        }
    }

    pub fn connect_to_flash_remoting<O: Into<NetConnectionObject<'gc>>>(
        context: &mut UpdateContext<'gc>,
        target: O,
        url: String,
    ) {
        let target = target.into();
        let connection = NetConnection {
            object: target,
            protocol: NetConnectionProtocol::FlashRemoting(FlashRemoting {
                url,
                headers: vec![],
                outgoing_queue: vec![],
            }),
        };
        let handle = context.net_connections.connections.insert(connection);

        if let Some(existing_handle) = target.set_handle(Some(handle)) {
            NetConnections::close(context, existing_handle, false);
        }

        // No open event here
    }

    pub fn close(context: &mut UpdateContext<'gc>, handle: NetConnectionHandle, is_explicit: bool) {
        let Some(connection) = context.net_connections.connections.remove(handle) else {
            return;
        };

        match connection.object {
            NetConnectionObject::Avm2(object) => {
                let mut activation = Avm2Activation::from_nothing(context);
                let event = Avm2EventObject::net_status_event(
                    &mut activation,
                    [
                        ("code", "NetConnection.Connect.Closed"),
                        ("level", "status"),
                    ],
                );
                Avm2::dispatch_event(activation.context, event, object.into());

                if is_explicit
                    && matches!(connection.protocol, NetConnectionProtocol::FlashRemoting(_))
                {
                    // [NA] I have no idea why, but a NetConnection receives a second and nonsensical event on close
                    let event = Avm2EventObject::net_status_event(
                        &mut activation,
                        [
                            ("code", ""),
                            ("description", ""),
                            ("details", ""),
                            ("level", "status"),
                        ],
                    );
                    Avm2::dispatch_event(activation.context, event, object.into());
                }
            }
            NetConnectionObject::Avm1(object) => {
                if let Err(e) = Avm1NetConnectionObject::on_status_event(
                    context,
                    object,
                    "NetConnection.Connect.Closed",
                ) {
                    tracing::error!("Unhandled error sending connection callback: {e}");
                }
                if is_explicit
                    && matches!(connection.protocol, NetConnectionProtocol::FlashRemoting(_))
                    && let Err(e) = Avm1NetConnectionObject::on_empty_status_event(context, object)
                {
                    tracing::error!("Unhandled error sending connection callback: {e}");
                }
            }
        }
    }

    pub fn update_connections(context: &mut UpdateContext<'gc>) {
        let player = context.player_handle();
        for (handle, connection) in context.net_connections.connections.iter_mut() {
            connection.update(handle, context.navigator, &player);
        }
    }

    pub fn send_without_response(
        context: &mut UpdateContext<'gc>,
        handle: NetConnectionHandle,
        command: String,
        message: AmfValue,
    ) {
        if let Some(connection) = context.net_connections.connections.get_mut(handle) {
            connection.send(command, None, message);
        }
    }

    pub fn send_avm2(
        context: &mut UpdateContext<'gc>,
        handle: NetConnectionHandle,
        command: String,
        message: AmfValue,
        responder: Avm2ResponderObject<'gc>,
    ) {
        let mc = context.gc();
        if let Some(connection) = context.net_connections.connections.get_mut(handle) {
            // TODO(moulins): it'd be nice to avoid the double indirection here...
            let responder_handle =
                ResponderHandle::Avm2(context.dynamic_root.stash(mc, Gc::new(mc, responder)));
            connection.send(command, Some(responder_handle), message);
        }
    }

    pub fn send_avm1(
        context: &mut UpdateContext<'gc>,
        handle: NetConnectionHandle,
        command: String,
        message: AmfValue,
        responder: Avm1Object<'gc>,
    ) {
        let mc = context.gc();
        if let Some(connection) = context.net_connections.connections.get_mut(handle) {
            // TODO(moulins): it'd be nice to avoid the double indirection here...
            let responder_handle =
                ResponderHandle::Avm1(context.dynamic_root.stash(mc, Gc::new(mc, responder)));
            connection.send(command, Some(responder_handle), message);
        }
    }

    pub fn set_header(&mut self, handle: NetConnectionHandle, header: Header) {
        if let Some(connection) = self.connections.get_mut(handle) {
            connection.set_header(header);
        }
    }

    pub fn is_connected(&self, handle: NetConnectionHandle) -> bool {
        self.connections
            .get(handle)
            .map(|c| c.is_connected())
            .unwrap_or_default()
    }

    pub fn get_connected_proxy_type(&self, handle: NetConnectionHandle) -> Option<&'static str> {
        self.connections
            .get(handle)
            .and_then(|c| c.connected_proxy_type())
    }

    pub fn get_far_id(&self, handle: NetConnectionHandle) -> Option<&'static str> {
        self.connections.get(handle).and_then(|c| c.far_id())
    }

    pub fn get_far_nonce(&self, handle: NetConnectionHandle) -> Option<&'static str> {
        self.connections.get(handle).and_then(|c| c.far_nonce())
    }

    pub fn get_near_id(&self, handle: NetConnectionHandle) -> Option<&'static str> {
        self.connections.get(handle).and_then(|c| c.near_id())
    }

    pub fn get_near_nonce(&self, handle: NetConnectionHandle) -> Option<&'static str> {
        self.connections.get(handle).and_then(|c| c.near_nonce())
    }

    pub fn get_protocol(&self, handle: NetConnectionHandle) -> Option<&'static str> {
        self.connections.get(handle).and_then(|c| c.protocol())
    }

    pub fn get_uri(&self, handle: NetConnectionHandle) -> Option<String> {
        self.connections.get(handle).and_then(|c| c.uri())
    }

    pub fn is_using_tls(&self, handle: NetConnectionHandle) -> Option<bool> {
        self.connections.get(handle).and_then(|c| c.using_tls())
    }
}

#[derive(Collect)]
#[collect(no_drop)]
pub struct NetConnection<'gc> {
    object: NetConnectionObject<'gc>,

    #[collect(require_static)]
    protocol: NetConnectionProtocol,
}

impl NetConnection<'_> {
    pub fn is_connected(&self) -> bool {
        match self.protocol {
            NetConnectionProtocol::Local => true,
            NetConnectionProtocol::FlashRemoting(_) => false,
            NetConnectionProtocol::Rtmp(_) => true,
        }
    }

    pub fn connected_proxy_type(&self) -> Option<&'static str> {
        match self.protocol {
            NetConnectionProtocol::Local => Some("none"),
            NetConnectionProtocol::FlashRemoting(_) => None,
            NetConnectionProtocol::Rtmp(_) => Some("none"),
        }
    }

    pub fn far_id(&self) -> Option<&'static str> {
        match self.protocol {
            NetConnectionProtocol::Local => Some(""),
            NetConnectionProtocol::FlashRemoting(_) => None,
            NetConnectionProtocol::Rtmp(_) => Some(""),
        }
    }

    pub fn far_nonce(&self) -> Option<&'static str> {
        match self.protocol {
            NetConnectionProtocol::Local => {
                Some("0000000000000000000000000000000000000000000000000000000000000000")
            }
            NetConnectionProtocol::FlashRemoting(_) => None,
            NetConnectionProtocol::Rtmp(_) => {
                Some("0000000000000000000000000000000000000000000000000000000000000000")
            }
        }
    }

    pub fn near_id(&self) -> Option<&'static str> {
        match self.protocol {
            NetConnectionProtocol::Local => Some(""),
            NetConnectionProtocol::FlashRemoting(_) => None,
            NetConnectionProtocol::Rtmp(_) => Some(""),
        }
    }

    pub fn near_nonce(&self) -> Option<&'static str> {
        match self.protocol {
            NetConnectionProtocol::Local => {
                Some("0000000000000000000000000000000000000000000000000000000000000000")
            }
            NetConnectionProtocol::FlashRemoting(_) => None,
            NetConnectionProtocol::Rtmp(_) => {
                Some("0000000000000000000000000000000000000000000000000000000000000000")
            }
        }
    }

    pub fn protocol(&self) -> Option<&'static str> {
        match self.protocol {
            NetConnectionProtocol::Local => Some("rtmp"),
            NetConnectionProtocol::FlashRemoting(_) => None,
            NetConnectionProtocol::Rtmp(_) => Some("rtmp"),
        }
    }

    pub fn uri(&self) -> Option<String> {
        match &self.protocol {
            NetConnectionProtocol::Local => Some("null".to_string()), // Yes, it's a string "null", not a real null.
            NetConnectionProtocol::FlashRemoting(remoting) => Some(remoting.url.to_string()),
            NetConnectionProtocol::Rtmp(rtmp) => Some(rtmp.url.to_string()),
        }
    }

    pub fn using_tls(&self) -> Option<bool> {
        match &self.protocol {
            NetConnectionProtocol::Local => Some(false),
            NetConnectionProtocol::FlashRemoting(_) => None,
            NetConnectionProtocol::Rtmp(_) => Some(false),
        }
    }

    pub fn send(
        &mut self,
        command: String,
        responder_handle: Option<ResponderHandle>,
        message: AmfValue,
    ) {
        match &mut self.protocol {
            NetConnectionProtocol::Local => {}
            NetConnectionProtocol::FlashRemoting(remoting) => {
                remoting.send(command, responder_handle, message)
            }
            NetConnectionProtocol::Rtmp(rtmp) => {
                rtmp.send_call(command, responder_handle, message);
            }
        }
    }

    pub fn update(
        &mut self,
        self_handle: NetConnectionHandle,
        navigator: &mut dyn NavigatorBackend,
        player: &Arc<Mutex<Player>>,
    ) {
        match &mut self.protocol {
            NetConnectionProtocol::Local => {}
            NetConnectionProtocol::FlashRemoting(remoting) => {
                if remoting.has_pending_packet() {
                    navigator.spawn_future(remoting.flush_queue(self_handle, player.clone()));
                }
            }
            NetConnectionProtocol::Rtmp(_) => {
                // The Odin worker thread drives RTMP I/O — nothing to do
                // per-tick on this side.
            }
        }
    }

    pub fn set_header(&mut self, header: Header) {
        match &mut self.protocol {
            NetConnectionProtocol::Local => {}
            NetConnectionProtocol::FlashRemoting(remoting) => {
                remoting.set_header(header);
            }
            NetConnectionProtocol::Rtmp(_) => {
                let _ = header;
            }
        }
    }
}

#[derive(Debug)]
pub enum NetConnectionProtocol {
    /// A "local" connection, caused by connecting to null
    Local,

    /// Flash Remoting protocol, caused by connecting to a `http://` address.
    FlashRemoting(FlashRemoting),

    /// RTMP / RTMPE / RTMPS / RTMPT family — actual socket and protocol
    /// stack live in the Odin worker; we just hold the opaque handle so we
    /// can route status events back to the AS3 object.
    Rtmp(RtmpConnection),
}

#[derive(Debug)]
pub struct RtmpConnection {
    /// Original URL as the SWF asked for it (including rtmpe:// scheme).
    pub url: String,
    /// Opaque handle returned by the Odin `connect` callback.
    pub odin_handle: u64,
    /// Next AMF0 transaction id to assign. RTMP convention: 1 is the
    /// `connect` command (already used on the Odin side), so we start at 2.
    pub next_txid: u32,
    /// Responder per outstanding call(), keyed by txid. Drained when the
    /// server replies with `_result` or `_error` for that transaction.
    pub pending: std::collections::HashMap<u32, ResponderHandle>,
}

impl RtmpConnection {
    /// Ship an AS3 `NetConnection.call("command", responder, ...args)` over
    /// the wire: AMF0-encode `[command, txid, null_obj, args...]`, stash the
    /// responder under `txid`, and hand the bytes to the Odin backend.
    pub fn send_call(
        &mut self,
        command: String,
        responder_handle: Option<ResponderHandle>,
        message: AmfValue,
    ) {
        let txid = self.next_txid;
        self.next_txid = self.next_txid.wrapping_add(1);

        let mut payload: Vec<u8> = Vec::with_capacity(64);
        amf0_write_string(&mut payload, &command);
        amf0_write_number(&mut payload, txid as f64);
        amf0_write_null(&mut payload);

        // `message` is a StrictArray packing the AS3 arguments — unwrap it
        // and emit each element as a top-level AMF0 value, which is what
        // RTMP servers expect for command-message arguments.
        if let AmfValue::StrictArray(_, args) = &message {
            for arg in args { amf0_write_value(&mut payload, arg); }
        } else {
            amf0_write_value(&mut payload, &message);
        }

        if let Some(resp) = responder_handle {
            self.pending.insert(txid, resp);
        }

        net_connection_backend::call(self.odin_handle, txid, &payload);
    }

    pub fn take_pending(&mut self, txid: u32) -> Option<ResponderHandle> {
        self.pending.remove(&txid)
    }
}

// --- AMF0 encoder (subset of flash_lso::types::Value) ---------------------
// Just enough to ship `NetConnection.call(...)` payloads to an RTMP server.
// `flash_lso`'s per-value writer is `pub(crate)`, so we inline our own here
// rather than vendor the dep. Mirrors AMF0 spec §2 markers.

const AMF0_NUMBER: u8       = 0x00;
const AMF0_BOOLEAN: u8      = 0x01;
const AMF0_STRING: u8       = 0x02;
const AMF0_OBJECT: u8       = 0x03;
const AMF0_NULL: u8         = 0x05;
const AMF0_UNDEFINED: u8    = 0x06;
const AMF0_ECMA_ARRAY: u8   = 0x08;
const AMF0_OBJECT_END: u8   = 0x09;
const AMF0_STRICT_ARRAY: u8 = 0x0a;
const AMF0_DATE: u8         = 0x0b;
const AMF0_LONG_STRING: u8  = 0x0c;
const AMF0_XML_DOCUMENT: u8 = 0x0f;
const AMF0_TYPED_OBJECT: u8 = 0x10;

fn amf0_write_utf8(buf: &mut Vec<u8>, s: &str) {
    let bytes = s.as_bytes();
    buf.extend_from_slice(&(bytes.len() as u16).to_be_bytes());
    buf.extend_from_slice(bytes);
}

fn amf0_write_long_utf8(buf: &mut Vec<u8>, s: &str) {
    let bytes = s.as_bytes();
    buf.extend_from_slice(&(bytes.len() as u32).to_be_bytes());
    buf.extend_from_slice(bytes);
}

fn amf0_write_string(buf: &mut Vec<u8>, s: &str) {
    if s.len() > 65535 {
        buf.push(AMF0_LONG_STRING);
        amf0_write_long_utf8(buf, s);
    } else {
        buf.push(AMF0_STRING);
        amf0_write_utf8(buf, s);
    }
}

fn amf0_write_number(buf: &mut Vec<u8>, n: f64) {
    buf.push(AMF0_NUMBER);
    buf.extend_from_slice(&n.to_be_bytes());
}

fn amf0_write_null(buf: &mut Vec<u8>) {
    buf.push(AMF0_NULL);
}

fn amf0_write_object_inner(buf: &mut Vec<u8>, elements: &[Element]) {
    for el in elements {
        amf0_write_utf8(buf, &el.name);
        amf0_write_value(buf, &el.value);
    }
    // Object terminator: empty UTF-8 key followed by OBJECT_END marker.
    buf.extend_from_slice(&[0x00, 0x00, AMF0_OBJECT_END]);
}

pub fn amf0_write_value(buf: &mut Vec<u8>, v: &AmfValue) {
    match v {
        AmfValue::Number(n)      => amf0_write_number(buf, *n),
        AmfValue::Bool(b)        => { buf.push(AMF0_BOOLEAN); buf.push(if *b {1} else {0}); }
        AmfValue::String(s)      => amf0_write_string(buf, s),
        AmfValue::Null           => buf.push(AMF0_NULL),
        AmfValue::Undefined      => buf.push(AMF0_UNDEFINED),
        AmfValue::Object(_, els, class_def) => {
            if let Some(cd) = class_def
                && !cd.name.is_empty()
            {
                buf.push(AMF0_TYPED_OBJECT);
                amf0_write_utf8(buf, &cd.name);
                amf0_write_object_inner(buf, els);
            } else {
                buf.push(AMF0_OBJECT);
                amf0_write_object_inner(buf, els);
            }
        }
        AmfValue::ECMAArray(_, _dense, assoc, count) => {
            buf.push(AMF0_ECMA_ARRAY);
            buf.extend_from_slice(&count.to_be_bytes());
            amf0_write_object_inner(buf, assoc);
        }
        AmfValue::StrictArray(_, items) => {
            buf.push(AMF0_STRICT_ARRAY);
            buf.extend_from_slice(&(items.len() as u32).to_be_bytes());
            for it in items { amf0_write_value(buf, it); }
        }
        AmfValue::Date(ts, tz) => {
            buf.push(AMF0_DATE);
            buf.extend_from_slice(&ts.to_be_bytes());
            buf.extend_from_slice(&tz.unwrap_or(0).to_be_bytes());
        }
        AmfValue::XML(s, _is_string) => {
            buf.push(AMF0_XML_DOCUMENT);
            amf0_write_long_utf8(buf, s);
        }
        AmfValue::Reference(_)
        | AmfValue::Unsupported
        | AmfValue::Custom(_, _, _) => buf.push(AMF0_UNDEFINED),
        // AMF3-only variants. `serialize_value(..., AMF0, ...)` shouldn't
        // produce them, but be safe — emit Undefined rather than corrupting
        // the wire stream.
        _ => buf.push(AMF0_UNDEFINED),
    }
}

#[derive(Debug)]
pub struct FlashRemoting {
    url: String,
    headers: Vec<Header>,
    outgoing_queue: Vec<(Message, Option<ResponderHandle>)>,
}

impl FlashRemoting {
    pub fn send(
        &mut self,
        command: String,
        responder_handle: Option<ResponderHandle>,
        message: AmfValue,
    ) {
        self.outgoing_queue.push((
            Message {
                target_uri: command,
                response_uri: format!("/{}", self.outgoing_queue.len() + 1), // Flash is 1-based... simplifies tests to stay the same
                contents: Rc::new(message),
            },
            responder_handle,
        ));
    }

    pub fn has_pending_packet(&self) -> bool {
        !self.outgoing_queue.is_empty()
    }

    pub fn set_header(&mut self, header: Header) {
        // Only one header of the same name (case insensitive) should exist
        self.headers
            .retain(|h| !h.name.eq_ignore_ascii_case(&header.name));

        self.headers.push(header);
    }

    pub fn flush_queue(
        &mut self,
        self_handle: NetConnectionHandle,
        player: Arc<Mutex<Player>>,
    ) -> OwnedFuture<(), Error> {
        let queue = std::mem::take(&mut self.outgoing_queue);
        let (messages, responder_handles): (Vec<_>, Vec<_>) = queue.into_iter().unzip();
        let packet = Packet {
            version: AMFVersion::AMF0,
            headers: self.headers.clone(),
            messages,
        };
        let url = self.url.clone();

        Box::pin(async move {
            let bytes = flash_lso::packet::write::write_to_bytes(&packet, true)
                .expect("Must be able to serialize a packet");
            let request = Request::post(url, Some((bytes, "application/x-amf".to_string())));
            let fetch = player.lock().unwrap().fetch(request, FetchReason::Other);
            let response: Result<_, ErrorResponse> = async {
                let response = fetch.await?;
                let url = response.url().to_string();
                let body = response
                    .body()
                    .await
                    .map_err(|error| ErrorResponse { url, error })?;

                Ok(body)
            }
            .await;
            let response = match response {
                Ok(response) => response,
                Err(response) => {
                    player.lock().unwrap().update(|uc| {
                        tracing::error!(
                            "Couldn't submit AMF Packet to {}: {:?}",
                            response.url,
                            response.error
                        );
                        if let Some(connection) = uc.net_connections.connections.get(self_handle) {
                            match connection.object {
                                NetConnectionObject::Avm2(object) => {
                                    let mut activation = Avm2Activation::from_nothing(uc);
                                    let event = Avm2EventObject::net_status_event(
                                        &mut activation,
                                        [
                                            ("code", "NetConnection.Call.Failed"),
                                            ("level", "error"),
                                            ("details", &response.url),
                                            ("description", "HTTP: Failed"),
                                        ],
                                    );
                                    Avm2::dispatch_event(activation.context, event, object.into());
                                }
                                NetConnectionObject::Avm1(object) => {
                                    if let Err(e) =
                                        Avm1NetConnectionObject::on_empty_status_event(uc, object)
                                    {
                                        tracing::error!(
                                            "Unhandled error sending connection callback: {e}"
                                        );
                                    }
                                }
                            }
                        }
                    });
                    return Ok(());
                }
            };

            // Flash completely ignores invalid responses, it seems
            if let Ok(response_packet) = flash_lso::packet::read::parse(&response) {
                player.lock().unwrap().update(|uc| {
                    for message in response_packet.messages {
                        if let Some(target_uri) = message.target_uri.strip_prefix('/') {
                            let mut responder = None;
                            if let Some(index) = target_uri
                                .strip_suffix("/onStatus")
                                .and_then(|str| str::parse::<usize>(str).ok())
                            {
                                responder = responder_handles
                                    .get(index.wrapping_sub(1))
                                    .cloned()
                                    .flatten()
                                    .map(|handle| (handle, ResponderCallback::Status));
                            } else if let Some(index) = target_uri
                                .strip_suffix("/onResult")
                                .and_then(|str| str::parse::<usize>(str).ok())
                            {
                                responder = responder_handles
                                    .get(index.wrapping_sub(1))
                                    .cloned()
                                    .flatten()
                                    .map(|handle| (handle, ResponderCallback::Result));
                            }

                            if let Some((responder_handle, callback)) = responder {
                                responder_handle.call(uc, callback, message.contents);
                            }
                        }
                    }
                });
            }

            Ok(())
        })
    }
}
