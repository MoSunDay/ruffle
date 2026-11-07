use naga_agal::{Filter, SamplerConfig, Wrapping};

use crate::bitmaps::WgpuSamplerConfig;
use crate::descriptors::Descriptors;

/// Maps Stage3D sampler configurations to the shared wgpu samplers.
pub(super) fn wgpu_samplers<'a>(
    descriptors: &'a Descriptors,
    sampler_configs: &[SamplerConfig; 8],
) -> [&'a wgpu::Sampler; 8] {
    sampler_configs.map(|sampler| match (sampler.wrapping, sampler.filter) {
        (Wrapping::Clamp, Filter::Linear) => &descriptors.bitmap_samplers.clamp_linear,
        (Wrapping::Clamp, Filter::Nearest) => &descriptors.bitmap_samplers.clamp_nearest,
        (Wrapping::Repeat, Filter::Linear) => &descriptors.bitmap_samplers.repeat_linear,
        (Wrapping::Repeat, Filter::Nearest) => &descriptors.bitmap_samplers.repeat_nearest,
        (Wrapping::ClampURepeatV, Filter::Linear) => {
            &descriptors.bitmap_samplers.clamp_u_repeat_v_linear
        }
        (Wrapping::ClampURepeatV, Filter::Nearest) => {
            &descriptors.bitmap_samplers.clamp_u_repeat_v_nearest
        }
        (Wrapping::RepeatUClampV, Filter::Linear) => {
            &descriptors.bitmap_samplers.repeat_u_clamp_v_linear
        }
        (Wrapping::RepeatUClampV, Filter::Nearest) => {
            &descriptors.bitmap_samplers.repeat_u_clamp_v_nearest
        }
        (
            wrapping,
            Filter::Anisotropic2x
            | Filter::Anisotropic4x
            | Filter::Anisotropic8x
            | Filter::Anisotropic16x,
        ) => {
            &descriptors.bitmap_samplers.anisotropic[&WgpuSamplerConfig {
                anisotropy_clamp: match sampler.filter {
                    Filter::Anisotropic2x => 2,
                    Filter::Anisotropic4x => 4,
                    Filter::Anisotropic8x => 8,
                    Filter::Anisotropic16x => 16,
                    _ => unreachable!(),
                },
                address_mode_u: match wrapping {
                    Wrapping::Clamp => wgpu::AddressMode::ClampToEdge,
                    Wrapping::Repeat => wgpu::AddressMode::Repeat,
                    Wrapping::ClampURepeatV => wgpu::AddressMode::ClampToEdge,
                    Wrapping::RepeatUClampV => wgpu::AddressMode::Repeat,
                },
                address_mode_v: match wrapping {
                    Wrapping::Clamp => wgpu::AddressMode::ClampToEdge,
                    Wrapping::Repeat => wgpu::AddressMode::Repeat,
                    Wrapping::ClampURepeatV => wgpu::AddressMode::Repeat,
                    Wrapping::RepeatUClampV => wgpu::AddressMode::ClampToEdge,
                },
            }]
        }
    })
}
