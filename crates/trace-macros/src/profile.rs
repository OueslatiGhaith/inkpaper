use proc_macro2::TokenStream;
use quote::quote;
use syn::{
    Expr, Ident, ImplItemFn, ItemFn, Path, Token,
    parse::{Parse, ParseStream},
};

pub(crate) fn expand_profile(args: TokenStream, input: TokenStream) -> syn::Result<TokenStream> {
    let args = syn::parse2::<ProfileArgs>(args)?;

    if let Ok(mut function) = syn::parse2::<ItemFn>(input.clone()) {
        let instrumentation = instrumentation(&args);
        let original = function.block;

        function.block = Box::new(syn::parse_quote!({
            #instrumentation
            #original
        }));

        return Ok(quote! { #function });
    }

    if let Ok(mut function) = syn::parse2::<ImplItemFn>(input.clone()) {
        let instrumentation = instrumentation(&args);
        let original = function.block;

        function.block = syn::parse_quote!({
            #instrumentation
            #original
        });

        return Ok(quote! { #function });
    }

    Err(syn::Error::new_spanned(
        input,
        "`profile` can only be applied to functions and methods",
    ))
}

fn instrumentation(args: &ProfileArgs) -> TokenStream {
    let event = &args.event;

    match &args.arg {
        Some(arg) => quote! { ::inkpaper_trace::profile_scope!(#event, arg = #arg); },
        None => quote! { ::inkpaper_trace::profile_scope!(#event); },
    }
}

struct ProfileArgs {
    event: Path,
    arg: Option<Expr>,
}

impl Parse for ProfileArgs {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let event = input.parse::<Path>()?;

        if input.is_empty() {
            return Ok(Self { event, arg: None });
        }

        input.parse::<Token![,]>()?;

        if input.is_empty() {
            return Ok(Self { event, arg: None });
        }

        let name = input.parse::<Ident>()?;
        if name != "arg" {
            return Err(syn::Error::new(name.span(), "expected `arg = <expr>`"));
        }

        input.parse::<Token![=]>()?;

        let arg = input.parse::<Expr>()?;

        if !input.is_empty() {
            input.parse::<Token![,]>()?;

            if !input.is_empty() {
                return Err(input.error("unexpected tokens after profiling argument"));
            }
        }

        Ok(Self {
            event,
            arg: Some(arg),
        })
    }
}
