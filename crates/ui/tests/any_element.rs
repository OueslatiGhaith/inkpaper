#![cfg(feature = "alloc")]

use inkpaper_ui::prelude::*;

type TestRuntime = Runtime<
    4096, // entity bytes
    1,    // entity slots
    4096, // callback bytes
    16,   // callback slots
    64,   // frame nodes
    1024, // frame text bytes
    32,   // element states
>;

struct Static {
    detail: bool,
}

impl Render for Static {
    fn render<'a>(&'a mut self, _: &mut Context<'_, Self>) -> impl IntoElement + 'a {
        let content = if self.detail {
            Either::Left(div().child("Title").child("Detail"))
        } else {
            Either::Right(text("Title"))
        };

        div().child(content)
    }
}

struct Erased {
    detail: bool,
}

impl Render for Erased {
    fn render<'a>(&'a mut self, _: &mut Context<'_, Self>) -> impl IntoElement + 'a {
        let content = if self.detail {
            div().child("Title").child("Detail").into_any_element()
        } else {
            text("Title").into_any_element()
        };

        div().child(content)
    }
}

fn frame_shape<T: Render + 'static>(root: T) -> (usize, usize) {
    let mut runtime = TestRuntime::default();

    runtime.create_root(|_| root).unwrap();
    runtime.rebuild().unwrap();

    (runtime.frame_node_count(), runtime.frame_text_bytes_used())
}

#[test]
fn any_element_mounts_like_the_element_it_wraps() {
    assert_eq!(
        frame_shape(Erased { detail: false }),
        frame_shape(Static { detail: false }),
    );
    assert_eq!(
        frame_shape(Erased { detail: true }),
        frame_shape(Static { detail: true }),
    );
}
