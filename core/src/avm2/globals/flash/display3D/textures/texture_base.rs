use crate::avm2::Activation;
use crate::avm2::Error;
use crate::avm2::Value;
use crate::avm2::function::FunctionArgs;

pub fn dispose<'gc>(
    _activation: &mut Activation<'_, 'gc>,
    this: Value<'gc>,
    _args: FunctionArgs<'_, 'gc>,
) -> Result<Value<'gc>, Error<'gc>> {
    let this = this.as_object().unwrap();
    if let Some(texture) = this.as_texture() {
        texture.dispose();
    }
    Ok(Value::Undefined)
}
