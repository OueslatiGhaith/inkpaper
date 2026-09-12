use inkpaper_ui_style_schema::tailwind::{ValueKind, border, color, radius, spacing, typography};
use proc_macro2::{Literal, Span, TokenStream};
use quote::quote;
use syn::{Expr, Ident};

use crate::rsx::spec::{ArgumentKind, UtilityReceiver, UtilitySpec};

use super::{ClassValue, color::oklch_to_srgb, parse_class_value};

pub(super) fn emit_no_argument_utility(
    receiver: TokenStream,
    spec: &UtilitySpec,
    span: Span,
) -> TokenStream {
    let method = Ident::new(spec.rust_name, span);

    match spec.receiver {
        UtilityReceiver::Styled => quote! { ::inkpaper_ui::Styled::#method(#receiver) },
        UtilityReceiver::TextStyled => quote! { ::inkpaper_ui::TextStyled::#method(#receiver)},
        UtilityReceiver::Image => quote! { ::inkpaper_ui::Image::#method(#receiver) },
    }
}

pub(super) fn emit_argument_utility(
    receiver: TokenStream,
    spec: &UtilitySpec,
    class_name: &str,
    value: &str,
    span: Span,
) -> syn::Result<TokenStream> {
    if spec.argument_types.len() != 1 {
        return Err(syn::Error::new(
            span,
            format!(
                "utility class `{class_name}` cannot be represented by class syntax because it takes {} arguments",
                spec.argument_types.len(),
            ),
        ));
    }

    match parse_class_value(value, span)? {
        ClassValue::Rust { hint, expression } => {
            if let Some(hint) = hint
                && !hint.accepts(spec.value_kind)
            {
                return Err(syn::Error::new(
                    span,
                    format!(
                        "`{}` Rust value does not apply to `{}`",
                        hint.name(),
                        spec.rust_name,
                    ),
                ));
            }

            emit_rust_value(receiver, spec, expression, span)
        }
        ClassValue::Arbitrary(value) => {
            emit_arbitrary_value(receiver, spec, class_name, value, span)
        }
        ClassValue::Tailwind(value) => emit_tailwind_value(receiver, spec, class_name, value, span),
    }
}

fn emit_rust_value(
    receiver: TokenStream,
    spec: &UtilitySpec,
    expression: &str,
    span: Span,
) -> syn::Result<TokenStream> {
    let expression = syn::parse_str::<Expr>(expression).map_err(|error| {
        syn::Error::new(
            span,
            format!("invalid Rust expression in class value: {error}"),
        )
    })?;

    Ok(emit_utility_call(
        receiver,
        spec,
        quote! { #expression },
        span,
    ))
}

fn emit_arbitrary_value(
    receiver: TokenStream,
    spec: &UtilitySpec,
    class_name: &str,
    value: &str,
    span: Span,
) -> syn::Result<TokenStream> {
    if spec.value_kind == Some(ValueKind::Color) {
        let rgb = color::parse_hex(value).ok_or_else(|| {
            syn::Error::new(
                span,
                format!("arbitrary color `{class_name}-[{value}]` must be `#rgb` or `#rrggbb`"),
            )
        })?;

        return Ok(emit_color_utility(receiver, spec, rgb, span));
    }

    match spec.argument_kind() {
        ArgumentKind::Pixels => {
            let value = parse_arbitrary_pixels(class_name, value, span)?;
            let value = Literal::i32_unsuffixed(value);

            Ok(emit_utility_call(
                receiver,
                spec,
                quote! { ::inkpaper_ui::px(#value) },
                span,
            ))
        }
        ArgumentKind::U16 => {
            let value = value.parse::<u16>().map_err(|_| {
                syn::Error::new(span, format!("utility class `{class_name}` requires a u16"))
            })?;

            let value = Literal::u16_unsuffixed(value);

            Ok(emit_utility_call(receiver, spec, quote! { #value }, span))
        }
        ArgumentKind::Unsupported => Err(syn::Error::new(
            span,
            format!("arbitrary values are not supported for `{class_name}`"),
        )),
        ArgumentKind::None => {
            unreachable!("argument utility unexpectedly had no arguments")
        }
    }
}

fn emit_tailwind_value(
    receiver: TokenStream,
    spec: &UtilitySpec,
    class_name: &str,
    value: &str,
    span: Span,
) -> syn::Result<TokenStream> {
    match spec.value_kind {
        Some(ValueKind::Spacing) => {
            let pixels = spacing::resolve_pixels(value).ok_or_else(|| {
                syn::Error::new(
                    span,
                    format!("`{class_name}-{value}` cannot be represented"),
                )
            })?;

            emit_pixel_utility(receiver, spec, pixels, span)
        }
        Some(ValueKind::BorderWidth) => {
            let pixels = border::resolve_pixels(value).ok_or_else(|| {
                syn::Error::new(span, format!("invalid border width `{class_name}-{value}`"))
            })?;

            emit_pixel_utility(receiver, spec, pixels, span)
        }
        Some(ValueKind::Radius) => {
            let pixels = radius::resolve_pixels(value).ok_or_else(|| {
                syn::Error::new(span, format!("unknown radius `{class_name}-{value}`"))
            })?;

            emit_pixel_utility(receiver, spec, pixels, span)
        }
        Some(ValueKind::Integer) => {
            let value = value.parse::<u16>().map_err(|_| {
                syn::Error::new(span, format!("`{class_name}-{value}` requires a u16"))
            })?;

            let value = Literal::u16_unsuffixed(value);

            Ok(emit_utility_call(receiver, spec, quote! { #value }, span))
        }
        Some(ValueKind::FontSize) => {
            let font_size = typography::font_size::resolve(value).ok_or_else(|| {
                syn::Error::new(span, format!("unknown font size `{class_name}-{value}`"))
            })?;

            Ok(emit_font_size_utility(receiver, spec, font_size, span))
        }
        Some(ValueKind::LineHeight) => {
            let pixels = typography::line_height::resolve_pixels(value).ok_or_else(|| {
                syn::Error::new(
                    span,
                    format!(
                        "`{class_name}-{value}` cannot be represented as an integer pixel line height"
                    ),
                )
            })?;

            emit_pixel_utility(receiver, spec, pixels, span)
        }
        Some(ValueKind::Color) => {
            let value = color::resolve(value).ok_or_else(|| {
                syn::Error::new(span, format!("unknown color `{class_name}-{value}`"))
            })?;

            let rgb = oklch_to_srgb(value);

            Ok(emit_color_utility(receiver, spec, rgb, span))
        }
        None => match spec.argument_kind() {
            ArgumentKind::U16 => {
                let value = value.parse::<u16>().map_err(|_| {
                    syn::Error::new(
                        span,
                        format!("utility class `{class_name}` requires a u16 value"),
                    )
                })?;

                let value = Literal::u16_unsuffixed(value);

                Ok(emit_utility_call(receiver, spec, quote! { #value }, span))
            }
            _ => Err(syn::Error::new(
                span,
                format!("utility `{class_name}` has no value scale"),
            )),
        },
    }
}

fn emit_pixel_utility(
    receiver: TokenStream,
    spec: &UtilitySpec,
    pixels: i32,
    span: Span,
) -> syn::Result<TokenStream> {
    let pixels = Literal::i32_unsuffixed(pixels);

    Ok(emit_utility_call(
        receiver,
        spec,
        quote! { ::inkpaper_ui::px(#pixels) },
        span,
    ))
}

fn emit_color_utility(
    receiver: TokenStream,
    spec: &UtilitySpec,
    rgb: color::Rgb,
    span: Span,
) -> TokenStream {
    let red = Literal::u8_unsuffixed(rgb.red);
    let green = Literal::u8_unsuffixed(rgb.green);
    let blue = Literal::u8_unsuffixed(rgb.blue);

    emit_utility_call(
        receiver,
        spec,
        quote! { ::inkpaper_ui::Color::rgb(#red, #green, #blue) },
        span,
    )
}

fn emit_font_size_utility(
    receiver: TokenStream,
    spec: &UtilitySpec,
    font_size: typography::FontSize,
    span: Span,
) -> TokenStream {
    let size = Literal::i32_unsuffixed(font_size.size_px);
    let line_height = Literal::i32_unsuffixed(font_size.line_height_px);

    let receiver = emit_utility_call(receiver, spec, quote! { ::inkpaper_ui::px(#size) }, span);

    quote! { ::inkpaper_ui::TextStyled::line_height(#receiver, ::inkpaper_ui::px(#line_height)) }
}

fn emit_utility_call(
    receiver: TokenStream,
    spec: &UtilitySpec,
    argument: TokenStream,
    span: Span,
) -> TokenStream {
    let method = Ident::new(spec.rust_name, span);

    match spec.receiver {
        UtilityReceiver::Styled => quote! { ::inkpaper_ui::Styled::#method(#receiver, #argument) },
        UtilityReceiver::TextStyled => {
            quote! { ::inkpaper_ui::TextStyled::#method(#receiver, #argument) }
        }
        UtilityReceiver::Image => quote! { ::inkpaper_ui::Image::#method(#receiver, #argument) },
    }
}

fn parse_arbitrary_pixels(class_name: &str, value: &str, span: Span) -> syn::Result<i32> {
    let Some(value) = value.strip_suffix("px") else {
        return Err(syn::Error::new(
            span,
            format!("arbitrary `{class_name}` values require the `px` unit"),
        ));
    };

    value
        .parse::<i32>()
        .map_err(|_| syn::Error::new(span, format!("invalid arbitrary pixel value `{value}px`")))
}
