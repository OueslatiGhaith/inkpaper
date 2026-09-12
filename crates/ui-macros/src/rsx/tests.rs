use std::str::FromStr;

use proc_macro2::TokenStream;
use quote::quote;

use super::expand_rsx;

#[test]
fn expands_static_text_element() {
    let result = expand_rsx(quote! {
        <text class="text-lg text-zinc-700">
            "Hello"
        </text>
    });

    assert!(result.is_ok());
}

#[test]
fn expands_dynamic_text_element() {
    let result = expand_rsx(quote! {
        <text class="text-lg">
            {title}
        </text>
    });

    assert!(result.is_ok());
}

#[test]
fn expands_text_nested_inside_div() {
    let result = expand_rsx(quote! {
        <div class="flex flex-col gap-2">
            <text class="text-lg">
                "Title"
            </text>

            <text class="text-sm text-zinc-500">
                {subtitle}
            </text>
        </div>
    });

    assert!(result.is_ok());
}

#[test]
fn typed_color_value_resolves_text_namespace() {
    let result = expand_rsx(quote! {
        <text class="text-{color:theme.ink}">
            "Hello"
        </text>
    });

    assert!(result.is_ok());
}

#[test]
fn typed_length_value_resolves_text_namespace() {
    let result = expand_rsx(quote! {
        <text class="text-{length:font_size}">
            "Hello"
        </text>
    });

    assert!(result.is_ok());
}

#[test]
fn typed_color_value_resolves_border_namespace() {
    let result = expand_rsx(quote! {
        <div class="border-2 border-{color:theme.border}"/>
    });

    assert!(result.is_ok());
}

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
fn expands_image_element() {
    let result = expand_rsx(quote! {
        <image
            source={cover}
            class="w-24 h-32 object-cover"
        />
    });

    assert!(result.is_ok());
}

#[test]
fn expands_image_inside_div() {
    let result = expand_rsx(quote! {
        <div class="flex">
            <image
                source={cover}
                class="w-24 h-32 object-contain"
            />
        </div>
    });

    assert!(result.is_ok());
}

#[test]
fn expands_image_filter_utilities() {
    let result = expand_rsx(quote! {
        <image
            source={cover}
            class="grayscale invert"
        />
    });

    assert!(result.is_ok());
}

#[test]
fn expands_arbitrary_image_dimensions() {
    let result = expand_rsx(quote! {
        <image
            source={cover}
            class="w-[110px] h-[160px]"
        />
    });

    assert!(result.is_ok());
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
fn expands_custom_component() {
    let result = expand_rsx(quote! {
        <TopBar
            clock={clock}
            battery={battery}
        />
    });

    assert!(result.is_ok());
}

#[test]
fn expands_custom_component_inside_div() {
    let result = expand_rsx(quote! {
        <div class="flex flex-col">
            <TopBar
                clock={clock}
                battery={battery}
            />
        </div>
    });

    assert!(result.is_ok());
}

#[test]
fn expands_custom_component_with_no_props() {
    let result = expand_rsx(quote! {
        <LoadingIndicator />
    });

    assert!(result.is_ok());
}

#[test]
fn rejects_custom_component_children_for_now() {
    let error = expand_rsx(quote! {
        <Panel>
            <text>"Hello"</text>
        </Panel>
    })
    .unwrap_err();

    assert!(error.to_string().contains("children are not supported yet"));
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
fn expands_if_else() {
    let result = expand_rsx(rsx_tokens(
        r#"
        {#if visible}
            <text>"Visible"</text>
        {:else}
            <text>"Hidden"</text>
        {/if}
        "#,
    ));

    assert!(result.is_ok());
}

#[test]
fn expands_else_if() {
    let result = expand_rsx(rsx_tokens(
        r#"
        {#if primary}
            <text>"Primary"</text>
        {:else if secondary}
            <text>"Secondary"</text>
        {:else}
            <text>"Fallback"</text>
        {/if}
        "#,
    ));

    assert!(result.is_ok());
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
fn rejects_multiple_nodes_in_branch() {
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

    assert!(error.to_string().contains("exactly one renderable child"));
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
