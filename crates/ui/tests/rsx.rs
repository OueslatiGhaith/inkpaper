use inkpaper_ui::prelude::*;

fn assert_into_element(element: impl IntoElement) {
    let _ = element;
}

#[test]
fn rsx_builds_nested_divs() {
    let tree = rsx! {
        <div class="flex flex-col gap-2 p-3">
            <div class="w-full h-6"/>
            <div class="flex-1"/>
        </div>
    };

    assert_into_element(tree);
}

#[test]
fn rsx_accepts_rust_expression_children() {
    let child = text("Hello");

    let tree = rsx! {
        <div class="w-full p-3">
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
fn rsx_supports_tailwind_spacing() {
    let tree = rsx! {
        <div class="flex flex-col items-center justify-between w-full min-w-10 max-w-300 p-4 px-3 gap-2 top-1 rounded-lg border-2 "/>
    };

    assert_into_element(tree);
}

#[test]
fn rsx_supports_arbitrary_pixel_values() {
    let tree = rsx! {
        <div class="w-[240px] h-[160px] p-[10px] gap-[6px] rounded-[7px] border-[3px]" />
    };

    assert_into_element(tree);
}

#[test]
fn rsx_supports_rust_values_inside_classes() {
    let padding = px(10);
    let width = px(240);
    let background = Color::rgb(240, 240, 240);

    let tree = rsx! {
        <div class="w-{width} p-{padding} bg-{background}" />
    };

    assert_into_element(tree);
}

#[test]
fn rsx_rust_values_may_contain_whitespace() {
    let tree = rsx! {
        <div class="
            p-{if true { px(10) } else { px(20) }}
            bg-{Color::rgb(240, 240, 240)}
        "/>
    };

    assert_into_element(tree);
}

#[test]
fn rsx_supports_tailwind_typography_defaults() {
    let tree = rsx! {
        <div class="text-lg leading-6 text-center wrap">
            {text("Hello")}
        </div>
    };

    assert_into_element(tree);
}

#[test]
fn rsx_supports_tailwind_utility_aliases() {
    let tree = rsx! {
        <div class="basis-4 basis-auto grow-2 shrink-1 "/>
    };

    assert_into_element(tree);
}

#[test]
fn rsx_supports_tailwind_background_colors() {
    let tree = rsx! {
        <div class="
            bg-red-500
            rounded-lg
        "/>
    };

    assert_into_element(tree);
}

#[test]
fn rsx_supports_tailwind_neutral_colors() {
    let tree = rsx! {
        <div class="bg-zinc-100">
            <div class="bg-slate-500"/>
            <div class="bg-neutral-900"/>
            <div class="bg-mauve-500"/>
            <div class="bg-olive-500"/>
            <div class="bg-mist-500"/>
            <div class="bg-taupe-500"/>
        </div>
    };

    assert_into_element(tree);
}

#[test]
fn rsx_supports_tailwind_black_and_white() {
    let tree = rsx! {
        <div class="bg-white">
            <div class="bg-black"/>
        </div>
    };

    assert_into_element(tree);
}

#[test]
fn rsx_supports_tailwind_text_colors() {
    let tree = rsx! {
        <div class="
            text-lg
            text-red-500
        ">
            {text("Hello")}
        </div>
    };

    assert_into_element(tree);
}

#[test]
fn rsx_supports_tailwind_border_colors() {
    let tree = rsx! {
        <div class="border-2 border-blue-500" />
    };

    assert_into_element(tree);
}

#[test]
fn rsx_supports_arbitrary_hex_colors() {
    let tree = rsx! {
        <div class="bg-[#f03] border-2 border-[#1a2b3c] text-[#abcdef]">
            {text("Hello")}
        </div>
    };

    assert_into_element(tree);
}

#[test]
fn rsx_distinguishes_overloaded_tailwind_namespaces() {
    let tree = rsx! {
        <div class="border-2 border-red-500 text-lg text-zinc-700">
            {text("Hello")}
        </div>
    };

    assert_into_element(tree);
}
