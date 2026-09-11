use inkpaper_ui::prelude::*;

fn assert_into_element(element: impl IntoElement) {
    let _ = element;
}

#[test]
fn rsx_builds_nested_divs() {
    let tree = rsx! {
        <div class="flex flex-col gap-8 p-12">
            <div class="w-full h-24"/>
            <div class="flex-1"/>
        </div>
    };

    assert_into_element(tree);
}

#[test]
fn rsx_accepts_rust_expression_children() {
    let child = text("Hello");

    let tree = rsx! {
        <div class="w-full p-12">
            {child}
        </div>
    };

    assert_into_element(tree);
}

#[test]
fn rsx_accepts_complex_expression_children() {
    let enabled = true;

    let tree = rsx! {
        <div class="w-full">
            {
                div()
                    .p(px(8))
                    .when(enabled, |element| element.flex())
            }
        </div>
    };

    assert_into_element(tree);
}

#[test]
fn rsx_uses_schema_generated_layout_classes() {
    let tree = rsx! {
        <div class="relative flex flex-col items-center justify-between flex-1 w-full h-full min-w-10 max-w-300 min-h-20 max-h-400 p-12 px-14 py-16 pt-1 pr-2 pb-3 pl-4 m-10 mx-12 my-14 mt-5 mr-6 mb-7 ml-8 gap-18 border-2 rounded-8 overflow-hidden top-1 right-2 bottom-3 left-4">
            <div class="absolute flex flex-row items-start justify-start w-100 h-50"/>
            <div class="items-end justify-end"/>
            <div class="justify-center"/>
        </div>
    };

    assert_into_element(tree);
}

#[test]
fn rsx_uses_schema_generated_text_classes() {
    let tree = rsx! {
        <div class="font-size-14 line-height-18 wrap text-center max-lines-2 text-ellipsis">
            {text("Hello")}
        </div>
    };

    assert_into_element(tree);
}
