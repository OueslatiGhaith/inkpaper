use proc_macro::TokenStream;
use proc_macro2::Span;
use syn::LitStr;

mod component;
mod rsx;
mod svg;

#[proc_macro]
pub fn rsx(input: TokenStream) -> TokenStream {
    match rsx::expand_rsx(input.into()) {
        Ok(tokens) => tokens.into(),
        Err(error) => error.into_compile_error().into(),
    }
}

#[proc_macro_attribute]
pub fn component(args: TokenStream, input: TokenStream) -> TokenStream {
    if !args.is_empty() {
        return syn::Error::new(Span::call_site(), "`component` does not accept arguments")
            .into_compile_error()
            .into();
    }

    match component::expand_component(input.into()) {
        Ok(tokens) => tokens.into(),
        Err(error) => error.into_compile_error().into(),
    }
}

#[proc_macro]
pub fn include_svg(input: TokenStream) -> TokenStream {
    let path = match syn::parse::<LitStr>(input) {
        Ok(path) => path,
        Err(error) => return error.into_compile_error().into(),
    };

    match svg::expand_include_svg(path) {
        Ok(tokens) => tokens.into(),
        Err(error) => error.into_compile_error().into(),
    }
}
