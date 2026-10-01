use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::rc::Rc;
use std::sync::Arc;

use crate::callable::{
    Callable, InterpretedFunction, NativeFunction, PropertyAccessor, call_callable,
};
use crate::class::{
    BoundBridgedSuperData, BoundSuperData, BridgedClassDef, BridgedEnumDef, BridgedInstanceData,
    ClassDef, EnumDef, ExtTypeDef, LateVariable,
};
use crate::env::Environment;
use crate::error::InterpError;
use crate::interp::InterpreterVisitor;
use crate::rt::RuntimeType;
use num_bigint::BigInt;
use num_traits::ToPrimitive;

/// A runtime value flowing between Rust and interpreted Dart code.
///
/// Most variants mirror Dart's built-in types directly (`Int`, `Double`,
/// `Str`, `List`, `Map`, `Set`, `Record`, `BigInt`); the rest are the
/// interpreter's internal representations (classes, functions, generators,
/// futures) or bridged native objects (`Bridged`, `BridgedClass`, ...).
/// Prefer the ergonomic constructors/accessors ([`Value::str`],
/// [`Value::list`], [`Value::as_int`], ...) over pattern matching, and
/// [`crate::FromValue`]/[`crate::ToValue`] in native-function boundaries.
#[derive(Clone)]
pub enum Value {
    /// Dart `null`.
    Null,
    /// Dart `bool`.
    Bool(bool),
    /// Dart `int` (64-bit; see [`Value::BigInt`] for arbitrary precision).
    Int(i64),
    /// Dart `double` (IEEE-754 binary64).
    Double(f64),
    /// Dart `String` (shared, immutable).
    Str(Rc<str>),
    /// Dart `List<T>` (mutable, indexed).
    List(Rc<RefCell<Vec<Value>>>),
    /// Dart `Map<K, V>` (insertion-ordered).
    Map(Rc<RefCell<DartMap>>),
    /// Dart `Set<T>`.
    Set(Rc<RefCell<DartSet>>),
    /// An instance of a Dart class.
    Instance(Rc<RefCell<InstanceData>>),
    /// A Dart class object (the `Type`/constructor source).
    Class(Rc<ClassDef>),
    /// A Dart `enum` type object.
    EnumType(Rc<EnumDef>),
    /// A Dart `enum` value.
    EnumValue(Rc<RefCell<EnumValueData>>),
    /// A Dart function (interpreted).
    Function(Rc<InterpretedFunction>),
    /// A native function registered from Rust.
    NativeFn(Rc<NativeFunction>),
    /// A Dart record `(a, b, {c})`.
    Record(Rc<RefCell<RecordData>>),
    /// An instance of an `ExtType` (e.g. a `dart:` native surface).
    ExtTypeInst(Rc<RefCell<ExtTypeInstData>>),
    /// A Dart method bound to `super`.
    BoundSuper(Rc<BoundSuperData>),
    /// A bridged method bound to a `BridgedSuper` superclass.
    BoundBridgedSuper(Rc<BoundBridgedSuperData>),
    /// A bridged (Rust-native) object visible from Dart.
    Bridged(Rc<RefCell<BridgedInstanceData>>),
    /// A bridged class definition.
    BridgedClass(Arc<BridgedClassDef>),
    /// A bridged enum type.
    BridgedEnum(Rc<BridgedEnumDef>),
    /// A bridged enum value.
    BridgedEnumValue(Rc<RefCell<crate::class::BridgedEnumValueData>>),
    /// A bridged method reached through `super`.
    BridgedSuperMethod(Rc<crate::callable::BridgedSuperMethodCallable>),
    /// A bridged method reached through a mixin.
    BridgedMixinMethod(Rc<crate::callable::BridgedMixinMethodCallable>),
    /// A bridged enum method reached through a mixin.
    BridgedEnumMixinMethod(Rc<crate::callable::BridgedEnumMixinMethodCallable>),
    /// An extension type (`ExtType`) definition.
    ExtType(Rc<ExtTypeDef>),
    /// An `extension` member environment.
    Extension(Rc<crate::class::ExtensionDef>),
    /// A getter/setter pair (property accessor).
    PropertyAccessor(Rc<PropertyAccessor>),
    /// A `late` variable cell (uninitialized until first use).
    Late(Rc<RefCell<LateVariable>>),
    /// A runtime `Type` object.
    Type(Rc<RuntimeType>),
    /// A loaded module's (exported) environment.
    Module(Rc<RefCell<Environment>>),
    /// A synchronous generator (`sync*`).
    SyncGen(Rc<RefCell<crate::callable::SyncGenerator>>),
    /// An internal `StringBuffer` (string interpolation target).
    StringBuffer(Rc<RefCell<String>>),
    /// Arbitrary-precision `BigInt`.
    BigInt(num_bigint::BigInt),
    /// An in-flight `dart:async` future.
    Future(Rc<RefCell<crate::async_state::FutureState>>),
}

impl Value {
    pub fn str(s: impl Into<String>) -> Value {
        Value::Str(Rc::from(s.into().as_str()))
    }

    pub fn list() -> Value {
        Value::List(Rc::new(RefCell::new(Vec::new())))
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::Str(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_int(&self) -> Option<i64> {
        match self {
            Value::Int(i) => Some(*i),
            _ => None,
        }
    }

    pub fn is_null(&self) -> bool {
        matches!(self, Value::Null)
    }

    pub fn is_num(&self) -> bool {
        matches!(self, Value::Int(_) | Value::Double(_))
    }

    pub fn num_to_f64(&self) -> Option<f64> {
        match self {
            Value::Int(i) => Some(*i as f64),
            Value::Double(d) => Some(*d),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Bool(b) => Some(*b),
            _ => None,
        }
    }

    pub fn ptr(&self) -> Option<usize> {
        match self {
            Value::List(v) => Some(Rc::as_ptr(v) as usize),
            Value::Map(v) => Some(Rc::as_ptr(v) as usize),
            Value::Set(v) => Some(Rc::as_ptr(v) as usize),
            Value::Instance(v) => Some(Rc::as_ptr(v) as usize),
            Value::Class(v) => Some(Rc::as_ptr(v) as usize),
            Value::EnumType(v) => Some(Rc::as_ptr(v) as usize),
            Value::EnumValue(v) => Some(Rc::as_ptr(v) as usize),
            Value::Function(v) => Some(Rc::as_ptr(v) as usize),
            Value::NativeFn(v) => Some(Rc::as_ptr(v) as usize),
            Value::Record(v) => Some(Rc::as_ptr(v) as usize),
            Value::ExtTypeInst(v) => Some(Rc::as_ptr(v) as usize),
            Value::BoundSuper(v) => Some(Rc::as_ptr(v) as usize),
            Value::BoundBridgedSuper(v) => Some(Rc::as_ptr(v) as usize),
            Value::Bridged(v) => Some(Rc::as_ptr(v) as usize),
            Value::BridgedClass(v) => Some(std::sync::Arc::as_ptr(v) as usize),
            Value::BridgedEnum(v) => Some(Rc::as_ptr(v) as usize),
            Value::BridgedEnumValue(v) => Some(Rc::as_ptr(v) as usize),
            Value::BridgedSuperMethod(v) => Some(Rc::as_ptr(v) as usize),
            Value::BridgedMixinMethod(v) => Some(Rc::as_ptr(v) as usize),
            Value::BridgedEnumMixinMethod(v) => Some(Rc::as_ptr(v) as usize),
            Value::ExtType(v) => Some(Rc::as_ptr(v) as usize),
            Value::Extension(v) => Some(Rc::as_ptr(v) as usize),
            Value::PropertyAccessor(v) => Some(Rc::as_ptr(v) as usize),
            Value::Late(v) => Some(Rc::as_ptr(v) as usize),
            Value::Type(v) => Some(Rc::as_ptr(v) as usize),
            Value::Module(v) => Some(Rc::as_ptr(v) as usize),
            Value::SyncGen(v) => Some(Rc::as_ptr(v) as usize),
            _ => None,
        }
    }

    pub fn to_callable(&self) -> Option<Callable> {
        match self {
            Value::Function(f) => Some(Callable::Interpreted(f.clone())),
            Value::NativeFn(n) => Some(Callable::Native(n.clone())),
            Value::Class(c) => Some(Callable::Class(c.clone())),
            Value::ExtType(e) => Some(Callable::ExtType(e.clone())),
            Value::PropertyAccessor(p) => Some(Callable::PropertyAccessor(p.clone())),
            _ => None,
        }
    }
}

pub struct InstanceData {
    pub klass: Rc<ClassDef>,
    pub fields: Vec<Option<Value>>,
    pub bridged_super_object: Option<Value>,
    pub type_arguments: Vec<Rc<RuntimeType>>,
}

/// Global registry mapping instance field names to dense Vec slots.
///
/// Keeps `InstanceData.fields` a `Vec<Option<Value>>` indexed by slot instead
/// of a `HashMap<String, Value>`: no per-field `String` allocation, no hash
/// bucket per entry, and an empty map's fixed 48 B HashMap header becomes a
/// 24 B empty Vec. Slots are process-global but only ever used as indices, so
/// sharing a single registry across contexts is safe; entries are bounded by
/// the total number of distinct field names ever used.
fn field_slot(name: &str) -> usize {
    use std::sync::OnceLock;
    static SLOTS: OnceLock<std::sync::Mutex<HashMap<String, usize>>> = OnceLock::new();
    let mut m = SLOTS
        .get_or_init(|| std::sync::Mutex::new(HashMap::new()))
        .lock()
        .unwrap();
    match m.get(name) {
        Some(&s) => s,
        None => {
            let s = m.len();
            m.insert(name.to_string(), s);
            s
        }
    }
}

impl InstanceData {
    pub fn new(
        klass: Rc<ClassDef>,
        type_arguments: Vec<Rc<RuntimeType>>,
    ) -> Rc<RefCell<InstanceData>> {
        Rc::new(RefCell::new(InstanceData {
            klass,
            fields: Vec::new(),
            bridged_super_object: None,
            type_arguments,
        }))
    }

    pub fn get_field(&self, name: &str) -> Option<Value> {
        let slot = field_slot(name);
        self.fields.get(slot).and_then(|v| v.clone()).map(|v| match v {
            Value::Late(l) => {
                let mut l = l.borrow_mut();

                l.get()
            }
            v => v,
        })
    }

    pub fn set_field(&mut self, name: &str, value: Value) {
        let slot = field_slot(name);
        if slot >= self.fields.len() {
            self.fields.resize(slot + 1, None);
        }
        self.fields[slot] = Some(value);
    }
}

pub struct RecordData {
    pub positional_fields: Vec<Value>,
    pub named_fields: Vec<(String, Value)>,
}

pub struct EnumValueData {
    pub parent_enum: Rc<EnumDef>,
    pub name: String,
    pub index: usize,
    pub fields: HashMap<String, Value>,
}

pub struct ExtTypeInstData {
    pub extension_type: Rc<ExtTypeDef>,
    pub representation_field_name: String,
    pub representation_value: Value,
}

pub struct DartMap {
    pub entries: Vec<(Value, Value)>,
}

impl Default for DartMap {
    fn default() -> Self {
        Self::new()
    }
}

impl DartMap {
    pub fn new() -> Self {
        DartMap { entries: vec![] }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn get(&self, key: &Value) -> Option<Value> {
        self.entries
            .iter()
            .find(|(k, _)| value_eq(k, key))
            .map(|(_, v)| v.clone())
    }

    pub fn contains(&self, key: &Value) -> bool {
        self.entries.iter().any(|(k, _)| value_eq(k, key))
    }

    pub fn set(&mut self, key: Value, value: Value) {
        if let Some((_, v)) = self.entries.iter_mut().find(|(k, _)| value_eq(k, &key)) {
            *v = value;
        } else {
            self.entries.push((key, value));
        }
    }

    pub fn remove(&mut self, key: &Value) -> Option<Value> {
        let idx = self.entries.iter().position(|(k, _)| value_eq(k, key))?;
        Some(self.entries.remove(idx).1)
    }

    pub fn keys(&self) -> Vec<Value> {
        self.entries.iter().map(|(k, _)| k.clone()).collect()
    }
}

pub struct DartSet {
    pub items: Vec<Value>,
}

impl Default for DartSet {
    fn default() -> Self {
        Self::new()
    }
}

impl DartSet {
    pub fn new() -> Self {
        DartSet { items: vec![] }
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn contains(&self, v: &Value) -> bool {
        self.items.iter().any(|i| value_eq(i, v))
    }

    pub fn insert(&mut self, v: Value) -> bool {
        if self.contains(&v) {
            false
        } else {
            self.items.push(v);
            true
        }
    }

    pub fn remove(&mut self, v: &Value) -> bool {
        if let Some(idx) = self.items.iter().position(|i| value_eq(i, v)) {
            self.items.remove(idx);
            true
        } else {
            false
        }
    }
}

impl std::fmt::Debug for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&value_display(self))
    }
}

pub fn value_eq(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Null, Value::Null) => true,
        (Value::Bool(x), Value::Bool(y)) => x == y,
        (Value::Int(x), Value::Int(y)) => x == y,
        (Value::Double(x), Value::Double(y)) => x == y,
        (Value::Int(x), Value::Double(y)) => (*x as f64) == *y,
        (Value::Double(x), Value::Int(y)) => *x == (*y as f64),
        (Value::Str(x), Value::Str(y)) => x == y,
        (Value::List(x), Value::List(y)) => Rc::ptr_eq(x, y),
        (Value::Map(x), Value::Map(y)) => Rc::ptr_eq(x, y),
        (Value::Set(x), Value::Set(y)) => Rc::ptr_eq(x, y),
        (Value::Instance(x), Value::Instance(y)) => Rc::ptr_eq(x, y),
        (Value::Class(x), Value::Class(y)) => Rc::ptr_eq(x, y),
        (Value::EnumType(x), Value::EnumType(y)) => Rc::ptr_eq(x, y),
        (Value::Function(x), Value::Function(y)) => Rc::ptr_eq(x, y),
        (Value::NativeFn(x), Value::NativeFn(y)) => Rc::ptr_eq(x, y),
        (Value::ExtType(x), Value::ExtType(y)) => Rc::ptr_eq(x, y),
        (Value::Extension(x), Value::Extension(y)) => Rc::ptr_eq(x, y),
        (Value::PropertyAccessor(x), Value::PropertyAccessor(y)) => Rc::ptr_eq(x, y),
        (Value::Late(x), Value::Late(y)) => Rc::ptr_eq(x, y),
        (Value::Type(x), Value::Type(y)) => Rc::ptr_eq(x, y),
        (Value::Module(x), Value::Module(y)) => Rc::ptr_eq(x, y),
        (Value::SyncGen(x), Value::SyncGen(y)) => Rc::ptr_eq(x, y),
        (Value::StringBuffer(x), Value::StringBuffer(y)) => Rc::ptr_eq(x, y),
        (Value::BigInt(x), Value::BigInt(y)) => x == y,
        (Value::Future(x), Value::Future(y)) => Rc::ptr_eq(x, y),
        (Value::BigInt(x), Value::Int(y)) => &BigInt::from(*y) == x,
        (Value::Int(x), Value::BigInt(y)) => &BigInt::from(*x) == y,
        (Value::BridgedClass(x), Value::BridgedClass(y)) => std::sync::Arc::ptr_eq(x, y),
        (Value::BridgedEnum(x), Value::BridgedEnum(y)) => Rc::ptr_eq(x, y),
        (Value::Bridged(x), Value::Bridged(y)) => {
            Rc::ptr_eq(x, y)
                || (std::sync::Arc::ptr_eq(&x.borrow().bridged_class, &y.borrow().bridged_class)
                    && value_eq(&x.borrow().native, &y.borrow().native))
        }
        (Value::BridgedEnumValue(x), Value::BridgedEnumValue(y)) => {
            Rc::ptr_eq(&x.borrow().parent_enum, &y.borrow().parent_enum)
                && value_eq(&x.borrow().native_value, &y.borrow().native_value)
        }
        (Value::BridgedSuperMethod(x), Value::BridgedSuperMethod(y)) => Rc::ptr_eq(x, y),
        (Value::BridgedMixinMethod(x), Value::BridgedMixinMethod(y)) => Rc::ptr_eq(x, y),
        (Value::BridgedEnumMixinMethod(x), Value::BridgedEnumMixinMethod(y)) => Rc::ptr_eq(x, y),
        (Value::EnumValue(x), Value::EnumValue(y)) => {
            Rc::ptr_eq(&x.borrow().parent_enum, &y.borrow().parent_enum)
                && x.borrow().index == y.borrow().index
        }
        (Value::Record(x), Value::Record(y)) => {
            let x = x.borrow();
            let y = y.borrow();
            x.positional_fields.len() == y.positional_fields.len()
                && x.named_fields.len() == y.named_fields.len()
                && x.positional_fields
                    .iter()
                    .zip(y.positional_fields.iter())
                    .all(|(a, b)| value_eq(a, b))
                && x.named_fields.iter().all(|(k, v)| {
                    y.named_fields
                        .iter()
                        .find(|(k2, _)| k2 == k)
                        .is_some_and(|(_, v2)| value_eq(v, v2))
                })
        }
        (Value::Bridged(x), other) => match &x.borrow().native {
            Value::List(l) => match other {
                Value::List(ol) => {
                    let l = l.borrow();
                    let ol = ol.borrow();
                    l.len() == ol.len() && l.iter().zip(ol.iter()).all(|(a, b)| value_eq(a, b))
                }
                _ => false,
            },
            Value::Map(m) => match other {
                Value::Map(om) => {
                    let m = m.borrow();
                    let om = om.borrow();
                    m.entries.len() == om.entries.len()
                        && m.entries.iter().all(|(k, v)| {
                            om.entries
                                .iter()
                                .find(|(k2, _)| value_eq(k, k2))
                                .is_some_and(|(_, v2)| value_eq(v, v2))
                        })
                }
                _ => false,
            },
            Value::Set(s) => match other {
                Value::Set(os) => {
                    let s = s.borrow();
                    let os = os.borrow();
                    s.len() == os.len()
                        && s.items
                            .iter()
                            .all(|v| os.items.iter().any(|o| value_eq(v, o)))
                }
                _ => false,
            },
            Value::Str(s) => match other {
                Value::Str(os) => s == os,
                _ => false,
            },
            Value::BigInt(b) => match other {
                Value::BigInt(ob) => b == ob,
                Value::Int(i) => b == &BigInt::from(*i),
                _ => false,
            },
            _ => false,
        },
        (other, Value::Bridged(y)) => value_eq(&Value::Bridged(y.clone()), other),
        _ => false,
    }
}

fn hash_value(v: &Value, hasher: &mut impl Hasher) {
    match v {
        Value::Null => 0u8.hash(hasher),
        Value::Bool(b) => {
            1u8.hash(hasher);
            b.hash(hasher);
        }
        Value::Int(i) => {
            2u8.hash(hasher);
            i.hash(hasher);
        }
        Value::Double(d) => {
            2u8.hash(hasher);
            if d.fract() == 0.0 && d.is_finite() && d.abs() < 9.2e18 {
                (*d as i64).hash(hasher);
            } else {
                d.to_bits().hash(hasher);
            }
        }
        Value::Str(s) => {
            3u8.hash(hasher);
            s.hash(hasher);
        }
        Value::List(l) => {
            4u8.hash(hasher);
            (Rc::as_ptr(l) as usize).hash(hasher);
        }
        Value::Map(m) => {
            5u8.hash(hasher);
            (Rc::as_ptr(m) as usize).hash(hasher);
        }
        Value::Set(s) => {
            6u8.hash(hasher);
            (Rc::as_ptr(s) as usize).hash(hasher);
        }
        Value::Instance(i) => {
            7u8.hash(hasher);
            (Rc::as_ptr(i) as usize).hash(hasher);
        }
        Value::Class(c) => {
            8u8.hash(hasher);
            (Rc::as_ptr(c) as usize).hash(hasher);
        }
        Value::EnumType(e) => {
            9u8.hash(hasher);
            (Rc::as_ptr(e) as usize).hash(hasher);
        }
        Value::EnumValue(e) => {
            10u8.hash(hasher);
            (Rc::as_ptr(&e.borrow().parent_enum) as usize).hash(hasher);
            e.borrow().index.hash(hasher);
        }
        Value::Function(f) => {
            11u8.hash(hasher);
            (Rc::as_ptr(f) as usize).hash(hasher);
        }
        Value::NativeFn(f) => {
            12u8.hash(hasher);
            (Rc::as_ptr(f) as usize).hash(hasher);
        }
        Value::Record(r) => {
            13u8.hash(hasher);
            let r = r.borrow();
            for f in &r.positional_fields {
                hash_value(f, hasher);
            }
            let mut named: Vec<&(String, Value)> = r.named_fields.iter().collect();
            named.sort_by(|a, b| a.0.cmp(&b.0));
            for (k, v) in named {
                k.hash(hasher);
                hash_value(v, hasher);
            }
        }
        Value::ExtTypeInst(e) => {
            14u8.hash(hasher);
            (Rc::as_ptr(e) as usize).hash(hasher);
        }
        Value::BoundSuper(b) => {
            15u8.hash(hasher);
            (Rc::as_ptr(b) as usize).hash(hasher);
        }
        Value::BoundBridgedSuper(b) => {
            16u8.hash(hasher);
            (Rc::as_ptr(b) as usize).hash(hasher);
        }
        Value::Bridged(b) => {
            17u8.hash(hasher);
            (std::sync::Arc::as_ptr(&b.borrow().bridged_class) as usize).hash(hasher);
            hash_value(&b.borrow().native, hasher);
        }
        Value::BridgedClass(b) => {
            18u8.hash(hasher);
            (std::sync::Arc::as_ptr(b) as usize).hash(hasher);
        }
        Value::BridgedEnum(b) => {
            19u8.hash(hasher);
            (Rc::as_ptr(b) as usize).hash(hasher);
        }
        Value::BridgedEnumValue(b) => {
            20u8.hash(hasher);
            (Rc::as_ptr(&b.borrow().parent_enum) as usize).hash(hasher);
            hash_value(&b.borrow().native_value, hasher);
        }
        Value::BridgedSuperMethod(b) => {
            27u8.hash(hasher);
            (Rc::as_ptr(b) as usize).hash(hasher);
        }
        Value::BridgedMixinMethod(b) => {
            28u8.hash(hasher);
            (Rc::as_ptr(b) as usize).hash(hasher);
        }
        Value::BridgedEnumMixinMethod(b) => {
            29u8.hash(hasher);
            (Rc::as_ptr(b) as usize).hash(hasher);
        }
        Value::ExtType(e) => {
            21u8.hash(hasher);
            (Rc::as_ptr(e) as usize).hash(hasher);
        }
        Value::Extension(e) => {
            22u8.hash(hasher);
            (Rc::as_ptr(e) as usize).hash(hasher);
        }
        Value::PropertyAccessor(p) => {
            23u8.hash(hasher);
            (Rc::as_ptr(p) as usize).hash(hasher);
        }
        Value::Late(l) => {
            24u8.hash(hasher);
            (Rc::as_ptr(l) as usize).hash(hasher);
        }
        Value::Type(t) => {
            25u8.hash(hasher);
            (Rc::as_ptr(t) as usize).hash(hasher);
        }
        Value::Module(m) => {
            26u8.hash(hasher);
            (Rc::as_ptr(m) as usize).hash(hasher);
        }
        Value::SyncGen(g) => {
            30u8.hash(hasher);
            (Rc::as_ptr(g) as usize).hash(hasher);
        }
        Value::StringBuffer(s) => {
            31u8.hash(hasher);
            s.borrow().hash(hasher);
        }
        Value::BigInt(b) => {
            32u8.hash(hasher);
            match b.to_i64() {
                Some(i) => i.hash(hasher),
                None => b.to_string().hash(hasher),
            }
        }
        Value::Future(f) => {
            33u8.hash(hasher);
            (Rc::as_ptr(f) as usize).hash(hasher);
        }
    }
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        value_eq(self, other)
    }
}

/// Maps a native (non-bridged) value to the Dart type name its bridged class
/// is registered under. Used for deterministic bridged-class lookup.
pub fn native_type_name(value: &Value) -> Option<&'static str> {
    match value {
        Value::Null => Some("Null"),
        Value::Bool(_) => Some("bool"),
        Value::Int(_) => Some("int"),
        Value::Double(_) => Some("double"),
        Value::Str(_) => Some("String"),
        Value::List(_) => Some("List"),
        Value::Map(_) => Some("Map"),
        Value::Set(_) => Some("Set"),
        Value::Function(_) | Value::NativeFn(_) => Some("Function"),
        Value::StringBuffer(_) => Some("StringBuffer"),
        Value::BigInt(_) => Some("BigInt"),
        Value::Type(_) => Some("Type"),
        Value::Future(_) => Some("Future"),
        _ => None,
    }
}

impl Eq for Value {}

impl Hash for Value {
    fn hash<H: Hasher>(&self, state: &mut H) {
        hash_value(self, state);
    }
}

pub fn fmt_double(v: f64) -> String {
    if v.is_nan() {
        return "NaN".to_string();
    }
    if v.is_infinite() {
        return if v > 0.0 {
            "Infinity".to_string()
        } else {
            "-Infinity".to_string()
        };
    }
    if v == v.trunc() && v.abs() < 1e21 {
        let i = v as i64;
        if (i as f64) == v {
            return format!("{i}.0");
        }
    }
    let s = format!("{v}");
    if let Some((mant, exp)) = s.split_once('e')
        && let Ok(e) = exp.parse::<i64>()
    {
        return format!("{mant}e{e:+}");
    }
    s
}

pub fn value_display(v: &Value) -> String {
    match v {
        Value::Null => "null".to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Int(i) => i.to_string(),
        Value::Double(d) => fmt_double(*d),
        Value::Str(s) => s.to_string(),
        Value::List(l) => {
            let l = l.borrow();
            format!(
                "[{}]",
                l.iter().map(value_display).collect::<Vec<_>>().join(", ")
            )
        }
        Value::Map(m) => {
            let m = m.borrow();
            format!(
                "{{{}}}",
                m.entries
                    .iter()
                    .map(|(k, v)| format!("{}: {}", value_display(k), value_display(v)))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        }
        Value::Set(s) => {
            let s = s.borrow();
            format!(
                "{{{}}}",
                s.items
                    .iter()
                    .map(value_display)
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        }
        Value::Instance(i) => {
            let i = i.borrow();
            if i.type_arguments.is_empty() {
                format!("<instance of {}>", i.klass.name)
            } else {
                let args: Vec<String> = i.type_arguments.iter().map(|t| t.name()).collect();
                format!("<instance of {}<{}>>", i.klass.name, args.join(", "))
            }
        }
        Value::Class(c) => format!("<class {}>", c.name),
        Value::EnumType(e) => format!("<enum {}>", e.name),
        Value::EnumValue(v) => {
            let v = v.borrow();
            format!("{}.{}", v.parent_enum.name, v.name)
        }
        Value::Function(_) => "Closure".to_string(),
        Value::NativeFn(f) => match &f.name {
            Some(n) => format!("NativeFunction({n})"),
            None => "NativeFunction".to_string(),
        },
        Value::Record(r) => {
            let r = r.borrow();
            let mut parts: Vec<String> = r.positional_fields.iter().map(value_display).collect();
            for (k, v) in &r.named_fields {
                parts.push(format!("{k}: {}", value_display(v)));
            }
            format!("({})", parts.join(", "))
        }
        Value::ExtTypeInst(e) => {
            let e = e.borrow();
            format!("<extension type instance {}>", e.extension_type.name)
        }
        Value::Bridged(b) => {
            let b = b.borrow();
            format!("BridgedInstance({})", value_display(&b.native))
        }
        Value::BridgedClass(b) => format!("<bridged class {}>", b.name),
        Value::BridgedEnum(b) => format!("<bridged enum {}>", b.name),
        Value::BridgedEnumValue(b) => {
            let b = b.borrow();
            format!("{}.{}", b.parent_enum.name, b.name)
        }
        Value::BridgedSuperMethod(_) => "BridgedSuperMethod".to_string(),
        Value::BridgedMixinMethod(_) => "BridgedMixinMethod".to_string(),
        Value::BridgedEnumMixinMethod(_) => "BridgedEnumMixinMethod".to_string(),
        Value::ExtType(e) => format!("<extension type {}>", e.name),
        Value::Extension(_) => "Extension".to_string(),
        Value::PropertyAccessor(_) => "<property>".to_string(),
        Value::Late(_) => "LateVariable".to_string(),
        Value::Type(t) => t.name(),
        Value::Module(_) => "<module>".to_string(),
        Value::SyncGen(_) => "SyncGeneratorIterable".to_string(),
        Value::StringBuffer(s) => s.borrow().clone(),
        Value::BigInt(b) => b.to_string(),
        Value::Future(f) => {
            let st = f.borrow();
            if st.completed {
                match (&st.value, &st.error) {
                    (Some(v), _) => value_display(v),
                    (None, Some(e)) => value_display(e),
                    _ => "null".to_string(),
                }
            } else {
                "Future".to_string()
            }
        }
        Value::BoundSuper(_) => "<super>".to_string(),
        Value::BoundBridgedSuper(_) => "<super>".to_string(),
    }
}

pub fn instance_get(
    inst: &Rc<RefCell<InstanceData>>,
    name: &str,
    visitor: Option<&mut InterpreterVisitor>,
) -> Result<Value, InterpError> {
    {
        let i = inst.borrow();
        if let Some(Some(v)) = i.fields.get(field_slot(name)) {
            return Ok(match v {
                Value::Late(l) => l.borrow_mut().get(),
                v => v.clone(),
            });
        }
    }

    let klass = inst.borrow().klass.clone();
    let mut current: Option<Rc<ClassDef>> = Some(klass.clone());
    while let Some(c) = current {
        if let Some(getter) = c.find_instance_getter(name) {
            let bound = getter.bind_instance(inst.clone());
            return match visitor {
                Some(v) => call_callable(&Callable::Interpreted(bound), v, vec![], HashMap::new()),
                None => Ok(Value::Function(bound)),
            };
        }
        if let Ok(field) = c.get_static_field(name) {
            return Ok(field);
        }
        if let Some(sg) = c.find_static_getter(name) {
            let bound = sg.bind_instance(inst.clone());
            return match visitor {
                Some(v) => call_callable(&Callable::Interpreted(bound), v, vec![], HashMap::new()),
                None => Ok(Value::Function(bound)),
            };
        }
        if let Some(sm) = c.find_static_method(name) {
            return Ok(Value::Function(sm.bind_instance(inst.clone())));
        }
        if let Some(m) = c.find_instance_method(name) {
            return Ok(Value::Function(m.bind_instance(inst.clone())));
        }
        if let Some(bc) = &c.bridged_superclass {
            let has_super_object = inst.borrow().bridged_super_object.is_some();
            if has_super_object {
                let native_target = inst.borrow().bridged_super_object.clone().unwrap();
                if let Some(adapter) = bc.find_instance_getter_adapter(name)
                    && let Some(v) = visitor
                {
                    let result = adapter(v, native_target)?;
                    let _ = bc;
                    return translate_bridged_result(result);
                }
                if let Some(_adapter) = bc.find_instance_method_adapter(name) {
                    return Ok(Value::BridgedSuperMethod(Rc::new(
                        crate::callable::BridgedSuperMethodCallable {
                            native_target,
                            method_name: name.to_string(),
                            bridged_class: bc.clone(),
                        },
                    )));
                }
            }
        }
        current = c.superclass.clone();
    }

    for mixin in klass.mixins.borrow().iter().rev() {
        if let Some(getter) = mixin.find_instance_getter(name) {
            let bound = getter.bind_instance(inst.clone());
            return match visitor {
                Some(v) => call_callable(&Callable::Interpreted(bound), v, vec![], HashMap::new()),
                None => Ok(Value::Function(bound)),
            };
        }
        if let Some(m) = mixin.find_instance_method(name) {
            return Ok(Value::Function(m.bind_instance(inst.clone())));
        }
    }

    for bm in klass.bridged_mixins.borrow().iter().rev() {
        if let Some(adapter) = bm.find_instance_getter_adapter(name)
            && let Some(v) = visitor
        {
            let result = adapter(v, Value::Instance(inst.clone()))?;
            return translate_bridged_result(result);
        }
        if let Some(_adapter) = bm.find_instance_method_adapter(name) {
            return Ok(Value::BridgedMixinMethod(Rc::new(
                crate::callable::BridgedMixinMethodCallable {
                    instance: Value::Instance(inst.clone()),
                    method_name: name.to_string(),
                    bridged_class: bm.clone(),
                },
            )));
        }
    }

    Err(InterpError::runtime(format!(
        "Undefined property '{name}' on {}.",
        inst.borrow().klass.name
    )))
}

pub fn instance_set(
    inst: &Rc<RefCell<InstanceData>>,
    name: &str,
    value: Value,
    visitor: Option<&mut InterpreterVisitor>,
) -> Result<(), InterpError> {
    let klass = inst.borrow().klass.clone();
    let mut current: Option<Rc<ClassDef>> = Some(klass.clone());
    while let Some(c) = current {
        if let Some(setter) = c.find_instance_setter(name) {
            let bound = setter.bind_instance(inst.clone());
            if let Some(v) = visitor {
                call_callable(
                    &Callable::Interpreted(bound),
                    v,
                    vec![value],
                    HashMap::new(),
                )?;
                return Ok(());
            }
        }
        if let Some(bc) = &c.bridged_superclass {
            let has_super_object = inst.borrow().bridged_super_object.is_some();
            if has_super_object {
                let native_target = inst.borrow().bridged_super_object.clone().unwrap();
                if let Some(adapter) = bc.find_instance_setter_adapter(name)
                    && let Some(v) = visitor
                {
                    adapter(v, native_target, value)?;
                    return Ok(());
                }
            }
        }
        current = c.superclass.clone();
    }
    for bm in klass.bridged_mixins.borrow().iter().rev() {
        if let Some(adapter) = bm.find_instance_setter_adapter(name)
            && let Some(v) = visitor
        {
            adapter(v, Value::Instance(inst.clone()), value.clone())?;
            return Ok(());
        }
    }

    {
        let mut i = inst.borrow_mut();
        if let Some(Some(Value::Late(l))) = i.fields.get(field_slot(name)) {
            l.borrow_mut().assign(value);
        } else {
            let slot = field_slot(name);
            if slot >= i.fields.len() {
                i.fields.resize(slot + 1, None);
            }
            i.fields[slot] = Some(value);
        }
    }
    Ok(())
}

pub fn instance_has_field(inst: &Rc<RefCell<InstanceData>>, name: &str) -> bool {
    inst.borrow().fields.get(field_slot(name)).is_some_and(|o| o.is_some())
}

pub fn instance_try_get_field(inst: &Rc<RefCell<InstanceData>>, name: &str) -> Option<Value> {
    inst.borrow().fields.get(field_slot(name)).and_then(|o| o.clone())
}

pub fn instance_get_field_direct(
    inst: &Rc<RefCell<InstanceData>>,
    name: &str,
) -> Result<Value, InterpError> {
    match inst.borrow().fields.get(field_slot(name)) {
        Some(Some(Value::Late(l))) => Ok(l.borrow_mut().get()),
        Some(Some(v)) => Ok(v.clone()),
        _ => Err(InterpError::runtime(format!("Field not found: {name}"))),
    }
}

pub fn translate_bridged_result(result: Value) -> Result<Value, InterpError> {
    Ok(result)
}

pub fn enum_value_get(
    enum_value: &Rc<RefCell<EnumValueData>>,
    name: &str,
    visitor: Option<&mut InterpreterVisitor>,
) -> Result<Value, InterpError> {
    if name == "name" {
        return Ok(Value::str(enum_value.borrow().name.clone()));
    }
    if name == "index" {
        return Ok(Value::Int(enum_value.borrow().index as i64));
    }
    {
        let v = enum_value.borrow();
        if let Some(f) = v.fields.get(name) {
            return Ok(f.clone());
        }
    }
    let parent_enum = enum_value.borrow().parent_enum.clone();
    if let Some(getter) = parent_enum.getters.borrow().get(name) {
        let bound = getter.bind_enum_value(enum_value.clone());
        return match visitor {
            Some(v) => call_callable(&Callable::Interpreted(bound), v, vec![], HashMap::new()),
            None => Ok(Value::Function(bound)),
        };
    }
    if let Some(m) = parent_enum.methods.borrow().get(name) {
        return Ok(Value::Function(m.bind_enum_value(enum_value.clone())));
    }
    for mixin in parent_enum.mixins.borrow().iter().rev() {
        if let Some(getter) = mixin.find_instance_getter(name) {
            let bound = getter.bind_enum_value(enum_value.clone());
            return match visitor {
                Some(v) => call_callable(&Callable::Interpreted(bound), v, vec![], HashMap::new()),
                None => Ok(Value::Function(bound)),
            };
        }
        if let Some(m) = mixin.find_instance_method(name) {
            return Ok(Value::Function(m.bind_enum_value(enum_value.clone())));
        }
    }
    for bm in parent_enum.bridged_mixins.borrow().iter().rev() {
        if let Some(adapter) = bm.find_instance_getter_adapter(name)
            && let Some(v) = visitor
        {
            let result = adapter(v, Value::EnumValue(enum_value.clone()))?;
            return Ok(result);
        }
        if let Some(_adapter) = bm.find_instance_method_adapter(name) {
            return Ok(Value::BridgedEnumMixinMethod(Rc::new(
                crate::callable::BridgedEnumMixinMethodCallable {
                    enum_value: Value::EnumValue(enum_value.clone()),
                    method_name: name.to_string(),
                    call_target: crate::callable::BridgedEnumMixinCallTarget::Class(bm.clone()),
                },
            )));
        }
    }
    Err(InterpError::runtime(format!(
        "Undefined property {name} on enum value {}",
        enum_value.borrow().name
    )))
}

pub fn enum_value_set(
    enum_value: &Rc<RefCell<EnumValueData>>,
    name: &str,
    value: Value,
    _visitor: Option<&mut InterpreterVisitor>,
) -> Result<(), InterpError> {
    let parent_enum = enum_value.borrow().parent_enum.clone();
    if let Some(setter) = parent_enum.setters.borrow().get(name) {
        let bound = setter.bind_enum_value(enum_value.clone());
        if let Some(v) = _visitor {
            call_callable(
                &Callable::Interpreted(bound),
                v,
                vec![value],
                HashMap::new(),
            )?;
            return Ok(());
        }
    }
    enum_value
        .borrow_mut()
        .fields
        .insert(name.to_string(), value);
    Ok(())
}

pub fn value_to_string(value: &Value) -> String {
    value_display(value)
}

pub fn is_truthy(value: &Value) -> Result<bool, InterpError> {
    match value {
        Value::Bool(b) => Ok(*b),
        Value::Bridged(b) => {
            let native = &b.borrow().native;
            if let Value::Bool(b2) = native {
                Ok(*b2)
            } else {
                Err(InterpError::runtime("Expected a boolean value"))
            }
        }
        _ => Err(InterpError::runtime(
            "Expected a boolean value, got ".to_string() + &value_display(value),
        )),
    }
}

pub fn is_bool_value(value: &Value) -> bool {
    matches!(value, Value::Bool(_))
}

pub fn num_add(a: &Value, b: &Value) -> Option<Value> {
    match (a, b) {
        (Value::Int(x), Value::Int(y)) => Some(Value::Int(x.wrapping_add(*y))),
        (Value::Int(x), Value::Double(y)) => Some(Value::Double(*x as f64 + y)),
        (Value::Double(x), Value::Int(y)) => Some(Value::Double(x + *y as f64)),
        (Value::Double(x), Value::Double(y)) => Some(Value::Double(x + y)),
        _ => None,
    }
}

pub fn num_sub(a: &Value, b: &Value) -> Option<Value> {
    match (a, b) {
        (Value::Int(x), Value::Int(y)) => Some(Value::Int(x.wrapping_sub(*y))),
        (Value::Int(x), Value::Double(y)) => Some(Value::Double(*x as f64 - y)),
        (Value::Double(x), Value::Int(y)) => Some(Value::Double(x - *y as f64)),
        (Value::Double(x), Value::Double(y)) => Some(Value::Double(x - y)),
        _ => None,
    }
}

pub fn num_mul(a: &Value, b: &Value) -> Option<Value> {
    match (a, b) {
        (Value::Int(x), Value::Int(y)) => Some(Value::Int(x.wrapping_mul(*y))),
        (Value::Int(x), Value::Double(y)) => Some(Value::Double(*x as f64 * y)),
        (Value::Double(x), Value::Int(y)) => Some(Value::Double(x * *y as f64)),
        (Value::Double(x), Value::Double(y)) => Some(Value::Double(x * y)),
        _ => None,
    }
}

pub fn num_div(a: &Value, b: &Value) -> Option<Value> {
    match (a, b) {
        (Value::Int(x), Value::Int(y)) => Some(Value::Double(*x as f64 / *y as f64)),
        (Value::Int(x), Value::Double(y)) => Some(Value::Double(*x as f64 / y)),
        (Value::Double(x), Value::Int(y)) => Some(Value::Double(x / *y as f64)),
        (Value::Double(x), Value::Double(y)) => Some(Value::Double(x / y)),
        _ => None,
    }
}

pub fn num_trunc_div(a: &Value, b: &Value) -> Option<Result<Value, InterpError>> {
    match (a, b) {
        (Value::Int(x), Value::Int(y)) => {
            if *y == 0 {
                Some(Err(InterpError::runtime(
                    "Division by zero in integer division (~/)".to_string(),
                )))
            } else {
                Some(Ok(Value::Int(x.wrapping_div(*y))))
            }
        }
        (Value::Int(x), Value::Double(y)) => {
            if *y == 0.0 {
                Some(Err(InterpError::runtime(
                    "Division by zero in integer division (~/)".to_string(),
                )))
            } else {
                Some(Ok(Value::Int((*x as f64 / y).trunc() as i64)))
            }
        }
        (Value::Double(x), Value::Int(y)) => {
            if *y == 0 {
                Some(Err(InterpError::runtime(
                    "Division by zero in integer division (~/)".to_string(),
                )))
            } else {
                Some(Ok(Value::Int((x / *y as f64).trunc() as i64)))
            }
        }
        (Value::Double(x), Value::Double(y)) => {
            if *y == 0.0 {
                Some(Err(InterpError::runtime(
                    "Division by zero in integer division (~/)".to_string(),
                )))
            } else {
                Some(Ok(Value::Int((x / y).trunc() as i64)))
            }
        }
        _ => None,
    }
}

pub fn num_rem(a: &Value, b: &Value) -> Option<Result<Value, InterpError>> {
    match (a, b) {
        (Value::Int(x), Value::Int(y)) => {
            if *y == 0 {
                Some(Err(InterpError::runtime(
                    "Division by zero in %".to_string(),
                )))
            } else {
                Some(Ok(Value::Int(x.wrapping_rem(*y))))
            }
        }
        (Value::Int(x), Value::Double(y)) => {
            if *y == 0.0 {
                Some(Err(InterpError::runtime(
                    "Division by zero in %".to_string(),
                )))
            } else {
                Some(Ok(Value::Double((*x as f64) % y)))
            }
        }
        (Value::Double(x), Value::Int(y)) => {
            if *y == 0 {
                Some(Err(InterpError::runtime(
                    "Division by zero in %".to_string(),
                )))
            } else {
                Some(Ok(Value::Double(x % *y as f64)))
            }
        }
        (Value::Double(x), Value::Double(y)) => {
            if *y == 0.0 {
                Some(Err(InterpError::runtime(
                    "Division by zero in %".to_string(),
                )))
            } else {
                Some(Ok(Value::Double(x % y)))
            }
        }
        _ => None,
    }
}

pub fn num_neg(a: &Value) -> Option<Value> {
    match a {
        Value::Int(x) => Some(Value::Int(x.wrapping_neg())),
        Value::Double(x) => Some(Value::Double(-x)),
        _ => None,
    }
}

pub fn num_compare(a: &Value, b: &Value) -> Option<std::cmp::Ordering> {
    match (a, b) {
        (Value::Int(x), Value::Int(y)) => Some(x.cmp(y)),
        (Value::Int(x), Value::Double(y)) => (*x as f64).partial_cmp(y),
        (Value::Double(x), Value::Int(y)) => x.partial_cmp(&(*y as f64)),
        (Value::Double(x), Value::Double(y)) => x.partial_cmp(y),
        _ => None,
    }
}
