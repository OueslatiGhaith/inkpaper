use proc_macro2::{Span, TokenStream};
use quote::quote;
use syn::{
    Block, Expr, Ident, ImplItemFn, ItemFn, LitStr, Signature, Token, parenthesized,
    parse::{Parse, ParseStream},
    spanned::Spanned,
};

pub(crate) fn expand_instrument(args: TokenStream, input: TokenStream) -> syn::Result<TokenStream> {
    let args = syn::parse2::<InstrumentArgs>(args)?;

    if let Ok(mut function) = syn::parse2::<ItemFn>(input.clone()) {
        validate_signature(&function.sig)?;

        let original = *function.block;

        *function.block = instrument_block(&function.sig, &args, original);

        return Ok(quote! { #function });
    }

    if let Ok(mut function) = syn::parse2::<ImplItemFn>(input.clone()) {
        validate_signature(&function.sig)?;

        let original = function.block;

        function.block = instrument_block(&function.sig, &args, original);

        return Ok(quote! { #function });
    }

    Err(syn::Error::new_spanned(
        input,
        "`#[instrument]` can only be applied to functions and methods",
    ))
}

fn validate_signature(signature: &Signature) -> syn::Result<()> {
    if let Some(asyncness) = signature.asyncness {
        return Err(syn::Error::new(
            asyncness.span(),
            concat!(
                "`#[instrument]` currently supports synchronous functions only; ",
                "async functions require async trace spans",
            ),
        ));
    }

    if let Some(constness) = signature.constness {
        return Err(syn::Error::new(
            constness.span(),
            "`#[instrument]` cannot be applied to `const fn`",
        ));
    }

    Ok(())
}

fn instrument_block(signature: &Signature, args: &InstrumentArgs, original: Block) -> Block {
    let name = args
        .name
        .clone()
        .unwrap_or_else(|| LitStr::new(&signature.ident.to_string(), signature.ident.span()));

    let target = match &args.target {
        Some(target) => quote! { #target },
        None => quote! { module_path!() },
    };

    let fields = args.fields.as_deref().unwrap_or(&[]);

    let span = match fields {
        [] => quote! { ::inkpaper_trace::span!(target: #target, #name) },
        [field] => {
            let field_name = &field.name;

            let value = &field.value;

            quote! {
                ::inkpaper_trace::span!(
                    target: #target,
                    #name,
                    #field_name = #value,
                )
            }
        }

        [first, second] => {
            let first_name = &first.name;
            let first_value = &first.value;

            let second_name = &second.name;
            let second_value = &second.value;

            quote! {
                ::inkpaper_trace::span!(
                    target: #target,
                    #name,
                    #first_name = #first_value,
                    #second_name = #second_value,
                )
            }
        }

        _ => {
            unreachable!("InstrumentArgs rejects more than two fields")
        }
    };

    let guard = Ident::new("__inkpaper_trace_guard", Span::mixed_site());

    syn::parse_quote!({
        let #guard = #span;

        #original
    })
}

#[derive(Default)]
struct InstrumentArgs {
    target: Option<LitStr>,
    name: Option<LitStr>,
    fields: Option<Vec<FieldArg>>,
}

struct FieldArg {
    name: Ident,
    value: Expr,
}

impl Parse for InstrumentArgs {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut args = Self::default();

        while !input.is_empty() {
            let key = input.parse::<Ident>()?;

            match key.to_string().as_str() {
                "target" => {
                    if args.target.is_some() {
                        return Err(syn::Error::new(key.span(), "duplicate `target` argument"));
                    }

                    input.parse::<Token![=]>()?;

                    args.target = Some(input.parse::<LitStr>()?);
                }

                "name" => {
                    if args.name.is_some() {
                        return Err(syn::Error::new(key.span(), "duplicate `name` argument"));
                    }

                    input.parse::<Token![=]>()?;

                    args.name = Some(input.parse::<LitStr>()?);
                }

                "fields" => {
                    if args.fields.is_some() {
                        return Err(syn::Error::new(key.span(), "duplicate `fields` argument"));
                    }

                    let content;

                    parenthesized!(
                        content in input
                    );

                    let mut fields = Vec::new();

                    while !content.is_empty() {
                        let name = content.parse::<Ident>()?;

                        if fields.iter().any(|field: &FieldArg| field.name == name) {
                            return Err(syn::Error::new(
                                name.span(),
                                format!("duplicate trace field `{name}`"),
                            ));
                        }

                        if fields.len() == 2 {
                            return Err(syn::Error::new(
                                name.span(),
                                "`#[instrument]` supports at most two fields",
                            ));
                        }

                        content.parse::<Token![=]>()?;

                        let value = content.parse::<Expr>()?;

                        fields.push(FieldArg { name, value });

                        if content.is_empty() {
                            break;
                        }

                        content.parse::<Token![,]>()?;
                    }

                    args.fields = Some(fields);
                }

                _ => {
                    return Err(syn::Error::new(
                        key.span(),
                        concat!(
                            "unknown `#[instrument]` argument; ",
                            "expected `target`, `name`, or `fields`",
                        ),
                    ));
                }
            }

            if input.is_empty() {
                break;
            }

            input.parse::<Token![,]>()?;
        }

        Ok(args)
    }
}

#[cfg(test)]
mod tests {
    use quote::quote;

    use super::expand_instrument;

    #[test]
    fn instruments_free_function() {
        let expanded = expand_instrument(
            quote! {
                target = "ui.render",
                name = "render",
                fields(
                    frame = frame_id,
                    full = full,
                )
            },
            quote! {
                fn render(
                    frame_id: u32,
                    full: bool,
                ) -> u32 {
                    frame_id
                }
            },
        )
        .unwrap()
        .to_string();

        assert!(expanded.contains("\"ui.render\""));

        assert!(expanded.contains("\"render\""));

        assert!(expanded.contains("frame = frame_id"));

        assert!(expanded.contains("full = full"));
    }

    #[test]
    fn instruments_method() {
        let expanded = expand_instrument(
            quote! {
                target =
                    "reader.pagination",
                fields(
                    chapter =
                        chapter_index,
                )
            },
            quote! {
                fn paginate(
                    &mut self,
                    chapter_index: u32,
                ) {
                    self.finish();
                }
            },
        )
        .unwrap()
        .to_string();

        assert!(expanded.contains("\"reader.pagination\""));

        assert!(expanded.contains("\"paginate\""));

        assert!(expanded.contains("chapter = chapter_index"));
    }

    #[test]
    fn rejects_async_function() {
        let error = expand_instrument(
            quote! {},
            quote! {
                async fn present() {}
            },
        )
        .unwrap_err();

        assert_eq!(
            error.to_string(),
            concat!(
                "`#[instrument]` currently supports synchronous functions only; ",
                "async functions require async trace spans",
            ),
        );
    }

    #[test]
    fn rejects_const_function() {
        let error = expand_instrument(
            quote! {},
            quote! {
                const fn calculate() {}
            },
        )
        .unwrap_err();

        assert_eq!(
            error.to_string(),
            "`#[instrument]` cannot be applied to `const fn`",
        );
    }

    #[test]
    fn rejects_more_than_two_fields() {
        let error = expand_instrument(
            quote! {
                fields(
                    first = 1,
                    second = 2,
                    third = 3,
                )
            },
            quote! {
                fn work() {}
            },
        )
        .unwrap_err();

        assert_eq!(
            error.to_string(),
            "`#[instrument]` supports at most two fields",
        );
    }

    #[test]
    fn rejects_duplicate_fields() {
        let error = expand_instrument(
            quote! {
                fields(
                    value = 1,
                    value = 2,
                )
            },
            quote! {
                fn work() {}
            },
        )
        .unwrap_err();

        assert_eq!(error.to_string(), "duplicate trace field `value`");
    }

    #[test]
    fn rejects_unknown_argument() {
        let error = expand_instrument(
            quote! {
                level = "debug"
            },
            quote! {
                fn work() {}
            },
        )
        .unwrap_err();

        assert_eq!(
            error.to_string(),
            concat!(
                "unknown `#[instrument]` argument; ",
                "expected `target`, `name`, or `fields`",
            ),
        );
    }
}
