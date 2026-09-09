use crate::avm2::activation::Activation;
use crate::avm2::object::TObject;
use crate::avm2::object::script_object::ScriptObjectData;
use crate::worker::MessageChannelHandle;
use core::cell::Cell;
use core::fmt;
use gc_arena::{Collect, Gc, GcWeak, Mutation};
use llflash_common::utils::HasPrefixField;
use std::sync::Arc;

#[derive(Clone, Collect, Copy)]
#[collect(no_drop)]
pub struct MessageChannelObject<'gc>(pub Gc<'gc, MessageChannelObjectData<'gc>>);

#[derive(Clone, Collect, Copy, Debug)]
#[collect(no_drop)]
pub struct MessageChannelObjectWeak<'gc>(pub GcWeak<'gc, MessageChannelObjectData<'gc>>);

impl fmt::Debug for MessageChannelObject<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MessageChannelObject")
            .field("ptr", &Gc::as_ptr(self.0))
            .field("sender", &self.handle().sender())
            .field("receiver", &self.handle().receiver())
            .finish()
    }
}

#[derive(Collect, HasPrefixField)]
#[collect(no_drop)]
#[repr(C, align(8))]
pub struct MessageChannelObjectData<'gc> {
    base: ScriptObjectData<'gc>,
    #[collect(require_static)]
    handle: Arc<MessageChannelHandle>,
    #[collect(require_static)]
    last_observed_sequence: Cell<u64>,
}

impl<'gc> TObject<'gc> for MessageChannelObject<'gc> {
    fn gc_base(&self) -> Gc<'gc, ScriptObjectData<'gc>> {
        HasPrefixField::as_prefix_gc(self.0)
    }
}

impl<'gc> MessageChannelObject<'gc> {
    pub fn new(activation: &mut Activation<'_, 'gc>, handle: Arc<MessageChannelHandle>) -> Self {
        let class = activation.avm2().classes().messagechannel;
        let base = ScriptObjectData::new(class);
        let sequence = handle.sequence();
        let object = MessageChannelObject(Gc::new(
            activation.gc(),
            MessageChannelObjectData {
                base,
                handle,
                last_observed_sequence: Cell::new(sequence),
            },
        ));
        activation
            .context
            .worker_message_channels
            .push(MessageChannelObjectWeak(Gc::downgrade(object.0)));
        object
    }

    pub fn handle(self) -> Arc<MessageChannelHandle> {
        self.0.handle.clone()
    }

    pub fn pending_event_count(self, current_worker: u64) -> u64 {
        if self.0.handle.receiver() != current_worker {
            return 0;
        }

        let sequence = self.0.handle.sequence();
        let previous = self.0.last_observed_sequence.replace(sequence);
        sequence.saturating_sub(previous)
    }
}

impl<'gc> MessageChannelObjectWeak<'gc> {
    pub fn upgrade(self, mc: &Mutation<'gc>) -> Option<MessageChannelObject<'gc>> {
        self.0.upgrade(mc).map(MessageChannelObject)
    }
}
