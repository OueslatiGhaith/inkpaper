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
