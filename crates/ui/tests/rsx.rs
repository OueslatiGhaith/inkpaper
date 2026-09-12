#![deny(unused_braces)]

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

#[test]
fn rsx_supports_static_text_elements() {
    let tree = rsx! {
        <text class="text-lg text-zinc-700">
            "Hello"
        </text>
    };

    assert_into_element(tree);
}

#[test]
fn rsx_supports_dynamic_text_elements() {
    let title = "Continue reading";

    let tree = rsx! {
        <text class="text-lg text-zinc-700">
            {title}
        </text>
    };

    assert_into_element(tree);
}

#[test]
fn rsx_supports_text_elements_inside_divs() {
    let subtitle = "Chapter 4";

    let tree = rsx! {
        <div class="flex flex-col gap-2">
            <text class="text-lg text-black">
                "Continue reading"
            </text>

            <text class="text-sm text-zinc-500">
                {subtitle}
            </text>
        </div>
    };

    assert_into_element(tree);
}

#[test]
fn rsx_supports_dynamic_text_colors() {
    let ink = Color::rgb(32, 32, 32);

    let tree = rsx! {
        <text class="text-lg text-{color:ink}">
            "Hello"
        </text>
    };

    assert_into_element(tree);
}

#[test]
fn rsx_supports_dynamic_font_sizes() {
    let font_size = px(18);

    let tree = rsx! {
        <text class="text-{length:font_size}">
            "Hello"
        </text>
    };

    assert_into_element(tree);
}

#[test]
fn rsx_supports_dynamic_border_colors() {
    let border_color = Color::rgb(128, 128, 128);

    let tree = rsx! {
        <div class="border-2 border-{color:border_color}" />
    };

    assert_into_element(tree);
}

#[test]
fn rsx_supports_image_elements() {
    let source = ImageSource::new(ImageId::new(1), Size::new(px(120), px(180)));

    let tree = rsx! {
        <image
            source={source}
            class="w-24 h-32 object-cover"
        />
    };

    assert_into_element(tree);
}

#[test]
fn rsx_supports_nested_image_elements() {
    let source = ImageSource::new(ImageId::new(1), Size::new(px(120), px(180)));

    let tree = rsx! {
        <div class="flex flex-col gap-2">
            <image
                source={source}
                class="w-24 h-32 object-cover"
            />

            <text class="text-sm text-zinc-500">
                "Book cover"
            </text>
        </div>
    };

    assert_into_element(tree);
}

#[test]
fn rsx_supports_arbitrary_image_dimensions() {
    let source = ImageSource::new(ImageId::new(1), Size::new(px(120), px(180)));

    let tree = rsx! {
        <image
            source={source}
            class="w-[110px] h-[160px] object-contain"
        />
    };

    assert_into_element(tree);
}

#[test]
fn rsx_supports_dynamic_image_dimensions() {
    let source = ImageSource::new(ImageId::new(1), Size::new(px(120), px(180)));

    let width = px(96);
    let height = px(128);

    let tree = rsx! {
        <image
            source={source}
            class="w-{length:width} h-{length:height} object-cover"
        />
    };

    assert_into_element(tree);
}

#[test]
fn rsx_supports_image_filters() {
    let source = ImageSource::new(ImageId::new(1), Size::new(px(120), px(180)));

    let tree = rsx! {
        <image
            source={source}
            class="w-24 h-32 object-cover grayscale invert"
        />
    };

    assert_into_element(tree);
}

struct TestCard<'a> {
    title: &'a str,
    width: Pixels,
}

struct TestCardProps<'a> {
    title: &'a str,
    width: Pixels,
}

impl<'a> From<TestCardProps<'a>> for TestCard<'a> {
    fn from(props: TestCardProps<'a>) -> Self {
        Self {
            title: props.title,
            width: props.width,
        }
    }
}

impl RenderOnce for TestCard<'_> {
    fn render(self, _: &AppContext<'_>) -> impl IntoElement {
        div().w(self.width).child(self.title)
    }
}

#[test]
fn rsx_supports_custom_components() {
    let title = "Continue reading";
    let width = px(240);

    let tree = rsx! {
        <TestCard
            title={title}
            width={width}
        />
    };

    assert_into_element(tree);
}

#[test]
fn rsx_supports_custom_components_as_children() {
    let title = "Continue reading";

    let tree = rsx! {
        <div class="flex flex-col gap-4">
            <TestCard
                title={title}
                width={px(240)}
            />

            <text class="text-sm text-zinc-500">
                "Recent"
            </text>
        </div>
    };

    assert_into_element(tree);
}

#[test]
fn rsx_supports_conditionals() {
    let downloaded = false;
    let downloading = true;

    let tree = rsx! {
        {#if downloaded}
            <text>"Read"</text>
        {:else if downloading}
            <text>"Downloading..."</text>
        {:else}
            <text>"Download"</text>
        {/if}
    };

    assert_into_element(tree);
}

#[test]
fn rsx_supports_conditional_children() {
    let available = true;

    let tree = rsx! {
        <div class="flex flex-col gap-2">
            {#if available}
                <text>"Available"</text>
            {:else}
                <text>"Unavailable"</text>
            {/if}
        </div>
    };

    assert_into_element(tree);
}

#[test]
fn rsx_supports_fragments() {
    let tree = rsx! {
        <div class="flex flex-col gap-2">
            <>
                <text>"First"</text>
                <text>"Second"</text>
            </>
        </div>
    };

    assert_into_element(tree);
}

#[test]
fn rsx_supports_nested_fragments() {
    let tree = rsx! {
        <div class="flex flex-col gap-2">
            <>
                <text>"First"</text>

                <>
                    <text>"Second"</text>
                    <text>"Third"</text>
                </>
            </>
        </div>
    };

    assert_into_element(tree);
}

#[test]
fn rsx_supports_multiple_conditional_children() {
    let available = true;

    let tree = rsx! {
        <div class="flex flex-col gap-2">
            <text>"Before"</text>

            {#if available}
                <text>"Available"</text>
                <text>"Ready"</text>
            {:else}
                <text>"Unavailable"</text>
                <text>"Try again later"</text>
            {/if}

            <text>"After"</text>
        </div>
    };

    assert_into_element(tree);
}

#[test]
fn rsx_supports_conditional_without_else() {
    let show_badge = true;

    let tree = rsx! {
        <div class="flex flex-col gap-2">
            <text>"Book"</text>

            {#if show_badge}
                <text>"New"</text>
            {/if}

            <text>"Footer"</text>
        </div>
    };

    assert_into_element(tree);
}

#[test]
fn rsx_supports_fragments_inside_conditionals() {
    let expanded = true;

    let tree = rsx! {
        <div class="flex flex-col gap-2">
            {#if expanded}
                <>
                    <text>"Details"</text>
                    <text>"Metadata"</text>
                </>
            {:else}
                <text>"Collapsed"</text>
            {/if}
        </div>
    };

    assert_into_element(tree);
}
