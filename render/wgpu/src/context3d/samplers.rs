use naga_agal::{Filter, Mipmap, SamplerConfig, Wrapping};

use crate::bitmaps::WgpuSamplerConfig;
use crate::descriptors::Descriptors;

/// Maps Stage3D sampler configurations to the shared wgpu samplers.
pub(super) fn wgpu_samplers<'a>(
    descriptors: &'a Descriptors,
    sampler_configs: &[SamplerConfig; 8],
) -> [&'a wgpu::Sampler; 8] {
    sampler_configs.map(|sampler| {
        let (filter, anisotropy_clamp) = match sampler.filter {
            Filter::Nearest => (0, 1),
            Filter::Linear => (1, 1),
            Filter::Anisotropic2x => (2, 2),
            Filter::Anisotropic4x => (3, 4),
            Filter::Anisotropic8x => (4, 8),
            Filter::Anisotropic16x => (5, 16),
        };
        let mipmap = match sampler.mipmap {
            Mipmap::Disable => 0,
            Mipmap::Nearest => 1,
            Mipmap::Linear => 2,
        };
        let (address_mode_u, address_mode_v) = match sampler.wrapping {
            Wrapping::Clamp => (
                wgpu::AddressMode::ClampToEdge,
                wgpu::AddressMode::ClampToEdge,
            ),
            Wrapping::Repeat => (wgpu::AddressMode::Repeat, wgpu::AddressMode::Repeat),
            Wrapping::ClampURepeatV => (wgpu::AddressMode::ClampToEdge, wgpu::AddressMode::Repeat),
            Wrapping::RepeatUClampV => (wgpu::AddressMode::Repeat, wgpu::AddressMode::ClampToEdge),
        };
        &descriptors.bitmap_samplers.stage3d[&WgpuSamplerConfig {
            anisotropy_clamp,
            address_mode_u,
            address_mode_v,
            filter,
            mipmap,
        }]
    })
}
