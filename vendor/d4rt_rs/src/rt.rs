use std::fmt;
use std::rc::Rc;
use std::sync::Arc;

use crate::class::{ClassDef, EnumDef, ExtTypeDef};
use crate::value::Value;

#[derive(Debug, Clone)]
pub struct TypeParamDef {
    pub name: String,
    pub bound: Option<Rc<RuntimeType>>,
}

#[derive(Debug, Clone)]
pub struct FunctionRuntimeType {
    pub return_type: Rc<RuntimeType>,
    pub positional_parameter_types: Vec<Rc<RuntimeType>>,
    pub required_positional_parameter_count: usize,
    pub named_parameter_types: Vec<(String, Rc<RuntimeType>)>,
    pub required_named_parameters: Vec<String>,
    pub type_parameter_count: usize,
    pub is_untyped: bool,
}

impl FunctionRuntimeType {
    pub fn untyped() -> Self {
        FunctionRuntimeType {
            return_type: Rc::new(RuntimeType::Named("dynamic".into())),
            positional_parameter_types: vec![],
            required_positional_parameter_count: 0,
            named_parameter_types: vec![],
            required_named_parameters: vec![],
            type_parameter_count: 0,
            is_untyped: true,
        }
    }
}

#[derive(Debug, Clone)]
pub struct RecordRuntimeType {
    pub positional_field_types: Vec<Rc<RuntimeType>>,
    pub named_field_types: Vec<(String, Rc<RuntimeType>)>,
}

#[derive(Debug, Clone)]
pub enum RuntimeType {
    Named(String),
    Applied {
        base: Rc<RuntimeType>,
        args: Vec<Rc<RuntimeType>>,
    },
    Function(FunctionRuntimeType),
    Record(RecordRuntimeType),
    Class(Rc<ClassDef>),
    Enum(Rc<EnumDef>),
    TypeParam(TypeParamDef),
    Native(Arc<crate::class::BridgedClassDef>),
    ExtType(Rc<ExtTypeDef>),
}

impl RuntimeType {
    pub fn dynamic() -> Rc<Self> {
        Rc::new(RuntimeType::Named("dynamic".into()))
    }

    pub fn object() -> Rc<Self> {
        Rc::new(RuntimeType::Named("Object".into()))
    }

    pub fn void() -> Rc<Self> {
        Rc::new(RuntimeType::Named("void".into()))
    }

    pub fn named(name: &str) -> Rc<Self> {
        Rc::new(RuntimeType::Named(name.to_string()))
    }

    pub fn name(&self) -> String {
        match self {
            RuntimeType::Named(n) => n.clone(),
            RuntimeType::Applied { base, args } => {
                let arg_strs: Vec<String> = args.iter().map(|a| a.name()).collect();
                format!("{}<{}>", base.name(), arg_strs.join(", "))
            }
            RuntimeType::Function(f) => f.name(),
            RuntimeType::Record(r) => r.name(),
            RuntimeType::Class(c) => c.name.clone(),
            RuntimeType::Enum(e) => e.name.clone(),
            RuntimeType::TypeParam(t) => t.name.clone(),
            RuntimeType::Native(n) => n.name.clone(),
            RuntimeType::ExtType(e) => e.name.clone(),
        }
    }

    pub fn is_subtype_of(&self, other: &RuntimeType, value: Option<&Value>) -> bool {
        match self {
            RuntimeType::Named(name) => match other {
                RuntimeType::Named(oname) => {
                    if std::ptr::eq(self, other) || name == oname {
                        return true;
                    }
                    if oname == "dynamic" {
                        return true;
                    }
                    if oname == "Object" {
                        return name != "void";
                    }
                    if (name == "int" || name == "double") && oname == "num" {
                        return true;
                    }
                    false
                }
                RuntimeType::Native(n) => {
                    if n.name == "dynamic" || n.name == "Object" {
                        return true;
                    }
                    if (name == "int" || name == "double") && n.name == "num" {
                        return true;
                    }
                    name == &n.name
                }
                RuntimeType::Class(c) => {
                    if c.name == "Object" {
                        return name != "void";
                    }
                    if (name == "int" || name == "double") && c.name == "num" {
                        return true;
                    }
                    false
                }
                _ => false,
            },
            RuntimeType::Applied { base, args } => match other {
                RuntimeType::Applied {
                    base: obase,
                    args: oargs,
                } => {
                    let base_matches = Rc::ptr_eq(base, obase)
                        || base.name() == obase.name()
                        || base.is_subtype_of(obase, value);
                    if !base_matches || args.len() != oargs.len() {
                        return false;
                    }
                    for (i, t) in args.iter().enumerate() {
                        if oargs[i].name() == "dynamic" || oargs[i].name() == "Object" {
                            continue;
                        }
                        if !t.is_subtype_of(&oargs[i], value) {
                            return false;
                        }
                    }
                    true
                }
                RuntimeType::Named(n) if n == "dynamic" || n == "Object" => true,
                _ => base.is_subtype_of(other, value),
            },
            RuntimeType::Function(f) => {
                let oname = other.name();
                if oname == "dynamic" || oname == "Object" || oname == "Function" {
                    return true;
                }
                match other {
                    RuntimeType::Function(of) => {
                        if of.is_untyped {
                            return true;
                        }
                        if f.is_untyped {
                            return false;
                        }
                        if f.type_parameter_count != of.type_parameter_count {
                            return false;
                        }
                        if f.required_positional_parameter_count
                            != of.required_positional_parameter_count
                            || f.positional_parameter_types.len()
                                != of.positional_parameter_types.len()
                        {
                            return false;
                        }
                        if f.required_named_parameters.len() != of.required_named_parameters.len()
                            || !f
                                .required_named_parameters
                                .iter()
                                .all(|n| of.required_named_parameters.contains(n))
                            || f.named_parameter_types.len() != of.named_parameter_types.len()
                        {
                            return false;
                        }
                        for (name, _) in &f.named_parameter_types {
                            if !of.named_parameter_types.iter().any(|(n, _)| n == name) {
                                return false;
                            }
                        }
                        if !f.return_type.is_subtype_of(&of.return_type, None) {
                            return false;
                        }
                        for i in 0..f.positional_parameter_types.len() {
                            if !of.positional_parameter_types[i]
                                .is_subtype_of(&f.positional_parameter_types[i], None)
                            {
                                return false;
                            }
                        }
                        for (name, ty) in &f.named_parameter_types {
                            let oty = of
                                .named_parameter_types
                                .iter()
                                .find(|(n, _)| n == name)
                                .map(|(_, t)| t)
                                .unwrap();
                            if !oty.is_subtype_of(ty, None) {
                                return false;
                            }
                        }
                        true
                    }
                    _ => false,
                }
            }
            RuntimeType::Record(r) => {
                let oname = other.name();
                if oname == "dynamic" || oname == "Object" || oname == "Record" {
                    return true;
                }
                match other {
                    RuntimeType::Record(or) => {
                        if r.positional_field_types.len() != or.positional_field_types.len()
                            || r.named_field_types.len() != or.named_field_types.len()
                        {
                            return false;
                        }
                        for (name, _) in &r.named_field_types {
                            if !or.named_field_types.iter().any(|(n, _)| n == name) {
                                return false;
                            }
                        }
                        for i in 0..r.positional_field_types.len() {
                            if !r.positional_field_types[i]
                                .is_subtype_of(&or.positional_field_types[i], None)
                            {
                                return false;
                            }
                        }
                        for (name, ty) in &r.named_field_types {
                            let oty = or
                                .named_field_types
                                .iter()
                                .find(|(n, _)| n == name)
                                .map(|(_, t)| t)
                                .unwrap();
                            if !ty.is_subtype_of(oty, None) {
                                return false;
                            }
                        }
                        true
                    }
                    _ => false,
                }
            }
            RuntimeType::Class(c) => match other {
                RuntimeType::Class(oc) => {
                    if Rc::ptr_eq(c, oc) {
                        return true;
                    }
                    c.is_subtype_of_class(oc)
                }
                RuntimeType::Named(n) => {
                    if n == "dynamic" || n == "Object" {
                        return true;
                    }
                    if n == "Function" && c.name == "Function" {
                        return true;
                    }
                    false
                }
                RuntimeType::Native(n) => {
                    n.name == "dynamic" || n.name == "Object" || n.name == c.name
                }
                _ => false,
            },
            RuntimeType::Enum(e) => match other {
                RuntimeType::Enum(oe) => Rc::ptr_eq(e, oe),
                RuntimeType::Named(n) => n == "dynamic" || n == "Object" || *n == e.name,
                _ => false,
            },
            RuntimeType::TypeParam(t) => match &t.bound {
                Some(b) => b.is_subtype_of(other, value),
                None => other.name() == "dynamic" || other.name() == "Object",
            },
            RuntimeType::Native(na) => match other {
                RuntimeType::Native(on) => {
                    if Arc::ptr_eq(na, on) || na.name == on.name {
                        return true;
                    }
                    if na.is_subtype_of_fn.is_some()
                        && let Some(f) = &na.is_subtype_of_fn
                        && f(value.unwrap_or(&Value::Null))
                    {
                        return true;
                    }
                    if (na.name == "int" || na.name == "double") && on.name == "num" {
                        return true;
                    }
                    if on.name == "dynamic" || on.name == "Object" {
                        return true;
                    }
                    false
                }
                RuntimeType::Named(n) => {
                    if n == "dynamic" || n == "Object" || *n == na.name {
                        return true;
                    }
                    if (na.name == "int" || na.name == "double") && n == "num" {
                        return true;
                    }
                    false
                }
                _ => false,
            },
            RuntimeType::ExtType(e) => match other {
                RuntimeType::ExtType(oe) => e.name == oe.name,
                RuntimeType::Class(oc) => {
                    if let RuntimeType::Class(rep) = &*e.representation_type {
                        rep.name == oc.name
                    } else {
                        false
                    }
                }
                RuntimeType::Named(n) => n == "dynamic" || n == "Object" || n == &e.name,
                _ => false,
            },
        }
    }
}

impl FunctionRuntimeType {
    pub fn name(&self) -> String {
        if self.is_untyped
            && self.positional_parameter_types.is_empty()
            && self.named_parameter_types.is_empty()
            && self.type_parameter_count == 0
        {
            return "Function".into();
        }
        let mut parts: Vec<String> = vec![];
        for (i, p) in self.positional_parameter_types.iter().enumerate() {
            if i == self.required_positional_parameter_count
                && self.required_positional_parameter_count < self.positional_parameter_types.len()
            {
                parts.push("[".into());
            }
            parts.push(p.name());
            if i < self.positional_parameter_types.len() - 1 {
                parts.push(", ".into());
            }
        }
        if self.required_positional_parameter_count < self.positional_parameter_types.len() {
            parts.push("]".into());
        }
        if !self.named_parameter_types.is_empty() {
            if !parts.is_empty() {
                parts.push(", ".into());
            }
            parts.push("{".into());
            let mut named = self.named_parameter_types.clone();
            named.sort_by(|a, b| a.0.cmp(&b.0));
            for (i, (name, ty)) in named.iter().enumerate() {
                if self.required_named_parameters.contains(name) {
                    parts.push("required ".into());
                }
                parts.push(format!("{} {}", ty.name(), name));
                if i < named.len() - 1 {
                    parts.push(", ".into());
                }
            }
            parts.push("}".into());
        }
        let type_param_suffix = if self.type_parameter_count > 0 {
            format!("<{}>", vec!["_"; self.type_parameter_count].join(", "))
        } else {
            String::new()
        };
        format!(
            "{} Function{}({})",
            self.return_type.name(),
            type_param_suffix,
            parts.concat()
        )
    }
}

impl RecordRuntimeType {
    pub fn name(&self) -> String {
        if self.positional_field_types.is_empty() && self.named_field_types.is_empty() {
            return "Record".into();
        }
        let mut parts: Vec<String> = vec![];
        for (i, t) in self.positional_field_types.iter().enumerate() {
            parts.push(t.name());
            if i < self.positional_field_types.len() - 1 || !self.named_field_types.is_empty() {
                parts.push(", ".into());
            }
        }
        if !self.named_field_types.is_empty() {
            parts.push("{".into());
            let mut named = self.named_field_types.clone();
            named.sort_by(|a, b| a.0.cmp(&b.0));
            for (i, (name, ty)) in named.iter().enumerate() {
                parts.push(format!("{} {}", ty.name(), name));
                if i < named.len() - 1 {
                    parts.push(", ".into());
                }
            }
            parts.push("}".into());
        }
        format!("({})", parts.concat())
    }
}

impl fmt::Display for RuntimeType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name())
    }
}

pub fn runtime_type_of(value: Option<&Value>) -> Option<Rc<RuntimeType>> {
    match value {
        None | Some(Value::Null) => Some(RuntimeType::named("Null")),
        Some(Value::Bool(_)) => Some(RuntimeType::named("bool")),
        Some(Value::Int(_)) => Some(RuntimeType::named("int")),
        Some(Value::Double(_)) => Some(RuntimeType::named("double")),
        Some(Value::Str(_)) => Some(RuntimeType::named("String")),
        Some(Value::StringBuffer(_)) => Some(RuntimeType::named("StringBuffer")),
        Some(Value::BigInt(_)) => Some(RuntimeType::named("BigInt")),
        Some(Value::List(_)) => Some(RuntimeType::named("List")),
        Some(Value::Map(_)) => Some(RuntimeType::named("Map")),
        Some(Value::Set(_)) => Some(RuntimeType::named("Set")),
        Some(Value::Function(f)) => Some(Rc::new(RuntimeType::Function(
            f.callable_runtime_type.clone(),
        ))),
        Some(Value::NativeFn(f)) => Some(Rc::new(RuntimeType::Function(
            f.callable_runtime_type.clone(),
        ))),
        Some(Value::Class(c)) => Some(Rc::new(RuntimeType::Class(c.clone()))),
        Some(Value::EnumType(e)) => Some(Rc::new(RuntimeType::Enum(e.clone()))),
        Some(Value::EnumValue(v)) => {
            Some(Rc::new(RuntimeType::Enum(v.borrow().parent_enum.clone())))
        }
        Some(Value::Instance(inst)) => {
            let inst = inst.borrow();
            if inst.type_arguments.is_empty() {
                Some(Rc::new(RuntimeType::Class(inst.klass.clone())))
            } else {
                Some(Rc::new(RuntimeType::Applied {
                    base: Rc::new(RuntimeType::Class(inst.klass.clone())),
                    args: inst.type_arguments.clone(),
                }))
            }
        }
        Some(Value::ExtTypeInst(e)) => Some(Rc::new(RuntimeType::ExtType(
            e.borrow().extension_type.clone(),
        ))),
        Some(Value::Record(r)) => {
            let r = r.borrow();
            let positional: Vec<Rc<RuntimeType>> = r
                .positional_fields
                .iter()
                .map(|f| runtime_type_of(Some(f)).unwrap_or_else(RuntimeType::dynamic))
                .collect();
            let mut named: Vec<(String, Rc<RuntimeType>)> = r
                .named_fields
                .iter()
                .map(|(k, v)| {
                    (
                        k.clone(),
                        runtime_type_of(Some(v)).unwrap_or_else(RuntimeType::dynamic),
                    )
                })
                .collect();
            named.sort_by(|a, b| a.0.cmp(&b.0));
            Some(Rc::new(RuntimeType::Record(RecordRuntimeType {
                positional_field_types: positional,
                named_field_types: named,
            })))
        }
        Some(Value::Type(t)) => Some(t.clone()),
        Some(Value::Bridged(b)) => Some(Rc::new(RuntimeType::Native(
            b.borrow().bridged_class.clone(),
        ))),
        Some(Value::Extension(_)) | Some(Value::ExtType(_)) | Some(Value::PropertyAccessor(_)) => {
            None
        }
        Some(Value::Late(_)) => None,
        Some(Value::BoundSuper(_)) => None,
        Some(Value::BoundBridgedSuper(_)) => None,
        Some(Value::BridgedClass(b)) => Some(Rc::new(RuntimeType::Named(b.name.clone()))),
        Some(Value::BridgedEnum(b)) => Some(Rc::new(RuntimeType::Named(b.name.clone()))),
        Some(Value::BridgedEnumValue(b)) => Some(Rc::new(RuntimeType::Named(
            b.borrow().parent_enum.name.clone(),
        ))),
        Some(Value::BridgedSuperMethod(_))
        | Some(Value::BridgedMixinMethod(_))
        | Some(Value::BridgedEnumMixinMethod(_)) => None,
        Some(Value::Module(_)) => None,
        Some(Value::SyncGen(_)) => None,
        Some(Value::Future(_)) => Some(RuntimeType::named("Future")),
    }
}
