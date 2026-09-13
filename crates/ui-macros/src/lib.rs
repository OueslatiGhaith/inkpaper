use proc_macro::TokenStream;
use proc_macro2::Span;

mod component;
mod rsx;

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
