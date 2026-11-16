//! `flash.system.WorkerDomain` native methods

use crate::avm2::Error;
use crate::avm2::activation::Activation;
use crate::avm2::function::FunctionArgs;
use crate::avm2::object::{WorkerDomainObject, WorkerObject};
use crate::avm2::parameters::ParametersExt;
use crate::avm2::value::Value;

/// Implements `WorkerDomain.createWorker`
pub fn create_worker<'gc>(
    activation: &mut Activation<'_, 'gc>,
    _this: Value<'gc>,
    args: FunctionArgs<'_, 'gc>,
) -> Result<Value<'gc>, Error<'gc>> {
    let swf = args.get_object(activation, 0, "swf")?;
    let _give_app_privileges = args.get_bool(1);

    // Route A (same-arena emulation): keep the SWF bytes on the WorkerObject;
    // the player boots them on the next tick after `start()`.
    let bytes = swf
        .as_bytearray()
        .map(|b| b.bytes().to_vec())
        .unwrap_or_default();

    let worker = WorkerObject::new(activation);
    worker.set_swf_bytes(activation.gc(), bytes);

    Ok(worker.into())
}

/// Implements `WorkerDomain.current`
pub fn get_current<'gc>(
    activation: &mut Activation<'_, 'gc>,
    _this: Value<'gc>,
    _args: FunctionArgs<'_, 'gc>,
) -> Result<Value<'gc>, Error<'gc>> {
    if let Some(domain) = activation.context.avm2.main_worker_domain {
        return Ok(domain.into());
    }
    let domain = WorkerDomainObject::new(activation);
    activation.context.avm2.main_worker_domain = Some(domain);
    Ok(domain.into())
}
