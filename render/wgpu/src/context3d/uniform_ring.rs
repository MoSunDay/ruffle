//! Per-draw snapshots of the Stage3D program constants.
//!
//! Stage3D has a single mutable uniform state, but wgpu renders command buffers
//! asynchronously. We copy the current constant buffers into a ring of aligned
//! slots for each draw, then bind the slot with a dynamic uniform offset.

use std::cell::Cell;

use wgpu::{Buffer, BufferDescriptor, BufferUsages, CommandEncoder, Device};

pub(super) const VERTEX_SHADER_UNIFORMS_BUFFER_SIZE: u64 = 250 * 4 * 4;
pub(super) const FRAGMENT_SHADER_UNIFORMS_BUFFER_SIZE: u64 = 28 * 4 * 4;

/// `min_uniform_buffer_offset_alignment` is at most 256 on WebGL2 and on all
/// native backends targeting WebGPU-compatible limits.
const UNIFORM_OFFSET_ALIGNMENT: u64 = 256;
const VERTEX_SLOT_STRIDE: u64 = align_up(VERTEX_SHADER_UNIFORMS_BUFFER_SIZE);
const FRAGMENT_SLOT_STRIDE: u64 = align_up(FRAGMENT_SHADER_UNIFORMS_BUFFER_SIZE);

/// Upper bound on draws per submission; typical frames use far fewer. The
/// ring is reset after each submitted frame, and extra draws flush early.
const MAX_UNIFORM_SLOTS: u32 = 512;

const fn align_up(size: u64) -> u64 {
    (size + UNIFORM_OFFSET_ALIGNMENT - 1) & !(UNIFORM_OFFSET_ALIGNMENT - 1)
}

pub(super) struct UniformRing {
    vertex: Buffer,
    fragment: Buffer,
    next_slot: Cell<u32>,
}

impl UniformRing {
    pub fn new(device: &Device) -> Self {
        let vertex = device.create_buffer(&BufferDescriptor {
            label: Some("Stage3D vertex uniform ring"),
            size: VERTEX_SLOT_STRIDE * MAX_UNIFORM_SLOTS as u64,
            usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let fragment = device.create_buffer(&BufferDescriptor {
            label: Some("Stage3D fragment uniform ring"),
            size: FRAGMENT_SLOT_STRIDE * MAX_UNIFORM_SLOTS as u64,
            usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Self {
            vertex,
            fragment,
            next_slot: Cell::new(0),
        }
    }

    pub fn buffers(&self) -> (&Buffer, &Buffer) {
        (&self.vertex, &self.fragment)
    }

    /// Copies the current program constants into the next pair of slots.
    /// Returns `None` when the ring is full and the caller should submit first.
    pub fn prepare(
        &self,
        encoder: &mut CommandEncoder,
        vertex_source: &Buffer,
        fragment_source: &Buffer,
    ) -> Option<(u32, u32)> {
        let slot = self.next_slot.get();
        if slot >= MAX_UNIFORM_SLOTS {
            return None;
        }
        self.next_slot.set(slot + 1);
        let vertex_offset = slot as u64 * VERTEX_SLOT_STRIDE;
        let fragment_offset = slot as u64 * FRAGMENT_SLOT_STRIDE;
        encoder.copy_buffer_to_buffer(
            vertex_source,
            0,
            &self.vertex,
            vertex_offset,
            VERTEX_SHADER_UNIFORMS_BUFFER_SIZE,
        );
        encoder.copy_buffer_to_buffer(
            fragment_source,
            0,
            &self.fragment,
            fragment_offset,
            FRAGMENT_SHADER_UNIFORMS_BUFFER_SIZE,
        );
        Some((vertex_offset as u32, fragment_offset as u32))
    }

    pub fn reset(&self) {
        self.next_slot.set(0);
    }
}
