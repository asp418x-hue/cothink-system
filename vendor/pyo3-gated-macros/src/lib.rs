//! Internal proc-macro implementation for `pyo3-gated`.

use proc_macro::TokenStream;
use quote::quote;
use syn::{Item, ItemEnum, ItemImpl, ItemStruct, parse_macro_input};

mod args;
mod attrs;
mod expand;
mod paths;

use args::{MacroArgs, ModuleArgs};

#[proc_macro]
#[doc(hidden)]
pub fn __pyo3_gated_stub_gen_alias(_input: TokenStream) -> TokenStream {
    let facade = paths::facade_crate_ident();
    quote! {
        extern crate #facade as pyo3;
        extern crate #facade as pyo3_stub_gen;
    }
    .into()
}

#[proc_macro_attribute]
pub fn py_compat_struct(attr: TokenStream, input: TokenStream) -> TokenStream {
    let args = parse_macro_input!(attr as MacroArgs);
    let input_struct = parse_macro_input!(input as ItemStruct);
    expand::expand_struct(args, input_struct).into()
}

#[proc_macro_attribute]
pub fn py_compat(attr: TokenStream, input: TokenStream) -> TokenStream {
    let args = parse_macro_input!(attr as MacroArgs);
    let item = parse_macro_input!(input as Item);
    expand::expand_dispatch(args, item).into()
}

#[proc_macro_attribute]
pub fn py_compat_enum(attr: TokenStream, input: TokenStream) -> TokenStream {
    let args = parse_macro_input!(attr as MacroArgs);
    let input_enum = parse_macro_input!(input as ItemEnum);
    expand::expand_enum(args, input_enum).into()
}

#[proc_macro_attribute]
pub fn py_compat_methods(attr: TokenStream, input: TokenStream) -> TokenStream {
    let args = parse_macro_input!(attr as MacroArgs);
    let input_impl = parse_macro_input!(input as ItemImpl);
    expand::expand_methods(args, input_impl).into()
}

#[proc_macro_attribute]
pub fn py_compat_fn(attr: TokenStream, input: TokenStream) -> TokenStream {
    let args = parse_macro_input!(attr as MacroArgs);
    let input_fn = parse_macro_input!(input as syn::ItemFn);
    expand::expand_fn(args, input_fn).into()
}

#[proc_macro]
pub fn define_py_module(input: TokenStream) -> TokenStream {
    let args = parse_macro_input!(input as ModuleArgs);
    expand::expand_module(args).into()
}
