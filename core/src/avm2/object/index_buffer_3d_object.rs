//! Object representation for IndexBuffer3D objects

use crate::avm2::activation::Activation;
use crate::avm2::object::script_object::ScriptObjectData;
use crate::avm2::object::{Object, TObject};
use gc_arena::{Collect, Gc, GcWeak};
use ruffle_common::utils::HasPrefixField;
use ruffle_render::backend::IndexBuffer;
use std::cell::RefCell;
use std::rc::Rc;

use super::Context3DObject;

#[derive(Clone, Collect, Copy)]
#[collect(no_drop)]
pub struct IndexBuffer3DObject<'gc>(pub Gc<'gc, IndexBuffer3DObjectData<'gc>>);

#[derive(Clone, Collect, Copy, Debug)]
#[collect(no_drop)]
pub struct IndexBuffer3DObjectWeak<'gc>(pub GcWeak<'gc, IndexBuffer3DObjectData<'gc>>);

impl<'gc> IndexBuffer3DObject<'gc> {
    pub fn from_handle(
        activation: &mut Activation<'_, 'gc>,
        context3d: Context3DObject<'gc>,
        handle: Rc<dyn IndexBuffer>,
        num_indices: u32,
    ) -> Object<'gc> {
        let class = activation.avm2().classes().indexbuffer3d;
        let disposed_handle = context3d.disposed_index_buffer_handle();

        IndexBuffer3DObject(Gc::new(
            activation.gc(),
            IndexBuffer3DObjectData {
                base: ScriptObjectData::new(class),
                context3d,
                handle: RefCell::new(Some(handle)),
                disposed_handle,
                num_indices: num_indices as usize,
            },
        ))
        .into()
    }

    /// Capacity passed to `Context3D.createIndexBuffer`. `drawTriangles`
    /// with `numTriangles == -1` renders all remaining triangles in the
    /// buffer, so this must not be confused with the last uploaded count.
    pub fn count(self) -> usize {
        self.0.num_indices
    }

    pub fn handle(&self) -> Rc<dyn IndexBuffer> {
        self.0
            .handle
            .borrow()
            .as_ref()
            .cloned()
            .unwrap_or_else(|| self.0.disposed_handle.clone())
    }

    pub fn dispose(self) {
        self.0.handle.borrow_mut().take();
    }

    pub fn context3d(self) -> Context3DObject<'gc> {
        self.0.context3d
    }
}

#[derive(Collect, HasPrefixField)]
#[collect(no_drop)]
#[repr(C, align(8))]
pub struct IndexBuffer3DObjectData<'gc> {
    /// Base script object
    base: ScriptObjectData<'gc>,

    handle: RefCell<Option<Rc<dyn IndexBuffer>>>,

    #[collect(require_static)]
    disposed_handle: Rc<dyn IndexBuffer>,

    num_indices: usize,

    context3d: Context3DObject<'gc>,
}

impl<'gc> TObject<'gc> for IndexBuffer3DObject<'gc> {
    fn gc_base(&self) -> Gc<'gc, ScriptObjectData<'gc>> {
        HasPrefixField::as_prefix_gc(self.0)
    }
}

impl std::fmt::Debug for IndexBuffer3DObject<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "IndexBuffer3D")
    }
}
