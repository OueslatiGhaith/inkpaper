use core::marker::PhantomData;

use crate::{Runtime, RuntimeResources};

#[doc(hidden)]
pub struct Unset;

#[doc(hidden)]
pub struct EntityCapacity<const BYTES: usize, const SLOTS: usize>;

#[doc(hidden)]
pub struct CallbackCapacity<const BYTES: usize, const SLOTS: usize>;

#[doc(hidden)]
pub struct FrameCapacity<const NODES: usize, const TEXT_BYTES: usize>;

#[doc(hidden)]
pub struct ElementStateCapacity<const SLOTS: usize>;

#[doc(hidden)]
pub struct GlobalCapacity<const BYTES: usize, const SLOTS: usize>;

#[doc(hidden)]
pub struct ResourceSet<R>(PhantomData<fn() -> R>);

pub struct RuntimeBuilder<
    ENTITIES = Unset,
    CALLBACKS = Unset,
    FRAME = Unset,
    ELEMENT_STATES = Unset,
    GLOBALS = GlobalCapacity<0, 0>,
    RESOURCES = ResourceSet<()>,
> {
    marker: PhantomData<
        fn() -> (
            ENTITIES,
            CALLBACKS,
            FRAME,
            ELEMENT_STATES,
            GLOBALS,
            RESOURCES,
        ),
    >,
}

impl Default for RuntimeBuilder {
    fn default() -> Self {
        Self {
            marker: PhantomData,
        }
    }
}

impl<CALLBACKS, FRAME, ELEMENT_STATES, GLOBALS, RESOURCES>
    RuntimeBuilder<Unset, CALLBACKS, FRAME, ELEMENT_STATES, GLOBALS, RESOURCES>
{
    pub const fn entities<const BYTES: usize, const SLOTS: usize>(
        self,
    ) -> RuntimeBuilder<
        EntityCapacity<BYTES, SLOTS>,
        CALLBACKS,
        FRAME,
        ELEMENT_STATES,
        GLOBALS,
        RESOURCES,
    > {
        RuntimeBuilder {
            marker: PhantomData,
        }
    }
}

impl<ENTITIES, FRAME, ELEMENT_STATES, GLOBALS, RESOURCES>
    RuntimeBuilder<ENTITIES, Unset, FRAME, ELEMENT_STATES, GLOBALS, RESOURCES>
{
    pub const fn callbacks<const BYTES: usize, const SLOTS: usize>(
        self,
    ) -> RuntimeBuilder<
        ENTITIES,
        CallbackCapacity<BYTES, SLOTS>,
        FRAME,
        ELEMENT_STATES,
        GLOBALS,
        RESOURCES,
    > {
        RuntimeBuilder {
            marker: PhantomData,
        }
    }
}

impl<ENTITIES, CALLBACKS, ELEMENT_STATES, GLOBALS, RESOURCES>
    RuntimeBuilder<ENTITIES, CALLBACKS, Unset, ELEMENT_STATES, GLOBALS, RESOURCES>
{
    pub const fn frame<const NODES: usize, const TEXT_BYTES: usize>(
        self,
    ) -> RuntimeBuilder<
        ENTITIES,
        CALLBACKS,
        FrameCapacity<NODES, TEXT_BYTES>,
        ELEMENT_STATES,
        GLOBALS,
        RESOURCES,
    > {
        RuntimeBuilder {
            marker: PhantomData,
        }
    }
}

impl<ENTITIES, CALLBACKS, FRAME, GLOBALS, RESOURCES>
    RuntimeBuilder<ENTITIES, CALLBACKS, FRAME, Unset, GLOBALS, RESOURCES>
{
    pub const fn element_states<const SLOTS: usize>(
        self,
    ) -> RuntimeBuilder<ENTITIES, CALLBACKS, FRAME, ElementStateCapacity<SLOTS>, GLOBALS, RESOURCES>
    {
        RuntimeBuilder {
            marker: PhantomData,
        }
    }
}

impl<ENTITIES, CALLBACKS, FRAME, ELEMENT_STATES, GLOBALS, RESOURCES>
    RuntimeBuilder<ENTITIES, CALLBACKS, FRAME, ELEMENT_STATES, GLOBALS, RESOURCES>
{
    pub const fn globals<const BYTES: usize, const SLOTS: usize>(
        self,
    ) -> RuntimeBuilder<
        ENTITIES,
        CALLBACKS,
        FRAME,
        ELEMENT_STATES,
        GlobalCapacity<BYTES, SLOTS>,
        RESOURCES,
    > {
        RuntimeBuilder {
            marker: PhantomData,
        }
    }

    pub const fn resources<R>(
        self,
    ) -> RuntimeBuilder<ENTITIES, CALLBACKS, FRAME, ELEMENT_STATES, GLOBALS, ResourceSet<R>> {
        RuntimeBuilder {
            marker: PhantomData,
        }
    }

    pub const fn render_resources<
        'resource,
        const FONTS: usize,
        const GLYPH_SLOTS: usize,
        const GLYPH_BYTES: usize,
        const IMAGES: usize,
    >(
        self,
    ) -> RuntimeBuilder<
        ENTITIES,
        CALLBACKS,
        FRAME,
        ELEMENT_STATES,
        GLOBALS,
        ResourceSet<RuntimeResources<'resource, FONTS, GLYPH_SLOTS, GLYPH_BYTES, IMAGES>>,
    > {
        RuntimeBuilder {
            marker: PhantomData,
        }
    }
}

impl<
    const ENTITY_BYTES: usize,
    const ENTITY_SLOTS: usize,
    const CALLBACK_BYTES: usize,
    const CALLBACK_SLOTS: usize,
    const FRAME_NODES: usize,
    const FRAME_TEXT_BYTES: usize,
    const ELEMENT_STATES: usize,
    const GLOBAL_BYTES: usize,
    const GLOBAL_SLOTS: usize,
    RESOURCES,
>
    RuntimeBuilder<
        EntityCapacity<ENTITY_BYTES, ENTITY_SLOTS>,
        CallbackCapacity<CALLBACK_BYTES, CALLBACK_SLOTS>,
        FrameCapacity<FRAME_NODES, FRAME_TEXT_BYTES>,
        ElementStateCapacity<ELEMENT_STATES>,
        GlobalCapacity<GLOBAL_BYTES, GLOBAL_SLOTS>,
        ResourceSet<RESOURCES>,
    >
where
    RESOURCES: Default,
{
    pub fn build(
        self,
    ) -> Runtime<
        ENTITY_BYTES,
        ENTITY_SLOTS,
        CALLBACK_BYTES,
        CALLBACK_SLOTS,
        FRAME_NODES,
        FRAME_TEXT_BYTES,
        ELEMENT_STATES,
        GLOBAL_BYTES,
        GLOBAL_SLOTS,
        RESOURCES,
    > {
        Runtime::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builder_constructs_runtime_with_default_globals_and_resources() {
        let runtime = RuntimeBuilder::default()
            .entities::<256, 4>()
            .callbacks::<256, 4>()
            .frame::<16, 256>()
            .element_states::<8>()
            .build();

        assert_eq!(runtime.frame_node_count(), 0);
        assert_eq!(runtime.global_capacity(), 0);
        assert_eq!(runtime.global_byte_capacity(), 0);
    }

    #[test]
    fn builder_configures_global_capacities() {
        let runtime = RuntimeBuilder::default()
            .entities::<256, 4>()
            .callbacks::<256, 4>()
            .frame::<16, 256>()
            .element_states::<8>()
            .globals::<128, 3>()
            .build();

        assert_eq!(runtime.global_capacity(), 3);
        assert_eq!(runtime.global_byte_capacity(), 128);
    }

    #[test]
    fn builder_configures_backend_independent_render_resources() {
        let runtime = RuntimeBuilder::default()
            .entities::<256, 4>()
            .callbacks::<256, 4>()
            .frame::<16, 256>()
            .element_states::<8>()
            .render_resources::<2, 8, 512, 3>()
            .build();

        assert_eq!(runtime.glyph_cache_capacity_bytes(), 512,);
        assert_eq!(runtime.glyph_cache_used_bytes(), 0,);
    }
}
