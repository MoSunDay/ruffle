use naga::valid::{Capabilities, ValidationFlags, Validator};
use naga_agal::{Filter, SamplerConfig, VertexAttributeFormat, Wrapping};
use ruffle_render::backend::{
    Context3DCompareMode, Context3DStencilAction, Context3DTextureFilter, Context3DTriangleFace,
    Context3DVertexBufferFormat, Context3DWrapMode, Texture,
};

use wgpu::{
    BindGroupEntry, BindingResource, BufferDescriptor, BufferUsages, FrontFace, TextureView,
};
use wgpu::{Buffer, DepthStencilState};
use wgpu::{ColorTargetState, RenderPipelineDescriptor, TextureFormat, VertexState};

pub(super) trait IntoWgpu {
    type WgpuType;

    fn into_wgpu(self) -> Self::WgpuType;
}

impl IntoWgpu for Context3DCompareMode {
    type WgpuType = wgpu::CompareFunction;

    fn into_wgpu(self) -> wgpu::CompareFunction {
        match self {
            Context3DCompareMode::Always => wgpu::CompareFunction::Always,
            Context3DCompareMode::Equal => wgpu::CompareFunction::Equal,
            Context3DCompareMode::Greater => wgpu::CompareFunction::Greater,
            Context3DCompareMode::GreaterEqual => wgpu::CompareFunction::GreaterEqual,
            Context3DCompareMode::Less => wgpu::CompareFunction::Less,
            Context3DCompareMode::LessEqual => wgpu::CompareFunction::LessEqual,
            Context3DCompareMode::Never => wgpu::CompareFunction::Never,
            Context3DCompareMode::NotEqual => wgpu::CompareFunction::NotEqual,
        }
    }
}

impl IntoWgpu for Context3DStencilAction {
    type WgpuType = wgpu::StencilOperation;

    fn into_wgpu(self) -> wgpu::StencilOperation {
        match self {
            Context3DStencilAction::DecrementSaturate => wgpu::StencilOperation::DecrementClamp,
            Context3DStencilAction::DecrementWrap => wgpu::StencilOperation::DecrementWrap,
            Context3DStencilAction::IncrementSaturate => wgpu::StencilOperation::IncrementClamp,
            Context3DStencilAction::IncrementWrap => wgpu::StencilOperation::IncrementWrap,
            Context3DStencilAction::Invert => wgpu::StencilOperation::Invert,
            Context3DStencilAction::Keep => wgpu::StencilOperation::Keep,
            Context3DStencilAction::Set => wgpu::StencilOperation::Replace,
            Context3DStencilAction::Zero => wgpu::StencilOperation::Zero,
        }
    }
}

use std::cell::Cell;
use std::num::NonZeroU64;
use std::rc::Rc;

use crate::context3d::VertexBufferWrapper;
use crate::context3d::shader_pair::{ShaderCompileData, ShaderTextureInfo};
use crate::descriptors::Descriptors;

use super::pipeline_cache::{
    BindGroupKey, CachedPipeline, PipelineCache, PipelineKey, ShaderIdentity,
};
use super::uniform_ring::{
    FRAGMENT_SHADER_UNIFORMS_BUFFER_SIZE, UniformRing, VERTEX_SHADER_UNIFORMS_BUFFER_SIZE,
};
use super::{MAX_VERTEX_ATTRIBUTES, ShaderPairAgal, VertexAttributeInfo};

pub(super) const AGAL_FLOATS_PER_REGISTER: u64 = 4;

fn vertex_format(format: Context3DVertexBufferFormat) -> VertexAttributeFormat {
    match format {
        Context3DVertexBufferFormat::Float4 => VertexAttributeFormat::Float4,
        Context3DVertexBufferFormat::Float3 => VertexAttributeFormat::Float3,
        Context3DVertexBufferFormat::Float2 => VertexAttributeFormat::Float2,
        Context3DVertexBufferFormat::Float1 => VertexAttributeFormat::Float1,
        Context3DVertexBufferFormat::Bytes4 => VertexAttributeFormat::Bytes4,
    }
}

// The flash Context3D API is similar to OpenGL - it has many methods
// which modify the current state (`setVertexBufferAt`, `setCulling`, etc.)
// These methods can be called at any time.
//
// In WGPU, this state is associated by a `RenderPipeline` object,
// which needs to be rebuilt whenever the state changes.
//
// To match up these APIs, we store the current state in `CurrentPipeline`.
// Whenever a state-changing `Context3DCommand` is executed, we mark the `CurrentPipeline`
// as dirty. When a `wgpu::RenderPipeline` is actually needed by `drawTriangles`,
// we build a new `wgpu::RenderPipeline` from the `CurrentPipeline` state (if it's dirty).
//
// The `CurrentPipeline` state (including the compiled `wgpu::RenderPipeline`) is stored
// in `WgpuContext3D`, and is re-used across calls to `present`. Due to lifetime issues,
// we don't actually store the `wgpu::RenderPipeline` in `CurrentPipeline` - it's
// instead stored in `WgpuContext3D`.
pub struct CurrentPipeline {
    shaders: Option<Rc<ShaderPairAgal>>,

    culling: Context3DTriangleFace,

    bound_textures: [Option<BoundTextureData>; 8],

    pub vertex_shader_uniforms: Buffer,
    pub fragment_shader_uniforms: Buffer,
    uniform_ring: UniformRing,

    has_depth_texture: bool,

    color_mask: wgpu::ColorWrites,

    depth_mask: bool,
    pass_compare_mode: wgpu::CompareFunction,

    /// Stencil test configuration (face operations, compare functions, masks), set via `setStencilActions`.
    stencil: wgpu::StencilState,
    /// Reference value used for stencil comparison and operations, set via `setStencilReferenceValue`.
    stencil_ref_value: u32,

    color_component: wgpu::BlendComponent,
    alpha_component: wgpu::BlendComponent,

    sample_count: u32,

    target_format: TextureFormat,

    dirty: Cell<bool>,
    cache: PipelineCache,

    // Sampler configuration information for each texture slot.
    // This is updated by `Context3D.setSamplerStateAt`, as well
    // as in `Context3D.setProgram` (based on the sampling opcodes
    // in the program). All texture slots have a sampler set by default
    // (which allows rendering with an 'ignoresampler' tex opcode,
    // and no calls to Context3D.setSamplerStateAt)
    sampler_configs: [SamplerConfig; 8],
}

#[derive(Clone)]
pub struct BoundTextureData {
    /// This is used to allow us to remove a bound texture when
    /// it's used with `setRenderToTexture`. The actual shader binding
    /// uses `view`
    pub id: Rc<dyn Texture>,
    pub view: TextureView,
    pub cube: bool,
}

impl CurrentPipeline {
    pub fn new(descriptors: &Descriptors) -> Self {
        let vertex_shader_uniforms = descriptors.device.create_buffer(&BufferDescriptor {
            label: Some("Vertex shader uniforms"),
            size: VERTEX_SHADER_UNIFORMS_BUFFER_SIZE,
            usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST | BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });

        let fragment_shader_uniforms = descriptors.device.create_buffer(&BufferDescriptor {
            label: Some("Fragment shader uniforms"),
            size: FRAGMENT_SHADER_UNIFORMS_BUFFER_SIZE,
            usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST | BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });

        let uniform_ring = UniformRing::new(&descriptors.device);

        CurrentPipeline {
            shaders: None,
            bound_textures: std::array::from_fn(|_| None),
            vertex_shader_uniforms,
            fragment_shader_uniforms,
            uniform_ring,
            dirty: Cell::new(true),
            cache: PipelineCache::new(),
            culling: Context3DTriangleFace::None,

            has_depth_texture: false,

            color_mask: wgpu::ColorWrites::ALL,

            depth_mask: true,
            pass_compare_mode: wgpu::CompareFunction::LessEqual,

            stencil: wgpu::StencilState {
                front: wgpu::StencilFaceState::IGNORE,
                back: wgpu::StencilFaceState::IGNORE,
                read_mask: u32::MAX,
                write_mask: u32::MAX,
            },
            stencil_ref_value: 0,

            color_component: wgpu::BlendComponent::REPLACE,
            alpha_component: wgpu::BlendComponent::REPLACE,
            sample_count: 1,

            target_format: TextureFormat::Rgba8Unorm,

            sampler_configs: [SamplerConfig::default(); 8],
        }
    }

    pub fn stencil_ref_value(&self) -> u32 {
        self.stencil_ref_value
    }

    /// Snapshots the current program constants for one draw. Returns the
    /// dynamic vertex/fragment offsets to bind.
    pub fn prepare_uniforms(&self, encoder: &mut wgpu::CommandEncoder) -> Option<(u32, u32)> {
        self.uniform_ring.prepare(
            encoder,
            &self.vertex_shader_uniforms,
            &self.fragment_shader_uniforms,
        )
    }

    pub fn reset_uniform_ring(&self) {
        self.uniform_ring.reset();
    }

    pub fn set_shaders(&mut self, shaders: Option<Rc<ShaderPairAgal>>) {
        let shaders_unchanged = match (&self.shaders, &shaders) {
            (Some(old), Some(new)) => Rc::ptr_eq(old, new),
            (None, None) => true,
            _ => false,
        };
        self.shaders = shaders;

        // `setProgram` always applies the fragment shader's sampler configs, even
        // when it is the same program that is already bound. The program may have
        // been re-selected after `setSamplerStateAt`, which has to be overridden.
        let mut samplers_changed = false;
        if let Some(shaders) = &self.shaders {
            for (i, sampler_config) in shaders.fragment_sampler_configs().iter().enumerate() {
                // When we call `Context3D.setProgram`, sampler configs from the fragment shader override
                // any previously set sampler configs (if 'ignoresampler' was set in the program, then the corresponding
                // array entry will be `None`).
                if let Some(sampler_config) = sampler_config {
                    if self.sampler_configs[i] != *sampler_config {
                        self.sampler_configs[i] = *sampler_config;
                        samplers_changed = true;
                    }
                }
            }
        }

        if !shaders_unchanged || samplers_changed {
            self.dirty.set(true);
        }
    }

    pub fn has_unbound_required_textures(&self) -> bool {
        if let Some(shaders) = &self.shaders {
            for (i, sampler_config) in shaders.fragment_sampler_configs().iter().enumerate() {
                if sampler_config.is_some() && self.bound_textures[i].is_none() {
                    return true;
                }
            }
        }
        false
    }

    /// Returns true if `texture` is already bound to `index` (with the same
    /// cube-map flag), so re-creating the bind group can be skipped.
    pub fn texture_is_bound(&self, index: usize, texture: &Rc<dyn Texture>, cube: bool) -> bool {
        self.bound_textures[index]
            .as_ref()
            .is_some_and(|bound| bound.cube == cube && Rc::ptr_eq(&bound.id, texture))
    }

    /// Returns true if no texture is bound to `index`.
    pub fn texture_slot_is_empty(&self, index: usize) -> bool {
        self.bound_textures[index].is_none()
    }

    pub fn update_texture_at(&mut self, index: usize, texture: Option<BoundTextureData>) {
        // FIXME - determine if the texture actually changed
        self.dirty.set(true);
        self.bound_textures[index] = texture;
    }

    pub fn remove_texture(&mut self, texture: &Rc<dyn Texture>) {
        for i in 0..self.bound_textures.len() {
            if let Some(bound_texture) = &self.bound_textures[i] {
                // Ignore the vtable pointer
                if std::ptr::eq(
                    Rc::as_ptr(&bound_texture.id) as *const (),
                    Rc::as_ptr(texture) as *const (),
                ) {
                    self.update_texture_at(i, None);
                }
            }
        }
    }

    pub fn update_vertex_buffer_at(&self, _index: usize) {
        // FIXME - check if it's the same, so we can skip rebuilding the pipeline
        self.dirty.set(true);
    }

    pub fn update_color_mask(&mut self, color_mask: wgpu::ColorWrites) {
        if self.color_mask != color_mask {
            self.dirty.set(true);
        }
        self.color_mask = color_mask;
    }

    pub fn update_depth(&mut self, depth_mask: bool, pass_compare_mode: wgpu::CompareFunction) {
        if self.depth_mask != depth_mask || self.pass_compare_mode != pass_compare_mode {
            self.dirty.set(true);
        }
        self.depth_mask = depth_mask;
        self.pass_compare_mode = pass_compare_mode;
    }

    pub fn update_has_depth_texture(&mut self, has_depth_texture: bool) {
        if self.has_depth_texture != has_depth_texture {
            self.dirty.set(true);
            self.has_depth_texture = has_depth_texture;
        }
    }

    pub fn update_sample_count(&mut self, sample_count: u32) {
        if self.sample_count != sample_count {
            self.dirty.set(true);
            self.sample_count = sample_count;
        }
    }

    pub fn update_target_format(&mut self, format: TextureFormat) {
        if self.target_format != format {
            self.dirty.set(true);
            self.target_format = format;
        }
    }

    /// If the pipeline is dirty, recompiles it and returns `Some(freshly_compiled_pipeline`)
    /// Otherwise, returns `None`.
    ///
    /// If the shaders fail to compile, the pipeline stays dirty.
    fn make_pipeline_key(
        &self,
        vertex_attributes: &[Option<VertexAttributeInfo>; MAX_VERTEX_ATTRIBUTES],
    ) -> PipelineKey {
        let mut texture_infos = [None; 8];
        for (i, bound_texture) in self.bound_textures.iter().enumerate() {
            texture_infos[i] = bound_texture
                .as_ref()
                .map(|bound_texture| bound_texture.cube);
        }

        let vertex_attributes = vertex_attributes.clone().map(|attr| {
            attr.map(|attr| {
                (
                    attr.buffer.data_32_per_vertex,
                    vertex_format(attr.format),
                    attr.offset_in_32bit_units,
                )
            })
        });

        PipelineKey {
            shaders: self.shaders.clone().map(ShaderIdentity::new),
            culling: match self.culling {
                Context3DTriangleFace::None => 0,
                Context3DTriangleFace::Back => 1,
                Context3DTriangleFace::Front => 2,
                Context3DTriangleFace::FrontAndBack => 3,
            },
            texture_infos,
            vertex_attributes,
            has_depth_texture: self.has_depth_texture,
            color_mask: self.color_mask,
            depth_mask: self.depth_mask,
            pass_compare_mode: self.pass_compare_mode,
            stencil: self.stencil.clone(),
            color_component: self.color_component,
            alpha_component: self.alpha_component,
            sample_count: self.sample_count,
            target_format: self.target_format,
        }
    }

    fn make_bind_group_key(
        &self,
        pipeline: PipelineKey,
        samplers: &[&wgpu::Sampler; 8],
    ) -> BindGroupKey {
        BindGroupKey {
            pipeline,
            textures: std::array::from_fn(|i| {
                self.bound_textures[i]
                    .as_ref()
                    .map(|texture| texture.view.clone())
            }),
            cubes: std::array::from_fn(|i| {
                self.bound_textures[i]
                    .as_ref()
                    .is_some_and(|texture| texture.cube)
            }),
            samplers: std::array::from_fn(|i| (*samplers[i]).clone()),
        }
    }

    fn build_bind_group(
        &self,
        descriptors: &Descriptors,
        layout: &wgpu::BindGroupLayout,
        wgpu_samplers: &[&wgpu::Sampler; 8],
    ) -> wgpu::BindGroup {
        let (vertex_uniforms, fragment_uniforms) = self.uniform_ring.buffers();
        let mut bind_group_entries = vec![
            BindGroupEntry {
                binding: 0,
                resource: BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: vertex_uniforms,
                    offset: 0,
                    size: Some(NonZeroU64::new(VERTEX_SHADER_UNIFORMS_BUFFER_SIZE).unwrap()),
                }),
            },
            BindGroupEntry {
                binding: 1,
                resource: BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: fragment_uniforms,
                    offset: 0,
                    size: Some(NonZeroU64::new(FRAGMENT_SHADER_UNIFORMS_BUFFER_SIZE).unwrap()),
                }),
            },
        ];

        for (i, bound_texture) in self.bound_textures.iter().enumerate() {
            if let Some(bound_texture) = bound_texture {
                bind_group_entries.push(BindGroupEntry {
                    binding: naga_agal::TEXTURE_START_BIND_INDEX + i as u32,
                    resource: BindingResource::TextureView(&bound_texture.view),
                });
                bind_group_entries.push(BindGroupEntry {
                    binding: naga_agal::TEXTURE_SAMPLER_START_BIND_INDEX + i as u32,
                    resource: BindingResource::Sampler(wgpu_samplers[i]),
                });
            }
        }

        let bind_group_label = create_debug_label!("Bind group");
        descriptors
            .device
            .create_bind_group(&wgpu::BindGroupDescriptor {
                label: bind_group_label.as_deref(),
                layout,
                entries: &bind_group_entries,
            })
    }

    fn build_pipeline(
        &self,
        descriptors: &Descriptors,
        vertex_attributes: &[Option<VertexAttributeInfo>; MAX_VERTEX_ATTRIBUTES],
    ) -> Result<CachedPipeline, naga_agal::Error> {
        let agal_attributes = vertex_attributes.clone().map(|attr| {
            attr.map(|attr| match attr.format {
                Context3DVertexBufferFormat::Float4 => naga_agal::VertexAttributeFormat::Float4,
                Context3DVertexBufferFormat::Float3 => naga_agal::VertexAttributeFormat::Float3,
                Context3DVertexBufferFormat::Float2 => naga_agal::VertexAttributeFormat::Float2,
                Context3DVertexBufferFormat::Float1 => naga_agal::VertexAttributeFormat::Float1,
                Context3DVertexBufferFormat::Bytes4 => naga_agal::VertexAttributeFormat::Bytes4,
            })
        });

        let mut texture_infos = [None; 8];
        for (i, bound_texture) in self.bound_textures.iter().enumerate() {
            if let Some(bound_texture) = bound_texture {
                if bound_texture.cube {
                    texture_infos[i] = Some(ShaderTextureInfo::Cube);
                } else {
                    texture_infos[i] = Some(ShaderTextureInfo::D2);
                }
            }
        }

        let compiled_shaders = self.shaders.as_ref().expect("Missing shaders!").compile(
            descriptors,
            ShaderCompileData {
                vertex_attributes: agal_attributes,
                texture_infos,
            },
        )?;

        let bind_group_layout = compiled_shaders.bind_group_layout.clone();
        let pipeline_layout_label = create_debug_label!("Pipeline layout");
        let pipeline_layout =
            descriptors
                .device
                .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: pipeline_layout_label.as_deref(),
                    bind_group_layouts: &[Some(&bind_group_layout)],
                    immediate_size: 0,
                });

        struct BufferData {
            buffer: Rc<VertexBufferWrapper>,
            attrs: Vec<wgpu::VertexAttribute>,
            total_size: usize,
        }

        // The user can call Context3D.setVertexBufferAt with a a mixture of vertex buffers.
        // We need to create one 'BufferData' struct for each distinct vertex buffer
        // across all of the calls to 'setVertexBufferAt'. The 'BufferData' keeps track
        // of all of the bound indices associated with that buffer.
        let mut index_per_buffer: Vec<BufferData> = Vec::new();

        for (i, attr) in vertex_attributes.iter().enumerate() {
            if let Some(attr) = attr {
                let (format, entry_size_bytes) = match attr.format {
                    Context3DVertexBufferFormat::Float4 => (
                        wgpu::VertexFormat::Float32x4,
                        4 * std::mem::size_of::<f32>(),
                    ),
                    Context3DVertexBufferFormat::Float3 => (
                        wgpu::VertexFormat::Float32x3,
                        3 * std::mem::size_of::<f32>(),
                    ),
                    Context3DVertexBufferFormat::Float2 => (
                        wgpu::VertexFormat::Float32x2,
                        2 * std::mem::size_of::<f32>(),
                    ),
                    Context3DVertexBufferFormat::Float1 => {
                        (wgpu::VertexFormat::Float32, std::mem::size_of::<f32>())
                    }
                    // AGAL shaders always work with floating-point values, so
                    // we use Unorm8x4 to convert the bytes to floats in the range
                    // [0, 1].
                    Context3DVertexBufferFormat::Bytes4 => (wgpu::VertexFormat::Unorm8x4, 4),
                };

                let buffer_data = index_per_buffer
                    .iter_mut()
                    .find(|data| Rc::ptr_eq(&data.buffer, &attr.buffer));

                let buffer_data = if let Some(buffer_data) = buffer_data {
                    buffer_data
                } else {
                    index_per_buffer.push(BufferData {
                        buffer: attr.buffer.clone(),
                        attrs: Vec::new(),
                        total_size: 0,
                    });
                    index_per_buffer.last_mut().unwrap()
                };

                let offset_bytes = attr.offset_in_32bit_units * 4;

                // ActionScript may call setVertexBufferAt with overlapping offsets, e.g.:

                // ```
                // setVertexBufferAt(0, buffer, 0, "float3"]
                // setVertexBufferAt(1, buffer, 2, "float4"]
                // setVertexBufferAt(2, buffer, 6, "float2"]
                // ```
                //
                // To correctly compute the total size, we look at the furthest extent of any buffer
                // (its offset plus its length), rather than simply adding up all of the lengths.
                buffer_data.total_size = std::cmp::max(
                    buffer_data.total_size,
                    entry_size_bytes + offset_bytes as usize,
                );
                buffer_data.attrs.push(wgpu::VertexAttribute {
                    format,
                    offset: offset_bytes,
                    shader_location: i as u32,
                })
            }
        }

        let cull_mode = match self.culling {
            Context3DTriangleFace::Back => Some(wgpu::Face::Back),
            Context3DTriangleFace::Front => Some(wgpu::Face::Front),
            // Wgpu cannot express culling both faces. Flash treats
            // FRONT_AND_BACK as "draw nothing", but this game relies on the
            // triangles still being drawn (observed freeze otherwise), so we
            // fall back to drawing everything (matching prior behavior).
            Context3DTriangleFace::FrontAndBack => None,
            Context3DTriangleFace::None => None,
        };

        let depth_stencil = if self.has_depth_texture {
            Some(DepthStencilState {
                format: TextureFormat::Depth24PlusStencil8,
                depth_write_enabled: Some(self.depth_mask),
                depth_compare: Some(self.pass_compare_mode),
                stencil: self.stencil.clone(),
                bias: Default::default(),
            })
        } else {
            None
        };

        let wgpu_vertex_buffers = index_per_buffer
            .iter()
            .map(|data| {
                // This value is set when Context3D.createVertexBuffer is called.
                // We may not all of the data associated with a single vertex
                // (e.g. we might have 8 floats per vertex, but only
                // call setVertexBufferAt once to bind the first 4 floats per vertex.
                // However, the total size of the bindings can be at most the total
                // amount of data per vertex. Verify that here
                let data_bytes_per_vertex = (data.buffer.data_32_per_vertex * 4) as u64;
                if data.total_size > data_bytes_per_vertex as usize {
                    panic!("Total size of bound vertex attributes {:?} exceeds data_bytes_per_vertex {:?}", data.total_size,
                    data_bytes_per_vertex);
                }

                let attrs = &data.attrs;
                Some(wgpu::VertexBufferLayout {
                    array_stride: data_bytes_per_vertex,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: attrs,
                })
            })
            .collect::<Vec<_>>();

        let pipeline = descriptors
            .device
            .create_render_pipeline(&RenderPipelineDescriptor {
                label: create_debug_label!("RenderPipeline").as_deref(),
                layout: Some(&pipeline_layout),
                vertex: VertexState {
                    module: &compiled_shaders.vertex_module,
                    entry_point: Some(naga_agal::SHADER_ENTRY_POINT),
                    buffers: &wgpu_vertex_buffers,
                    compilation_options: Default::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &compiled_shaders.fragment_module,
                    entry_point: Some(naga_agal::SHADER_ENTRY_POINT),
                    targets: &[Some(ColorTargetState {
                        format: self.target_format,
                        blend: Some(wgpu::BlendState {
                            color: self.color_component,
                            alpha: self.alpha_component,
                        }),
                        write_mask: self.color_mask,
                    })],
                    compilation_options: Default::default(),
                }),
                primitive: wgpu::PrimitiveState {
                    // Stage3d appears to use clockwise winding:
                    // https://stackoverflow.com/questions/8677498/stage3d-culling-confusion
                    front_face: FrontFace::Cw,
                    cull_mode,
                    ..Default::default()
                },
                depth_stencil,
                multisample: wgpu::MultisampleState {
                    count: self.sample_count,
                    mask: !0,
                    alpha_to_coverage_enabled: false,
                },
                multiview_mask: None,
                cache: None,
            });

        // `pipeline_layout` is kept alive by the render pipeline internally.
        drop(pipeline_layout);
        Ok(CachedPipeline {
            pipeline,
            bind_group_layout,
        })
    }

    /// If the pipeline is dirty, rebuilds it from the cached pipeline/bind group
    /// state and returns the handles. Otherwise returns `None`.
    ///
    /// If the shaders fail to compile, the pipeline stays dirty.
    pub fn rebuild_pipeline(
        &self,
        descriptors: &Descriptors,
        vertex_attributes: &[Option<VertexAttributeInfo>; MAX_VERTEX_ATTRIBUTES],
    ) -> Result<Option<(wgpu::RenderPipeline, wgpu::BindGroup)>, naga_agal::Error> {
        if !self.dirty.get() {
            return Ok(None);
        }

        let key = self.make_pipeline_key(vertex_attributes);
        let cached = match self.cache.get_pipeline(&key) {
            Some(cached) => cached,
            None => {
                let cached = self.build_pipeline(descriptors, vertex_attributes)?;
                self.cache.put_pipeline(key.clone(), cached.clone());
                cached
            }
        };

        let wgpu_samplers = super::samplers::wgpu_samplers(descriptors, &self.sampler_configs);
        let bind_group_key = self.make_bind_group_key(key, &wgpu_samplers);
        let bind_group = match self.cache.get_bind_group(&bind_group_key) {
            Some(bind_group) => bind_group,
            None => {
                let bind_group =
                    self.build_bind_group(descriptors, &cached.bind_group_layout, &wgpu_samplers);
                self.cache
                    .put_bind_group(bind_group_key, bind_group.clone());
                bind_group
            }
        };

        self.dirty.set(false);
        Ok(Some((cached.pipeline, bind_group)))
    }

    pub fn set_culling(&mut self, face: Context3DTriangleFace) {
        if self.culling != face {
            self.culling = face;
            self.dirty.set(true);
        }
    }

    pub fn update_blend_factors(
        &mut self,
        color_component: wgpu::BlendComponent,
        alpha_component: wgpu::BlendComponent,
    ) {
        if color_component != self.color_component || alpha_component != self.alpha_component {
            self.color_component = color_component;
            self.alpha_component = alpha_component;
            self.dirty.set(true);
        }
    }

    pub(crate) fn update_sampler_state_at(
        &mut self,
        sampler: usize,
        wrap: ruffle_render::backend::Context3DWrapMode,
        filter: ruffle_render::backend::Context3DTextureFilter,
    ) {
        let sampler_config = SamplerConfig {
            wrapping: match wrap {
                Context3DWrapMode::Clamp => Wrapping::Clamp,
                Context3DWrapMode::Repeat => Wrapping::Repeat,
                Context3DWrapMode::ClampURepeatV => Wrapping::ClampURepeatV,
                Context3DWrapMode::RepeatUClampV => Wrapping::RepeatUClampV,
            },
            filter: match filter {
                Context3DTextureFilter::Linear => Filter::Linear,
                Context3DTextureFilter::Nearest => Filter::Nearest,
                Context3DTextureFilter::Anisotropic2X => Filter::Anisotropic2x,
                Context3DTextureFilter::Anisotropic4X => Filter::Anisotropic4x,
                Context3DTextureFilter::Anisotropic8X => Filter::Anisotropic8x,
                Context3DTextureFilter::Anisotropic16X => Filter::Anisotropic16x,
            },
            // FIXME - implement this
            mipmap: naga_agal::Mipmap::Disable,
        };
        if self.sampler_configs[sampler] != sampler_config {
            self.dirty.set(true);
            self.sampler_configs[sampler] = sampler_config;
        }
    }

    pub fn update_stencil_actions(
        &mut self,
        triangle_face: Context3DTriangleFace,
        compare_mode: Context3DCompareMode,
        on_both_pass: Context3DStencilAction,
        on_depth_fail: Context3DStencilAction,
        on_depth_pass_stencil_fail: Context3DStencilAction,
    ) {
        let stencil_state = wgpu::StencilFaceState {
            compare: compare_mode.into_wgpu(),
            fail_op: on_depth_pass_stencil_fail.into_wgpu(),
            depth_fail_op: on_depth_fail.into_wgpu(),
            pass_op: on_both_pass.into_wgpu(),
        };

        let (front, back) = match triangle_face {
            Context3DTriangleFace::None => (
                wgpu::StencilFaceState::IGNORE,
                wgpu::StencilFaceState::IGNORE,
            ),
            Context3DTriangleFace::Front => (stencil_state, wgpu::StencilFaceState::IGNORE),
            Context3DTriangleFace::Back => (wgpu::StencilFaceState::IGNORE, stencil_state),
            Context3DTriangleFace::FrontAndBack => (stencil_state, stencil_state),
        };

        if self.stencil.front != front || self.stencil.back != back {
            self.stencil.front = front;
            self.stencil.back = back;
            self.dirty.set(true);
        }
    }

    pub fn update_stencil_reference_value(
        &mut self,
        reference_value: u32,
        read_mask: u32,
        write_mask: u32,
    ) {
        if self.stencil.read_mask != read_mask || self.stencil.write_mask != write_mask {
            self.dirty.set(true);
            self.stencil.read_mask = read_mask;
            self.stencil.write_mask = write_mask;
        }

        self.stencil_ref_value = reference_value;
    }
}

// This is useful for debugging shader issues
#[expect(dead_code)]
fn to_wgsl(module: &naga::Module) -> String {
    let mut out = String::new();

    let mut validator = Validator::new(ValidationFlags::all(), Capabilities::all());
    let module_info = validator
        .validate(module)
        .unwrap_or_else(|e| panic!("Validation failed: {e:#?}"));

    let mut writer =
        naga::back::wgsl::Writer::new(&mut out, naga::back::wgsl::WriterFlags::EXPLICIT_TYPES);

    writer.write(module, &module_info).expect("Writing failed");
    out
}
