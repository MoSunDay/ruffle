//! `flash.system.Worker` native methods

use crate::avm2::Error;
use crate::avm2::activation::Activation;
use crate::avm2::function::FunctionArgs;
use crate::avm2::object::{MessageChannelObject, WorkerObject, WorkerState};
use crate::avm2::parameters::ParametersExt;
use crate::avm2::value::Value;
use crate::avm2_stub_method;
use crate::string::AvmString;

/// Implements `Worker.createMessageChannel`
pub fn create_message_channel<'gc>(
    activation: &mut Activation<'_, 'gc>,
    _this: Value<'gc>,
    args: FunctionArgs<'_, 'gc>,
) -> Result<Value<'gc>, Error<'gc>> {
    avm2_stub_method!(activation, "flash.system.Worker", "createMessageChannel");

    let _receiver = args.get_object(activation, 0, "receiver")?;

    let message_channel = MessageChannelObject::new(activation);

    Ok(message_channel.into())
}

/// Implements `Worker.state` getter
pub fn get_state<'gc>(
    activation: &mut Activation<'_, 'gc>,
    this: Value<'gc>,
    _args: FunctionArgs<'_, 'gc>,
) -> Result<Value<'gc>, Error<'gc>> {
    let worker = this
        .as_object()
        .and_then(|o| o.as_worker_object())
        .expect("Worker.state on non-worker");
    Ok(AvmString::new_utf8(activation.gc(), worker.state().as_str()).into())
}

/// Implements `Worker.setSharedProperty`
pub fn set_shared_property<'gc>(
    activation: &mut Activation<'_, 'gc>,
    this: Value<'gc>,
    args: FunctionArgs<'_, 'gc>,
) -> Result<Value<'gc>, Error<'gc>> {
    let worker = this
        .as_object()
        .and_then(|o| o.as_worker_object())
        .expect("Worker.setSharedProperty on non-worker");

    let key = args.get_string_non_null(activation, 0, "key")?;
    let value = args.get_value(1);
    worker.set_shared_property(activation.gc(), key, value);

    Ok(Value::Undefined)
}

/// Implements `Worker.getSharedProperty`
pub fn get_shared_property<'gc>(
    activation: &mut Activation<'_, 'gc>,
    this: Value<'gc>,
    args: FunctionArgs<'_, 'gc>,
) -> Result<Value<'gc>, Error<'gc>> {
    let worker = this
        .as_object()
        .and_then(|o| o.as_worker_object())
        .expect("Worker.getSharedProperty on non-worker");

    let key = args.get_string_non_null(activation, 0, "key")?;
    Ok(worker.get_shared_property(key).unwrap_or(Value::Undefined))
}

/// Implements `Worker.current`
pub fn get_current<'gc>(
    activation: &mut Activation<'_, 'gc>,
    _this: Value<'gc>,
    _args: FunctionArgs<'_, 'gc>,
) -> Result<Value<'gc>, Error<'gc>> {
    if let Some(worker) = activation.context.avm2.current_worker {
        return Ok(worker.into());
    }
    if let Some(worker) = activation.context.avm2.main_worker {
        return Ok(worker.into());
    }
    let worker = WorkerObject::new(activation);
    worker.set_state(activation.gc(), WorkerState::Running);
    activation.context.avm2.main_worker = Some(worker);
    Ok(worker.into())
}

/// Implements `Worker.start`: mark the worker running and queue its SWF for
/// boot on the next player tick. `workerState` is dispatched asynchronously.
pub fn start<'gc>(
    activation: &mut Activation<'_, 'gc>,
    this: Value<'gc>,
    _args: FunctionArgs<'_, 'gc>,
) -> Result<Value<'gc>, Error<'gc>> {
    let worker = this
        .as_object()
        .and_then(|o| o.as_worker_object())
        .expect("Worker.start on non-worker");

    if worker.state() == WorkerState::New {
        worker.set_state(activation.gc(), WorkerState::Running);
        activation.context.avm2.worker_boot_queue.push(worker);
    }

    Ok(Value::Undefined)
}

/// Implements `Worker.terminate`
pub fn terminate<'gc>(
    activation: &mut Activation<'_, 'gc>,
    this: Value<'gc>,
    _args: FunctionArgs<'_, 'gc>,
) -> Result<Value<'gc>, Error<'gc>> {
    let worker = this
        .as_object()
        .and_then(|o| o.as_worker_object())
        .expect("Worker.terminate on non-worker");

    worker.set_state(activation.gc(), WorkerState::Terminated);
    worker.set_root(activation.gc(), None);

    let avm2 = &mut activation.context.avm2;
    avm2.workers.retain(|w| !WorkerObject::ptr_eq(*w, worker));
    avm2.worker_boot_queue
        .retain(|w| !WorkerObject::ptr_eq(*w, worker));
    avm2.worker_state_queue
        .retain(|w| !WorkerObject::ptr_eq(*w, worker));

    Ok(Value::Undefined)
}
