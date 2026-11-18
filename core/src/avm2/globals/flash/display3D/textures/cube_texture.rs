use crate::avm2::Activation;
use crate::avm2::Error;
use crate::avm2::Value;
use crate::avm2::error::make_error_1034;
use crate::avm2::function::FunctionArgs;
use crate::avm2::globals::flash::display3D::textures::atf_jpegxr::do_compressed_upload;
use crate::avm2::parameters::ParametersExt;

use super::texture::do_copy;

pub fn upload_from_byte_array<'gc>(
    activation: &mut Activation<'_, 'gc>,
    this: Value<'gc>,
    args: FunctionArgs<'_, 'gc>,
) -> Result<Value<'gc>, Error<'gc>> {
    let this = this.as_object().unwrap();

    let texture = this.as_texture().unwrap();
    let data = args.get_object(activation, 0, "data")?;
    let byte_array_offset = args.get_u32(1);
    let side = args.get_u32(2);
    let mip_level = args.get_u32(3);

    do_copy(
        activation,
        data,
        texture,
        byte_array_offset,
        side,
        mip_level,
    )?;
    Ok(Value::Undefined)
}

pub fn upload_compressed_texture_from_byte_array<'gc>(
    activation: &mut Activation<'_, 'gc>,
    this: Value<'gc>,
    args: FunctionArgs<'_, 'gc>,
) -> Result<Value<'gc>, Error<'gc>> {
    let this = this.as_object().unwrap();

    let texture = this.as_texture().unwrap();
    let data = args.get_object(activation, 0, "data")?;
    let byte_array_offset = args.get_u32(1) as usize;
    // The upload is synchronous and copies the data eagerly, so the async
    // hint can be accepted without changing behaviour.
    let _async_ = args.get_bool(2);

    do_compressed_upload(activation, texture, data, byte_array_offset, true)?;
    Ok(Value::Undefined)
}

pub fn upload_from_bitmap_data<'gc>(
    activation: &mut Activation<'_, 'gc>,
    this: Value<'gc>,
    args: FunctionArgs<'_, 'gc>,
) -> Result<Value<'gc>, Error<'gc>> {
    let this = this.as_object().unwrap();

    if let Some(texture) = this.as_texture() {
        let source_obj = args.get_object(activation, 0, "source")?;

        if let Some(source) = source_obj.as_bitmap_data() {
            let side = args.get_u32(1);
            let mip_level = args.get_u32(2);
            texture.context3d().copy_bitmapdata_to_texture(
                &source.sync(activation.context.renderer).borrow(),
                texture.handle(),
                side,
                mip_level,
            );
        } else {
            let bitmapdata_class = activation.avm2().classes().bitmapdata;
            return Err(make_error_1034(
                activation,
                Value::Object(source_obj),
                bitmapdata_class.inner_class_definition(),
            ));
        }
    }
    Ok(Value::Undefined)
}
