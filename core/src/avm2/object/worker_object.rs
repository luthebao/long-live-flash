use crate::avm2::activation::Activation;
use crate::avm2::object::TObject;
use crate::avm2::object::script_object::ScriptObjectData;
use crate::worker::{WorkerExecutionState, WorkerHandle};
use core::cell::Cell;
use core::fmt;
use gc_arena::{Collect, Gc, GcWeak, Mutation};
use llflash_common::utils::HasPrefixField;
use std::sync::Arc;

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
            .field("worker_id", &self.handle().id())
            .finish()
    }
}

#[derive(Collect, HasPrefixField)]
#[collect(no_drop)]
#[repr(C, align(8))]
pub struct WorkerObjectData<'gc> {
    base: ScriptObjectData<'gc>,
    #[collect(require_static)]
    handle: Arc<WorkerHandle>,
    #[collect(require_static)]
    last_observed_state: Cell<WorkerExecutionState>,
}

impl<'gc> TObject<'gc> for WorkerObject<'gc> {
    fn gc_base(&self) -> Gc<'gc, ScriptObjectData<'gc>> {
        HasPrefixField::as_prefix_gc(self.0)
    }
}

impl<'gc> WorkerObject<'gc> {
    pub fn new(activation: &mut Activation<'_, 'gc>, handle: Arc<WorkerHandle>) -> Self {
        let class = activation.avm2().classes().worker;
        let base = ScriptObjectData::new(class);
        let state = handle.state();
        let object = WorkerObject(Gc::new(
            activation.gc(),
            WorkerObjectData {
                base,
                handle,
                last_observed_state: Cell::new(state),
            },
        ));
        activation
            .context
            .worker_objects
            .push(WorkerObjectWeak(Gc::downgrade(object.0)));
        object
    }

    pub fn handle(self) -> Arc<WorkerHandle> {
        self.0.handle.clone()
    }

    pub fn observe_state_change(self) -> Option<WorkerExecutionState> {
        let state = self.0.handle.state();
        if state != self.0.last_observed_state.get() {
            self.0.last_observed_state.set(state);
            Some(state)
        } else {
            None
        }
    }
}

impl<'gc> WorkerObjectWeak<'gc> {
    pub fn upgrade(self, mc: &Mutation<'gc>) -> Option<WorkerObject<'gc>> {
        self.0.upgrade(mc).map(WorkerObject)
    }
}
