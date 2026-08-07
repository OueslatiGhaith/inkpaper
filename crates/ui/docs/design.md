# Declarative Embedded UI Framework in Rust

## Architecture and API Design Specification

**Status:** Pre-implementation design document  
**Primary backend:** [`embedded-graphics`](https://github.com/embedded-graphics/embedded-graphics)  
**Primary API inspiration:** GPUI  
**Target:** `no_std` embedded systems, with allocator-free operation as a first-class requirement

---

# 1. Executive Summary

This project is a declarative UI framework for embedded Rust built on top of `embedded-graphics`.

The framework should provide a programming model that feels closer to GPUI than to traditional embedded widget libraries: stateful views render declarative element trees, layout and styling are expressed using a fluent builder API, behavior is attached using listeners, and reusable state can live in runtime-managed `Entity<T>` objects.

At the same time, the implementation must respect embedded constraints that desktop UI frameworks normally ignore:

- `no_std` must be a primary target, not an afterthought.
- Heap allocation must not be required.
- Memory usage should be bounded and predictable.
- Application state and UI state should not rely on `Rc`, `Arc`, `RefCell`, `Box<dyn Trait>`, or a general-purpose allocator.
- The framework should work with small microcontrollers as well as more capable devices.
- Rendering must eventually support slow displays such as SPI LCDs and e-ink panels efficiently.
- The backend should remain compatible with the `embedded-graphics::DrawTarget` ecosystem.

The core design is therefore a hybrid retained/declarative architecture:

```text
persistent runtime-managed state
        │
        │ Entity<T>: Render
        ▼
declarative temporary elements
        │
        │ mount/lower
        ▼
compact frame node tree
        │
        ├── layout
        ├── interaction / hit testing
        └── paint
                │
                ▼
        embedded-graphics
```

The public API should look approximately like this:

```rust
struct Counter {
    value: i32,
}

impl Counter {
    fn increment(
        &mut self,
        _: &ClickEvent,
        cx: &mut Context<Self>,
    ) {
        self.value += 1;
        cx.notify();
    }
}

impl Render for Counter {
    fn render(
        &mut self,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + '_ {
        div()
            .flex()
            .flex_col()
            .items_center()
            .gap(px(8))
            .child(self.value)
            .child(
                div()
                    .id("increment")
                    .px(px(8))
                    .py(px(4))
                    .border_1()
                    .on_click(cx.listener(Self::increment))
                    .child("+")
            )
    }
}
```

The framework should deliberately expose **mechanisms**, not a design system. A `Button`, `Card`, `Toggle`, application theme, primary/secondary colors, etc. should live in application code or higher-level crates. The core framework provides generic elements, layout, style primitives, interaction, state, and rendering infrastructure.

---

# 2. Design Goals

## 2.1 Primary goals

### Declarative UI

UI should be described as a tree built from ordinary Rust expressions:

```rust
div()
    .flex()
    .flex_col()
    .child("Settings")
    .child(...)
```

There should be no imperative sequence such as:

```rust
ui.begin_column();
ui.add_text(...);
ui.add_button(...);
ui.end_column();
```

and no user-managed screen coordinate calculations for ordinary layout.

### GPUI-like API ergonomics

Where concepts map well to embedded systems, the framework should stay close to GPUI naming and fluent API design:

```rust
.flex()
.flex_col()
.w_full()
.h_full()
.size_full()
.gap(px(8))
.p(px(8))
.px(px(8))
.py(px(4))
.items_center()
.justify_between()
.relative()
.absolute()
.top(px(0))
.overflow_hidden()
.id("foo")
.child(...)
.children(...)
.when(...)
```

This familiarity is valuable and GPUI has already made many good API design decisions.

### `no_std` first

The core framework should work without `std`.

Where possible, the minimal configuration should also work without `alloc`.

### Deterministic memory

Users should be able to know or bound how much memory the UI runtime can consume.

Rather than silently allocating more heap memory, fixed-capacity storage should report explicit capacity errors or fail in a clearly defined way.

### Normal Rust ownership where possible

The framework should use Rust's type system rather than hiding everything behind trait objects.

High-level elements should remain generic until they are lowered into a compact runtime representation.

### Runtime-managed persistent state

Stateful views should use typed runtime handles:

```rust
Entity<T>
```

rather than forcing all nested state to be physically owned by its parent.

### Generic rendering backend

The framework renders through `embedded-graphics`, preferably remaining generic over `DrawTarget` rather than forcing a specific framebuffer representation.

### Device-independent input

The event model should support:

- touch
- mouse-like pointers
- rotary encoders
- D-pads
- physical buttons
- keyboards where available

without application components being tied directly to a particular hardware driver.

### Efficient future rendering

The first implementation may redraw the full screen, but the architecture must leave room for:

- damage tracking
- partial repaint
- clipping
- scroll regions
- e-ink partial refresh
- slow SPI display optimization

---

# 3. Explicit Non-Goals

The following should **not** be responsibilities of the core framework.

## 3.1 No built-in design system

The framework should not know about:

```text
primary
secondary
danger
surface
ButtonVariant::Primary
dark mode
material design
application themes
```

It may provide generic properties such as:

```text
background color
foreground/text color
border
font/text style
spacing
layout
```

but semantic design-system concepts belong elsewhere.

A user can build:

```rust
fn primary_button(content: impl IntoElement) -> impl IntoElement {
    div()
        .px(px(8))
        .py(px(4))
        .bg(APP_PRIMARY)
        .child(content)
}
```

or a separate higher-level widget library can provide one.

## 3.2 No fixed widget taxonomy

The framework should not require a unique fundamental element type for every use case such as:

```text
Row
Column
Container
Stack
ScrollView
Button
Card
```

Instead, `Div` should be the universal box/layout/interactivity primitive.

Higher-level abstractions can be built from `div()`.

## 3.3 No CSS implementation

The fluent API may borrow naming from flexbox/Tailwind/GPUI, but the framework is not a browser.

We do not initially need:

- CSS cascade
- selectors
- style sheets
- `em` / `rem`
- viewport units
- CSS grid
- `calc()`
- full flexbox edge cases
- browser intrinsic layout semantics

## 3.4 No React hooks runtime

The initial state model should not introduce:

```rust
use_state(...)
use_effect(...)
```

Persistent state lives in normal Rust structs managed as entities.

## 3.5 No mandatory VDOM diff

The framework does not need React's DOM-style reconciliation architecture.

`embedded-graphics` does not expose a mutable DOM. Initially, it is acceptable to rebuild the declarative tree and repaint the screen.

Persistent IDs and entities should later enable more efficient dirty-region tracking without requiring a traditional VDOM.

---

# 4. Core Mental Model

There are four important kinds of things in the system.

## 4.1 Entities

Entities hold persistent application or view state.

```rust
Entity<T>
```

Examples:

```rust
Entity<App>
Entity<Settings>
Entity<Counter>
Entity<NetworkModel>
```

An entity is persistent across renders.

## 4.2 Declarative elements

Elements are temporary descriptions returned while rendering.

Examples:

```text
Div
Text
Image
EntityElement<T>
custom element recipes
```

They exist only long enough to be lowered into the current frame representation.

## 4.3 Runtime frame nodes

After mounting/lowering, high-level generic element types disappear and become compact runtime nodes.

Nodes are used for:

- layout
- clipping
- painting
- hit testing
- traversal
- z-order

They exist only for the current rendered scene.

## 4.4 Persistent element interaction state

Some visual elements need state across frames:

- pressed/active state
- focus
- scroll offset
- drag state
- previous bounds
- potentially animation state

This state is separate from both `Entity<T>` and temporary frame nodes.

It is identified by a persistent element identity derived from `.id(...)`.

---

# 5. Core Public API Vocabulary

The initial public abstraction set should be small:

```text
Render
Context<T>

Entity<T>
EntityId

IntoElement
Element

Div
Text
Image

Children
ParentElement

Styled
StyledExt

InteractiveElement
InteractiveElementExt

Stateful<E>
StatefulInteractiveElement
StatefulInteractiveElementExt

Fluent

ElementId
IntoElementId

Listener<E>

input/event types
```

Important runtime-only types:

```text
NodeId
ElementStateId
MountCx
FrameArena
EntityArena
ListenerArena
```

---

# 6. `Div` as the Central UI Primitive

## 6.1 Why `Div`

Instead of defining fundamental elements for every layout use case, the framework exposes one generic compositional box:

```rust
div()
```

A horizontal row is:

```rust
div()
    .flex()
    .flex_row()
```

A vertical column is:

```rust
div()
    .flex()
    .flex_col()
```

An overlay root can be:

```rust
div()
    .relative()
```

with positioned children:

```rust
div()
    .relative()
    .child(content)
    .child(
        div()
            .absolute()
            .top(px(2))
            .right(px(2))
            .size(px(6))
    )
```

A clickable custom control is still a `Div`:

```rust
div()
    .id("save")
    .px(px(8))
    .py(px(4))
    .on_click(...)
    .child("Save")
```

## 6.2 `Div` does not replace true leaf primitives

`Div` is the universal **box/composition/layout/interactivity** primitive.

It should not artificially replace concepts that are intrinsically different rendering primitives.

Likely core leaf primitives include:

```text
Text
Image
possibly Canvas/CustomElement
```

Strings should convert into text automatically:

```rust
div().child("Hello")
```

rather than always requiring:

```rust
div().child(text("Hello"))
```

Explicit `text(...)` remains useful when text-specific styling or behavior is needed.

---

# 7. Styling and Layout API

The API should remain deliberately close to GPUI where practical.

## 7.1 Units

Use explicit units:

```rust
px(8)
```

rather than making raw integers silently mean pixels.

Example:

```rust
div()
    .w(px(120))
    .h(px(40))
```

This leaves space for future units without ambiguity.

Potential core representation:

```rust
#[derive(Clone, Copy)]
pub struct Pixels(pub i32);

pub const fn px(value: i32) -> Pixels {
    Pixels(value)
}
```

A broader length type may eventually contain:

```rust
pub enum Length {
    Auto,
    Px(Pixels),
    Relative(...),
    // potentially other constrained embedded-friendly units
}
```

Exact numeric storage remains open; integer/fixed-point representation may be preferable on devices without efficient FPUs.

## 7.2 Sizing

Desired API:

```rust
.w(px(100))
.h(px(50))
.min_w(px(50))
.max_w(px(200))
.min_h(px(20))
.max_h(px(100))

.w_full()
.h_full()
.size_full()
.size(px(40))
```

## 7.3 Flex layout

Desired vocabulary:

```rust
.flex()
.flex_row()
.flex_col()

.flex_1()
.flex_grow()
.flex_shrink()
.flex_none()
```

Alignment:

```rust
.items_start()
.items_end()
.items_center()
.items_stretch()

.justify_start()
.justify_end()
.justify_center()
.justify_between()
.justify_around()
```

Potential self-alignment methods:

```rust
.self_start()
.self_end()
.self_center()
.self_stretch()
```

## 7.4 Gaps and spacing

```rust
.gap(px(8))
```

Padding:

```rust
.p(px(8))
.px(px(8))
.py(px(4))
.pt(px(4))
.pr(px(8))
.pb(px(4))
.pl(px(8))
```

Margin:

```rust
.m(px(8))
.mx(px(8))
.my(px(4))
.mt(px(4))
.mr(px(8))
.mb(px(4))
.ml(px(8))
```

## 7.5 Positioning

Desired API:

```rust
.relative()
.absolute()

.top(...)
.right(...)
.bottom(...)
.left(...)
.inset(...)
```

## 7.6 Overflow and clipping

Eventually:

```rust
.overflow_hidden()
.overflow_scroll()
```

Potential axis-specific forms:

```rust
.overflow_x_scroll()
.overflow_y_scroll()
```

A dedicated `ScrollView` should be optional convenience rather than a required fundamental element.

## 7.7 Paint/style properties

Examples:

```rust
.bg(color)
.border_1()
.border_color(color)
.rounded(px(4))
```

Potential directional variants can be added only as needed.

The core framework should avoid semantic style concepts such as `Primary`, `Danger`, etc.

---

# 8. Style Trait Architecture

The fluent API should be expressed using traits with default methods rather than macros.

A minimal underlying trait:

```rust
pub trait Styled: Sized {
    fn style_mut(&mut self) -> &mut Style;
}
```

Extension methods:

```rust
pub trait StyledExt: Styled {
    fn flex(mut self) -> Self {
        self.style_mut().layout.display = Display::Flex;
        self
    }

    fn flex_col(mut self) -> Self {
        self.style_mut().layout.flex_direction = FlexDirection::Column;
        self
    }

    fn w(mut self, width: impl Into<Length>) -> Self {
        self.style_mut().layout.width = width.into();
        self
    }

    fn p(mut self, value: impl Into<Length>) -> Self {
        let value = value.into();
        self.style_mut().layout.padding = Edges::all(value);
        self
    }
}

impl<T: Styled> StyledExt for T {}
```

Internally, style should probably be separated by responsibility:

```rust
pub struct Style {
    pub layout: LayoutStyle,
    pub paint: PaintStyle,
    pub text: TextStyle,
}
```

The exact split remains implementation detail.

The important API property is that users can simply write:

```rust
div()
    .flex()
    .w_full()
    .p(px(8))
    .bg(color)
```

without caring which internal style struct each property belongs to.

---

# 9. Conditional Fluent Composition

Conditional styling/composition should stay close to GPUI.

## 9.1 `.when(...)`

```rust
div()
    .when(self.selected, |this| {
        this.border_1()
    })
```

This should be implemented generically:

```rust
pub trait Fluent: Sized {
    fn when(
        self,
        condition: bool,
        f: impl FnOnce(Self) -> Self,
    ) -> Self {
        if condition {
            f(self)
        } else {
            self
        }
    }

    fn when_some<T>(
        self,
        value: Option<T>,
        f: impl FnOnce(Self, T) -> Self,
    ) -> Self {
        match value {
            Some(value) => f(self, value),
            None => self,
        }
    }
}

impl<T> Fluent for T {}
```

These closures execute immediately while building the declarative element.

They are **not** stored callbacks and therefore have no persistent memory cost.

## 9.2 Optional children

`Option<E>` should be usable naturally:

```rust
div()
    .child(
        self.error.as_ref().map(|error| {
            div().child(error.as_str())
        })
    )
```

This should remain allocation-free.

## 9.3 Heterogeneous conditional branches

Rust cannot directly return two unrelated concrete types from:

```rust
if condition {
    div()
} else {
    image(...)
}
```

without type erasure.

An allocator-free framework can provide an enum-based helper:

```rust
enum Either<A, B> {
    A(A),
    B(B),
}
```

and a helper such as:

```rust
either(condition, || div(), || image(...))
```

The exact public helper name is not yet fixed.

---

# 10. Declarative Children Without Allocation

One of the central implementation challenges is supporting:

```rust
div()
    .child(A)
    .child(B)
    .child(C)
```

without using:

```rust
Vec<Box<dyn Element>>
```

The selected design is a compile-time heterogeneous child list.

## 10.1 `Div<C>`

```rust
pub struct Div<C = NoChildren> {
    style: Style,
    interactivity: Interactivity,
    children: C,
}
```

Initially:

```rust
div()
```

has type:

```rust
Div<NoChildren>
```

## 10.2 Heterogeneous push list

```rust
pub struct NoChildren;

pub struct Push<C, E> {
    previous: C,
    element: E,
}
```

Therefore:

```rust
div()
    .child(A)
    .child(B)
```

has a type conceptually equivalent to:

```rust
Div<Push<Push<NoChildren, A>, B>>
```

The user never needs to name this type.

## 10.3 `Children`

```rust
pub trait Children {
    fn mount_children(
        self,
        parent: NodeId,
        cx: &mut MountCx<'_>,
    ) -> Result<(), MountError>;
}
```

Empty implementation:

```rust
impl Children for NoChildren {
    fn mount_children(
        self,
        _: NodeId,
        _: &mut MountCx<'_>,
    ) -> Result<(), MountError> {
        Ok(())
    }
}
```

Recursive implementation:

```rust
impl<C, E> Children for Push<C, E>
where
    C: Children,
    E: IntoElement,
{
    fn mount_children(
        self,
        parent: NodeId,
        cx: &mut MountCx<'_>,
    ) -> Result<(), MountError> {
        self.previous.mount_children(parent, cx)?;

        let child = self.element.into_element();
        let child_id = child.mount(cx)?;

        cx.append_child(parent, child_id)?;

        Ok(())
    }
}
```

Previous children are mounted first so source order is preserved.

## 10.4 `.child(...)`

Conceptually:

```rust
impl<C> Div<C>
where
    C: Children,
{
    pub fn child<E>(self, element: E) -> Div<Push<C, E>>
    where
        E: IntoElement,
    {
        Div {
            style: self.style,
            interactivity: self.interactivity,
            children: Push {
                previous: self.children,
                element,
            },
        }
    }
}
```

This is zero-allocation and context-free.

---

# 11. Dynamic Children and Iterators

Dynamic lists must also remain natural:

```rust
div()
    .children(
        self.items.iter().map(|item| {
            div().child(item.name.as_str())
        })
    )
```

An iterator's items all have one concrete Rust type, so dynamic length does not require dynamic element type erasure.

## 11.1 `Many<I>`

```rust
pub struct Many<I> {
    iter: I,
}
```

```rust
impl<I, E> Children for Many<I>
where
    I: IntoIterator<Item = E>,
    E: IntoElement,
{
    fn mount_children(
        self,
        parent: NodeId,
        cx: &mut MountCx<'_>,
    ) -> Result<(), MountError> {
        for element in self.iter {
            let child = element.into_element();
            let node = child.mount(cx)?;
            cx.append_child(parent, node)?;
        }

        Ok(())
    }
}
```

`.children(iterator)` can internally append a `Many<I>` into the same generic child list.

No temporary `Vec` is required.

---

# 12. `ParentElement`

Although `Div` could expose `.child()` as an inherent method, a capability trait is useful because wrappers such as `Stateful<E>` should preserve child composition.

Conceptual interface:

```rust
pub trait ParentElement: Sized {
    type WithChild<E>: ParentElement
    where
        E: IntoElement;

    fn child<E>(
        self,
        child: E,
    ) -> Self::WithChild<E>
    where
        E: IntoElement;
}
```

For `Div<C>`:

```rust
impl<C: Children> ParentElement for Div<C> {
    type WithChild<E>
        = Div<Push<C, E>>
    where
        E: IntoElement;

    fn child<E>(self, child: E) -> Self::WithChild<E>
    where
        E: IntoElement,
    {
        // construct Div<Push<C, E>>
    }
}
```

The exact trait syntax should be validated on stable Rust during the first prototype.

---

# 13. `IntoElement` and `Element`

These are the bridge from high-level declarative Rust values to runtime frame nodes.

## 13.1 `IntoElement`

```rust
pub trait IntoElement {
    type Element: Element;

    fn into_element(self) -> Self::Element;
}
```

Framework primitives generally implement this by identity:

```rust
impl<C> IntoElement for Div<C>
where
    C: Children,
{
    type Element = Self;

    fn into_element(self) -> Self {
        self
    }
}
```

Strings, numeric values, entities, images, and other convenient types may convert into appropriate concrete element types.

## 13.2 `Element`

The high-level element should be consumed when lowered:

```rust
pub trait Element: Sized {
    fn mount(
        self,
        cx: &mut MountCx<'_>,
    ) -> Result<NodeId, MountError>;
}
```

`mount(self)` is intentionally consuming.

This allows a generic declarative value to disappear after lowering and avoids retaining huge compile-time generic types in runtime storage.

---

# 14. Borrowed Render Data

A major usability goal is allowing views to render borrowed data directly:

```rust
.child(self.name.as_str())
```

without requiring users to clone or allocate strings.

The proposed lifecycle is:

```text
borrow Entity<T>
      │
      ▼
T::render(...)
      │
      ▼
temporary element tree may borrow T
      │
      ▼
mount/lower immediately
      │
      ▼
copy/encode necessary runtime data into frame storage
      │
      ▼
drop temporary elements
      │
      ▼
release entity borrow
```

Therefore a high-level `Text<'a>` may borrow:

```rust
&'a str
```

as long as the mounted frame representation no longer contains that reference after the entity borrow ends.

For text, the runtime may copy text into frame scratch storage, or generate a paint representation that owns/copies the necessary data.

This requirement should strongly influence frame arena design.

---

# 15. Persistent Application State: `Entity<T>`

`Entity<T>` should be a core concept rather than an optional advanced feature.

## 15.1 Motivation

Without entities, parent views would need to own children directly:

```rust
struct App {
    settings: Settings,
}
```

and rendering nested stateful components would require awkward nested mutable borrowing:

```rust
cx.component(&mut self.settings)
```

An entity model instead allows:

```rust
struct App {
    settings: Entity<Settings>,
}
```

and:

```rust
div().child(self.settings)
```

The parent does not borrow the child's state while rendering.

## 15.2 `Entity<T>` semantics

`Entity<T>` is not a smart pointer analogous to `Rc<T>`.

It is a **typed handle/capability** identifying runtime-owned persistent state.

Conceptual type:

```rust
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Entity<T> {
    id: EntityId,
    marker: PhantomData<fn() -> T>,
}
```

It should likely be `Copy`.

Copying an `Entity<T>` copies only a small handle; it does not modify a reference count.

## 15.3 No `Deref`

This should not compile:

```rust
entity.some_field
*entity
```

State access must go through runtime context methods so aliasing can be controlled:

```rust
entity.read(cx, |state| {
    ...
});
```

and:

```rust
entity.update(cx, |state, cx| {
    ...
});
```

## 15.4 Entity lifetime

For the first version, entities may simply live for the lifetime of the runtime/application.

This avoids:

- arbitrary free operations
- fragmentation
- weak handles
- reference counting
- complicated lifetime/reclamation logic

Later, dynamic removal can be added if real applications require it.

---

# 16. `EntityId`

`EntityId` identifies runtime persistent state.

Suggested representation:

```rust
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct EntityId {
    slot: u16,
    generation: u16,
}
```

Even if initial entities are never destroyed, reserving generation semantics early can make future dynamic entity reclamation safer.

Exact bit widths are not yet fixed.

Important rule:

> `EntityId` is application-state identity, not element identity and not frame-node identity.

---

# 17. Entity Storage

Without arbitrary deallocation, persistent state can live in a fixed-capacity heterogeneous bump arena.

Conceptually:

```text
Entity arena
┌──────────────────────────────┐
│ App                          │
├──────────────────────────────┤
│ Header                       │
├──────────────────────────────┤
│ Settings                     │
├──────────────────────────────┤
│ Counter                      │
├──────────────────────────────┤
│ unused                       │
└──────────────────────────────┘
```

Metadata might contain:

```rust
struct EntityMeta {
    offset: usize,
    // type checking metadata
    // borrow state
    // drop function
    // generation
}
```

Values are placed into aligned fixed storage.

Entity allocation is therefore approximately:

```text
align bump pointer
write T
create metadata
advance pointer
```

No general-purpose allocator or fragmentation is required.

Capacity exhaustion should be explicit.

---

# 18. Entity Creation

Desired style:

```rust
let counter = cx.new(|cx| Counter::new(cx))?;
```

or for simple values:

```rust
let counter = cx.new(|_| Counter { value: 0 })?;
```

The exact error/construction signature needs prototyping.

Possible form:

```rust
pub fn new<U>(
    &mut self,
    constructor: impl FnOnce(&mut Context<U>) -> U,
) -> Result<Entity<U>, CapacityError>
where
    U: 'static;
```

Potential future `try_new` can allow fallible constructors.

Fixed-capacity failure should not be silently hidden.

---

# 19. `Render`

Stateful entity-backed views implement:

```rust
pub trait Render: 'static {
    fn render<'a>(
        &'a mut self,
        cx: &'a mut Context<'_, Self>,
    ) -> impl IntoElement + 'a;
}
```

The exact stable-Rust signature must be validated in code.

Semantic requirements:

1. Rendering has mutable access to the current entity.
2. Rendering has a typed `Context<Self>`.
3. The returned declarative element may borrow from `self` and possibly context-provided transient data.
4. The returned element is mounted before the entity borrow is released.
5. The resulting runtime frame data must not retain illegal references into the entity.

Example:

```rust
impl Render for Settings {
    fn render(
        &mut self,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + '_ {
        div()
            .flex()
            .flex_col()
            .child(self.title.as_str())
            .child(
                div()
                    .id("save")
                    .on_click(cx.listener(Self::save))
                    .child("Save")
            )
    }
}
```

---

# 20. Rendering `Entity<T>` Inside Other Views

For `T: Render`, `Entity<T>` should implement `IntoElement`.

Example:

```rust
struct App {
    header: Entity<Header>,
    content: Entity<Content>,
}

impl Render for App {
    fn render(
        &mut self,
        _cx: &mut Context<Self>,
    ) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .child(self.header)
            .child(self.content)
    }
}
```

The parent should **not** immediately render the child while it is itself borrowed.

Instead, mounting produces an entity placeholder node:

```text
Div
├── EntityNode(Header #2)
└── EntityNode(Content #3)
```

After the parent's render borrow is released, the runtime expands each entity node independently:

```text
borrow App
→ render App
→ release App

borrow Header
→ render Header
→ release Header

borrow Content
→ render Content
→ release Content
```

This is a central reason to use entities.

---

# 21. One Visual Mount Per Entity Per Frame

A renderable entity should initially be allowed to appear at most once in the element tree during a frame.

This should be considered invalid:

```rust
div()
    .child(self.counter)
    .child(self.counter)
```

because the same entity-local element identity would now correspond to two visual locations.

The runtime should detect this, at least in debug builds.

If two independently rendered views need shared state, use separate view entities referencing a common model entity:

```text
Entity<CounterView> #7 ──┐
                         ├── Entity<CounterModel> #2
Entity<CounterView> #8 ──┘
```

This keeps visual identity and shared model state distinct.

---

# 22. `Context<T>`

`Context<T>` represents framework capabilities while operating on a particular entity.

Initial public API should remain deliberately small.

Conceptual methods:

```rust
impl<T: 'static> Context<'_, T> {
    pub fn entity(&self) -> Entity<T>;

    pub fn entity_id(&self) -> EntityId;

    pub fn listener<E, F>(
        &mut self,
        listener: F,
    ) -> Listener<E>
    where
        F: Fn(
            &mut T,
            &E,
            &mut Context<T>,
        ) + 'static;

    pub fn notify(&mut self);

    pub fn new<U>(
        &mut self,
        constructor: impl FnOnce(&mut Context<U>) -> U,
    ) -> Result<Entity<U>, CapacityError>
    where
        U: 'static;
}
```

Cross-entity read/update APIs should also exist, likely on `Entity<T>` using an application/context capability.

Avoid adding GPUI's much larger desktop application services until concrete embedded use cases require them.

---

# 23. Runtime Entity Borrow Checking

Because `Entity<T>` access is dynamic by handle, compile-time Rust borrowing cannot prevent every aliasing error.

Each entity metadata record should therefore maintain a runtime borrow state similar in spirit to `RefCell`:

```rust
enum BorrowState {
    Free,
    Shared(u16),
    Exclusive,
}
```

A read:

```rust
entity.read(cx, ...)
```

requires a shared borrow.

An update:

```rust
entity.update(cx, ...)
```

requires an exclusive borrow.

Illegal recursive access should produce a well-defined developer error, likely a debug panic or an internal borrow error surfaced through selected APIs.

Example invalid pattern:

```rust
counter.update(cx, |counter, cx| {
    counter_entity.update(cx, ...); // same entity recursively
});
```

The framework should never invoke undefined behavior due to dynamic aliasing.

---

# 24. Entity Read and Update APIs

Desired style:

```rust
self.network.read(cx, |network| {
    let connected = network.connected;
});
```

and:

```rust
self.network.update(cx, |network, cx| {
    network.enabled = true;
    cx.notify();
});
```

Possible convenience methods:

```rust
read
read_with
update
```

Exact signatures remain open because they interact with the distinction between `Context<T>` and a more generic application context.

The invariant is more important than the syntax:

> State behind `Entity<T>` is only accessed through the runtime so borrow rules and notifications can be managed correctly.

---

# 25. Listeners

The framework should support GPUI-like listeners:

```rust
.on_click(cx.listener(Self::increment))
```

and capturing closures:

```rust
.on_click(cx.listener(move |this, _, cx| {
    this.selected = Some(id);
    cx.notify();
}))
```

without requiring heap allocation.

## 25.1 Typed public listener

```rust
pub struct Listener<E> {
    id: ListenerId,
    marker: PhantomData<fn(E)>,
}
```

This provides compile-time event type safety.

For example:

```rust
.on_click(...)
```

requires:

```rust
Listener<ClickEvent>
```

and cannot accidentally receive a `Listener<PointerMoveEvent>`.

## 25.2 Listener closure storage

Closures should be stored in a fixed-capacity **frame callback arena**, not in `Box<dyn Fn>`.

Conceptually:

```text
Frame callback arena
┌────────────────────────────┐
│ closure #0                 │
│ closure #1                 │
│ closure #2                 │
│ ...                        │
│ unused                     │
└────────────────────────────┘
```

A callback record needs enough metadata to invoke an erased heterogeneous closure safely:

```text
closure bytes
invoke trampoline
possibly drop trampoline
associated entity ID
associated event type metadata if required
```

The `Div`/frame node stores only a small listener identifier.

## 25.3 `'static` captures

Stored listeners should require `'static` captures.

Good captures:

```text
u32
usize
enums
Entity<T>
small Copy identifiers
owned fixed-capacity values where appropriate
```

A listener should not capture a borrowed reference into the current `self`, because the entity may mutate before the event fires.

The callback receives `&mut T` again when dispatched.

---

# 26. Listener Lifetime

Listener storage is **frame/scene scoped**, not application-lifetime scoped.

Conceptually:

```text
render generation N
    callbacks A B C
    nodes refer to A B C

input event
    callback B runs

new render generation N+1
    old callback arena reset
    callbacks D E F created
```

This is a major simplification compared with retaining heap-backed closures indefinitely.

The declarative tree is rebuilt anyway, so event callbacks can be rebuilt with it.

---

# 27. `cx.listener(...)` and Entities

A listener should target an entity by `EntityId`, not by storing a raw pointer to `&mut T` across frames.

Conceptually:

```rust
Listener<ClickEvent> {
    entity: EntityId(7),
    callback: CallbackId(23),
}
```

When dispatched:

```text
ClickEvent
   ↓
listener #23
   ↓
Entity #7
   ↓
runtime obtains exclusive borrow of Counter
   ↓
callback(
    &mut Counter,
    &ClickEvent,
    &mut Context<Counter>
)
```

This is safer and aligns naturally with the entity runtime.

---

# 28. `notify()` Semantics

`cx.notify()` means:

> The visual representation of this entity may have changed and should be reconsidered for rendering.

Initially, the simple implementation can be:

```text
notify
→ mark UI dirty
→ rebuild relevant/full tree
→ layout
→ repaint
```

Not every event handler must notify.

For example, an event handler may perform a non-visual side effect without repainting.

This distinction is especially important for slow or power-sensitive displays.

Later, entity identity can enable subtree-level invalidation and damage tracking.

---

# 29. Identity Model Overview

The framework uses multiple identity types for different purposes.

| Type             | Lifetime           | Purpose                            | Public?    |
| ---------------- | ------------------ | ---------------------------------- | ---------- |
| `EntityId`       | application        | persistent state identity          | indirectly |
| `Entity<T>`      | application        | typed handle to state              | yes        |
| `ElementId`      | across renders     | local stable UI identity           | yes        |
| `ElementStateId` | across renders     | resolved runtime UI state identity | no         |
| `NodeId`         | one rendered scene | transient frame node index         | no         |

These types must not be interchangeable.

---

# 30. `ElementId`

`.id(...)` assigns stable identity to a declarative element.

Examples:

```rust
.id("save")
.id(42)
.id(("network", network.id))
```

A possible allocator-free representation:

```rust
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub enum ElementId {
    Name(&'static str),
    Value(u64),
    NamedValue(&'static str, u64),
}
```

Conversion trait:

```rust
pub trait IntoElementId {
    fn into_element_id(self) -> ElementId;
}
```

Likely implementations:

```text
&'static str
u8/u16/u32/u64/usize
(&'static str, integer)
```

Dynamic borrowed strings should not initially be accepted as IDs because persistent identity would require allocation/copying.

Machine-friendly identifiers are preferred:

```rust
.id(("sensor", sensor.id))
```

---

# 31. Entity Namespace for Element IDs

`ElementId` is **local**, not globally unique.

Each renderable entity establishes an identity namespace.

Therefore both of these can safely use:

```rust
.id("save")
```

inside different entities:

```text
Entity<Settings> #4 / "save"
Entity<Dialog>   #9 / "save"
```

These are different persistent UI identities.

An important consequence is that moving an entity to a different location in the parent tree does not change the identity of its internal elements.

This is desirable for preserving:

- focus
- scroll state
- press state
- custom element state

---

# 32. Nested Element IDs

Identified elements form nested local identity scopes.

Example:

```rust
div()
    .id("settings")
    .child(
        div()
            .id("display")
            .child(
                div().id("brightness")
            )
    )
```

Conceptual resolved identity:

```text
Entity #4
  / settings
    / display
      / brightness
```

Unnamed elements do not contribute to persistent identity paths.

---

# 33. `ElementStateId`

The runtime should not repeatedly store full identity paths.

Instead, it maintains a persistent element-state table and resolves local IDs into compact internal handles.

Conceptual entry:

```rust
struct ElementStateEntry {
    parent: Option<ElementStateId>,
    id: ElementId,
    last_seen_generation: u32,
    state: InteractionState,
}
```

During render, the current persistent scope plus a local `ElementId` resolves to an `ElementStateId`.

Example:

```text
Entity #4 / settings / display / brightness
```

may resolve to:

```rust
ElementStateId(19)
```

Event/hit-test structures then carry only the compact runtime identifier.

`ElementStateId` should remain private to the framework.

---

# 34. Element State Lifetime

Element state should survive while an identified element continues to appear across render generations.

Suggested initial policy:

```text
frame N:     element present
frame N+1:   element present -> state preserved
frame N+2:   element absent  -> state can be reclaimed
```

If state must survive an element disappearing and later returning, that state should generally belong in an `Entity<T>` instead.

This provides a clean rule:

> Entities hold important long-lived application/view state. Element state holds ephemeral UI interaction state.

A mark-and-sweep or generation-based state cleanup can keep persistent element storage bounded.

---

# 35. Dynamic Lists and IDs

List elements that require state must use distinct stable IDs.

Incorrect:

```rust
div()
    .children(
        self.networks.iter().map(|network| {
            div()
                .id("network")
                .child(network.name())
        })
    )
```

Correct:

```rust
div()
    .children(
        self.networks.iter().map(|network| {
            div()
                .id(("network", network.id()))
                .child(network.name())
        })
    )
```

When the list reorders, persistent element state follows the ID rather than the frame position.

The runtime should detect duplicate IDs within the same persistent identity scope and provide a useful debug error.

---

# 36. No Separate React-Style `key()` Initially

A separate `.key(...)` concept is not necessary in the initial architecture.

`.id(...)` already means:

> This element has stable identity across renders.

That one mechanism can power:

- stateful interactions
- focus
- scroll state
- list item identity
- previous bounds
- custom persistent UI state
- future damage tracking

Avoiding a separate `key` keeps the mental model simpler.

---

# 37. `NodeId`

`NodeId` identifies a node only within the current frame/runtime element tree.

Possible representation:

```rust
#[derive(Clone, Copy)]
pub(crate) struct NodeId(u16);
```

It is merely an arena index.

A `NodeId` can change arbitrarily between frames.

It is used for:

- parent/child traversal
- layout
- paint order
- hit testing
- clipping
- z-order

It must never be used as persistent identity.

Example:

```text
Frame 1: NodeId(5) = save button
Frame 2: NodeId(5) = unrelated text node
```

This is valid.

---

# 38. `.id()` Changes the Type

A major API decision is that `.id(...)` should not merely set an optional field.

It should transform the element into a stateful wrapper:

```rust
div()              // Div<C>
div().id("save")  // Stateful<Div<C>>
```

Conceptual wrapper:

```rust
pub struct Stateful<E> {
    id: ElementId,
    element: E,
    stateful_interactivity: StatefulInteractivity,
}
```

This lets the type system express which operations require persistent identity.

---

# 39. `InteractiveElement`

Stateless/single-event interaction can be available on ordinary interactive elements.

Conceptual trait:

```rust
pub trait InteractiveElement: Sized {
    fn interactivity_mut(&mut self) -> &mut Interactivity;
}
```

Extension methods may include:

```rust
.on_pointer_down(...)
.on_pointer_up(...)
.on_pointer_move(...)
```

These are single current-tree events and do not necessarily require cross-frame identity.

`.id(...)` should also be provided through this capability:

```rust
fn id(
    self,
    id: impl IntoElementId,
) -> Stateful<Self>
```

---

# 40. Stateful Interactions

Some interactions inherently require persistent identity.

Examples:

- click
- focus
- active/pressed state
- drag
- scroll

A click spans multiple input moments:

```text
pointer down
   ↓
remember target identity
   ↓
possibly rerender
   ↓
pointer up
   ↓
compare persistent target identity
```

Therefore `.on_click(...)` should only exist after `.id(...)`.

This should fail to compile:

```rust
div()
    .on_click(...)
```

This should compile:

```rust
div()
    .id("save")
    .on_click(...)
```

The API encodes a real runtime requirement rather than merely a stylistic preference.

---

# 41. `StatefulInteractiveElement`

Conceptual trait:

```rust
pub trait StatefulInteractiveElement: InteractiveElement {
    fn stateful_interactivity_mut(
        &mut self,
    ) -> &mut StatefulInteractivity;
}
```

Extension API can expose:

```rust
.on_click(...)
.focusable()
.on_focus(...)
.on_blur(...)
```

Later:

```rust
.on_drag(...)
.scrollable(...)
```

Only `Stateful<E>` and other types that genuinely provide stable identity should implement this trait.

---

# 42. Capability Forwarding Through `Stateful<E>`

Calling `.id()` must not make the rest of the fluent API awkward.

This must work:

```rust
div()
    .id("save")
    .flex()
    .px(px(8))
    .child("Save")
    .on_click(...)
```

Therefore `Stateful<E>` forwards capabilities.

If:

```rust
E: Styled
```

then:

```rust
Stateful<E>: Styled
```

If:

```rust
E: ParentElement
```

then `Stateful<E>` also behaves as a parent, preserving the wrapper as its child-list type changes.

Example idea:

```rust
impl<E: Styled> Styled for Stateful<E> {
    fn style_mut(&mut self) -> &mut Style {
        self.element.style_mut()
    }
}
```

`ParentElement` forwarding is more type-heavy and should be validated carefully in the prototype.

---

# 43. Input and Event Model

The framework should normalize hardware inputs into platform-independent events.

Potential low-level events:

```rust
pub struct PointerDownEvent {
    pub position: Point,
    pub button: PointerButton,
}

pub struct PointerUpEvent {
    pub position: Point,
    pub button: PointerButton,
}

pub struct PointerMoveEvent {
    pub position: Point,
}
```

Semantic events:

```rust
pub struct ClickEvent {
    pub position: Point,
}
```

Additional future input types:

```text
ScrollEvent
KeyDown/KeyUp
Encoder event
FocusNext
FocusPrevious
Activate
```

Use the term **pointer** rather than mouse for the generic spatial input abstraction because embedded devices frequently use touch.

---

# 44. Hit Testing

Layout produces bounds for nodes.

Interactive nodes register hit-test metadata referring to:

- frame `NodeId`
- persistent `ElementStateId` when required
- listener IDs
- clipping/z-order information

A pointer event can then:

```text
physical input
   ↓
normalize position/event
   ↓
hit test front-to-back
   ↓
resolve interactive target
   ↓
dispatch listener
```

The exact capture/bubble propagation model can be deferred.

Initial v0.1 interaction can route directly to the topmost target.

---

# 45. Focus

Focus should eventually be a first-class runtime concept because many embedded devices are not touch-driven.

An identified element can become focusable:

```rust
div()
    .id("settings")
    .focusable()
```

The same controls should eventually work with:

- touch
- rotary encoder
- D-pad
- buttons

Potential semantic navigation:

```text
FocusNext
FocusPrevious
Activate
```

Focus state belongs in persistent element interaction state, not application structs.

---

# 46. Persistent Memory Model

The architecture should use at least two distinct memory regions.

## 46.1 Persistent entity arena

Lifetime: entire runtime/application.

Contains:

- entity values
- entity metadata
- borrow state
- entity generations

## 46.2 Persistent element-state storage

Lifetime: across renders, but entries may be reclaimed when elements disappear.

Contains:

- resolved element identity hierarchy
- interaction state
- focus/press/scroll metadata
- previous bounds

## 46.3 Frame arena

Lifetime: one rendered scene/generation.

Contains things such as:

- runtime nodes
- child linkage
- temporary mounted text bytes
- listener closures
- hit-test records
- layout scratch
- paint commands if using a paint-list architecture

The frame arena resets wholesale when a new scene is built.

---

# 47. Arena Configuration

The framework should provide deterministic capacity without making the public API unbearable.

Avoid forcing users to specify a huge const-generic list like:

```rust
Ui<256, 4096, 32, 64, 8, ...>
```

Potential alternatives:

### A typed memory configuration

```rust
let memory = UiMemory::<DefaultEmbeddedProfile>::new();
let ui = Ui::new(memory);
```

### Explicit backing memory

```rust
static mut UI_MEMORY: UiMemory<...> = ...;
```

### Builder/config object

If `alloc` is available, runtime memory can be dynamically allocated while preserving the same API.

This part should be designed after measuring the actual prototype's storage needs.

---

# 48. Capacity Errors

Allocator-free design means capacity is finite.

Potential failures include:

- entity arena full
- frame node arena full
- frame scratch/string arena full
- listener callback arena full
- persistent element-state table full
- hit-test table full

The framework should define explicit error categories rather than produce mysterious failures.

Potential type:

```rust
pub enum CapacityError {
    Entities,
    EntityBytes,
    Nodes,
    FrameBytes,
    Listeners,
    ElementState,
    HitTests,
}
```

Exact granularity remains open.

Debug diagnostics should ideally report requested vs available capacity.

---

# 49. Runtime Rendering Pipeline

A full render generation should conceptually execute:

```text
1. determine dirty entities / root render need
2. reset transient frame arena
3. render root entity
4. mount/lower returned declarative element
5. expand encountered child entities after releasing parent borrows
6. resolve persistent element IDs
7. build compact node tree
8. perform layout
9. build/update hit-test metadata
10. paint
11. commit element-state generation bookkeeping
12. reclaim stale element-state entries
```

Initially, full-tree rebuild and full-screen paint are acceptable.

---

# 50. Layout Pipeline

Layout should be separate from painting.

Never implement elements as a single method that mixes:

```text
layout + draw + event handling
```

Instead:

```text
mount/build
   ↓
layout
   ↓
hit-test preparation
   ↓
paint
```

This separation is required for future:

- clipping
- scrolling
- damage tracking
- layout inspection
- e-ink partial refresh
- independent event routing

---

# 51. Layout Algorithm Direction

The user-facing layout API should be GPUI-like, but the internal engine need not copy GPUI implementation details.

Possible strategies:

1. implement a compact custom subset of flex layout;
2. use/adapt an existing no-std-compatible layout engine if one fits memory constraints;
3. initially support only deterministic row/column flex behaviors and expand later.

The first implementation only needs the subset required by the fluent API that actually exists.

Do not expose unsupported CSS-like concepts merely for API completeness.

---

# 52. Rendering Backend

`embedded-graphics` is the rendering backend, not the UI architecture.

The UI framework should ultimately translate runtime node/style information into `embedded-graphics` drawing operations.

Prefer generic rendering over `DrawTarget` rather than trait-object erasure.

Conceptually:

```rust
pub fn paint<D>(
    frame: &Frame,
    target: &mut D,
) -> Result<(), D::Error>
where
    D: DrawTarget,
{
    ...
}
```

This allows compiler specialization/monomorphization and fits the existing embedded-graphics ecosystem.

---

# 53. Paint Stage

Nodes should not arbitrarily draw themselves during layout.

Potential runtime process:

```text
layout tree
    ↓
paint traversal
    ↓
for each node:
    background
    border
    content/leaf primitive
    children
```

Exact ordering must account for clipping and positioned elements.

An alternative is to lower into a paint-command list after layout.

That decision can remain internal as long as the public element API does not require direct drawing during layout.

---

# 54. Color Representation — Open Design Question

One unresolved issue is whether framework style color values should be directly generic over the target's `PixelColor` type.

Example direct approach:

```rust
div().bg(BinaryColor::On)
```

or:

```rust
div().bg(Rgb565::BLACK)
```

This is attractive because it uses embedded-graphics types directly, but it may cause the target color generic to spread throughout:

```text
Div<C, Color>
Style<Color>
Frame<Color>
...
```

Alternative: framework-independent color representation converted by the backend.

That avoids generic infection but introduces conversion semantics and questions for monochrome/palette displays.

This should be decided only after a small prototype demonstrates how invasive the generic really becomes.

The framework should **not** solve this with a built-in semantic theme system.

---

# 55. Text Representation — Open but Important

User ergonomics should ideally support:

```rust
.child("Temperature")
.child(self.temperature)
```

without manual formatting boilerplate everywhere.

Possible `IntoElement` implementations:

```text
&str
char
integer primitives
heapless::String
other fixed-capacity string types
```

Numeric values may be formatted into frame scratch storage during mount.

Floating-point formatting should potentially be feature-gated because it can significantly affect code size on embedded targets.

Text API should avoid requiring heap allocation.

---

# 56. Image Representation

Image should be a dedicated leaf primitive rather than encoded through `Div`.

Desired use:

```rust
div()
    .child(image(&ICON))
```

The image element can borrow static or render-lifetime image data and lower it into appropriate paint metadata.

Exact support depends on `embedded-graphics` image abstractions.

---

# 57. Custom Elements

Advanced users will eventually need lower-level custom rendering for:

- graphs
- waveforms
- gauges
- game canvases
- terminal widgets
- custom instrumentation

The extension point should be designed only after the basic `Element`/mount/layout/paint stages stabilize.

A custom element should be able to participate in:

- measurement/layout
- painting
- hit testing/interactivity where applicable

without requiring the core framework to define a new built-in element for each use case.

---

# 58. Stateless Reusable Components

Not every reusable UI abstraction should require an `Entity<T>`.

Plain Rust functions should remain useful:

```rust
fn separator() -> impl IntoElement {
    div()
        .w_full()
        .h(px(1))
}
```

and:

```rust
fn card(content: impl IntoElement) -> impl IntoElement {
    div()
        .p(px(8))
        .border_1()
        .child(content)
}
```

This is the preferred form for stateless composition.

---

# 59. `RenderOnce` — Proposed but Deferred

A GPUI-like `RenderOnce` concept is useful for reusable stateless structs:

```rust
struct IconButton<'a> {
    icon: &'a Icon,
    label: &'a str,
}
```

Potential trait:

```rust
pub trait RenderOnce {
    fn render(
        self,
        cx: &mut ElementContext<'_>,
    ) -> impl IntoElement;
}
```

However, there are stable-Rust/coherence questions around blanket `IntoElement` implementations for both `Element` and all `RenderOnce` values.

Therefore `RenderOnce` should **not** block the initial prototype.

Initial core can use:

```text
Render + Entity<T>
Element + IntoElement
plain Rust functions
```

Then `RenderOnce` can be added after the trait coherence model is verified.

Unlike GPUI, we may want `RenderOnce` to support borrowed non-`'static` data because embedded declarative components frequently benefit from zero-copy borrowing.

---

# 60. Event Dispatch and Rerendering

An event handler can mutate entity state.

After listener execution, the old frame's interaction declarations may no longer correspond exactly to the new state.

Safe initial policy:

```text
input event
   ↓
hit test current frame
   ↓
dispatch one listener
   ↓
listener may mutate entity
   ↓
if notify requested:
    rebuild/repaint before processing next visual interaction
```

Even if visual repaint is not requested, the framework must carefully define whether interaction metadata remains valid after mutations.

A conservative initial implementation may rebuild interaction metadata after any mutating listener while using `notify()` specifically to decide whether painting is necessary.

This distinction should be verified during implementation.

---

# 61. Full Redraw First, Damage Tracking Later

Version 0.1 should prioritize correctness:

```text
state changes
→ rebuild scene
→ layout
→ full repaint
```

Do not prematurely optimize with a complex diff engine.

However, persistent identities should make future damage tracking possible.

For each persistent element/entity, retain previous bounds/fingerprint metadata.

Changed content:

```text
old bounds == new bounds
but visual fingerprint changed
→ dirty old/new bounds
```

Moved content:

```text
old bounds != new bounds
→ dirty union(old, new)
```

Removed content:

```text
→ dirty old bounds
```

This is particularly important for:

- e-ink
- low-bandwidth SPI displays
- low-power systems

---

# 62. Display Flush Should Remain Separate From Painting

The framework should distinguish:

```text
what pixels/regions need repainting
```

from:

```text
how the hardware display is physically flushed/refreshed
```

Eventually the API may look conceptually like:

```rust
let damage = ui.render(...)?;
ui.paint(&mut target, &damage)?;
display.flush(&damage)?;
```

Some displays can ignore damage and flush everything.

E-ink drivers may use damage rectangles for partial refresh.

The core UI should not hard-code a particular driver's refresh model.

---

# 63. Theme Boundary

To explicitly record the decision:

**The framework should not contain a `Theme` abstraction as a required core service.**

If application code wants:

```rust
struct Theme {
    primary: Color,
    surface: Color,
    text: Color,
}
```

it can own that normally.

A separate ecosystem crate may later provide styled widgets and themes.

Core APIs remain literal/mechanical:

```rust
.bg(...)
.border_color(...)
.text_color(...)
```

not semantic:

```rust
.variant(ButtonVariant::Primary)
```

---

# 64. Built-in Widgets Boundary

The core framework should initially avoid a large built-in widget library.

A button can be built from mechanisms:

```rust
fn button(
    id: impl IntoElementId,
    content: impl IntoElement,
    listener: Listener<ClickEvent>,
) -> impl IntoElement {
    div()
        .id(id)
        .px(px(8))
        .py(px(4))
        .border_1()
        .on_click(listener)
        .child(content)
}
```

Higher-level crates can later provide:

```text
Button
Toggle
Slider
TextInput
Menu
List
Card
Dialog
```

The core only needs enough generic behavior to make these implementable.

---

# 65. Example: Counter

```rust
struct Counter {
    count: i32,
}

impl Counter {
    fn increment(
        &mut self,
        _: &ClickEvent,
        cx: &mut Context<Self>,
    ) {
        self.count += 1;
        cx.notify();
    }

    fn decrement(
        &mut self,
        _: &ClickEvent,
        cx: &mut Context<Self>,
    ) {
        self.count -= 1;
        cx.notify();
    }
}

impl Render for Counter {
    fn render(
        &mut self,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + '_ {
        div()
            .flex()
            .flex_col()
            .items_center()
            .gap(px(8))
            .child(self.count)
            .child(
                div()
                    .flex()
                    .gap(px(4))
                    .child(
                        div()
                            .id("decrement")
                            .px(px(8))
                            .py(px(4))
                            .border_1()
                            .on_click(cx.listener(Self::decrement))
                            .child("-")
                    )
                    .child(
                        div()
                            .id("increment")
                            .px(px(8))
                            .py(px(4))
                            .border_1()
                            .on_click(cx.listener(Self::increment))
                            .child("+")
                    )
            )
    }
}
```

This example exercises:

- `Render`
- `Context<Self>`
- listener methods
- stable element identity
- stateful interaction
- generic `div()`
- flex layout
- fluent style
- integer-to-text conversion

---

# 66. Example: Nested Entities

```rust
struct App {
    header: Entity<Header>,
    settings: Entity<Settings>,
}

impl App {
    fn new(cx: &mut Context<Self>) -> Result<Self, CapacityError> {
        let header = cx.new(|_| Header::new())?;
        let settings = cx.new(|_| Settings::new())?;

        Ok(Self {
            header,
            settings,
        })
    }
}

impl Render for App {
    fn render(
        &mut self,
        _cx: &mut Context<Self>,
    ) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .size_full()
            .child(self.header)
            .child(self.settings)
    }
}
```

No parent-owned nested mutable state and no message forwarding are required.

---

# 67. Example: Dynamic List

```rust
impl Render for DeviceList {
    fn render(
        &mut self,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + '_ {
        div()
            .flex()
            .flex_col()
            .children(
                self.devices.iter().map(|device| {
                    let id = device.id;

                    div()
                        .id(("device", id))
                        .px(px(8))
                        .py(px(4))
                        .when(self.selected == Some(id), |this| {
                            this.border_1()
                        })
                        .on_click(
                            cx.listener(move |this, _, cx| {
                                this.selected = Some(id);
                                cx.notify();
                            })
                        )
                        .child(device.name.as_str())
                })
            )
    }
}
```

This demonstrates:

- dynamic iterators
- captured listener values
- stable list identity
- conditional fluent styling
- borrowed text

This exact pattern should be one of the prototype's compile tests because it stresses several lifetime constraints simultaneously.

---

# 68. Example: Shared Model, Multiple Views

If two views need the same underlying data, avoid rendering one entity twice.

```rust
struct App {
    model: Entity<TemperatureModel>,
    compact: Entity<CompactTemperatureView>,
    detailed: Entity<DetailedTemperatureView>,
}
```

Each view can store:

```rust
model: Entity<TemperatureModel>
```

Then both read/update shared state through the runtime while retaining independent visual element namespaces.

This is the recommended model/view separation when shared data is necessary.

---

# 69. No-Alloc Internals vs Optional `alloc`

The core architecture should work without allocation.

An optional `alloc` feature may later provide:

- dynamically growing arenas
- more flexible strings
- convenience element erasure such as `AnyElement`
- easier application configuration
- potentially more flexible callback storage

But `alloc` support should be an implementation enhancement rather than a separate public programming model.

A UI written against the basic declarative API should ideally compile in both modes.

---

# 70. Potential Feature Flags

Possible future feature structure:

```toml
[features]
default = []
alloc = []
std = ["alloc"]
float = []
simulator = ["std"]
```

Other backend/format features should only be added when needed.

Avoid premature feature proliferation.

---

# 71. Suggested Crate Structure

Do not split into many crates before the abstractions stabilize.

For the initial implementation, one crate is probably best:

```text
src/
├── lib.rs
├── entity.rs
├── context.rs
├── render.rs
├── element/
│   ├── mod.rs
│   ├── div.rs
│   ├── text.rs
│   ├── image.rs
│   └── stateful.rs
├── style/
│   ├── mod.rs
│   ├── layout.rs
│   └── paint.rs
├── event/
│   ├── mod.rs
│   ├── pointer.rs
│   └── listener.rs
├── runtime/
│   ├── mod.rs
│   ├── entity_arena.rs
│   ├── frame_arena.rs
│   ├── element_state.rs
│   ├── mount.rs
│   └── node.rs
├── layout.rs
└── renderer.rs
```

Once boundaries become stable, it may make sense to split into:

```text
ui-core
ui-embedded-graphics
ui-widgets
ui-simulator
```

but not before implementation proves those boundaries useful.

---

# 72. First Prototype Scope

The first code prototype should deliberately **not** implement a complete UI framework.

It exists to validate the Rust type/lifetime/coherence model.

Minimum prototype:

```text
Render
Context<T>
Entity<T>
EntityId

IntoElement
Element

Div
NoChildren / Push / Many
.child()
.children()

Styled + a handful of style methods

.id() -> Stateful<Div>

InteractiveElement
StatefulInteractiveElement

Listener<E> type skeleton

MountCx
NodeId
minimal fixed node arena
```

No real layout or `embedded-graphics` painting is required yet.

The objective is answering:

1. Can `Render` safely return elements borrowing `self`?
2. Can `cx.listener(...)` coexist with those borrows?
3. Can dynamic `.children(self.items.iter().map(...))` compile when listeners are also created from `cx`?
4. Does the generic child-list approach compile without pathological ergonomics?
5. Can `Stateful<E>` forward `ParentElement` and `Styled` cleanly?
6. Do blanket `IntoElement` implementations create coherence conflicts?
7. Can child entities be deferred until the parent borrow is released?
8. Can all of this work on stable Rust without `alloc`?

These are higher priority than drawing a rectangle.

---

# 73. Prototype Compile-Test Cases

The prototype should include compile tests/examples for all difficult API patterns.

## 73.1 Simple static tree

```rust
div()
    .child("A")
    .child("B")
```

## 73.2 Styling before and after `.id()`

```rust
div()
    .flex()
    .id("x")
    .p(px(4))
```

and:

```rust
div()
    .id("x")
    .flex()
    .p(px(4))
```

Both should work.

## 73.3 Child after `.id()`

```rust
div()
    .id("x")
    .child("hello")
```

## 73.4 Dynamic iterator

```rust
div()
    .children(items.iter().map(|item| {
        div().child(item.name.as_str())
    }))
```

## 73.5 Dynamic iterator plus IDs

```rust
div()
    .children(items.iter().map(|item| {
        div()
            .id(("item", item.id))
            .child(item.name.as_str())
    }))
```

## 73.6 Dynamic iterator plus listener capture

```rust
div()
    .children(items.iter().map(|item| {
        let id = item.id;

        div()
            .id(("item", id))
            .on_click(cx.listener(move |this, _, cx| {
                this.selected = Some(id);
                cx.notify();
            }))
            .child(item.name.as_str())
    }))
```

This is likely one of the most important lifetime tests.

## 73.7 Child entity

```rust
div().child(self.child_entity)
```

## 73.8 Optional child

```rust
div().child(self.error.as_ref().map(|e| e.as_str()))
```

## 73.9 Conditional modifier

```rust
div().when(self.selected, |this| this.border_1())
```

---

# 74. Expected Hard Rust Problems

Several design areas are intentionally marked for validation rather than assumed trivial.

## 74.1 `Render` lifetime coupling

Borrowing `self` for text while also mutably borrowing `cx` to register listeners in sibling branches may trigger borrow-checker issues depending on exact signatures.

The prototype must validate the desired ergonomic example, not just simplified toy cases.

## 74.2 Generic associated type ergonomics

`ParentElement::WithChild<E>` must remain stable-Rust compatible and forward correctly through `Stateful<E>`.

## 74.3 Coherence

Blanket implementations such as:

```rust
impl<E: Element> IntoElement for E
```

can conflict with future blanket implementations for stateless component abstractions.

Do not commit to overly broad blanket impls before evaluating future `RenderOnce` needs.

## 74.4 Erased callback invocation

Heterogeneous closure storage in fixed byte memory requires carefully audited unsafe code for:

- alignment
- lifetime
- invocation trampoline
- drop behavior
- event type correctness
- entity type correctness

Unsafe code should be isolated behind a small internal abstraction with extensive tests.

## 74.5 Entity type erasure

The heterogeneous entity arena similarly requires safe metadata and runtime type invariants.

Prefer typed public handles with all unsafe erasure confined inside the arena implementation.

---

# 75. Unsafe Code Policy

A no-alloc heterogeneous entity/callback arena will likely require some unsafe Rust.

The framework should adopt a strict rule:

> Unsafe code is allowed only in narrow runtime storage primitives; public UI abstractions remain safe.

Likely unsafe modules:

```text
entity_arena
listener_arena
possibly frame scratch arena
```

Each unsafe invariant should be documented explicitly.

Examples:

- an entity slot's stored runtime type must match the `Entity<T>` used to access it;
- entity values are correctly aligned;
- mutable borrows cannot overlap;
- listener trampoline type matches its registered event and entity type;
- callback bytes remain alive until listener arena reset;
- drop functions run exactly once when necessary.

Miri tests should be used where possible under `std` test builds.

---

# 76. Error and Debugging Philosophy

Embedded capacity errors should be explicit, but programmer-logic errors should also be easy to diagnose.

Examples of programmer errors:

```text
duplicate ElementId in one scope
same renderable Entity mounted twice
recursive exclusive entity borrow
stateful operation attempted without identity
```

The last one should ideally be prevented at compile time.

Others should produce strong debug assertions/messages.

In optimized embedded builds, behavior should remain defined even if diagnostics are reduced.

---

# 77. Testing Strategy

## 77.1 Pure unit tests

Test:

- child mounting order
- style modifiers
- `ElementId` conversion
- persistent element state lookup
- duplicate ID detection
- entity borrowing rules
- arena alignment/capacity
- callback invocation

## 77.2 Compile-pass tests

Use compile test infrastructure for intended API patterns.

## 77.3 Compile-fail tests

Useful intended failures:

```rust
div().on_click(...)
```

without `.id()`.

Also test event-listener type mismatch.

## 77.4 Miri

Run entity/callback arena tests under Miri when possible.

## 77.5 Simulator integration

Once layout/paint exists, use `embedded-graphics-simulator` for desktop visual testing.

## 77.6 Embedded smoke targets

Eventually compile real no-std examples for representative MCUs such as Cortex-M and ESP targets to detect accidental allocator/std dependencies.

---

# 78. Performance Philosophy

The framework should optimize for predictable embedded behavior rather than chasing desktop-style benchmark metrics prematurely.

Important metrics:

- static RAM consumed by UI configuration
- persistent entity bytes
- persistent element-state bytes
- peak frame arena usage
- maximum stack depth during generic child mounting
- layout time
- paint time
- bytes transferred to display
- generated firmware/code size

In particular, deeply recursive type-level child lists may create recursive mounting code; implementation should measure stack/code-size behavior and consider flattening strategies if necessary.

---

# 79. Code Size Considerations

Heavy generic APIs can cause monomorphization/code-size growth.

The architecture intentionally lowers generic declarative elements early into compact runtime nodes so the expensive layout/paint runtime is not generic over every unique element-tree type.

Only the mount/lowering glue is monomorphized per declarative structure.

This balance should be measured on real firmware.

If generic child chains cause excessive code growth, optimizations can be introduced internally without changing the public API.

---

# 80. Stack Usage Considerations

Recursive `Push<C, E>` child mounting is elegant, but nested static child chains may create recursive calls.

For ordinary UI trees this is probably acceptable, but embedded targets require measurement.

Potential future alternatives if needed:

- compiler-inlined recursion
- tuple-based child packs
- mounting into a temporary compact stack
- macro-generated flattening

Do not complicate the API before measurements show a problem.

---

# 81. `embedded-graphics` Integration Boundary

Core UI concepts should not expose unnecessary backend details.

Application code should not need to know about primitive drawing calls when using normal elements.

Backend integration owns:

```text
Rectangle / RoundedRectangle drawing
text drawing
image drawing
clip conversion
target color conversion
```

Custom low-level canvas elements may expose direct access to a bounded paint context later.

---

# 82. Recommended Implementation Order

## Phase 0 — API type prototype

Implement only enough to compile desired syntax.

1. `Pixels`, basic `Style`
2. `Div<C>`
3. `NoChildren`, `Push`, `Many`
4. `IntoElement`, `Element`
5. `ParentElement`
6. `Styled` + extension methods
7. `ElementId`, `.id() -> Stateful<E>`
8. capability forwarding
9. `Listener<E>` type shell
10. `Render`, `Context<T>` type shell
11. `Entity<T>`/`EntityId` shell
12. minimal mount arena

## Phase 1 — Entity runtime

1. fixed aligned entity arena
2. typed handles
3. runtime borrow checking
4. entity creation
5. deferred entity rendering
6. entity mount-once detection

## Phase 2 — Listener runtime

1. frame callback arena
2. typed listener registration
3. erased invocation trampolines
4. pointer-down/up dispatch
5. click state
6. `notify()` invalidation

## Phase 3 — Layout

1. fixed sizing
2. full sizing
3. flex row/column
4. gap
5. padding/margin
6. align/justify
7. relative/absolute positioning
8. clipping

## Phase 4 — Paint

1. backgrounds
2. borders
3. text
4. image
5. renderer generic over `DrawTarget`
6. simulator example

## Phase 5 — Navigation and scrolling

1. focus
2. encoder/D-pad navigation
3. scrolling
4. scroll persistent element state

## Phase 6 — Optimization

1. persistent previous bounds
2. damage tracking
3. clipped partial repaint
4. display flush/damage interface
5. e-ink experiments

---

# 83. Decisions Considered Locked for the First Prototype

The following should be treated as current design commitments unless prototyping demonstrates a concrete Rust limitation.

1. `div()` is the central generic compositional/layout element.
2. Do not introduce fundamental `Row`, `Column`, `Container`, etc. types.
3. The fluent layout/style API should stay close to GPUI naming.
4. The core framework does not own application themes/design systems.
5. Stateful views use `Entity<T>`.
6. `Entity<T>` is a typed runtime handle, not an `Rc`-style owning smart pointer.
7. `Entity<T>` should be tiny and likely `Copy`.
8. Persistent entity state uses fixed-capacity runtime storage in no-alloc mode.
9. `Render` is the primary stateful view trait.
10. A renderable `Entity<T>` can be inserted directly as a child.
11. Child entity rendering is deferred until parent mutable borrowing ends.
12. A renderable entity may appear at most once per rendered scene initially.
13. Listeners should look like `cx.listener(...)`.
14. Listener closures live in frame storage rather than heap allocation.
15. Captured listener data must be safe to keep until the current frame is replaced; initially require `'static` captures.
16. `.id(...)` establishes stable UI identity.
17. `.id(...)` changes the concrete type to `Stateful<E>`.
18. Stateful interactions such as `.on_click(...)` require a stateful/identified element.
19. Entities create element-ID namespaces.
20. `ElementId`, `ElementStateId`, and `NodeId` are distinct concepts.
21. No separate React-like `.key()` initially.
22. High-level element children use generic heterogeneous compile-time composition, not `Vec<Box<dyn Element>>`.
23. `.children(iterator)` must work allocation-free.
24. High-level elements are lowered/consumed into compact frame nodes.
25. Layout and painting are separate stages.
26. Full redraw is acceptable initially.
27. The architecture must preserve a path to damage tracking and partial refresh.

---

# 84. Deliberately Open Questions

These should be answered by prototypes/measurements rather than speculation.

## 84.1 Exact `Render` lifetime signature

Can the ideal API compile cleanly for borrowed strings plus `cx.listener` inside dynamic iterators?

## 84.2 `RenderOnce`

What exact stateless struct abstraction avoids coherence issues while allowing borrowed data?

## 84.3 Color representation

Should style use target-native `PixelColor`, a framework color type, or another generic boundary?

## 84.4 Layout implementation

Custom minimal flex engine vs reusable external implementation.

## 84.5 Arena public configuration

Const generics, provided backing storage, profiles, builders, or some combination.

## 84.6 Entity deletion

Whether dynamic entity destruction is actually needed and, if so, when to add generational reclamation/weak handles.

## 84.7 Element-state storage capacity/reclamation

Best compact representation and stale-entry cleanup algorithm.

## 84.8 Text storage

Copy into frame scratch vs another mounted representation.

## 84.9 `notify()` granularity

Full runtime dirty flag initially; eventual per-entity/subtree invalidation semantics.

## 84.10 Interaction rebuild after non-notifying state mutations

Whether the runtime always rebuilds frame interaction metadata after callbacks or can preserve it safely under stricter semantics.

## 84.11 Async

No async runtime should be built now. Later embedded async integration may use Embassy or executor-neutral hooks, but only after the synchronous core is stable.

---

# 85. API Design Principles Going Forward

When new features are proposed, evaluate them against these principles.

## Mechanism over taxonomy

Prefer:

```rust
div().overflow_y_scroll()
```

as a primitive mechanism before creating a mandatory `ScrollView` element type.

## Persistent state should be explicit

Long-lived important state goes in entities.

Hidden element state should be limited to UI mechanics.

## Identity should have purpose

Do not add IDs everywhere automatically if an operation does not require persistent identity.

## Embedded constraints should shape internals, not ruin ergonomics

The user should still get:

```rust
cx.listener(...)
.children(iterator)
.child(entity)
```

The runtime can use arenas and type erasure internally to make that possible without a heap.

## Avoid speculative complexity

Do not implement a full desktop entity system, subscriptions, weak references, async tasks, accessibility tree, or huge widget catalogue until actual embedded use cases justify them.

## Preserve stable Rust if realistically possible

Nightly-only type-system tricks should not be the foundation unless stable Rust proves fundamentally insufficient.

---

# 86. Long-Term Direction

If the initial architecture succeeds, the framework could eventually support a layered ecosystem:

```text
core-ui
    declarative element/runtime/entity/event/layout abstractions

embedded-graphics backend
    painting to DrawTarget

widgets
    reusable generic controls

platform adapters
    touch controllers
    rotary encoders
    keyboard/button mappings

simulator tooling
    desktop iteration/debugging

design-system crates
    optional, application/ecosystem-owned
```

Potential advanced features, only after the fundamentals are stable:

- transitions/animations
- partial rerender by entity
- damage tracking
- e-ink refresh policies
- accessibility semantics for capable devices
- async task/event integration
- custom canvas primitives
- text editing
- virtualized lists
- alloc-backed dynamic convenience mode
- dev inspection/layout overlays in simulator builds

The important constraint is that these should extend the same core mental model rather than replacing it.

---

# 87. Final Working Model

The design can be summarized as follows:

```text
                      ┌──────────────────────┐
                      │ Persistent Entity    │
                      │ Entity<T>            │
                      │ T: Render            │
                      └──────────┬───────────┘
                                 │
                          render(&mut self)
                                 │
                                 ▼
                 ┌────────────────────────────┐
                 │ Declarative Rust Elements  │
                 │                            │
                 │ Div<C>                     │
                 │ Text                       │
                 │ Image                      │
                 │ Entity<Child>              │
                 │ Stateful<E>                │
                 └────────────┬───────────────┘
                              │
                         mount/lower
                              │
                              ▼
                 ┌────────────────────────────┐
                 │ Frame Runtime Tree         │
                 │ NodeId                     │
                 │ layout style               │
                 │ paint data                 │
                 │ hit-test/listener refs     │
                 └───────┬────────┬───────────┘
                         │        │
                      layout    input
                         │        │
                         │        ▼
                         │   ElementStateId
                         │   persistent interaction state
                         │        │
                         ▼        ▼
                      painting / callbacks
                         │
                         ▼
                 embedded-graphics DrawTarget
```

Persistent identity hierarchy:

```text
Entity<T>
   │
   └── EntityId
        establishes namespace
             │
             └── ElementId via .id(...)
                    │
                    └── runtime ElementStateId

Frame-only representation:

Element
   │
   └── NodeId
```

Memory hierarchy:

```text
application lifetime
    ├── entity arena
    └── persistent element-state storage

render-generation lifetime
    └── frame arena
         ├── nodes
         ├── listener closures
         ├── mounted text/scratch
         ├── hit-test records
         └── layout scratch
```

And the intended application code remains concise:

```rust
impl Render for MyView {
    fn render(
        &mut self,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + '_ {
        div()
            .flex()
            .flex_col()
            .size_full()
            .p(px(8))
            .gap(px(4))
            .child(self.title.as_str())
            .children(
                self.items.iter().map(|item| {
                    let id = item.id;

                    div()
                        .id(("item", id))
                        .when(
                            self.selected == Some(id),
                            |this| this.border_1(),
                        )
                        .on_click(
                            cx.listener(move |this, _, cx| {
                                this.selected = Some(id);
                                cx.notify();
                            })
                        )
                        .child(item.label.as_str())
                })
            )
    }
}
```

If we can make this style compile safely, efficiently, and predictably on a small `no_std` target, the framework has achieved its core design goal.
