//! Caches for `wgpu::RenderPipeline` and `wgpu::BindGroup` objects.
//!
//! Stage3D exposes a mutable state machine, while wgpu objects are immutable.
//! The original implementation rebuilt the pipeline and bind group whenever
//! any state changed, which happens on almost every `drawTriangles` call.
//! These caches make rebuilding a cheap hash lookup when the state is
//! unchanged from a previous frame.

use std::cell::RefCell;
use std::hash::{Hash, Hasher};
use std::num::NonZeroUsize;
use std::rc::Rc;

use lru::LruCache;
use naga_agal::VertexAttributeFormat;

use super::{MAX_VERTEX_ATTRIBUTES, ShaderPairAgal};

const PIPELINE_CACHE_SIZE: usize = 128;
const BIND_GROUP_CACHE_SIZE: usize = 128;

/// Identity wrapper around a shader pair.
///
/// The cache holds an `Rc`, so the allocation cannot be reused while the
/// pointer-based hash is stored in a key.
#[derive(Clone)]
pub struct ShaderIdentity(Rc<ShaderPairAgal>);

impl ShaderIdentity {
    pub fn new(shaders: Rc<ShaderPairAgal>) -> Self {
        Self(shaders)
    }
}

impl PartialEq for ShaderIdentity {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl Eq for ShaderIdentity {}

impl Hash for ShaderIdentity {
    fn hash<H: Hasher>(&self, state: &mut H) {
        (Rc::as_ptr(&self.0) as usize).hash(state);
    }
}

/// Everything that determines a `wgpu::RenderPipeline`.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct PipelineKey {
    pub shaders: Option<ShaderIdentity>,
    pub culling: u8,
    /// For each texture slot: `None` if unbound, `Some(true)` for a cube map.
    pub texture_infos: [Option<bool>; 8],
    /// For each vertex attribute: (stride in 32-bit units, format, offset).
    pub vertex_attributes: [Option<(u8, VertexAttributeFormat, u64)>; MAX_VERTEX_ATTRIBUTES],
    pub has_depth_texture: bool,
    pub color_mask: wgpu::ColorWrites,
    pub depth_mask: bool,
    pub pass_compare_mode: wgpu::CompareFunction,
    pub stencil: wgpu::StencilState,
    pub color_component: wgpu::BlendComponent,
    pub alpha_component: wgpu::BlendComponent,
    pub sample_count: u32,
    pub target_format: wgpu::TextureFormat,
}

#[derive(Clone)]
pub struct CachedPipeline {
    pub pipeline: wgpu::RenderPipeline,
    pub bind_group_layout: wgpu::BindGroupLayout,
}

/// Everything that determines a `wgpu::BindGroup` for a pipeline.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct BindGroupKey {
    pub pipeline: PipelineKey,
    pub textures: [Option<wgpu::TextureView>; 8],
    pub cubes: [bool; 8],
    pub samplers: [wgpu::Sampler; 8],
}

pub struct PipelineCache {
    pipelines: RefCell<LruCache<PipelineKey, CachedPipeline>>,
    bind_groups: RefCell<LruCache<BindGroupKey, wgpu::BindGroup>>,
}

impl PipelineCache {
    pub fn new() -> Self {
        Self {
            pipelines: RefCell::new(LruCache::new(
                NonZeroUsize::new(PIPELINE_CACHE_SIZE).unwrap(),
            )),
            bind_groups: RefCell::new(LruCache::new(
                NonZeroUsize::new(BIND_GROUP_CACHE_SIZE).unwrap(),
            )),
        }
    }

    pub fn get_pipeline(&self, key: &PipelineKey) -> Option<CachedPipeline> {
        self.pipelines.borrow_mut().get(key).cloned()
    }

    pub fn put_pipeline(&self, key: PipelineKey, pipeline: CachedPipeline) {
        self.pipelines.borrow_mut().put(key, pipeline);
    }

    pub fn get_bind_group(&self, key: &BindGroupKey) -> Option<wgpu::BindGroup> {
        self.bind_groups.borrow_mut().get(key).cloned()
    }

    pub fn put_bind_group(&self, key: BindGroupKey, bind_group: wgpu::BindGroup) {
        self.bind_groups.borrow_mut().put(key, bind_group);
    }
}
