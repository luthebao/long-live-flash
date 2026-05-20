use crate::avm2::amf::serialize_value;
use crate::avm2::error::make_error_2126;
pub use crate::avm2::object::net_connection_allocator;
use crate::avm2::parameters::ParametersExt;
use crate::backend::net_connection as net_connection_backend;
use crate::net_connection::NetConnections;
use crate::string::AvmString;
use crate::{
    avm2::{Activation, Error, Value},
    avm2_stub_method,
};
use flash_lso::packet::Header;
use flash_lso::types::AMFVersion;
use flash_lso::types::ObjectId;
use flash_lso::types::Value as AMFValue;
use fnv::FnvHashMap;
use ruffle_wstr::WStr;
use std::rc::Rc;

pub fn connect<'gc>(
    activation: &mut Activation<'_, 'gc>,
    this: Value<'gc>,
    args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    let this = this.as_object().unwrap();

    let connection = this
        .as_net_connection()
        .expect("Must be NetConnection object");

    let url = args.try_get_string(0);

    if let Some(url) = url {
        let url_lower = url.to_ascii_lowercase();

        if url_lower.starts_with(WStr::from_units(b"http://"))
            || url_lower.starts_with(WStr::from_units(b"https://"))
        {
            // HTTP(S) is for Flash Remoting, which is just POST requests to the URL.
            NetConnections::connect_to_flash_remoting(
                activation.context,
                connection,
                url.to_string(),
            );
        } else if is_rtmp_scheme(&url_lower)
            && net_connection_backend::has_connect_hook()
        {
            tracing::info!(target: "avm_warning", "NetConnection.connect routed to RTMP backend: {}", url);
            // Route rtmp/rtmps/rtmpe/rtmpt/rtmpte to the host backend (Odin).
            // Extra `connect()` arguments past the URL get AMF0-serialised
            // and appended to the protocol-level connect command. Most
            // Flash MMO clients ship credentials this way:
            //   nc.connect("rtmpe://host/app", connectType, user, pass, …)
            let url_str = url.to_string();
            let mut args_buf: Vec<u8> = Vec::new();
            let mut object_table = FnvHashMap::default();
            for arg in &args[1..] {
                if let Some(v) =
                    serialize_value(activation, *arg, AMFVersion::AMF0, &mut object_table)
                {
                    crate::net_connection::amf0_write_value(&mut args_buf, &v);
                }
            }
            let swf_url = activation.context.root_swf.url().to_string();
            let page_url = activation
                .context
                .page_url
                .as_deref()
                .unwrap_or("")
                .to_string();
            let odin_handle =
                net_connection_backend::connect(&url_str, &swf_url, &page_url, &args_buf);
            if odin_handle == 0 {
                avm2_stub_method!(
                    activation,
                    "flash.net.NetConnection",
                    "connect",
                    "RTMP host backend returned a zero handle"
                );
            } else {
                NetConnections::connect_to_rtmp(
                    activation.context,
                    connection,
                    url_str,
                    odin_handle,
                );
            }
        } else {
            avm2_stub_method!(
                activation,
                "flash.net.NetConnection",
                "connect",
                "with non-null, non-http command"
            );
        }
    } else {
        NetConnections::connect_to_local(activation.context, connection);
    }

    Ok(Value::Undefined)
}

fn is_rtmp_scheme(url_lower: &WStr) -> bool {
    url_lower.starts_with(WStr::from_units(b"rtmp://"))
        || url_lower.starts_with(WStr::from_units(b"rtmps://"))
        || url_lower.starts_with(WStr::from_units(b"rtmpe://"))
        || url_lower.starts_with(WStr::from_units(b"rtmpt://"))
        || url_lower.starts_with(WStr::from_units(b"rtmpte://"))
}

pub fn close<'gc>(
    activation: &mut Activation<'_, 'gc>,
    this: Value<'gc>,
    _args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    let this = this.as_object().unwrap();

    let connection = this
        .as_net_connection()
        .expect("Must be NetConnection object");
    if let Some(previous_handle) = connection.set_handle(None) {
        NetConnections::close(activation.context, previous_handle, true);
    }

    Ok(Value::Undefined)
}

pub fn get_connected<'gc>(
    activation: &mut Activation<'_, 'gc>,
    this: Value<'gc>,
    _args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    let this = this.as_object().unwrap();

    let this = this
        .as_net_connection()
        .expect("Must be NetConnection object");

    if let Some(handle) = this.handle() {
        return Ok(activation
            .context
            .net_connections
            .is_connected(handle)
            .into());
    }

    Ok(false.into())
}

pub fn get_connected_proxy_type<'gc>(
    activation: &mut Activation<'_, 'gc>,
    this: Value<'gc>,
    _args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    let this = this.as_object().unwrap();

    let this = this
        .as_net_connection()
        .expect("Must be NetConnection object");

    if let Some(result) = this.handle().and_then(|handle| {
        activation
            .context
            .net_connections
            .get_connected_proxy_type(handle)
    }) {
        return Ok(AvmString::new_utf8(activation.gc(), result).into());
    }

    Err(make_error_2126(activation))
}

pub fn get_far_id<'gc>(
    activation: &mut Activation<'_, 'gc>,
    this: Value<'gc>,
    _args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    let this = this.as_object().unwrap();

    let this = this
        .as_net_connection()
        .expect("Must be NetConnection object");

    if let Some(result) = this
        .handle()
        .and_then(|handle| activation.context.net_connections.get_far_id(handle))
    {
        return Ok(AvmString::new_utf8(activation.gc(), result).into());
    }

    Err(make_error_2126(activation))
}

pub fn get_far_nonce<'gc>(
    activation: &mut Activation<'_, 'gc>,
    this: Value<'gc>,
    _args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    let this = this.as_object().unwrap();

    let this = this
        .as_net_connection()
        .expect("Must be NetConnection object");

    if let Some(result) = this
        .handle()
        .and_then(|handle| activation.context.net_connections.get_far_nonce(handle))
    {
        return Ok(AvmString::new_utf8(activation.gc(), result).into());
    }

    Err(make_error_2126(activation))
}

pub fn get_near_id<'gc>(
    activation: &mut Activation<'_, 'gc>,
    this: Value<'gc>,
    _args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    let this = this.as_object().unwrap();

    let this = this
        .as_net_connection()
        .expect("Must be NetConnection object");

    if let Some(result) = this
        .handle()
        .and_then(|handle| activation.context.net_connections.get_near_id(handle))
    {
        return Ok(AvmString::new_utf8(activation.gc(), result).into());
    }

    Err(make_error_2126(activation))
}

pub fn get_near_nonce<'gc>(
    activation: &mut Activation<'_, 'gc>,
    this: Value<'gc>,
    _args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    let this = this.as_object().unwrap();

    let this = this
        .as_net_connection()
        .expect("Must be NetConnection object");

    if let Some(result) = this
        .handle()
        .and_then(|handle| activation.context.net_connections.get_near_nonce(handle))
    {
        return Ok(AvmString::new_utf8(activation.gc(), result).into());
    }

    Err(make_error_2126(activation))
}

pub fn get_protocol<'gc>(
    activation: &mut Activation<'_, 'gc>,
    this: Value<'gc>,
    _args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    let this = this.as_object().unwrap();

    let this = this
        .as_net_connection()
        .expect("Must be NetConnection object");

    if let Some(result) = this
        .handle()
        .and_then(|handle| activation.context.net_connections.get_protocol(handle))
    {
        return Ok(AvmString::new_utf8(activation.gc(), result).into());
    }

    Err(make_error_2126(activation))
}

pub fn get_uri<'gc>(
    activation: &mut Activation<'_, 'gc>,
    this: Value<'gc>,
    _args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    let this = this.as_object().unwrap();

    let this = this
        .as_net_connection()
        .expect("Must be NetConnection object");

    if let Some(result) = this
        .handle()
        .and_then(|handle| activation.context.net_connections.get_uri(handle))
    {
        return Ok(AvmString::new_utf8(activation.gc(), result).into());
    }

    Ok(Value::Null)
}

pub fn get_using_tls<'gc>(
    activation: &mut Activation<'_, 'gc>,
    this: Value<'gc>,
    _args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    let this = this.as_object().unwrap();

    let this = this
        .as_net_connection()
        .expect("Must be NetConnection object");

    if let Some(result) = this
        .handle()
        .and_then(|handle| activation.context.net_connections.is_using_tls(handle))
    {
        return Ok(result.into());
    }

    Err(make_error_2126(activation))
}

pub fn call<'gc>(
    activation: &mut Activation<'_, 'gc>,
    this: Value<'gc>,
    args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    let this = this.as_object().unwrap();

    let connection = this
        .as_net_connection()
        .expect("Must be NetConnection object");

    let command = args.get_string(activation, 0);
    let responder = args.try_get_object(1).and_then(|o| o.as_responder());
    let mut arguments = Vec::new();

    let mut object_table = FnvHashMap::default();
    for arg in &args[2..] {
        if let Some(value) = serialize_value(activation, *arg, AMFVersion::AMF0, &mut object_table)
        {
            arguments.push(Rc::new(value));
        }
    }

    if let Some(handle) = connection.handle() {
        if let Some(responder) = responder {
            NetConnections::send_avm2(
                activation.context,
                handle,
                command.to_string(),
                AMFValue::StrictArray(ObjectId::INVALID, arguments),
                responder,
            );
        } else {
            NetConnections::send_without_response(
                activation.context,
                handle,
                command.to_string(),
                AMFValue::StrictArray(ObjectId::INVALID, arguments),
            );
        }

        return Ok(Value::Undefined);
    }

    Err(make_error_2126(activation))
}

pub fn add_header<'gc>(
    activation: &mut Activation<'_, 'gc>,
    this: Value<'gc>,
    args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    let this = this.as_object().unwrap();

    let connection = this
        .as_net_connection()
        .expect("Must be NetConnection object");

    // [NA] The documentation says that the header persists for the duration of this object.
    // However, this doesn't seem to be true - if you set a header and then open a connection,
    // the header is lost.
    // Therefore, we'll only store them on an active connection object - and lose them otherwise.

    // [NA] Another thing the docs have wrong, it says that you can remove a header by just calling
    // `addHeader(name)` - but this is clearly false. It instead replaces the value of the header
    // with a null value, sending that over the wire.

    let name = args.get_string(activation, 0);
    let must_understand = args.get_bool(1);
    // FIXME - do we re-use the same object reference table for all headers?
    let value = serialize_value(
        activation,
        args.get_value(2),
        AMFVersion::AMF0,
        &mut Default::default(),
    )
    .unwrap_or(AMFValue::Null);

    if let Some(handle) = connection.handle() {
        activation.context.net_connections.set_header(
            handle,
            Header {
                name: name.to_string(),
                must_understand,
                value: Rc::new(value),
            },
        );
    }

    Ok(Value::Undefined)
}
