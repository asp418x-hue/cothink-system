use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use crate::class::{BridgedClassDef, BridgedEnumDef, ExtensionDef};
use crate::error::InterpError;
use crate::rt::{RuntimeType, runtime_type_of};
use crate::value::Value;

pub struct Environment {
    pub enclosing: Option<Rc<RefCell<Environment>>>,
    pub values: HashMap<String, Value>,
    pub bridged_classes: HashMap<String, Arc<BridgedClassDef>>,
    pub bridged_enums: HashMap<String, Rc<BridgedEnumDef>>,
    pub unnamed_extensions: Vec<Rc<ExtensionDef>>,
    pub prefixed_imports: HashMap<String, Rc<RefCell<Environment>>>,
    pub annotated_types: HashMap<usize, Rc<RuntimeType>>,
}

impl Environment {
    pub fn new() -> Rc<RefCell<Self>> {
        Rc::new(RefCell::new(Environment {
            enclosing: None,
            values: HashMap::new(),
            bridged_classes: HashMap::new(),
            bridged_enums: HashMap::new(),
            unnamed_extensions: Vec::new(),
            prefixed_imports: HashMap::new(),
            annotated_types: HashMap::new(),
        }))
    }

    pub fn child(enclosing: &Rc<RefCell<Environment>>) -> Rc<RefCell<Environment>> {
        Rc::new(RefCell::new(Environment {
            enclosing: Some(enclosing.clone()),
            values: HashMap::new(),
            bridged_classes: HashMap::new(),
            bridged_enums: HashMap::new(),
            unnamed_extensions: Vec::new(),
            prefixed_imports: HashMap::new(),
            annotated_types: HashMap::new(),
        }))
    }

    pub fn define(this: &Rc<RefCell<Environment>>, name: &str, value: Value) {
        let mut env = this.borrow_mut();
        if env.bridged_classes.contains_key(name) || env.bridged_enums.contains_key(name) {
            crate::log::warn(&format!(
                "Redefining variable or colliding with bridged type: {name}"
            ));
        }
        env.values.insert(name.to_string(), value);
    }

    pub fn get(this: &Rc<RefCell<Environment>>, name: &str) -> Result<Value, InterpError> {
        if let Some(prefix) = this.borrow().prefixed_imports.get(name) {
            return Ok(Value::Module(prefix.clone()));
        }
        if name.contains('.') {
            let parts: Vec<&str> = name.split('.').collect();
            if parts.len() == 2 {
                let prefix = parts[0];
                let ident = parts[1];
                let env = this.borrow();
                if let Some(prefix_env) = env.prefixed_imports.get(prefix) {
                    return Environment::get(prefix_env, ident).map_err(|e| {
                        InterpError::runtime(format!(
                            "Undefined name '{ident}' in imported prefix '{prefix}'. Original error: {}",
                            e.message().unwrap_or("unknown")
                        ))
                    });
                }
            } else {
                return Err(InterpError::runtime(format!(
                    "Complex prefixed access not supported: {name}. Use the form prefix.identifier."
                )));
            }
        }

        let mut current: Rc<RefCell<Environment>> = this.clone();
        loop {
            let env = current.borrow();
            if env.prefixed_imports.contains_key(name) {
                let prefix_env = env.prefixed_imports[name].clone();
                return Ok(Value::Module(prefix_env));
            }
            if let Some(v) = env.values.get(name) {
                return Ok(v.clone());
            }
            if env.bridged_classes.contains_key(name) {
                return Ok(Value::BridgedClass(env.bridged_classes[name].clone()));
            }
            if env.bridged_enums.contains_key(name) {
                return Ok(Value::BridgedEnum(env.bridged_enums[name].clone()));
            }
            match &env.enclosing {
                Some(parent) => {
                    let parent = parent.clone();
                    drop(env);
                    current = parent;
                }
                None => return Err(InterpError::runtime(format!("Undefined variable: {name}"))),
            }
        }
    }

    pub fn assign(
        this: &Rc<RefCell<Environment>>,
        name: &str,
        value: Value,
    ) -> Result<Value, InterpError> {
        let mut current: Rc<RefCell<Environment>> = this.clone();
        loop {
            if current.borrow().values.contains_key(name) {
                current
                    .borrow_mut()
                    .values
                    .insert(name.to_string(), value.clone());
                return Ok(value);
            }
            if current.borrow().bridged_classes.contains_key(name) {
                return Err(InterpError::runtime(format!(
                    "Cannot assign to the name of a bridged class: {name}"
                )));
            }
            if current.borrow().bridged_enums.contains_key(name) {
                return Err(InterpError::runtime(format!(
                    "Cannot assign to the name of a bridged enum: {name}"
                )));
            }
            let parent = match current.borrow().enclosing.clone() {
                Some(p) => p,
                None => {
                    return Err(InterpError::runtime(format!(
                        "Assigning to undefined variable '{name}'."
                    )));
                }
            };
            current = parent;
        }
    }

    pub fn is_defined_locally(this: &Rc<RefCell<Environment>>, name: &str) -> bool {
        this.borrow().values.contains_key(name)
    }

    pub fn find_defining_environment(
        this: &Rc<RefCell<Environment>>,
        name: &str,
    ) -> Option<Rc<RefCell<Environment>>> {
        let mut current = this.clone();
        loop {
            if current.borrow().values.contains_key(name) {
                return Some(current);
            }
            let enclosing = current.borrow().enclosing.clone();
            {
                let parent = enclosing?;
                current = parent
            }
        }
    }

    pub fn define_bridge(this: &Rc<RefCell<Environment>>, bridged: Arc<BridgedClassDef>) {
        let mut env = this.borrow_mut();
        if env.values.contains_key(&bridged.name)
            || env.bridged_classes.contains_key(&bridged.name)
            || env.bridged_enums.contains_key(&bridged.name)
        {
            crate::log::warn(&format!(
                "Redefining bridged class or colliding with existing definition: {}",
                bridged.name
            ));
        }
        env.bridged_classes
            .insert(bridged.name.clone(), bridged.clone());
    }

    pub fn define_bridged_enum(this: &Rc<RefCell<Environment>>, bridged: Rc<BridgedEnumDef>) {
        let mut env = this.borrow_mut();
        if env.values.contains_key(&bridged.name)
            || env.bridged_classes.contains_key(&bridged.name)
            || env.bridged_enums.contains_key(&bridged.name)
        {
            crate::log::warn(&format!(
                "Redefining bridged enum or colliding with existing definition: {}",
                bridged.name
            ));
        }
        env.bridged_enums.insert(bridged.name.clone(), bridged);
    }

    pub fn define_prefixed_import(
        this: &Rc<RefCell<Environment>>,
        prefix: &str,
        import_env: Rc<RefCell<Environment>>,
    ) {
        this.borrow_mut()
            .prefixed_imports
            .insert(prefix.to_string(), import_env);
    }

    pub fn add_unnamed_extension(this: &Rc<RefCell<Environment>>, ext: Rc<ExtensionDef>) {
        this.borrow_mut().unnamed_extensions.push(ext);
    }

    pub fn annotate_runtime_type(
        this: &Rc<RefCell<Environment>>,
        value: &Value,
        t: Option<Rc<RuntimeType>>,
    ) {
        let (Some(t), Some(ptr)) = (t, value.ptr()) else {
            return;
        };
        if matches!(
            value,
            Value::Int(_) | Value::Double(_) | Value::Str(_) | Value::Bool(_)
        ) {
            return;
        }
        this.borrow_mut().annotated_types.insert(ptr, t);
    }

    pub fn get_annotated_runtime_type(
        this: &Rc<RefCell<Environment>>,
        value: &Value,
    ) -> Option<Rc<RuntimeType>> {
        if matches!(
            value,
            Value::Int(_) | Value::Double(_) | Value::Str(_) | Value::Bool(_)
        ) {
            return None;
        }
        let ptr = value.ptr()?;
        this.borrow().annotated_types.get(&ptr).cloned()
    }

    pub fn get_runtime_type(
        this: &Rc<RefCell<Environment>>,
        value: &Value,
    ) -> Option<Rc<RuntimeType>> {
        if let Some(ann) = Environment::get_annotated_runtime_type(this, value) {
            return Some(ann);
        }
        match value {
            Value::Module(_) => None,
            Value::Bridged(_) | Value::BridgedClass(_) | Value::BridgedEnum(_) => {
                runtime_type_of(Some(value))
            }
            _ => runtime_type_of(Some(value)),
        }
    }

    pub fn shallow_copy_filtered(
        this: &Rc<RefCell<Environment>>,
        show: Option<&[String]>,
        hide: Option<&[String]>,
    ) -> Rc<RefCell<Environment>> {
        let new_env = Environment {
            enclosing: this.borrow().enclosing.clone(),
            values: HashMap::new(),
            bridged_classes: HashMap::new(),
            bridged_enums: HashMap::new(),
            unnamed_extensions: Vec::new(),
            prefixed_imports: HashMap::new(),
            annotated_types: HashMap::new(),
        };
        let new_env = Rc::new(RefCell::new(new_env));
        let env = this.borrow();
        let include = |name: &str| -> bool {
            if let Some(show) = show {
                show.contains(&name.to_string())
            } else if let Some(hide) = hide {
                !hide.contains(&name.to_string())
            } else {
                true
            }
        };
        for (name, value) in env.values.iter() {
            if include(name) {
                new_env
                    .borrow_mut()
                    .values
                    .insert(name.clone(), value.clone());
            }
        }
        for (name, bc) in env.bridged_classes.iter() {
            if include(name) {
                new_env
                    .borrow_mut()
                    .bridged_classes
                    .insert(name.clone(), bc.clone());
            }
        }
        for (name, be) in env.bridged_enums.iter() {
            if include(name) {
                new_env
                    .borrow_mut()
                    .bridged_enums
                    .insert(name.clone(), be.clone());
            }
        }
        for (name, p) in env.prefixed_imports.iter() {
            if include(name) {
                new_env
                    .borrow_mut()
                    .prefixed_imports
                    .insert(name.clone(), p.clone());
            }
        }
        new_env
            .borrow_mut()
            .unnamed_extensions
            .extend(env.unnamed_extensions.iter().cloned());
        new_env
    }

    pub fn import_environment(
        this: &Rc<RefCell<Environment>>,
        other: &Rc<RefCell<Environment>>,
        show: Option<&[String]>,
        hide: Option<&[String]>,
    ) -> Result<(), InterpError> {
        let source = if show.is_some() || hide.is_some() {
            Environment::shallow_copy_filtered(other, show, hide)
        } else {
            other.clone()
        };
        let source_values: Vec<(String, Value)> = source
            .borrow()
            .values
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        let source_classes: Vec<(String, Arc<BridgedClassDef>)> = source
            .borrow()
            .bridged_classes
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        let source_enums: Vec<(String, Rc<BridgedEnumDef>)> = source
            .borrow()
            .bridged_enums
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        let source_imports: Vec<(String, Rc<RefCell<Environment>>)> = source
            .borrow()
            .prefixed_imports
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        let source_extensions: Vec<Rc<ExtensionDef>> = source.borrow().unnamed_extensions.clone();

        let mut env = this.borrow_mut();
        for (name, value) in source_values {
            if let Some(existing) = env.values.get(&name) {
                if !crate::value::value_eq(existing, &value) {
                    return Err(InterpError::runtime(format!(
                        "Name conflict in environment: Symbol '{name}' is already defined with a different value."
                    )));
                }
                continue;
            }
            if env.bridged_classes.contains_key(&name)
                || env.bridged_enums.contains_key(&name)
                || env.prefixed_imports.contains_key(&name)
            {
                return Err(InterpError::runtime(format!(
                    "Name conflict in environment: Symbol '{name}' is already defined."
                )));
            }
            env.values.insert(name, value);
        }
        for (name, bc) in source_classes {
            if let Some(existing) = env.bridged_classes.get(&name) {
                if !std::sync::Arc::ptr_eq(existing, &bc) {
                    return Err(InterpError::runtime(format!(
                        "Name conflict in environment: Symbol '{name}' (bridged class) is already defined with a different value."
                    )));
                }
                continue;
            }
            if env.values.contains_key(&name)
                || env.bridged_enums.contains_key(&name)
                || env.prefixed_imports.contains_key(&name)
            {
                return Err(InterpError::runtime(format!(
                    "Name conflict in environment: Symbol '{name}' (bridged class) is already defined."
                )));
            }
            env.bridged_classes.insert(name, bc.clone());
        }
        for (name, be) in source_enums {
            if let Some(existing) = env.bridged_enums.get(&name) {
                if !Rc::ptr_eq(existing, &be) {
                    return Err(InterpError::runtime(format!(
                        "Name conflict in environment: Symbol '{name}' (bridged enum) is already defined with a different value."
                    )));
                }
                continue;
            }
            if env.values.contains_key(&name)
                || env.bridged_classes.contains_key(&name)
                || env.prefixed_imports.contains_key(&name)
            {
                return Err(InterpError::runtime(format!(
                    "Name conflict in environment: Symbol '{name}' (bridged enum) is already defined."
                )));
            }
            env.bridged_enums.insert(name, be);
        }
        for (name, p) in source_imports {
            if let Some(existing) = env.prefixed_imports.get(&name) {
                if !Rc::ptr_eq(existing, &p) {
                    return Err(InterpError::runtime(format!(
                        "Name conflict in environment: Symbol '{name}' (prefixed import) is already defined with a different environment."
                    )));
                }
                continue;
            }
            if env.values.contains_key(&name)
                || env.bridged_classes.contains_key(&name)
                || env.bridged_enums.contains_key(&name)
            {
                return Err(InterpError::runtime(format!(
                    "Name conflict in environment: Symbol '{name}' (prefixed import) is already defined or collides with another symbol type."
                )));
            }
            env.prefixed_imports.insert(name, p);
        }
        env.unnamed_extensions.extend(source_extensions);
        Ok(())
    }

    /// Merges the *public* top-level names of `other` into `this`, mirroring
    /// Dart's `export` directive. Private (`_`-prefixed) names and prefixed
    /// imports stay library-local and are not re-exported; conflict
    /// detection matches [`Environment::import_environment`].
    pub fn export_environment(
        this: &Rc<RefCell<Environment>>,
        other: &Rc<RefCell<Environment>>,
        show: Option<&[String]>,
        hide: Option<&[String]>,
    ) -> Result<(), InterpError> {
        let public = Environment::new();
        {
            let src = other.borrow();
            let mut dst = public.borrow_mut();
            for (name, value) in src.values.iter() {
                if !name.starts_with('_') {
                    dst.values.insert(name.clone(), value.clone());
                }
            }
            for (name, bc) in src.bridged_classes.iter() {
                if !name.starts_with('_') {
                    dst.bridged_classes.insert(name.clone(), bc.clone());
                }
            }
            for (name, be) in src.bridged_enums.iter() {
                if !name.starts_with('_') {
                    dst.bridged_enums.insert(name.clone(), be.clone());
                }
            }
            dst.unnamed_extensions
                .extend(src.unnamed_extensions.iter().cloned());
        }
        if show.is_some() || hide.is_some() {
            let filtered = Environment::shallow_copy_filtered(&public, show, hide);
            Environment::import_environment(this, &filtered, None, None)
        } else {
            Environment::import_environment(this, &public, None, None)
        }
    }

    pub fn find_extension_member(
        this: &Rc<RefCell<Environment>>,
        target: &Value,
        name: &str,
    ) -> Option<crate::callable::Callable> {
        let target_type = Environment::get_runtime_type(this, target)?;
        let mut current = this.clone();
        loop {
            let ext_candidates: Vec<Rc<ExtensionDef>> = {
                let env = current.borrow();
                let mut v: Vec<Rc<ExtensionDef>> = env.unnamed_extensions.clone();
                for value in env.values.values() {
                    if let Value::Extension(e) = value {
                        v.push(e.clone());
                    }
                }
                v
            };
            for ext in ext_candidates {
                if target_type.is_subtype_of(&ext.on_type, Some(target))
                    && let Some(member) = ext.find_member(name)
                {
                    return Some(member);
                }
            }
            let parent = match current.borrow().enclosing.clone() {
                Some(p) => p,
                None => break,
            };
            current = parent;
        }
        None
    }

    pub fn to_bridged_class(
        this: &Rc<RefCell<Environment>>,
        value: &Value,
    ) -> Option<Arc<BridgedClassDef>> {
        let native_name = crate::value::native_type_name(value);
        let mut current = this.clone();
        loop {
            let env = current.borrow();
            if let Some(name) = native_name {
                for bc in env.bridged_classes.values() {
                    if bc.native_type.as_deref() == Some(name) {
                        return Some(bc.clone());
                    }
                }
            }
            for bc in env.bridged_classes.values() {
                if bc.is_subtype_of_fn.as_ref().is_some_and(|f| f(value)) {
                    return Some(bc.clone());
                }
            }
            match &env.enclosing {
                Some(p) => {
                    let p = p.clone();
                    drop(env);
                    current = p;
                }
                None => break,
            }
        }
        None
    }

    pub fn to_bridged_instance(this: &Rc<RefCell<Environment>>, value: &Value) -> Option<Value> {
        match value {
            Value::Bridged(_) => Some(value.clone()),
            v => {
                let bc = Environment::to_bridged_class(this, v)?;
                Some(Value::Bridged(Rc::new(RefCell::new(
                    crate::class::BridgedInstanceData {
                        bridged_class: bc,
                        native: v.clone(),
                        type_arguments: vec![],
                        bridged_super_object: None,
                    },
                ))))
            }
        }
    }
}
