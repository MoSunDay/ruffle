use std::collections::HashMap;

use crate::avm2::activation::Activation;
use crate::avm2::object::script_object::ScriptObjectData;
use crate::avm2::object::{Object, TObject};
use crate::avm2::value::Value;
use crate::display_object::{DisplayObject, TDisplayObject};
use crate::string::AvmString;
use core::fmt;
use gc_arena::barrier::unlock;
use gc_arena::{Collect, Gc, GcWeak, Mutation, lock::RefLock};
use ruffle_common::utils::HasPrefixField;

/// Worker lifecycle states (subset of Flash semantics; see Worker.state).
#[derive(Clone, Copy, Collect, Debug, PartialEq, Eq)]
#[collect(no_drop)]
pub enum WorkerState {
    New,
    Running,
    Terminated,
}

impl WorkerState {
    pub fn as_str(self) -> &'static str {
        match self {
            WorkerState::New => "new",
            WorkerState::Running => "running",
            WorkerState::Terminated => "terminated",
        }
    }
}

#[derive(Clone, Collect, Copy)]
#[collect(no_drop)]
pub struct WorkerObject<'gc>(pub Gc<'gc, WorkerObjectData<'gc>>);

#[derive(Clone, Collect, Copy, Debug)]
#[collect(no_drop)]
pub struct WorkerObjectWeak<'gc>(pub GcWeak<'gc, WorkerObjectData<'gc>>);

impl fmt::Debug for WorkerObject<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WorkerObject")
            .field("ptr", &Gc::as_ptr(self.0))
            .finish()
    }
}

#[derive(Collect, HasPrefixField)]
#[collect(no_drop)]
#[repr(C, align(8))]
pub struct WorkerObjectData<'gc> {
    /// Base script object
    base: ScriptObjectData<'gc>,

    /// Lifecycle state, starts as `New`.
    state: RefLock<WorkerState>,

    /// Properties shared through `setSharedProperty`. Same-arena emulation
    /// (route A): values are shared by reference, not copied across threads.
    shared_properties: RefLock<HashMap<AvmString<'gc>, Value<'gc>>>,

    /// SWF bytes handed to `WorkerDomain.createWorker`. Kept for the future
    /// in-arena worker execution (route A slice 2).
    swf_bytes: RefLock<Vec<u8>>,

    /// Root movie clip of the in-arena worker (route A slice 2). Kept alive
    /// while the worker is running so the orphan/timeline pump can reach it.
    root: RefLock<Option<DisplayObject<'gc>>>,
}

impl<'gc> TObject<'gc> for WorkerObject<'gc> {
    fn gc_base(&self) -> Gc<'gc, ScriptObjectData<'gc>> {
        HasPrefixField::as_prefix_gc(self.0)
    }
}

impl<'gc> WorkerObject<'gc> {
    pub fn new(activation: &mut Activation<'_, 'gc>) -> Self {
        let class = activation.avm2().classes().worker;
        let base = ScriptObjectData::new(class);
        WorkerObject(Gc::new(
            activation.gc(),
            WorkerObjectData {
                base,
                state: RefLock::new(WorkerState::New),
                shared_properties: RefLock::new(HashMap::new()),
                swf_bytes: RefLock::new(Vec::new()),
                root: RefLock::new(None),
            },
        ))
    }

    pub fn state(&self) -> WorkerState {
        *self.0.state.borrow()
    }

    pub fn set_state(&self, mc: &Mutation<'gc>, state: WorkerState) {
        *unlock!(Gc::write(mc, self.0), WorkerObjectData, state).borrow_mut() = state;
    }

    pub fn set_shared_property(&self, mc: &Mutation<'gc>, key: AvmString<'gc>, value: Value<'gc>) {
        unlock!(Gc::write(mc, self.0), WorkerObjectData, shared_properties)
            .borrow_mut()
            .insert(key, value);
    }

    pub fn get_shared_property(&self, key: AvmString<'gc>) -> Option<Value<'gc>> {
        self.0.shared_properties.borrow().get(&key).cloned()
    }

    pub fn set_swf_bytes(&self, mc: &Mutation<'gc>, bytes: Vec<u8>) {
        *unlock!(Gc::write(mc, self.0), WorkerObjectData, swf_bytes).borrow_mut() = bytes;
    }

    pub fn swf_bytes(&self) -> Vec<u8> {
        self.0.swf_bytes.borrow().clone()
    }

    pub fn root(&self) -> Option<DisplayObject<'gc>> {
        *self.0.root.borrow()
    }

    /// Whether `object` is this worker's root AS3 object.
    pub fn owns_root(&self, object: Object<'gc>) -> bool {
        self.root()
            .and_then(|root| root.object2())
            .is_some_and(|root| Object::ptr_eq(root.into(), object))
    }

    pub fn set_root(&self, mc: &Mutation<'gc>, root: Option<DisplayObject<'gc>>) {
        *unlock!(Gc::write(mc, self.0), WorkerObjectData, root).borrow_mut() = root;
    }

    pub fn ptr_eq(a: WorkerObject<'gc>, b: WorkerObject<'gc>) -> bool {
        Gc::ptr_eq(a.0, b.0)
    }
}
