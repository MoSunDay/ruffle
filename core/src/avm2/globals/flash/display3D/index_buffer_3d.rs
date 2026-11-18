use crate::avm2::Activation;
use crate::avm2::Error;
use crate::avm2::Value;
use crate::avm2::error::make_error_3669;
use crate::avm2::function::FunctionArgs;
use crate::avm2::parameters::ParametersExt;

pub fn upload_from_byte_array<'gc>(
    activation: &mut Activation<'_, 'gc>,
    this: Value<'gc>,
    args: FunctionArgs<'_, 'gc>,
) -> Result<Value<'gc>, Error<'gc>> {
    let this = this.as_object().unwrap();

    if let Some(index_buffer) = this.as_index_buffer() {
        let byte_array = args.get_object(activation, 0, "byteArray")?;
        let Some(byte_array) = byte_array.as_bytearray() else {
            return Err(make_error_3669(activation));
        };

        let byte_offset = args.get_u32(1);
        let start_offset = args.get_u32(2) as usize;
        let count = args.get_u32(3) as usize;

        let capacity = index_buffer.count();
        if start_offset > capacity || count > capacity - start_offset {
            return Err(make_error_3669(activation));
        }

        let data = byte_array
            // Each index is always 16 bits (2 bytes)
            .read_at(count * 2, byte_offset as usize)
            .map_err(|e| e.to_avm(activation))?;

        index_buffer
            .context3d()
            .upload_index_buffer_data(index_buffer, data, start_offset);
    }
    Ok(Value::Undefined)
}

pub fn upload_from_vector<'gc>(
    activation: &mut Activation<'_, 'gc>,
    this: Value<'gc>,
    args: FunctionArgs<'_, 'gc>,
) -> Result<Value<'gc>, Error<'gc>> {
    let this = this.as_object().unwrap();

    if let Some(index_buffer) = this.as_index_buffer() {
        let vector = args.get_object(activation, 0, "data")?;

        let Some(vector) = vector.as_vector_storage() else {
            return Err(make_error_3669(activation));
        };

        let start_offset = args.get_u32(1) as usize;
        let count = args.get_u32(2) as usize;

        let capacity = index_buffer.count();
        if start_offset > capacity || count > capacity - start_offset || vector.length() < count {
            return Err(make_error_3669(activation));
        }

        let data = vector
            .iter()
            .map(|val| {
                // FIXME - use the low 16 bytes
                val.as_u32() as u16
            })
            .take(count)
            .collect::<Vec<_>>();

        let data_bytes = bytemuck::cast_slice::<u16, u8>(&data);

        index_buffer
            .context3d()
            .upload_index_buffer_data(index_buffer, data_bytes, start_offset);
    }
    Ok(Value::Undefined)
}

pub fn dispose<'gc>(
    _activation: &mut Activation<'_, 'gc>,
    this: Value<'gc>,
    _args: FunctionArgs<'_, 'gc>,
) -> Result<Value<'gc>, Error<'gc>> {
    if let Some(index_buffer) = this.as_object().unwrap().as_index_buffer() {
        index_buffer.dispose();
    }
    Ok(Value::Undefined)
}
