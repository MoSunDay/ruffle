//! Accumulates Stage3D uploads and draw passes into two command encoders.
//!
//! The original implementation submitted two command buffers for every
//! `drawTriangles` call. Stage3D scenes routinely issue hundreds of draws per
//! frame, so that turned into thousands of queue submissions per second.
//! Batching them into one upload and one render submission per frame keeps all
//! of the command encoding while removing the per-draw submission overhead.

use wgpu::util::StagingBelt;
use wgpu::{CommandEncoder, CommandEncoderDescriptor, Device, Queue};

pub(super) struct FrameBatch {
    device: Device,
    upload: Option<CommandEncoder>,
    render: Option<CommandEncoder>,
    has_commands: bool,
}

impl FrameBatch {
    pub fn new(device: &Device) -> Self {
        Self {
            device: device.clone(),
            upload: Some(Self::make_encoder(device, "Buffer command encoder")),
            render: Some(Self::make_encoder(device, "Render command encoder")),
            has_commands: false,
        }
    }

    fn make_encoder(device: &Device, label: &str) -> CommandEncoder {
        device.create_command_encoder(&CommandEncoderDescriptor { label: Some(label) })
    }

    pub fn upload(&mut self) -> &mut CommandEncoder {
        self.has_commands = true;
        self.upload.as_mut().expect("upload encoder is available")
    }

    /// Temporarily moves the render encoder out so that `WgpuContext3D` can
    /// borrow its own fields immutably while recording a render pass.
    pub fn take_render(&mut self) -> CommandEncoder {
        self.has_commands = true;
        self.render.take().expect("render encoder is available")
    }

    pub fn put_render(&mut self, encoder: CommandEncoder) {
        self.render = Some(encoder);
    }

    pub fn flush(&mut self, queue: &Queue, staging_belt: &mut StagingBelt) {
        if !self.has_commands {
            return;
        }
        staging_belt.finish();
        let upload = self
            .upload
            .take()
            .expect("upload encoder is available")
            .finish();
        let render = self
            .render
            .take()
            .expect("render encoder is available")
            .finish();
        queue.submit([upload, render]);
        staging_belt.recall();
        self.upload = Some(Self::make_encoder(&self.device, "Buffer command encoder"));
        self.render = Some(Self::make_encoder(&self.device, "Render command encoder"));
        self.has_commands = false;
    }
}
