use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use crate::ast::{CompilationUnit, NodeId};
use crate::callable::{Callable, InterpretedFunction, arity};
use crate::env::Environment;
use crate::error::InterpError;
use crate::rt::RuntimeType;
use crate::value::{EnumValueData, InstanceData, Value};

/// Predicate used by bridged classes to answer `is` checks against a host
/// value (e.g. "is it a `String` as the host sees it?").
pub type SubtypePredicate = Box<dyn Fn(&Value) -> bool + Send + Sync>;

#[derive(Clone)]
pub struct FactoryRedirect {
    pub target_class: Rc<ClassDef>,
    pub target_ctor: String,
}

pub struct ClassDef {
    pub name: String,
    pub unit: Rc<CompilationUnit>,
    pub superclass: Option<Rc<ClassDef>>,
    pub class_definition_environment: Rc<RefCell<Environment>>,
    pub field_declarations: RefCell<Vec<NodeId>>,
    pub methods: RefCell<HashMap<String, Rc<InterpretedFunction>>>,
    pub getters: RefCell<HashMap<String, Rc<InterpretedFunction>>>,
    pub setters: RefCell<HashMap<String, Rc<InterpretedFunction>>>,
    pub static_methods: RefCell<HashMap<String, Rc<InterpretedFunction>>>,
    pub static_getters: RefCell<HashMap<String, Rc<InterpretedFunction>>>,
    pub static_setters: RefCell<HashMap<String, Rc<InterpretedFunction>>>,
    pub static_fields: RefCell<HashMap<String, Value>>,
    pub constructors: RefCell<HashMap<String, Rc<InterpretedFunction>>>,
    pub operators: RefCell<HashMap<String, Rc<InterpretedFunction>>>,
    pub is_abstract: bool,
    pub interfaces: RefCell<Vec<Rc<ClassDef>>>,
    pub bridged_interfaces: RefCell<Vec<Arc<BridgedClassDef>>>,
    pub is_mixin: bool,
    pub on_clause_types: RefCell<Vec<Rc<ClassDef>>>,
    pub mixins: RefCell<Vec<Rc<ClassDef>>>,
    pub bridged_mixins: RefCell<Vec<Arc<BridgedClassDef>>>,
    pub type_parameter_names: Vec<String>,
    pub type_parameter_bounds: HashMap<String, Option<Rc<RuntimeType>>>,
    pub is_final: bool,
    pub is_interface: bool,
    pub is_base: bool,
    pub is_sealed: bool,
    pub bridged_superclass: Option<Arc<BridgedClassDef>>,
    pub factory_redirects: RefCell<HashMap<String, FactoryRedirect>>,
}

impl std::fmt::Debug for ClassDef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ClassDef({})", self.name)
    }
}

impl ClassDef {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        name: String,
        unit: Rc<CompilationUnit>,
        class_definition_environment: Rc<RefCell<Environment>>,
        is_abstract: bool,
        is_mixin: bool,
        is_final: bool,
        is_interface: bool,
        is_base: bool,
        is_sealed: bool,
        type_parameter_names: Vec<String>,
        type_parameter_bounds: HashMap<String, Option<Rc<RuntimeType>>>,
    ) -> Rc<ClassDef> {
        Rc::new(ClassDef {
            name,
            unit,
            superclass: None,
            class_definition_environment,
            field_declarations: RefCell::new(vec![]),
            methods: RefCell::new(HashMap::new()),
            getters: RefCell::new(HashMap::new()),
            setters: RefCell::new(HashMap::new()),
            static_methods: RefCell::new(HashMap::new()),
            static_getters: RefCell::new(HashMap::new()),
            static_setters: RefCell::new(HashMap::new()),
            static_fields: RefCell::new(HashMap::new()),
            constructors: RefCell::new(HashMap::new()),
            operators: RefCell::new(HashMap::new()),
            is_abstract,
            interfaces: RefCell::new(vec![]),
            bridged_interfaces: RefCell::new(vec![]),
            is_mixin,
            on_clause_types: RefCell::new(vec![]),
            mixins: RefCell::new(vec![]),
            bridged_mixins: RefCell::new(vec![]),
            type_parameter_names,
            type_parameter_bounds,
            is_final,
            is_interface,
            is_base,
            is_sealed,
            bridged_superclass: None,
            factory_redirects: RefCell::new(HashMap::new()),
        })
    }

    pub fn find_instance_method(&self, name: &str) -> Option<Rc<InterpretedFunction>> {
        if let Some(m) = self.methods.borrow().get(name) {
            return Some(m.clone());
        }
        for mixin in self.mixins.borrow().iter().rev() {
            if let Some(m) = mixin.find_instance_method(name) {
                return Some(m);
            }
        }
        if let Some(superclass) = &self.superclass
            && let Some(m) = superclass.find_instance_method(name)
        {
            return Some(m);
        }
        if let Some(bc) = &self.bridged_superclass
            && bc.find_instance_method_adapter(name).is_some()
        {
            return None;
        }
        None
    }

    pub fn find_instance_getter(&self, name: &str) -> Option<Rc<InterpretedFunction>> {
        if let Some(g) = self.getters.borrow().get(name) {
            return Some(g.clone());
        }
        for mixin in self.mixins.borrow().iter().rev() {
            if let Some(g) = mixin.find_instance_getter(name) {
                return Some(g);
            }
        }
        if let Some(superclass) = &self.superclass
            && let Some(g) = superclass.find_instance_getter(name)
        {
            return Some(g);
        }
        if let Some(bc) = &self.bridged_superclass
            && bc.find_instance_getter_adapter(name).is_some()
        {
            return None;
        }
        None
    }

    pub fn find_instance_setter(&self, name: &str) -> Option<Rc<InterpretedFunction>> {
        if let Some(s) = self.setters.borrow().get(name) {
            return Some(s.clone());
        }
        for mixin in self.mixins.borrow().iter().rev() {
            if let Some(s) = mixin.find_instance_setter(name) {
                return Some(s);
            }
        }
        if let Some(superclass) = &self.superclass
            && let Some(s) = superclass.find_instance_setter(name)
        {
            return Some(s);
        }
        if let Some(bc) = &self.bridged_superclass
            && bc.find_instance_setter_adapter(name).is_some()
        {
            return None;
        }
        None
    }

    pub fn find_static_method(&self, name: &str) -> Option<Rc<InterpretedFunction>> {
        self.static_methods.borrow().get(name).cloned()
    }

    pub fn find_static_getter(&self, name: &str) -> Option<Rc<InterpretedFunction>> {
        self.static_getters.borrow().get(name).cloned()
    }

    pub fn find_static_setter(&self, name: &str) -> Option<Rc<InterpretedFunction>> {
        self.static_setters.borrow().get(name).cloned()
    }

    pub fn find_operator(&self, name: &str) -> Option<Rc<InterpretedFunction>> {
        if let Some(op) = self.operators.borrow().get(name) {
            return Some(op.clone());
        }
        for mixin in self.mixins.borrow().iter().rev() {
            if let Some(op) = mixin.find_operator(name) {
                return Some(op);
            }
        }
        if let Some(superclass) = &self.superclass
            && let Some(op) = superclass.find_operator(name)
        {
            return Some(op);
        }
        None
    }

    pub fn find_constructor(&self, name: &str) -> Option<Rc<InterpretedFunction>> {
        self.constructors.borrow().get(name).cloned()
    }

    pub fn get_static_field(&self, name: &str) -> Result<Value, InterpError> {
        match self.static_fields.borrow().get(name) {
            Some(Value::Late(l)) => Ok(l.borrow_mut().get()),
            Some(v) => Ok(v.clone()),
            None => Err(InterpError::runtime(format!(
                "Static field not found: {}.{}",
                self.name, name
            ))),
        }
    }

    pub fn set_static_field(&self, name: &str, value: Value) {
        let existing = self.static_fields.borrow().get(name).cloned();
        match existing {
            Some(Value::Late(l)) => l.borrow_mut().assign(value),
            _ => {
                self.static_fields
                    .borrow_mut()
                    .insert(name.to_string(), value);
            }
        }
    }

    pub fn check_type_satisfies_bound(type_arg: &RuntimeType, bound: &RuntimeType) -> bool {
        if type_arg.name() == bound.name() {
            return true;
        }
        if bound.name() == "num" {
            let n = type_arg.name();
            if n == "num" || n == "int" || n == "double" {
                return true;
            }
            if let RuntimeType::Native(bc) = type_arg {
                let n = bc.name.clone();
                if n == "num" || n == "int" || n == "double" {
                    return true;
                }
            }
        }
        if bound.name() == "Object" {
            return type_arg.name() != "void";
        }
        if bound.name() == "String" {
            let n = type_arg.name();
            if n == "String" {
                return true;
            }
            if let RuntimeType::Native(bc) = type_arg
                && bc.name == "String"
            {
                return true;
            }
        }
        let bname = bound.name();
        if bname == "Comparable" || bname.starts_with("Comparable<") {
            let n = type_arg.name();
            if ["String", "int", "double", "num", "DateTime"].contains(&n.as_str()) {
                return true;
            }
            if let RuntimeType::Native(bc) = type_arg
                && ["String", "int", "double", "num", "DateTime"].contains(&bc.name.as_str())
            {
                return true;
            }
        }
        type_arg.is_subtype_of(bound, None)
    }

    pub fn get_validated_type_arguments(
        &self,
        type_arguments: Option<&[Rc<RuntimeType>]>,
    ) -> Result<Vec<Rc<RuntimeType>>, InterpError> {
        let expected_count = self.type_parameter_names.len();
        let mut result: Vec<Rc<RuntimeType>> = Vec::new();
        let empty: Vec<Rc<RuntimeType>> = vec![];
        let args = type_arguments.unwrap_or(&empty);
        for (i, arg) in args.iter().enumerate() {
            if i >= expected_count {
                break;
            }
            let bound = self
                .type_parameter_bounds
                .get(&self.type_parameter_names[i])
                .cloned()
                .flatten();
            if let Some(bound) = bound
                && !ClassDef::check_type_satisfies_bound(arg, &bound)
            {
                return Err(InterpError::runtime(format!(
                    "Type argument '{}' does not satisfy bound '{}' for class '{}'",
                    arg.name(),
                    bound.name(),
                    self.name
                )));
            }
            result.push(arg.clone());
        }
        while result.len() < expected_count {
            result.push(RuntimeType::dynamic());
        }
        Ok(result)
    }

    pub fn is_subtype_of_class(&self, other: &ClassDef) -> bool {
        if std::ptr::eq(self, other) {
            return true;
        }
        if let Some(superclass) = &self.superclass
            && superclass.is_subtype_of_class(other)
        {
            return true;
        }
        for mixin in self.mixins.borrow().iter() {
            if mixin.is_subtype_of_class(other) {
                return true;
            }
        }
        for interface in self.interfaces.borrow().iter() {
            if interface.is_subtype_of_class(other) {
                return true;
            }
        }
        false
    }

    pub fn get_abstract_inherited_members(&self) -> HashMap<String, String> {
        let mut result = HashMap::new();
        let mut current = self.superclass.clone();
        while let Some(c) = current {
            for (name, m) in c.methods.borrow().iter() {
                if m.is_abstract {
                    result.insert(name.clone(), "method".to_string());
                }
            }
            for (name, g) in c.getters.borrow().iter() {
                if g.is_abstract {
                    result.insert(name.clone(), "getter".to_string());
                }
            }
            for (name, s) in c.setters.borrow().iter() {
                if s.is_abstract {
                    result.insert(name.clone(), "setter".to_string());
                }
            }
            current = c.superclass.clone();
        }
        result
    }

    pub fn get_concrete_members(&self) -> HashMap<String, String> {
        let mut result = HashMap::new();
        for (name, m) in self.methods.borrow().iter() {
            if !m.is_abstract {
                result.insert(name.clone(), "method".to_string());
            }
        }
        for (name, g) in self.getters.borrow().iter() {
            if !g.is_abstract {
                result.insert(name.clone(), "getter".to_string());
            }
        }
        for (name, s) in self.setters.borrow().iter() {
            if !s.is_abstract {
                result.insert(name.clone(), "setter".to_string());
            }
        }
        result
    }

    pub fn get_all_interface_members(&self) -> HashMap<String, String> {
        let mut result = HashMap::new();
        let mut queue: Vec<Rc<ClassDef>> = self.interfaces.borrow().clone();
        let mut visited: Vec<usize> = vec![];
        while let Some(interface) = queue.pop() {
            let ptr = Rc::as_ptr(&interface) as usize;
            if visited.contains(&ptr) {
                continue;
            }
            visited.push(ptr);
            for name in interface.methods.borrow().keys() {
                result.insert(name.clone(), "method".to_string());
            }
            for name in interface.getters.borrow().keys() {
                result.insert(name.clone(), "getter".to_string());
            }
            for name in interface.setters.borrow().keys() {
                result.insert(name.clone(), "setter".to_string());
            }
            let mut chain: Vec<Rc<ClassDef>> = vec![interface];
            while let Some(c) = chain.pop() {
                if let Some(sc) = &c.superclass {
                    for name in sc.methods.borrow().keys() {
                        result.insert(name.clone(), "method".to_string());
                    }
                    for name in sc.getters.borrow().keys() {
                        result.insert(name.clone(), "getter".to_string());
                    }
                    for name in sc.setters.borrow().keys() {
                        result.insert(name.clone(), "setter".to_string());
                    }
                    chain.push(sc.clone());
                }
            }
        }
        if let Some(superclass) = &self.superclass {
            for (name, kind) in superclass.get_all_interface_members() {
                result.insert(name, kind);
            }
        }
        result
    }

    pub fn get_all_concrete_members(this: &Rc<ClassDef>) -> HashMap<String, String> {
        let mut result = HashMap::new();
        let mut current = Some(this.clone());
        while let Some(c) = current {
            for (name, kind) in c.get_concrete_members() {
                result.entry(name).or_insert(kind);
            }
            for name in c.static_fields.borrow().keys() {
                result.insert(name.clone(), "field".to_string());
            }
            current = c.superclass.clone();
        }
        result
    }
}

pub struct EnumDef {
    pub name: String,
    pub unit: Rc<CompilationUnit>,
    pub declaration_environment: Rc<RefCell<Environment>>,
    pub value_names: Vec<String>,
    pub values: RefCell<HashMap<String, Rc<RefCell<EnumValueData>>>>,
    pub methods: RefCell<HashMap<String, Rc<InterpretedFunction>>>,
    pub getters: RefCell<HashMap<String, Rc<InterpretedFunction>>>,
    pub setters: RefCell<HashMap<String, Rc<InterpretedFunction>>>,
    pub static_methods: RefCell<HashMap<String, Rc<InterpretedFunction>>>,
    pub static_getters: RefCell<HashMap<String, Rc<InterpretedFunction>>>,
    pub static_setters: RefCell<HashMap<String, Rc<InterpretedFunction>>>,
    pub static_fields: RefCell<HashMap<String, Value>>,
    pub constructors: RefCell<HashMap<String, Rc<InterpretedFunction>>>,
    pub field_declarations: RefCell<Vec<NodeId>>,
    pub mixins: RefCell<Vec<Rc<ClassDef>>>,
    pub bridged_mixins: RefCell<Vec<Arc<BridgedClassDef>>>,
}

impl std::fmt::Debug for EnumDef {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "EnumDef({})", self.name)
    }
}

impl EnumDef {
    pub fn placeholder(
        name: String,
        unit: Rc<CompilationUnit>,
        declaration_environment: Rc<RefCell<Environment>>,
        value_names: Vec<String>,
    ) -> Rc<EnumDef> {
        Rc::new(EnumDef {
            name,
            unit,
            declaration_environment,
            value_names,
            values: RefCell::new(HashMap::new()),
            methods: RefCell::new(HashMap::new()),
            getters: RefCell::new(HashMap::new()),
            setters: RefCell::new(HashMap::new()),
            static_methods: RefCell::new(HashMap::new()),
            static_getters: RefCell::new(HashMap::new()),
            static_setters: RefCell::new(HashMap::new()),
            static_fields: RefCell::new(HashMap::new()),
            constructors: RefCell::new(HashMap::new()),
            field_declarations: RefCell::new(vec![]),
            mixins: RefCell::new(vec![]),
            bridged_mixins: RefCell::new(vec![]),
        })
    }
}

pub struct ExtensionDef {
    pub name: Option<String>,
    pub on_type: Rc<RuntimeType>,
    pub members: HashMap<String, crate::callable::Callable>,
    pub static_methods: HashMap<String, Rc<InterpretedFunction>>,
    pub static_getters: HashMap<String, Rc<InterpretedFunction>>,
    pub static_setters: HashMap<String, Rc<InterpretedFunction>>,
    pub static_fields: RefCell<HashMap<String, Value>>,
}

impl ExtensionDef {
    pub fn find_member(&self, name: &str) -> Option<crate::callable::Callable> {
        self.members.get(name).cloned()
    }

    pub fn find_static_method(&self, name: &str) -> Option<Rc<InterpretedFunction>> {
        self.static_methods.get(name).cloned()
    }

    pub fn find_static_getter(&self, name: &str) -> Option<Rc<InterpretedFunction>> {
        self.static_getters.get(name).cloned()
    }

    pub fn find_static_setter(&self, name: &str) -> Option<Rc<InterpretedFunction>> {
        self.static_setters.get(name).cloned()
    }

    pub fn get_static_field(&self, name: &str) -> Result<Value, InterpError> {
        self.static_fields
            .borrow()
            .get(name)
            .cloned()
            .ok_or_else(|| InterpError::runtime(format!("Static field not found: {name}")))
    }

    pub fn set_static_field(&self, name: &str, value: Value) {
        self.static_fields
            .borrow_mut()
            .insert(name.to_string(), value);
    }
}

pub struct ExtTypeDef {
    pub name: String,
    pub representation_field_name: String,
    pub representation_type: Rc<RuntimeType>,
    pub declaration_environment: Rc<RefCell<Environment>>,
    pub methods: HashMap<String, Rc<InterpretedFunction>>,
    pub getters: HashMap<String, Rc<InterpretedFunction>>,
    pub setters: HashMap<String, Rc<InterpretedFunction>>,
    pub operators: HashMap<String, Rc<InterpretedFunction>>,
    pub type_parameter_names: Vec<String>,
    pub type_parameter_bounds: HashMap<String, Option<Rc<RuntimeType>>>,
}

impl std::fmt::Debug for ExtTypeDef {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "ExtTypeDef({})", self.name)
    }
}

impl ExtTypeDef {
    pub fn find_instance_method(&self, name: &str) -> Option<Rc<InterpretedFunction>> {
        self.methods.get(name).cloned()
    }
    pub fn find_instance_getter(&self, name: &str) -> Option<Rc<InterpretedFunction>> {
        self.getters.get(name).cloned()
    }
    pub fn find_instance_setter(&self, name: &str) -> Option<Rc<InterpretedFunction>> {
        self.setters.get(name).cloned()
    }
    pub fn find_operator(&self, name: &str) -> Option<Rc<InterpretedFunction>> {
        self.operators.get(name).cloned()
    }
}

pub type MethodAdapter = Box<
    dyn Fn(
            &mut crate::interp::InterpreterVisitor,
            Value,
            Vec<Value>,
            HashMap<String, Value>,
        ) -> Result<Value, InterpError>
        + Send
        + Sync,
>;
pub type GetterAdapter = Box<
    dyn Fn(&mut crate::interp::InterpreterVisitor, Value) -> Result<Value, InterpError>
        + Send
        + Sync,
>;
pub type SetterAdapter = Box<
    dyn Fn(&mut crate::interp::InterpreterVisitor, Value, Value) -> Result<(), InterpError>
        + Send
        + Sync,
>;
pub type StaticMethodAdapter = Box<
    dyn Fn(
            &mut crate::interp::InterpreterVisitor,
            Vec<Value>,
            HashMap<String, Value>,
        ) -> Result<Value, InterpError>
        + Send
        + Sync,
>;
pub type StaticGetterAdapter =
    Box<dyn Fn(&mut crate::interp::InterpreterVisitor) -> Result<Value, InterpError> + Send + Sync>;
pub type StaticSetterAdapter = Box<
    dyn Fn(&mut crate::interp::InterpreterVisitor, Value) -> Result<(), InterpError> + Send + Sync,
>;
pub type ConstructorAdapter = Box<
    dyn Fn(
            &mut crate::interp::InterpreterVisitor,
            Vec<Value>,
            HashMap<String, Value>,
        ) -> Result<Value, InterpError>
        + Send
        + Sync,
>;

pub struct BridgedClassDef {
    pub name: String,
    pub native_type: Option<String>,
    pub native_names: Vec<String>,
    pub type_parameter_count: usize,
    pub can_be_used_as_mixin: bool,
    pub is_subtype_of_fn: Option<SubtypePredicate>,
    pub constructors: HashMap<String, ConstructorAdapter>,
    pub static_methods: HashMap<String, StaticMethodAdapter>,
    pub static_getters: HashMap<String, StaticGetterAdapter>,
    pub static_setters: HashMap<String, StaticSetterAdapter>,
    pub methods: HashMap<String, MethodAdapter>,
    pub getters: HashMap<String, GetterAdapter>,
    pub setters: HashMap<String, SetterAdapter>,
}

impl std::fmt::Debug for BridgedClassDef {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "BridgedClassDef({})", self.name)
    }
}

impl BridgedClassDef {
    pub fn new(name: &str) -> BridgedClassDef {
        BridgedClassDef {
            name: name.to_string(),
            native_type: None,
            native_names: vec![],
            type_parameter_count: 0,
            can_be_used_as_mixin: false,
            is_subtype_of_fn: None,
            constructors: HashMap::new(),
            static_methods: HashMap::new(),
            static_getters: HashMap::new(),
            static_setters: HashMap::new(),
            methods: HashMap::new(),
            getters: HashMap::new(),
            setters: HashMap::new(),
        }
    }

    pub fn find_constructor_adapter(&self, name: &str) -> Option<&ConstructorAdapter> {
        self.constructors.get(name)
    }

    pub fn find_instance_method_adapter(&self, name: &str) -> Option<&MethodAdapter> {
        self.methods.get(name)
    }

    pub fn find_instance_getter_adapter(&self, name: &str) -> Option<&GetterAdapter> {
        self.getters.get(name)
    }

    pub fn find_instance_setter_adapter(&self, name: &str) -> Option<&SetterAdapter> {
        self.setters.get(name)
    }

    pub fn find_static_method_adapter(&self, name: &str) -> Option<&StaticMethodAdapter> {
        self.static_methods.get(name)
    }

    pub fn find_static_getter_adapter(&self, name: &str) -> Option<&StaticGetterAdapter> {
        self.static_getters.get(name)
    }

    pub fn find_static_setter_adapter(&self, name: &str) -> Option<&StaticSetterAdapter> {
        self.static_setters.get(name)
    }
}

#[derive(Clone)]
pub struct BridgedEnumValueData {
    pub parent_enum: Rc<BridgedEnumDef>,
    pub name: String,
    pub native_value: Value,
    pub fields: HashMap<String, Value>,
}

pub struct BridgedEnumDef {
    pub name: String,
    pub values: RefCell<Vec<Rc<BridgedEnumValueData>>>,
    pub methods: HashMap<String, MethodAdapter>,
    pub getters: HashMap<String, GetterAdapter>,
    pub static_methods: HashMap<String, StaticMethodAdapter>,
    pub static_getters: HashMap<String, StaticGetterAdapter>,
    pub static_setters: HashMap<String, StaticSetterAdapter>,
}

impl BridgedEnumDef {
    /// Creates a bridged enum whose values reference the enum itself as their parent.
    pub fn new(name: &str, values: Vec<(&str, Value)>) -> Rc<BridgedEnumDef> {
        let def = Rc::new(BridgedEnumDef {
            name: name.to_string(),
            values: RefCell::new(Vec::with_capacity(values.len())),
            methods: HashMap::new(),
            getters: HashMap::new(),
            static_methods: HashMap::new(),
            static_getters: HashMap::new(),
            static_setters: HashMap::new(),
        });
        let values: Vec<Rc<BridgedEnumValueData>> = values
            .into_iter()
            .map(|(value_name, native_value)| {
                Rc::new(BridgedEnumValueData {
                    parent_enum: def.clone(),
                    name: value_name.to_string(),
                    native_value,
                    fields: HashMap::new(),
                })
            })
            .collect();
        def.values.borrow_mut().extend(values);
        def
    }
}

pub struct BridgedInstanceData {
    pub bridged_class: Arc<BridgedClassDef>,
    pub native: Value,
    pub type_arguments: Vec<Rc<RuntimeType>>,
    pub bridged_super_object: Option<Value>,
}

pub struct BoundSuperData {
    pub instance: Rc<RefCell<InstanceData>>,
    pub start_lookup_class: Rc<ClassDef>,
}

pub struct BoundBridgedSuperData {
    pub instance: Rc<RefCell<InstanceData>>,
    pub start_lookup_class: Arc<BridgedClassDef>,
}

pub struct LateVariable {
    pub is_initialized: bool,
    pub value: Value,
    pub initializer: Option<Box<dyn FnMut() -> Result<Value, InterpError>>>,
    pub name: String,
    pub is_final: bool,
}

impl LateVariable {
    pub fn get(&mut self) -> Value {
        if !self.is_initialized {
            if let Some(initializer) = &mut self.initializer {
                self.value = initializer().unwrap_or(Value::Null);
                self.is_initialized = true;
            } else {
                panic!("LateInitializationError")
            }
        }
        self.value.clone()
    }

    pub fn assign(&mut self, value: Value) {
        if self.is_final && self.is_initialized {
            panic!("LateInitializationError")
        }
        self.value = value;
        self.is_initialized = true;
    }

    pub fn is_initialized(&self) -> bool {
        self.is_initialized
    }
}

pub fn callable_arity(c: &Callable) -> usize {
    arity(c)
}
