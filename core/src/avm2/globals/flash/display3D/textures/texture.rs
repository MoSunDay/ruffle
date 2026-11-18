use ruffle_render::backend::Context3DTextureFormat;

use super::atf_jpegxr::do_compressed_upload;
use super::dxt::{decode_dxt1, decode_dxt5};
use crate::avm2::Activation;
use crate::avm2::Value;
use crate::avm2::error::{make_error_1034, make_error_3669};
use crate::avm2::function::FunctionArgs;
use crate::avm2::object::TextureObject;
use crate::avm2::parameters::ParametersExt;
use crate::avm2::{Error, Object};

pub fn do_copy<'gc>(
    activation: &mut Activation<'_, 'gc>,
    data: Object<'gc>,
    texture: TextureObject<'gc>,
    byte_array_offset: u32,
    side: u32,
    mip_level: u32,
) -> Result<(), Error<'gc>> {
    let width = (texture.handle().width() >> mip_level).max(1);
    let height = (texture.handle().height() >> mip_level).max(1);
    let format = texture.original_format();
    let (source, pixels) = match format {
        Context3DTextureFormat::Bgra => (width as usize * height as usize * 4, true),
        Context3DTextureFormat::BgraPacked | Context3DTextureFormat::BgrPacked => {
            (width as usize * height as usize * 2, true)
        }
        Context3DTextureFormat::RgbaHalfFloat => (width as usize * height as usize * 8, false),
        Context3DTextureFormat::Compressed => {
            let blocks = width.div_ceil(4) as usize * height.div_ceil(4) as usize;
            (blocks * 8, false)
        }
        Context3DTextureFormat::CompressedAlpha => {
            let blocks = width.div_ceil(4) as usize * height.div_ceil(4) as usize;
            (blocks * 16, false)
        }
    };

    let Some(bytearray) = data.as_bytearray() else {
        let bytearray_class = activation.avm2().classes().bytearray;
        return Err(make_error_1034(
            activation,
            Value::Object(data),
            bytearray_class.inner_class_definition(),
        ));
    };
    let input = bytearray
        .read_at(source, byte_array_offset as usize)
        .map_err(|_| make_error_3669(activation))?;

    let output = if pixels {
        let mut rgba = Vec::with_capacity((width * height * 4) as usize);
        match format {
            Context3DTextureFormat::Bgra => {
                for pixel in input.as_chunks::<4>().0 {
                    // The ByteArray is in BGRA format.
                    rgba.extend_from_slice(&[pixel[2], pixel[1], pixel[0], pixel[3]]);
                }
            }
            Context3DTextureFormat::BgraPacked => {
                for pixel in input.as_chunks::<2>().0 {
                    let value = u16::from_le_bytes([pixel[0], pixel[1]]);
                    // Little-endian BGRA4444: blue, green, red, alpha.
                    rgba.push(expand_4bits(((value >> 8) & 0xF) as u8));
                    rgba.push(expand_4bits(((value >> 4) & 0xF) as u8));
                    rgba.push(expand_4bits((value & 0xF) as u8));
                    rgba.push(expand_4bits(((value >> 12) & 0xF) as u8));
                }
            }
            Context3DTextureFormat::BgrPacked => {
                for pixel in input.as_chunks::<2>().0 {
                    let value = u16::from_le_bytes([pixel[0], pixel[1]]);
                    rgba.push(expand_5bits(((value >> 11) & 0x1F) as u8));
                    rgba.push(expand_6bits(((value >> 5) & 0x3F) as u8));
                    rgba.push(expand_5bits((value & 0x1F) as u8));
                    rgba.push(255);
                }
            }
            _ => return Err(make_error_3669(activation)),
        }
        rgba
    } else if matches!(format, Context3DTextureFormat::Compressed) {
        decode_dxt1(&input, width, height)
    } else if matches!(format, Context3DTextureFormat::CompressedAlpha) {
        decode_dxt5(&input, width, height)
    } else {
        input.to_vec()
    };

    texture
        .context3d()
        .copy_pixels_to_texture(output, texture.handle(), side, mip_level);
    let _ = activation;
    Ok(())
}

fn expand_4bits(value: u8) -> u8 {
    value * 17
}

fn expand_5bits(value: u8) -> u8 {
    (value << 3) | (value >> 2)
}

fn expand_6bits(value: u8) -> u8 {
    (value << 2) | (value >> 4)
}

pub fn upload_compressed_texture_from_byte_array_internal<'gc>(
    activation: &mut Activation<'_, 'gc>,
    this: Value<'gc>,
    args: FunctionArgs<'_, 'gc>,
) -> Result<Value<'gc>, Error<'gc>> {
    let this = this.as_object().unwrap();

    let texture = this.as_texture().unwrap();
    let data = args.get_object(activation, 0, "data")?;
    let byte_array_offset = args.get_u32(1) as usize;

    do_compressed_upload(activation, texture, data, byte_array_offset, false)?;

    Ok(Value::Undefined)
}

pub fn upload_from_byte_array<'gc>(
    activation: &mut Activation<'_, 'gc>,
    this: Value<'gc>,
    args: FunctionArgs<'_, 'gc>,
) -> Result<Value<'gc>, Error<'gc>> {
    let this = this.as_object().unwrap();

    let texture = this.as_texture().unwrap();
    let data = args.get_object(activation, 0, "data")?;
    let byte_array_offset = args.get_u32(1);
    let mip_level = args.get_u32(2);

    do_copy(activation, data, texture, byte_array_offset, 0, mip_level)?;
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
            let mip_level = args.get_u32(1);
            texture.context3d().copy_bitmapdata_to_texture(
                &source.sync(activation.context.renderer).borrow(),
                texture.handle(),
                0,
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
