use proc_macro::TokenStream;

mod instrument;

/// Instruments a synchronous function with an InkPaper trace span.
///
/// The default span name is the function or method name, and the
/// default target is `module_path!()`.
///
/// ```
/// #[inkpaper_trace::instrument(
///     target = "reader.pagination",
///     fields(chapter = chapter),
/// )]
/// fn paginate(chapter: u32) {
///     let _ = chapter;
/// }
///
/// paginate(3);
/// ```
///
/// Async functions are deliberately unsupported until the trace
/// runtime has an async span model.
///
/// ```compile_fail
/// #[inkpaper_trace::instrument]
/// async fn present() {}
/// ```
///
/// Instrumentation supports at most two explicit fields.
///
/// ```compile_fail
/// #[inkpaper_trace::instrument(
///     fields(
///         first = 1u32,
///         second = 2u32,
///         third = 3u32,
///     ),
/// )]
/// fn work() {}
/// ```
#[proc_macro_attribute]
pub fn instrument(args: TokenStream, input: TokenStream) -> TokenStream {
    match instrument::expand_instrument(args.into(), input.into()) {
        Ok(tokens) => tokens.into(),
        Err(error) => error.into_compile_error().into(),
    }
}
