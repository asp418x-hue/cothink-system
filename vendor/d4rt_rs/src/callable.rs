use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use crate::ast::{CompilationUnit, NodeId};
use crate::class::{BridgedClassDef, ClassDef, ExtTypeDef};
use crate::env::Environment;
use crate::error::InterpError;
use crate::interp::InterpreterVisitor;

/// Host-implemented function invoked from Dart code; the payload of a
/// [`NativeFunction`].
pub type NativeFn = Box<
    dyn Fn(
        &mut InterpreterVisitor,
        Vec<Value>,
        HashMap<String, Value>,
    ) -> Result<Value, InterpError>,
>;
use crate::rt::{FunctionRuntimeType, RuntimeType};
use crate::value::{EnumValueData, InstanceData, Value};

#[derive(Clone)]
pub struct Parameter {
    pub name: String,
    pub is_optional_positional: bool,
    pub is_named: bool,
    pub is_required: bool,
    pub type_: Option<Rc<RuntimeType>>,
    pub has_default: bool,
    pub default: Option<NodeId>,
}

impl Parameter {
    /// The name callers use for this parameter. Field formals (`this.x`,
    /// `super.x`) are stored with the `this.`/`super.` prefix so field
    /// binding can find them, but Dart callers pass the bare field name.
    pub fn named_key(&self) -> &str {
        self.name
            .strip_prefix("this.")
            .or_else(|| self.name.strip_prefix("super."))
            .unwrap_or(&self.name)
    }
}

#[derive(Clone)]
pub struct InterpretedFunction {
    pub unit: Rc<CompilationUnit>,
    pub name: Option<String>,
    pub parameters: Vec<Parameter>,
    pub body: NodeId,
    pub return_type: Option<Rc<RuntimeType>>,
    pub environment: Rc<RefCell<Environment>>,
    pub is_async: bool,
    pub is_generator: bool,
    pub is_abstract: bool,
    pub is_external: bool,
    pub is_getter: bool,
    pub is_setter: bool,
    pub is_factory: bool,
    pub is_constructor: bool,
    pub initializers: Option<NodeId>,
    pub bound_instance: Option<Value>,
    pub bound_enum_value: Option<Value>,
    pub callable_runtime_type: FunctionRuntimeType,
}

impl InterpretedFunction {
    pub fn bind_instance(&self, instance: Rc<RefCell<InstanceData>>) -> Rc<InterpretedFunction> {
        let mut f = self.clone();
        f.bound_instance = Some(Value::Instance(instance));
        f.bound_enum_value = None;
        Rc::new(f)
    }

    pub fn bind_enum_value(
        &self,
        enum_value: Rc<RefCell<EnumValueData>>,
    ) -> Rc<InterpretedFunction> {
        let mut f = self.clone();
        f.bound_enum_value = Some(Value::EnumValue(enum_value));
        f.bound_instance = None;
        Rc::new(f)
    }

    /// Binds a static method/getter to its enclosing class so that
    /// unqualified static member references resolve inside its body.
    pub fn bind_class(&self, class_def: Rc<ClassDef>) -> Rc<InterpretedFunction> {
        let mut f = self.clone();
        f.bound_instance = Some(Value::Class(class_def));
        f.bound_enum_value = None;
        Rc::new(f)
    }

    pub fn required_positional_count(&self) -> usize {
        self.parameters
            .iter()
            .take_while(|p| !p.is_optional_positional && !p.is_named)
            .count()
    }

    pub fn positional_param_count(&self) -> usize {
        self.parameters.iter().take_while(|p| !p.is_named).count()
    }
}

pub struct NativeFunction {
    pub name: Option<String>,
    pub min_arity: usize,
    pub max_arity: Option<usize>,
    pub is_async: bool,
    pub is_generator: bool,
    pub closure: NativeFn,
    pub callable_runtime_type: FunctionRuntimeType,
}

impl NativeFunction {
    pub fn callable_runtime_type(&self) -> FunctionRuntimeType {
        self.callable_runtime_type.clone()
    }
}

pub struct PropertyAccessor {
    pub name: String,
    pub getter: Option<Callable>,
    pub setter: Option<Callable>,
}

#[derive(Clone)]
pub enum Callable {
    Interpreted(Rc<InterpretedFunction>),
    Native(Rc<NativeFunction>),
    Class(Rc<ClassDef>),
    ExtType(Rc<ExtTypeDef>),
    PropertyAccessor(Rc<PropertyAccessor>),
}

pub struct BridgedSuperMethodCallable {
    pub native_target: Value,
    pub method_name: String,
    pub bridged_class: Arc<BridgedClassDef>,
}

pub struct BridgedMixinMethodCallable {
    pub instance: Value,
    pub method_name: String,
    pub bridged_class: Arc<BridgedClassDef>,
}

pub enum BridgedEnumMixinCallTarget {
    Class(Arc<BridgedClassDef>),
    EnumDef(Rc<crate::class::BridgedEnumDef>),
}

pub struct BridgedEnumMixinMethodCallable {
    pub enum_value: Value,
    pub method_name: String,
    pub call_target: BridgedEnumMixinCallTarget,
}

pub struct SyncGenerator {
    pub function: Rc<InterpretedFunction>,
    pub environment: Rc<RefCell<Environment>>,
    pub buffer: Vec<Value>,
    pub next_index: usize,
    pub started: bool,
    pub completed: bool,
}

impl SyncGenerator {
    pub fn new(
        function: Rc<InterpretedFunction>,
        environment: Rc<RefCell<Environment>>,
    ) -> Rc<RefCell<SyncGenerator>> {
        Rc::new(RefCell::new(SyncGenerator {
            function,
            environment,
            buffer: Vec::new(),
            next_index: 0,
            started: false,
            completed: false,
        }))
    }
}

pub fn arity(c: &Callable) -> usize {
    match c {
        Callable::Interpreted(f) => f.required_positional_count(),
        Callable::Native(n) => n.min_arity,
        Callable::Class(cls) => match cls.constructors.borrow().get("") {
            Some(c) => c.required_positional_count(),
            None => 0,
        },
        Callable::ExtType(_) => 1,
        Callable::PropertyAccessor(_) => 0,
    }
}

pub fn call_callable(
    callable: &Callable,
    visitor: &mut InterpreterVisitor,
    positional: Vec<Value>,
    named: HashMap<String, Value>,
) -> Result<Value, InterpError> {
    match callable {
        Callable::Interpreted(f) => visitor.call_interpreted_function(f, positional, named),
        Callable::Native(n) => visitor.call_native_function(n, positional, named),
        Callable::Class(c) => visitor.call_constructor(c, positional, named),
        Callable::ExtType(e) => visitor.call_ext_type_constructor(e, positional, named),
        Callable::PropertyAccessor(p) => match &p.getter {
            Some(getter) => call_callable(getter, visitor, positional, named),
            None => Err(InterpError::runtime(format!(
                "Cannot call property accessor '{}' without a getter.",
                p.name
            ))),
        },
    }
}

pub fn make_callable_runtime_type(
    return_type: Option<Rc<RuntimeType>>,
    parameters: &[Parameter],
) -> FunctionRuntimeType {
    let mut positional: Vec<Rc<RuntimeType>> = Vec::new();
    let mut required_count = 0usize;
    let mut named_types: Vec<(String, Rc<RuntimeType>)> = Vec::new();
    let mut required_named: Vec<String> = Vec::new();
    for p in parameters {
        let t = p.type_.clone().unwrap_or_else(RuntimeType::dynamic);
        if p.is_named {
            if p.is_required {
                required_named.push(p.name.clone());
            }
            named_types.push((p.name.clone(), t));
        } else {
            positional.push(t);
            if !p.is_optional_positional {
                required_count += 1;
            }
        }
    }
    named_types.sort_by(|a, b| a.0.cmp(&b.0));
    FunctionRuntimeType {
        return_type: return_type.unwrap_or_else(RuntimeType::dynamic),
        positional_parameter_types: positional,
        required_positional_parameter_count: required_count,
        named_parameter_types: named_types,
        required_named_parameters: required_named,
        type_parameter_count: 0,
        is_untyped: false,
    }
}
