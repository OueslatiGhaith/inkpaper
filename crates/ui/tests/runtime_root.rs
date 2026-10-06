use inkpaper_ui::prelude::*;

type TestRuntime = Runtime<FixedStorage<1024, 4, 1024, 8, 32, 256, 16>>;

struct TestApp {
    show_extra: bool,
}

impl Render for TestApp {
    fn render<'a>(&'a mut self, _: &mut Context<'_, Self>) -> impl IntoElement + 'a {
        div()
            .child("root")
            .when(self.show_extra, |root| root.child("extra"))
    }
}

#[test]
fn rebuild_without_root_returns_error() {
    let mut runtime = TestRuntime::default();

    assert_eq!(runtime.rebuild(), Err(FrameBuildError::RootNotSet),);
}

#[test]
fn create_root_registers_entity_for_rebuild() {
    let mut runtime = TestRuntime::default();

    runtime
        .create_root(|_| TestApp { show_extra: false })
        .unwrap();

    assert_eq!(runtime.invalidation(), Invalidation::Rebuild,);

    runtime.rebuild().unwrap();

    assert!(runtime.frame_node_count() > 0,);

    assert_eq!(runtime.invalidation(), Invalidation::None,);
}

#[test]
fn rebuild_reuses_registered_root_entity() {
    let mut runtime = TestRuntime::default();

    let app = runtime
        .create_root(|_| TestApp { show_extra: false })
        .unwrap();

    runtime.rebuild().unwrap();

    let original_nodes = runtime.frame_node_count();

    runtime
        .update(app, |app, cx| {
            app.show_extra = true;
            cx.notify();
        })
        .unwrap();

    assert_eq!(runtime.invalidation(), Invalidation::Rebuild,);

    runtime.rebuild().unwrap();

    assert_eq!(runtime.frame_node_count(), original_nodes + 1,);
}
