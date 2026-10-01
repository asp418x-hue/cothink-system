use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use crate::ast::{CompilationUnit, NodeId};
use crate::callable::{
    BridgedEnumMixinCallTarget, BridgedEnumMixinMethodCallable, BridgedMixinMethodCallable,
    BridgedSuperMethodCallable, Callable, InterpretedFunction, NativeFunction, Parameter,
    SyncGenerator, call_callable, make_callable_runtime_type,
};
use crate::class::{
    BoundBridgedSuperData, BoundSuperData, BridgedInstanceData, ClassDef, EnumDef, ExtTypeDef,
    FactoryRedirect, LateVariable,
};
use crate::env::Environment;
use crate::error::InterpError;
use crate::rt::{RuntimeType, runtime_type_of};
use crate::value::{
    DartMap, DartSet, EnumValueData, ExtTypeInstData, InstanceData, RecordData, Value,
    enum_value_get, enum_value_set, instance_get, instance_set, is_truthy, num_add, num_compare,
    num_div, num_mul, num_neg, num_rem, num_sub, num_trunc_div, value_display, value_eq,
};

fn is_bigintish(v: &Value) -> bool {
    match v {
        Value::BigInt(_) => true,
        Value::Bridged(b) => matches!(&b.borrow().native, Value::BigInt(_)),
        _ => false,
    }
}

fn loop_declared_names(unit: &Rc<CompilationUnit>, init: NodeId) -> Vec<String> {
    let mut names = Vec::new();
    if let Some(d) = unit.first_child_of_kind(init, "initialized_variable_definition") {
        if let Some(n) = unit.child_by_field(d, "name") {
            names.push(unit.text(n).to_string());
        } else if let Some(i) = unit.first_child_of_kind(d, "initialized_identifier") {
            names.push(unit.text(i).to_string());
        }
        for extra in unit.named_children(d) {
            if unit.kind(extra) == "initialized_identifier"
                && let Some(n) = unit.child_by_field(extra, "name")
            {
                names.push(unit.text(n).to_string());
            }
        }
        return names;
    }
    for child in unit.named_children(init) {
        if matches!(
            unit.kind(child),
            "initialized_identifier_list" | "uninitialized_identifier_list"
        ) {
            for item in unit.named_children(child) {
                let ik = unit.kind(item);
                if ik == "initialized_identifier" || ik == "simple_identifier" || ik == "identifier"
                {
                    names.push(unit.text(item).to_string());
                }
            }
        }
    }
    names
}

fn bigint_op(
    left: &Value,
    right: &Value,
    f: impl Fn(num_bigint::BigInt, num_bigint::BigInt) -> num_bigint::BigInt,
) -> Option<Value> {
    if !(is_bigintish(left) || is_bigintish(right)) {
        return None;
    }
    let l = crate::stdlib::helpers::as_bigint(left)?;
    let r = crate::stdlib::helpers::as_bigint(right)?;
    Some(crate::stdlib::helpers::wrap(
        &crate::stdlib::helpers::class("BigInt"),
        Value::BigInt(f(l, r)),
    ))
}

fn bigint_cmp(left: &Value, right: &Value) -> Option<std::cmp::Ordering> {
    if !(is_bigintish(left) || is_bigintish(right)) {
        return None;
    }
    let l = crate::stdlib::helpers::as_bigint(left)?;
    let r = crate::stdlib::helpers::as_bigint(right)?;
    Some(l.cmp(&r))
}

/// Returns true if `ancestor` is `target` or an ancestor of `target`.
#[allow(dead_code)]
fn unit_contains(unit: &Rc<CompilationUnit>, ancestor: NodeId, target: NodeId) -> bool {
    let mut cur = target;
    loop {
        if cur == ancestor {
            return true;
        }
        match unit.parent(cur) {
            Some(p) => cur = p,
            None => return false,
        }
    }
}

/// Walks up from `id` to the enclosing `function_body` or
/// `function_expression_body` node (the boundary of the current function).
#[allow(dead_code)]
fn enclosing_function_body(unit: &Rc<CompilationUnit>, id: NodeId) -> Option<NodeId> {
    let mut cur = id;
    while let Some(p) = unit.parent(cur) {
        let k = unit.kind(p);
        if k == "function_body" || k == "function_expression_body" {
            return Some(p);
        }
        cur = p;
    }
    None
}

pub struct InterpreterVisitor {
    pub current_environment: Rc<RefCell<Environment>>,
    pub current_name: String,
    pub stack_trace: Vec<(String, usize, usize)>,
    pub sync_generator: Option<Rc<RefCell<SyncGenerator>>>,
    pub current_exception: Option<Value>,
    pub await_cache: HashMap<AwaitCacheKey, Result<Value, Value>>,
    pub loop_iterations: Vec<usize>,
    pub async_frames: Vec<crate::async_state::AsyncFrame>,
    /// Resume replay state: the await node (with its loop-iteration stack)
    /// where the previous pass suspended. While set, statements before that
    /// await are skipped so already-executed side effects (declaration
    /// initializers, callback-driven writes) are not replayed.
    pub resume_state: Option<(NodeId, Vec<usize>)>,
    /// Current interpreted call depth; runaway recursion is aborted with a
    /// runtime error before it overflows the host stack and kills the
    /// process (library mode must never crash on Dart errors).
    pub call_depth: usize,
}

/// Maximum interpreted call depth. Deliberately far below the ~8 MiB
/// default thread stack so deep (but legitimate) recursion still works
/// while runaway recursion fails gracefully.
const MAX_CALL_DEPTH: usize = 120;

type AwaitCacheKey = (NodeId, Vec<usize>);

/// A try/catch-relevant clause (type, on-symbol, stack, body) captured while
/// walking a fan-in/suspension point.
type CatchHandler = (Option<String>, Option<String>, Option<String>, NodeId);

/// A parsed function signature: name, return type, parameters, and the
/// getter/setter/factory/abstract/external flags.
type ParsedSignature = (
    Option<String>,
    Option<Rc<RuntimeType>>,
    Vec<Parameter>,
    bool,
    bool,
    bool,
);

/// Snapshot of interpreter state saved across async suspensions: the await
/// cache, the loop iteration markers and the resume point.
type SkipState = (
    HashMap<AwaitCacheKey, Result<Value, Value>>,
    Vec<usize>,
    Option<(NodeId, Vec<usize>)>,
);

impl InterpreterVisitor {
    pub fn new(environment: Rc<RefCell<Environment>>) -> InterpreterVisitor {
        InterpreterVisitor {
            current_environment: environment,
            current_name: "".to_string(),
            stack_trace: Vec::new(),
            sync_generator: None,
            current_exception: None,
            await_cache: HashMap::new(),
            loop_iterations: Vec::new(),
            async_frames: Vec::new(),
            resume_state: None,
            call_depth: 0,
        }
    }

    fn push_frame(&mut self, unit: &Rc<CompilationUnit>, name: &str, node: NodeId) {
        self.current_name = name.to_string();
        let (line, col) = unit.line_col(unit.node(node).start);
        self.stack_trace.push((name.to_string(), line, col));
    }

    fn pop_frame(&mut self) {
        self.stack_trace.pop();
    }

    pub fn execute_top_level(&mut self, unit: &Rc<CompilationUnit>) -> Result<Value, InterpError> {
        for &child in unit.children(0) {
            let kind = unit.kind(child);
            match kind {
                "function_declaration" | "local_function_declaration" => {
                    self.define_function(unit, child)?;
                }
                "top_level_variable_declaration"
                | "static_final_declaration"
                | "static_final_declaration_list" => {
                    self.visit_statement(unit, child)?;
                }
                "class_declaration" => {
                    self.define_class(unit, child)?;
                }
                "enum_declaration" => {
                    self.define_enum(unit, child)?;
                }
                "typedef" => {
                    self.define_typedef(unit, child)?;
                }
                "mixin_declaration" | "extension_declaration" | "extension_type_declaration" => {
                    return Err(InterpError::unimplemented(format!(
                        "Top-level declaration '{}' requires the declaration pass (not yet implemented)",
                        kind
                    )));
                }
                "library_import" => {
                    self.visit_import(unit, child)?;
                }
                "import_or_export" => {
                    if let Some(imp) = unit.first_child_of_kind(child, "library_import") {
                        self.visit_import(unit, imp)?;
                    } else if let Some(exp) = unit.first_child_of_kind(child, "library_export") {
                        self.visit_export(unit, exp)?;
                    }
                }
                "library_export" => {
                    self.visit_export(unit, child)?;
                }
                "part_directive" | "part_of_directive" | "library_name" | "script_tag"
                | "empty_statement" | "comment" => {}
                _ => {
                    self.visit_statement(unit, child)?;
                }
            }
        }
        Ok(Value::Null)
    }

    // =========================================================================
    // Imports
    // =========================================================================

    /// Handles `import 'dart:math' as math show pi, hide foo;`.
    ///
    /// Supported URIs:
    /// - `dart:*` built-in libraries (lazily loaded into a dedicated
    ///   environment per URI).
    /// - `package:...` libraries registered via
    ///   [`crate::stdlib::register_package_library`] / `Context::register_package_library`.
    /// - Virtual modules registered with
    ///   [`Context::register_dart_module`](crate::Context::register_dart_module),
    ///   matched by the exact URI string (e.g. `./mylib.dart`).
    /// - Relative filesystem URIs (`./foo.dart`, `foo.dart`) resolved against
    ///   the current working directory, but only when the context is granted
    ///   `Permission::ModuleFiles` (plus filesystem read permission for the
    ///   path). Without that flag, filesystem loading is refused and the
    ///   module must be registered virtually.
    fn visit_import(
        &mut self,
        unit: &Rc<CompilationUnit>,
        id: NodeId,
    ) -> Result<Value, InterpError> {
        let Some(spec) = unit
            .first_child_of_kind(id, "import_specification")
            .or_else(|| unit.named_children(id).first().copied())
        else {
            return Ok(Value::Null);
        };
        let uri = unit
            .child_by_field(spec, "uri")
            .map(|u| {
                unit.text(u)
                    .trim_matches(|c| c == '\'' || c == '"')
                    .to_string()
            })
            .unwrap_or_default();
        if uri.is_empty() {
            return Ok(Value::Null);
        }
        let alias = unit
            .child_by_field(spec, "alias")
            .map(|a| unit.text(a).to_string());
        let (mut show, mut hide): (Option<Vec<String>>, Option<Vec<String>>) = (None, None);
        for c in unit.children_of_kind(spec, "combinator") {
            let is_show = unit.children(c).iter().any(|&cc| unit.kind(cc) == "show");
            let names: Vec<String> = unit
                .children(c)
                .iter()
                .filter(|&&cc| unit.kind(cc) == "identifier")
                .map(|&cc| unit.text(cc).to_string())
                .collect();
            if is_show {
                show = Some(names);
            } else {
                hide = Some(names);
            }
        }

        let imported = self.resolve_library(&uri)?;
        if let Some(alias) = alias {
            let filtered =
                Environment::shallow_copy_filtered(&imported, show.as_deref(), hide.as_deref());
            Environment::define_prefixed_import(&self.current_environment, &alias, filtered);
        } else {
            Environment::import_environment(
                &self.current_environment,
                &imported,
                show.as_deref(),
                hide.as_deref(),
            )?;
        }
        Ok(Value::Null)
    }

    /// Handles `export 'uri' show a, hide b;`: re-exports the public
    /// top-level names of the target library into the current one, using
    /// the same resolution rules as [`Self::visit_import`] (virtual module
    /// registry first, filesystem only with `Permission::ModuleFiles`).
    fn visit_export(
        &mut self,
        unit: &Rc<CompilationUnit>,
        id: NodeId,
    ) -> Result<Value, InterpError> {
        let uri = unit
            .child_by_field(id, "uri")
            .map(|u| {
                unit.text(u)
                    .trim_matches(|c| c == '\'' || c == '"')
                    .to_string()
            })
            .unwrap_or_default();
        if uri.is_empty() {
            return Ok(Value::Null);
        }
        let (mut show, mut hide): (Option<Vec<String>>, Option<Vec<String>>) = (None, None);
        for c in unit.children_of_kind(id, "combinator") {
            let is_show = unit.children(c).iter().any(|&cc| unit.kind(cc) == "show");
            let names: Vec<String> = unit
                .children(c)
                .iter()
                .filter(|&&cc| unit.kind(cc) == "identifier")
                .map(|&cc| unit.text(cc).to_string())
                .collect();
            if is_show {
                show = Some(names);
            } else {
                hide = Some(names);
            }
        }
        let imported = self.resolve_library(&uri)?;
        Environment::export_environment(
            &self.current_environment,
            &imported,
            show.as_deref(),
            hide.as_deref(),
        )?;
        Ok(Value::Null)
    }

    /// Resolves a module URI to its environment, shared by `import` and
    /// `export`. Order: `dart:*` built-ins, registered Rust module
    /// libraries ([`crate::stdlib::register_module_library`], which covers
    /// `package:` URIs and bare names alike), the virtual module registry
    /// ([`Context::register_dart_module`]), then the filesystem — which is
    /// only consulted when the context holds `Permission::ModuleFiles`
    /// (and the path passes the filesystem read permission).
    fn resolve_library(&self, uri: &str) -> Result<Rc<RefCell<Environment>>, InterpError> {
        if let Some(env) = crate::stdlib::load_library(uri) {
            return Ok(env);
        }
        if let Some(env) = crate::stdlib::module_library_environment(uri) {
            return Ok(env);
        }
        if let Some(env) = self.load_virtual_module(uri) {
            return Ok(env);
        }
        let fs_modules = crate::permission::require_module_files().is_ok();
        if fs_modules && let Some(env) = self.load_filesystem_library(uri) {
            return Ok(env);
        }
        let extra = if !uri.starts_with("dart:") && !uri.starts_with("package:") && !fs_modules {
            " (filesystem module loading is not granted: register the module with \
             Context::register_dart_module or grant Permission::ModuleFiles)"
        } else {
            ""
        };
        Err(InterpError::runtime(format!(
            "Unresolved import: '{uri}'{extra}"
        )))
    }

    fn load_virtual_module(&self, uri: &str) -> Option<Rc<RefCell<Environment>>> {
        let state = crate::state::current();
        let source = state
            .virtual_modules
            .borrow()
            .get(uri)
            .cloned()
            .or_else(|| crate::stdlib::dart_module_source(uri))?;
        let unit = Rc::new(CompilationUnit::parse(&source).ok()?);
        let env = Environment::new();
        let mut visitor = InterpreterVisitor::new(env.clone());
        visitor.execute_top_level(&unit).ok()?;
        Some(env)
    }

    fn load_filesystem_library(&self, uri: &str) -> Option<Rc<RefCell<Environment>>> {
        let path = std::path::Path::new(uri);
        let full = if path.is_absolute() {
            path.to_path_buf()
        } else {
            std::path::PathBuf::from(".").join(path)
        };
        crate::permission::require_fs_read(&full.to_string_lossy()).ok()?;
        let source = std::fs::read_to_string(&full).ok()?;
        let unit = Rc::new(CompilationUnit::parse(&source).ok()?);
        let env = Environment::new();
        let mut visitor = InterpreterVisitor::new(env.clone());
        visitor.execute_top_level(&unit).ok()?;
        Some(env)
    }

    // =========================================================================
    // Statements
    // =========================================================================

    pub fn visit_statement(
        &mut self,
        unit: &Rc<CompilationUnit>,
        id: NodeId,
    ) -> Result<Value, InterpError> {
        if self.resume_state.is_some() {
            match unit.kind(id) {
                "return_statement" | "break_statement" | "continue_statement" => {
                    self.resume_state = None;
                    return self.visit_statement_inner(unit, id);
                }
                _ => {}
            }
            let (rn, ri) = self.resume_state.clone().unwrap();
            let mut skip = true;
            if self.node_is_ancestor_of(unit, id, rn) {
                let i = &self.loop_iterations;
                skip = i.len() > ri.len()
                    || ri.get(..i.len()).map(|p| p != i.as_slice()).unwrap_or(true);
            }
            if skip {
                return Ok(Value::Null);
            }
        }
        self.visit_statement_inner(unit, id)
    }

    /// Whether `ancestor` is an ancestor-or-self of `descendant` (walking
    /// parent pointers upward from `descendant`).
    fn node_is_ancestor_of(
        &self,
        unit: &Rc<CompilationUnit>,
        ancestor: NodeId,
        descendant: NodeId,
    ) -> bool {
        let mut cur = Some(descendant);
        while let Some(c) = cur {
            if c == ancestor {
                return true;
            }
            cur = unit.parent(c);
        }
        false
    }

    fn visit_statement_inner(
        &mut self,
        unit: &Rc<CompilationUnit>,
        id: NodeId,
    ) -> Result<Value, InterpError> {
        match unit.kind(id) {
            "block" => self.visit_block(unit, id),
            "local_variable_declaration"
            | "top_level_variable_declaration"
            | "static_final_declaration" => self.visit_variable_declaration(unit, id),
            "expression_statement" => {
                let expr = unit.named_children(id).first().copied();
                match expr {
                    Some(e) => self.visit_expression(unit, e),
                    None => Ok(Value::Null),
                }
            }
            "if_statement" => self.visit_if(unit, id),
            "while_statement" => self.visit_while(unit, id),
            "do_statement" => self.visit_do(unit, id),
            "for_statement" => self.visit_for(unit, id),
            "return_statement" => self.visit_return(unit, id),
            "break_statement" => self.visit_break(unit, id),
            "continue_statement" => self.visit_continue(unit, id),
            "labeled_statement" => self.visit_labeled(unit, id),
            "try_statement" => self.visit_try(unit, id),
            "rethrow_statement" => {
                let v = self.current_exception.clone().unwrap_or(Value::Null);
                Err(InterpError::Throw(v))
            }
            "yield_statement" => self.visit_yield(unit, id),
            "yield_each_statement" => self.visit_yield_each(unit, id),
            "assert_statement" => self.visit_assert(unit, id),
            "empty_statement" => Ok(Value::Null),
            "comment" => Ok(Value::Null),
            "switch_statement" => self.visit_switch(unit, id),
            "function_declaration" | "local_function_declaration" => self.define_function(unit, id),
            "class_declaration" => self.define_class(unit, id),
            "enum_declaration" => self.define_enum(unit, id),
            "mixin_declaration" | "extension_declaration" | "extension_type_declaration" => Err(
                InterpError::unimplemented("Nested declarations are not yet supported"),
            ),
            "throw_expression" => self.visit_expression(unit, id),
            other => Err(InterpError::unimplemented(format!(
                "Statement '{}' is not yet implemented",
                other
            ))),
        }
    }

    fn visit_block(
        &mut self,
        unit: &Rc<CompilationUnit>,
        id: NodeId,
    ) -> Result<Value, InterpError> {
        let env = Environment::child(&self.current_environment);
        self.run_block_in_env(unit, id, &env)
    }
    fn run_block_in_env(
        &mut self,
        unit: &Rc<CompilationUnit>,
        id: NodeId,
        env: &Rc<RefCell<Environment>>,
    ) -> Result<Value, InterpError> {
        let saved = self.current_environment.clone();
        self.current_environment = env.clone();
        let mut result = Ok(Value::Null);
        for child in unit.named_children(id) {
            result = self.visit_statement(unit, child);
            if result.is_err() {
                break;
            }
        }
        self.current_environment = saved;
        result
    }

    fn visit_variable_declaration(
        &mut self,
        unit: &Rc<CompilationUnit>,
        id: NodeId,
    ) -> Result<Value, InterpError> {
        if let Some(init_def) = unit.first_child_of_kind(id, "initialized_variable_definition") {
            let name = unit
                .child_by_field(init_def, "name")
                .map(|n| unit.text(n).to_string())
                .unwrap_or_else(|| {
                    unit.first_child_of_kind(init_def, "initialized_identifier")
                        .map(|i| unit.text(i).to_string())
                        .unwrap_or_default()
                });
            let value = match unit.child_by_field(init_def, "value") {
                Some(v) => self.visit_expression(unit, v)?,
                None => Value::Null,
            };
            Environment::define(&self.current_environment, &name, value);
            for extra in unit.named_children(init_def) {
                if unit.kind(extra) == "initialized_identifier" {
                    let ename = unit
                        .child_by_field(extra, "name")
                        .map(|n| unit.text(n).to_string())
                        .unwrap_or_default();
                    let evalue = match unit.child_by_field(extra, "value") {
                        Some(v) => self.visit_expression(unit, v)?,
                        None => Value::Null,
                    };
                    Environment::define(&self.current_environment, &ename, evalue);
                }
            }
            return Ok(Value::Null);
        }
        if let Some(pattern_decl) = unit.first_child_of_kind(id, "pattern_variable_declaration") {
            let value = match unit
                .named_children(pattern_decl)
                .iter()
                .copied()
                .find(|&c| unit.nodes[c].is_named)
            {
                Some(v) => self.visit_expression(unit, v)?,
                None => Value::Null,
            };
            return self.bind_pattern(unit, pattern_decl, value);
        }
        for child in unit.named_children(id) {
            let kind = unit.kind(child);
            if kind == "initialized_identifier_list"
                || kind == "uninitialized_identifier_list"
                // `const` / `final` top-level declarations parse as
                // `static_final_declaration_list` in tree-sitter-dart.
                || kind == "static_final_declaration_list"
            {
                for item in unit.named_children(child) {
                    let item_kind = unit.kind(item);
                    if item_kind == "initialized_identifier"
                        || item_kind == "static_final_declaration"
                    {
                        let name = unit
                            .child_by_field(item, "name")
                            .map(|n| unit.text(n).to_string())
                            .unwrap_or_default();
                        let value = match unit.child_by_field(item, "value") {
                            Some(v) => self.visit_expression(unit, v)?,
                            None => Value::Null,
                        };
                        Environment::define(&self.current_environment, &name, value);
                    } else if item_kind == "uninitialized_identifier" {
                        let name = unit
                            .child_by_field(item, "name")
                            .map(|n| unit.text(n).to_string())
                            .unwrap_or_default();
                        Environment::define(&self.current_environment, &name, Value::Null);
                    }
                }
                continue;
            }
            if kind == "initialized_identifier" || kind == "typed_identifier" {
                let name = unit.text(child).to_string();
                Environment::define(&self.current_environment, &name, Value::Null);
            }
        }
        Ok(Value::Null)
    }

    fn visit_if(&mut self, unit: &Rc<CompilationUnit>, id: NodeId) -> Result<Value, InterpError> {
        // The condition is the first named child that is neither the
        // consequence nor the alternative (this grammar version does not
        // label the condition field).
        let cond = match unit.child_by_field(id, "condition") {
            Some(c) => self.visit_expression(unit, c)?,
            None => {
                match unit.named_children(id).iter().copied().find(|&c| {
                    let f = unit.node(c).field;
                    f != Some("consequence") && f != Some("alternative")
                }) {
                    Some(c) => self.visit_expression(unit, c)?,
                    None => Value::Bool(true),
                }
            }
        };
        if is_truthy(&cond)? {
            if let Some(cons) = unit.child_by_field(id, "consequence") {
                return self.visit_statement(unit, cons);
            }
        } else if let Some(alt) = unit.child_by_field(id, "alternative") {
            return self.visit_statement(unit, alt);
        }
        Ok(Value::Null)
    }

    fn visit_while(
        &mut self,
        unit: &Rc<CompilationUnit>,
        id: NodeId,
    ) -> Result<Value, InterpError> {
        let mut iter = 0usize;
        loop {
            self.loop_iterations.push(iter);
            let mut exit = false;
            match self.eval_condition_field(unit, id) {
                Ok(true) => {
                    let body = unit.child_by_field(id, "body");
                    if let Some(b) = body {
                        match self.visit_statement(unit, b) {
                            Err(InterpError::Break(_)) => exit = true,
                            Err(InterpError::Continue(_)) => {}
                            Err(e) => {
                                self.loop_iterations.pop();
                                return Err(e);
                            }
                            Ok(_) => {}
                        }
                    } else {
                        exit = true;
                    }
                }
                Ok(false) => exit = true,
                Err(e) => {
                    self.loop_iterations.pop();
                    return Err(e);
                }
            }
            self.loop_iterations.pop();
            if exit {
                break;
            }
            iter += 1;
        }
        Ok(Value::Null)
    }

    fn visit_do(&mut self, unit: &Rc<CompilationUnit>, id: NodeId) -> Result<Value, InterpError> {
        let mut iter = 0usize;
        loop {
            self.loop_iterations.push(iter);
            let body = unit.child_by_field(id, "body");
            if let Some(b) = body {
                match self.visit_statement(unit, b) {
                    Err(InterpError::Break(_)) => {
                        self.loop_iterations.pop();
                        break;
                    }
                    Err(InterpError::Continue(_)) => {}
                    Err(e) => {
                        self.loop_iterations.pop();
                        return Err(e);
                    }
                    Ok(_) => {}
                }
            }
            let mut exit = false;
            match self.eval_condition_field(unit, id) {
                Ok(true) => {}
                Ok(false) => exit = true,
                Err(e) => {
                    self.loop_iterations.pop();
                    return Err(e);
                }
            }
            self.loop_iterations.pop();
            if exit {
                break;
            }
            iter += 1;
        }
        Ok(Value::Null)
    }

    fn visit_for(&mut self, unit: &Rc<CompilationUnit>, id: NodeId) -> Result<Value, InterpError> {
        if unit.children(id).iter().any(|&c| unit.kind(c) == "await") {
            return self.visit_await_for(unit, id);
        }
        if let Some(value_field) = unit.child_by_field(id, "value") {
            let iterable = self.visit_expression(unit, value_field)?;
            let pattern = unit
                .named_children(id)
                .into_iter()
                .find(|&c| unit.kind(c).ends_with("_pattern"));
            let name = unit
                .child_by_field(id, "name")
                .map(|n| unit.text(n).to_string())
                .or_else(|| {
                    unit.child_by_field(id, "init")
                        .and_then(|init| unit.first_child_of_kind(init, "initialized_identifier"))
                        .map(|n| unit.text(n).to_string())
                });
            let Some(name) = name else {
                let Some(pattern) = pattern else {
                    return Err(InterpError::runtime("For-in loop without a loop variable"));
                };
                return self.for_in_loop_with_pattern(unit, id, &iterable, pattern);
            };
            return self.for_in_loop(unit, id, &iterable, &name);
        }
        let init = unit.child_by_field(id, "init");
        let loop_env = match init {
            Some(init) if unit.kind(init) == "local_variable_declaration" => {
                let env = Environment::child(&self.current_environment);
                let saved = self.current_environment.clone();
                self.current_environment = env.clone();
                // The init resets the loop state on every resume pass, so it
                // must not be skipped by the resume replay machinery.
                let r = self.visit_statement_inner(unit, init);
                self.current_environment = saved;
                r?;
                Some(env)
            }
            Some(init) => {
                self.visit_expression(unit, init)?;
                None
            }
            None => None,
        };
        let declared = match init {
            Some(init) if unit.kind(init) == "local_variable_declaration" => {
                loop_declared_names(unit, init)
            }
            _ => Vec::new(),
        };
        let outer_saved = self.current_environment.clone();
        if let Some(le) = &loop_env {
            self.current_environment = le.clone();
        }
        let mut result = Ok(Value::Null);
        let mut loop_iter = 0usize;
        loop {
            self.loop_iterations.push(loop_iter);
            if let Some(cond) = unit.child_by_field(id, "condition") {
                match self.visit_truthy_condition(unit, cond) {
                    Ok(true) => {}
                    Ok(false) => {
                        self.loop_iterations.pop();
                        break;
                    }
                    Err(e) => {
                        self.loop_iterations.pop();
                        result = Err(e);
                        break;
                    }
                }
            }
            let iter_env = loop_env.as_ref().map(|le| {
                let e = Environment::child(le);
                for n in &declared {
                    let v = Environment::get(le, n).unwrap_or(Value::Null);
                    Environment::define(&e, n, v);
                }
                e
            });
            let saved = self.current_environment.clone();
            if let Some(e) = &iter_env {
                self.current_environment = e.clone();
            }
            if let Some(body) = unit.child_by_field(id, "body") {
                match self.visit_statement(unit, body) {
                    Err(InterpError::Break(_)) => {
                        self.current_environment = saved;
                        self.loop_iterations.pop();
                        break;
                    }
                    Err(InterpError::Continue(_)) => {}
                    Err(e) => {
                        self.current_environment = saved;
                        self.loop_iterations.pop();
                        result = Err(e);
                        break;
                    }
                    Ok(_) => {}
                }
            }
            if let (Some(le), Some(e)) = (loop_env.as_ref(), &iter_env) {
                for n in &declared {
                    if let Ok(v) = Environment::get(e, n) {
                        let _ = Environment::assign(le, n, v);
                    }
                }
            }
            self.current_environment = saved;
            for &upd in &unit.nodes[id]
                .children
                .iter()
                .copied()
                .filter(|&c| unit.nodes[c].field == Some("update"))
                .collect::<Vec<_>>()
            {
                if let Err(e) = self.visit_expression(unit, upd) {
                    result = Err(e);
                    break;
                }
            }
            if result.is_err() {
                break;
            }
            self.loop_iterations.pop();
            loop_iter += 1;
        }
        self.current_environment = outer_saved;
        result
    }

    fn for_in_loop(
        &mut self,
        unit: &Rc<CompilationUnit>,
        id: NodeId,
        iterable: &Value,
        name: &str,
    ) -> Result<Value, InterpError> {
        let items = self.collect_iterable(iterable)?;
        for (iter, item) in items.into_iter().enumerate() {
            self.loop_iterations.push(iter);
            let env = Environment::child(&self.current_environment);
            let saved = self.current_environment.clone();
            self.current_environment = env.clone();
            Environment::define(&env, name, item);
            let body = unit.child_by_field(id, "body");
            let result = match body {
                Some(b) => self.visit_statement(unit, b),
                None => Ok(Value::Null),
            };
            self.current_environment = saved;
            self.loop_iterations.pop();
            match result {
                Err(InterpError::Break(_)) => break,
                Err(InterpError::Continue(_)) => continue,
                Err(e) => return Err(e),
                Ok(_) => {}
            }
        }
        Ok(Value::Null)
    }

    fn for_in_loop_with_pattern(
        &mut self,
        unit: &Rc<CompilationUnit>,
        id: NodeId,
        iterable: &Value,
        pattern: NodeId,
    ) -> Result<Value, InterpError> {
        let items = self.collect_iterable(iterable)?;
        for (iter, item) in items.into_iter().enumerate() {
            self.loop_iterations.push(iter);
            let env = Environment::child(&self.current_environment);
            let saved = self.current_environment.clone();
            self.current_environment = env.clone();
            self.bind_pattern(unit, pattern, item)?;
            let body = unit.child_by_field(id, "body");
            let result = match body {
                Some(b) => self.visit_statement(unit, b),
                None => Ok(Value::Null),
            };
            self.current_environment = saved;
            self.loop_iterations.pop();
            match result {
                Err(InterpError::Break(_)) => break,
                Err(InterpError::Continue(_)) => continue,
                Err(e) => return Err(e),
                Ok(_) => {}
            }
        }
        Ok(Value::Null)
    }

    /// `await for (var x in stream) body` inside an async context: the
    /// subscription is created once (and cached so resume passes reuse
    /// it); each iteration awaits the next event through the iterator
    /// state, suspending the pass until it arrives.
    fn visit_await_for(
        &mut self,
        unit: &Rc<CompilationUnit>,
        id: NodeId,
    ) -> Result<Value, InterpError> {
        if std::env::var("D4RT_DBG").is_ok() {
            eprintln!(
                "[dbg] visit_await_for id={id} resume={:?}",
                self.resume_state
            );
        }
        let name = unit
            .child_by_field(id, "name")
            .map(|n| unit.text(n).to_string())
            .or_else(|| {
                unit.child_by_field(id, "init")
                    .and_then(|init| unit.first_child_of_kind(init, "initialized_identifier"))
                    .map(|n| unit.text(n).to_string())
            })
            .ok_or_else(|| InterpError::runtime("Await-for loop without a loop variable"))?;
        let sub_key = (id, self.loop_iterations.clone());
        let it = match self.await_cache.get(&sub_key).cloned() {
            Some(Ok(v)) => v,
            Some(Err(e)) => return Err(InterpError::Throw(e)),
            None => {
                let value_field = unit.child_by_field(id, "value").or_else(|| {
                    unit.named_children(id)
                        .into_iter()
                        .find(|&c| unit.kind(c) == "call_expression")
                });
                let value_field = value_field.ok_or_else(|| {
                    InterpError::runtime("Await-for loop without a stream expression")
                })?;
                let iterable = self.visit_expression(unit, value_field)?;
                let (s_state, s_ops) = crate::stdlib::stream_lib::gen_state_parts(&iterable)
                    .ok_or_else(|| {
                        InterpError::runtime(format!(
                            "await for expects a Stream, got {}",
                            value_display(&iterable)
                        ))
                    })?;
                let it = Value::List(Rc::new(RefCell::new(vec![
                    Value::Bool(true),  // 0 started
                    Value::Bool(false), // 1 done
                    Value::Null,        // 2 current
                    Value::Bool(false), // 3 has current
                    Value::Null,        // 4 pending future
                    Value::Null,        // 5 error
                    Value::Null,        // 6 subscription
                    Value::list(),      // 7 overflow queue
                ])));
                let marker = Value::List(Rc::new(RefCell::new(vec![
                    Value::Int(crate::stdlib::stream_lib::I_ITER_KIND),
                    Value::Future(Rc::new(
                        RefCell::new(crate::async_state::FutureState::new()),
                    )),
                    it.clone(),
                ])));
                if std::env::var("D4RT_DBG").is_ok() {
                    eprintln!(
                        "[dbg] subscribing: ptr={:?} listeners {} events {}",
                        Rc::as_ptr(&s_state),
                        crate::stdlib::stream_lib::as_list_rc(
                            &s_state.borrow().get(1).cloned().unwrap_or(Value::Null)
                        )
                        .map(|l| l.borrow().len())
                        .unwrap_or(0),
                        crate::stdlib::stream_lib::as_list_rc(
                            &s_state.borrow().first().cloned().unwrap_or(Value::Null)
                        )
                        .map(|l| l.borrow().len())
                        .unwrap_or(0)
                    );
                }
                let sub = crate::stdlib::stream_lib::gen_subscribe(self, &s_state, &s_ops, marker);
                if std::env::var("D4RT_DBG").is_ok() {
                    eprintln!(
                        "[dbg] after subscribe: listeners {}",
                        crate::stdlib::stream_lib::as_list_rc(
                            &s_state.borrow().get(1).cloned().unwrap_or(Value::Null)
                        )
                        .map(|l| l.borrow().len())
                        .unwrap_or(0)
                    );
                }
                if let Some(l) = crate::stdlib::stream_lib::as_list_rc(&it) {
                    l.borrow_mut()[6] = sub;
                }
                self.await_cache.insert(sub_key, Ok(it.clone()));
                it
            }
        };
        let mut result = Ok(Value::Null);
        let mut iter = 0usize;
        loop {
            self.loop_iterations.push(iter);
            match self.await_for_fetch(unit, id, &it) {
                Ok(Some(v)) => {
                    if std::env::var("D4RT_DBG").is_ok() {
                        eprintln!(
                            "[dbg] await-for body: value={:?} resume={:?}",
                            value_display(&v),
                            self.resume_state
                        );
                    }
                    let env = Environment::child(&self.current_environment);
                    let saved = self.current_environment.clone();
                    self.current_environment = env.clone();
                    Environment::define(&env, &name, v);
                    let body = unit.child_by_field(id, "body");
                    let r = match body {
                        Some(b) => self.visit_statement(unit, b),
                        None => Ok(Value::Null),
                    };
                    self.current_environment = saved;
                    self.loop_iterations.pop();
                    iter += 1;
                    match r {
                        Err(InterpError::Break(_)) => break,
                        Err(InterpError::Continue(_)) => continue,
                        Err(e) => {
                            result = Err(e);
                            break;
                        }
                        Ok(_) => {}
                    }
                }
                Ok(None) => {
                    self.loop_iterations.pop();
                    break;
                }
                Err(e) => {
                    self.loop_iterations.pop();
                    result = Err(e);
                    break;
                }
            }
        }
        if !matches!(result, Err(InterpError::Suspend)) {
            crate::stdlib::stream_lib::gen_cancel_sub(&it);
        }
        result
    }

    /// Fetches the next value of an `await for` iterator: replays cached
    /// results, consumes delivered events, and otherwise suspends the
    /// current async pass until the next event (or done/error) arrives.
    fn await_for_fetch(
        &mut self,
        _unit: &Rc<CompilationUnit>,
        id: NodeId,
        it: &Value,
    ) -> Result<Option<Value>, InterpError> {
        if std::env::var("D4RT_DBG").is_ok() {
            eprintln!(
                "[dbg] fetch id={id} iters={:?} resume={:?}",
                self.loop_iterations, self.resume_state
            );
        }
        if let Some((rn, ri)) = &self.resume_state
            && *rn == id
            && ri == &self.loop_iterations
        {
            self.resume_state = None;
        }
        let cache_key = (id, self.loop_iterations.clone());
        if let Some(cached) = self.await_cache.get(&cache_key).cloned() {
            return match cached {
                Err(e) => Err(InterpError::Throw(e)),
                Ok(v) => Ok(Some(v)),
            };
        }
        let list = crate::stdlib::stream_lib::as_list_rc(it)
            .ok_or_else(|| InterpError::runtime("await-for: bad iterator state"))?;
        let (done, current, has_current, error) = {
            let b = list.borrow();
            (
                b.get(1).cloned().unwrap_or(Value::Bool(false)),
                b.get(2).cloned().unwrap_or(Value::Null),
                b.get(3).cloned().unwrap_or(Value::Bool(false)),
                b.get(5).cloned().unwrap_or(Value::Null),
            )
        };
        if std::env::var("D4RT_DBG").is_ok() {
            eprintln!(
                "[dbg] fetch state: done={} has_current={}",
                is_truthy(&done).unwrap_or(false),
                is_truthy(&has_current).unwrap_or(false)
            );
        }
        if std::env::var("D4RT_DBG").is_ok() {
            eprintln!(
                "[dbg] fetch {}: it={:?} queue={}",
                cache_key.1.len(),
                list.borrow().iter().take(6).collect::<Vec<_>>(),
                list.borrow()
                    .get(7)
                    .and_then(crate::stdlib::stream_lib::as_list_rc)
                    .map(|q| q.borrow().len())
                    .unwrap_or(0)
            );
        }
        if is_truthy(&has_current)? {
            list.borrow_mut()[3] = Value::Bool(false);
            let refill = if list.borrow().len() > 7 {
                let q = list.borrow()[7].clone();
                let q =
                    crate::stdlib::stream_lib::as_list_rc(&q).expect("await-for overflow queue");
                let mut qb = q.borrow_mut();
                let next = qb.first().cloned();
                if next.is_some() {
                    qb.remove(0);
                }
                next
            } else {
                None
            };
            if let Some(next) = refill {
                list.borrow_mut()[2] = next;
                list.borrow_mut()[3] = Value::Bool(true);
            }
            self.await_cache.insert(cache_key, Ok(current.clone()));
            return Ok(Some(current));
        }
        if !error.is_null() {
            return Err(InterpError::Throw(error));
        }
        if is_truthy(&done)? {
            return Ok(None);
        }
        let f = Rc::new(RefCell::new(crate::async_state::FutureState::new()));
        list.borrow_mut()[4] = Value::Future(f.clone());
        let cont = self.make_resume_cont(id)?;
        self.resume_state = None;
        f.borrow_mut()
            .continuations
            .push(crate::async_state::Continuation::Resume(cont));
        Err(InterpError::Suspend)
    }

    fn collect_iterable(&mut self, iterable: &Value) -> Result<Vec<Value>, InterpError> {
        match iterable {
            Value::List(l) => Ok(l.borrow().clone()),
            Value::Set(s) => Ok(s.borrow().items.clone()),
            Value::Str(text) => Ok(text
                .chars()
                .map(|c| Value::str(c.to_string()))
                .collect::<Vec<_>>()),
            Value::SyncGen(g) => {
                let mut items = Vec::new();
                while let Some(v) = self.run_sync_generator(g)? {
                    items.push(v);
                }
                Ok(items)
            }
            other => Err(InterpError::runtime(format!(
                "For-in loop expects an iterable, got {}",
                value_display(other)
            ))),
        }
    }

    fn visit_return(
        &mut self,
        unit: &Rc<CompilationUnit>,
        id: NodeId,
    ) -> Result<Value, InterpError> {
        let expr = unit.named_children(id).first().copied();
        match expr {
            Some(e) => {
                let v = self.visit_expression(unit, e)?;
                Err(InterpError::Return(v))
            }
            None => Err(InterpError::Return(Value::Null)),
        }
    }

    fn visit_break(
        &mut self,
        unit: &Rc<CompilationUnit>,
        id: NodeId,
    ) -> Result<Value, InterpError> {
        let label = unit
            .first_child_of_kind(id, "identifier")
            .map(|n| unit.text(n).to_string());
        Err(InterpError::Break(label))
    }

    fn visit_continue(
        &mut self,
        unit: &Rc<CompilationUnit>,
        id: NodeId,
    ) -> Result<Value, InterpError> {
        let label = unit
            .first_child_of_kind(id, "identifier")
            .map(|n| unit.text(n).to_string());
        Err(InterpError::Continue(label))
    }

    fn visit_labeled(
        &mut self,
        unit: &Rc<CompilationUnit>,
        id: NodeId,
    ) -> Result<Value, InterpError> {
        let label = unit
            .first_child_of_kind(id, "identifier")
            .map(|n| unit.text(n).to_string());
        let stmt = unit
            .named_children(id)
            .iter()
            .rev()
            .find(|&&c| unit.kind(c) != "identifier")
            .copied();
        let Some(stmt) = stmt else {
            return Ok(Value::Null);
        };
        match self.visit_statement(unit, stmt) {
            Err(InterpError::Break(Some(l))) if label.as_deref() == Some(l.as_str()) => {
                Ok(Value::Null)
            }
            Err(InterpError::Continue(Some(l))) if label.as_deref() == Some(l.as_str()) => {
                Ok(Value::Null)
            }
            other => other,
        }
    }

    fn visit_try(&mut self, unit: &Rc<CompilationUnit>, id: NodeId) -> Result<Value, InterpError> {
        let try_block = unit.child_by_field(id, "body");
        let mut catch_handlers: Vec<CatchHandler> = Vec::new();
        let mut finally_block: Option<NodeId> = None;
        let mut pending_type: Option<String> = None;
        let mut pending_exc: Option<String> = None;
        let mut pending_stack: Option<String> = None;

        let children = unit.children(id);
        let try_start = try_block
            .map(|b| children.iter().position(|&c| c == b).unwrap_or(0))
            .unwrap_or(0);
        let mut i = try_start + 1;
        while i < children.len() {
            let child = children[i];
            match unit.kind(child) {
                "type" => {
                    pending_type = Some(self.type_name_text(unit, child));
                }
                "catch_clause" => {
                    pending_exc = unit
                        .child_by_field(child, "exception")
                        .map(|n| unit.text(n).to_string());
                    pending_stack = unit
                        .child_by_field(child, "stack_trace")
                        .map(|n| unit.text(n).to_string());
                }
                "block" => {
                    catch_handlers.push((
                        pending_type.take(),
                        pending_exc.take(),
                        pending_stack.take(),
                        child,
                    ));
                }
                "finally_clause" => {
                    finally_block = unit.first_child_of_kind(child, "block");
                }
                _ => {}
            }
            i += 1;
        }

        let try_result = match try_block {
            Some(b) => {
                // When the resume point sits inside a catch/finally handler
                // (not the try block), replaying the pass must re-execute
                // the try block so a cached error propagates and enters the
                // handler; otherwise its statements would be skipped and
                // the resume node would never run.
                let saved_resume = self.resume_state.clone().filter(|(rn, _)| {
                    self.node_is_ancestor_of(unit, id, *rn)
                        && !self.node_is_ancestor_of(unit, b, *rn)
                });
                if saved_resume.is_some() {
                    self.resume_state = None;
                }
                let result = self.visit_statement(unit, b);
                if let Some(r) = saved_resume {
                    self.resume_state = Some(r);
                }
                result
            }
            None => Ok(Value::Null),
        };

        match try_result {
            Err(InterpError::Throw(value)) => {
                for (on_type, exc_name, stack_name, body) in &catch_handlers {
                    if let Some(t) = on_type {
                        let vtype = runtime_type_of(Some(&value))
                            .map(|t| t.name())
                            .unwrap_or_default();
                        if !type_name_matches(t, &vtype) {
                            continue;
                        }
                    }
                    let env = Environment::child(&self.current_environment);
                    let saved = self.current_environment.clone();
                    self.current_environment = env.clone();
                    self.current_exception = Some(value.clone());
                    if let Some(exc) = exc_name {
                        Environment::define(&env, exc, value.clone());
                    }
                    if let Some(st) = stack_name {
                        let trace = self.stack_trace_text();
                        Environment::define(&env, st, Value::str(trace));
                    }
                    let result = self.visit_statement(unit, *body);
                    self.current_environment = saved;
                    return self.finish_try(unit, finally_block, result);
                }
                self.finish_try(unit, finally_block, Err(InterpError::Throw(value)))
            }
            Err(e @ (InterpError::Runtime(_) | InterpError::Unimplemented(_)))
                if !catch_handlers.is_empty() =>
            {
                let msg = e.to_string();
                for (on_type, exc_name, stack_name, body) in &catch_handlers {
                    if let Some(t) = on_type
                        && !type_name_matches(t, "String")
                    {
                        continue;
                    }
                    let env = Environment::child(&self.current_environment);
                    let saved = self.current_environment.clone();
                    self.current_environment = env.clone();
                    if let Some(exc) = exc_name {
                        Environment::define(&env, exc, Value::str(msg.clone()));
                    }
                    if let Some(st) = stack_name {
                        let trace = self.stack_trace_text();
                        Environment::define(&env, st, Value::str(trace));
                    }
                    let result = self.visit_statement(unit, *body);
                    self.current_environment = saved;
                    return self.finish_try(unit, finally_block, result);
                }
                self.finish_try(unit, finally_block, Err(e))
            }
            Err(e) => self.finish_try(unit, finally_block, Err(e)),
            Ok(v) => self.finish_try(unit, finally_block, Ok(v)),
        }
    }

    fn finish_try(
        &mut self,
        unit: &Rc<CompilationUnit>,
        finally_block: Option<NodeId>,
        result: Result<Value, InterpError>,
    ) -> Result<Value, InterpError> {
        if let Some(fb) = finally_block {
            self.visit_statement(unit, fb)?;
        }
        result
    }

    fn visit_yield(
        &mut self,
        unit: &Rc<CompilationUnit>,
        id: NodeId,
    ) -> Result<Value, InterpError> {
        let cache_key = (id, self.loop_iterations.clone());
        if self
            .async_frames
            .last()
            .and_then(|f| f.stream_state.clone())
            .is_some()
        {
            if let Some((rn, ri)) = &self.resume_state
                && *rn == id
                && ri == &self.loop_iterations
            {
                self.resume_state = None;
            }
            if let Some(cached) = self.await_cache.get(&cache_key).cloned() {
                if std::env::var("D4RT_DBG").is_ok() {
                    eprintln!(
                        "[dbg] yield {} CACHED {:?}",
                        id,
                        cached.as_ref().map(|v| (v.clone(),))
                    );
                }
                return match cached {
                    Err(e) => Err(InterpError::Throw(e)),
                    Ok(_) => Ok(Value::Null),
                };
            }
            let expr = unit.named_children(id).first().copied();
            let value = match expr {
                Some(e) => self.visit_expression(unit, e)?,
                None => Value::Null,
            };
            let state = self
                .async_frames
                .last()
                .and_then(|f| f.stream_state.clone())
                .expect("async* frame");
            crate::stdlib::stream_lib::gen_emit(&state, value.clone());
            self.await_cache.insert(cache_key, Ok(value));
            let cont = self.make_resume_cont(id)?;
            self.resume_state = None;
            crate::async_state::enqueue_task(crate::async_state::Task::Resume(cont));
            return Err(InterpError::Suspend);
        }
        let expr = unit.named_children(id).first().copied();
        let value = match expr {
            Some(e) => self.visit_expression(unit, e)?,
            None => Value::Null,
        };
        match &self.sync_generator {
            Some(g) => {
                g.borrow_mut().buffer.push(value);
                Ok(Value::Null)
            }
            None => Err(InterpError::runtime(
                "Yield statement used outside a generator function",
            )),
        }
    }

    fn visit_yield_each(
        &mut self,
        unit: &Rc<CompilationUnit>,
        id: NodeId,
    ) -> Result<Value, InterpError> {
        let expr = unit.named_children(id).first().copied();
        let Some(e) = expr else {
            return Err(InterpError::runtime("yield* without expression"));
        };
        if self
            .async_frames
            .last()
            .and_then(|f| f.stream_state.clone())
            .is_some()
        {
            let cache_key = (id, self.loop_iterations.clone());
            if let Some((rn, ri)) = &self.resume_state
                && *rn == id
                && ri == &self.loop_iterations
            {
                self.resume_state = None;
            }
            if let Some(cached) = self.await_cache.get(&cache_key).cloned() {
                return match cached {
                    Err(e) => Err(InterpError::Throw(e)),
                    Ok(_) => Ok(Value::Null),
                };
            }
            let value = self.visit_expression(unit, e)?;
            let state = self
                .async_frames
                .last()
                .and_then(|f| f.stream_state.clone())
                .expect("async* frame");
            let (s_state, s_ops) =
                crate::stdlib::stream_lib::gen_state_parts(&value).ok_or_else(|| {
                    InterpError::runtime(format!(
                        "yield* expects a Stream, got {}",
                        value_display(&value)
                    ))
                })?;
            let marker = Value::List(Rc::new(RefCell::new(vec![
                Value::Int(crate::stdlib::stream_lib::I_GEN_FWD_KIND),
                Value::Null,
                state,
                Value::Bool(true),
            ])));
            let key = crate::stdlib::stream_lib::list_ptr(&marker)
                .ok_or_else(|| InterpError::runtime("yield*: bad marker"))?;
            self.await_cache.insert(cache_key, Ok(Value::Null));
            let cont = self.make_resume_cont(id)?;
            crate::stdlib::stream_lib::stash_yield_star(key, cont);
            crate::stdlib::stream_lib::gen_subscribe(self, &s_state, &s_ops, marker);
            self.resume_state = None;
            return Err(InterpError::Suspend);
        }
        let value = self.visit_expression(unit, e)?;
        let Some(generator) = self.sync_generator.clone() else {
            return Err(InterpError::runtime(
                "Yield statement used outside a generator function",
            ));
        };
        match &value {
            Value::SyncGen(inner) => {
                while let Some(v) = self.run_sync_generator(inner)? {
                    generator.borrow_mut().buffer.push(v);
                }
                Ok(Value::Null)
            }
            Value::List(items) => {
                let items = items.borrow().clone();
                let mut buffer = generator.borrow_mut().buffer.clone();
                buffer.extend(items);
                generator.borrow_mut().buffer = buffer;
                Ok(Value::Null)
            }
            Value::Set(s) => {
                let items = s.borrow().items.clone();
                let mut buffer = generator.borrow_mut().buffer.clone();
                buffer.extend(items);
                generator.borrow_mut().buffer = buffer;
                Ok(Value::Null)
            }
            other => Err(InterpError::runtime(format!(
                "yield* expects an iterable, got {}",
                value_display(other)
            ))),
        }
    }

    fn visit_assert(
        &mut self,
        unit: &Rc<CompilationUnit>,
        id: NodeId,
    ) -> Result<Value, InterpError> {
        let mut exprs = unit.named_children(id);
        if exprs.first().is_some_and(|&c| unit.kind(c) == "assertion") {
            exprs = unit.named_children(exprs[0]);
        }
        let Some(&cond_node) = exprs.first() else {
            return Ok(Value::Null);
        };
        let cond = self.visit_expression(unit, cond_node)?;
        if !is_truthy(&cond)? {
            let msg = match exprs.get(1) {
                Some(&m) => value_display(&self.visit_expression(unit, m)?),
                None => "".to_string(),
            };
            return Err(InterpError::runtime(if msg.is_empty() {
                "Assertion failed".to_string()
            } else {
                format!("Assertion failed: {msg}")
            }));
        }
        Ok(Value::Null)
    }

    fn visit_switch(
        &mut self,
        unit: &Rc<CompilationUnit>,
        id: NodeId,
    ) -> Result<Value, InterpError> {
        let cond = self.eval_condition_value(unit, id)?;
        let body = unit.child_by_field(id, "body");
        let Some(body) = body else {
            return Ok(Value::Null);
        };
        let sections = unit.children(body);
        let mut section_info: Vec<(Option<NodeId>, Vec<NodeId>)> = Vec::new();
        for &child in sections {
            let kind = unit.kind(child);
            if kind == "switch_statement_case" {
                let value_node = unit
                    .first_child_of_kind(child, "constant_pattern")
                    .and_then(|cp| unit.named_children(cp).first().copied())
                    .or_else(|| unit.first_child_of_kind(child, "expression"));
                let stmts: Vec<NodeId> = unit
                    .named_children(child)
                    .into_iter()
                    .filter(|&c| unit.kind(c) != "constant_pattern" && unit.kind(c) != "label")
                    .collect();
                section_info.push((value_node, stmts));
            } else if kind == "switch_statement_default" {
                let stmts: Vec<NodeId> = unit
                    .named_children(child)
                    .into_iter()
                    .filter(|&c| unit.kind(c) != "label")
                    .collect();
                section_info.push((None, stmts));
            }
        }

        let mut default_section: Option<Vec<NodeId>> = None;
        for (val, stmts) in &section_info {
            let matched = match val {
                Some(v) => {
                    let case_val = self.visit_expression(unit, *v)?;
                    value_eq(&case_val, &cond)
                }
                None => {
                    default_section = Some(stmts.clone());
                    false
                }
            };
            if matched {
                let result = self.run_switch_section(unit, stmts);
                match result {
                    Err(InterpError::Break(_)) | Ok(_) => return Ok(Value::Null),
                    Err(e) => return Err(e),
                }
            }
        }
        if let Some(stmts) = default_section {
            let result = self.run_switch_section(unit, &stmts);
            match result {
                Err(InterpError::Break(_)) | Ok(_) => return Ok(Value::Null),
                Err(e) => return Err(e),
            }
        }
        Ok(Value::Null)
    }

    fn run_switch_section(
        &mut self,
        unit: &Rc<CompilationUnit>,
        stmts: &[NodeId],
    ) -> Result<Value, InterpError> {
        for &s in stmts {
            self.visit_statement(unit, s)?;
        }
        Ok(Value::Null)
    }

    fn eval_condition_field(
        &mut self,
        unit: &Rc<CompilationUnit>,
        id: NodeId,
    ) -> Result<bool, InterpError> {
        match unit.child_by_field(id, "condition") {
            Some(cond) => self.visit_truthy_condition(unit, cond),
            None => Ok(true),
        }
    }

    fn visit_truthy_condition(
        &mut self,
        unit: &Rc<CompilationUnit>,
        node: NodeId,
    ) -> Result<bool, InterpError> {
        let v = self.visit_expression(unit, node)?;
        is_truthy(&v)
    }

    fn eval_condition_value(
        &mut self,
        unit: &Rc<CompilationUnit>,
        id: NodeId,
    ) -> Result<Value, InterpError> {
        match unit.child_by_field(id, "condition") {
            Some(cond) => self.visit_expression(unit, cond),
            None => Ok(Value::Null),
        }
    }

    fn stack_trace_text(&self) -> String {
        self.stack_trace
            .iter()
            .map(|(name, line, col)| format!("#{name} at {line}:{col}"))
            .collect::<Vec<_>>()
            .join("\n")
    }

    // =========================================================================
    // Function creation
    // =========================================================================

    fn define_function(
        &mut self,
        unit: &Rc<CompilationUnit>,
        id: NodeId,
    ) -> Result<Value, InterpError> {
        let f = self.build_function_from_declaration(unit, id)?;
        let name = f.name.clone().unwrap_or_default();
        Environment::define(&self.current_environment, &name, Value::Function(f));
        Ok(Value::Null)
    }

    // =========================================================================
    // Declaration pass (classes, enums, typedefs)
    // =========================================================================

    fn define_class(
        &mut self,
        unit: &Rc<CompilationUnit>,
        id: NodeId,
    ) -> Result<Value, InterpError> {
        let name = unit
            .child_by_field(id, "name")
            .map(|n| unit.text(n).to_string())
            .ok_or_else(|| InterpError::runtime("class_declaration without a name"))?;

        let mut type_parameter_names = Vec::new();
        let mut type_parameter_bounds = HashMap::new();
        if let Some(tp) = unit.child_by_field(id, "type_parameters") {
            for child in unit.named_children(tp) {
                if unit.kind(child) == "type_parameter" {
                    let tname = unit
                        .child_by_field(child, "name")
                        .map(|n| unit.text(n).to_string())
                        .unwrap_or_default();
                    let bound = unit
                        .child_by_field(child, "bound")
                        .map(|b| self.parse_type(unit, b));
                    type_parameter_bounds.insert(tname.clone(), bound);
                    type_parameter_names.push(tname);
                }
            }
        }

        let class_env = Environment::child(&self.current_environment);
        let mut class_def = ClassDef::new(
            name.clone(),
            unit.clone(),
            class_env,
            false,
            false,
            false,
            false,
            false,
            false,
            type_parameter_names,
            type_parameter_bounds,
        );

        if let Some(superclass) = unit.child_by_field(id, "superclass")
            && let Some(t) = unit.child_by_field(superclass, "type")
        {
            let rt = self.parse_type(unit, t);
            match &*rt {
                RuntimeType::Class(c) => {
                    Rc::get_mut(&mut class_def).unwrap().superclass = Some(c.clone());
                }
                RuntimeType::Native(n) => {
                    Rc::get_mut(&mut class_def).unwrap().bridged_superclass = Some(n.clone());
                }
                _ => {}
            }
        }
        for field in ["interfaces", "mixins"] {
            if let Some(clause) = unit.child_by_field(id, field) {
                let types: Vec<NodeId> = unit.children_of_kind(clause, "type");
                for t in types {
                    let rt = self.parse_type(unit, t);
                    match &*rt {
                        RuntimeType::Class(c) => {
                            if field == "interfaces" {
                                Rc::get_mut(&mut class_def)
                                    .unwrap()
                                    .interfaces
                                    .borrow_mut()
                                    .push(c.clone());
                            } else {
                                Rc::get_mut(&mut class_def)
                                    .unwrap()
                                    .mixins
                                    .borrow_mut()
                                    .push(c.clone());
                            }
                        }
                        RuntimeType::Native(n) => {
                            if field == "interfaces" {
                                Rc::get_mut(&mut class_def)
                                    .unwrap()
                                    .bridged_interfaces
                                    .borrow_mut()
                                    .push(n.clone());
                            } else {
                                Rc::get_mut(&mut class_def)
                                    .unwrap()
                                    .bridged_mixins
                                    .borrow_mut()
                                    .push(n.clone());
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
        Environment::define(
            &self.current_environment,
            &name,
            Value::Class(class_def.clone()),
        );
        Environment::define(
            &class_def.class_definition_environment,
            &name,
            Value::Class(class_def.clone()),
        );

        let saved = self.current_environment.clone();
        self.current_environment = class_def.class_definition_environment.clone();
        let result = self.collect_class_members(unit, id, &class_def);
        self.current_environment = saved;
        result?;

        Ok(Value::Null)
    }

    fn collect_class_members(
        &mut self,
        unit: &Rc<CompilationUnit>,
        id: NodeId,
        class_def: &Rc<ClassDef>,
    ) -> Result<(), InterpError> {
        let body = unit.child_by_field(id, "body");
        let Some(body) = body else {
            return Ok(());
        };
        for member in unit.children_of_kind(body, "class_member") {
            for child in unit.named_children(member) {
                match unit.kind(child) {
                    "method_declaration" => {
                        let sig = unit.child_by_field(child, "signature");
                        let body = unit.child_by_field(child, "body");
                        let (name, return_type, parameters, getter, setter, factory) =
                            self.parse_signature(unit, sig)?;
                        let Some(name) = name else {
                            continue;
                        };
                        let is_constructor = sig
                            .map(|s| {
                                !unit.children_of_kind(s, "constructor_signature").is_empty()
                                    || !unit
                                        .children_of_kind(s, "constant_constructor_signature")
                                        .is_empty()
                                    || !unit
                                        .children_of_kind(s, "factory_constructor_signature")
                                        .is_empty()
                            })
                            .unwrap_or(false);
                        if is_constructor {
                            let mut f = self.build_function(
                                unit,
                                Some(name.clone()),
                                parameters,
                                return_type,
                                body,
                                getter,
                                setter,
                                factory,
                                false,
                                false,
                            );
                            if let Some(f_mut) = Rc::get_mut(&mut f) {
                                f_mut.is_constructor = true;
                                f_mut.initializers = unit
                                    .child_by_field(child, "initializers")
                                    .or_else(|| {
                                        unit.children_of_kind(child, "initializers")
                                            .first()
                                            .copied()
                                    })
                                    .or_else(|| {
                                        sig.and_then(|s| {
                                            unit.children_of_kind(s, "initializers")
                                                .first()
                                                .copied()
                                        })
                                    })
                                    .or_else(|| {
                                        unit.children_of_kind(child, "redirection").first().copied()
                                    });
                            }
                            class_def.constructors.borrow_mut().insert(name, f);
                            continue;
                        }
                        let is_static = sig
                            .map(|s| !unit.children_of_kind(s, "static").is_empty())
                            .unwrap_or(false);
                        let is_operator = sig
                            .map(|s| unit.first_child_of_kind(s, "operator_signature").is_some())
                            .unwrap_or(false);
                        let f = self.build_function(
                            unit,
                            Some(name.clone()),
                            parameters,
                            return_type,
                            body,
                            getter,
                            setter,
                            factory,
                            false,
                            false,
                        );
                        if is_operator {
                            class_def.operators.borrow_mut().insert(name, f);
                        } else if is_static && getter {
                            class_def.static_getters.borrow_mut().insert(name, f);
                        } else if is_static && setter {
                            class_def.static_setters.borrow_mut().insert(name, f);
                        } else if is_static {
                            class_def.static_methods.borrow_mut().insert(name, f);
                        } else if getter {
                            class_def.getters.borrow_mut().insert(name, f);
                        } else if setter {
                            class_def.setters.borrow_mut().insert(name, f);
                        } else {
                            class_def.methods.borrow_mut().insert(name, f);
                        }
                    }
                    "declaration" => {
                        let sig = unit
                            .first_child_of_kind(child, "constructor_signature")
                            .or_else(|| {
                                unit.first_child_of_kind(child, "constant_constructor_signature")
                            })
                            .or_else(|| {
                                unit.first_child_of_kind(child, "factory_constructor_signature")
                            })
                            .or_else(|| {
                                unit.first_child_of_kind(
                                    child,
                                    "redirecting_factory_constructor_signature",
                                )
                            });
                        match sig {
                            Some(sig_node) => {
                                let (name, return_type, parameters, _g, _s, factory) =
                                    self.parse_signature(unit, Some(sig_node))?;
                                let ctor_name = name.unwrap_or_default();
                                let body = unit.child_by_field(child, "body");
                                if unit
                                    .first_child_of_kind(
                                        child,
                                        "redirecting_factory_constructor_signature",
                                    )
                                    .is_some()
                                {
                                    let target = unit
                                        .child_by_field(sig_node, "target")
                                        .map(|t| unit.text(t).to_string())
                                        .unwrap_or_default();
                                    let (target_class, target_ctor) = match target.split_once('.') {
                                        Some((c, ct)) => (c.to_string(), ct.to_string()),
                                        None => (target.clone(), "".to_string()),
                                    };
                                    self.register_factory_redirect(
                                        class_def,
                                        &ctor_name,
                                        &target_class,
                                        &target_ctor,
                                    )?;
                                    let mut f = self.build_function(
                                        unit,
                                        Some(ctor_name),
                                        parameters,
                                        return_type,
                                        None,
                                        false,
                                        false,
                                        factory,
                                        false,
                                        false,
                                    );
                                    if let Some(f_mut) = Rc::get_mut(&mut f) {
                                        f_mut.is_constructor = true;
                                    }
                                    class_def
                                        .constructors
                                        .borrow_mut()
                                        .insert(f.name.clone().unwrap(), f);
                                } else {
                                    let mut f = self.build_function(
                                        unit,
                                        Some(ctor_name.clone()),
                                        parameters,
                                        return_type,
                                        body,
                                        false,
                                        false,
                                        factory,
                                        false,
                                        false,
                                    );
                                    if let Some(f_mut) = Rc::get_mut(&mut f) {
                                        f_mut.is_constructor = true;
                                        f_mut.initializers = unit
                                            .child_by_field(child, "initializers")
                                            .or_else(|| {
                                                unit.children_of_kind(child, "initializers")
                                                    .first()
                                                    .copied()
                                            })
                                            .or_else(|| {
                                                unit.children_of_kind(child, "redirection")
                                                    .first()
                                                    .copied()
                                            });
                                    }
                                    class_def.constructors.borrow_mut().insert(ctor_name, f);
                                }
                            }
                            None => {
                                let is_static = !unit.children_of_kind(child, "static").is_empty();
                                let is_late = !unit.children_of_kind(child, "late").is_empty();
                                let is_final = !unit.children_of_kind(child, "final").is_empty();
                                // tree-sitter-dart splits the name list by
                                // modifier: `static const` / `static final`
                                // declarations use `static_final_declaration_list`
                                // (children `static_final_declaration`), plain
                                // `static` uses `initialized_identifier_list`
                                // (children `initialized_identifier`). Register
                                // both so `static const` and `static final`
                                // fields resolve like plain static ones.
                                for list_kind in [
                                    "initialized_identifier_list",
                                    "static_final_declaration_list",
                                ] {
                                    let Some(list) = unit.first_child_of_kind(child, list_kind)
                                    else {
                                        continue;
                                    };
                                    let init_kind = if list_kind == "static_final_declaration_list"
                                    {
                                        "static_final_declaration"
                                    } else {
                                        "initialized_identifier"
                                    };
                                    for init in unit.children_of_kind(list, init_kind) {
                                        let fname = unit
                                            .child_by_field(init, "name")
                                            .map(|n| unit.text(n).to_string())
                                            .unwrap_or_default();
                                        let initializer = unit.child_by_field(init, "value");
                                        if is_static {
                                            let value = match initializer {
                                                Some(v) => {
                                                    if is_late {
                                                        Value::Late(Rc::new(RefCell::new(
                                                            LateVariable {
                                                                is_initialized: false,
                                                                value: Value::Null,
                                                                initializer: None,
                                                                name: fname.clone(),
                                                                is_final,
                                                            },
                                                        )))
                                                    } else {
                                                        self.visit_expression(unit, v)?
                                                    }
                                                }
                                                None => Value::Null,
                                            };
                                            class_def.set_static_field(&fname, value);
                                        } else {
                                            class_def.field_declarations.borrow_mut().push(init);
                                        }
                                    }
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        Ok(())
    }

    fn register_factory_redirect(
        &mut self,
        class_def: &Rc<ClassDef>,
        ctor_name: &str,
        target_class: &str,
        target_ctor: &str,
    ) -> Result<(), InterpError> {
        let target = Environment::get(&self.current_environment, target_class)?;
        let Value::Class(tc) = target else {
            return Err(InterpError::runtime(format!(
                "Redirecting factory target '{target_class}' is not a class"
            )));
        };
        let tc = if tc.name == class_def.name {
            class_def.clone()
        } else {
            tc
        };
        class_def.factory_redirects.borrow_mut().insert(
            ctor_name.to_string(),
            FactoryRedirect {
                target_class: tc,
                target_ctor: target_ctor.to_string(),
            },
        );
        Ok(())
    }

    fn define_enum(
        &mut self,
        unit: &Rc<CompilationUnit>,
        id: NodeId,
    ) -> Result<Value, InterpError> {
        let name = unit
            .child_by_field(id, "name")
            .map(|n| unit.text(n).to_string())
            .ok_or_else(|| InterpError::runtime("enum_declaration without a name"))?;
        let body = unit.child_by_field(id, "body");
        let mut constants: Vec<NodeId> = Vec::new();
        let mut value_names: Vec<String> = Vec::new();
        let mut member_nodes: Vec<NodeId> = Vec::new();
        if let Some(body) = body {
            for child in unit.named_children(body) {
                match unit.kind(child) {
                    "enum_constant" => {
                        let vname = unit
                            .child_by_field(child, "name")
                            .map(|n| unit.text(n).to_string())
                            .unwrap_or_default();
                        constants.push(child);
                        value_names.push(vname);
                    }
                    "class_member" => member_nodes.push(child),
                    _ => {}
                }
            }
        }
        let enum_def = EnumDef::placeholder(
            name.clone(),
            unit.clone(),
            self.current_environment.clone(),
            value_names,
        );
        Environment::define(
            &self.current_environment,
            &name,
            Value::EnumType(enum_def.clone()),
        );

        let mut field_names: Vec<String> = Vec::new();
        let mut ctor_formals: Vec<String> = Vec::new();
        for member in &member_nodes {
            for member_child in unit.named_children(*member) {
                if unit.kind(member_child) == "declaration" {
                    if let Some(il) =
                        unit.first_child_of_kind(member_child, "initialized_identifier_list")
                    {
                        for n in unit.named_children(il) {
                            if let Some(fid) = unit.child_by_field(n, "name") {
                                field_names.push(unit.text(fid).to_string());
                            }
                        }
                    }
                    let cs = unit
                        .named_children(member_child)
                        .into_iter()
                        .find(|&c| unit.kind(c).ends_with("constructor_signature"));
                    if let Some(cs) = cs
                        && let Some(pl) = unit.child_by_field(cs, "parameters")
                    {
                        for fp in unit.named_children(pl) {
                            if let Some(cp) = unit.first_child_of_kind(fp, "constructor_param")
                                && let Some(fid) = unit.nodes[cp]
                                    .children
                                    .iter()
                                    .copied()
                                    .skip_while(|&c| unit.kind(c) != "this")
                                    .skip(1)
                                    .find(|&c| unit.kind(c) == "identifier")
                            {
                                ctor_formals.push(unit.text(fid).to_string());
                            }
                        }
                    }
                }
            }
        }

        for (i, &c) in constants.iter().enumerate() {
            let vname = unit
                .child_by_field(c, "name")
                .map(|n| unit.text(n).to_string())
                .unwrap_or_default();
            let data = Rc::new(RefCell::new(EnumValueData {
                parent_enum: enum_def.clone(),
                name: vname.clone(),
                index: i,
                fields: HashMap::new(),
            }));
            let mut arg_values: Vec<Value> = Vec::new();
            if let Some(args) = unit.first_child_of_kind(c, "arguments") {
                for a in unit.named_children(args) {
                    arg_values.push(self.visit_expression(unit, a)?);
                }
            }
            for (fi, fname) in ctor_formals.iter().enumerate() {
                if let Some(v) = arg_values.get(fi) {
                    data.borrow_mut().fields.insert(fname.clone(), v.clone());
                }
            }
            for fname in &field_names {
                if !ctor_formals.contains(fname) {
                    data.borrow_mut().fields.insert(fname.clone(), Value::Null);
                }
            }
            enum_def.values.borrow_mut().insert(vname, data);
        }
        for member in member_nodes {
            for member_child in unit.named_children(member) {
                if unit.kind(member_child) == "method_declaration" {
                    let sig = unit.child_by_field(member_child, "signature");
                    let body = unit.child_by_field(member_child, "body");
                    let (mname, return_type, parameters, getter, setter, _factory) =
                        self.parse_signature(unit, sig)?;
                    let Some(mname) = mname else { continue };
                    let f = self.build_function(
                        unit,
                        Some(mname.clone()),
                        parameters,
                        return_type,
                        body,
                        getter,
                        setter,
                        false,
                        false,
                        false,
                    );
                    if getter {
                        enum_def.getters.borrow_mut().insert(mname, f);
                    } else if setter {
                        enum_def.setters.borrow_mut().insert(mname, f);
                    } else {
                        enum_def.methods.borrow_mut().insert(mname, f);
                    }
                }
            }
        }
        Ok(Value::Null)
    }

    fn define_typedef(
        &mut self,
        unit: &Rc<CompilationUnit>,
        id: NodeId,
    ) -> Result<Value, InterpError> {
        let name = unit
            .first_child_of_kind(id, "type_identifier")
            .or_else(|| unit.first_child_of_kind(id, "identifier"))
            .map(|n| unit.text(n).to_string())
            .unwrap_or_default();
        let t = match unit.first_child_of_kind(id, "type") {
            Some(t) => self.parse_type(unit, t),
            None => RuntimeType::dynamic(),
        };
        Environment::define(&self.current_environment, &name, Value::Type(t));
        Ok(Value::Null)
    }

    pub fn build_function_from_declaration(
        &mut self,
        unit: &Rc<CompilationUnit>,
        id: NodeId,
    ) -> Result<Rc<InterpretedFunction>, InterpError> {
        let signature = unit.child_by_field(id, "signature").or_else(|| {
            unit.named_children(id)
                .into_iter()
                .find(|&c| unit.kind(c).ends_with("_signature"))
        });
        let body = unit.child_by_field(id, "body").or_else(|| {
            unit.named_children(id)
                .into_iter()
                .find(|&c| unit.kind(c) == "function_body")
        });
        let (name, return_type, parameters, getter, setter, factory) =
            self.parse_signature(unit, signature)?;
        let f = self.build_function(
            unit,
            name,
            parameters,
            return_type,
            body,
            getter,
            setter,
            factory,
            false,
            false,
        );
        Ok(f)
    }

    fn parse_signature(
        &mut self,
        unit: &Rc<CompilationUnit>,
        signature: Option<NodeId>,
    ) -> Result<ParsedSignature, InterpError> {
        let Some(sig) = signature else {
            return Ok((None, None, vec![], false, false, false));
        };
        let sigs: Vec<NodeId> = if matches!(
            unit.kind(sig),
            "function_signature"
                | "getter_signature"
                | "setter_signature"
                | "operator_signature"
                | "constructor_signature"
                | "constant_constructor_signature"
                | "factory_constructor_signature"
                | "redirecting_factory_constructor_signature"
        ) {
            vec![sig]
        } else {
            unit.named_children(sig)
        };
        for child in sigs {
            match unit.kind(child) {
                "function_signature" => {
                    let name = unit
                        .child_by_field(child, "name")
                        .map(|n| unit.text(n).to_string());
                    let ret = unit
                        .child_by_field(child, "return_type")
                        .map(|t| self.parse_type(unit, t));
                    let params = match unit.child_by_field(child, "parameters") {
                        Some(p) => self.parse_parameter_list(unit, p)?,
                        None => vec![],
                    };
                    return Ok((name, ret, params, false, false, false));
                }
                "getter_signature" => {
                    let name = unit
                        .child_by_field(child, "name")
                        .map(|n| unit.text(n).to_string());
                    let ret = unit
                        .child_by_field(child, "return_type")
                        .map(|t| self.parse_type(unit, t));
                    return Ok((name, ret, vec![], true, false, false));
                }
                "setter_signature" => {
                    let name = unit
                        .child_by_field(child, "name")
                        .map(|n| unit.text(n).to_string());
                    let params = match unit.child_by_field(child, "parameters") {
                        Some(p) => self.parse_parameter_list(unit, p)?,
                        None => vec![],
                    };
                    return Ok((name, None, params, false, true, false));
                }
                "operator_signature" => {
                    let text = unit.text(child);
                    let op = extract_operator_name(text);
                    let params = self.parse_parameter_list(unit, child)?;
                    return Ok((Some(op), None, params, false, false, false));
                }
                "constructor_signature"
                | "constant_constructor_signature"
                | "factory_constructor_signature"
                | "redirecting_factory_constructor_signature" => {
                    let idents: Vec<NodeId> = unit
                        .named_children(child)
                        .into_iter()
                        .filter(|&c| unit.kind(c) == "identifier")
                        .collect();
                    let name = idents
                        .get(1)
                        .map(|&i| unit.text(i).to_string())
                        .unwrap_or_default();
                    let params = match unit.child_by_field(child, "parameters") {
                        Some(p) => self.parse_parameter_list(unit, p)?,
                        None => vec![],
                    };
                    let factory = unit.kind(child).starts_with("factory");
                    return Ok((Some(name), None, params, false, false, factory));
                }
                _ => {}
            }
        }
        Ok((None, None, vec![], false, false, false))
    }

    #[allow(clippy::too_many_arguments)]
    pub fn build_function(
        &mut self,
        unit: &Rc<CompilationUnit>,
        name: Option<String>,
        parameters: Vec<Parameter>,
        return_type: Option<Rc<RuntimeType>>,
        body: Option<NodeId>,
        is_getter: bool,
        is_setter: bool,
        is_factory: bool,
        is_abstract: bool,
        is_external: bool,
    ) -> Rc<InterpretedFunction> {
        let callable_runtime_type = make_callable_runtime_type(return_type.clone(), &parameters);
        let Some(body) = body else {
            let f = InterpretedFunction {
                unit: unit.clone(),
                name,
                parameters,
                body: 0,
                return_type,
                environment: self.current_environment.clone(),
                is_async: false,
                is_generator: false,
                is_abstract,
                is_external,
                is_getter,
                is_setter,
                is_factory,
                is_constructor: false,
                initializers: None,
                bound_instance: None,
                bound_enum_value: None,
                callable_runtime_type,
            };
            return Rc::new(f);
        };
        let body_text = unit.text(body);
        let (is_async, is_generator) = function_body_flags(body_text);
        let f = InterpretedFunction {
            unit: unit.clone(),
            name,
            parameters,
            body,
            return_type,
            environment: self.current_environment.clone(),
            is_async,
            is_generator,
            is_abstract,
            is_external,
            is_getter,
            is_setter,
            is_factory,
            is_constructor: false,
            initializers: None,
            bound_instance: None,
            bound_enum_value: None,
            callable_runtime_type,
        };
        Rc::new(f)
    }

    fn parse_parameter_list(
        &mut self,
        unit: &Rc<CompilationUnit>,
        id: NodeId,
    ) -> Result<Vec<Parameter>, InterpError> {
        let mut params: Vec<Parameter> = Vec::new();
        for child in unit.named_children(id) {
            match unit.kind(child) {
                "formal_parameter" => {
                    params.push(self.parse_formal_parameter(unit, child, false, false, false)?);
                }
                "optional_formal_parameters" => {
                    let text = unit.text(child);
                    let is_named = text.starts_with('{');
                    let mut next_required = false;
                    for &c in unit.children(child) {
                        let kind = unit.kind(c);
                        if kind == "required" {
                            next_required = true;
                        } else if kind == "formal_parameter" {
                            let p = self.parse_formal_parameter(
                                unit,
                                c,
                                !is_named,
                                is_named,
                                if is_named { next_required } else { false },
                            )?;
                            next_required = false;
                            params.push(p);
                        } else if unit.node(c).is_named && kind != "comment" {
                            // Default value expression (e.g. decimal_integer_literal).
                            if let Some(last) = params.last_mut() {
                                last.has_default = true;
                                last.default = Some(c);
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        Ok(params)
    }

    fn parse_formal_parameter(
        &mut self,
        unit: &Rc<CompilationUnit>,
        id: NodeId,
        is_optional_positional: bool,
        is_named: bool,
        is_required: bool,
    ) -> Result<Parameter, InterpError> {
        let name = match unit.child_by_field(id, "name") {
            Some(n) => unit.text(n).to_string(),
            None => {
                for child in unit.named_children(id) {
                    match unit.kind(child) {
                        "super_formal_parameter" | "constructor_param" => {
                            if let Some(n) = unit.child_by_field(child, "name") {
                                let field = unit.text(n).to_string();
                                let prefix = if unit.kind(child) == "super_formal_parameter" {
                                    "super."
                                } else {
                                    "this."
                                };
                                return Ok(Parameter {
                                    name: format!("{prefix}{field}"),
                                    is_optional_positional,
                                    is_named,
                                    is_required,
                                    type_: unit
                                        .child_by_field(child, "type")
                                        .map(|t| self.parse_type(unit, t)),
                                    has_default: false,
                                    default: None,
                                });
                            }
                            let last_ident = unit
                                .named_children(child)
                                .iter()
                                .rfind(|&&c| unit.kind(c) == "identifier")
                                .copied();
                            if let Some(n) = last_ident {
                                let field = unit.text(n).to_string();
                                let prefix = if unit.kind(child) == "super_formal_parameter" {
                                    "super."
                                } else {
                                    "this."
                                };
                                return Ok(Parameter {
                                    name: format!("{prefix}{field}"),
                                    is_optional_positional,
                                    is_named,
                                    is_required,
                                    type_: unit
                                        .child_by_field(child, "type")
                                        .map(|t| self.parse_type(unit, t)),
                                    has_default: false,
                                    default: None,
                                });
                            }
                        }
                        _ => {}
                    }
                }
                if let Some(n) = unit.first_child_of_kind(id, "identifier") {
                    // Bare identifier parameter (e.g. lambda `(x) => ...`).
                    unit.text(n).to_string()
                } else {
                    return Err(InterpError::runtime("Malformed formal parameter"));
                }
            }
        };
        let type_ = unit
            .first_child_of_kind(id, "type")
            .map(|t| self.parse_type(unit, t));
        Ok(Parameter {
            name,
            is_optional_positional,
            is_named,
            is_required,
            type_,
            has_default: false,
            default: None,
        })
    }

    pub fn parse_type(&mut self, unit: &Rc<CompilationUnit>, id: NodeId) -> Rc<RuntimeType> {
        for child in unit.named_children(id) {
            match unit.kind(child) {
                "void_type" => return RuntimeType::void(),
                "function_type" => return RuntimeType::named("Function"),
                "record_type" => return RuntimeType::dynamic(),
                _ => {}
            }
        }
        let base = unit
            .first_child_of_kind(id, "type_identifier")
            .or_else(|| unit.first_child_of_kind(id, "qualified"))
            .or_else(|| unit.first_child_of_kind(id, "identifier"))
            .or_else(|| unit.first_child_of_kind(id, "type"));
        let type_args = unit.first_child_of_kind(id, "type_arguments");
        let base_text = match base {
            Some(b) => unit.text(b).to_string(),
            None => unit.text(id).to_string(),
        };
        let base_type = self.resolve_type_name(&base_text);
        match type_args {
            Some(ta) => {
                let args: Vec<Rc<RuntimeType>> = unit
                    .children_of_kind(ta, "type")
                    .iter()
                    .map(|&a| self.parse_type(unit, a))
                    .collect();
                Rc::new(RuntimeType::Applied {
                    base: base_type,
                    args,
                })
            }
            None => base_type,
        }
    }

    fn resolve_type_name(&mut self, name: &str) -> Rc<RuntimeType> {
        if let Ok(v) = Environment::get(&self.current_environment, name) {
            match v {
                Value::Class(c) => return Rc::new(RuntimeType::Class(c)),
                Value::EnumType(e) => return Rc::new(RuntimeType::Enum(e)),
                Value::BridgedClass(b) => return Rc::new(RuntimeType::Native(b)),
                Value::ExtType(e) => return Rc::new(RuntimeType::ExtType(e)),
                _ => {}
            }
        }
        RuntimeType::named(name)
    }

    fn type_name_text(&mut self, unit: &Rc<CompilationUnit>, id: NodeId) -> String {
        let base = unit
            .first_child_of_kind(id, "type_identifier")
            .or_else(|| unit.first_child_of_kind(id, "qualified"))
            .or_else(|| unit.first_child_of_kind(id, "identifier"))
            .or_else(|| unit.first_child_of_kind(id, "type"));
        match base {
            Some(b) => unit.text(b).to_string(),
            None => unit.text(id).to_string(),
        }
    }

    // =========================================================================
    // Expressions
    // =========================================================================

    pub fn visit_expression(
        &mut self,
        unit: &Rc<CompilationUnit>,
        id: NodeId,
    ) -> Result<Value, InterpError> {
        match unit.kind(id) {
            "null_literal" => Ok(Value::Null),
            "true" => Ok(Value::Bool(true)),
            "false" => Ok(Value::Bool(false)),
            "decimal_integer_literal" | "hex_integer_literal" => {
                self.visit_integer_literal(unit, id)
            }
            "decimal_floating_point_literal" => {
                let text = unit.text(id).replace('_', "");
                match text.parse::<f64>() {
                    Ok(v) => Ok(Value::Double(v)),
                    Err(_) => Err(InterpError::runtime(format!(
                        "Invalid double literal: {text}"
                    ))),
                }
            }
            "string_literal" => self.visit_string_literal(unit, id),
            "symbol_literal" => Ok(Value::str(unit.text(id))),
            "identifier" => self.visit_identifier(unit, id),
            "this" => self.visit_this(unit, id),
            "super" => Err(InterpError::runtime(
                "'super' can only be used in a member context",
            )),
            "parenthesized_expression" => {
                let inner = unit.named_children(id).first().copied();
                match inner {
                    Some(e) => self.visit_expression(unit, e),
                    None => Ok(Value::Null),
                }
            }
            "list_literal" => self.visit_list_literal(unit, id),
            "set_or_map_literal" => self.visit_set_or_map_literal(unit, id),
            "record_literal" => self.visit_record_literal(unit, id),
            "member_expression" => self.visit_member_expression(unit, id),
            "null_aware_member_expression" => self.visit_null_aware_member_expression(unit, id),
            "static_member_shorthand" => self.visit_static_member_shorthand(unit, id),
            "call_expression" => self.visit_call_expression(unit, id),
            "index_expression" => self.visit_index_expression(unit, id, false),
            "null_aware_index_expression" => self.visit_index_expression(unit, id, true),
            "null_assertion_expression" => {
                let value = unit
                    .child_by_field(id, "value")
                    .map(|v| self.visit_expression(unit, v))
                    .unwrap_or(Ok(Value::Null))?;
                if value.is_null() {
                    Err(InterpError::runtime(
                        "Null check operator used on a null value",
                    ))
                } else {
                    Ok(value)
                }
            }
            "assignable_expression" => {
                let inner = unit.named_children(id).first().copied();
                match inner {
                    Some(e) => self.visit_expression(unit, e),
                    None => Err(InterpError::runtime("Malformed assignable expression")),
                }
            }
            "if_null_expression" => self.visit_if_null_expression(unit, id),
            "logical_or_expression" => {
                let exprs = unit.named_children(id);
                let mut result = Value::Bool(false);
                for e in exprs {
                    let v = self.visit_expression(unit, e)?;
                    let t = is_truthy(&v)?;
                    result = v;
                    if t {
                        break;
                    }
                }
                Ok(result)
            }
            "logical_and_expression" => {
                let exprs = unit.named_children(id);
                let mut result = Value::Bool(true);
                for e in exprs {
                    let v = self.visit_expression(unit, e)?;
                    let t = is_truthy(&v)?;
                    result = v;
                    if !t {
                        break;
                    }
                }
                Ok(result)
            }
            "equality_expression" => self.visit_binary(unit, id),
            "relational_expression" => self.visit_binary(unit, id),
            "shift_expression" => self.visit_binary(unit, id),
            "bitwise_and_expression" | "bitwise_or_expression" | "bitwise_xor_expression" => {
                self.visit_binary(unit, id)
            }
            "additive_expression" => self.visit_binary(unit, id),
            "multiplicative_expression" => self.visit_binary(unit, id),
            "unary_expression" => self.visit_unary(unit, id),
            "postfix_expression" => self.visit_postfix(unit, id),
            "conditional_expression" => self.visit_conditional(unit, id),
            "assignment_expression" => self.visit_assignment(unit, id),
            "throw_expression" => {
                let value = unit
                    .child_by_field(id, "value")
                    .map(|v| self.visit_expression(unit, v))
                    .unwrap_or(Ok(Value::Null))?;
                Err(InterpError::Throw(value))
            }
            "new_expression" | "const_object_expression" => self.visit_instantiation(unit, id),
            "constructor_tearoff" => {
                let name = unit
                    .first_child_of_kind(id, "type_identifier")
                    .or_else(|| unit.first_child_of_kind(id, "identifier"))
                    .map(|n| unit.text(n).to_string())
                    .unwrap_or_default();
                self.lookup_class_or_error(&name)
            }
            "function_expression" => self.visit_function_expression(unit, id),
            "instantiation_expression" => {
                let func = unit.child_by_field(id, "function");
                match func {
                    Some(f)
                        if unit.kind(f) == "new_expression"
                            || unit.kind(f) == "const_object_expression" =>
                    {
                        self.visit_instantiation(unit, f)
                    }
                    Some(f) => self.visit_expression(unit, f),
                    None => Err(InterpError::runtime("Malformed instantiation expression")),
                }
            }
            "type_test_expression" => self.visit_type_test(unit, id),
            "type_cast_expression" => self.visit_type_cast(unit, id),
            "await_expression" => self.visit_await(unit, id),
            "cascade_section" => Err(InterpError::unimplemented(
                "Cascade expressions (..) are not yet implemented",
            )),
            "switch_expression" => Err(InterpError::unimplemented(
                "Switch expressions are not yet implemented",
            )),
            "empty_statement" => Ok(Value::Null),
            "type" => Ok(Value::Type(self.parse_type(unit, id))),
            other => Err(InterpError::unimplemented(format!(
                "Expression '{}' is not yet implemented",
                other
            ))),
        }
    }

    fn visit_await(
        &mut self,
        unit: &Rc<CompilationUnit>,
        id: NodeId,
    ) -> Result<Value, InterpError> {
        if let Some((rn, ri)) = &self.resume_state
            && *rn == id
            && ri == &self.loop_iterations
        {
            self.resume_state = None;
        }
        let cache_key = (id, self.loop_iterations.clone());
        if let Some(cached) = self.await_cache.get(&cache_key).cloned() {
            match cached {
                Err(e) => return Err(InterpError::Throw(e)),
                Ok(v) => match crate::async_state::unwrap_future(&v) {
                    None => return Ok(v),
                    Some(f) => {
                        let status = {
                            let st = f.borrow();
                            (st.completed, st.value.clone(), st.error.clone())
                        };
                        if status.0 {
                            return match (status.1, status.2) {
                                (Some(val), _) => Ok(val),
                                (None, Some(e)) => Err(InterpError::Throw(e)),
                                _ => Ok(Value::Null),
                            };
                        }
                        return self.suspend_at_await(id, f);
                    }
                },
            }
        }
        let expr = unit
            .child_by_field(id, "expression")
            .or_else(|| unit.named_children(id).first().copied());
        let value = match expr {
            Some(e) => self.visit_expression(unit, e)?,
            None => Value::Null,
        };
        match crate::async_state::unwrap_future(&value) {
            None => Ok(value),
            Some(f) => {
                let status = {
                    let st = f.borrow();
                    (st.completed, st.value.clone(), st.error.clone())
                };
                if status.0 {
                    return match (status.1, status.2) {
                        (Some(v), _) => Ok(v),
                        (None, Some(e)) => Err(InterpError::Throw(e)),
                        _ => Ok(Value::Null),
                    };
                }
                self.await_cache.insert(cache_key, Ok(value.clone()));
                self.suspend_at_await(id, f)
            }
        }
    }

    fn suspend_at_await(
        &mut self,
        id: NodeId,
        f: Rc<RefCell<crate::async_state::FutureState>>,
    ) -> Result<Value, InterpError> {
        let cont = self.make_resume_cont(id)?;
        self.resume_state = None;
        f.borrow_mut()
            .continuations
            .push(crate::async_state::Continuation::Resume(cont));
        Err(InterpError::Suspend)
    }

    /// Captures the state needed to resume the current async pass at
    /// `id` (the node that suspended) with the current cache snapshot.
    fn make_resume_cont(
        &mut self,
        id: NodeId,
    ) -> Result<crate::async_state::ResumeContinuation, InterpError> {
        let frame = self
            .async_frames
            .last()
            .cloned()
            .ok_or(InterpError::AsyncSuspensionOutsideAsync)?;
        let cache = std::mem::take(&mut self.await_cache);
        Ok(crate::async_state::ResumeContinuation {
            unit: frame.unit.clone(),
            body: frame.body,
            function_env: self.current_environment.clone(),
            completer: frame.completer.clone(),
            cache,
            resume_node: id,
            resume_iters: self.loop_iterations.clone(),
            stream_state: frame.stream_state.clone(),
        })
    }

    fn visit_integer_literal(
        &mut self,
        unit: &Rc<CompilationUnit>,
        id: NodeId,
    ) -> Result<Value, InterpError> {
        let text = unit.text(id).replace('_', "");
        let value = if text.starts_with("0x") || text.starts_with("0X") {
            i64::from_str_radix(&text[2..], 16)
        } else {
            text.parse::<i64>()
        };
        match value {
            Ok(v) => Ok(Value::Int(v)),
            Err(_) => Err(InterpError::runtime(format!(
                "Invalid integer literal: {text}"
            ))),
        }
    }

    fn visit_identifier(
        &mut self,
        unit: &Rc<CompilationUnit>,
        id: NodeId,
    ) -> Result<Value, InterpError> {
        let name = unit.text(id);
        if let Some(ext_member) = self.resolve_identifier_member(name) {
            return ext_member;
        }
        match Environment::get(&self.current_environment, name) {
            Ok(v) => Ok(v),
            Err(_) => {
                // Bare instance field/member access: resolve against `this`.
                match Environment::get(&self.current_environment, "this") {
                    Ok(Value::Instance(inst)) => instance_get(&inst, name, Some(self)),
                    Ok(Value::EnumValue(ev)) => enum_value_get(&ev, name, Some(self)),
                    Ok(Value::Class(cls)) => self.get_class_member(&cls, name),
                    Ok(Value::BoundSuper(b)) => {
                        self.get_super_property(&Value::BoundSuper(b), name)
                    }
                    _ => Err(InterpError::runtime(format!("Undefined variable: {name}"))),
                }
            }
        }
    }

    fn resolve_identifier_member(&mut self, _name: &str) -> Option<Result<Value, InterpError>> {
        None
    }

    fn visit_this(
        &mut self,
        _unit: &Rc<CompilationUnit>,
        _id: NodeId,
    ) -> Result<Value, InterpError> {
        Environment::get(&self.current_environment, "this")
    }

    fn visit_string_literal(
        &mut self,
        unit: &Rc<CompilationUnit>,
        id: NodeId,
    ) -> Result<Value, InterpError> {
        let mut result = String::new();
        for &child in unit.children(id) {
            let kind = unit.kind(child);
            if kind.starts_with("string_literal_") || kind.starts_with("raw_string_literal_") {
                let is_single = kind.contains("single");
                result.push_str(&self.compose_string_part(unit, child, is_single)?);
            }
        }
        Ok(Value::str(result))
    }

    fn stringify_interp(&mut self, v: &Value) -> Result<String, InterpError> {
        match v {
            Value::Instance(_)
            | Value::EnumValue(_)
            | Value::Bridged(_)
            | Value::Record(_)
            | Value::Type(_) => {
                if let Ok(method) = self.get_property(v, "toString")
                    && let Ok(Value::Str(s)) = self.call_value(&method, vec![], HashMap::new())
                {
                    return Ok(s.to_string());
                }
                Ok(value_display(v))
            }
            _ => Ok(value_display(v)),
        }
    }

    fn compose_string_part(
        &mut self,
        unit: &Rc<CompilationUnit>,
        id: NodeId,
        is_single: bool,
    ) -> Result<String, InterpError> {
        let kind = unit.kind(id);
        if kind.starts_with("raw_string_literal_") {
            let text = unit.text(id);
            let chars: Vec<char> = text.chars().collect();
            if chars.len() >= 2 {
                let content: String = chars[2..chars.len() - 1].iter().collect();
                return Ok(content);
            }
            return Ok(text.to_string());
        }
        // The tree-sitter lexer silently drops quote characters (e.g. `"`
        // inside single-quoted strings) from the token stream. When the
        // literal has no interpolation we reconstruct the content from the
        // raw text instead of the child tokens.
        if !unit
            .children(id)
            .iter()
            .any(|&c| unit.kind(c) == "template_substitution")
        {
            return Ok(compose_plain_string(unit.text(id)));
        }
        let mut result = String::new();
        for &child in unit.children(id) {
            match unit.kind(child) {
                "template_substitution" => {
                    let inner = unit.named_children(child).first().copied();
                    match inner {
                        Some(e) if unit.kind(e) == "identifier_dollar_escaped" => {
                            let name = unit.text(e);
                            let v = match Environment::get(&self.current_environment, name) {
                                Ok(v) => v,
                                Err(_) => {
                                    match Environment::get(&self.current_environment, "this") {
                                        Ok(Value::Instance(inst)) => {
                                            instance_get(&inst, name, Some(self))?
                                        }
                                        Ok(Value::EnumValue(ev)) => {
                                            enum_value_get(&ev, name, Some(self))?
                                        }
                                        Ok(Value::Class(cls)) => {
                                            self.get_class_member(&cls, name)?
                                        }
                                        _ => Environment::get(&self.current_environment, name)?,
                                    }
                                }
                            };
                            result.push_str(&self.stringify_interp(&v)?);
                        }
                        Some(e) => {
                            let v = self.visit_expression(unit, e)?;
                            result.push_str(&self.stringify_interp(&v)?);
                        }
                        None => {}
                    }
                }
                "escape_sequence" => {
                    result.push_str(&decode_escape(unit.text(child)));
                }
                other
                    if other.starts_with("template_chars_")
                        || other.starts_with("template_chars") =>
                {
                    result.push_str(unit.text(child));
                }
                k if k == "\\\"" && is_single => result.push('"'),
                _ => {}
            }
        }
        Ok(result)
    }

    fn visit_list_literal(
        &mut self,
        unit: &Rc<CompilationUnit>,
        id: NodeId,
    ) -> Result<Value, InterpError> {
        let mut items: Vec<Value> = Vec::new();
        for child in unit.named_children(id) {
            let kind = unit.kind(child);
            match kind {
                "comment" => {}
                "type_arguments" => {}
                "spread_element" => {
                    let v = unit
                        .child_by_field(child, "value")
                        .map(|v| self.visit_expression(unit, v))
                        .unwrap_or(Ok(Value::Null))?;
                    match v {
                        Value::List(l) => items.extend(l.borrow().iter().cloned()),
                        Value::Set(s) => items.extend(s.borrow().items.iter().cloned()),
                        Value::Str(text) => {
                            items.extend(text.chars().map(|c| Value::str(c.to_string())))
                        }
                        Value::Null => {}
                        other => {
                            return Err(InterpError::runtime(format!(
                                "Spread operator expects an iterable, got {}",
                                value_display(&other)
                            )));
                        }
                    }
                }
                "null_aware_element" => {
                    let v = self.visit_expression(unit, child)?;
                    if !v.is_null() {
                        items.push(v);
                    }
                }
                "if_element" => {
                    let cond_node = unit
                        .child_by_field(child, "condition")
                        .or_else(|| unit.named_children(child).first().copied());
                    let cond = match cond_node {
                        Some(c) => is_truthy(&self.visit_expression(unit, c)?)?,
                        None => false,
                    };
                    if cond {
                        if let Some(cons) = unit.child_by_field(child, "consequence") {
                            self.collect_element(unit, cons, &mut items)?;
                        }
                    } else if let Some(alt) = unit.child_by_field(child, "alternative") {
                        self.collect_element(unit, alt, &mut items)?;
                    }
                }
                "for_element" => {
                    let saved = self.current_environment.clone();
                    if let Some(value_field) = unit.child_by_field(child, "value") {
                        let iterable = self.visit_expression(unit, value_field)?;
                        let name = unit
                            .child_by_field(child, "name")
                            .map(|n| unit.text(n).to_string());
                        let items_for = match iterable {
                            Value::List(l) => l.borrow().clone(),
                            Value::Set(s) => s.borrow().items.clone(),
                            _ => vec![],
                        };
                        for item in items_for {
                            let env = Environment::child(&self.current_environment);
                            self.current_environment = env.clone();
                            if let Some(n) = &name {
                                Environment::define(&env, n, item);
                            }
                            if let Some(body) = unit.child_by_field(child, "body") {
                                self.collect_element(unit, body, &mut items)?;
                            }
                            self.current_environment = saved.clone();
                        }
                    } else {
                        if let Some(init) = unit.child_by_field(child, "init") {
                            if unit.kind(init) == "local_variable_declaration" {
                                self.visit_statement(unit, init)?;
                            } else {
                                self.visit_expression(unit, init)?;
                            }
                        }
                        loop {
                            if let Some(cond) = unit.child_by_field(child, "condition")
                                && !self.visit_truthy_condition(unit, cond)?
                            {
                                break;
                            }
                            if let Some(body) = unit.child_by_field(child, "body") {
                                self.collect_element(unit, body, &mut items)?;
                            }
                            if let Some(update) = unit.child_by_field(child, "update") {
                                self.visit_expression(unit, update)?;
                            }
                        }
                    }
                    self.current_environment = saved;
                }
                other => {
                    if unit.nodes[child].is_named {
                        let v = self.visit_expression(unit, child)?;
                        items.push(v);
                    }
                    let _ = other;
                }
            }
        }
        Ok(Value::List(Rc::new(RefCell::new(items))))
    }

    fn collect_element(
        &mut self,
        unit: &Rc<CompilationUnit>,
        node: NodeId,
        items: &mut Vec<Value>,
    ) -> Result<(), InterpError> {
        let kind = unit.kind(node);
        match kind {
            "comment" => {}
            "spread_element" => {
                let v = unit
                    .child_by_field(node, "value")
                    .map(|v| self.visit_expression(unit, v))
                    .unwrap_or(Ok(Value::Null))?;
                match v {
                    Value::List(l) => items.extend(l.borrow().iter().cloned()),
                    Value::Set(s) => items.extend(s.borrow().items.iter().cloned()),
                    Value::Null => {}
                    other => {
                        return Err(InterpError::runtime(format!(
                            "Spread operator expects an iterable, got {}",
                            value_display(&other)
                        )));
                    }
                }
            }
            _ => {
                let v = self.visit_expression(unit, node)?;
                items.push(v);
            }
        }
        Ok(())
    }

    fn visit_set_or_map_literal(
        &mut self,
        unit: &Rc<CompilationUnit>,
        id: NodeId,
    ) -> Result<Value, InterpError> {
        let named = unit.named_children(id);
        let is_map = named
            .iter()
            .any(|&c| unit.kind(c) == "pair" || unit.kind(c) == "null_aware_pair");
        if is_map {
            let mut map = DartMap::new();
            for child in named {
                self.collect_map_element(unit, child, &mut map)?;
            }
            Ok(Value::Map(Rc::new(RefCell::new(map))))
        } else {
            let mut set = DartSet::new();
            for child in named {
                match unit.kind(child) {
                    "comment" => {}
                    "type_arguments" => {}
                    "spread_element" => {
                        let v = unit
                            .child_by_field(child, "value")
                            .map(|v| self.visit_expression(unit, v))
                            .unwrap_or(Ok(Value::Null))?;
                        match v {
                            Value::Set(s) => {
                                for item in s.borrow().items.clone() {
                                    set.insert(item);
                                }
                            }
                            Value::List(l) => {
                                for item in l.borrow().iter().cloned() {
                                    set.insert(item);
                                }
                            }
                            _ => {}
                        }
                    }
                    "if_element" => {
                        let cond = match unit
                            .child_by_field(child, "condition")
                            .or_else(|| unit.named_children(child).first().copied())
                        {
                            Some(c) => is_truthy(&self.visit_expression(unit, c)?)?,
                            None => false,
                        };
                        if cond {
                            if let Some(cons) = unit.child_by_field(child, "consequence") {
                                let v = self.visit_expression(unit, cons)?;
                                set.insert(v);
                            }
                        } else if let Some(alt) = unit.child_by_field(child, "alternative") {
                            let v = self.visit_expression(unit, alt)?;
                            set.insert(v);
                        }
                    }
                    _ => {
                        let v = self.visit_expression(unit, child)?;
                        set.insert(v);
                    }
                }
            }
            Ok(Value::Set(Rc::new(RefCell::new(set))))
        }
    }

    fn collect_map_element(
        &mut self,
        unit: &Rc<CompilationUnit>,
        node: NodeId,
        map: &mut DartMap,
    ) -> Result<(), InterpError> {
        match unit.kind(node) {
            "comment" => {}
            "type_arguments" => {}
            "pair" => {
                let key = unit
                    .child_by_field(node, "key")
                    .map(|k| self.visit_expression(unit, k))
                    .unwrap_or(Ok(Value::Null))?;
                let value = unit
                    .child_by_field(node, "value")
                    .map(|v| self.visit_expression(unit, v))
                    .unwrap_or(Ok(Value::Null))?;
                map.set(key, value);
            }
            "null_aware_pair" => {
                let key = unit
                    .child_by_field(node, "key")
                    .map(|k| self.visit_expression(unit, k))
                    .unwrap_or(Ok(Value::Null))?;
                let value = unit
                    .child_by_field(node, "value")
                    .map(|v| self.visit_expression(unit, v))
                    .unwrap_or(Ok(Value::Null))?;
                if !value.is_null() {
                    map.set(key, value);
                }
            }
            "spread_element" => {
                let v = unit
                    .child_by_field(node, "value")
                    .map(|v| self.visit_expression(unit, v))
                    .unwrap_or(Ok(Value::Null))?;
                if let Value::Map(m) = v {
                    let entries = m.borrow().entries.clone();
                    for (k, val) in entries {
                        map.set(k, val);
                    }
                }
            }
            "if_element" => {
                let cond = match unit
                    .child_by_field(node, "condition")
                    .or_else(|| unit.named_children(node).first().copied())
                {
                    Some(c) => is_truthy(&self.visit_expression(unit, c)?)?,
                    None => false,
                };
                if cond {
                    if let Some(cons) = unit.child_by_field(node, "consequence") {
                        self.collect_map_element(unit, cons, map)?;
                    }
                } else if let Some(alt) = unit.child_by_field(node, "alternative") {
                    self.collect_map_element(unit, alt, map)?;
                }
            }
            _ => {
                let v = self.visit_expression(unit, node)?;
                if let Value::Map(m) = v {
                    let entries = m.borrow().entries.clone();
                    for (k, val) in entries {
                        map.set(k, val);
                    }
                }
            }
        }
        Ok(())
    }

    fn visit_record_literal(
        &mut self,
        unit: &Rc<CompilationUnit>,
        id: NodeId,
    ) -> Result<Value, InterpError> {
        let mut positional: Vec<Value> = Vec::new();
        let mut named: Vec<(String, Value)> = Vec::new();
        let children = unit.named_children(id);
        let mut i = 0;
        while i < children.len() {
            let child = children[i];
            match unit.kind(child) {
                "comment" => {}
                "record_field" => match unit.first_child_of_kind(child, "label") {
                    Some(label) => {
                        let name = unit
                            .first_child_of_kind(label, "identifier")
                            .map(|i| unit.text(i).to_string())
                            .unwrap_or_default();
                        let value = unit
                            .named_children(child)
                            .iter()
                            .rev()
                            .find(|&&c| unit.kind(c) != "label")
                            .map(|&c| self.visit_expression(unit, c))
                            .unwrap_or(Ok(Value::Null))?;
                        named.push((name, value));
                    }
                    None => {
                        let value = unit
                            .named_children(child)
                            .first()
                            .map(|&c| self.visit_expression(unit, c))
                            .unwrap_or(Ok(Value::Null))?;
                        positional.push(value);
                    }
                },
                "label" => {
                    // Single-field record: the label is a direct child,
                    // followed by the value expression.
                    let name = unit
                        .first_child_of_kind(child, "identifier")
                        .map(|i| unit.text(i).to_string())
                        .unwrap_or_default();
                    if let Some(&value_node) = children.get(i + 1) {
                        let value = self.visit_expression(unit, value_node)?;
                        named.push((name, value));
                        i += 1;
                    }
                }
                _ => {
                    let v = self.visit_expression(unit, child)?;
                    positional.push(v);
                }
            }
            i += 1;
        }
        Ok(Value::Record(Rc::new(RefCell::new(RecordData {
            positional_fields: positional,
            named_fields: named,
        }))))
    }

    fn visit_member_expression(
        &mut self,
        unit: &Rc<CompilationUnit>,
        id: NodeId,
    ) -> Result<Value, InterpError> {
        let object = unit.child_by_field(id, "object");
        let property = unit.child_by_field(id, "property");
        let Some(object) = object else {
            return Err(InterpError::runtime(
                "Malformed member expression (no object)",
            ));
        };
        let name = property
            .map(|p| unit.text(p).to_string())
            .unwrap_or_default();
        if unit.kind(object) == "super" {
            let bound = Environment::get(&self.current_environment, "super")?;
            return self.get_super_property(&bound, &name);
        }
        let target = self.visit_expression(unit, object)?;
        self.get_property(&target, &name)
    }

    fn visit_null_aware_member_expression(
        &mut self,
        unit: &Rc<CompilationUnit>,
        id: NodeId,
    ) -> Result<Value, InterpError> {
        let object = unit.child_by_field(id, "object");
        let property = unit.child_by_field(id, "property");
        let Some(object) = object else {
            return Err(InterpError::runtime(
                "Malformed member expression (no object)",
            ));
        };
        let name = property
            .map(|p| unit.text(p).to_string())
            .unwrap_or_default();
        let target = self.visit_expression(unit, object)?;
        if target.is_null() {
            return Ok(Value::Null);
        }
        self.get_property(&target, &name)
    }

    fn visit_static_member_shorthand(
        &mut self,
        unit: &Rc<CompilationUnit>,
        id: NodeId,
    ) -> Result<Value, InterpError> {
        let named = unit.named_children(id);
        if !named.is_empty() {
            let first = named[0];
            if unit.kind(first) == "identifier" {
                let name = unit.text(first);
                if let Ok(v) = Environment::get(&self.current_environment, name)
                    && let Some(prop) = named.get(1)
                {
                    return self.get_property(&v, unit.text(*prop));
                }
            }
        }
        Err(InterpError::unimplemented(
            "Static member shorthand is not fully supported",
        ))
    }

    fn visit_call_expression(
        &mut self,
        unit: &Rc<CompilationUnit>,
        id: NodeId,
    ) -> Result<Value, InterpError> {
        let function = unit.child_by_field(id, "function");
        let Some(function) = function else {
            return Err(InterpError::runtime(
                "Malformed call expression (no function)",
            ));
        };
        // A null-aware method call (`obj?.method(...)`) short-circuits to
        // null when the receiver is null, without evaluating the arguments.
        if unit.kind(function) == "null_aware_member_expression"
            && let Some(object) = unit.child_by_field(function, "object")
            && self.visit_expression(unit, object)?.is_null()
        {
            return Ok(Value::Null);
        }
        let arguments = unit.child_by_field(id, "arguments");
        let (positional, named) = self.evaluate_arguments(unit, arguments)?;
        let callee = self.visit_expression(unit, function)?;
        self.call_value(&callee, positional, named)
    }

    fn evaluate_arguments(
        &mut self,
        unit: &Rc<CompilationUnit>,
        arguments: Option<NodeId>,
    ) -> Result<(Vec<Value>, HashMap<String, Value>), InterpError> {
        let mut positional: Vec<Value> = Vec::new();
        let mut named: HashMap<String, Value> = HashMap::new();
        let Some(args) = arguments else {
            return Ok((positional, named));
        };
        for child in unit.named_children(args) {
            match unit.kind(child) {
                "named_argument" => {
                    let name = unit
                        .first_child_of_kind(child, "label")
                        .and_then(|l| unit.first_child_of_kind(l, "identifier"))
                        .or_else(|| unit.first_child_of_kind(child, "identifier"))
                        .map(|n| unit.text(n).to_string())
                        .unwrap_or_default();
                    let value = unit
                        .named_children(child)
                        .iter()
                        .last()
                        .map(|&c| self.visit_expression(unit, c))
                        .unwrap_or(Ok(Value::Null))?;
                    named.insert(name, value);
                }
                _ => {
                    let v = self.visit_expression(unit, child)?;
                    positional.push(v);
                }
            }
        }
        Ok((positional, named))
    }

    fn visit_index_expression(
        &mut self,
        unit: &Rc<CompilationUnit>,
        id: NodeId,
        null_aware: bool,
    ) -> Result<Value, InterpError> {
        let object = unit.child_by_field(id, "object");
        let index = unit.child_by_field(id, "index");
        let Some(object) = object else {
            return Err(InterpError::runtime(
                "Malformed index expression (no object)",
            ));
        };
        let target = self.visit_expression(unit, object)?;
        if null_aware && target.is_null() {
            return Ok(Value::Null);
        }
        let idx = match index {
            Some(i) => self.visit_expression(unit, i)?,
            None => {
                return Err(InterpError::runtime(
                    "Malformed index expression (no index)",
                ));
            }
        };
        self.get_index(&target, &idx)
    }

    fn get_index(&mut self, target: &Value, index: &Value) -> Result<Value, InterpError> {
        match target {
            Value::List(l) => {
                let Some(i) = index.as_int() else {
                    return Err(InterpError::runtime("List index must be an integer"));
                };
                let l = l.borrow();
                if i < 0 || i as usize >= l.len() {
                    return Err(InterpError::runtime(format!(
                        "Index out of range: {i} (length {})",
                        l.len()
                    )));
                }
                Ok(l[i as usize].clone())
            }
            Value::Map(m) => {
                let m = m.borrow();
                Ok(m.get(index).unwrap_or(Value::Null))
            }
            Value::Instance(inst) => {
                let klass = inst.borrow().klass.clone();
                if let Some(op) = klass.find_operator("[]") {
                    let bound = op.bind_instance(inst.clone());
                    return call_callable(
                        &Callable::Interpreted(bound),
                        self,
                        vec![index.clone()],
                        HashMap::new(),
                    );
                }
                Err(InterpError::runtime(
                    "Operator [] is not defined for this object",
                ))
            }
            Value::Str(_) => {
                let bridged = self.wrap_native(target)?;
                let b = match &bridged {
                    Value::Bridged(b) => b.clone(),
                    _ => unreachable!(),
                };
                let bc = b.borrow().bridged_class.clone();
                if let Some(m) = bc.find_instance_method_adapter("[]") {
                    let native = b.borrow().native.clone();
                    return m(self, native, vec![index.clone()], HashMap::new());
                }
                Err(InterpError::runtime("Cannot index into String"))
            }
            Value::Bridged(b) => {
                let bc = b.borrow().bridged_class.clone();
                if let Some(m) = bc.find_instance_method_adapter("[]") {
                    let native = b.borrow().native.clone();
                    return m(self, native, vec![index.clone()], HashMap::new());
                }
                Err(InterpError::runtime(format!(
                    "Operator [] is not defined for {}",
                    bc.name
                )))
            }
            other => Err(InterpError::runtime(format!(
                "Cannot index into {}",
                value_display(other)
            ))),
        }
    }

    fn set_index(
        &mut self,
        target: &Value,
        index: &Value,
        value: Value,
    ) -> Result<(), InterpError> {
        match target {
            Value::List(l) => {
                let Some(i) = index.as_int() else {
                    return Err(InterpError::runtime("List index must be an integer"));
                };
                let mut l = l.borrow_mut();
                if i < 0 || i as usize >= l.len() {
                    return Err(InterpError::runtime(format!(
                        "Index out of range: {i} (length {})",
                        l.len()
                    )));
                }
                l[i as usize] = value;
                Ok(())
            }
            Value::Map(m) => {
                m.borrow_mut().set(index.clone(), value);
                Ok(())
            }
            Value::Instance(inst) => {
                let klass = inst.borrow().klass.clone();
                if let Some(op) = klass.find_operator("[]=") {
                    let bound = op.bind_instance(inst.clone());
                    call_callable(
                        &Callable::Interpreted(bound),
                        self,
                        vec![index.clone(), value],
                        HashMap::new(),
                    )?;
                    return Ok(());
                }
                Err(InterpError::runtime(
                    "Operator []= is not defined for this object",
                ))
            }
            Value::Bridged(b) => {
                let bc = b.borrow().bridged_class.clone();
                let native = b.borrow().native.clone();
                if let Some(m) = bc.find_instance_method_adapter("[]=") {
                    m(self, native, vec![index.clone(), value], HashMap::new())?;
                    return Ok(());
                }
                match &native {
                    Value::Map(m) => {
                        m.borrow_mut().set(index.clone(), value);
                        Ok(())
                    }
                    Value::List(l) => {
                        let Some(i) = index.as_int() else {
                            return Err(InterpError::runtime("List index must be an integer"));
                        };
                        let mut l = l.borrow_mut();
                        if i < 0 || i as usize >= l.len() {
                            return Err(InterpError::runtime(format!(
                                "Index out of range: {i} (length {})",
                                l.len()
                            )));
                        }
                        l[i as usize] = value;
                        Ok(())
                    }
                    _ => Err(InterpError::runtime(format!(
                        "Cannot assign to index of {}",
                        value_display(target)
                    ))),
                }
            }
            other => Err(InterpError::runtime(format!(
                "Cannot assign to index of {}",
                value_display(other)
            ))),
        }
    }

    fn visit_if_null_expression(
        &mut self,
        unit: &Rc<CompilationUnit>,
        id: NodeId,
    ) -> Result<Value, InterpError> {
        let exprs = unit.named_children(id);
        if exprs.len() >= 2 {
            let left = self.visit_expression(unit, exprs[0])?;
            if left.is_null() {
                self.visit_expression(unit, exprs[1])
            } else {
                Ok(left)
            }
        } else if let Some(&e) = exprs.first() {
            self.visit_expression(unit, e)
        } else {
            Ok(Value::Null)
        }
    }

    fn visit_binary(
        &mut self,
        unit: &Rc<CompilationUnit>,
        id: NodeId,
    ) -> Result<Value, InterpError> {
        let children = unit.children(id);
        let mut operands: Vec<NodeId> = Vec::new();
        let mut operators: Vec<String> = Vec::new();
        for &c in children {
            let node = &unit.nodes[c];
            if node.is_named {
                match unit.kind(c) {
                    "type_test" | "type_cast" | "relational_operator" | "is_operator" => {
                        operators.push(unit.text(c).to_string());
                    }
                    _ => operands.push(c),
                }
            } else {
                let text = unit.text(c);
                if is_binary_operator(text) || text == "is" || text == "is!" {
                    operators.push(text.to_string());
                }
            }
        }
        if operands.is_empty() {
            return Err(InterpError::runtime("Malformed binary expression"));
        }
        let mut result = self.visit_expression(unit, operands[0])?;
        for (i, op) in operators.iter().enumerate() {
            let right = if i + 1 < operands.len() {
                self.visit_expression(unit, operands[i + 1])?
            } else {
                return Err(InterpError::runtime(
                    "Malformed binary expression (missing operand)",
                ));
            };
            result = self.apply_binary_operator(op, &result, &right)?;
        }
        Ok(result)
    }

    fn apply_binary_operator(
        &mut self,
        op: &str,
        left: &Value,
        right: &Value,
    ) -> Result<Value, InterpError> {
        match op {
            "==" => self.equals(left, right),
            "!=" => {
                let eq = self.equals(left, right)?;
                Ok(Value::Bool(!is_truthy(&eq)?))
            }
            "+" => self.binary_add(left, right),
            "-" => {
                if let Some(v) = num_sub(left, right) {
                    Ok(v)
                } else if let Some(v) = bigint_op(left, right, |a, b| a - b) {
                    Ok(v)
                } else if let Some(v) = self.call_operator(left, "-", vec![right.clone()])? {
                    Ok(v)
                } else {
                    Err(binary_op_error(op, left, right))
                }
            }
            "*" => self.binary_multiply(left, right),
            "/" => {
                if let Some(v) = num_div(left, right) {
                    Ok(v)
                } else if let Some(v) = bigint_op(left, right, |a, b| a / b) {
                    Ok(v)
                } else if let Some(v) = self.call_operator(left, "/", vec![right.clone()])? {
                    Ok(v)
                } else {
                    Err(binary_op_error(op, left, right))
                }
            }
            "~/" => match num_trunc_div(left, right) {
                Some(Ok(v)) => Ok(v),
                Some(Err(e)) => Err(e),
                None => {
                    if let Some(v) = bigint_op(left, right, |a, b| a / b) {
                        Ok(v)
                    } else if let Some(v) = self.call_operator(left, "~/", vec![right.clone()])? {
                        Ok(v)
                    } else {
                        Err(binary_op_error(op, left, right))
                    }
                }
            },
            "%" => match num_rem(left, right) {
                Some(Ok(v)) => Ok(v),
                Some(Err(e)) => Err(e),
                None => {
                    if let Some(v) = bigint_op(left, right, |a, b| a % b) {
                        Ok(v)
                    } else if let Some(v) = self.call_operator(left, "%", vec![right.clone()])? {
                        Ok(v)
                    } else {
                        Err(binary_op_error(op, left, right))
                    }
                }
            },
            "<" | ">" | "<=" | ">=" => self.compare(op, left, right),
            "<<" | ">>" | ">>>" => self.shift(op, left, right),
            "&" | "|" | "^" => self.bitwise(op, left, right),
            _ => Err(binary_op_error(op, left, right)),
        }
    }

    fn equals(&mut self, left: &Value, right: &Value) -> Result<Value, InterpError> {
        if let Some(v) = self.call_operator(left, "==", vec![right.clone()])? {
            return Ok(v);
        }
        Ok(Value::Bool(value_eq(left, right)))
    }

    fn binary_add(&mut self, left: &Value, right: &Value) -> Result<Value, InterpError> {
        if let Some(v) = num_add(left, right) {
            return Ok(v);
        }
        if let Some(v) = bigint_op(left, right, |a, b| a + b) {
            return Ok(v);
        }
        match (left, right) {
            (Value::Str(a), Value::Str(b)) => Ok(Value::str(format!("{a}{b}"))),
            (Value::List(a), Value::List(b)) => {
                let mut items = a.borrow().clone();
                items.extend(b.borrow().iter().cloned());
                Ok(Value::List(Rc::new(RefCell::new(items))))
            }
            _ => {
                if let Some(v) = self.call_operator(left, "+", vec![right.clone()])? {
                    Ok(v)
                } else {
                    Err(binary_op_error("+", left, right))
                }
            }
        }
    }

    fn binary_multiply(&mut self, left: &Value, right: &Value) -> Result<Value, InterpError> {
        if let Some(v) = num_mul(left, right) {
            return Ok(v);
        }
        if let Some(v) = bigint_op(left, right, |a, b| a * b) {
            return Ok(v);
        }
        if let (Value::Str(s), Value::Int(n)) = (left, right) {
            if *n <= 0 {
                return Ok(Value::str(""));
            }
            let mut out = String::new();
            for _ in 0..*n {
                out.push_str(s);
            }
            return Ok(Value::str(out));
        }
        if let Some(v) = self.call_operator(left, "*", vec![right.clone()])? {
            Ok(v)
        } else {
            Err(binary_op_error("*", left, right))
        }
    }

    fn compare(&mut self, op: &str, left: &Value, right: &Value) -> Result<Value, InterpError> {
        if let Some(v) = self.call_operator(left, op, vec![right.clone()])? {
            return Ok(v);
        }
        let ordering = match (left, right) {
            (Value::Str(a), Value::Str(b)) => Some(a.cmp(b)),
            _ => match bigint_cmp(left, right) {
                Some(o) => Some(o),
                None => num_compare(left, right),
            },
        };
        match ordering {
            Some(o) => {
                let result = match op {
                    "<" => o.is_lt(),
                    ">" => o.is_gt(),
                    "<=" => o.is_le(),
                    ">=" => o.is_ge(),
                    _ => false,
                };
                Ok(Value::Bool(result))
            }
            None => Err(binary_op_error(op, left, right)),
        }
    }

    fn shift(&mut self, op: &str, left: &Value, right: &Value) -> Result<Value, InterpError> {
        if let (Some(a), Some(b)) = (left.as_int(), right.as_int()) {
            let result = match op {
                "<<" => a.wrapping_shl(b as u32),
                ">>" => a.wrapping_shr(b as u32),
                ">>>" => ((a as u64) >> (b as u32)) as i64,
                _ => unreachable!(),
            };
            return Ok(Value::Int(result));
        }
        if let Some(v) = self.call_operator(left, op, vec![right.clone()])? {
            return Ok(v);
        }
        Err(binary_op_error(op, left, right))
    }

    fn bitwise(&mut self, op: &str, left: &Value, right: &Value) -> Result<Value, InterpError> {
        if let (Some(a), Some(b)) = (left.as_int(), right.as_int()) {
            let result = match op {
                "&" => a & b,
                "|" => a | b,
                "^" => a ^ b,
                _ => unreachable!(),
            };
            return Ok(Value::Int(result));
        }
        if let Some(v) = self.call_operator(left, op, vec![right.clone()])? {
            return Ok(v);
        }
        Err(binary_op_error(op, left, right))
    }

    fn call_operator(
        &mut self,
        value: &Value,
        op: &str,
        args: Vec<Value>,
    ) -> Result<Option<Value>, InterpError> {
        match value {
            Value::Instance(inst) => {
                let klass = inst.borrow().klass.clone();
                if let Some(m) = klass.find_operator(op) {
                    let bound = m.bind_instance(inst.clone());
                    let v =
                        call_callable(&Callable::Interpreted(bound), self, args, HashMap::new())?;
                    return Ok(Some(v));
                }
            }
            Value::Bridged(b) => {
                let bc = b.borrow().bridged_class.clone();
                let native = b.borrow().native.clone();
                if let Some(m) = bc.find_instance_method_adapter(op) {
                    let v = m(self, native, args, HashMap::new())?;
                    return Ok(Some(v));
                }
            }
            _ => {}
        }
        Ok(None)
    }

    fn visit_unary(
        &mut self,
        unit: &Rc<CompilationUnit>,
        id: NodeId,
    ) -> Result<Value, InterpError> {
        let children = unit.children(id);
        let mut operator: Option<String> = None;
        let mut operand: Option<NodeId> = None;
        for &c in children {
            if unit.kind(c) == "prefix_operator" || unit.kind(c) == "negate_operator" {
                if let Some(anon) = unit
                    .children(c)
                    .iter()
                    .find(|&&cc| !unit.nodes[cc].is_named)
                    .copied()
                {
                    let text = unit.text(anon);
                    if text == "!" || text == "-" || text == "~" || text == "++" || text == "--" {
                        operator = Some(text.to_string());
                    }
                }
            } else if !unit.nodes[c].is_named {
                let text = unit.text(c);
                if text == "!" || text == "-" || text == "~" || text == "++" || text == "--" {
                    operator = Some(text.to_string());
                }
            } else {
                operand = Some(c);
            }
        }
        let Some(operand) = operand else {
            return Err(InterpError::runtime(
                "Malformed unary expression (no operand)",
            ));
        };
        let Some(op) = operator else {
            return self.visit_expression(unit, operand);
        };
        match op.as_str() {
            "!" => {
                let v = self.visit_expression(unit, operand)?;
                Ok(Value::Bool(!is_truthy(&v)?))
            }
            "-" => {
                let v = self.visit_expression(unit, operand)?;
                match num_neg(&v) {
                    Some(v) => Ok(v),
                    None => {
                        if let Some(v) = self.call_operator(&v, "unary-", vec![])? {
                            Ok(v)
                        } else {
                            Err(InterpError::runtime(format!(
                                "Unary - is not defined for {}",
                                value_display(&v)
                            )))
                        }
                    }
                }
            }
            "~" => {
                let v = self.visit_expression(unit, operand)?;
                match v.as_int() {
                    Some(i) => Ok(Value::Int(!i)),
                    None => match crate::stdlib::helpers::as_bigint(&v) {
                        Some(b) => Ok(crate::stdlib::helpers::wrap(
                            &crate::stdlib::helpers::class("BigInt"),
                            Value::BigInt(!b),
                        )),
                        None => Err(InterpError::runtime(
                            "Bitwise not is only defined for integers",
                        )),
                    },
                }
            }
            "++" | "--" => {
                let v = self.visit_expression(unit, operand)?;
                let nv = self.inc_dec(v.clone(), op == "++")?;
                self.assign_to_node(unit, operand, nv.clone())?;
                Ok(nv)
            }
            _ => Err(InterpError::runtime(format!("Unknown unary operator {op}"))),
        }
    }

    fn visit_postfix(
        &mut self,
        unit: &Rc<CompilationUnit>,
        id: NodeId,
    ) -> Result<Value, InterpError> {
        let argument = unit.child_by_field(id, "argument");
        let Some(argument) = argument else {
            return Err(InterpError::runtime("Malformed postfix expression"));
        };
        let text = unit.text(id);
        let op = if text.ends_with("++") {
            "++"
        } else if text.ends_with("--") {
            "--"
        } else {
            return Err(InterpError::runtime(format!(
                "Unknown postfix operator: {text}"
            )));
        };
        let old = self.visit_expression(unit, argument)?;
        let new = self.inc_dec(old.clone(), op == "++")?;
        self.assign_to_node(unit, argument, new)?;
        Ok(old)
    }

    fn inc_dec(&mut self, value: Value, increment: bool) -> Result<Value, InterpError> {
        match &value {
            Value::Int(i) => Ok(Value::Int(if increment { *i + 1 } else { *i - 1 })),
            Value::Double(d) => Ok(Value::Double(if increment { d + 1.0 } else { d - 1.0 })),
            other => Err(InterpError::runtime(format!(
                "Cannot increment {}",
                value_display(other)
            ))),
        }
    }

    fn visit_conditional(
        &mut self,
        unit: &Rc<CompilationUnit>,
        id: NodeId,
    ) -> Result<Value, InterpError> {
        let cond_node = unit.named_children(id).first().copied();
        let cond = match cond_node {
            Some(c) => is_truthy(&self.visit_expression(unit, c)?)?,
            None => false,
        };
        if cond {
            if let Some(cons) = unit.child_by_field(id, "consequence") {
                self.visit_expression(unit, cons)
            } else {
                Ok(Value::Null)
            }
        } else if let Some(alt) = unit.child_by_field(id, "alternative") {
            self.visit_expression(unit, alt)
        } else {
            Ok(Value::Null)
        }
    }

    fn visit_assignment(
        &mut self,
        unit: &Rc<CompilationUnit>,
        id: NodeId,
    ) -> Result<Value, InterpError> {
        let left = unit.child_by_field(id, "left");
        let operator = unit.child_by_field(id, "operator");
        let right = unit.child_by_field(id, "right");
        let Some(left) = left else {
            return Err(InterpError::runtime("Malformed assignment (no left side)"));
        };
        let op = operator
            .map(|o| unit.text(o).to_string())
            .unwrap_or_default();
        let Some(right) = right else {
            return Err(InterpError::runtime("Malformed assignment (no right side)"));
        };
        match op.as_str() {
            "=" => {
                let value = self.visit_expression(unit, right)?;
                self.assign_to_node(unit, left, value.clone())?;
                Ok(value)
            }
            "??=" => {
                let current = self.evaluate_lvalue(unit, left)?;
                if current.is_null() {
                    let value = self.visit_expression(unit, right)?;
                    self.assign_to_node(unit, left, value.clone())?;
                    Ok(value)
                } else {
                    Ok(current)
                }
            }
            _ => {
                let current = self.evaluate_lvalue(unit, left)?;
                let rhs = self.visit_expression(unit, right)?;
                let base_op = &op[..op.len() - 1];
                let new_value = match base_op {
                    "=" => rhs,
                    _ => self.apply_binary_operator(base_op, &current, &rhs)?,
                };
                self.assign_to_node(unit, left, new_value.clone())?;
                Ok(new_value)
            }
        }
    }

    fn evaluate_lvalue(
        &mut self,
        unit: &Rc<CompilationUnit>,
        node: NodeId,
    ) -> Result<Value, InterpError> {
        match unit.kind(node) {
            "identifier" => {
                let name = unit.text(node);
                Environment::get(&self.current_environment, name)
            }
            "member_expression" => {
                let object = unit.child_by_field(node, "object");
                let property = unit.child_by_field(node, "property");
                let obj = self.visit_expression(unit, object.unwrap_or(node))?;
                let name = property
                    .map(|p| unit.text(p).to_string())
                    .unwrap_or_default();
                self.get_property(&obj, &name)
            }
            "index_expression" | "null_aware_index_expression" => {
                let object = unit.child_by_field(node, "object");
                let index = unit.child_by_field(node, "index");
                let obj = self.visit_expression(unit, object.unwrap_or(node))?;
                let idx = match index {
                    Some(i) => self.visit_expression(unit, i)?,
                    None => Value::Null,
                };
                self.get_index(&obj, &idx)
            }
            "super" => Environment::get(&self.current_environment, "super"),
            "assignable_expression" => {
                if let Some(index) = unit.child_by_field(node, "index") {
                    let object = unit.child_by_field(node, "object");
                    let obj = self.visit_expression(unit, object.unwrap_or(node))?;
                    let idx = self.visit_expression(unit, index)?;
                    return self.get_index(&obj, &idx);
                }
                if let Some(property) = unit.child_by_field(node, "property") {
                    let object = unit.child_by_field(node, "object");
                    let obj = self.visit_expression(unit, object.unwrap_or(node))?;
                    return self.get_property(&obj, unit.text(property));
                }
                let inner = unit.named_children(node).first().copied();
                match inner {
                    Some(i) => self.evaluate_lvalue(unit, i),
                    None => Err(InterpError::runtime("Malformed assignment target")),
                }
            }
            other => Err(InterpError::unimplemented(format!(
                "Assignment target '{}' is not yet supported",
                other
            ))),
        }
    }

    fn assign_to_node(
        &mut self,
        unit: &Rc<CompilationUnit>,
        node: NodeId,
        value: Value,
    ) -> Result<(), InterpError> {
        match unit.kind(node) {
            "identifier" => {
                let name = unit.text(node);
                match Environment::assign(&self.current_environment, name, value.clone()) {
                    Ok(_) => Ok(()),
                    Err(_) => match Environment::get(&self.current_environment, "this") {
                        Ok(Value::Instance(inst)) => instance_set(&inst, name, value, Some(self)),
                        Ok(Value::EnumValue(ev)) => enum_value_set(&ev, name, value, Some(self)),
                        _ => {
                            Environment::assign(&self.current_environment, name, value).map(|_| ())
                        }
                    },
                }
            }
            "member_expression" => {
                let object = unit.child_by_field(node, "object");
                let property = unit.child_by_field(node, "property");
                let obj = self.visit_expression(unit, object.unwrap_or(node))?;
                let name = property
                    .map(|p| unit.text(p).to_string())
                    .unwrap_or_default();
                self.set_property(&obj, &name, value)
            }
            "null_aware_member_expression" => {
                let object = unit.child_by_field(node, "object");
                let property = unit.child_by_field(node, "property");
                let obj = self.visit_expression(unit, object.unwrap_or(node))?;
                if obj.is_null() {
                    return Ok(());
                }
                let name = property
                    .map(|p| unit.text(p).to_string())
                    .unwrap_or_default();
                self.set_property(&obj, &name, value)
            }
            "index_expression" | "null_aware_index_expression" => {
                let object = unit.child_by_field(node, "object");
                let index = unit.child_by_field(node, "index");
                let obj = self.visit_expression(unit, object.unwrap_or(node))?;
                let idx = match index {
                    Some(i) => self.visit_expression(unit, i)?,
                    None => Value::Null,
                };
                self.set_index(&obj, &idx, value)
            }
            "super" => {
                let bound = Environment::get(&self.current_environment, "super")?;
                self.set_super_property(&bound, "unnamed", value)
            }
            "assignable_expression" => {
                if let Some(index) = unit.child_by_field(node, "index") {
                    let object = unit.child_by_field(node, "object");
                    let obj = self.visit_expression(unit, object.unwrap_or(node))?;
                    let idx = self.visit_expression(unit, index)?;
                    return self.set_index(&obj, &idx, value);
                }
                if let Some(property) = unit.child_by_field(node, "property") {
                    let object = unit.child_by_field(node, "object");
                    let obj = self.visit_expression(unit, object.unwrap_or(node))?;
                    return self.set_property(&obj, unit.text(property), value);
                }
                let inner = unit.named_children(node).first().copied();
                match inner {
                    Some(i) => self.assign_to_node(unit, i, value),
                    None => Err(InterpError::runtime("Malformed assignment target")),
                }
            }
            other => Err(InterpError::unimplemented(format!(
                "Assignment to '{}' is not yet supported",
                other
            ))),
        }
    }

    fn visit_type_test(
        &mut self,
        unit: &Rc<CompilationUnit>,
        id: NodeId,
    ) -> Result<Value, InterpError> {
        let children = unit.children(id);
        let mut operand: Option<NodeId> = None;
        let mut test: Option<NodeId> = None;
        let mut is_not = false;
        for &c in children {
            let kind = unit.kind(c);
            match kind {
                "type_test" => {
                    test = Some(c);
                    if let Some(op) = unit.first_child_of_kind(c, "is_operator")
                        && unit.text(op).contains('!')
                    {
                        is_not = true;
                    }
                }
                _ => {
                    if unit.nodes[c].is_named {
                        operand = Some(c);
                    } else if unit.text(c) == "is!" {
                        is_not = true;
                    }
                }
            }
        }
        let Some(operand) = operand else {
            return Err(InterpError::runtime("Malformed type test"));
        };
        let value = self.visit_expression(unit, operand)?;
        let target = match test {
            Some(t) => {
                let ttype = unit
                    .first_child_of_kind(t, "type")
                    .or_else(|| unit.first_child_of_kind(t, "type_identifier"));
                match ttype {
                    Some(tn) => self.parse_type(unit, tn),
                    None => RuntimeType::dynamic(),
                }
            }
            None => RuntimeType::dynamic(),
        };
        let result = self.type_matches(&value, &target);
        Ok(Value::Bool(if is_not { !result } else { result }))
    }

    fn visit_type_cast(
        &mut self,
        unit: &Rc<CompilationUnit>,
        id: NodeId,
    ) -> Result<Value, InterpError> {
        let children = unit.children(id);
        let mut operand: Option<NodeId> = None;
        let mut cast: Option<NodeId> = None;
        for &c in children {
            match unit.kind(c) {
                "type_cast" => cast = Some(c),
                _ => {
                    if unit.nodes[c].is_named {
                        operand = Some(c);
                    }
                }
            }
        }
        let Some(operand) = operand else {
            return Err(InterpError::runtime("Malformed type cast"));
        };
        let value = self.visit_expression(unit, operand)?;
        let target = match cast {
            Some(t) => {
                let ttype = unit.first_child_of_kind(t, "type");
                match ttype {
                    Some(tn) => self.parse_type(unit, tn),
                    None => RuntimeType::dynamic(),
                }
            }
            None => RuntimeType::dynamic(),
        };
        if self.type_matches(&value, &target) {
            Ok(value)
        } else {
            Err(InterpError::runtime(format!(
                "Type cast error: {} is not a subtype of {}",
                runtime_type_of(Some(&value))
                    .map(|t| t.name())
                    .unwrap_or_else(|| "?".to_string()),
                target.name()
            )))
        }
    }

    fn type_matches(&mut self, value: &Value, target: &RuntimeType) -> bool {
        let tname = target.name();
        let actual = runtime_type_of(Some(value))
            .map(|t| t.name())
            .unwrap_or_default();
        if tname == "dynamic" || tname == "Object" || tname == "?" {
            return true;
        }
        if actual == tname {
            return true;
        }
        match value {
            Value::Int(_) | Value::Double(_) => tname == "num",
            Value::Str(_) => tname == "Pattern" || tname == "Comparable",
            Value::Function(_) | Value::NativeFn(_) => tname == "Function",
            Value::List(_) | Value::Set(_) => tname == "Iterable",
            Value::Bridged(_) => match Environment::get(&self.current_environment, &tname) {
                Ok(Value::BridgedClass(bc)) => {
                    bc.is_subtype_of_fn.as_ref().is_some_and(|f| f(value))
                }
                _ => false,
            },
            _ => false,
        }
    }

    fn visit_instantiation(
        &mut self,
        unit: &Rc<CompilationUnit>,
        id: NodeId,
    ) -> Result<Value, InterpError> {
        let type_node = unit.child_by_field(id, "type");
        let constructor = unit.child_by_field(id, "constructor");
        let arguments = unit.child_by_field(id, "arguments");
        let (positional, named) = self.evaluate_arguments(unit, arguments)?;
        let class_name = match type_node {
            Some(t) => {
                let base = unit
                    .first_child_of_kind(t, "type_identifier")
                    .or_else(|| unit.first_child_of_kind(t, "qualified"))
                    .or_else(|| unit.first_child_of_kind(t, "identifier"))
                    .map(|n| unit.text(n).to_string());
                match base {
                    Some(b) => b,
                    None => unit.text(t).to_string(),
                }
            }
            None => match constructor {
                Some(c) => unit.text(c).to_string(),
                None => {
                    return Err(InterpError::runtime("Malformed instantiation (no type)"));
                }
            },
        };
        let class_value = Environment::get(&self.current_environment, &class_name)?;
        let Value::BridgedClass(bc) = &class_value else {
            let Value::Class(class_def) = class_value else {
                return Err(InterpError::runtime(format!("{class_name} is not a class")));
            };
            let ctor_name = if unit.child_by_field(id, "constructor").is_some() {
                let ctext = unit.text(unit.child_by_field(id, "constructor").unwrap());
                if ctext.contains('.') {
                    ctext.split('.').next_back().unwrap_or("").to_string()
                } else {
                    "".to_string()
                }
            } else {
                "".to_string()
            };
            return self.instantiate_class(&class_def, &ctor_name, positional, named);
        };
        let ctor_name = if unit.child_by_field(id, "constructor").is_some() {
            let ctext = unit.text(unit.child_by_field(id, "constructor").unwrap());
            if ctext.contains('.') {
                ctext.split('.').next_back().unwrap_or("").to_string()
            } else {
                "".to_string()
            }
        } else {
            "".to_string()
        };
        if let Some(c) = bc.find_constructor_adapter(&ctor_name) {
            return c(self, positional, named);
        }
        Err(InterpError::runtime(format!(
            "{class_name} has no constructor '{ctor_name}'"
        )))
    }

    fn instantiate_class(
        &mut self,
        class_def: &Rc<ClassDef>,
        ctor_name: &str,
        positional: Vec<Value>,
        named: HashMap<String, Value>,
    ) -> Result<Value, InterpError> {
        if let Some(redirect) = class_def.factory_redirects.borrow().get(ctor_name).cloned() {
            return self.instantiate_class(
                &redirect.target_class,
                &redirect.target_ctor,
                positional,
                named,
            );
        }
        let ctor = class_def.find_constructor(ctor_name);
        let unit = class_def.unit.clone();
        if let Some(c) = &ctor
            && let Some(init) = c.initializers
        {
            let redirect_style = unit.kind(init) == "redirection";
            let has_this_token = !unit.children_of_kind(init, "this").is_empty();
            for entry in unit.named_children(init) {
                let kids: Vec<NodeId> = unit.named_children(entry);
                let is_this_entry = (redirect_style && has_this_token)
                    || unit.kind(entry) == "this"
                    || !unit.children_of_kind(entry, "this").is_empty();
                if !is_this_entry {
                    continue;
                }
                let mut target = "";
                for &k in kids.iter() {
                    if unit.kind(k) == "identifier" {
                        target = unit.text(k);
                        break;
                    }
                }
                let args = if unit.kind(entry) == "arguments" {
                    Some(entry)
                } else {
                    kids.iter().copied().find(|&k| unit.kind(k) == "arguments")
                };
                let (pos, nam) = match args {
                    Some(a) => self.evaluate_arguments(&unit, Some(a))?,
                    None => (vec![], HashMap::new()),
                };
                return self.instantiate_class(class_def, target, pos, nam);
            }
        }
        let instance = InstanceData::new(class_def.clone(), vec![]);
        let env = match &ctor {
            Some(c) => self.bind_arguments(c, positional, named)?,
            None => Environment::child(&class_def.class_definition_environment),
        };
        env.borrow_mut()
            .values
            .insert("this".to_string(), Value::Instance(instance.clone()));
        if let Some(ctor) = &ctor {
            for p in &ctor.parameters {
                if let Some(field) = p.name.strip_prefix("this.") {
                    let v = Environment::get(&env, &p.name)?;
                    instance_set(&instance, field, v, Some(self))?;
                }
            }
        }
        self.initialize_field_declarations(class_def, &instance, &env)?;
        if let Some(ctor) = &ctor {
            self.run_constructor_initializers(class_def, ctor, &instance, &env)?;
        } else {
            self.call_super_constructor(class_def, &instance, vec![], HashMap::new())?;
        }
        if let Some(ctor) = ctor {
            let result = self.call_interpreted_function_body(&ctor, &env);
            if let Err(InterpError::Return(_)) = result {
                // constructors cannot return a value; ignore
            } else {
                result?;
            }
        }
        Ok(Value::Instance(instance))
    }

    fn initialize_field_declarations(
        &mut self,
        class_def: &Rc<ClassDef>,
        instance: &Rc<RefCell<InstanceData>>,
        env: &Rc<RefCell<Environment>>,
    ) -> Result<(), InterpError> {
        let unit = class_def.unit.clone();
        let saved = self.current_environment.clone();
        self.current_environment = env.clone();
        let declarations: Vec<NodeId> = class_def.field_declarations.borrow().clone();
        for init in declarations {
            let fname = unit
                .child_by_field(init, "name")
                .map(|n| unit.text(n).to_string())
                .unwrap_or_default();
            let value = match unit.child_by_field(init, "value") {
                Some(v) => Some(self.visit_expression(&unit, v)?),
                None => None,
            };
            if let Some(value) = value {
                instance_set(instance, &fname, value, Some(self))?;
            }
        }
        self.current_environment = saved;
        Ok(())
    }

    fn run_constructor_initializers(
        &mut self,
        class_def: &Rc<ClassDef>,
        ctor: &Rc<InterpretedFunction>,
        instance: &Rc<RefCell<InstanceData>>,
        env: &Rc<RefCell<Environment>>,
    ) -> Result<(), InterpError> {
        let unit = class_def.unit.clone();
        let mut super_called = false;
        let mut super_positional: Vec<Value> = Vec::new();
        let mut super_named: HashMap<String, Value> = HashMap::new();
        let mut has_super_formals = false;

        for p in &ctor.parameters {
            if let Some(field) = p.name.strip_prefix("super.") {
                has_super_formals = true;
                let v = Environment::get(env, &p.name)?;
                if p.is_named {
                    super_named.insert(field.to_string(), v);
                } else {
                    super_positional.push(v);
                }
            }
        }

        if let Some(initializers) = ctor.initializers {
            let saved = self.current_environment.clone();
            self.current_environment = env.clone();
            let redirect_style = unit.kind(initializers) == "redirection";
            let has_super_token = !unit.children_of_kind(initializers, "super").is_empty();
            let has_this_token = !unit.children_of_kind(initializers, "this").is_empty();
            for entry in unit.named_children(initializers) {
                let kids: Vec<NodeId> = unit.named_children(entry);
                let is_super_entry = redirect_style && has_super_token
                    || unit.kind(entry) == "super"
                    || !unit.children_of_kind(entry, "super").is_empty();
                let is_this_entry = redirect_style && has_this_token
                    || unit.kind(entry) == "this"
                    || !unit.children_of_kind(entry, "this").is_empty();
                if is_super_entry {
                    let args = if unit.kind(entry) == "arguments" {
                        Some(entry)
                    } else {
                        kids.iter().copied().find(|&k| unit.kind(k) == "arguments")
                    };
                    let (pos, nam) = match args {
                        Some(a) => self.evaluate_arguments(&unit, Some(a))?,
                        None => (vec![], HashMap::new()),
                    };
                    super_called = true;
                    self.call_super_constructor(class_def, instance, pos, nam)?;
                } else if is_this_entry {
                    // Redirecting constructor call `this(...)` is handled in
                    // instantiate_class; nothing to do here.
                } else if let Some(field_init) = if unit.kind(entry) == "field_initializer" {
                    Some(entry)
                } else {
                    kids.iter()
                        .copied()
                        .find(|&k| unit.kind(k) == "field_initializer")
                } {
                    let field = unit
                        .child_by_field(field_init, "name")
                        .map(|n| unit.text(n).to_string())
                        .unwrap_or_default();
                    if let Some(v) = unit.child_by_field(field_init, "value") {
                        let value = self.visit_expression(&unit, v)?;
                        instance_set(instance, &field, value, Some(self))?;
                    }
                } else {
                    let assignment = if unit.kind(entry) == "assignment_expression" {
                        Some(entry)
                    } else {
                        kids.iter()
                            .copied()
                            .find(|&k| unit.kind(k) == "assignment_expression")
                    };
                    if let Some(assignment) = assignment {
                        let left = unit.child_by_field(assignment, "left");
                        let right = unit.child_by_field(assignment, "right");
                        if let Some(l) = left {
                            let mut field = unit.text(l).to_string();
                            if let Some(stripped) = field.strip_prefix("this.") {
                                field = stripped.to_string();
                            }
                            if let Some(r) = right {
                                let value = self.visit_expression(&unit, r)?;
                                instance_set(instance, &field, value, Some(self))?;
                            }
                        }
                    }
                }
            }
            self.current_environment = saved;
        }

        if !super_called
            && (has_super_formals
                || class_def.superclass.is_some()
                || class_def.bridged_superclass.is_some())
        {
            self.call_super_constructor(class_def, instance, super_positional, super_named)?;
        }
        Ok(())
    }

    fn call_super_constructor(
        &mut self,
        class_def: &Rc<ClassDef>,
        instance: &Rc<RefCell<InstanceData>>,
        positional: Vec<Value>,
        named: HashMap<String, Value>,
    ) -> Result<(), InterpError> {
        if let Some(sc) = class_def.superclass.clone() {
            let super_ctor = sc.find_constructor("");
            let env = match &super_ctor {
                Some(c) => self.bind_arguments(c, positional, named)?,
                None => Environment::child(&sc.class_definition_environment),
            };
            env.borrow_mut()
                .values
                .insert("this".to_string(), Value::Instance(instance.clone()));
            if let Some(ctor) = &super_ctor {
                for p in &ctor.parameters {
                    if let Some(field) = p.name.strip_prefix("this.") {
                        let v = Environment::get(&env, &p.name)?;
                        instance_set(instance, field, v, Some(self))?;
                    }
                }
            }
            self.initialize_field_declarations(&sc, instance, &env)?;
            if let Some(ctor) = &super_ctor {
                self.run_constructor_initializers(&sc, ctor, instance, &env)?;
            } else {
                self.call_super_constructor(&sc, instance, vec![], HashMap::new())?;
            }
            if let Some(ctor) = super_ctor {
                let result = self.call_interpreted_function_body(&ctor, &env);
                if let Err(InterpError::Return(_)) = result {
                } else {
                    result?;
                }
            }
        } else if let Some(bc) = class_def.bridged_superclass.clone()
            && let Some(adapter) = bc.find_constructor_adapter("")
        {
            let native = adapter(self, positional, named)?;
            instance.borrow_mut().bridged_super_object = Some(native);
        }
        Ok(())
    }

    fn visit_function_expression(
        &mut self,
        unit: &Rc<CompilationUnit>,
        id: NodeId,
    ) -> Result<Value, InterpError> {
        let parameters = unit.child_by_field(id, "parameters");
        let body = unit.child_by_field(id, "body");
        let params = match parameters {
            Some(p) if unit.kind(p) == "formal_parameter_list" => {
                self.parse_parameter_list(unit, p)?
            }
            _ => vec![],
        };
        let f = self.build_function(
            unit, None, params, None, body, false, false, false, false, false,
        );
        Ok(Value::Function(f))
    }

    // =========================================================================
    // Property access
    // =========================================================================

    fn get_property(&mut self, target: &Value, name: &str) -> Result<Value, InterpError> {
        match target {
            Value::Instance(inst) => instance_get(inst, name, Some(self)),
            Value::EnumValue(ev) => enum_value_get(ev, name, Some(self)),
            Value::Record(r) => {
                let r = r.borrow();
                if let Some(n) = name.strip_prefix('$')
                    && let Ok(idx) = n.parse::<usize>()
                {
                    if idx < r.positional_fields.len() {
                        return Ok(r.positional_fields[idx].clone());
                    }
                    return Err(InterpError::runtime(format!(
                        "Record has no positional field ${idx}"
                    )));
                }
                if let Some((_, v)) = r.named_fields.iter().find(|(k, _)| k == name) {
                    return Ok(v.clone());
                }
                Err(InterpError::runtime(format!(
                    "Record has no field '{name}'"
                )))
            }
            Value::Class(c) => self.get_class_member(c, name),
            Value::EnumType(e) => self.get_enum_type_member(e, name),
            Value::ExtTypeInst(e) => self.get_ext_type_inst_member(e, name),
            Value::BoundSuper(_b) => self.get_super_property(target, name),
            Value::BoundBridgedSuper(b) => {
                let bc = b.start_lookup_class.clone();
                let target = Value::Instance(b.instance.clone());
                if let Some(g) = bc.find_instance_getter_adapter(name) {
                    return g(self, target);
                }
                if bc.find_instance_method_adapter(name).is_some() {
                    return Ok(Value::BridgedSuperMethod(Rc::new(
                        BridgedSuperMethodCallable {
                            native_target: target,
                            method_name: name.to_string(),
                            bridged_class: bc,
                        },
                    )));
                }
                Err(InterpError::runtime(format!(
                    "Undefined property '{name}' on bridged super"
                )))
            }
            Value::Extension(ext) => self.get_extension_member(ext, name),
            Value::Module(env) => match Environment::get(env, name) {
                Ok(v) => Ok(v),
                Err(_) => Err(InterpError::runtime(format!(
                    "Undefined name '{name}' in imported prefix"
                ))),
            },
            Value::Bridged(b) => self.bridged_instance_get(b, name),
            Value::BridgedEnumValue(b) => {
                let be = b.borrow();
                if name == "name" {
                    return Ok(Value::str(be.name.clone()));
                }
                if name == "index" {
                    return Ok(Value::Int(0));
                }
                if let Some(f) = be.fields.get(name) {
                    return Ok(f.clone());
                }
                let parent = be.parent_enum.clone();
                let target = Value::BridgedEnumValue(b.clone());
                if let Some(g) = parent.getters.get(name) {
                    return g(self, target);
                }
                if parent.methods.contains_key(name) {
                    return Ok(Value::BridgedEnumMixinMethod(Rc::new(
                        BridgedEnumMixinMethodCallable {
                            enum_value: target,
                            method_name: name.to_string(),
                            call_target: BridgedEnumMixinCallTarget::EnumDef(parent.clone()),
                        },
                    )));
                }
                Err(InterpError::runtime(format!(
                    "Undefined property '{name}' on bridged enum value"
                )))
            }
            Value::BridgedClass(bc) => self.get_bridged_class_member(bc, name),
            Value::BridgedEnum(bc) => {
                if let Some(v) = bc.values.borrow().iter().find(|v| v.name == name) {
                    return Ok(Value::BridgedEnumValue(Rc::new(RefCell::new(
                        v.as_ref().clone(),
                    ))));
                }
                Err(InterpError::runtime(format!(
                    "Undefined value '{name}' on bridged enum {}",
                    bc.name
                )))
            }
            Value::PropertyAccessor(p) => match &p.getter {
                Some(g) => call_callable(g, self, vec![], HashMap::new()),
                None => Err(InterpError::runtime(format!(
                    "Property '{}' has no getter",
                    p.name
                ))),
            },
            Value::Null => match self.wrap_native(target) {
                Ok(bridged) => {
                    let b = match &bridged {
                        Value::Bridged(b) => b.clone(),
                        _ => unreachable!(),
                    };
                    self.bridged_instance_get(&b, name)
                }
                Err(_) => Err(InterpError::runtime(format!(
                    "Undefined property '{name}' on null"
                ))),
            },
            Value::List(items) if name == "length" => Ok(Value::Int(items.borrow().len() as i64)),
            Value::Map(m) if name == "length" => Ok(Value::Int(m.borrow().entries.len() as i64)),
            Value::Set(s) if name == "length" => Ok(Value::Int(s.borrow().items.len() as i64)),
            Value::Str(s) if name == "length" => Ok(Value::Int(s.chars().count() as i64)),
            native @ (Value::Int(_)
            | Value::Double(_)
            | Value::Str(_)
            | Value::Bool(_)
            | Value::List(_)
            | Value::Map(_)
            | Value::Set(_)
            | Value::Type(_)
            | Value::StringBuffer(_)
            | Value::BigInt(_)
            | Value::Function(_)
            | Value::NativeFn(_)
            | Value::SyncGen(_)
            | Value::Future(_)) => {
                let bridged = self.wrap_native(native)?;
                let b = match &bridged {
                    Value::Bridged(b) => b.clone(),
                    _ => unreachable!(),
                };
                self.bridged_instance_get(&b, name)
            }
            other => Err(InterpError::runtime(format!(
                "Undefined property '{name}' on {}",
                value_display(other)
            ))),
        }
    }

    fn wrap_native(&mut self, value: &Value) -> Result<Value, InterpError> {
        match Environment::to_bridged_instance(&self.current_environment, value) {
            Some(v) => Ok(v),
            None => Err(InterpError::runtime(format!(
                "No bridged class registered for {}",
                value_display(value)
            ))),
        }
    }

    pub fn display_value(&mut self, value: &Value) -> Result<String, InterpError> {
        if let Value::Bridged(b) = value {
            let bc = b.borrow().bridged_class.clone();
            let native = b.borrow().native.clone();
            if let Some(m) = bc.find_instance_method_adapter("toString") {
                let s = m(self, native, vec![], HashMap::new())?;
                if let Value::Str(s) = s {
                    return Ok(s.to_string());
                }
            }
        }
        Ok(value_display(value))
    }

    pub(crate) fn bridged_instance_get(
        &mut self,
        bridged: &Rc<RefCell<BridgedInstanceData>>,
        name: &str,
    ) -> Result<Value, InterpError> {
        let bc = bridged.borrow().bridged_class.clone();
        let native = bridged.borrow().native.clone();
        if name == "runtimeType" {
            return Ok(Value::Type(Rc::new(RuntimeType::Native(bc))));
        }
        if let Some(g) = bc.find_instance_getter_adapter(name) {
            return g(self, native);
        }
        if bc.find_instance_method_adapter(name).is_some() {
            return Ok(Value::BridgedSuperMethod(Rc::new(
                BridgedSuperMethodCallable {
                    native_target: native,
                    method_name: name.to_string(),
                    bridged_class: bc,
                },
            )));
        }
        Err(InterpError::runtime(format!(
            "Undefined property '{name}' on {}",
            bc.name
        )))
    }

    fn get_class_member(
        &mut self,
        class_def: &Rc<ClassDef>,
        name: &str,
    ) -> Result<Value, InterpError> {
        if let Some(m) = class_def.find_static_method(name) {
            return Ok(Value::Function(m.bind_class(class_def.clone())));
        }
        if let Some(g) = class_def.find_static_getter(name) {
            let bound = g.clone();
            return call_callable(&Callable::Interpreted(bound), self, vec![], HashMap::new());
        }
        if let Ok(f) = class_def.get_static_field(name) {
            return Ok(f);
        }
        if let Some(c) = class_def.find_constructor(name) {
            return Ok(Value::Function(c));
        }
        if let Some(bc) = &class_def.bridged_superclass {
            if let Some(g) = bc.find_static_getter_adapter(name) {
                return g(self);
            }
            if let Some(_m) = bc.find_static_method_adapter(name) {
                let bc = bc.clone();
                let native = Value::BridgedClass(bc.clone());
                return Ok(Value::BridgedSuperMethod(Rc::new(
                    BridgedSuperMethodCallable {
                        native_target: native,
                        method_name: name.to_string(),
                        bridged_class: bc,
                    },
                )));
            }
        }
        Err(InterpError::runtime(format!(
            "Undefined static member '{name}' on {}",
            class_def.name
        )))
    }

    fn get_enum_type_member(
        &mut self,
        enum_def: &Rc<EnumDef>,
        name: &str,
    ) -> Result<Value, InterpError> {
        if name == "values" {
            let mut items = Vec::new();
            for value_name in &enum_def.value_names {
                if let Some(v) = enum_def.values.borrow().get(value_name) {
                    items.push(Value::EnumValue(v.clone()));
                }
            }
            return Ok(Value::List(Rc::new(RefCell::new(items))));
        }
        if let Some(v) = enum_def.values.borrow().get(name) {
            return Ok(Value::EnumValue(v.clone()));
        }
        if let Some(m) = enum_def.static_methods.borrow().get(name) {
            return Ok(Value::Function(m.clone()));
        }
        if let Some(g) = enum_def.static_getters.borrow().get(name) {
            return call_callable(
                &Callable::Interpreted(g.clone()),
                self,
                vec![],
                HashMap::new(),
            );
        }
        if let Ok(f) = enum_def
            .static_fields
            .borrow()
            .get(name)
            .cloned()
            .ok_or(InterpError::runtime(""))
        {
            return Ok(f);
        }
        Err(InterpError::runtime(format!(
            "Undefined static member '{name}' on enum {}",
            enum_def.name
        )))
    }

    fn get_ext_type_inst_member(
        &mut self,
        ext_inst: &Rc<RefCell<ExtTypeInstData>>,
        name: &str,
    ) -> Result<Value, InterpError> {
        let ext = ext_inst.borrow().extension_type.clone();
        if let Some(m) = ext.find_instance_method(name) {
            let mut f = (*m).clone();
            f.bound_instance = Some(ext_inst.borrow().representation_value.clone());
            return Ok(Value::Function(Rc::new(f)));
        }
        if let Some(g) = ext.find_instance_getter(name) {
            let mut f = (*g).clone();
            f.bound_instance = Some(ext_inst.borrow().representation_value.clone());
            return call_callable(
                &Callable::Interpreted(Rc::new(f)),
                self,
                vec![],
                HashMap::new(),
            );
        }
        if name == ext_inst.borrow().representation_field_name {
            return Ok(ext_inst.borrow().representation_value.clone());
        }
        Err(InterpError::runtime(format!(
            "Undefined property '{name}' on extension type {}",
            ext.name
        )))
    }

    fn get_extension_member(
        &mut self,
        ext: &Rc<crate::class::ExtensionDef>,
        name: &str,
    ) -> Result<Value, InterpError> {
        if let Some(m) = ext.find_static_method(name) {
            return Ok(Value::Function(m));
        }
        if let Some(g) = ext.find_static_getter(name) {
            return call_callable(
                &Callable::Interpreted(g.clone()),
                self,
                vec![],
                HashMap::new(),
            );
        }
        if let Ok(f) = ext.get_static_field(name) {
            return Ok(f);
        }
        Err(InterpError::runtime(format!(
            "Undefined static member '{name}' on extension"
        )))
    }

    pub(crate) fn get_bridged_class_member(
        &mut self,
        bridged_class: &Arc<crate::class::BridgedClassDef>,
        name: &str,
    ) -> Result<Value, InterpError> {
        if let Some(g) = bridged_class.find_static_getter_adapter(name) {
            return g(self);
        }
        if let Some(_m) = bridged_class.find_static_method_adapter(name) {
            let bc = bridged_class.clone();
            let native = Value::BridgedClass(bc.clone());
            return Ok(Value::BridgedSuperMethod(Rc::new(
                BridgedSuperMethodCallable {
                    native_target: native,
                    method_name: name.to_string(),
                    bridged_class: bc,
                },
            )));
        }
        if let Some(_c) = bridged_class.find_constructor_adapter(name) {
            let bc = bridged_class.clone();
            let ctor_name = name.to_string();
            return Ok(Value::NativeFn(Rc::new(NativeFunction {
                name: Some(format!("{}.{}", bc.name, ctor_name)),
                min_arity: 0,
                max_arity: None,
                is_async: false,
                is_generator: false,
                closure: Box::new(move |v, positional, named| {
                    let c = bc
                        .find_constructor_adapter(&ctor_name)
                        .expect("constructor adapter");
                    c(v, positional, named)
                }),
                callable_runtime_type: crate::rt::FunctionRuntimeType::untyped(),
            })));
        }
        Err(InterpError::runtime(format!(
            "Undefined static member '{name}' on {}",
            bridged_class.name
        )))
    }

    fn get_super_property(&mut self, bound: &Value, name: &str) -> Result<Value, InterpError> {
        let Value::BoundSuper(b) = bound else {
            let Value::BoundBridgedSuper(bb) = bound else {
                return Err(InterpError::runtime("'super' is not bound in this context"));
            };
            let bc = bb.start_lookup_class.clone();
            let target = Value::Instance(bb.instance.clone());
            if let Some(g) = bc.find_instance_getter_adapter(name) {
                return g(self, target);
            }
            if bc.find_instance_method_adapter(name).is_some() {
                return Ok(Value::BridgedSuperMethod(Rc::new(
                    BridgedSuperMethodCallable {
                        native_target: target,
                        method_name: name.to_string(),
                        bridged_class: bc,
                    },
                )));
            }
            return Err(InterpError::runtime(format!(
                "Undefined property '{name}' on bridged super"
            )));
        };
        let inst = b.instance.clone();
        let mut current = b.start_lookup_class.superclass.clone();
        while let Some(c) = current {
            if let Some(getter) = c.find_instance_getter(name) {
                let bound = getter.bind_instance(inst.clone());
                return call_callable(&Callable::Interpreted(bound), self, vec![], HashMap::new());
            }
            if let Some(m) = c.find_instance_method(name) {
                return Ok(Value::Function(m.bind_instance(inst.clone())));
            }
            if let Some(bc) = &c.bridged_superclass {
                let native_target = inst.borrow().bridged_super_object.clone();
                if let Some(nt) = native_target {
                    if let Some(g) = bc.find_instance_getter_adapter(name) {
                        return g(self, nt);
                    }
                    if bc.find_instance_method_adapter(name).is_some() {
                        return Ok(Value::BridgedSuperMethod(Rc::new(
                            BridgedSuperMethodCallable {
                                native_target: nt,
                                method_name: name.to_string(),
                                bridged_class: bc.clone(),
                            },
                        )));
                    }
                }
            }
            current = c.superclass.clone();
        }
        Err(InterpError::runtime(format!(
            "Undefined property '{name}' on super"
        )))
    }

    fn set_property(
        &mut self,
        target: &Value,
        name: &str,
        value: Value,
    ) -> Result<(), InterpError> {
        match target {
            Value::Instance(inst) => instance_set(inst, name, value, Some(self)),
            Value::EnumValue(ev) => enum_value_set(ev, name, value, Some(self)),
            Value::Class(c) => {
                if let Some(s) = c.find_static_setter(name) {
                    let bound = s.clone();
                    call_callable(
                        &Callable::Interpreted(bound),
                        self,
                        vec![value],
                        HashMap::new(),
                    )?;
                    return Ok(());
                }
                let has_static = c.static_fields.borrow().contains_key(name);
                if has_static {
                    c.set_static_field(name, value);
                    return Ok(());
                }
                Err(InterpError::runtime(format!(
                    "Cannot set undefined static property '{name}' on {}",
                    c.name
                )))
            }
            Value::BoundSuper(b) => {
                let inst = b.instance.clone();
                let mut current = b.start_lookup_class.superclass.clone();
                while let Some(c) = current {
                    if let Some(s) = c.find_instance_setter(name) {
                        let bound = s.bind_instance(inst.clone());
                        call_callable(
                            &Callable::Interpreted(bound),
                            self,
                            vec![value],
                            HashMap::new(),
                        )?;
                        return Ok(());
                    }
                    current = c.superclass.clone();
                }
                Err(InterpError::runtime(format!(
                    "Undefined setter '{name}' on super"
                )))
            }
            Value::Bridged(b) => {
                let bc = b.borrow().bridged_class.clone();
                let native = b.borrow().native.clone();
                if let Some(s) = bc.find_instance_setter_adapter(name) {
                    s(self, native, value)?;
                    return Ok(());
                }
                Err(InterpError::runtime(format!(
                    "Cannot set property '{name}' on {}",
                    bc.name
                )))
            }
            Value::List(l) if name == "length" => {
                let len = match &value {
                    Value::Int(i) => *i,
                    Value::Double(d) => *d as i64,
                    _ => return Err(InterpError::runtime("List.length must be assigned an int")),
                };
                let mut items = l.borrow_mut();
                let len = len.max(0) as usize;
                if len < items.len() {
                    items.truncate(len);
                } else {
                    items.resize(len, Value::Null);
                }
                Ok(())
            }
            Value::List(_) => Err(InterpError::runtime(format!(
                "Cannot set property '{name}' on a List"
            ))),
            Value::Null => Err(InterpError::runtime(format!(
                "Cannot set property '{name}' on null"
            ))),
            other => Err(InterpError::runtime(format!(
                "Cannot set property '{name}' on {}",
                value_display(other)
            ))),
        }
    }

    fn set_super_property(
        &mut self,
        bound: &Value,
        name: &str,
        value: Value,
    ) -> Result<(), InterpError> {
        let Value::BoundSuper(b) = bound else {
            return Err(InterpError::runtime("'super' is not bound in this context"));
        };
        let inst = b.instance.clone();
        let mut current = b.start_lookup_class.superclass.clone();
        while let Some(c) = current {
            if let Some(s) = c.find_instance_setter(name) {
                let bound = s.bind_instance(inst.clone());
                call_callable(
                    &Callable::Interpreted(bound),
                    self,
                    vec![value],
                    HashMap::new(),
                )?;
                return Ok(());
            }
            current = c.superclass.clone();
        }
        Err(InterpError::runtime(format!(
            "Undefined setter '{name}' on super"
        )))
    }

    fn lookup_class_or_error(&mut self, name: &str) -> Result<Value, InterpError> {
        Environment::get(&self.current_environment, name)
    }

    // =========================================================================
    // Calling
    // =========================================================================

    pub fn call_value(
        &mut self,
        callee: &Value,
        positional: Vec<Value>,
        named: HashMap<String, Value>,
    ) -> Result<Value, InterpError> {
        match callee.to_callable() {
            Some(c) => call_callable(&c, self, positional, named),
            None => match callee {
                Value::BridgedSuperMethod(m) => {
                    self.call_bridged_super_method(m, positional, named)
                }
                Value::BridgedMixinMethod(m) => {
                    self.call_bridged_mixin_method(m, positional, named)
                }
                Value::BridgedEnumMixinMethod(m) => {
                    self.call_bridged_enum_mixin_method(m, positional, named)
                }
                Value::BridgedClass(bc) => {
                    if let Some(c) = bc.find_constructor_adapter("") {
                        return c(self, positional, named);
                    }
                    Err(InterpError::runtime(format!(
                        "{} has no unnamed constructor",
                        bc.name
                    )))
                }
                other => Err(InterpError::runtime(format!(
                    "{} is not a function",
                    value_display(other)
                ))),
            },
        }
    }

    pub fn call_interpreted_function(
        &mut self,
        f: &Rc<InterpretedFunction>,
        positional: Vec<Value>,
        named: HashMap<String, Value>,
    ) -> Result<Value, InterpError> {
        if f.is_async && f.is_generator {
            return self.call_async_star_function(f, positional, named);
        }
        if f.is_async {
            return self.call_async_function(f, positional, named);
        }
        if f.is_constructor {
            let mut current = Some(f.environment.clone());
            while let Some(env) = current {
                let env = env.borrow();
                let found = env.values.iter().find_map(|(_, v)| match v {
                    Value::Class(cls) => Some(cls.clone()),
                    _ => None,
                });
                if let Some(cls) = found {
                    drop(env);
                    let ctor_name = f.name.clone().unwrap_or_default();
                    return self.instantiate_class(&cls, &ctor_name, positional, named);
                }
                current = env.enclosing.clone();
            }
            return Err(InterpError::runtime(format!(
                "Constructor '{}' has no associated class",
                f.name.clone().unwrap_or_default()
            )));
        }
        if f.is_generator {
            let env = self.bind_arguments(f, positional, named)?;
            let generator = SyncGenerator::new(f.clone(), env.clone());
            return Ok(Value::SyncGen(generator));
        }
        let env = self.bind_arguments(f, positional, named)?;
        let saved_skip = self.take_skip_state();
        let result = self.call_interpreted_function_body(f, &env);
        self.restore_skip_state(saved_skip);
        result
    }

    fn take_skip_state(&mut self) -> SkipState {
        (
            std::mem::take(&mut self.await_cache),
            std::mem::take(&mut self.loop_iterations),
            std::mem::take(&mut self.resume_state),
        )
    }

    fn restore_skip_state(&mut self, state: SkipState) {
        self.await_cache = state.0;
        self.loop_iterations = state.1;
        self.resume_state = state.2;
    }

    /// Invokes an `async` function: binds arguments, runs the body inside
    /// an async frame and returns a [`Value::Future`] for the completer.
    /// A suspension (`InterpError::Suspend`) simply registers a
    /// continuation on the pending future; the event loop resumes it.
    fn call_async_function(
        &mut self,
        f: &Rc<InterpretedFunction>,
        positional: Vec<Value>,
        named: HashMap<String, Value>,
    ) -> Result<Value, InterpError> {
        let env = self.bind_arguments(f, positional, named)?;
        let state = Rc::new(RefCell::new(crate::async_state::FutureState::new()));
        let saved_skip = self.take_skip_state();
        self.async_frames.push(crate::async_state::AsyncFrame {
            unit: f.unit.clone(),
            body: f.body,
            completer: state.clone(),
            stream_state: None,
        });
        let result = self.call_interpreted_function_body(f, &env);
        self.async_frames.pop();
        self.restore_skip_state(saved_skip);
        match result {
            Ok(v) => crate::async_state::complete(&state, Ok(v)),
            Err(InterpError::Suspend) => {}
            Err(e) => {
                crate::async_state::complete(&state, Err(crate::async_state::exception_value(e)))
            }
        }
        Ok(Value::Future(state))
    }

    /// Invokes an `async*` generator: binds arguments, creates the stream
    /// the body emits into and runs the body until its first suspension.
    /// Returns the stream; the body is resumed by tasks (each `yield`
    /// enqueues a [`Task::Resume`], each completed `await` resumes through
    /// its continuation) until it finishes, at which point the stream is
    /// closed via the `AsyncStarDone` continuation on the completer.
    fn call_async_star_function(
        &mut self,
        f: &Rc<InterpretedFunction>,
        positional: Vec<Value>,
        named: HashMap<String, Value>,
    ) -> Result<Value, InterpError> {
        let env = self.bind_arguments(f, positional, named)?;
        let (stream, state) = crate::stdlib::stream_lib::make_gen_stream();
        let completer = Rc::new(RefCell::new(crate::async_state::FutureState::new()));
        completer.borrow_mut().continuations.push(
            crate::async_state::Continuation::AsyncStarDone {
                state: state.clone(),
            },
        );
        self.run_async_star_pass(&f.unit, f.body, &env, &completer, &state);
        Ok(stream)
    }

    /// Runs one pass of an `async*` body: replays the body top-down with
    /// the captured cache/resume state, then handles the outcome.
    fn run_async_star_pass(
        &mut self,
        unit: &Rc<CompilationUnit>,
        body: NodeId,
        env: &Rc<RefCell<Environment>>,
        completer: &Rc<RefCell<crate::async_state::FutureState>>,
        state: &Value,
    ) {
        if std::env::var("D4RT_DBG").is_ok() {
            eprintln!("[dbg] gen pass start resume={:?}", self.resume_state);
        }
        let saved_skip = self.take_skip_state();
        let saved_env = self.current_environment.clone();
        self.current_environment = env.clone();
        self.async_frames.push(crate::async_state::AsyncFrame {
            unit: unit.clone(),
            body,
            completer: completer.clone(),
            stream_state: Some(state.clone()),
        });
        let result = self.execute_function_body_with_frame(unit, body, "<async*>");
        self.async_frames.pop();
        self.current_environment = saved_env;
        self.restore_skip_state(saved_skip);
        if std::env::var("D4RT_DBG").is_ok() {
            eprintln!(
                "[dbg] gen pass end result={:?}",
                result.as_ref().map(|_| "<ok>").or(Err(()))
            );
        }
        match result {
            Ok(v) => crate::async_state::complete(completer, Ok(v)),
            Err(InterpError::Suspend) => {}
            Err(e) => {
                crate::async_state::complete(completer, Err(crate::async_state::exception_value(e)))
            }
        }
    }

    /// Resumes a suspended async body: restores the captured environment
    /// and cache, then re-runs the body top-down. Every already-executed
    /// `await` replays its cached future instead of re-evaluating its
    /// expression; only never-seen awaits evaluate fresh futures (and
    /// suspend again). Because each pass replays identically, the pass
    /// that reaches the end of the function produces the correct result.
    pub fn resume_continuation(
        &mut self,
        cont: &crate::async_state::ResumeContinuation,
    ) -> Result<Value, InterpError> {
        if std::env::var("D4RT_DBG").is_ok() {
            eprintln!(
                "[dbg] resume cont node={} iters={:?} stream_state={}",
                cont.resume_node,
                cont.resume_iters,
                cont.stream_state.is_some()
            );
        }
        self.current_environment = cont.function_env.clone();
        self.await_cache = cont.cache.clone();
        self.loop_iterations = Vec::new();
        self.resume_state = Some((cont.resume_node, cont.resume_iters.clone()));
        self.async_frames.push(crate::async_state::AsyncFrame {
            unit: cont.unit.clone(),
            body: cont.body,
            completer: cont.completer.clone(),
            stream_state: cont.stream_state.clone(),
        });
        let result = self.execute_function_body_with_frame(&cont.unit, cont.body, "<async-resume>");
        self.async_frames.pop();
        result
    }

    /// Drains the async task queue, running each task through `self`.
    /// Returns whether any task was processed.
    pub fn pump_tasks(&mut self) -> bool {
        let mut any = false;
        while let Some(task) = crate::async_state::dequeue_task() {
            any = true;
            crate::async_state::process_task(self, task);
        }
        any
    }

    fn bind_arguments(
        &mut self,
        f: &Rc<InterpretedFunction>,
        positional: Vec<Value>,
        named: HashMap<String, Value>,
    ) -> Result<Rc<RefCell<Environment>>, InterpError> {
        let required = f.required_positional_count();
        let total_positional = f.positional_param_count();
        if positional.len() < required {
            return Err(InterpError::runtime(format!(
                "Function '{}' expects at least {required} positional argument(s), got {}",
                f.name.clone().unwrap_or_else(|| "<anonymous>".to_string()),
                positional.len()
            )));
        }
        if positional.len() > total_positional {
            return Err(InterpError::runtime(format!(
                "Function '{}' expects at most {total_positional} positional argument(s), got {}",
                f.name.clone().unwrap_or_else(|| "<anonymous>".to_string()),
                positional.len()
            )));
        }
        for k in named.keys() {
            if !f
                .parameters
                .iter()
                .any(|p| p.is_named && p.named_key() == k)
            {
                return Err(InterpError::runtime(format!(
                    "Function '{}' has no named parameter '{}'",
                    f.name.clone().unwrap_or_else(|| "<anonymous>".to_string()),
                    k
                )));
            }
        }
        let env = Environment::child(&f.environment);
        if let Some(inst) = &f.bound_instance {
            env.borrow_mut()
                .values
                .insert("this".to_string(), inst.clone());
            if let Value::Instance(inst_ref) = inst {
                let class = {
                    let mut current = Some(f.environment.clone());
                    let mut found = None;
                    while let Some(env) = current {
                        let env_ref = env.borrow();
                        let hit = env_ref.values.iter().find_map(|(_, v)| match v {
                            Value::Class(cls) => Some(cls.clone()),
                            _ => None,
                        });
                        if hit.is_some() {
                            found = hit;
                            break;
                        }
                        current = env_ref.enclosing.clone();
                    }
                    found
                };
                if let Some(cls) = class {
                    if let Some(superclass) = &cls.superclass {
                        let sup = Value::BoundSuper(Rc::new(BoundSuperData {
                            instance: inst_ref.clone(),
                            start_lookup_class: cls.clone(),
                        }));
                        let _ = superclass;
                        env.borrow_mut().values.insert("super".to_string(), sup);
                    } else if let Some(bridged_super) = &cls.bridged_superclass {
                        let sup = Value::BoundBridgedSuper(Rc::new(BoundBridgedSuperData {
                            instance: inst_ref.clone(),
                            start_lookup_class: bridged_super.clone(),
                        }));
                        env.borrow_mut().values.insert("super".to_string(), sup);
                    }
                }
            }
        }
        if let Some(ev) = &f.bound_enum_value {
            env.borrow_mut()
                .values
                .insert("this".to_string(), ev.clone());
        }
        let mut positional_iter = positional.into_iter();
        let saved = self.current_environment.clone();
        for p in &f.parameters {
            if p.is_named {
                let value = match named.get(p.named_key()) {
                    Some(v) => v.clone(),
                    None => {
                        if p.has_default {
                            self.current_environment = env.clone();
                            let def = self.visit_expression(&f.unit, p.default.unwrap_or(0))?;
                            self.current_environment = saved.clone();
                            def
                        } else if p.is_required {
                            return Err(InterpError::runtime(format!(
                                "Missing required named parameter '{}' in call to '{}'",
                                p.name,
                                f.name.clone().unwrap_or_else(|| "<anonymous>".to_string())
                            )));
                        } else {
                            Value::Null
                        }
                    }
                };
                Environment::define(&env, &p.name, value);
            } else {
                let value = match positional_iter.next() {
                    Some(v) => v,
                    None => {
                        if p.has_default {
                            self.current_environment = env.clone();
                            let def = self.visit_expression(&f.unit, p.default.unwrap_or(0))?;
                            self.current_environment = saved.clone();
                            def
                        } else {
                            Value::Null
                        }
                    }
                };
                Environment::define(&env, &p.name, value);
            }
        }
        self.current_environment = saved;
        Ok(env)
    }

    fn call_interpreted_function_body(
        &mut self,
        f: &Rc<InterpretedFunction>,
        env: &Rc<RefCell<Environment>>,
    ) -> Result<Value, InterpError> {
        if self.call_depth >= MAX_CALL_DEPTH {
            return Err(InterpError::runtime(format!(
                "Stack overflow: maximum call depth ({MAX_CALL_DEPTH}) exceeded — \
                 runaway recursion in Dart code"
            )));
        }
        self.call_depth += 1;
        let name = f.name.clone().unwrap_or_else(|| "<anonymous>".to_string());
        let saved = self.current_environment.clone();
        self.current_environment = env.clone();
        let result = self.execute_function_body_with_frame(&f.unit, f.body, &name);
        self.current_environment = saved;
        self.call_depth -= 1;
        result
    }

    fn execute_function_body_with_frame(
        &mut self,
        unit: &Rc<CompilationUnit>,
        body: NodeId,
        name: &str,
    ) -> Result<Value, InterpError> {
        self.push_frame(unit, name, body);
        let result = self.execute_function_body(unit, body);
        self.pop_frame();
        match result {
            Err(InterpError::Return(v)) => Ok(v),
            other => other,
        }
    }

    fn execute_function_body(
        &mut self,
        unit: &Rc<CompilationUnit>,
        body: NodeId,
    ) -> Result<Value, InterpError> {
        if body == 0 {
            return Ok(Value::Null);
        }
        let kind = unit.kind(body);
        if kind == "block" {
            return self.visit_block(unit, body);
        }
        if kind == "function_body" || kind == "function_expression_body" {
            let children = unit.named_children(body);
            if let Some(&block) = children.iter().find(|&&c| unit.kind(c) == "block") {
                return self.visit_block(unit, block);
            }
            if let Some(&expr) = children.first() {
                return self.visit_expression(unit, expr);
            }
            return Ok(Value::Null);
        }
        self.visit_expression(unit, body)
    }

    pub fn call_native_function(
        &mut self,
        native: &Rc<NativeFunction>,
        positional: Vec<Value>,
        named: HashMap<String, Value>,
    ) -> Result<Value, InterpError> {
        if positional.len() < native.min_arity {
            return Err(InterpError::runtime(format!(
                "Native function '{}' expects at least {} argument(s), got {}",
                native
                    .name
                    .clone()
                    .unwrap_or_else(|| "<anonymous>".to_string()),
                native.min_arity,
                positional.len()
            )));
        }
        if let Some(max) = native.max_arity
            && positional.len() > max
        {
            return Err(InterpError::runtime(format!(
                "Native function '{}' expects at most {max} argument(s), got {}",
                native
                    .name
                    .clone()
                    .unwrap_or_else(|| "<anonymous>".to_string()),
                positional.len()
            )));
        }
        (native.closure)(self, positional, named)
    }

    pub fn call_constructor(
        &mut self,
        class_def: &Rc<ClassDef>,
        positional: Vec<Value>,
        named: HashMap<String, Value>,
    ) -> Result<Value, InterpError> {
        self.instantiate_class(class_def, "", positional, named)
    }

    pub fn call_ext_type_constructor(
        &mut self,
        ext_type: &Rc<ExtTypeDef>,
        positional: Vec<Value>,
        named: HashMap<String, Value>,
    ) -> Result<Value, InterpError> {
        let representation = positional
            .first()
            .cloned()
            .or_else(|| named.get(&ext_type.representation_field_name).cloned())
            .unwrap_or(Value::Null);
        Ok(Value::ExtTypeInst(Rc::new(RefCell::new(ExtTypeInstData {
            extension_type: ext_type.clone(),
            representation_field_name: ext_type.representation_field_name.clone(),
            representation_value: representation,
        }))))
    }

    pub fn call_bridged_super_method(
        &mut self,
        m: &Rc<BridgedSuperMethodCallable>,
        positional: Vec<Value>,
        named: HashMap<String, Value>,
    ) -> Result<Value, InterpError> {
        match &m.native_target {
            Value::BridgedClass(bc) => match bc.find_static_method_adapter(&m.method_name) {
                Some(adapter) => adapter(self, positional, named),
                None => Err(InterpError::runtime(format!(
                    "Bridged static method '{}' not found on {}",
                    m.method_name, bc.name
                ))),
            },
            _ => match m.bridged_class.find_instance_method_adapter(&m.method_name) {
                Some(adapter) => adapter(self, m.native_target.clone(), positional, named),
                None => Err(InterpError::runtime(format!(
                    "Bridged method '{}' not found on {}",
                    m.method_name, m.bridged_class.name
                ))),
            },
        }
    }

    pub fn call_bridged_mixin_method(
        &mut self,
        m: &Rc<BridgedMixinMethodCallable>,
        positional: Vec<Value>,
        named: HashMap<String, Value>,
    ) -> Result<Value, InterpError> {
        match m.bridged_class.find_instance_method_adapter(&m.method_name) {
            Some(adapter) => adapter(self, m.instance.clone(), positional, named),
            None => Err(InterpError::runtime(format!(
                "Bridged mixin method '{}' not found on {}",
                m.method_name, m.bridged_class.name
            ))),
        }
    }

    pub fn call_bridged_enum_mixin_method(
        &mut self,
        m: &Rc<BridgedEnumMixinMethodCallable>,
        positional: Vec<Value>,
        named: HashMap<String, Value>,
    ) -> Result<Value, InterpError> {
        match &m.call_target {
            BridgedEnumMixinCallTarget::Class(bc) => {
                let adapter = bc
                    .find_instance_method_adapter(&m.method_name)
                    .ok_or_else(|| {
                        InterpError::runtime(format!(
                            "Bridged enum mixin method '{}' not found",
                            m.method_name
                        ))
                    })?;
                adapter(self, m.enum_value.clone(), positional, named)
            }
            BridgedEnumMixinCallTarget::EnumDef(be) => {
                let adapter = be.methods.get(&m.method_name).ok_or_else(|| {
                    InterpError::runtime(format!(
                        "Bridged enum mixin method '{}' not found",
                        m.method_name
                    ))
                })?;
                adapter(self, m.enum_value.clone(), positional, named)
            }
        }
    }

    // =========================================================================
    // Sync generators
    // =========================================================================

    pub fn run_sync_generator(
        &mut self,
        generator: &Rc<RefCell<SyncGenerator>>,
    ) -> Result<Option<Value>, InterpError> {
        {
            let mut g = generator.borrow_mut();
            if g.completed && g.next_index >= g.buffer.len() {
                return Ok(None);
            }
            if !g.started {
                g.started = true;
                let f = g.function.clone();
                let gen_env = g.environment.clone();
                drop(g);
                let saved = self.current_environment.clone();
                let saved_gen = self.sync_generator.clone();
                self.current_environment = gen_env.clone();
                self.sync_generator = Some(generator.clone());
                let result = self.execute_function_body(&f.unit, f.body);
                self.sync_generator = saved_gen;
                self.current_environment = saved;
                let mut g = generator.borrow_mut();
                g.completed = true;
                if let Err(e) = result {
                    if matches!(e, InterpError::Return(_)) {
                        return Ok(None);
                    }
                    return Err(e);
                }
            }
        }
        let mut g = generator.borrow_mut();
        if g.next_index < g.buffer.len() {
            let v = g.buffer[g.next_index].clone();
            g.next_index += 1;
            Ok(Some(v))
        } else {
            Ok(None)
        }
    }

    // =========================================================================
    // Patterns
    // =========================================================================

    fn bind_pattern(
        &mut self,
        unit: &Rc<CompilationUnit>,
        pattern: NodeId,
        value: Value,
    ) -> Result<Value, InterpError> {
        let kind = unit.kind(pattern);
        if kind == "record_pattern" {
            let fields: Vec<Value> = match &value {
                Value::Record(r) => r.borrow().positional_fields.clone(),
                Value::List(l) => l.borrow().clone(),
                Value::Bridged(b) => match &b.borrow().native {
                    Value::List(l) => l.borrow().clone(),
                    _ => Vec::new(),
                },
                other => {
                    return Err(InterpError::runtime(format!(
                        "Record pattern does not match {}",
                        value_display(other)
                    )));
                }
            };
            let mut idx = 0;
            for &child in unit.children(pattern) {
                if unit.kind(child) == "constant_pattern" || unit.kind(child) == "variable_pattern"
                {
                    if let Some(&name) = unit
                        .named_children(child)
                        .first()
                        .filter(|&&n| matches!(unit.kind(n), "identifier" | "simple_identifier"))
                    {
                        let v = fields.get(idx).cloned().unwrap_or(Value::Null);
                        Environment::define(&self.current_environment, unit.text(name), v);
                    }
                    idx += 1;
                }
            }
            return Ok(Value::Null);
        }
        for &child in unit.children(pattern) {
            let kind = unit.kind(child);
            if kind == "variable_pattern" || kind == "identifier" {
                let name = unit.text(child);
                Environment::define(&self.current_environment, name, value.clone());
            } else if kind == "list_pattern" {
                let _ = value.clone();
            } else if kind == "record_pattern" {
                self.bind_pattern(unit, child, value.clone())?;
            } else if kind == "map_pattern" {
                let _ = value.clone();
            }
        }
        Ok(Value::Null)
    }
}

// =============================================================================
// Helpers
// =============================================================================

fn is_binary_operator(text: &str) -> bool {
    matches!(
        text,
        "+" | "-"
            | "*"
            | "/"
            | "~/"
            | "%"
            | "<"
            | ">"
            | "<="
            | ">="
            | "=="
            | "!="
            | "&"
            | "|"
            | "^"
            | "<<"
            | ">>"
            | ">>>"
    )
}

fn binary_op_error(op: &str, left: &Value, right: &Value) -> InterpError {
    InterpError::runtime(format!(
        "Operator {op} is not defined for {} and {}",
        value_display(left),
        value_display(right)
    ))
}

/// Builds the value of a non-interpolated string literal from its raw
/// text by stripping the surrounding quotes and decoding escape
/// sequences. Used as a fallback when the grammar's token stream omits
/// characters (e.g. `"` inside single-quoted strings).
fn compose_plain_string(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() < 2 {
        return text.to_string();
    }
    // Strip the surrounding quotes. Regular literals have one quote per
    // side; multiline literals use three (`"""`). Only strip the quote
    // kind that actually delimits the literal.
    let quote = chars[0];
    let mut start = 0;
    while start < chars.len() && chars[start] == quote {
        start += 1;
    }
    let mut end = chars.len();
    while end > start && chars[end - 1] == quote {
        end -= 1;
    }
    let content: Vec<char> = chars[start..end].to_vec();
    let mut out = String::new();
    let mut i = 0;
    while i < content.len() {
        if content[i] == '\\' {
            let mut n = 2;
            if content.get(i + 1) == Some(&'u') {
                if content.get(i + 2) == Some(&'{') {
                    let mut j = i + 3;
                    while j < content.len() && content[j] != '}' {
                        j += 1;
                    }
                    n = j + 1 - i;
                } else {
                    n = 6;
                }
            } else if content.get(i + 1) == Some(&'x') {
                n = 4;
            }
            let esc: String = content[i..].iter().take(n).collect();
            out.push_str(&decode_escape(&esc));
            i += n;
        } else {
            out.push(content[i]);
            i += 1;
        }
    }
    out
}

fn decode_escape(text: &str) -> String {
    let body = text.strip_prefix('\\').unwrap_or(text);
    match body {
        "n" => "\n".to_string(),
        "t" => "\t".to_string(),
        "r" => "\r".to_string(),
        "f" => "\u{0c}".to_string(),
        "v" => "\u{0b}".to_string(),
        "b" => "\u{08}".to_string(),
        "\\" => "\\".to_string(),
        "$" => "$".to_string(),
        "'" => "'".to_string(),
        "\"" => "\"".to_string(),
        "/" => "/".to_string(),
        _ => {
            if let Some(hex) = body.strip_prefix('x') {
                if let Ok(code) = u32::from_str_radix(hex, 16)
                    && let Some(c) = char::from_u32(code)
                {
                    return c.to_string();
                }
            } else if let Some(hex) = body.strip_prefix('u') {
                if let Some(inner) = hex.strip_prefix('{') {
                    if let Some(inner) = inner.strip_suffix('}')
                        && let Ok(code) = u32::from_str_radix(inner, 16)
                        && let Some(c) = char::from_u32(code)
                    {
                        return c.to_string();
                    }
                } else if let Ok(code) = u32::from_str_radix(hex, 16)
                    && let Some(c) = char::from_u32(code)
                {
                    return c.to_string();
                }
            }
            text.to_string()
        }
    }
}

fn function_body_flags(text: &str) -> (bool, bool) {
    let trimmed = text.trim_start();
    if trimmed.starts_with("async*") {
        (true, true)
    } else if trimmed.starts_with("sync*") {
        (false, true)
    } else if trimmed.starts_with("async") {
        (true, false)
    } else {
        (false, false)
    }
}

fn extract_operator_name(text: &str) -> String {
    let mut t = text.trim().to_string();
    if let Some(idx) = t.find('(') {
        t.truncate(idx);
    }
    let parts: Vec<&str> = t.split_whitespace().collect();
    let op = parts.last().unwrap_or(&"");
    if *op == "operator" {
        "unnamed".to_string()
    } else {
        op.to_string()
    }
}

fn type_name_matches(pattern: &str, actual: &str) -> bool {
    if pattern == actual {
        return true;
    }
    if pattern == "Object" || pattern == "dynamic" {
        return true;
    }
    if pattern == "num" && (actual == "int" || actual == "double") {
        return true;
    }
    if actual == "Null" && pattern == "Null" {
        return true;
    }
    false
}

// =============================================================================
// Public API
// =============================================================================

pub fn evaluate(source: &str) -> Result<Value, InterpError> {
    crate::context::catch_dart_panic(|| {
        if crate::state::try_current().is_some() {
            evaluate_inner(source)
        } else {
            let state = crate::state::ContextState::new();
            crate::state::with_current(&state, || evaluate_inner(source))
        }
    })
}

fn evaluate_inner(source: &str) -> Result<Value, InterpError> {
    let unit = Rc::new(CompilationUnit::parse(source)?);
    let env = Environment::new();
    crate::stdlib::core::load_core(&env);
    {
        let print_fn = NativeFunction {
            name: Some("print".to_string()),
            min_arity: 1,
            max_arity: Some(1),
            is_async: false,
            is_generator: false,
            closure: Box::new(|visitor, positional, _named| {
                let s = positional
                    .first()
                    .map(|v| {
                        visitor
                            .display_value(v)
                            .unwrap_or_else(|_| value_display(v))
                    })
                    .unwrap_or_default();
                println!("{s}");
                Ok(Value::Null)
            }),
            callable_runtime_type: crate::rt::FunctionRuntimeType::untyped(),
        };
        Environment::define(&env, "print", Value::NativeFn(Rc::new(print_fn)));
    }
    let mut visitor = InterpreterVisitor::new(env.clone());
    visitor.execute_top_level(&unit)?;
    match Environment::get(&env, "main") {
        Ok(Value::Function(main_fn)) => {
            visitor.call_interpreted_function(&main_fn, vec![], HashMap::new())
        }
        Ok(Value::NativeFn(n)) => visitor.call_native_function(&n, vec![], HashMap::new()),
        Ok(other) => Err(InterpError::runtime(format!(
            "main is not a function: {}",
            value_display(&other)
        ))),
        Err(_) => Ok(Value::Null),
    }
}

pub fn evaluate_expression(source: &str) -> Result<Value, InterpError> {
    crate::context::catch_dart_panic(|| {
        let env = Environment::new();
        evaluate_expression_in(&env, source)
    })
}

pub fn evaluate_expression_in(
    env: &Rc<RefCell<Environment>>,
    source: &str,
) -> Result<Value, InterpError> {
    if crate::state::try_current().is_some() {
        evaluate_expression_inner(env, source)
    } else {
        // Standalone entry point: run under a throwaway state so stdlib
        // calls (imports, class registration) have a Context to route
        // through.
        let state = crate::state::ContextState::new();
        crate::state::with_current(&state, || evaluate_expression_inner(env, source))
    }
}

fn evaluate_expression_inner(
    env: &Rc<RefCell<Environment>>,
    source: &str,
) -> Result<Value, InterpError> {
    let wrapped = format!("void main() {{ return ({source}); }}");
    let unit = Rc::new(CompilationUnit::parse(&wrapped)?);
    let mut visitor = InterpreterVisitor::new(env.clone());
    let fd = unit
        .first_child_of_kind(0, "function_declaration")
        .ok_or_else(|| InterpError::runtime("internal: no function"))?;
    let body = unit
        .child_by_field(fd, "body")
        .ok_or_else(|| InterpError::runtime("internal: no body"))?;
    let block = unit
        .first_child_of_kind(body, "block")
        .ok_or_else(|| InterpError::runtime("internal: no block"))?;
    let ret = unit
        .first_child_of_kind(block, "return_statement")
        .ok_or_else(|| InterpError::runtime("internal: no return"))?;
    let expr = unit
        .named_children(ret)
        .first()
        .copied()
        .ok_or_else(|| InterpError::runtime("internal: no expression"))?;
    visitor.visit_expression(&unit, expr)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn eval_expr(src: &str) -> Value {
        evaluate_expression(src).unwrap()
    }

    #[test]
    fn test_arithmetic() {
        assert_eq!(eval_expr("1 + 2 * 3"), Value::Int(7));
        assert_eq!(eval_expr("(1 + 2) * 3"), Value::Int(9));
        assert_eq!(eval_expr("7 ~/ 2"), Value::Int(3));
        assert_eq!(eval_expr("7 % 3"), Value::Int(1));
        assert_eq!(eval_expr("2 < 3"), Value::Bool(true));
        assert_eq!(eval_expr("1 == 1"), Value::Bool(true));
        assert_eq!(eval_expr("1 != 2"), Value::Bool(true));
        assert_eq!(eval_expr("10 / 4"), Value::Double(2.5));
        assert_eq!(eval_expr("5 - 8"), Value::Int(-3));
    }

    #[test]
    fn test_strings() {
        assert_eq!(
            eval_expr("\"hello\" + \" \" + \"world\""),
            Value::str("hello world")
        );
        assert_eq!(eval_expr("'a' * 3"), Value::str("aaa"));
        assert_eq!(
            eval_expr(r#""\n\t\x41\u{1F600}""#),
            Value::str("\n\tA\u{1f600}")
        );
        let v = eval_expr("\"value: ${1 + 2}\"");
        assert_eq!(v, Value::str("value: 3"));
        assert_eq!(eval_expr(r#"r"raw\n""#), Value::str(r"raw\n"));
    }

    #[test]
    fn test_literals() {
        assert_eq!(eval_expr("[1, 2, 3][1]"), Value::Int(2));
        assert_eq!(eval_expr("{'a': 1, 'b': 2}['b']"), Value::Int(2));
        assert_eq!(eval_expr("{'a': 1}['z']"), Value::Null);
        assert_eq!(eval_expr("(1, 2).$1"), Value::Int(2));
        assert_eq!(eval_expr("(x: 1).x"), Value::Int(1));
        assert_eq!(eval_expr("null ?? 5"), Value::Int(5));
        assert_eq!(eval_expr("3 ?? 5"), Value::Int(3));
        assert_eq!(eval_expr("0x1F"), Value::Int(31));
        assert_eq!(eval_expr("1_000"), Value::Int(1000));
    }

    #[test]
    fn test_control_flow() {
        assert_eq!(eval_expr("true ? 1 : 2"), Value::Int(1));
        assert_eq!(eval_expr("false && (1 / 0)"), Value::Bool(false));
        assert_eq!(eval_expr("true || (1 / 0)"), Value::Bool(true));
        assert_eq!(eval_expr("2 == 2 ? 'yes' : 'no'"), Value::str("yes"));
    }

    #[test]
    fn test_functions() {
        let src = r#"
            int add(int a, int b) { return a + b; }
            void main() { return add(2, 3); }
        "#;
        let result = evaluate(src).unwrap();
        assert_eq!(result, Value::Int(5));

        let src = r#"
            int fib(int n) {
              if (n < 2) { return n; }
              return fib(n - 1) + fib(n - 2);
            }
            void main() { return fib(10); }
        "#;
        let result = evaluate(src).unwrap();
        assert_eq!(result, Value::Int(55));

        let src = r#"
            void main() {
              var f = (int x) => x * 2;
              return f(21);
            }
        "#;
        let result = evaluate(src).unwrap();
        assert_eq!(result, Value::Int(42));

        let src = r#"
            int g(int x, [int y = 10]) { return x + y; }
            void main() { return g(1) + g(1, 2); }
        "#;
        let result = evaluate(src).unwrap();
        assert_eq!(result, Value::Int(14));

        let src = r#"
            int h({required int a, int b = 5}) { return a + b; }
            void main() { return h(a: 1) + h(a: 1, b: 2); }
        "#;
        let result = evaluate(src).unwrap();
        assert_eq!(result, Value::Int(9));
    }

    #[test]
    fn test_generators() {
        let src = r#"
            Iterable<int> nums() sync* {
              yield 1;
              yield 2;
              yield 3;
            }
            void main() {
              var total = 0;
              for (var n in nums()) { total += n; }
              return total;
            }
        "#;
        let result = evaluate(src).unwrap();
        assert_eq!(result, Value::Int(6));
    }

    #[test]
    fn test_string_interp_ident() {
        let src = r#"
            void main() { var name = 'd4rt'; return 'hi $name!'; }
        "#;
        let result = evaluate(src).unwrap();
        assert_eq!(result, Value::str("hi d4rt!"));
    }

    #[test]
    fn test_errors() {
        assert!(evaluate_expression("1 + 'a'").is_err());
        assert!(evaluate_expression("undefined_var").is_err());
        assert!(matches!(
            evaluate_expression("throw 'boom'"),
            Err(InterpError::Throw(v)) if v == Value::str("boom")
        ));
    }
}
