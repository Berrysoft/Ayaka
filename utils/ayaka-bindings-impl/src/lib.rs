use proc_macro::TokenStream;
use proc_macro_crate::{crate_name, FoundCrate};
use quote::{quote, TokenStreamExt};
use syn::{
    parse_macro_input, parse_str, AttributeArgs, FnArg, ForeignItem, Ident, ItemFn, ItemForeignMod,
    Lit, NestedMeta,
};

#[proc_macro_attribute]
pub fn export(_attr: TokenStream, input: TokenStream) -> TokenStream {
    let func = input.clone();
    let func = parse_macro_input!(func as ItemFn);
    let name = func.sig.ident;
    let name_str = name.to_string();
    let expname = parse_str::<Ident>(&format!("__export_{}", name_str)).unwrap();
    let input = proc_macro2::TokenStream::from(input);
    let export_func = quote! {
        #[doc(hidden)]
        #[allow(unsafe_code)]
        #[export_name = #name_str]
        unsafe extern "C" fn #expname(len: usize, data: *const u8) -> u64 {
            ::ayaka_bindings::__export(len, data, #name)
        }
        #input
    };
    TokenStream::from(export_func)
}

#[proc_macro_attribute]
pub fn import(attr: TokenStream, input: TokenStream) -> TokenStream {
    let attr = parse_macro_input!(attr as AttributeArgs);
    let mut module = String::new();
    for a in attr {
        match a {
            NestedMeta::Lit(Lit::Str(s)) => module = s.value(),
            _ => unimplemented!(),
        }
    }
    let input = parse_macro_input!(input as ItemForeignMod);
    let mut imports = quote! {};
    for func in input.items {
        match func {
            ForeignItem::Fn(func) => {
                let attrs = func.attrs;
                let vis = func.vis;
                let sig = func.sig;

                let params = sig.inputs.clone();
                let params = params
                    .into_iter()
                    .map(|arg| match arg {
                        FnArg::Typed(p) => p.pat,
                        _ => unimplemented!(),
                    })
                    .collect::<Vec<_>>();

                let name = sig.ident.clone();
                let name_str = name.to_string();
                let impname = parse_str::<Ident>(&format!("__import_{}", name_str)).unwrap();
                let bindings_crate_name = match crate_name("ayaka-bindings").unwrap() {
                    FoundCrate::Itself => quote!(crate),
                    FoundCrate::Name(name) => quote!(::#name),
                };
                let import_func = quote! {
                    #[doc(hidden)]
                    #[link(wasm_import_module = #module)]
                    extern "C" {
                        #[link_name = #name_str]
                        fn #impname(len: usize, data: *const u8) -> u64;
                    }
                    #[allow(unsafe_code)]
                    #(#attrs)* #vis #sig {
                        unsafe { #bindings_crate_name::__import(#impname, (#(#params,)*)) }
                    }
                };
                imports.append_all(import_func);
            }
            _ => unimplemented!(),
        }
    }
    TokenStream::from(imports)
}
