use proc_macro2::{Span, TokenStream};
use syn::{
    Expr, ExprClosure, LitBool, LitStr, Token,
    parse::{Parse, ParseStream},
    punctuated::Punctuated,
};

pub(crate) struct MacroArgs {
    pub feature: String,
    pub stub_gen: StubGenMode,
    pub pyclass_args: Option<TokenStream>,
    pub pyclass_args_span: Option<Span>,
    pub pyfunction_args: Option<TokenStream>,
    pub pyfunction_args_span: Option<Span>,
    pub pyo3_crate: Option<TokenStream>,
    pub pyo3_crate_attr: Option<String>,
    pub py_only: bool,
    pub py_only_span: Option<Span>,
}

pub(crate) struct ModuleArgs {
    pub feature: String,
    pub pyo3_crate: Option<TokenStream>,
    pub pyo3_crate_attr: Option<String>,
    pub module: syn::Ident,
    pub classes: Vec<syn::Type>,
    pub functions: Vec<syn::Path>,
    pub constants: Vec<ModuleConstant>,
    pub init: Option<ExprClosure>,
    pub doc: Option<LitStr>,
}

pub(crate) struct ModuleConstant {
    pub name: LitStr,
    pub value: Expr,
}

pub(crate) enum StubGenMode {
    Disabled,
    Feature(String),
}

enum RawStubGen {
    Unset,
    Disabled,
    SameAsFeature,
    Feature(String),
}

impl Default for MacroArgs {
    fn default() -> Self {
        Self {
            feature: "python".to_string(),
            stub_gen: StubGenMode::Feature("stub-gen".to_string()),
            pyclass_args: None,
            pyclass_args_span: None,
            pyfunction_args: None,
            pyfunction_args_span: None,
            pyo3_crate: None,
            pyo3_crate_attr: None,
            py_only: false,
            py_only_span: None,
        }
    }
}

impl Parse for MacroArgs {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut feature = None::<String>;
        let mut stub_gen = RawStubGen::Unset;
        let mut pyclass_args = None::<TokenStream>;
        let mut pyclass_args_span = None::<Span>;
        let mut pyfunction_args = None::<TokenStream>;
        let mut pyfunction_args_span = None::<Span>;
        let mut pyo3_crate = None::<TokenStream>;
        let mut pyo3_crate_attr = None::<String>;
        let mut py_only = false;
        let mut py_only_span = None::<Span>;

        while !input.is_empty() {
            let ident: syn::Ident = input.parse()?;
            match ident.to_string().as_str() {
                "feature" => {
                    if feature.is_some() {
                        return Err(syn::Error::new(
                            ident.span(),
                            "duplicate `feature` argument",
                        ));
                    }
                    input.parse::<Token![=]>()?;
                    let value = input.parse::<LitStr>()?.value();
                    if value.is_empty() {
                        return Err(syn::Error::new(ident.span(), "`feature` must not be empty"));
                    }
                    feature = Some(value);
                }
                "stub_gen" => {
                    if !matches!(stub_gen, RawStubGen::Unset) {
                        return Err(syn::Error::new(
                            ident.span(),
                            "duplicate `stub_gen` argument",
                        ));
                    }
                    input.parse::<Token![=]>()?;
                    if input.peek(LitBool) {
                        let b: LitBool = input.parse()?;
                        stub_gen = if b.value() {
                            RawStubGen::SameAsFeature
                        } else {
                            RawStubGen::Disabled
                        };
                    } else {
                        let value = input.parse::<LitStr>()?.value();
                        if value.is_empty() {
                            return Err(syn::Error::new(
                                ident.span(),
                                "`stub_gen` feature must not be empty",
                            ));
                        }
                        stub_gen = RawStubGen::Feature(value);
                    }
                }
                "pyclass_args" => {
                    if pyclass_args.is_some() {
                        return Err(syn::Error::new(
                            ident.span(),
                            "duplicate `pyclass_args` argument",
                        ));
                    }
                    pyclass_args_span = Some(ident.span());
                    let inner;
                    syn::parenthesized!(inner in input);
                    pyclass_args = Some(inner.parse::<TokenStream>()?);
                }
                "pyfunction_args" => {
                    if pyfunction_args.is_some() {
                        return Err(syn::Error::new(
                            ident.span(),
                            "duplicate `pyfunction_args` argument",
                        ));
                    }
                    pyfunction_args_span = Some(ident.span());
                    let inner;
                    syn::parenthesized!(inner in input);
                    pyfunction_args = Some(inner.parse::<TokenStream>()?);
                }
                "pyo3_crate" => {
                    if pyo3_crate.is_some() {
                        return Err(syn::Error::new(
                            ident.span(),
                            "duplicate `pyo3_crate` argument",
                        ));
                    }
                    input.parse::<Token![=]>()?;
                    let value = input.parse::<LitStr>()?.value();
                    if value.is_empty() {
                        return Err(syn::Error::new(
                            ident.span(),
                            "`pyo3_crate` must not be empty",
                        ));
                    }
                    let path: syn::Path = syn::parse_str(&value).map_err(|_| {
                        syn::Error::new(ident.span(), "`pyo3_crate` must be a valid Rust path")
                    })?;
                    pyo3_crate = Some(quote::quote! { #path });
                    pyo3_crate_attr = Some(value);
                }
                "py_only" => {
                    if py_only {
                        return Err(syn::Error::new(
                            ident.span(),
                            "duplicate `py_only` argument",
                        ));
                    }
                    py_only = true;
                    py_only_span = Some(ident.span());
                }
                other => {
                    return Err(syn::Error::new(
                        ident.span(),
                        format!(
                            "unknown argument `{other}`; expected `feature`, `stub_gen`, `pyclass_args`, `pyfunction_args`, `pyo3_crate`, or `py_only`"
                        ),
                    ));
                }
            }
            let _ = input.parse::<Token![,]>();
        }

        let feature = feature.unwrap_or_else(|| "python".to_string());
        let stub_gen = match stub_gen {
            RawStubGen::Unset => StubGenMode::Feature("stub-gen".to_string()),
            RawStubGen::SameAsFeature => StubGenMode::Feature(feature.clone()),
            RawStubGen::Feature(feature) => StubGenMode::Feature(feature),
            RawStubGen::Disabled => StubGenMode::Disabled,
        };

        Ok(MacroArgs {
            feature,
            stub_gen,
            pyclass_args,
            pyclass_args_span,
            pyfunction_args,
            pyfunction_args_span,
            pyo3_crate,
            pyo3_crate_attr,
            py_only,
            py_only_span,
        })
    }
}

impl MacroArgs {
    pub(crate) fn pyo3_path(&self) -> TokenStream {
        self.pyo3_crate
            .clone()
            .unwrap_or_else(crate::paths::pyo3_crate_path)
    }

    pub(crate) fn pyo3_crate_attr(&self) -> Option<LitStr> {
        crate_attr_literal(self.pyo3_crate_attr.as_deref())
    }

    pub(crate) fn should_strip_stub_gen(&self) -> bool {
        matches!(self.stub_gen, StubGenMode::Disabled)
    }

    pub(crate) fn reject_fn_only_args(&self) -> Option<syn::Error> {
        if self.pyfunction_args.is_some() {
            return Some(syn::Error::new(
                self.pyfunction_args_span
                    .unwrap_or_else(proc_macro2::Span::call_site),
                "`pyfunction_args` applies only to `#[py_compat_fn]` and function items passed to `#[py_compat]`",
            ));
        }
        if self.py_only {
            return Some(syn::Error::new(
                self.py_only_span
                    .unwrap_or_else(proc_macro2::Span::call_site),
                "`py_only` applies only to `#[py_compat_fn]` and function items passed to `#[py_compat]`",
            ));
        }
        None
    }

    pub(crate) fn reject_class_only_args_on_fn(&self) -> Option<syn::Error> {
        if self.pyclass_args.is_some() {
            return Some(syn::Error::new(
                self.pyclass_args_span
                    .unwrap_or_else(proc_macro2::Span::call_site),
                "`pyclass_args` applies only to structs/enums; use `pyfunction_args` for functions",
            ));
        }
        None
    }
}

impl ModuleArgs {
    pub(crate) fn pyo3_path(&self) -> TokenStream {
        self.pyo3_crate
            .clone()
            .unwrap_or_else(crate::paths::pyo3_crate_path)
    }

    pub(crate) fn pyo3_crate_attr(&self) -> Option<LitStr> {
        crate_attr_literal(self.pyo3_crate_attr.as_deref())
    }
}

fn crate_attr_literal(override_path: Option<&str>) -> Option<LitStr> {
    let path = override_path
        .map(ToOwned::to_owned)
        .unwrap_or_else(crate::paths::pyo3_crate_attr_path);
    Some(LitStr::new(&path, proc_macro2::Span::call_site()))
}

impl Parse for ModuleArgs {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut feature = None::<String>;
        let mut pyo3_crate = None::<TokenStream>;
        let mut pyo3_crate_attr = None::<String>;
        let mut module = None::<syn::Ident>;
        let mut classes = Vec::<syn::Type>::new();
        let mut classes_seen = false;
        let mut functions = Vec::<syn::Path>::new();
        let mut functions_seen = false;
        let mut constants = Vec::<ModuleConstant>::new();
        let mut constants_seen = false;
        let mut init = None::<ExprClosure>;
        let mut doc = None::<LitStr>;

        while !input.is_empty() {
            let ident: syn::Ident = input.parse()?;
            match ident.to_string().as_str() {
                "feature" => {
                    if feature.is_some() {
                        return Err(syn::Error::new(ident.span(), "duplicate `feature` entry"));
                    }
                    input.parse::<Token![=]>()?;
                    let value = input.parse::<LitStr>()?.value();
                    if value.is_empty() {
                        return Err(syn::Error::new(ident.span(), "`feature` must not be empty"));
                    }
                    feature = Some(value);
                }
                "pyo3_crate" => {
                    if pyo3_crate.is_some() {
                        return Err(syn::Error::new(
                            ident.span(),
                            "duplicate `pyo3_crate` entry",
                        ));
                    }
                    input.parse::<Token![=]>()?;
                    let value = input.parse::<LitStr>()?.value();
                    if value.is_empty() {
                        return Err(syn::Error::new(
                            ident.span(),
                            "`pyo3_crate` must not be empty",
                        ));
                    }
                    let path: syn::Path = syn::parse_str(&value).map_err(|_| {
                        syn::Error::new(ident.span(), "`pyo3_crate` must be a valid Rust path")
                    })?;
                    pyo3_crate = Some(quote::quote! { #path });
                    pyo3_crate_attr = Some(value);
                }
                "module" => {
                    if module.is_some() {
                        return Err(syn::Error::new(ident.span(), "duplicate `module` entry"));
                    }
                    module = Some(input.parse()?);
                }
                "classes" => {
                    if classes_seen {
                        return Err(syn::Error::new(ident.span(), "duplicate `classes` entry"));
                    }
                    classes_seen = true;
                    input.parse::<Token![:]>()?;
                    let content;
                    syn::bracketed!(content in input);
                    classes = Punctuated::<syn::Type, Token![,]>::parse_terminated(&content)?
                        .into_iter()
                        .collect();
                }
                "functions" => {
                    if functions_seen {
                        return Err(syn::Error::new(ident.span(), "duplicate `functions` entry"));
                    }
                    functions_seen = true;
                    input.parse::<Token![:]>()?;
                    let content;
                    syn::bracketed!(content in input);
                    functions = Punctuated::<syn::Path, Token![,]>::parse_terminated(&content)?
                        .into_iter()
                        .collect();
                }
                "constants" => {
                    if constants_seen {
                        return Err(syn::Error::new(ident.span(), "duplicate `constants` entry"));
                    }
                    constants_seen = true;
                    input.parse::<Token![:]>()?;
                    let content;
                    syn::bracketed!(content in input);
                    constants =
                        Punctuated::<ModuleConstant, Token![,]>::parse_terminated(&content)?
                            .into_iter()
                            .collect();
                }
                "init" => {
                    if init.is_some() {
                        return Err(syn::Error::new(ident.span(), "duplicate `init` entry"));
                    }
                    input.parse::<Token![:]>()?;
                    init = Some(input.parse()?);
                }
                "doc" => {
                    if doc.is_some() {
                        return Err(syn::Error::new(ident.span(), "duplicate `doc` entry"));
                    }
                    input.parse::<Token![:]>()?;
                    doc = Some(input.parse()?);
                }
                other => {
                    return Err(syn::Error::new(
                        ident.span(),
                        format!(
                            "unknown module entry `{other}`; expected `module`, `classes`, `functions`, `constants`, `init`, `doc`, `feature`, or `pyo3_crate`"
                        ),
                    ));
                }
            }

            let _ = input.parse::<Token![;]>();
            let _ = input.parse::<Token![,]>();
        }

        let module = module.ok_or_else(|| {
            syn::Error::new(
                proc_macro2::Span::call_site(),
                "`define_py_module!` requires `module <name>;`",
            )
        })?;
        let feature = feature.unwrap_or_else(|| "python".to_string());

        Ok(Self {
            feature,
            pyo3_crate,
            pyo3_crate_attr,
            module,
            classes,
            functions,
            constants,
            init,
            doc,
        })
    }
}

impl Parse for ModuleConstant {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let content;
        syn::parenthesized!(content in input);
        let name = content.parse::<LitStr>()?;
        content.parse::<Token![,]>()?;
        let value = content.parse::<Expr>()?;
        Ok(Self { name, value })
    }
}
