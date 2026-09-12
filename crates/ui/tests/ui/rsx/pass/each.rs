#![deny(unused_braces)]

use inkpaper_ui::prelude::*;

fn assert_into_element(element: impl IntoElement) {
    let _ = element;
}

fn main() {
    let books = ["Dune", "Neuromancer", "Foundation"];

    let tree = rsx! {
        <div class="flex flex-col gap-2">
            <text>"Library"</text>

            {#each books as book, index}
                <text>{book}</text>

                {#if index == 0}
                    <text>"First"</text>
                {/if}
            {:else}
                <text>"No books"</text>
            {/each}

            <text>"End"</text>
        </div>
    };

    assert_into_element(tree);
}
