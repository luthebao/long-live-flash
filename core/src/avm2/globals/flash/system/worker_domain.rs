//! `flash.system.WorkerDomain` native methods

use crate::avm2::Error;
use crate::avm2::activation::Activation;
use crate::avm2::object::{VectorObject, WorkerDomainObject, WorkerObject};
use crate::avm2::parameters::ParametersExt;
use crate::avm2::value::Value;
use crate::avm2::vector::VectorStorage;
use crate::worker::is_supported;

pub fn get_is_supported<'gc>(
    _activation: &mut Activation<'_, 'gc>,
    _this: Value<'gc>,
    _args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    Ok(is_supported().into())
}

pub fn create_worker<'gc>(
    activation: &mut Activation<'_, 'gc>,
    _this: Value<'gc>,
    args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    if !is_supported() {
        return Ok(Value::Null);
    }

    let swf = args.get_object(activation, 0, "swf")?;
    let _give_app_privileges = args.get_bool(1);
    let Some(bytearray) = swf.as_bytearray() else {
        return Err(Error::rust_error("WorkerDomain.createWorker requires a ByteArray".into()));
    };
    let bytes = bytearray.bytes().to_vec();

    let handle = activation.context.worker_runtime.domain().create_worker(bytes);
    Ok(WorkerObject::new(activation, handle).into())
}

pub fn list_workers<'gc>(
    activation: &mut Activation<'_, 'gc>,
    _this: Value<'gc>,
    _args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    let handles = activation.context.worker_runtime.domain().running_workers();
    let values = handles
        .into_iter()
        .map(|handle| WorkerObject::new(activation, handle).into())
        .collect();
    let storage = VectorStorage::from_values(
        values,
        false,
        Some(activation.avm2().classes().worker.inner_class_definition()),
    );
    Ok(VectorObject::from_vector(storage, activation).into())
}

pub fn instantiate_internal<'gc>(
    activation: &mut Activation<'_, 'gc>,
    _this: Value<'gc>,
    _args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    Ok(WorkerDomainObject::new(activation).into())
}
