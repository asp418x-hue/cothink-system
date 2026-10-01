use proc_macro2::TokenStream;
use quote::quote;
use syn::{Attribute, ImplItem, ItemEnum, Token};

use crate::args::StubGenMode;
use crate::paths::pyo3_stub_gen_path;

pub(crate) fn is_pyo3_related(attr: &Attribute) -> bool {
    is_pyo3_container_attr(attr) || is_pyo3_field_attr(attr)
}

pub(crate) fn is_sentinel(attr: &Attribute) -> bool {
    attr.path().is_ident("py_only")
        || attr.path().is_ident("py_attrs")
        || attr.path().is_ident("rust_only")
}

pub(crate) fn is_gen_stub(attr: &Attribute) -> bool {
    attr.path().is_ident("gen_stub")
}

pub(crate) fn is_pyo3_method_attr(attr: &Attribute) -> bool {
    if is_pyo3_signature_attr(attr) {
        return true;
    }

    attr.path()
        .segments
        .first()
        .map(|s| {
            matches!(
                s.ident.to_string().as_str(),
                "new"
                    | "getter"
                    | "setter"
                    | "staticmethod"
                    | "classmethod"
                    | "classattr"
                    | "pyo3_raw"
                    | "wrap_pyfunction"
                    | "args"
                    | "name"
                    | "text_signature"
            )
        })
        .unwrap_or(false)
}

pub(crate) fn is_pyo3_container_attr(attr: &Attribute) -> bool {
    attr.path()
        .segments
        .first()
        .map(|s| {
            matches!(
                s.ident.to_string().as_str(),
                "pyclass" | "pymethods" | "pyfunction" | "pymodule"
            )
        })
        .unwrap_or(false)
}

pub(crate) fn is_pyo3_field_attr(attr: &Attribute) -> bool {
    attr.path()
        .segments
        .first()
        .map(|s| s.ident == "pyo3")
        .unwrap_or(false)
}

pub(crate) fn is_pyo3_signature_attr(attr: &Attribute) -> bool {
    attr.path()
        .segments
        .first()
        .map(|s| {
            matches!(
                s.ident.to_string().as_str(),
                "pyo3" | "args" | "name" | "text_signature"
            )
        })
        .unwrap_or(false)
}

pub(crate) fn strip_pyo3_from_signature(sig: &mut syn::Signature) {
    for input in &mut sig.inputs {
        if let syn::FnArg::Typed(arg) = input {
            arg.attrs.retain(|a| !is_pyo3_related(a));
        }
    }
}

pub(crate) fn strip_gen_stub_from_signature(sig: &mut syn::Signature) {
    for input in &mut sig.inputs {
        if let syn::FnArg::Typed(arg) = input {
            arg.attrs.retain(|a| !is_gen_stub(a));
        }
    }
}

pub(crate) fn strip_gen_stub_from_item(item: &mut ImplItem) {
    match item {
        ImplItem::Fn(f) => {
            f.attrs.retain(|a| !is_gen_stub(a));
            strip_gen_stub_from_signature(&mut f.sig);
        }
        ImplItem::Const(c) => c.attrs.retain(|a| !is_gen_stub(a)),
        ImplItem::Type(t) => t.attrs.retain(|a| !is_gen_stub(a)),
        ImplItem::Macro(m) => m.attrs.retain(|a| !is_gen_stub(a)),
        _ => {}
    }
}

pub(crate) fn strip_gen_stub_from_fields(fields: &mut syn::Fields) {
    let iter: Box<dyn Iterator<Item = &mut syn::Field>> = match fields {
        syn::Fields::Named(f) => Box::new(f.named.iter_mut()),
        syn::Fields::Unnamed(f) => Box::new(f.unnamed.iter_mut()),
        syn::Fields::Unit => return,
    };
    for field in iter {
        field.attrs.retain(|a| !is_gen_stub(a));
    }
}

pub(crate) fn strip_gen_stub_from_variants(
    variants: &mut syn::punctuated::Punctuated<syn::Variant, Token![,]>,
) {
    for variant in variants.iter_mut() {
        variant.attrs.retain(|a| !is_gen_stub(a));
        strip_gen_stub_from_fields(&mut variant.fields);
    }
}

pub(crate) fn impl_item_attrs(item: &ImplItem) -> &[Attribute] {
    match item {
        ImplItem::Fn(f) => &f.attrs,
        ImplItem::Const(c) => &c.attrs,
        ImplItem::Type(t) => &t.attrs,
        ImplItem::Macro(m) => &m.attrs,
        _ => &[],
    }
}

pub(crate) fn strip_python_attrs_from_impl_item(item: &mut ImplItem) {
    match item {
        ImplItem::Fn(f) => {
            f.attrs.retain(|a| {
                !is_sentinel(a) && !is_gen_stub(a) && !is_pyo3_related(a) && !is_pyo3_method_attr(a)
            });
            strip_pyo3_from_signature(&mut f.sig);
            strip_gen_stub_from_signature(&mut f.sig);
        }
        ImplItem::Const(c) => c.attrs.retain(|a| {
            !is_sentinel(a) && !is_gen_stub(a) && !is_pyo3_related(a) && !is_pyo3_method_attr(a)
        }),
        ImplItem::Type(t) => t
            .attrs
            .retain(|a| !is_sentinel(a) && !is_gen_stub(a) && !is_pyo3_related(a)),
        ImplItem::Macro(m) => m
            .attrs
            .retain(|a| !is_sentinel(a) && !is_gen_stub(a) && !is_pyo3_related(a)),
        _ => {}
    }
}

pub(crate) fn strip_sentinels(item: &mut ImplItem) {
    match item {
        ImplItem::Fn(f) => f.attrs.retain(|a| !is_sentinel(a)),
        ImplItem::Const(c) => c.attrs.retain(|a| !is_sentinel(a)),
        ImplItem::Type(t) => t.attrs.retain(|a| !is_sentinel(a)),
        ImplItem::Macro(m) => m.attrs.retain(|a| !is_sentinel(a)),
        _ => {}
    }
}

pub(crate) fn strip_pyo3_from_fields(fields: &mut syn::Fields) {
    let iter: Box<dyn Iterator<Item = &mut syn::Field>> = match fields {
        syn::Fields::Named(f) => Box::new(f.named.iter_mut()),
        syn::Fields::Unnamed(f) => Box::new(f.unnamed.iter_mut()),
        syn::Fields::Unit => return,
    };
    for field in iter {
        field.attrs.retain(|a| !is_pyo3_related(a));
    }
}

pub(crate) fn strip_pyo3_from_variants(
    variants: &mut syn::punctuated::Punctuated<syn::Variant, Token![,]>,
) {
    for variant in variants.iter_mut() {
        variant.attrs.retain(|a| !is_pyo3_related(a));
        strip_pyo3_from_fields(&mut variant.fields);
    }
}

pub(crate) fn is_simple_enum(item: &ItemEnum) -> bool {
    item.variants
        .iter()
        .all(|v| matches!(v.fields, syn::Fields::Unit))
}

pub(crate) enum StubKind {
    Struct,
    SimpleEnum,
    ComplexEnum,
    Methods,
    Function,
}

pub(crate) fn stub_attr(mode: &StubGenMode, kind: StubKind) -> TokenStream {
    let StubGenMode::Feature(feature) = mode else {
        return quote! {};
    };

    let stub = pyo3_stub_gen_path();
    match kind {
        StubKind::Struct => quote! {
            #[allow(unexpected_cfgs)]
            #[cfg_attr(feature = #feature, #stub::derive::gen_stub_pyclass)]
        },
        StubKind::SimpleEnum => quote! {
            #[allow(unexpected_cfgs)]
            #[cfg_attr(feature = #feature, #stub::derive::gen_stub_pyclass_enum)]
        },
        StubKind::ComplexEnum => quote! {
            #[allow(unexpected_cfgs)]
            #[cfg_attr(feature = #feature, #stub::derive::gen_stub_pyclass_complex_enum)]
        },
        StubKind::Methods => quote! {
            #[allow(unexpected_cfgs)]
            #[cfg_attr(feature = #feature, #stub::derive::gen_stub_pymethods)]
        },
        StubKind::Function => quote! {
            #[allow(unexpected_cfgs)]
            #[cfg_attr(feature = #feature, #stub::derive::gen_stub_pyfunction)]
        },
    }
}
