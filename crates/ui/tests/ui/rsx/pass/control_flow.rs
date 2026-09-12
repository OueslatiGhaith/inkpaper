#![deny(unused_braces)]

use inkpaper_ui::prelude::*;

fn assert_into_element(element: impl IntoElement) {
    let _ = element;
}

fn main() {
    let expanded = true;
    let show_badge = false;

    let tree = rsx! {
        <div class="flex flex-col gap-2">
            <>
                <text>"Header"</text>
                <text>"Subtitle"</text>
            </>

            {#if expanded}
                <text>"Details"</text>
                <text>"Metadata"</text>
            {:else}
                <text>"Collapsed"</text>
            {/if}

            {#if show_badge}
                <text>"New"</text>
            {/if}

            <text>"Footer"</text>
        </div>
    };

    assert_into_element(tree);
}
