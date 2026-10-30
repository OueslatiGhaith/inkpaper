use std::{
    convert::Infallible,
    format,
    hash::{DefaultHasher, Hash, Hasher},
    rc::Rc,
    string::String,
    vec,
    vec::Vec,
};

use crate::{frame::NodeKind, *};

type TestRuntime = Runtime<TestStorage<8192, 16, 8192, 128, 256, 1024, 32, 64, 2>, KeyedText>;

struct Label {
    text: &'static str,
    renders: u32,
    activations: u32,
    /// shared with every listener this label registered, to count the live ones
    capture: Rc<()>,
}

impl Label {
    fn new(text: &'static str) -> Self {
        Self {
            text,
            renders: 0,
            activations: 0,
            capture: Rc::new(()),
        }
    }
}

impl Render for Label {
    fn render<'a>(&'a mut self, cx: &mut Context<'_, Self>) -> impl IntoElement + 'a {
        self.renders += 1;

        let capture = self.capture.clone();
        let activate = cx.listener(move |label: &mut Label, _: &ActivateEvent, _| {
            let _ = &capture;
            label.activations += 1;
        });

        div()
            .id(self.text)
            .bg(Color::rgb(200, 200, 200))
            .when_focused(|style| style.bg(Color::BLACK))
            .on_activate(activate)
            .child(self.text)
            .child(Badge)
    }
}

/// reads [`LabelTheme`] while mounting, through the app context
struct Badge;

impl RenderOnce for Badge {
    fn render(self, cx: &AppContext<'_>) -> impl IntoElement {
        let _ = cx.try_global::<LabelTheme>();

        div()
    }
}

struct ShellTheme;

impl Global for ShellTheme {}

struct LabelTheme;

impl Global for LabelTheme {}

struct Shell {
    first: Entity<Label>,
    second: Entity<Label>,
    show_second: bool,
    read_theme: bool,
    /// shows the first label's text, read from the label
    mirror_first: bool,
    /// notified by the shell's render
    touch_first: bool,
    renders: u32,
}

impl Render for Shell {
    fn render<'a>(&'a mut self, cx: &mut Context<'_, Self>) -> impl IntoElement + 'a {
        self.renders += 1;

        if self.read_theme {
            let _ = cx.try_global::<ShellTheme>();
        }
        let mirrored = self
            .mirror_first
            .then(|| self.first.read(cx, |label| label.text).unwrap());

        if self.touch_first {
            self.first
                .update(cx, |label, cx| {
                    label.text = "touched";
                    cx.notify();
                })
                .unwrap();
        }

        div()
            .id("shell")
            .bg(Color::rgb(100, 100, 100))
            .child(self.first)
            .children(self.show_second.then_some(self.second))
            .children(mirrored)
    }
}

struct Fixture {
    runtime: TestRuntime,
    shell: Entity<Shell>,
    first: Entity<Label>,
    second: Entity<Label>,
}

fn fixture(partial: bool) -> Fixture {
    let mut runtime = TestRuntime::default();
    let first = runtime.create(|_| Label::new("first")).unwrap();
    let second = runtime.create(|_| Label::new("second")).unwrap();
    let shell = runtime
        .create_root(|_| Shell {
            first,
            second,
            show_second: true,
            read_theme: true,
            mirror_first: false,
            touch_first: false,
            renders: 0,
        })
        .unwrap();

    runtime.set_partial_rebuilds(partial);
    runtime.rebuild().unwrap();
    runtime.take_invalidation();

    Fixture {
        runtime,
        shell,
        first,
        second,
    }
}

impl Fixture {
    fn label(&self, label: Entity<Label>) -> (u32, u32) {
        self.runtime
            .update(label, |label, _| (label.renders, label.activations))
            .unwrap()
    }

    fn shell_renders(&self) -> u32 {
        self.runtime
            .update(self.shell, |shell, _| shell.renders)
            .unwrap()
    }

    fn notify<T: 'static>(&self, entity: Entity<T>) {
        self.runtime.update(entity, |_, cx| cx.notify()).unwrap();
    }

    /// takes the invalidation and rebuilds, as the presenters do
    fn present(&mut self) {
        assert_eq!(self.runtime.take_invalidation(), Invalidation::Rebuild);
        self.runtime.rebuild().unwrap();
    }

    /// activates the `index`th focusable element
    fn activate(&mut self, index: usize) -> bool {
        self.runtime.clear_focus();
        for _ in 0..=index {
            assert!(self.runtime.focus_next());
        }

        self.runtime.activate_focused().unwrap()
    }

    /// the tree under the root, in tree order
    fn outline(&self) -> Vec<String> {
        let frame = self.runtime.frame();
        let mut outline = Vec::new();
        let mut current = self.runtime.root_node();

        while let Some(node) = current {
            let depth = core::iter::successors(frame.node(node).parent, |&parent| {
                frame.node(parent).parent
            })
            .count();
            let kind = match frame.node(node).kind {
                NodeKind::Div { .. } => String::from("div"),
                NodeKind::Text { text } => format!("{:?}", frame.text(text)),
                NodeKind::Entity { entity, .. } => format!("entity {}", entity.slot()),
                _ => String::from("other"),
            };

            outline.push(format!("{depth} {kind}"));
            current = frame.next_depth_first_node(node);
        }

        outline
    }

    /// every node's style in tree order, with focused styles applied
    fn styles(&self) -> Vec<Option<Style>> {
        let frame = self.runtime.frame();

        core::iter::successors(self.runtime.root_node(), |&node| {
            frame.next_depth_first_node(node)
        })
        .map(|node| frame.style(node))
        .collect()
    }
}

#[test]
fn partial_rebuild_renders_only_the_notified_entity() {
    let mut fixture = fixture(true);

    fixture.notify(fixture.second);
    fixture.present();

    assert_eq!(fixture.shell_renders(), 1);
    assert_eq!(fixture.label(fixture.first), (1, 0));
    assert_eq!(fixture.label(fixture.second), (2, 0));

    // a parent renders again without its clean children
    fixture.notify(fixture.shell);
    fixture.present();

    assert_eq!(fixture.shell_renders(), 2);
    assert_eq!(fixture.label(fixture.first), (1, 0));
    assert_eq!(fixture.label(fixture.second), (2, 0));
}

#[test]
fn partial_rebuilds_are_off_by_default() {
    let mut fixture = fixture(false);

    fixture.notify(fixture.second);
    fixture.present();

    assert_eq!(fixture.shell_renders(), 2);
    assert_eq!(fixture.label(fixture.first), (2, 0));
    assert_eq!(fixture.label(fixture.second), (2, 0));
}

#[test]
fn partial_rebuild_matches_a_full_rebuild() {
    let mut partial = fixture(true);
    let mut full = fixture(false);

    for fixture in [&mut partial, &mut full] {
        fixture
            .runtime
            .update(fixture.second, |label, cx| {
                label.text = "changed";
                cx.notify();
            })
            .unwrap();
        fixture.present();
    }

    assert_eq!(partial.outline(), full.outline());

    for fixture in [&mut partial, &mut full] {
        fixture
            .runtime
            .update(fixture.shell, |shell, cx| {
                shell.show_second = false;
                cx.notify();
            })
            .unwrap();
        fixture.present();
    }

    assert_eq!(partial.outline(), full.outline());
    // the partial rebuilds' leftovers were reclaimed
    let (partial_frame, full_frame) = (partial.runtime.frame(), full.runtime.frame());
    assert_eq!(partial_frame.detached_node_count(), 0);
    assert_eq!(partial_frame.node_count(), full_frame.node_count());
    assert_eq!(
        partial_frame.text_bytes_used(),
        full_frame.text_bytes_used()
    );

    for fixture in [&mut partial, &mut full] {
        fixture.runtime.clear_focus();
        assert!(fixture.runtime.focus_next());
    }
    assert_eq!(partial.styles(), full.styles());
}

#[test]
fn listeners_of_clean_and_rerendered_entities_stay_callable() {
    let mut fixture = fixture(true);
    let first_capture = fixture
        .runtime
        .update(fixture.first, |label, _| label.capture.clone())
        .unwrap();

    fixture.notify(fixture.first);
    fixture.present();

    // the previous render's listener was released, and its capture dropped
    assert_eq!(Rc::strong_count(&first_capture), 3);

    assert!(fixture.activate(0));
    assert!(fixture.activate(1));
    assert_eq!(fixture.label(fixture.first), (2, 1));
    assert_eq!(fixture.label(fixture.second), (1, 1));
}

#[test]
fn entities_that_leave_the_frame_release_their_listeners() {
    let mut fixture = fixture(true);
    let second_capture = fixture
        .runtime
        .update(fixture.second, |label, _| label.capture.clone())
        .unwrap();

    fixture
        .runtime
        .update(fixture.shell, |shell, cx| {
            shell.show_second = false;
            cx.notify();
        })
        .unwrap();
    fixture.present();

    // only the label and this test hold the capture
    assert_eq!(Rc::strong_count(&second_capture), 2);
    assert_eq!(
        fixture
            .runtime
            .frame()
            .entity_node(fixture.second.entity_id()),
        None
    );

    // mounted again, it renders from scratch
    fixture
        .runtime
        .update(fixture.shell, |shell, cx| {
            shell.show_second = true;
            cx.notify();
        })
        .unwrap();
    fixture.present();

    assert_eq!(fixture.label(fixture.second), (2, 0));
    assert!(fixture.activate(1));
    assert_eq!(fixture.label(fixture.second), (2, 1));
}

/// registers more listeners per render than half its runtime's callback slots
struct Busy {
    renders: u32,
}

impl Render for Busy {
    fn render<'a>(&'a mut self, cx: &mut Context<'_, Self>) -> impl IntoElement + 'a {
        self.renders += 1;

        for _ in 0..3 {
            let _ = cx.listener(|_: &mut Busy, _: &ActivateEvent, _| {});
        }
        let activate = cx.listener(|_: &mut Busy, _: &ActivateEvent, _| {});

        div().id("busy").on_activate(activate)
    }
}

/// enough static nodes that re-rendering [`Busy`] detaches few of them
struct BusyShell {
    busy: Entity<Busy>,
    renders: u32,
}

impl Render for BusyShell {
    fn render<'a>(&'a mut self, _: &mut Context<'_, Self>) -> impl IntoElement + 'a {
        self.renders += 1;

        div().child(self.busy).children((0..64).map(|_| div()))
    }
}

#[test]
fn rendering_an_entity_again_reuses_its_released_callback_slots() {
    // fixed storage whatever the features, with eight callback slots
    let mut runtime =
        Runtime::<FixedStorage<8192, 16, 1024, 8, 256, 1024, 32, 64, 2>, KeyedText>::default();
    let busy = runtime.create(|_| Busy { renders: 0 }).unwrap();
    let shell = runtime
        .create_root(|_| BusyShell { busy, renders: 0 })
        .unwrap();
    runtime.set_partial_rebuilds(true);
    runtime.rebuild().unwrap();

    for _ in 0..20 {
        runtime.update(busy, |_, cx| cx.notify()).unwrap();
        runtime.take_invalidation();
        runtime.rebuild().unwrap();
    }

    // every rebuild stayed partial, rather than falling back once the slots ran out
    assert_eq!(runtime.update(shell, |shell, _| shell.renders).unwrap(), 1);
    assert_eq!(runtime.update(busy, |busy, _| busy.renders).unwrap(), 21);
}

#[test]
fn focus_in_a_clean_entity_survives_a_partial_rebuild() {
    let mut fixture = fixture(true);

    fixture.runtime.clear_focus();
    assert!(fixture.runtime.focus_next());
    assert!(fixture.runtime.focus_next());
    let focused = fixture.runtime.focused_target().unwrap();

    fixture.notify(fixture.first);
    fixture.present();

    assert_eq!(fixture.runtime.focused_target(), Some(focused));
    assert!(fixture.runtime.activate_focused().unwrap());
    assert_eq!(fixture.label(fixture.second), (1, 1));
}

#[test]
fn a_child_notified_by_its_parents_render_renders_again() {
    let mut fixture = fixture(true);

    fixture
        .runtime
        .update(fixture.shell, |shell, cx| {
            shell.touch_first = true;
            cx.notify();
        })
        .unwrap();
    fixture.present();

    assert_eq!(fixture.label(fixture.first), (2, 0));
    assert!(
        fixture
            .outline()
            .iter()
            .any(|line| line.ends_with("\"touched\""))
    );
    // the child rendered in this rebuild, so it isn't dirty again
    assert_eq!(fixture.runtime.invalidation(), Invalidation::None);
}

#[test]
fn global_changes_render_only_their_readers() {
    let mut fixture = fixture(true);

    fixture.runtime.set_global(ShellTheme).unwrap();
    fixture.present();

    assert_eq!(fixture.shell_renders(), 2);
    assert_eq!(fixture.label(fixture.first), (1, 0));

    // the labels read this one while mounting their badges
    fixture.runtime.set_global(LabelTheme).unwrap();
    fixture.present();

    assert_eq!(fixture.shell_renders(), 2);
    assert_eq!(fixture.label(fixture.first), (2, 0));
    assert_eq!(fixture.label(fixture.second), (2, 0));

    drop(fixture.runtime.global_mut::<ShellTheme>());
    fixture.present();

    assert_eq!(fixture.shell_renders(), 3);
    assert_eq!(fixture.label(fixture.first), (2, 0));
}

#[test]
fn globals_nobody_reads_need_no_rebuild() {
    struct Unread;

    impl Global for Unread {}

    let mut fixture = fixture(true);

    fixture
        .runtime
        .update(fixture.shell, |shell, cx| {
            shell.read_theme = false;
            cx.notify();
        })
        .unwrap();
    fixture.present();

    fixture.runtime.set_global(Unread).unwrap();
    fixture.runtime.set_global(ShellTheme).unwrap();

    assert_eq!(fixture.runtime.invalidation(), Invalidation::None);
}

#[test]
fn reading_an_entity_renders_again_when_it_changes() {
    let mut fixture = fixture(true);

    fixture
        .runtime
        .update(fixture.shell, |shell, cx| {
            shell.mirror_first = true;
            cx.notify();
        })
        .unwrap();
    fixture.present();
    assert_eq!(fixture.shell_renders(), 2);

    set_text(&fixture, fixture.first, "renamed");
    fixture.present();

    assert_eq!(fixture.shell_renders(), 3);
    assert_eq!(fixture.label(fixture.first), (2, 0));
    assert!(
        fixture
            .outline()
            .iter()
            .filter(|line| line.ends_with("\"renamed\""))
            .count()
            == 2
    );

    // a label the shell doesn't read renders alone
    set_text(&fixture, fixture.second, "other");
    fixture.present();

    assert_eq!(fixture.shell_renders(), 3);
}

#[test]
fn a_render_forgets_what_it_no_longer_reads() {
    let mut fixture = fixture(true);

    fixture
        .runtime
        .update(fixture.shell, |shell, cx| {
            shell.mirror_first = true;
            cx.notify();
        })
        .unwrap();
    fixture.present();
    fixture
        .runtime
        .update(fixture.shell, |shell, cx| {
            shell.mirror_first = false;
            cx.notify();
        })
        .unwrap();
    fixture.present();
    assert_eq!(fixture.shell_renders(), 3);

    set_text(&fixture, fixture.first, "renamed");
    fixture.present();

    assert_eq!(fixture.shell_renders(), 3);
}

#[test]
fn partial_rebuilds_reclaim_the_subtrees_they_replace() {
    let mut fixture = fixture(true);
    let frame = fixture.runtime.frame();
    let (nodes, text) = (frame.node_count(), frame.text_bytes_used());

    for _ in 0..50 {
        fixture.notify(fixture.first);
        fixture.present();
        fixture.notify(fixture.shell);
        fixture.present();
    }

    // no rebuild fell back to a full one, which would render the second label again
    assert_eq!(fixture.shell_renders(), 51);
    assert_eq!(fixture.label(fixture.second).0, 1);

    let frame = fixture.runtime.frame();
    assert_eq!((frame.node_count(), frame.text_bytes_used()), (nodes, text));

    // the moved event bindings still reach both labels
    assert!(fixture.activate(0));
    assert!(fixture.activate(1));
    assert_eq!(fixture.label(fixture.first), (51, 1));
    assert_eq!(fixture.label(fixture.second), (1, 1));
}

const WIDTH: i32 = 80;
const HEIGHT: i32 = 60;

struct Measurer;

impl TextMeasurer for Measurer {
    fn measure_text(
        &self,
        text: &str,
        _: ResolvedTextStyle,
        available_width: AvailableSpace,
    ) -> Size {
        let width = available_width.limit(px(4 * text.len() as i32));
        let height = if text.is_empty() { px(0) } else { px(10) };

        Size::new(width, height)
    }
}

/// paints each box and text run as a solid area, keyed by what it paints, and only
/// inside the damage
struct Raster {
    cells: Vec<u64>,
    damage: DamageRegion,
}

impl Raster {
    fn new() -> Self {
        Self {
            cells: vec![0; (WIDTH * HEIGHT) as usize],
            damage: DamageRegion::full(),
        }
    }

    fn fill(&mut self, bounds: Rect, clip: Option<Rect>, key: &str) {
        // an empty key clears to the background
        let value = key.bytes().fold(0, |hash: u64, byte| {
            (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3) | 1
        });

        for y in 0..HEIGHT {
            for x in 0..WIDTH {
                let point = Point::new(px(x), px(y));
                let damaged = self.damage.is_full()
                    || self.damage.rects().iter().any(|rect| rect.contains(point));

                if damaged && bounds.contains(point) && clip.is_none_or(|clip| clip.contains(point))
                {
                    self.cells[(y * WIDTH + x) as usize] = value;
                }
            }
        }
    }
}

impl Painter for Raster {
    type Error = Infallible;

    fn draw_box(
        &mut self,
        bounds: Rect,
        paint: BoxPaint,
        clip: Option<Rect>,
    ) -> Result<(), Infallible> {
        self.fill(bounds, clip, &format!("{paint:?}"));
        Ok(())
    }

    fn draw_canvas(
        &mut self,
        _: Rect,
        _: Option<Rect>,
        _: &mut dyn FnMut(Rect, &mut dyn CanvasPainter),
    ) -> Result<(), Infallible> {
        Ok(())
    }

    fn draw_glyph(
        &mut self,
        _: &GlyphBitmap<'_>,
        _: Point,
        _: Color,
        _: Rect,
    ) -> Result<(), Self::Error> {
        Ok(())
    }

    fn fill_rect(&mut self, rect: Rect, color: Color, clip: Rect) -> Result<(), Self::Error> {
        self.fill(rect, Some(clip), &format!("{color:?}"));
        Ok(())
    }

    fn draw_pixels(&mut self, _: Point, _: &[Color]) -> Result<(), Self::Error> {
        Ok(())
    }
}

/// paints each text as a solid area in a color keyed by the text
#[derive(Default)]
struct KeyedText;

impl PaintResources for KeyedText {
    fn paint_text<P: Painter>(
        &mut self,
        painter: &mut P,
        text: &str,
        bounds: Rect,
        _: ResolvedTextStyle,
        clip: Option<Rect>,
    ) -> Result<(), PaintError<P::Error>> {
        let mut hasher = DefaultHasher::new();
        text.hash(&mut hasher);
        let [r, g, b, ..] = hasher.finish().to_le_bytes();

        painter
            .fill_rect(bounds, Color::rgb(r, g, b), clip.unwrap_or(bounds))
            .map_err(PaintError::Painter)
    }

    fn paint_image<P: Painter>(
        &mut self,
        _: &mut P,
        _: ImageSource,
        _: Rect,
        _: ImagePaint,
        _: Option<Rect>,
    ) -> Result<(), PaintError<P::Error>> {
        Ok(())
    }
}

impl Fixture {
    fn layout(&mut self) {
        self.runtime
            .layout_with_measurer(Size::new(px(WIDTH), px(HEIGHT)), &Measurer)
            .unwrap();
    }

    /// clears the damage and paints it, as the presenters do
    fn paint(&mut self, raster: &mut Raster, damage: DamageRegion) {
        raster.damage = damage;
        raster.fill(
            Rect::new(Point::ZERO, Size::new(px(WIDTH), px(HEIGHT))),
            None,
            "",
        );
        self.runtime.paint_with_damage(damage, raster).unwrap();
    }

    /// takes the invalidation, rebuilds, lays out and paints the rebuild's damage
    fn present_damage(&mut self, raster: &mut Raster) -> DamageRegion {
        self.present();
        self.layout();

        let damage = self.runtime.rebuild_damage();
        self.paint(raster, damage);

        damage
    }

    /// paints the current frame from scratch
    fn repaint(&mut self) -> Raster {
        let mut raster = Raster::new();
        self.paint(&mut raster, DamageRegion::full());

        raster
    }
}

fn painted_fixture() -> (Fixture, Raster) {
    let mut fixture = fixture(true);
    fixture.layout();
    let raster = fixture.repaint();

    (fixture, raster)
}

fn set_text(fixture: &Fixture, label: Entity<Label>, text: &'static str) {
    fixture
        .runtime
        .update(label, |label, cx| {
            label.text = text;
            cx.notify();
        })
        .unwrap();
}

#[test]
fn partial_rebuild_damages_only_the_rerendered_entity() {
    let (mut fixture, mut raster) = painted_fixture();

    // the same width, since the shell sizes to its widest label
    set_text(&fixture, fixture.second, "latest");
    let damage = fixture.present_damage(&mut raster);

    assert!(!damage.is_full());
    // the second label sits below the first, which needs no paint
    assert_eq!(damage.partial_damage_bounds().y(), px(10));
    assert!(raster.cells == fixture.repaint().cells);
}

#[test]
fn rendering_one_entity_many_times_keeps_its_damage_partial() {
    let (mut fixture, mut raster) = painted_fixture();

    for text in ["latest", "newest"].into_iter().cycle().take(20) {
        set_text(&fixture, fixture.second, text);
        let damage = fixture.present_damage(&mut raster);

        assert!(!damage.is_full());
        assert_eq!(damage.partial_damage_bounds().y(), px(10));
    }

    assert!(raster.cells == fixture.repaint().cells);
}

#[test]
fn partial_damage_covers_what_a_removed_child_painted() {
    let (mut fixture, mut raster) = painted_fixture();

    fixture
        .runtime
        .update(fixture.shell, |shell, cx| {
            shell.show_second = false;
            cx.notify();
        })
        .unwrap();
    let damage = fixture.present_damage(&mut raster);

    assert!(!damage.is_full());
    assert!(raster.cells == fixture.repaint().cells);
}

#[test]
fn resizing_a_clean_parent_damages_everything() {
    let (mut fixture, mut raster) = painted_fixture();

    // the shell sizes to its widest label, so it grows with the second one
    set_text(&fixture, fixture.second, "much wider");
    let damage = fixture.present_damage(&mut raster);

    assert!(damage.is_full());
    assert!(raster.cells == fixture.repaint().cells);
}

#[test]
fn moving_a_clean_entity_damages_everything() {
    let (mut fixture, mut raster) = painted_fixture();

    // the first label loses its height, so the clean second label moves up
    set_text(&fixture, fixture.first, "");
    let damage = fixture.present_damage(&mut raster);

    assert!(damage.is_full());
    assert!(raster.cells == fixture.repaint().cells);
}

#[test]
fn partial_rebuild_without_a_layout_before_it_damages_everything() {
    let (mut fixture, mut raster) = painted_fixture();

    set_text(&fixture, fixture.second, "changed");
    fixture.present();
    set_text(&fixture, fixture.second, "again");
    let damage = fixture.present_damage(&mut raster);

    assert!(damage.is_full());
}

#[test]
fn full_rebuilds_damage_everything() {
    let mut fixture = fixture(false);
    fixture.layout();
    let mut raster = fixture.repaint();

    set_text(&fixture, fixture.second, "changed");

    assert!(fixture.present_damage(&mut raster).is_full());
}
