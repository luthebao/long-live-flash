//! `flash.system.MessageChannel` native methods

use crate::avm2::Error;
use crate::avm2::activation::Activation;
use crate::avm2::parameters::ParametersExt;
use crate::avm2::value::Value;
use crate::string::AvmString;
use crate::worker::WorkerChannelError;

use super::worker::{deserialize_worker_value, serialize_worker_value};

pub fn send<'gc>(
    activation: &mut Activation<'_, 'gc>,
    this: Value<'gc>,
    args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    let channel = this
        .as_object()
        .and_then(|object| object.as_message_channel_object())
        .expect("MessageChannel.send called on non-MessageChannel");
    let handle = channel.handle();
    let current_worker = activation.context.worker_runtime.current().id();
    if current_worker != handle.sender() {
        return Err(Error::rust_error(
            "MessageChannel.send may only be called by the sending worker".into(),
        ));
    }

    let value = serialize_worker_value(activation, args.get_value(0))?;
    let queue_limit = args.get_i32(1);
    handle
        .send(value, queue_limit)
        .map_err(channel_error_to_avm)?;
    Ok(Value::Undefined)
}

pub fn receive<'gc>(
    activation: &mut Activation<'_, 'gc>,
    this: Value<'gc>,
    args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    let channel = this
        .as_object()
        .and_then(|object| object.as_message_channel_object())
        .expect("MessageChannel.receive called on non-MessageChannel");
    let handle = channel.handle();
    let current_worker = activation.context.worker_runtime.current().id();
    if current_worker != handle.receiver() {
        return Err(Error::rust_error(
            "MessageChannel.receive may only be called by the receiving worker".into(),
        ));
    }

    let block = args.get_bool(0);
    match handle.receive(block).map_err(channel_error_to_avm)? {
        Some(value) => deserialize_worker_value(activation, value),
        None => Ok(Value::Null),
    }
}

pub fn close<'gc>(
    _activation: &mut Activation<'_, 'gc>,
    this: Value<'gc>,
    _args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    let channel = this
        .as_object()
        .and_then(|object| object.as_message_channel_object())
        .expect("MessageChannel.close called on non-MessageChannel");
    channel.handle().close();
    Ok(Value::Undefined)
}

pub fn get_state<'gc>(
    activation: &mut Activation<'_, 'gc>,
    this: Value<'gc>,
    _args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    let channel = this
        .as_object()
        .and_then(|object| object.as_message_channel_object())
        .expect("MessageChannel.state called on non-MessageChannel");
    Ok(AvmString::new_utf8(activation.gc(), channel.handle().state().as_str()).into())
}

pub fn get_message_available<'gc>(
    _activation: &mut Activation<'_, 'gc>,
    this: Value<'gc>,
    _args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    let channel = this
        .as_object()
        .and_then(|object| object.as_message_channel_object())
        .expect("MessageChannel.messageAvailable called on non-MessageChannel");
    Ok(channel.handle().message_available().into())
}

fn channel_error_to_avm<'gc>(error: WorkerChannelError) -> Error<'gc> {
    match error {
        WorkerChannelError::Closed => Error::rust_error("MessageChannel is closed".into()),
        WorkerChannelError::QueueFull => Error::rust_error("MessageChannel queue is full".into()),
    }
}
