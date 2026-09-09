//! `flash.system.Worker` native methods

use crate::avm2::Error;
use crate::avm2::activation::Activation;
use crate::avm2::object::{MessageChannelObject, WorkerObject};
use crate::avm2::parameters::ParametersExt;
use crate::avm2::value::Value;
use crate::string::AvmString;
use crate::worker::{MessageChannelHandle, WorkerLaunchConfig, WorkerValue, start_worker};
use flash_lso::amf3::read::AMF3Decoder;
use flash_lso::types::{AMFVersion, Element};
use std::rc::Rc;

pub fn get_is_supported<'gc>(
    activation: &mut Activation<'_, 'gc>,
    _this: Value<'gc>,
    _args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    Ok(activation.context.worker_runtime.is_enabled().into())
}

pub fn get_is_primordial<'gc>(
    _activation: &mut Activation<'_, 'gc>,
    this: Value<'gc>,
    _args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    let worker = this
        .as_object()
        .and_then(|object| object.as_worker_object())
        .expect("Worker.isPrimordial called on non-Worker");
    Ok(worker.handle().is_primordial().into())
}

pub fn get_state<'gc>(
    activation: &mut Activation<'_, 'gc>,
    this: Value<'gc>,
    _args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    let worker = this
        .as_object()
        .and_then(|object| object.as_worker_object())
        .expect("Worker.state called on non-Worker");
    Ok(AvmString::new_utf8(activation.gc(), worker.handle().state().as_str()).into())
}

pub fn create_message_channel<'gc>(
    activation: &mut Activation<'_, 'gc>,
    this: Value<'gc>,
    args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    let sender = this
        .as_object()
        .and_then(|object| object.as_worker_object())
        .expect("Worker.createMessageChannel called on non-Worker")
        .handle();
    let receiver = args
        .get_object(activation, 0, "receiver")?
        .as_worker_object()
        .ok_or_else(|| Error::rust_error("Worker receiver is not a Worker".into()))?
        .handle();

    let handle = MessageChannelHandle::new(sender.id(), receiver.id());
    Ok(MessageChannelObject::new(activation, handle).into())
}

pub fn set_shared_property<'gc>(
    activation: &mut Activation<'_, 'gc>,
    this: Value<'gc>,
    args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    let worker = this
        .as_object()
        .and_then(|object| object.as_worker_object())
        .expect("Worker.setSharedProperty called on non-Worker");
    let key = args.get_string_non_null(activation, 0, "key")?;
    let key = key.to_utf8_lossy();
    let value = args.get_value(1);
    if matches!(value, Value::Null | Value::Undefined) {
        worker.handle().clear_shared_property(&key);
    } else {
        let value = serialize_worker_value(activation, value)?;
        worker
            .handle()
            .set_shared_property(key.into_owned(), value);
    }
    Ok(Value::Undefined)
}

pub fn get_shared_property<'gc>(
    activation: &mut Activation<'_, 'gc>,
    this: Value<'gc>,
    args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    let worker = this
        .as_object()
        .and_then(|object| object.as_worker_object())
        .expect("Worker.getSharedProperty called on non-Worker");
    let key = args.get_string_non_null(activation, 0, "key")?;
    let Some(value) = worker.handle().get_shared_property(&key.to_utf8_lossy()) else {
        return Ok(Value::Null);
    };
    deserialize_worker_value(activation, value)
}

pub fn start<'gc>(
    activation: &mut Activation<'_, 'gc>,
    this: Value<'gc>,
    _args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    let worker = this
        .as_object()
        .and_then(|object| object.as_worker_object())
        .expect("Worker.start called on non-Worker")
        .handle();

    let config = WorkerLaunchConfig {
        player_version: activation.context.player_version,
        player_runtime: activation.context.player_runtime,
        player_mode: activation.context.player_mode,
        worker_enabled: activation.context.worker_runtime.is_enabled(),
    };
    start_worker(activation.context.worker_runtime.domain(), worker, config);
    Ok(Value::Undefined)
}

pub fn terminate<'gc>(
    _activation: &mut Activation<'_, 'gc>,
    this: Value<'gc>,
    _args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    let worker = this
        .as_object()
        .and_then(|object| object.as_worker_object())
        .expect("Worker.terminate called on non-Worker");
    Ok(worker.handle().terminate().into())
}

pub fn instantiate_internal<'gc>(
    activation: &mut Activation<'_, 'gc>,
    _this: Value<'gc>,
    _args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    let handle = activation.context.worker_runtime.current();
    Ok(WorkerObject::new(activation, handle).into())
}

pub(crate) fn serialize_worker_value<'gc>(
    activation: &mut Activation<'_, 'gc>,
    value: Value<'gc>,
) -> Result<WorkerValue, Error<'gc>> {
    if let Some(object) = value.as_object() {
        if let Some(worker) = object.as_worker_object() {
            return Ok(WorkerValue::Worker(worker.handle()));
        }
        if let Some(channel) = object.as_message_channel_object() {
            return Ok(WorkerValue::MessageChannel(channel.handle()));
        }
    }

    let amf = crate::avm2::amf::serialize_value(
        activation,
        value,
        AMFVersion::AMF3,
        &mut Default::default(),
    )
    .unwrap_or(flash_lso::types::Value::Undefined);

    let element = Element::new("", Rc::new(amf));
    let mut lso = flash_lso::types::Lso::new(vec![element], "", AMFVersion::AMF3);
    let bytes = flash_lso::write::write_to_bytes(&mut lso)
        .map_err(|_| Error::rust_error("Failed to serialize worker value".into()))?;
    let value_offset = flash_lso::write::header_length(&lso.header) + 7;
    let value_end = bytes.len().saturating_sub(1);
    if value_offset > value_end {
        return Err(Error::rust_error("Invalid serialized worker value".into()));
    }

    Ok(WorkerValue::Serialized(
        bytes[value_offset..value_end].to_vec(),
    ))
}

pub(crate) fn deserialize_worker_value<'gc>(
    activation: &mut Activation<'_, 'gc>,
    value: WorkerValue,
) -> Result<Value<'gc>, Error<'gc>> {
    match value {
        WorkerValue::Worker(handle) => Ok(WorkerObject::new(activation, handle).into()),
        WorkerValue::MessageChannel(handle) => {
            Ok(MessageChannelObject::new(activation, handle).into())
        }
        WorkerValue::Serialized(bytes) => {
            let mut decoder = AMF3Decoder::default();
            let (_, amf) = decoder
                .parse_single_element(&bytes)
                .map_err(|_| Error::rust_error("Invalid worker message".into()))?;
            crate::avm2::amf::deserialize_value(activation, &amf)
        }
    }
}
