//! dart:collection: HashMap, LinkedHashMap, SplayTreeMap, HashSet,
//! LinkedHashSet, SplayTreeSet, ListQueue, Queue, UnmodifiableListView,
//! LinkedList, LinkedListEntry.
//!
//! The hash-based and tree-based containers share the core value
//! representations (`Value::Map` / `Value::Set` / `Value::List`); the
//! ordering guarantees of the real classes are approximated.

use super::helpers as h;
use crate::class::BridgedClassDef;
use crate::env::Environment;
use crate::error::InterpError;
use crate::value::{DartMap, DartSet, Value};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

fn define_bridge(env: &Rc<RefCell<Environment>>, bc: BridgedClassDef) {
    let bc = Arc::new(bc);
    h::register_class(bc.clone());
    Environment::define_bridge(env, bc);
}

pub fn load(env: &Rc<RefCell<Environment>>) {
    define_bridge(env, hash_map_definition());
    define_bridge(env, linked_hash_map_definition());
    define_bridge(env, splay_tree_map_definition());
    define_bridge(env, hash_set_definition());
    define_bridge(env, linked_hash_set_definition());
    define_bridge(env, splay_tree_set_definition());
    define_bridge(env, list_queue_definition());
    define_bridge(env, queue_definition());
    define_bridge(env, unmodifiable_list_view_definition());
    define_bridge(env, linked_list_definition());
    define_bridge(env, linked_list_entry_definition());
}

// ---------------------------------------------------------------------------
// Shared map behavior (HashMap, LinkedHashMap, SplayTreeMap)
// ---------------------------------------------------------------------------

fn map_class(name: &str, is_splay: bool) -> BridgedClassDef {
    let name = name.to_string();
    let mut bc = BridgedClassDef::new(&name);
    bc.native_type = Some(format!("d4rt::collection::{name}"));
    {
        let n = name.clone();
        bc.constructors.insert(
            "".into(),
            h::ctor(move |_v, _a, _n| Ok(wrap_cls(&n, empty_map()))),
        );
    }
    let static_map = |_is_splay: bool, n: String| {
        h::static_method(move |_v, args, _n| {
            let m = empty_map();
            match args.first() {
                Some(Value::Map(src)) => {
                    for (k, v) in src.borrow().entries.iter() {
                        map_put(&m, k.clone(), v.clone());
                    }
                }
                Some(Value::Null) => {}
                Some(other) => {
                    return Err(h::err(format!(
                        "Map.{n} expects a Map or iterable, got {}",
                        h::display(other)
                    )));
                }
                None => {}
            }
            Ok(wrap_cls(&n, m))
        })
    };
    bc.static_methods
        .insert("from".into(), static_map(is_splay, name.clone()));
    bc.static_methods
        .insert("of".into(), static_map(is_splay, name.clone()));
    {
        let n = name.clone();
        bc.static_methods.insert(
            "identity".into(),
            h::static_method(move |_v, _a, _n| Ok(wrap_cls(&n, empty_map()))),
        );
    }
    map_common_methods(&mut bc);
    bc
}

fn empty_map() -> Value {
    Value::Map(Rc::new(RefCell::new(DartMap { entries: vec![] })))
}

fn wrap_cls(name: &str, native: Value) -> Value {
    h::wrap(&h::class(name), native)
}

fn map_get_index(m: &Value, key: &Value) -> Option<usize> {
    let m = h::as_map(m)?;
    let m = m.borrow();
    m.entries.iter().position(|(k, _)| key_eq(k, key))
}

fn key_eq(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Str(x), Value::Str(y)) => x == y,
        (Value::Int(x), Value::Int(y)) => x == y,
        (Value::Double(x), Value::Double(y)) => x == y,
        (Value::Bool(x), Value::Bool(y)) => x == y,
        (Value::Null, Value::Null) => true,
        _ => std::ptr::eq(a, b) || crate::value::value_eq(a, b),
    }
}

fn map_put(m: &Value, key: Value, value: Value) {
    let mm = h::as_map(m).expect("map_put on non-map");
    let mut mm = mm.borrow_mut();
    if let Some(i) = mm.entries.iter().position(|(k, _)| key_eq(k, &key)) {
        mm.entries[i].1 = value;
    } else {
        mm.entries.push((key, value));
    }
}

fn map_common_methods(bc: &mut BridgedClassDef) {
    bc.getters.insert(
        "length".into(),
        h::getter(|_v, t| {
            let m = h::as_map(&t).ok_or_else(|| h::err("Not a map"))?;
            Ok(Value::Int(m.borrow().entries.len() as i64))
        }),
    );
    bc.getters.insert(
        "isEmpty".into(),
        h::getter(|_v, t| {
            let m = h::as_map(&t).ok_or_else(|| h::err("Not a map"))?;
            Ok(Value::Bool(m.borrow().entries.is_empty()))
        }),
    );
    bc.getters.insert(
        "isNotEmpty".into(),
        h::getter(|_v, t| {
            let m = h::as_map(&t).ok_or_else(|| h::err("Not a map"))?;
            Ok(Value::Bool(!m.borrow().entries.is_empty()))
        }),
    );
    bc.getters.insert(
        "keys".into(),
        h::getter(|_v, t| {
            let m = h::as_map(&t).ok_or_else(|| h::err("Not a map"))?;
            let m = m.borrow();
            let items = m.entries.iter().map(|(k, _)| k.clone()).collect();
            Ok(Value::Set(Rc::new(RefCell::new(DartSet { items }))))
        }),
    );
    bc.getters.insert(
        "values".into(),
        h::getter(|_v, t| {
            let m = h::as_map(&t).ok_or_else(|| h::err("Not a map"))?;
            let m = m.borrow();
            let items = m.entries.iter().map(|(_, v)| v.clone()).collect();
            Ok(Value::List(Rc::new(RefCell::new(items))))
        }),
    );
    bc.getters.insert(
        "entries".into(),
        h::getter(|_v, t| {
            let m = h::as_map(&t).ok_or_else(|| h::err("Not a map"))?;
            let m = m.borrow();
            let items: Vec<Value> = m
                .entries
                .iter()
                .map(|(k, v)| map_entry_value(k.clone(), v.clone()))
                .collect();
            Ok(Value::List(Rc::new(RefCell::new(items))))
        }),
    );
    bc.getters.insert(
        "hashCode".into(),
        h::getter(|_v, t| Ok(Value::Int(h::hash_code(&t)))),
    );
    bc.methods.insert(
        "containsKey".into(),
        h::method(|_v, t, args, _n| {
            h::require_args(&args, 1, "containsKey")?;
            Ok(Value::Bool(map_get_index(&t, &args[0]).is_some()))
        }),
    );
    bc.methods.insert(
        "containsValue".into(),
        h::method(|_v, t, args, _n| {
            h::require_args(&args, 1, "containsValue")?;
            let m = h::as_map(&t).ok_or_else(|| h::err("Not a map"))?;
            let m = m.borrow();
            Ok(Value::Bool(
                m.entries
                    .iter()
                    .any(|(_, v)| crate::value::value_eq(v, &args[0])),
            ))
        }),
    );
    bc.methods.insert(
        "putIfAbsent".into(),
        h::method(|_v, t, args, _n| {
            h::require_args(&args, 1, "putIfAbsent")?;
            if let Some(i) = map_get_index(&t, &args[0]) {
                let m = h::as_map(&t).unwrap();
                return Ok(m.borrow().entries[i].1.clone());
            }
            let key = args[0].clone();
            let f = args.get(1).cloned().unwrap_or(Value::Null);
            let value = _v.call_value(&f, vec![], std::collections::HashMap::new())?;
            map_put(&t, key, value.clone());
            Ok(value)
        }),
    );
    bc.methods.insert(
        "addAll".into(),
        h::method(|_v, t, args, _n| {
            h::require_args(&args, 1, "addAll")?;
            let src = h::as_map(&args[0]).ok_or_else(|| h::err("addAll expects a Map"))?;
            for (k, v) in src.borrow().entries.iter() {
                map_put(&t, k.clone(), v.clone());
            }
            Ok(Value::Null)
        }),
    );
    bc.methods.insert(
        "addEntries".into(),
        h::method(|_v, t, args, _n| {
            h::require_args(&args, 1, "addEntries")?;
            let src = h::as_list(&args[0])
                .ok_or_else(|| h::err("addEntries expects an Iterable of MapEntry"))?;
            for e in src.borrow().iter() {
                let e = map_entry_parts(e);
                if let Some((k, v)) = e {
                    map_put(&t, k, v);
                }
            }
            Ok(Value::Null)
        }),
    );
    bc.methods.insert(
        "remove".into(),
        h::method(|_v, t, args, _n| {
            h::require_args(&args, 1, "remove")?;
            let m = h::as_map(&t).ok_or_else(|| h::err("Not a map"))?;
            let mut m = m.borrow_mut();
            let i = m.entries.iter().position(|(k, _)| key_eq(k, &args[0]));
            match i {
                Some(i) => Ok(m.entries.remove(i).1),
                None => Ok(Value::Null),
            }
        }),
    );
    bc.methods.insert(
        "clear".into(),
        h::method(|_v, t, _args, _n| {
            let m = h::as_map(&t).ok_or_else(|| h::err("Not a map"))?;
            m.borrow_mut().entries.clear();
            Ok(Value::Null)
        }),
    );
    bc.methods.insert(
        "forEach".into(),
        h::method(|_v, t, args, _n| {
            h::require_args(&args, 1, "forEach")?;
            let m = h::as_map(&t).ok_or_else(|| h::err("Not a map"))?;
            let entries: Vec<(Value, Value)> = m.borrow().entries.to_vec();
            for (k, v) in entries {
                _v.call_value(&args[0], vec![k, v], std::collections::HashMap::new())?;
            }
            Ok(Value::Null)
        }),
    );
    bc.methods.insert(
        "map".into(),
        h::method(|_v, t, args, _n| {
            h::require_args(&args, 1, "map")?;
            let m = h::as_map(&t).ok_or_else(|| h::err("Not a map"))?;
            let entries: Vec<(Value, Value)> = m.borrow().entries.to_vec();
            let out = empty_map();
            for (k, v) in entries {
                let r = _v.call_value(
                    &args[0],
                    vec![k.clone(), v],
                    std::collections::HashMap::new(),
                )?;
                map_put(&out, k, r);
            }
            Ok(out)
        }),
    );
    bc.methods.insert(
        "update".into(),
        h::method(|_v, t, args, named| {
            h::require_args(&args, 2, "update")?;
            let m = h::as_map(&t).ok_or_else(|| h::err("Not a map"))?;
            let key = args[0].clone();
            let mut m = m.borrow_mut();
            let i = m.entries.iter().position(|(k, _)| key_eq(k, &key));
            match i {
                Some(i) => {
                    let newv = _v.call_value(
                        &args[1],
                        vec![m.entries[i].1.clone()],
                        std::collections::HashMap::new(),
                    )?;
                    m.entries[i].1 = newv.clone();
                    Ok(newv)
                }
                None => match named.get("ifAbsent") {
                    Some(if_absent) => {
                        let newv =
                            _v.call_value(if_absent, vec![], std::collections::HashMap::new())?;
                        m.entries.push((key, newv.clone()));
                        Ok(newv)
                    }
                    None => Err(h::err("Key not found in map")),
                },
            }
        }),
    );
    bc.methods.insert(
        "updateAll".into(),
        h::method(|_v, t, args, _n| {
            h::require_args(&args, 1, "updateAll")?;
            let m = h::as_map(&t).ok_or_else(|| h::err("Not a map"))?;
            let entries: Vec<(Value, Value)> = m.borrow().entries.to_vec();
            for (k, v) in entries {
                let newv = _v.call_value(
                    &args[0],
                    vec![k.clone(), v],
                    std::collections::HashMap::new(),
                )?;
                map_put(&t, k, newv);
            }
            Ok(Value::Null)
        }),
    );
    bc.methods.insert(
        "toString".into(),
        h::method(|_v, t, _args, _n| {
            let m = h::as_map(&t).ok_or_else(|| h::err("Not a map"))?;
            let m = m.borrow();
            let inner: Vec<String> = m
                .entries
                .iter()
                .map(|(k, v)| format!("{}: {}", h::display(k), h::display(v)))
                .collect();
            Ok(h::str(format!("{{{}}}", inner.join(", "))))
        }),
    );
    bc.methods.insert(
        "[]".into(),
        h::method(|_v, t, args, _n| {
            h::require_args(&args, 1, "[]")?;
            let m = h::as_map(&t).ok_or_else(|| h::err("Not a map"))?;
            let m = m.borrow();
            Ok(m.entries
                .iter()
                .find(|(k, _)| key_eq(k, &args[0]))
                .map(|(_, v)| v.clone())
                .unwrap_or(Value::Null))
        }),
    );
    bc.methods.insert(
        "[]=".into(),
        h::method(|_v, t, args, _n| {
            h::require_args(&args, 2, "[]=")?;
            map_put(&t, args[0].clone(), args[1].clone());
            Ok(Value::Null)
        }),
    );
}

fn map_entry_value(key: Value, value: Value) -> Value {
    Value::Bridged(Rc::new(RefCell::new(crate::class::BridgedInstanceData {
        bridged_class: h::class("MapEntry"),
        native: Value::List(Rc::new(RefCell::new(vec![key, value]))),
        type_arguments: vec![],
        bridged_super_object: None,
    })))
}

fn map_entry_parts(v: &Value) -> Option<(Value, Value)> {
    let l = h::as_list(v)?;
    let l = l.borrow();
    Some((l.first()?.clone(), l.get(1)?.clone()))
}

// ---------------------------------------------------------------------------
// Set classes
// ---------------------------------------------------------------------------

fn set_class(name: &str, splay: bool) -> BridgedClassDef {
    let name = name.to_string();
    let mut bc = BridgedClassDef::new(&name);
    bc.native_type = Some(format!("d4rt::collection::{name}"));
    {
        let n = name.clone();
        bc.constructors.insert(
            "".into(),
            h::ctor(move |_v, _a, _n| Ok(wrap_cls(&n, empty_set()))),
        );
    }
    {
        let n = name.clone();
        let sp = splay;
        bc.static_methods.insert(
            "from".into(),
            h::static_method(move |_v, args, _n| {
                let s = empty_set();
                if let Some(src) = args.first() {
                    for item in h::as_list(src)
                        .ok_or_else(|| h::err("Set.from expects an Iterable"))?
                        .borrow()
                        .iter()
                    {
                        set_add(&s, item.clone());
                    }
                }
                if sp {
                    set_sort(&s);
                }
                Ok(wrap_cls(&n, s))
            }),
        );
    }
    {
        let n = name.clone();
        let sp = splay;
        bc.static_methods.insert(
            "of".into(),
            h::static_method(move |_v, args, _n| {
                let s = empty_set();
                if let Some(src) = args.first() {
                    for item in h::as_list(src)
                        .ok_or_else(|| h::err("Set.of expects an Iterable"))?
                        .borrow()
                        .iter()
                    {
                        set_add(&s, item.clone());
                    }
                }
                if sp {
                    set_sort(&s);
                }
                Ok(wrap_cls(&n, s))
            }),
        );
    }
    {
        let n = name.clone();
        bc.static_methods.insert(
            "identity".into(),
            h::static_method(move |_v, _a, _n| Ok(wrap_cls(&n, empty_set()))),
        );
    }
    set_common_methods(&mut bc, splay);
    bc
}

fn empty_set() -> Value {
    Value::Set(Rc::new(RefCell::new(DartSet { items: vec![] })))
}

fn set_find_index(s: &Value, item: &Value) -> Option<usize> {
    let s = h::as_set(s)?;
    let s = s.borrow();
    s.items.iter().position(|i| key_eq(i, item))
}

fn set_add(s: &Value, item: Value) {
    if set_find_index(s, &item).is_none() {
        h::as_set(s).unwrap().borrow_mut().items.push(item);
    }
}

fn set_sort(s: &Value) {
    let items = h::as_set(s).unwrap().borrow().items.clone();
    let mut sorted = items;
    sorted.sort_by(key_cmp);
    h::as_set(s).unwrap().borrow_mut().items = sorted;
}

fn key_cmp(a: &Value, b: &Value) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    match (h::as_double(a), h::as_double(b)) {
        (Some(x), Some(y)) => x.partial_cmp(&y).unwrap_or(Ordering::Equal),
        _ => h::display(a).cmp(&h::display(b)),
    }
}

fn set_common_methods(bc: &mut BridgedClassDef, splay: bool) {
    bc.getters.insert(
        "length".into(),
        h::getter(|_v, t| {
            let s = h::as_set(&t).ok_or_else(|| h::err("Not a set"))?;
            Ok(Value::Int(s.borrow().items.len() as i64))
        }),
    );
    bc.getters.insert(
        "isEmpty".into(),
        h::getter(|_v, t| {
            let s = h::as_set(&t).ok_or_else(|| h::err("Not a set"))?;
            Ok(Value::Bool(s.borrow().items.is_empty()))
        }),
    );
    bc.getters.insert(
        "isNotEmpty".into(),
        h::getter(|_v, t| {
            let s = h::as_set(&t).ok_or_else(|| h::err("Not a set"))?;
            Ok(Value::Bool(!s.borrow().items.is_empty()))
        }),
    );
    bc.getters.insert(
        "first".into(),
        h::getter(|_v, t| {
            let s = h::as_set(&t).ok_or_else(|| h::err("Not a set"))?;
            s.borrow()
                .items
                .first()
                .cloned()
                .ok_or_else(|| h::err("Bad state: No element"))
        }),
    );
    bc.getters.insert(
        "last".into(),
        h::getter(|_v, t| {
            let s = h::as_set(&t).ok_or_else(|| h::err("Not a set"))?;
            s.borrow()
                .items
                .last()
                .cloned()
                .ok_or_else(|| h::err("Bad state: No element"))
        }),
    );
    let sp = splay;
    bc.methods.insert(
        "add".into(),
        h::method(move |_v, t, args, _n| {
            h::require_args(&args, 1, "add")?;
            let added = set_find_index(&t, &args[0]).is_none();
            set_add(&t, args[0].clone());
            if sp {
                set_sort(&t);
            }
            Ok(Value::Bool(added))
        }),
    );
    let sp = splay;
    bc.methods.insert(
        "addAll".into(),
        h::method(move |_v, t, args, _n| {
            h::require_args(&args, 1, "addAll")?;
            let src = h::as_list(&args[0]).ok_or_else(|| h::err("addAll expects an Iterable"))?;
            for item in src.borrow().iter() {
                set_add(&t, item.clone());
            }
            if sp {
                set_sort(&t);
            }
            Ok(Value::Null)
        }),
    );
    bc.methods.insert(
        "remove".into(),
        h::method(|_v, t, args, _n| {
            h::require_args(&args, 1, "remove")?;
            let s = h::as_set(&t).ok_or_else(|| h::err("Not a set"))?;
            let mut s = s.borrow_mut();
            match s.items.iter().position(|i| key_eq(i, &args[0])) {
                Some(i) => {
                    s.items.remove(i);
                    Ok(Value::Bool(true))
                }
                None => Ok(Value::Bool(false)),
            }
        }),
    );
    bc.methods.insert(
        "removeWhere".into(),
        h::method(|_v, t, args, _n| {
            h::require_args(&args, 1, "removeWhere")?;
            let s = h::as_set(&t).ok_or_else(|| h::err("Not a set"))?;
            let items: Vec<Value> = s.borrow().items.clone();
            for item in items {
                let keep = _v.call_value(
                    &args[0],
                    vec![item.clone()],
                    std::collections::HashMap::new(),
                )?;
                if is_true(&keep) {
                    set_remove(&t, &item);
                }
            }
            Ok(Value::Null)
        }),
    );
    bc.methods.insert(
        "retainWhere".into(),
        h::method(|_v, t, args, _n| {
            h::require_args(&args, 1, "retainWhere")?;
            let s = h::as_set(&t).ok_or_else(|| h::err("Not a set"))?;
            let items: Vec<Value> = s.borrow().items.clone();
            for item in items {
                let keep = _v.call_value(
                    &args[0],
                    vec![item.clone()],
                    std::collections::HashMap::new(),
                )?;
                if !is_true(&keep) {
                    set_remove(&t, &item);
                }
            }
            Ok(Value::Null)
        }),
    );
    bc.methods.insert(
        "clear".into(),
        h::method(|_v, t, _args, _n| {
            let s = h::as_set(&t).ok_or_else(|| h::err("Not a set"))?;
            s.borrow_mut().items.clear();
            Ok(Value::Null)
        }),
    );
    bc.methods.insert(
        "contains".into(),
        h::method(|_v, t, args, _n| {
            h::require_args(&args, 1, "contains")?;
            Ok(Value::Bool(set_find_index(&t, &args[0]).is_some()))
        }),
    );
    bc.methods.insert(
        "forEach".into(),
        h::method(|_v, t, args, _n| {
            h::require_args(&args, 1, "forEach")?;
            let s = h::as_set(&t).ok_or_else(|| h::err("Not a set"))?;
            let items: Vec<Value> = s.borrow().items.clone();
            for item in items {
                _v.call_value(&args[0], vec![item], std::collections::HashMap::new())?;
            }
            Ok(Value::Null)
        }),
    );
    bc.methods.insert(
        "union".into(),
        h::method(|_v, t, args, _n| {
            h::require_args(&args, 1, "union")?;
            let out = empty_set();
            let s = h::as_set(&t).ok_or_else(|| h::err("Not a set"))?;
            for item in s.borrow().items.iter() {
                set_add(&out, item.clone());
            }
            let other = h::as_set(&args[0]).ok_or_else(|| h::err("union expects a Set"))?;
            for item in other.borrow().items.iter() {
                set_add(&out, item.clone());
            }
            Ok(wrap_cls("HashSet", out))
        }),
    );
    bc.methods.insert(
        "intersection".into(),
        h::method(|_v, t, args, _n| {
            h::require_args(&args, 1, "intersection")?;
            let out = empty_set();
            let s = h::as_set(&t).ok_or_else(|| h::err("Not a set"))?;
            for item in s.borrow().items.iter() {
                if set_find_index(&args[0], item).is_some() {
                    set_add(&out, item.clone());
                }
            }
            Ok(wrap_cls("HashSet", out))
        }),
    );
    bc.methods.insert(
        "difference".into(),
        h::method(|_v, t, args, _n| {
            h::require_args(&args, 1, "difference")?;
            let out = empty_set();
            let s = h::as_set(&t).ok_or_else(|| h::err("Not a set"))?;
            for item in s.borrow().items.iter() {
                if set_find_index(&args[0], item).is_none() {
                    set_add(&out, item.clone());
                }
            }
            Ok(wrap_cls("HashSet", out))
        }),
    );
    bc.methods.insert(
        "lookup".into(),
        h::method(|_v, t, args, _n| {
            h::require_args(&args, 1, "lookup")?;
            let s = h::as_set(&t).ok_or_else(|| h::err("Not a set"))?;
            let s = s.borrow();
            Ok(s.items
                .iter()
                .find(|i| key_eq(i, &args[0]))
                .cloned()
                .unwrap_or(Value::Null))
        }),
    );
    bc.methods.insert(
        "toString".into(),
        h::method(|_v, t, _args, _n| {
            let s = h::as_set(&t).ok_or_else(|| h::err("Not a set"))?;
            let s = s.borrow();
            let inner: Vec<String> = s.items.iter().map(h::display).collect();
            Ok(h::str(format!("{{{}}}", inner.join(", "))))
        }),
    );
}

fn set_remove(s: &Value, item: &Value) {
    if let Some(i) = set_find_index(s, item) {
        h::as_set(s).unwrap().borrow_mut().items.remove(i);
    }
}

fn is_true(v: &Value) -> bool {
    crate::value::is_truthy(v).unwrap_or(false)
}

// ---------------------------------------------------------------------------
// Queue classes
// ---------------------------------------------------------------------------

fn list_queue_definition() -> BridgedClassDef {
    let mut bc = BridgedClassDef::new("ListQueue");
    bc.native_type = Some("d4rt::collection::ListQueue".to_string());
    bc.constructors.insert(
        "".into(),
        h::ctor(|_v, _a, _n| Ok(wrap_cls("ListQueue", empty_list()))),
    );
    bc.static_methods.insert(
        "from".into(),
        h::static_method(|_v, args, _n| {
            let l = empty_list();
            if let Some(src) = args.first() {
                let src =
                    h::as_list(src).ok_or_else(|| h::err("ListQueue.from expects an Iterable"))?;
                h::as_list(&l)
                    .unwrap()
                    .borrow_mut()
                    .extend(src.borrow().iter().cloned());
            }
            Ok(wrap_cls("ListQueue", l))
        }),
    );
    bc.static_methods.insert(
        "of".into(),
        h::static_method(|_v, args, _n| {
            let l = empty_list();
            if let Some(src) = args.first() {
                let src =
                    h::as_list(src).ok_or_else(|| h::err("ListQueue.of expects an Iterable"))?;
                h::as_list(&l)
                    .unwrap()
                    .borrow_mut()
                    .extend(src.borrow().iter().cloned());
            }
            Ok(wrap_cls("ListQueue", l))
        }),
    );
    queue_common_methods(&mut bc);
    bc
}

fn queue_definition() -> BridgedClassDef {
    let mut bc = BridgedClassDef::new("Queue");
    bc.native_type = Some("d4rt::collection::Queue".to_string());
    bc.constructors.insert(
        "".into(),
        h::ctor(|_v, _a, _n| Ok(wrap_cls("Queue", empty_list()))),
    );
    bc.static_methods.insert(
        "from".into(),
        h::static_method(|_v, args, _n| {
            let l = empty_list();
            if let Some(src) = args.first() {
                let src =
                    h::as_list(src).ok_or_else(|| h::err("Queue.from expects an Iterable"))?;
                h::as_list(&l)
                    .unwrap()
                    .borrow_mut()
                    .extend(src.borrow().iter().cloned());
            }
            Ok(wrap_cls("Queue", l))
        }),
    );
    bc.static_methods.insert(
        "of".into(),
        h::static_method(|_v, args, _n| {
            let l = empty_list();
            if let Some(src) = args.first() {
                let src = h::as_list(src).ok_or_else(|| h::err("Queue.of expects an Iterable"))?;
                h::as_list(&l)
                    .unwrap()
                    .borrow_mut()
                    .extend(src.borrow().iter().cloned());
            }
            Ok(wrap_cls("Queue", l))
        }),
    );
    queue_common_methods(&mut bc);
    bc
}

fn empty_list() -> Value {
    Value::List(Rc::new(RefCell::new(vec![])))
}

fn queue_common_methods(bc: &mut BridgedClassDef) {
    bc.getters.insert(
        "length".into(),
        h::getter(|_v, t| {
            let l = h::as_list(&t).ok_or_else(|| h::err("Not a queue"))?;
            Ok(Value::Int(l.borrow().len() as i64))
        }),
    );
    bc.getters.insert(
        "isEmpty".into(),
        h::getter(|_v, t| {
            let l = h::as_list(&t).ok_or_else(|| h::err("Not a queue"))?;
            Ok(Value::Bool(l.borrow().is_empty()))
        }),
    );
    bc.getters.insert(
        "isNotEmpty".into(),
        h::getter(|_v, t| {
            let l = h::as_list(&t).ok_or_else(|| h::err("Not a queue"))?;
            Ok(Value::Bool(!l.borrow().is_empty()))
        }),
    );
    bc.getters.insert(
        "first".into(),
        h::getter(|_v, t| {
            let l = h::as_list(&t).ok_or_else(|| h::err("Not a queue"))?;
            l.borrow()
                .first()
                .cloned()
                .ok_or_else(|| h::err("Bad state: No element"))
        }),
    );
    bc.getters.insert(
        "last".into(),
        h::getter(|_v, t| {
            let l = h::as_list(&t).ok_or_else(|| h::err("Not a queue"))?;
            l.borrow()
                .last()
                .cloned()
                .ok_or_else(|| h::err("Bad state: No element"))
        }),
    );
    bc.methods.insert(
        "add".into(),
        h::method(|_v, t, args, _n| {
            h::require_args(&args, 1, "add")?;
            h::as_list(&t)
                .ok_or_else(|| h::err("Not a queue"))?
                .borrow_mut()
                .push(args[0].clone());
            Ok(Value::Null)
        }),
    );
    bc.methods.insert(
        "addAll".into(),
        h::method(|_v, t, args, _n| {
            h::require_args(&args, 1, "addAll")?;
            let src = h::as_list(&args[0]).ok_or_else(|| h::err("addAll expects an Iterable"))?;
            h::as_list(&t)
                .ok_or_else(|| h::err("Not a queue"))?
                .borrow_mut()
                .extend(src.borrow().iter().cloned());
            Ok(Value::Null)
        }),
    );
    bc.methods.insert(
        "addFirst".into(),
        h::method(|_v, t, args, _n| {
            h::require_args(&args, 1, "addFirst")?;
            h::as_list(&t)
                .ok_or_else(|| h::err("Not a queue"))?
                .borrow_mut()
                .insert(0, args[0].clone());
            Ok(Value::Null)
        }),
    );
    bc.methods.insert(
        "addLast".into(),
        h::method(|_v, t, args, _n| {
            h::require_args(&args, 1, "addLast")?;
            h::as_list(&t)
                .ok_or_else(|| h::err("Not a queue"))?
                .borrow_mut()
                .push(args[0].clone());
            Ok(Value::Null)
        }),
    );
    bc.methods.insert(
        "removeFirst".into(),
        h::method(|_v, t, _args, _n| {
            let l = h::as_list(&t).ok_or_else(|| h::err("Not a queue"))?;
            let mut l = l.borrow_mut();
            if l.is_empty() {
                return Err(h::err("Bad state: No element"));
            }
            Ok(l.remove(0))
        }),
    );
    bc.methods.insert(
        "removeLast".into(),
        h::method(|_v, t, _args, _n| {
            let l = h::as_list(&t).ok_or_else(|| h::err("Not a queue"))?;
            let mut l = l.borrow_mut();
            l.pop().ok_or_else(|| h::err("Bad state: No element"))
        }),
    );
    bc.methods.insert(
        "removeWhere".into(),
        h::method(|_v, t, args, _n| {
            h::require_args(&args, 1, "removeWhere")?;
            let l = h::as_list(&t).ok_or_else(|| h::err("Not a queue"))?;
            let items: Vec<Value> = l.borrow().clone();
            for item in items {
                let keep = _v.call_value(
                    &args[0],
                    vec![item.clone()],
                    std::collections::HashMap::new(),
                )?;
                if !is_true(&keep) {
                    queue_remove_item(&t, &item);
                }
            }
            Ok(Value::Null)
        }),
    );
    bc.methods.insert(
        "retainWhere".into(),
        h::method(|_v, t, args, _n| {
            h::require_args(&args, 1, "retainWhere")?;
            let l = h::as_list(&t).ok_or_else(|| h::err("Not a queue"))?;
            let items: Vec<Value> = l.borrow().clone();
            for item in items {
                let keep = _v.call_value(
                    &args[0],
                    vec![item.clone()],
                    std::collections::HashMap::new(),
                )?;
                if !is_true(&keep) {
                    queue_remove_item(&t, &item);
                }
            }
            Ok(Value::Null)
        }),
    );
    bc.methods.insert(
        "clear".into(),
        h::method(|_v, t, _args, _n| {
            h::as_list(&t)
                .ok_or_else(|| h::err("Not a queue"))?
                .borrow_mut()
                .clear();
            Ok(Value::Null)
        }),
    );
    bc.methods.insert(
        "forEach".into(),
        h::method(|_v, t, args, _n| {
            h::require_args(&args, 1, "forEach")?;
            let l = h::as_list(&t).ok_or_else(|| h::err("Not a queue"))?;
            let items: Vec<Value> = l.borrow().clone();
            for item in items {
                _v.call_value(&args[0], vec![item], std::collections::HashMap::new())?;
            }
            Ok(Value::Null)
        }),
    );
    bc.methods.insert(
        "toList".into(),
        h::method(|_v, t, _args, _n| {
            let l = h::as_list(&t).ok_or_else(|| h::err("Not a queue"))?;
            Ok(Value::List(Rc::new(RefCell::new(l.borrow().clone()))))
        }),
    );
    bc.methods.insert(
        "toString".into(),
        h::method(|_v, t, _args, _n| {
            let l = h::as_list(&t).ok_or_else(|| h::err("Not a queue"))?;
            let inner: Vec<String> = l.borrow().iter().map(h::display).collect();
            Ok(h::str(format!("[{}]", inner.join(", "))))
        }),
    );
}

fn queue_remove_item(q: &Value, item: &Value) {
    let l = h::as_list(q).unwrap();
    let mut l = l.borrow_mut();
    if let Some(i) = l.iter().position(|x| crate::value::value_eq(x, item)) {
        l.remove(i);
    }
}

// ---------------------------------------------------------------------------
// UnmodifiableListView
// ---------------------------------------------------------------------------

fn unmodifiable_list_view_definition() -> BridgedClassDef {
    let mut bc = BridgedClassDef::new("UnmodifiableListView");
    bc.native_type = Some("d4rt::collection::UnmodifiableListView".to_string());
    bc.constructors.insert(
        "".into(),
        h::ctor(|_v, args, _n| {
            h::require_args(&args, 1, "UnmodifiableListView")?;
            let src = h::as_list(&args[0])
                .ok_or_else(|| h::err("UnmodifiableListView expects a List"))?;
            Ok(Value::Bridged(Rc::new(RefCell::new(
                crate::class::BridgedInstanceData {
                    bridged_class: h::class("UnmodifiableListView"),
                    native: Value::List(Rc::new(RefCell::new(src.borrow().clone()))),
                    type_arguments: vec![],
                    bridged_super_object: None,
                },
            ))))
        }),
    );
    bc.getters.insert(
        "length".into(),
        h::getter(|_v, t| {
            let l = h::as_list(&t).ok_or_else(|| h::err("Not a list"))?;
            Ok(Value::Int(l.borrow().len() as i64))
        }),
    );
    bc.getters.insert(
        "isEmpty".into(),
        h::getter(|_v, t| {
            let l = h::as_list(&t).ok_or_else(|| h::err("Not a list"))?;
            Ok(Value::Bool(l.borrow().is_empty()))
        }),
    );
    bc.methods.insert(
        "[]".into(),
        h::method(|_v, t, args, _n| {
            h::require_args(&args, 1, "[]")?;
            let idx = h::arg_int(&args, 0, "[]")?;
            let l = h::as_list(&t).ok_or_else(|| h::err("Not a list"))?;
            let l = l.borrow();
            l.get(idx as usize)
                .cloned()
                .ok_or_else(|| h::err("Index out of range"))
        }),
    );
    bc.methods.insert(
        "indexOf".into(),
        h::method(|_v, t, args, _n| {
            h::require_args(&args, 1, "indexOf")?;
            let l = h::as_list(&t).ok_or_else(|| h::err("Not a list"))?;
            let l = l.borrow();
            let mut start = 0;
            if let Some(s) = args.get(1).and_then(h::as_int) {
                start = s.max(0) as usize;
            }
            for (i, item) in l.iter().enumerate().skip(start) {
                if crate::value::value_eq(item, &args[0]) {
                    return Ok(Value::Int(i as i64));
                }
            }
            Ok(Value::Int(-1))
        }),
    );
    bc.methods.insert(
        "contains".into(),
        h::method(|_v, t, args, _n| {
            h::require_args(&args, 1, "contains")?;
            let l = h::as_list(&t).ok_or_else(|| h::err("Not a list"))?;
            Ok(Value::Bool(
                l.borrow()
                    .iter()
                    .any(|x| crate::value::value_eq(x, &args[0])),
            ))
        }),
    );
    bc.methods.insert(
        "toList".into(),
        h::method(|_v, t, _args, _n| {
            let l = h::as_list(&t).ok_or_else(|| h::err("Not a list"))?;
            Ok(Value::List(Rc::new(RefCell::new(l.borrow().clone()))))
        }),
    );
    bc.methods.insert(
        "toString".into(),
        h::method(|_v, t, _args, _n| {
            let l = h::as_list(&t).ok_or_else(|| h::err("Not a list"))?;
            let inner: Vec<String> = l.borrow().iter().map(h::display).collect();
            Ok(h::str(format!("[{}]", inner.join(", "))))
        }),
    );
    bc
}

// ---------------------------------------------------------------------------
// LinkedList / LinkedListEntry
// ---------------------------------------------------------------------------

// A LinkedListEntry is a bridged instance whose native value is a 4-cell list:
//   [0] = the wrapped value
//   [1] = the LinkedList it belongs to (or Null)
//   [2] = the previous entry (or Null)
//   [3] = the next entry (or Null)
// A LinkedList is a bridged instance whose native value is the list of entries.
const E_VALUE: usize = 0;
const E_LIST: usize = 1;
const E_PREV: usize = 2;
const E_NEXT: usize = 3;

fn entry_cells(e: &Value) -> Result<Rc<RefCell<Vec<Value>>>, InterpError> {
    h::as_list(e).ok_or_else(|| h::err("Not a LinkedListEntry"))
}

fn entry_cell(e: &Value, i: usize) -> Value {
    entry_cells(e)
        .map(|c| c.borrow().get(i).cloned().unwrap_or(Value::Null))
        .unwrap_or(Value::Null)
}

fn set_entry_cell(e: &Value, i: usize, v: Value) {
    if let Ok(c) = entry_cells(e) {
        let mut c = c.borrow_mut();
        while c.len() <= i {
            c.push(Value::Null);
        }
        c[i] = v;
    }
}

fn is_null(v: &Value) -> bool {
    matches!(v, Value::Null)
}

/// Removes `entry` from its list, updating the neighbours. Assumes the entry
/// is currently in a list; returns the entry's list.
fn unlink_entry(entry: &Value) -> Result<Value, InterpError> {
    let cells = entry_cells(entry)?;
    let list = cells.borrow().get(E_LIST).cloned().unwrap_or(Value::Null);
    if is_null(&list) {
        return Err(h::err(
            "Cannot unlink an entry that is not in a list or already unlinked.",
        ));
    }
    let prev = entry_cell(entry, E_PREV);
    let next = entry_cell(entry, E_NEXT);
    if !is_null(&prev) {
        set_entry_cell(&prev, E_NEXT, next.clone());
    }
    if !is_null(&next) {
        set_entry_cell(&next, E_PREV, prev.clone());
    }
    let l = h::as_list(&list).ok_or_else(|| h::err("LinkedList state corrupt"))?;
    let mut l = l.borrow_mut();
    if let Some(i) = l.iter().position(|x| crate::value::value_eq(x, entry)) {
        l.remove(i);
    }
    drop(l);
    set_entry_cell(entry, E_LIST, Value::Null);
    set_entry_cell(entry, E_PREV, Value::Null);
    set_entry_cell(entry, E_NEXT, Value::Null);
    Ok(list)
}

fn linked_list_definition() -> BridgedClassDef {
    let mut bc = BridgedClassDef::new("LinkedList");
    bc.native_type = Some("d4rt::collection::LinkedList".to_string());
    bc.constructors.insert(
        "".into(),
        h::ctor(|_v, args, named| {
            if !args.is_empty() || !named.is_empty() {
                return Err(h::err("Constructor LinkedList() does not take arguments."));
            }
            Ok(h::wrap(&h::class("LinkedList"), empty_list()))
        }),
    );
    bc.getters.insert(
        "length".into(),
        h::getter(|_v, t| {
            let l = h::as_list(&t).ok_or_else(|| h::err("Not a LinkedList"))?;
            Ok(Value::Int(l.borrow().len() as i64))
        }),
    );
    bc.getters.insert(
        "isEmpty".into(),
        h::getter(|_v, t| {
            let l = h::as_list(&t).ok_or_else(|| h::err("Not a LinkedList"))?;
            Ok(Value::Bool(l.borrow().is_empty()))
        }),
    );
    bc.getters.insert(
        "isNotEmpty".into(),
        h::getter(|_v, t| {
            let l = h::as_list(&t).ok_or_else(|| h::err("Not a LinkedList"))?;
            Ok(Value::Bool(!l.borrow().is_empty()))
        }),
    );
    bc.getters.insert(
        "first".into(),
        h::getter(|_v, t| {
            let l = h::as_list(&t).ok_or_else(|| h::err("Not a LinkedList"))?;
            l.borrow()
                .first()
                .cloned()
                .ok_or_else(|| h::err("Cannot get first from an empty LinkedList."))
        }),
    );
    bc.getters.insert(
        "last".into(),
        h::getter(|_v, t| {
            let l = h::as_list(&t).ok_or_else(|| h::err("Not a LinkedList"))?;
            l.borrow()
                .last()
                .cloned()
                .ok_or_else(|| h::err("Cannot get last from an empty LinkedList."))
        }),
    );
    bc.methods.insert(
        "add".into(),
        h::method(|_v, t, args, _n| {
            h::require_args(&args, 1, "add")?;
            let entry = &args[0];
            if !is_null(&entry_cell(entry, E_LIST)) {
                return Err(h::err("LinkedListEntry is already in a LinkedList"));
            }
            let l = h::as_list(&t).ok_or_else(|| h::err("Not a LinkedList"))?;
            let last = l.borrow().last().cloned();
            if let Some(last) = &last {
                set_entry_cell(last, E_NEXT, entry.clone());
            }
            set_entry_cell(entry, E_PREV, last.unwrap_or(Value::Null));
            set_entry_cell(entry, E_NEXT, Value::Null);
            set_entry_cell(entry, E_LIST, t.clone());
            l.borrow_mut().push(entry.clone());
            Ok(Value::Null)
        }),
    );
    bc.methods.insert(
        "remove".into(),
        h::method(|_v, t, args, _n| {
            h::require_args(&args, 1, "remove")?;
            let entry = &args[0];
            let in_this_list = {
                let list = entry_cell(entry, E_LIST);
                !is_null(&list) && crate::value::value_eq(&list, &t)
            };
            if !in_this_list {
                return Ok(Value::Bool(false));
            }
            unlink_entry(entry)?;
            Ok(Value::Bool(true))
        }),
    );
    bc.methods.insert(
        "clear".into(),
        h::method(|_v, t, _args, _n| {
            let l = h::as_list(&t).ok_or_else(|| h::err("Not a LinkedList"))?;
            let entries: Vec<Value> = l.borrow().clone();
            for e in &entries {
                set_entry_cell(e, E_LIST, Value::Null);
                set_entry_cell(e, E_PREV, Value::Null);
                set_entry_cell(e, E_NEXT, Value::Null);
            }
            l.borrow_mut().clear();
            Ok(Value::Null)
        }),
    );
    bc.methods.insert(
        "removeFirst".into(),
        h::method(|_v, t, _args, _n| {
            let l = h::as_list(&t).ok_or_else(|| h::err("Not a LinkedList"))?;
            let first = l
                .borrow()
                .first()
                .cloned()
                .ok_or_else(|| h::err("Cannot removeFirst from an empty LinkedList."))?;
            unlink_entry(&first)?;
            Ok(first)
        }),
    );
    bc
}

fn linked_list_entry_definition() -> BridgedClassDef {
    let mut bc = BridgedClassDef::new("LinkedListEntry");
    bc.native_type = Some("d4rt::collection::LinkedListEntry".to_string());
    bc.constructors.insert(
        "".into(),
        h::ctor(|_v, args, _n| {
            if args.len() != 1 {
                return Err(h::err(
                    "Constructor LinkedListEntry(value) expects one positional argument.",
                ));
            }
            Ok(h::wrap(
                &h::class("LinkedListEntry"),
                Value::List(Rc::new(RefCell::new(vec![
                    args[0].clone(),
                    Value::Null,
                    Value::Null,
                    Value::Null,
                ]))),
            ))
        }),
    );
    bc.getters.insert(
        "value".into(),
        h::getter(|_v, t| Ok(entry_cell(&t, E_VALUE))),
    );
    bc.getters
        .insert("list".into(), h::getter(|_v, t| Ok(entry_cell(&t, E_LIST))));
    bc.getters.insert(
        "previous".into(),
        h::getter(|_v, t| Ok(entry_cell(&t, E_PREV))),
    );
    bc.getters
        .insert("next".into(), h::getter(|_v, t| Ok(entry_cell(&t, E_NEXT))));
    bc.methods.insert(
        "unlink".into(),
        h::method(|_v, t, _args, _n| {
            unlink_entry(&t)?;
            Ok(Value::Null)
        }),
    );
    bc
}

// ---------------------------------------------------------------------------
// Public constructors
// ---------------------------------------------------------------------------

pub fn hash_map_definition() -> BridgedClassDef {
    map_class("HashMap", false)
}

pub fn linked_hash_map_definition() -> BridgedClassDef {
    map_class("LinkedHashMap", false)
}

pub fn splay_tree_map_definition() -> BridgedClassDef {
    map_class("SplayTreeMap", true)
}

pub fn hash_set_definition() -> BridgedClassDef {
    set_class("HashSet", false)
}

pub fn linked_hash_set_definition() -> BridgedClassDef {
    set_class("LinkedHashSet", false)
}

pub fn splay_tree_set_definition() -> BridgedClassDef {
    set_class("SplayTreeSet", true)
}

pub fn list_queue_definition_pub() -> BridgedClassDef {
    list_queue_definition()
}

pub fn linked_list_definition_pub() -> BridgedClassDef {
    linked_list_definition()
}

pub fn linked_list_entry_definition_pub() -> BridgedClassDef {
    linked_list_entry_definition()
}
