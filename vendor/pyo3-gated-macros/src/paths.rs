use proc_macro_crate::{FoundCrate, crate_name};
use proc_macro2::{Ident, Span, TokenStream};
use quote::quote;

pub(crate) fn facade_crate_ident() -> Ident {
    match crate_name("pyo3-gated") {
        Ok(FoundCrate::Name(name)) => Ident::new(&name, Span::call_site()),
        Ok(FoundCrate::Itself) | Err(_) => Ident::new("pyo3_gated", Span::call_site()),
    }
}

pub(crate) fn facade_crate_path() -> TokenStream {
    match crate_name("pyo3-gated") {
        Ok(FoundCrate::Itself) => quote!(crate),
        Ok(FoundCrate::Name(name)) => {
            let ident = Ident::new(&name, Span::call_site());
            quote!(::#ident)
        }
        Err(_) => quote!(::pyo3_gated),
    }
}

pub(crate) fn facade_crate_path_string() -> String {
    match crate_name("pyo3-gated") {
        Ok(FoundCrate::Itself) => "crate".to_string(),
        Ok(FoundCrate::Name(name)) => format!("::{name}"),
        Err(_) => "::pyo3_gated".to_string(),
    }
}

pub(crate) fn pyo3_stub_gen_path() -> TokenStream {
    let facade = facade_crate_path();
    quote!(#facade::__private::pyo3_stub_gen)
}

pub(crate) fn pyo3_crate_path() -> TokenStream {
    let facade = facade_crate_path();
    quote!(#facade::__private::pyo3)
}

pub(crate) fn pyo3_crate_attr_path() -> String {
    let facade = facade_crate_path_string();
    format!("{facade}::__private::pyo3")
}
