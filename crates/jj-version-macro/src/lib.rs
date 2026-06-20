use proc_macro::TokenStream;

#[proc_macro]
pub fn version(_input: TokenStream) -> TokenStream {
    "compile_error!(\"jj-version is not implemented yet\")"
        .parse()
        .expect("static compile_error token stream")
}
