use crate::avm2::Error;
use crate::avm2::Value;
use crate::avm2::activation::Activation;
use crate::avm2::error::make_error_3669;
use crate::avm2::function::FunctionArgs;
use crate::avm2::parameters::ParametersExt;

pub fn upload_from_byte_array<'gc>(
    activation: &mut Activation<'_, 'gc>,
    this: Value<'gc>,
    args: FunctionArgs<'_, 'gc>,
) -> Result<Value<'gc>, Error<'gc>> {
    let this = this.as_object().unwrap();

    if let Some(vertex_buffer) = this.as_vertex_buffer() {
        let byte_array = args.get_object(activation, 0, "data")?;

        let Some(byte_array) = byte_array.as_bytearray() else {
            return Err(make_error_3669(activation));
        };

        let byte_offset = args.get_u32(1);
        let start_vertex = args.get_u32(2) as usize;
        let num_vertices = args.get_u32(3) as usize;

        let capacity = vertex_buffer.num_vertices() as usize;
        if start_vertex > capacity || num_vertices > capacity - start_vertex {
            return Err(make_error_3669(activation));
        }

        let values = num_vertices
            .checked_mul(vertex_buffer.data32_per_vertex() as usize)
            .ok_or_else(|| make_error_3669(activation))?;
        let data = byte_array
            .read_at(
                values
                    .checked_mul(std::mem::size_of::<f32>())
                    .ok_or_else(|| make_error_3669(activation))?,
                byte_offset as usize,
            )
            .map_err(|e| e.to_avm(activation))?;

        vertex_buffer.context3d().upload_vertex_buffer_data(
            vertex_buffer,
            data,
            start_vertex,
            vertex_buffer.data32_per_vertex(),
        );
    }
    Ok(Value::Undefined)
}

pub fn upload_from_vector<'gc>(
    activation: &mut Activation<'_, 'gc>,
    this: Value<'gc>,
    args: FunctionArgs<'_, 'gc>,
) -> Result<Value<'gc>, Error<'gc>> {
    let this = this.as_object().unwrap();

    if let Some(vertex_buffer) = this.as_vertex_buffer() {
        let vector = args.get_object(activation, 0, "data")?;

        let Some(vector) = vector.as_vector_storage() else {
            return Err(make_error_3669(activation));
        };

        let start_vertex = args.get_u32(1) as usize;
        let num_vertices = args.get_u32(2) as usize;

        let capacity = vertex_buffer.num_vertices() as usize;
        if start_vertex > capacity || num_vertices > capacity - start_vertex {
            return Err(make_error_3669(activation));
        }
        let values = num_vertices
            .checked_mul(vertex_buffer.data32_per_vertex() as usize)
            .ok_or_else(|| make_error_3669(activation))?;
        if vector.length() < values {
            return Err(make_error_3669(activation));
        }

        let data = vector
            .iter()
            .map(|val| val.as_f64() as f32)
            .take(values)
            .collect::<Vec<_>>();

        let data_bytes = bytemuck::cast_slice::<f32, u8>(&data);

        vertex_buffer.context3d().upload_vertex_buffer_data(
            vertex_buffer,
            data_bytes,
            start_vertex,
            vertex_buffer.data32_per_vertex(),
        );
    }
    Ok(Value::Undefined)
}

pub fn dispose<'gc>(
    _activation: &mut Activation<'_, 'gc>,
    this: Value<'gc>,
    _args: FunctionArgs<'_, 'gc>,
) -> Result<Value<'gc>, Error<'gc>> {
    if let Some(vertex_buffer) = this.as_object().unwrap().as_vertex_buffer() {
        vertex_buffer.dispose();
    }
    Ok(Value::Undefined)
}
