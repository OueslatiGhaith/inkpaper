use std::str::FromStr;

use proc_macro2::TokenStream;
use quote::quote;

use super::expand_rsx;

#[test]
fn untyped_rust_value_remains_ambiguous_in_text_namespace() {
    let error = expand_rsx(quote! {
        <text class="text-{value}">
            "Hello"
        </text>
    })
    .unwrap_err();

    assert!(error.to_string().contains("ambiguous utility class"));
}

#[test]
fn rejects_layout_utility_on_text_element() {
    let error = expand_rsx(quote! {
        <text class="p-4">
            "Hello"
        </text>
    })
    .unwrap_err();

    assert!(error.to_string().contains("is not valid on <text>"));
}

#[test]
fn rejects_empty_text_element() {
    let error = expand_rsx(quote! {
        <text class="text-lg"></text>
    })
    .unwrap_err();

    assert!(error.to_string().contains("<text> requires content"));
}

#[test]
fn rejects_multiple_text_children() {
    let error = expand_rsx(quote! {
        <text>
            "Hello"
            {title}
        </text>
    })
    .unwrap_err();

    assert!(error.to_string().contains("exactly one content child"));
}

#[test]
fn expands_dynamic_image_dimensions() {
    let result = expand_rsx(quote! {
        <image
            source={cover}
            class="w-{length:width} h-{height}"
        />
    });

    assert!(result.is_ok());
}

#[test]
fn rejects_image_without_source() {
    let error = expand_rsx(quote! {
        <image class="w-24 h-32"/>
    })
    .unwrap_err();

    assert!(error.to_string().contains("requires a `source` attribute"));
}

#[test]
fn rejects_image_children() {
    let error = expand_rsx(quote! {
        <image source={cover}>
            <text>"Nope"</text>
        </image>
    })
    .unwrap_err();

    assert!(error.to_string().contains("cannot have children"));
}

#[test]
fn rejects_non_image_utility_on_image() {
    let error = expand_rsx(quote! {
        <image
            source={cover}
            class="p-4"
        />
    })
    .unwrap_err();

    assert!(error.to_string().contains("unknown <image> utility class"));
}

#[test]
fn rejects_unsupported_object_scale_down() {
    let error = expand_rsx(quote! {
        <image
            source={cover}
            class="object-scale-down"
        />
    })
    .unwrap_err();

    assert!(error.to_string().contains("unknown <image> utility class"));
}

#[test]
fn rejects_duplicate_component_props() {
    let error = expand_rsx(quote! {
        <TopBar
            clock={clock}
            clock={other_clock}
        />
    })
    .unwrap_err();

    assert!(
        error
            .to_string()
            .contains("duplicate component prop `clock`")
    );
}

#[test]
fn rejects_component_prop_without_value() {
    let error = expand_rsx(quote! {
        <TopBar clock />
    })
    .unwrap_err();

    assert!(error.to_string().contains("requires a Rust expression"));
}

#[test]
fn unwraps_trivial_braced_expression() {
    let expression: syn::Expr = syn::parse_quote! {
        { source }
    };

    let expression = super::attribute::unbrace_expr(&expression);

    assert!(matches!(expression, syn::Expr::Path(_)));
}

#[test]
fn preserves_nontrivial_block_expression() {
    let expression: syn::Expr = syn::parse_quote! {
        {
            let value = make_value();
            value
        }
    };

    let expression = super::attribute::unbrace_expr(&expression);

    assert!(matches!(expression, syn::Expr::Block(_)));
}

fn rsx_tokens(source: &str) -> TokenStream {
    TokenStream::from_str(source).expect("valid token stream")
}

#[test]
fn expands_nested_if() {
    let result = expand_rsx(rsx_tokens(
        r#"
        {#if outer}
            <div>
                {#if inner}
                    <text>"Inner"</text>
                {:else}
                    <text>"Other"</text>
                {/if}
            </div>
        {:else}
            <text>"Outer fallback"</text>
        {/if}
        "#,
    ));

    assert!(result.is_ok());
}

#[test]
fn rejects_if_without_else() {
    let error = expand_rsx(rsx_tokens(
        r#"
        {#if visible}
            <text>"Visible"</text>
        {/if}
        "#,
    ))
    .unwrap_err();

    assert!(error.to_string().contains("requires a `{:else}` branch"));
}

#[test]
fn rejects_multiple_nodes_in_root_conditional_branch() {
    let error = expand_rsx(rsx_tokens(
        r#"
        {#if visible}
            <text>"One"</text>
            <text>"Two"</text>
        {:else}
            <text>"Fallback"</text>
        {/if}
        "#,
    ))
    .unwrap_err();

    assert!(
        error
            .to_string()
            .contains("accepts exactly one root element")
    );
}

#[test]
fn rejects_unclosed_if() {
    let error = expand_rsx(rsx_tokens(
        r#"
        {#if visible}
            <text>"Visible"</text>
        {:else}
            <text>"Hidden"</text>
        "#,
    ))
    .unwrap_err();

    assert!(error.to_string().contains("expected `{/if}`"));
}

#[test]
fn rejects_fragment_as_root() {
    let error = expand_rsx(quote! {
        <>
            <text>"First"</text>
            <text>"Second"</text>
        </>
    })
    .unwrap_err();

    assert!(error.to_string().contains("fragment cannot be the root"));
}

#[test]
fn expands_empty_conditional_branch() {
    let result = expand_rsx(rsx_tokens(
        r#"
        <div>
            {#if visible}
            {:else}
                <text>"Fallback"</text>
            {/if}
        </div>
        "#,
    ));

    assert!(result.is_ok());
}

#[test]
fn expands_each_inside_conditional() {
    let result = expand_rsx(rsx_tokens(
        r#"
        <div>
            {#if loaded}
                {#each books as book}
                    <BookRow book={book} />
                {/each}
            {:else}
                <text>"Loading"</text>
            {/if}
        </div>
        "#,
    ));

    assert!(result.is_ok());
}

#[test]
fn rejects_each_as_root() {
    let error = expand_rsx(rsx_tokens(
        r#"
        {#each books as book}
            <BookRow book={book} />
        {/each}
        "#,
    ))
    .unwrap_err();

    assert!(
        error
            .to_string()
            .contains("root `{#each}` is not supported")
    );
}

#[test]
fn rejects_each_without_as() {
    let error = expand_rsx(rsx_tokens(
        r#"
        <div>
            {#each books}
                <text>"Book"</text>
            {/each}
        </div>
        "#,
    ))
    .unwrap_err();

    assert!(error.to_string().contains("requires `as`"));
}

#[test]
fn rejects_unclosed_each() {
    let error = expand_rsx(rsx_tokens(
        r#"
        <div>
            {#each books as book}
                <BookRow book={book} />
        </div>
        "#,
    ))
    .unwrap_err();

    assert!(error.to_string().contains("expected `{/each}`"));
}

#[test]
fn rejects_focusable_without_id() {
    let error = expand_rsx(quote! {
        <div focusable />
    })
    .unwrap_err();

    assert!(
        error
            .to_string()
            .contains("`focusable` requires an `id` attribute")
    );
}

#[test]
fn rejects_activate_without_id() {
    let error = expand_rsx(rsx_tokens(
        r#"
        <div on:activate={listener} />
        "#,
    ))
    .unwrap_err();

    assert!(
        error
            .to_string()
            .contains("`on:activate` requires an `id` attribute")
    );
}

#[test]
fn rejects_interaction_variant_without_id() {
    let error = expand_rsx(quote! {
        <div class="focus:bg-zinc-100" />
    })
    .unwrap_err();

    assert!(
        error
            .to_string()
            .contains("interaction variants require an `id` attribute")
    );
}

#[test]
fn rejects_interaction_variant_on_noninteractive_element() {
    let error = expand_rsx(quote! {
        <div
            id="button"
            class="focus:bg-zinc-100"
        />
    })
    .unwrap_err();

    assert!(
        error
            .to_string()
            .contains("require `focusable` or `on:activate`")
    );
}

#[test]
fn rejects_interaction_variant_on_text() {
    let error = expand_rsx(quote! {
        <text
            id="title"
            focusable
            class="focus:text-black"
        >
            "Title"
        </text>
    })
    .unwrap_err();

    assert!(
        error
            .to_string()
            .contains("interaction variants are not supported on <text>")
    );
}

#[test]
fn rejects_multiple_roots_in_keyed_each() {
    let error = expand_rsx(rsx_tokens(
        r#"
        <div>
            {#each books as book (book.id)}
                <text>{book.title}</text>
                <text>{book.author}</text>
            {/each}
        </div>
        "#,
    ))
    .unwrap_err();

    assert!(
        error
            .to_string()
            .contains("keyed `{#each}` requires exactly one root element")
    );
}

#[test]
fn expands_custom_component_children_with_fragment() {
    let result = expand_rsx(quote! {
        <Panel>
            <>
                <text>"One"</text>
                <text>"Two"</text>
            </>
        </Panel>
    });

    assert!(result.is_ok());
}

#[test]
fn rejects_duplicate_typed_event_attributes() {
    let error = expand_rsx(quote! {
        <div
            id="book"
            on:BookSelectedEvent={first}
            on:BookSelectedEvent={second}
        />
    })
    .unwrap_err();

    assert!(
        error
            .to_string()
            .contains("duplicate `on:BookSelectedEvent` attribute")
    );
}

#[test]
fn rejects_typed_event_without_identity() {
    let error = expand_rsx(quote! {
        <div on:BookSelectedEvent={selected} />
    })
    .unwrap_err();

    assert!(
        error
            .to_string()
            .contains("`on:BookSelectedEvent` requires an `id` attribute")
    );
}

#[test]
fn rejects_unknown_lowercase_event() {
    let error = expand_rsx(quote! {
        <div
            id="book"
            on:focus={listener}
        />
    })
    .unwrap_err();

    assert!(error.to_string().contains("unknown built-in event `focus`"));
}

#[test]
fn expands_svg() {
    let result = expand_rsx(quote! {
        <svg
            source={settings_icon}
            class="w-[32px] h-[32px] text-black"
        />
    });

    assert!(result.is_ok());
}

#[test]
fn rejects_svg_without_source() {
    let error = expand_rsx(quote! {
        <svg class="w-[32px] h-[32px]" />
    })
    .unwrap_err();

    assert!(
        error
            .to_string()
            .contains("<svg> requires a `source` attribute")
    );
}

#[test]
fn rejects_svg_children() {
    let error = expand_rsx(quote! {
        <svg source={settings_icon}>
            <text>"nope"</text>
        </svg>
    })
    .unwrap_err();

    assert!(error.to_string().contains("<svg> cannot have children"));
}

#[test]
fn rejects_image_utility_on_svg() {
    let error = expand_rsx(quote! {
        <svg
            source={settings_icon}
            class="grayscale"
        />
    })
    .unwrap_err();

    assert!(
        error
            .to_string()
            .contains("utility class `grayscale` is not valid on <svg>")
    );
}
