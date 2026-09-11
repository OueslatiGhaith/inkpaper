use proc_macro::TokenStream;

mod rsx;

#[proc_macro]
pub fn rsx(input: TokenStream) -> TokenStream {
    match rsx::expand_rsx(input.into()) {
        Ok(tokens) => tokens.into(),
        Err(error) => error.into_compile_error().into(),
    }
}
