use proc_macro2::TokenStream;
use quote::quote;
use syn::{Item, ItemEnum, ItemImpl, ItemStruct};

use crate::args::{MacroArgs, ModuleArgs};
use crate::attrs::{
    StubKind, impl_item_attrs, is_gen_stub, is_pyo3_related, is_simple_enum,
    strip_gen_stub_from_fields, strip_gen_stub_from_item, strip_gen_stub_from_signature,
    strip_gen_stub_from_variants, strip_pyo3_from_fields, strip_pyo3_from_signature,
    strip_pyo3_from_variants, strip_python_attrs_from_impl_item, strip_sentinels, stub_attr,
};

pub(crate) fn expand_struct(args: MacroArgs, input_struct: ItemStruct) -> TokenStream {
    if let Some(error) = args.reject_fn_only_args() {
        return error.to_compile_error();
    }

    let feature = &args.feature;
    let pyo3 = args.pyo3_path();
    let mut py_struct = input_struct.clone();
    let mut plain_struct = input_struct;

    plain_struct
        .attrs
        .retain(|a| !is_pyo3_related(a) && !is_gen_stub(a));
    strip_pyo3_from_fields(&mut plain_struct.fields);
    strip_gen_stub_from_fields(&mut plain_struct.fields);

    if args.should_strip_stub_gen() {
        py_struct.attrs.retain(|a| !is_gen_stub(a));
        strip_gen_stub_from_fields(&mut py_struct.fields);
    }

    let stub = stub_attr(&args.stub_gen, StubKind::Struct);
    let pyclass_inner = args.pyclass_args.as_ref().map_or_else(
        || attr_args(args.pyo3_crate_attr(), None),
        |a| attr_args(args.pyo3_crate_attr(), Some(a)),
    );

    quote! {
        #stub
        #[allow(unexpected_cfgs)]
        #[cfg(feature = #feature)]
        #[#pyo3::pyclass #pyclass_inner]
        #py_struct

        #[allow(unexpected_cfgs)]
        #[cfg(not(feature = #feature))]
        #plain_struct
    }
}

pub(crate) fn expand_enum(args: MacroArgs, input_enum: ItemEnum) -> TokenStream {
    if let Some(error) = args.reject_fn_only_args() {
        return error.to_compile_error();
    }

    let feature = &args.feature;
    let pyo3 = args.pyo3_path();
    let stub_kind = if is_simple_enum(&input_enum) {
        StubKind::SimpleEnum
    } else {
        StubKind::ComplexEnum
    };

    let mut py_enum = input_enum.clone();
    let mut plain_enum = input_enum;

    plain_enum
        .attrs
        .retain(|a| !is_pyo3_related(a) && !is_gen_stub(a));
    strip_pyo3_from_variants(&mut plain_enum.variants);
    strip_gen_stub_from_variants(&mut plain_enum.variants);

    if args.should_strip_stub_gen() {
        py_enum.attrs.retain(|a| !is_gen_stub(a));
        strip_gen_stub_from_variants(&mut py_enum.variants);
    }

    let stub = stub_attr(&args.stub_gen, stub_kind);
    let pyclass_inner = args.pyclass_args.as_ref().map_or_else(
        || attr_args(args.pyo3_crate_attr(), None),
        |a| attr_args(args.pyo3_crate_attr(), Some(a)),
    );

    quote! {
        #stub
        #[allow(unexpected_cfgs)]
        #[cfg(feature = #feature)]
        #[#pyo3::pyclass #pyclass_inner]
        #py_enum

        #[allow(unexpected_cfgs)]
        #[cfg(not(feature = #feature))]
        #plain_enum
    }
}

pub(crate) fn expand_methods(args: MacroArgs, input_impl: ItemImpl) -> TokenStream {
    if let Some(error) = args.reject_fn_only_args() {
        return error.to_compile_error();
    }
    if args.pyclass_args.is_some() {
        return syn::Error::new(
            args.pyclass_args_span
                .unwrap_or_else(proc_macro2::Span::call_site),
            "`pyclass_args` applies only to structs/enums",
        )
        .to_compile_error();
    }

    if input_impl.trait_.is_some() {
        return syn::Error::new_spanned(
            input_impl.impl_token,
            "`#[py_compat_methods]` only supports inherent `impl Type { ... }` blocks; split Python methods into a separate inherent impl and keep trait impls as normal Rust impls",
        )
        .to_compile_error();
    }

    let feature = &args.feature;
    let pyo3 = args.pyo3_path();
    let self_ty = &input_impl.self_ty;
    let (impl_generics, _ty_generics, where_clause) = input_impl.generics.split_for_impl();

    let pass_through_attrs: Vec<_> = input_impl
        .attrs
        .iter()
        .filter(|a| !is_pyo3_related(a))
        .collect();

    let stub_gen_disabled = args.should_strip_stub_gen();
    let mut py_items = Vec::<TokenStream>::new();
    let mut plain_items = Vec::<TokenStream>::new();

    for item in &input_impl.items {
        let attrs = impl_item_attrs(item);
        let is_py_only = attrs.iter().any(|a| a.path().is_ident("py_only"));
        let is_py_attrs = attrs.iter().any(|a| a.path().is_ident("py_attrs"));
        let is_rust_only = attrs.iter().any(|a| a.path().is_ident("rust_only"));

        if is_py_only && is_py_attrs {
            return syn::Error::new_spanned(
                item,
                "`#[py_only]` and `#[py_attrs]` cannot both appear on the same item",
            )
            .to_compile_error();
        }
        if is_py_only && is_rust_only {
            return syn::Error::new_spanned(
                item,
                "`#[py_only]` and `#[rust_only]` cannot both appear on the same item",
            )
            .to_compile_error();
        }
        if is_py_attrs && is_rust_only {
            return syn::Error::new_spanned(
                item,
                "`#[py_attrs]` and `#[rust_only]` cannot both appear on the same item",
            )
            .to_compile_error();
        }
        if !matches!(item, syn::ImplItem::Fn(_)) && !is_rust_only {
            return syn::Error::new_spanned(
                item,
                "`#[py_compat_methods]` can only expose functions to PyO3; mark associated consts/types/macros as `#[rust_only]` or move them outside the PyO3 impl block",
            )
            .to_compile_error();
        }

        let mut clean = item.clone();
        strip_sentinels(&mut clean);

        if is_py_only {
            if stub_gen_disabled {
                strip_gen_stub_from_item(&mut clean);
            }
            py_items.push(quote! { #clean });
        } else if is_rust_only {
            let mut plain_clean = clean;
            strip_python_attrs_from_impl_item(&mut plain_clean);
            plain_items.push(quote! { #plain_clean });
        } else if is_py_attrs {
            let mut stripped = clean.clone();
            strip_python_attrs_from_impl_item(&mut stripped);

            if stub_gen_disabled {
                strip_gen_stub_from_item(&mut clean);
            }

            py_items.push(quote! { #clean });
            plain_items.push(quote! { #stripped });
        } else {
            let mut py_clean = clean.clone();
            let mut plain_clean = clean;

            if stub_gen_disabled {
                strip_gen_stub_from_item(&mut py_clean);
            }
            strip_python_attrs_from_impl_item(&mut plain_clean);

            py_items.push(quote! { #py_clean });
            plain_items.push(quote! { #plain_clean });
        }
    }

    let stub = stub_attr(&args.stub_gen, StubKind::Methods);
    let pymethods_inner = attr_args(args.pyo3_crate_attr(), None);

    quote! {
        #stub
        #[allow(unexpected_cfgs)]
        #[cfg(feature = #feature)]
        #[#pyo3::pymethods #pymethods_inner]
        #(#pass_through_attrs)*
        impl #impl_generics #self_ty #where_clause {
            #(#py_items)*
        }

        #[allow(unexpected_cfgs)]
        #[cfg(not(feature = #feature))]
        #(#pass_through_attrs)*
        impl #impl_generics #self_ty #where_clause {
            #(#plain_items)*
        }
    }
}

pub(crate) fn expand_fn(args: MacroArgs, input_fn: syn::ItemFn) -> TokenStream {
    if let Some(error) = args.reject_class_only_args_on_fn() {
        return error.to_compile_error();
    }

    let feature = &args.feature;
    let pyo3 = args.pyo3_path();
    let mut py_fn = input_fn.clone();
    let mut plain_fn = input_fn;
    plain_fn
        .attrs
        .retain(|a| !is_pyo3_related(a) && !is_gen_stub(a));
    strip_pyo3_from_signature(&mut plain_fn.sig);
    strip_gen_stub_from_signature(&mut plain_fn.sig);

    if args.should_strip_stub_gen() {
        py_fn.attrs.retain(|a| !is_gen_stub(a));
        strip_gen_stub_from_signature(&mut py_fn.sig);
    }

    let stub = stub_attr(&args.stub_gen, StubKind::Function);
    let pyfunction_inner = args.pyfunction_args.as_ref().map_or_else(
        || attr_args(args.pyo3_crate_attr(), None),
        |a| attr_args(args.pyo3_crate_attr(), Some(a)),
    );

    let plain = if args.py_only {
        quote! {}
    } else {
        quote! {
            #[allow(unexpected_cfgs)]
            #[cfg(not(feature = #feature))]
            #plain_fn
        }
    };

    quote! {
        #stub
        #[allow(unexpected_cfgs)]
        #[cfg(feature = #feature)]
        #[#pyo3::pyfunction #pyfunction_inner]
        #py_fn

        #plain
    }
}

pub(crate) fn expand_dispatch(args: MacroArgs, item: Item) -> TokenStream {
    match item {
        Item::Struct(item) => expand_struct(args, item),
        Item::Enum(item) => expand_enum(args, item),
        Item::Impl(item) => expand_methods(args, item),
        Item::Fn(item) => expand_fn(args, item),
        other => syn::Error::new_spanned(
            other,
            "`#[py_compat]` supports structs, enums, inherent impl blocks, and free functions",
        )
        .to_compile_error(),
    }
}

pub(crate) fn expand_module(args: ModuleArgs) -> TokenStream {
    let feature = &args.feature;
    let pyo3 = args.pyo3_path();
    let pymodule_inner = attr_args(args.pyo3_crate_attr(), None);
    let module = &args.module;
    let classes = &args.classes;
    let functions = &args.functions;
    let constants = &args.constants;
    let constant_names = constants.iter().map(|c| &c.name);
    let constant_values = constants.iter().map(|c| &c.value);
    let init = args.init.as_ref();
    let doc_attr = args.doc.as_ref().map(|doc| quote! { #[doc = #doc] });
    let init_call = init.map(|init| {
        quote! {
            let __pyo3_gated_init_result: #pyo3::PyResult<()> = (#init)(m);
            __pyo3_gated_init_result?;
        }
    });

    quote! {
        #[allow(unexpected_cfgs)]
        #[cfg(feature = #feature)]
        #doc_attr
        #[#pyo3::pymodule #pymodule_inner]
        fn #module(
            m: &#pyo3::Bound<'_, #pyo3::types::PyModule>
        ) -> #pyo3::PyResult<()> {
            use #pyo3::types::PyModuleMethods as _;
            #(
                m.add_class::<#classes>()?;
            )*
            #(
                m.add_function(#pyo3::wrap_pyfunction!(#functions, m)?)?;
            )*
            #(
                m.add(#constant_names, #constant_values)?;
            )*
            #init_call
            Ok(())
        }
    }
}

fn attr_args(crate_attr: Option<syn::LitStr>, args: Option<&TokenStream>) -> TokenStream {
    match (crate_attr, args) {
        (Some(crate_attr), Some(args)) => quote! { (crate = #crate_attr, #args) },
        (Some(crate_attr), None) => quote! { (crate = #crate_attr) },
        (None, Some(args)) => quote! { (#args) },
        (None, None) => quote! {},
    }
}
