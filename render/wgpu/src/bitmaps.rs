use std::collections::HashMap;

#[derive(Debug)]
pub struct BitmapSamplers {
    pub repeat_linear: wgpu::Sampler,
    pub repeat_nearest: wgpu::Sampler,
    pub clamp_linear: wgpu::Sampler,
    pub clamp_nearest: wgpu::Sampler,
    pub clamp_u_repeat_v_linear: wgpu::Sampler,
    pub clamp_u_repeat_v_nearest: wgpu::Sampler,
    pub repeat_u_clamp_v_linear: wgpu::Sampler,
    pub repeat_u_clamp_v_nearest: wgpu::Sampler,
    /// Stage3D sampler combinations (wrapping, filter, and mip filter).
    pub stage3d: HashMap<WgpuSamplerConfig, wgpu::Sampler>,
}

#[derive(Debug, Hash, PartialEq, Eq)]
pub struct WgpuSamplerConfig {
    pub anisotropy_clamp: u8,
    pub address_mode_u: wgpu::AddressMode,
    pub address_mode_v: wgpu::AddressMode,
    /// 0 = nearest, 1 = linear, 2..=5 = anisotropic 2x/4x/8x/16x.
    pub filter: u8,
    /// 0 = disabled, 1 = nearest, 2 = linear.
    pub mipmap: u8,
}

fn create_sampler(
    device: &wgpu::Device,
    address_mode_u: wgpu::AddressMode,
    address_mode_v: wgpu::AddressMode,
    filter: wgpu::FilterMode,
    sampler_label: Option<String>,
) -> wgpu::Sampler {
    device.create_sampler(&wgpu::SamplerDescriptor {
        label: sampler_label.as_deref(),
        address_mode_u,
        address_mode_v,
        // FIXME - does this ever get used?
        address_mode_w: wgpu::AddressMode::Repeat,
        mag_filter: filter,
        min_filter: filter,
        mipmap_filter: match filter {
            wgpu::FilterMode::Linear => wgpu::MipmapFilterMode::Linear,
            wgpu::FilterMode::Nearest => wgpu::MipmapFilterMode::Nearest,
        },
        lod_min_clamp: 0.0,
        lod_max_clamp: 100.0,
        compare: None,
        anisotropy_clamp: 1,
        border_color: None,
    })
}

fn create_sampler_inner(
    device: &wgpu::Device,
    address_mode_u: wgpu::AddressMode,
    address_mode_v: wgpu::AddressMode,
    filter: wgpu::FilterMode,
    mipmap_filter: wgpu::MipmapFilterMode,
    lod_max_clamp: f32,
    anisotropy_clamp: u16,
    sampler_label: Option<String>,
) -> wgpu::Sampler {
    device.create_sampler(&wgpu::SamplerDescriptor {
        label: sampler_label.as_deref(),
        address_mode_u,
        address_mode_v,
        address_mode_w: wgpu::AddressMode::Repeat,
        mag_filter: filter,
        min_filter: filter,
        mipmap_filter,
        lod_min_clamp: 0.0,
        lod_max_clamp,
        compare: None,
        anisotropy_clamp,
        border_color: None,
    })
}

impl BitmapSamplers {
    pub fn new(device: &wgpu::Device) -> Self {
        let repeat_linear = create_sampler(
            device,
            wgpu::AddressMode::Repeat,
            wgpu::AddressMode::Repeat,
            wgpu::FilterMode::Linear,
            create_debug_label!("Repeat & Linear sampler"),
        );
        let repeat_nearest = create_sampler(
            device,
            wgpu::AddressMode::Repeat,
            wgpu::AddressMode::Repeat,
            wgpu::FilterMode::Nearest,
            create_debug_label!("Repeat & Nearest sampler"),
        );
        let clamp_linear = create_sampler(
            device,
            wgpu::AddressMode::ClampToEdge,
            wgpu::AddressMode::ClampToEdge,
            wgpu::FilterMode::Linear,
            create_debug_label!("Clamp & Linear sampler"),
        );
        let clamp_nearest = create_sampler(
            device,
            wgpu::AddressMode::ClampToEdge,
            wgpu::AddressMode::ClampToEdge,
            wgpu::FilterMode::Nearest,
            create_debug_label!("Clamp & Nearest sampler"),
        );

        let clamp_u_repeat_v_linear = create_sampler(
            device,
            wgpu::AddressMode::ClampToEdge,
            wgpu::AddressMode::Repeat,
            wgpu::FilterMode::Linear,
            create_debug_label!("Clamp U, Repeat V & Linear sampler"),
        );

        let clamp_u_repeat_v_nearest = create_sampler(
            device,
            wgpu::AddressMode::ClampToEdge,
            wgpu::AddressMode::Repeat,
            wgpu::FilterMode::Nearest,
            create_debug_label!("Clamp U, Repeat V & Nearest sampler"),
        );

        let repeat_u_clamp_v_linear = create_sampler(
            device,
            wgpu::AddressMode::Repeat,
            wgpu::AddressMode::ClampToEdge,
            wgpu::FilterMode::Linear,
            create_debug_label!("Repeat U, Clamp V & Linear sampler"),
        );

        let repeat_u_clamp_v_nearest = create_sampler(
            device,
            wgpu::AddressMode::Repeat,
            wgpu::AddressMode::ClampToEdge,
            wgpu::FilterMode::Nearest,
            create_debug_label!("Repeat U, Clamp V & Nearest sampler"),
        );

        let mut stage3d = HashMap::new();
        for (filter, anisotropy) in [(0, 1), (1, 1), (2, 2), (3, 4), (4, 8), (5, 16)] {
            for mipmap in 0..=2 {
                for u_mode in [wgpu::AddressMode::ClampToEdge, wgpu::AddressMode::Repeat] {
                    for v_mode in [wgpu::AddressMode::ClampToEdge, wgpu::AddressMode::Repeat] {
                        let sampler = create_sampler_inner(
                            device,
                            u_mode,
                            v_mode,
                            if filter == 0 {
                                wgpu::FilterMode::Nearest
                            } else {
                                wgpu::FilterMode::Linear
                            },
                            // wgpu requires every filter mode to be linear when
                            // anisotropic filtering is enabled.
                            if mipmap == 1 && filter < 2 {
                                wgpu::MipmapFilterMode::Nearest
                            } else {
                                wgpu::MipmapFilterMode::Linear
                            },
                            if mipmap == 0 { 0.0 } else { 100.0 },
                            anisotropy,
                            create_debug_label!(
                                "Stage3D filter={}, mipmap={}, u={:?}, v={:?} sampler",
                                filter,
                                mipmap,
                                u_mode,
                                v_mode
                            ),
                        );
                        stage3d.insert(
                            WgpuSamplerConfig {
                                anisotropy_clamp: anisotropy as u8,
                                address_mode_u: u_mode,
                                address_mode_v: v_mode,
                                filter,
                                mipmap,
                            },
                            sampler,
                        );
                    }
                }
            }
        }

        Self {
            repeat_linear,
            repeat_nearest,
            clamp_linear,
            clamp_nearest,
            clamp_u_repeat_v_linear,
            clamp_u_repeat_v_nearest,
            repeat_u_clamp_v_linear,
            repeat_u_clamp_v_nearest,
            stage3d,
        }
    }

    pub fn get_sampler(&self, is_repeating: bool, is_smoothed: bool) -> &wgpu::Sampler {
        match (is_repeating, is_smoothed) {
            (true, true) => &self.repeat_linear,
            (true, false) => &self.repeat_nearest,
            (false, true) => &self.clamp_linear,
            (false, false) => &self.clamp_nearest,
        }
    }
}
