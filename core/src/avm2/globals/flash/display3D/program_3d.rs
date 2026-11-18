use crate::avm2::Error;
use crate::avm2::activation::Activation;
use crate::avm2::error::{make_agal_upload_error, make_error_3669};
use crate::avm2::function::FunctionArgs;
use crate::avm2::parameters::ParametersExt;
use crate::avm2::value::Value;

pub fn upload<'gc>(
    activation: &mut Activation<'_, 'gc>,
    this: Value<'gc>,
    args: FunctionArgs<'_, 'gc>,
) -> Result<Value<'gc>, Error<'gc>> {
    let this = this.as_object().unwrap();

    if let Some(this) = this.as_program_3d() {
        let vertex_agal = args.get_object(activation, 0, "source_vertex")?;
        let Some(vertex_agal) = vertex_agal.as_bytearray() else {
            return Err(make_error_3669(activation));
        };
        let vertex_agal = vertex_agal.bytes().to_vec();

        let fragment_agal = args.get_object(activation, 1, "source_fragment")?;
        let Some(fragment_agal) = fragment_agal.as_bytearray() else {
            return Err(make_error_3669(activation));
        };
        let fragment_agal = fragment_agal.bytes().to_vec();

        this.context3d()
            .upload_shaders(this, vertex_agal, fragment_agal)
            .map_err(|e| make_agal_upload_error(activation, e))?;
    }
    Ok(Value::Undefined)
}

pub fn dispose<'gc>(
    _activation: &mut Activation<'_, 'gc>,
    this: Value<'gc>,
    _args: FunctionArgs<'_, 'gc>,
) -> Result<Value<'gc>, Error<'gc>> {
    if let Some(program) = this.as_object().unwrap().as_program_3d() {
        program.dispose();
    }
    Ok(Value::Undefined)
}
