use super::super::helpers as h;
use super::string::iterable_items;
use crate::class::BridgedClassDef;
use crate::error::InterpError;
use crate::interp::InterpreterVisitor;
use crate::value::{DartMap, DartSet, Value, value_display, value_eq};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

type M = Box<
    dyn Fn(
            &mut InterpreterVisitor,
            Value,
            Vec<Value>,
            HashMap<String, Value>,
        ) -> Result<Value, InterpError>
        + Send
        + Sync,
>;
type G = Box<dyn Fn(&mut InterpreterVisitor, Value) -> Result<Value, InterpError> + Send + Sync>;
#[allow(dead_code)]
type S =
    Box<dyn Fn(&mut InterpreterVisitor, Value, Value) -> Result<(), InterpError> + Send + Sync>;
type SM = Box<
    dyn Fn(
            &mut InterpreterVisitor,
            Vec<Value>,
            HashMap<String, Value>,
        ) -> Result<Value, InterpError>
        + Send
        + Sync,
>;
#[allow(dead_code)]
type SG = Box<dyn Fn(&mut InterpreterVisitor) -> Result<Value, InterpError> + Send + Sync>;
type C = Box<
    dyn Fn(
            &mut InterpreterVisitor,
            Vec<Value>,
            HashMap<String, Value>,
        ) -> Result<Value, InterpError>
        + Send
        + Sync,
>;

fn method<F>(f: F) -> M
where
    F: Fn(
            &mut InterpreterVisitor,
            Value,
            Vec<Value>,
            HashMap<String, Value>,
        ) -> Result<Value, InterpError>
        + Send
        + Sync
        + 'static,
{
    Box::new(f)
}

fn getter<F>(f: F) -> G
where
    F: Fn(&mut InterpreterVisitor, Value) -> Result<Value, InterpError> + Send + Sync + 'static,
{
    Box::new(f)
}

#[allow(dead_code)]
fn setter<F>(f: F) -> S
where
    F: Fn(&mut InterpreterVisitor, Value, Value) -> Result<(), InterpError> + Send + Sync + 'static,
{
    Box::new(f)
}

fn static_method<F>(f: F) -> SM
where
    F: Fn(
            &mut InterpreterVisitor,
            Vec<Value>,
            HashMap<String, Value>,
        ) -> Result<Value, InterpError>
        + Send
        + Sync
        + 'static,
{
    Box::new(f)
}

#[allow(dead_code)]
fn static_getter<F>(f: F) -> SG
where
    F: Fn(&mut InterpreterVisitor) -> Result<Value, InterpError> + Send + Sync + 'static,
{
    Box::new(f)
}

fn ctor<F>(f: F) -> C
where
    F: Fn(
            &mut InterpreterVisitor,
            Vec<Value>,
            HashMap<String, Value>,
        ) -> Result<Value, InterpError>
        + Send
        + Sync
        + 'static,
{
    Box::new(f)
}

pub fn list_value(items: Vec<Value>) -> Value {
    Value::List(Rc::new(RefCell::new(items)))
}

fn list_of_list(target: &Value) -> Result<Rc<RefCell<Vec<Value>>>, InterpError> {
    h::as_list(target).ok_or_else(|| h::err("Expected a List"))
}

pub fn list_definition() -> BridgedClassDef {
    let mut bc = BridgedClassDef::new("List");
    bc.native_type = Some("List".into());
    bc.static_methods.insert(
        "castFrom".into(),
        static_method(|_v, args, _n| {
            let src = h::as_list(
                args.first()
                    .ok_or_else(|| h::err("castFrom expects a List"))?,
            )
            .ok_or_else(|| h::err("castFrom expects a List"))?;
            Ok(list_value(src.borrow().clone()))
        }),
    );
    bc.static_methods.insert(
        "from".into(),
        static_method(|v, args, _n| {
            let src = args
                .first()
                .ok_or_else(|| h::err("List.from expects an Iterable"))?;
            Ok(list_value(iterable_items(src, v)?))
        }),
    );
    bc.static_methods.insert(
        "empty".into(),
        static_method(|_v, _a, _n| Ok(list_value(vec![]))),
    );
    bc.static_methods.insert(
        "generate".into(),
        static_method(|_v, args, _n| {
            let count = h::arg_int(&args, 0, "generate")?;
            let generator = args.get(1).cloned();
            let mut out = Vec::new();
            for i in 0..count {
                match &generator {
                    Some(f) => out.push(h::call_closure(_v, f, vec![Value::Int(i)])?),
                    None => out.push(Value::Null),
                }
            }
            Ok(list_value(out))
        }),
    );
    bc.static_methods.insert(
        "copyRange".into(),
        static_method(|_v, args, _n| {
            let target = args
                .first()
                .ok_or_else(|| h::err("copyRange expects a target List"))?;
            let at = h::arg_int(&args, 1, "copyRange")?;
            let source = args
                .get(2)
                .ok_or_else(|| h::err("copyRange expects a source List"))?;
            let start = args.get(3).and_then(h::as_int).unwrap_or(0);
            let end = args.get(4).and_then(h::as_int);
            let t = list_of_list(target)?;
            let s = h::as_list(source).ok_or_else(|| h::err("copyRange expects a source List"))?;
            let s_items = s.borrow();
            let end = end.map(|e| e as usize).unwrap_or(s_items.len());
            let mut t_items = t.borrow_mut();
            for (ti, item) in
                (at.max(0) as usize..).zip(s_items.iter().take(end).skip(start.max(0) as usize))
            {
                if ti >= t_items.len() {
                    t_items.push(item.clone());
                } else {
                    t_items[ti] = item.clone();
                }
            }
            Ok(Value::Null)
        }),
    );
    bc.static_methods.insert(
        "filled".into(),
        static_method(|_v, args, _n| {
            let count = h::arg_int(&args, 0, "filled")?;
            let fill = args.get(1).cloned().unwrap_or(Value::Null);
            Ok(list_value(vec![fill; count.max(0) as usize]))
        }),
    );
    bc.static_methods.insert(
        "identity".into(),
        static_method(|_v, args, _n| {
            let count = h::arg_int(&args, 0, "identity")?;
            Ok(list_value(vec![Value::Null; count.max(0) as usize]))
        }),
    );
    bc.static_methods.insert(
        "of".into(),
        static_method(|v, args, _n| {
            let src = args
                .first()
                .ok_or_else(|| h::err("List.of expects an Iterable"))?;
            Ok(list_value(iterable_items(src, v)?))
        }),
    );
    bc.static_methods.insert(
        "unmodifiable".into(),
        static_method(|v, args, _n| {
            let src = args
                .first()
                .ok_or_else(|| h::err("List.unmodifiable expects an Iterable"))?;
            Ok(list_value(iterable_items(src, v)?))
        }),
    );
    bc.static_methods.insert(
        "writeIterable".into(),
        static_method(|v, args, _n| {
            let target = args
                .first()
                .ok_or_else(|| h::err("writeIterable expects a target List"))?;
            let at = h::arg_int(&args, 1, "writeIterable")?;
            let source = args
                .get(2)
                .ok_or_else(|| h::err("writeIterable expects an Iterable"))?;
            let items = iterable_items(source, v)?;
            let t = list_of_list(target)?;
            let mut t_items = t.borrow_mut();
            for (ti, item) in (at.max(0) as usize..).zip(items) {
                if ti >= t_items.len() {
                    t_items.push(item);
                } else {
                    t_items[ti] = item;
                }
            }
            Ok(Value::Null)
        }),
    );
    bc.methods.insert(
        "[]".into(),
        method(|_v, t, args, _n| {
            let i = h::arg_int(&args, 0, "[]")?;
            let l = list_of_list(&t)?;
            let items = l.borrow();
            if i < 0 || i as usize >= items.len() {
                return Err(h::err(format!(
                    "Index out of range: {i} (length {})",
                    items.len()
                )));
            }
            Ok(items[i as usize].clone())
        }),
    );
    bc.methods.insert(
        "[]=".into(),
        method(|_v, t, args, _n| {
            let i = h::arg_int(&args, 0, "[]=")?;
            let v = args.get(1).cloned().unwrap_or(Value::Null);
            let l = list_of_list(&t)?;
            let mut items = l.borrow_mut();
            if i < 0 || i as usize >= items.len() {
                return Err(h::err(format!(
                    "Index out of range: {i} (length {})",
                    items.len()
                )));
            }
            items[i as usize] = v;
            Ok(Value::Null)
        }),
    );
    bc.methods.insert(
        "add".into(),
        method(|_v, t, args, _n| {
            let v = args.first().cloned().unwrap_or(Value::Null);
            list_of_list(&t)?.borrow_mut().push(v);
            Ok(Value::Null)
        }),
    );
    bc.methods.insert(
        "addAll".into(),
        method(|v, t, args, _n| {
            let src = args
                .first()
                .ok_or_else(|| h::err("addAll expects an Iterable"))?;
            let items = iterable_items(src, v)?;
            list_of_list(&t)?.borrow_mut().extend(items);
            Ok(Value::Null)
        }),
    );
    bc.methods.insert(
        "remove".into(),
        method(|_v, t, args, _n| {
            let needle = args.first().cloned().unwrap_or(Value::Null);
            let l = list_of_list(&t)?;
            let mut items = l.borrow_mut();
            match items.iter().position(|i| value_eq(i, &needle)) {
                Some(idx) => {
                    items.remove(idx);
                    Ok(Value::Bool(true))
                }
                None => Ok(Value::Bool(false)),
            }
        }),
    );
    bc.methods.insert(
        "removeAt".into(),
        method(|_v, t, args, _n| {
            let i = h::arg_int(&args, 0, "removeAt")?;
            let l = list_of_list(&t)?;
            let mut items = l.borrow_mut();
            if i < 0 || i as usize >= items.len() {
                return Err(h::err(format!("removeAt: index out of range {i}")));
            }
            Ok(items.remove(i as usize))
        }),
    );
    bc.methods.insert(
        "removeLast".into(),
        method(|_v, t, _a, _n| {
            let l = list_of_list(&t)?;
            let mut items = l.borrow_mut();
            items.pop().ok_or_else(|| h::err("removeLast: empty list"))
        }),
    );
    bc.methods.insert(
        "clear".into(),
        method(|_v, t, _a, _n| {
            list_of_list(&t)?.borrow_mut().clear();
            Ok(Value::Null)
        }),
    );
    bc.methods.insert(
        "contains".into(),
        method(|_v, t, args, _n| {
            let needle = args.first().cloned().unwrap_or(Value::Null);
            let l = list_of_list(&t)?;
            Ok(Value::Bool(l.borrow().iter().any(|i| value_eq(i, &needle))))
        }),
    );
    bc.methods.insert(
        "indexOf".into(),
        method(|_v, t, args, _n| {
            let needle = args.first().cloned().unwrap_or(Value::Null);
            let start = args.get(1).and_then(h::as_int).unwrap_or(0);
            let l = list_of_list(&t)?;
            let items = l.borrow();
            let idx = items
                .iter()
                .enumerate()
                .skip(start.max(0) as usize)
                .find(|(_, i)| value_eq(i, &needle))
                .map(|(i, _)| i as i64)
                .unwrap_or(-1);
            Ok(Value::Int(idx))
        }),
    );
    bc.methods.insert(
        "lastIndexOf".into(),
        method(|_v, t, args, _n| {
            let needle = args.first().cloned().unwrap_or(Value::Null);
            let end = args.get(1).and_then(h::as_int);
            let l = list_of_list(&t)?;
            let items = l.borrow();
            let limit = end
                .map(|e| (e.max(0) as usize).min(items.len()))
                .unwrap_or(items.len());
            let idx = items[..limit]
                .iter()
                .rposition(|i| value_eq(i, &needle))
                .map(|i| i as i64)
                .unwrap_or(-1);
            Ok(Value::Int(idx))
        }),
    );
    bc.methods.insert(
        "sublist".into(),
        method(|_v, t, args, _n| {
            let start = h::arg_int(&args, 0, "sublist")?;
            let end = args.get(1).and_then(h::as_int);
            let l = list_of_list(&t)?;
            let items = l.borrow();
            let s = (start.max(0) as usize).min(items.len());
            let e = end
                .map(|e| (e.max(0) as usize).min(items.len()))
                .unwrap_or(items.len());
            if s > e {
                return Err(h::err("sublist: invalid range"));
            }
            Ok(list_value(items[s..e].to_vec()))
        }),
    );
    bc.methods.insert(
        "forEach".into(),
        method(|_v, t, args, _n| {
            let f = args
                .first()
                .cloned()
                .ok_or_else(|| h::err("forEach expects a function"))?;
            let items = list_of_list(&t)?.borrow().clone();
            for item in items {
                h::call_closure(_v, &f, vec![item])?;
            }
            Ok(Value::Null)
        }),
    );
    bc.methods.insert(
        "any".into(),
        method(|_v, t, args, _n| {
            let f = args
                .first()
                .cloned()
                .ok_or_else(|| h::err("any expects a function"))?;
            let items = list_of_list(&t)?.borrow().clone();
            for item in items {
                let r = h::call_closure(_v, &f, vec![item])?;
                if h::as_bool(&r).unwrap_or(false) {
                    return Ok(Value::Bool(true));
                }
            }
            Ok(Value::Bool(false))
        }),
    );
    bc.methods.insert(
        "every".into(),
        method(|_v, t, args, _n| {
            let f = args
                .first()
                .cloned()
                .ok_or_else(|| h::err("every expects a function"))?;
            let items = list_of_list(&t)?.borrow().clone();
            for item in items {
                let r = h::call_closure(_v, &f, vec![item])?;
                if !h::as_bool(&r).unwrap_or(false) {
                    return Ok(Value::Bool(false));
                }
            }
            Ok(Value::Bool(true))
        }),
    );
    bc.methods.insert(
        "map".into(),
        method(|_v, t, args, _n| {
            let f = args
                .first()
                .cloned()
                .ok_or_else(|| h::err("map expects a function"))?;
            let items = list_of_list(&t)?.borrow().clone();
            let mut out = Vec::new();
            for item in items {
                out.push(h::call_closure(_v, &f, vec![item])?);
            }
            Ok(list_value(out))
        }),
    );
    bc.methods.insert(
        "indexWhere".into(),
        method(|_v, t, args, _n| {
            let f = args
                .first()
                .cloned()
                .ok_or_else(|| h::err("indexWhere expects a function"))?;
            let start = args.get(1).and_then(h::as_int).unwrap_or(0);
            let items = list_of_list(&t)?.borrow().clone();
            for (i, item) in items.iter().enumerate().skip(start.max(0) as usize) {
                let r = h::call_closure(_v, &f, vec![item.clone()])?;
                if h::as_bool(&r).unwrap_or(false) {
                    return Ok(Value::Int(i as i64));
                }
            }
            Ok(Value::Int(-1))
        }),
    );
    bc.methods.insert(
        "lastIndexWhere".into(),
        method(|_v, t, args, _n| {
            let f = args
                .first()
                .cloned()
                .ok_or_else(|| h::err("lastIndexWhere expects a function"))?;
            let items = list_of_list(&t)?.borrow().clone();
            let mut result = -1i64;
            for (i, item) in items.iter().enumerate() {
                let r = h::call_closure(_v, &f, vec![item.clone()])?;
                if h::as_bool(&r).unwrap_or(false) {
                    result = i as i64;
                }
            }
            Ok(Value::Int(result))
        }),
    );
    bc.methods.insert(
        "where".into(),
        method(|_v, t, args, _n| {
            let f = args
                .first()
                .cloned()
                .ok_or_else(|| h::err("where expects a function"))?;
            let items = list_of_list(&t)?.borrow().clone();
            let mut out = Vec::new();
            for item in items {
                let r = h::call_closure(_v, &f, vec![item.clone()])?;
                if h::as_bool(&r).unwrap_or(false) {
                    out.push(item);
                }
            }
            Ok(list_value(out))
        }),
    );
    bc.methods.insert(
        "expand".into(),
        method(|v, t, args, _n| {
            let f = args
                .first()
                .cloned()
                .ok_or_else(|| h::err("expand expects a function"))?;
            let items = list_of_list(&t)?.borrow().clone();
            let mut out = Vec::new();
            for item in items {
                let r = h::call_closure(v, &f, vec![item])?;
                out.extend(iterable_items(&r, v)?);
            }
            Ok(list_value(out))
        }),
    );
    bc.methods.insert(
        "reduce".into(),
        method(|_v, t, args, _n| {
            let f = args
                .first()
                .cloned()
                .ok_or_else(|| h::err("reduce expects a function"))?;
            let mut items = list_of_list(&t)?.borrow().clone().into_iter();
            let mut acc = items.next().ok_or_else(|| h::err("reduce: empty list"))?;
            for item in items {
                acc = h::call_closure(_v, &f, vec![acc, item])?;
            }
            Ok(acc)
        }),
    );
    bc.methods.insert(
        "fold".into(),
        method(|_v, t, args, _n| {
            let initial = args.first().cloned().unwrap_or(Value::Null);
            let f = args
                .get(1)
                .cloned()
                .ok_or_else(|| h::err("fold expects a function"))?;
            let mut acc = initial;
            for item in list_of_list(&t)?.borrow().clone() {
                acc = h::call_closure(_v, &f, vec![acc, item])?;
            }
            Ok(acc)
        }),
    );
    bc.methods.insert(
        "join".into(),
        method(|_v, t, args, _n| {
            let sep = args.first().and_then(h::as_str).unwrap_or("");
            let items = list_of_list(&t)?.borrow().clone();
            Ok(Value::str(
                items
                    .iter()
                    .map(value_display)
                    .collect::<Vec<_>>()
                    .join(sep),
            ))
        }),
    );
    bc.methods.insert(
        "take".into(),
        method(|_v, t, args, _n| {
            let n = h::arg_int(&args, 0, "take")?;
            let items = list_of_list(&t)?.borrow().clone();
            Ok(list_value(
                items.into_iter().take(n.max(0) as usize).collect(),
            ))
        }),
    );
    bc.methods.insert(
        "takeWhile".into(),
        method(|_v, t, args, _n| {
            let f = args
                .first()
                .cloned()
                .ok_or_else(|| h::err("takeWhile expects a function"))?;
            let items = list_of_list(&t)?.borrow().clone();
            let mut out = Vec::new();
            for item in items {
                let r = h::call_closure(_v, &f, vec![item.clone()])?;
                if !h::as_bool(&r).unwrap_or(false) {
                    break;
                }
                out.push(item);
            }
            Ok(list_value(out))
        }),
    );
    bc.methods.insert(
        "skip".into(),
        method(|_v, t, args, _n| {
            let n = h::arg_int(&args, 0, "skip")?;
            let items = list_of_list(&t)?.borrow().clone();
            Ok(list_value(
                items.into_iter().skip(n.max(0) as usize).collect(),
            ))
        }),
    );
    bc.methods.insert(
        "skipWhile".into(),
        method(|_v, t, args, _n| {
            let f = args
                .first()
                .cloned()
                .ok_or_else(|| h::err("skipWhile expects a function"))?;
            let items = list_of_list(&t)?.borrow().clone();
            let mut skipped = false;
            let mut out = Vec::new();
            for item in items {
                let r = h::call_closure(_v, &f, vec![item.clone()])?;
                if !skipped && h::as_bool(&r).unwrap_or(false) {
                    continue;
                }
                skipped = true;
                out.push(item);
            }
            Ok(list_value(out))
        }),
    );
    bc.methods.insert(
        "toList".into(),
        method(|_v, t, _a, _n| Ok(list_value(list_of_list(&t)?.borrow().clone()))),
    );
    bc.methods.insert(
        "toSet".into(),
        method(|_v, t, _a, _n| {
            let mut set = DartSet::new();
            for item in list_of_list(&t)?.borrow().clone() {
                set.insert(item);
            }
            Ok(Value::Set(Rc::new(RefCell::new(set))))
        }),
    );
    bc.methods.insert(
        "firstWhere".into(),
        method(|_v, t, args, named| {
            let f = args
                .first()
                .cloned()
                .ok_or_else(|| h::err("firstWhere expects a function"))?;
            for item in list_of_list(&t)?.borrow().clone() {
                let r = h::call_closure(_v, &f, vec![item.clone()])?;
                if h::as_bool(&r).unwrap_or(false) {
                    return Ok(item);
                }
            }
            match named.get("orElse") {
                Some(or) => h::call_closure(_v, or, vec![]),
                None => Err(h::err("firstWhere: no element found")),
            }
        }),
    );
    bc.methods.insert(
        "lastWhere".into(),
        method(|_v, t, args, named| {
            let f = args
                .first()
                .cloned()
                .ok_or_else(|| h::err("lastWhere expects a function"))?;
            let mut found = None;
            for item in list_of_list(&t)?.borrow().clone() {
                let r = h::call_closure(_v, &f, vec![item.clone()])?;
                if h::as_bool(&r).unwrap_or(false) {
                    found = Some(item);
                }
            }
            match found {
                Some(item) => Ok(item),
                None => match named.get("orElse") {
                    Some(or) => h::call_closure(_v, or, vec![]),
                    None => Err(h::err("lastWhere: no element found")),
                },
            }
        }),
    );
    bc.methods.insert(
        "singleWhere".into(),
        method(|_v, t, args, named| {
            let f = args
                .first()
                .cloned()
                .ok_or_else(|| h::err("singleWhere expects a function"))?;
            let mut found = None;
            for item in list_of_list(&t)?.borrow().clone() {
                let r = h::call_closure(_v, &f, vec![item.clone()])?;
                if h::as_bool(&r).unwrap_or(false) {
                    if found.is_some() {
                        return Err(h::err("singleWhere: more than one element found"));
                    }
                    found = Some(item);
                }
            }
            match found {
                Some(item) => Ok(item),
                None => match named.get("orElse") {
                    Some(or) => h::call_closure(_v, or, vec![]),
                    None => Err(h::err("singleWhere: no element found")),
                },
            }
        }),
    );
    bc.methods.insert(
        "insert".into(),
        method(|_v, t, args, _n| {
            let i = h::arg_int(&args, 0, "insert")?;
            let v = args.get(1).cloned().unwrap_or(Value::Null);
            let l = list_of_list(&t)?;
            let mut items = l.borrow_mut();
            let len = items.len();
            if i < 0 || i as usize > len {
                return Err(h::err(format!("insert: index out of range {i}")));
            }
            items.insert(i as usize, v);
            Ok(Value::Null)
        }),
    );
    bc.methods.insert(
        "insertAll".into(),
        method(|v, t, args, _n| {
            let i = h::arg_int(&args, 0, "insertAll")?;
            let src = args
                .get(1)
                .ok_or_else(|| h::err("insertAll expects an Iterable"))?;
            let items = iterable_items(src, v)?;
            let l = list_of_list(&t)?;
            let mut target = l.borrow_mut();
            let len = target.len();
            if i < 0 || i as usize > len {
                return Err(h::err(format!("insertAll: index out of range {i}")));
            }
            for (idx, item) in (i as usize..).zip(items) {
                target.insert(idx, item);
            }
            Ok(Value::Null)
        }),
    );
    bc.methods.insert(
        "setAll".into(),
        method(|_v, t, args, _n| {
            let i = h::arg_int(&args, 0, "setAll")?;
            let src = args
                .get(1)
                .ok_or_else(|| h::err("setAll expects an Iterable"))?;
            let items = iterable_items(src, _v)?;
            let l = list_of_list(&t)?;
            let mut target = l.borrow_mut();
            for (idx, item) in (i.max(0) as usize..).zip(items) {
                if idx >= target.len() {
                    target.push(item);
                } else {
                    target[idx] = item;
                }
            }
            Ok(Value::Null)
        }),
    );
    bc.methods.insert(
        "fillRange".into(),
        method(|_v, t, args, _n| {
            let start = h::arg_int(&args, 0, "fillRange")?;
            let end = h::arg_int(&args, 1, "fillRange")?;
            let fill = args.get(2).cloned().unwrap_or(Value::Null);
            let l = list_of_list(&t)?;
            let mut items = l.borrow_mut();
            let s = (start.max(0) as usize).min(items.len());
            let e = (end.max(0) as usize).min(items.len());
            for i in s..e {
                items[i] = fill.clone();
            }
            Ok(Value::Null)
        }),
    );
    bc.methods.insert(
        "replaceRange".into(),
        method(|_v, t, args, _n| {
            let start = h::arg_int(&args, 0, "replaceRange")?;
            let end = h::arg_int(&args, 1, "replaceRange")?;
            let src = args
                .get(2)
                .ok_or_else(|| h::err("replaceRange expects an Iterable"))?;
            let items = iterable_items(src, _v)?;
            let l = list_of_list(&t)?;
            let mut target = l.borrow_mut();
            let s = (start.max(0) as usize).min(target.len());
            let e = (end.max(0) as usize).min(target.len());
            target.splice(s..e, items);
            Ok(Value::Null)
        }),
    );
    bc.methods.insert(
        "removeRange".into(),
        method(|_v, t, args, _n| {
            let start = h::arg_int(&args, 0, "removeRange")?;
            let end = h::arg_int(&args, 1, "removeRange")?;
            let l = list_of_list(&t)?;
            let mut target = l.borrow_mut();
            let s = (start.max(0) as usize).min(target.len());
            let e = (end.max(0) as usize).min(target.len());
            if s > e {
                return Err(h::err("removeRange: invalid range"));
            }
            target.drain(s..e);
            Ok(Value::Null)
        }),
    );
    bc.methods.insert(
        "retainWhere".into(),
        method(|_v, t, args, _n| {
            let f = args
                .first()
                .cloned()
                .ok_or_else(|| h::err("retainWhere expects a function"))?;
            let l = list_of_list(&t)?;
            let mut keep = Vec::new();
            for item in l.borrow().clone() {
                let r = h::call_closure(_v, &f, vec![item.clone()])?;
                if h::as_bool(&r).unwrap_or(false) {
                    keep.push(item);
                }
            }
            *l.borrow_mut() = keep;
            Ok(Value::Null)
        }),
    );
    bc.methods.insert(
        "removeWhere".into(),
        method(|_v, t, args, _n| {
            let f = args
                .first()
                .cloned()
                .ok_or_else(|| h::err("removeWhere expects a function"))?;
            let l = list_of_list(&t)?;
            let mut keep = Vec::new();
            for item in l.borrow().clone() {
                let r = h::call_closure(_v, &f, vec![item.clone()])?;
                if !h::as_bool(&r).unwrap_or(false) {
                    keep.push(item);
                }
            }
            *l.borrow_mut() = keep;
            Ok(Value::Null)
        }),
    );
    bc.methods.insert(
        "sort".into(),
        method(|_v, t, args, _n| {
            let l = list_of_list(&t)?;
            let items = l.borrow().clone();
            let cmp = args.first().cloned();
            let sorted = match &cmp {
                Some(f) => insertion_sort(items, |a, b| {
                    let r = h::call_closure(_v, f, vec![a.clone(), b.clone()])?;
                    Ok(h::as_int(&r).unwrap_or(0))
                })?,
                None => {
                    let mut sorted = items;
                    sort_native(&mut sorted)?;
                    sorted
                }
            };
            *l.borrow_mut() = sorted;
            Ok(Value::Null)
        }),
    );
    bc.methods.insert(
        "shuffle".into(),
        method(|_v, t, _a, _n| {
            let l = list_of_list(&t)?;
            let mut items = l.borrow_mut();
            let mut rng = SimpleRng::new();
            for i in (1..items.len()).rev() {
                let j = (rng.next() as usize) % (i + 1);
                items.swap(i, j);
            }
            Ok(Value::Null)
        }),
    );
    bc.methods.insert(
        "asMap".into(),
        method(|_v, t, _a, _n| {
            let items = list_of_list(&t)?.borrow().clone();
            let mut map = DartMap::new();
            for (i, item) in items.into_iter().enumerate() {
                map.set(Value::Int(i as i64), item);
            }
            Ok(Value::Map(Rc::new(RefCell::new(map))))
        }),
    );
    bc.methods
        .insert("cast".into(), method(|_v, t, _a, _n| Ok(t)));
    bc.methods.insert(
        "followedBy".into(),
        method(|_v, t, args, _n| {
            let other = args
                .first()
                .ok_or_else(|| h::err("followedBy expects an Iterable"))?;
            let mut out = list_of_list(&t)?.borrow().clone();
            out.extend(iterable_items(other, _v)?);
            Ok(list_value(out))
        }),
    );
    bc.methods.insert(
        "elementAt".into(),
        method(|_v, t, args, _n| {
            let i = h::arg_int(&args, 0, "elementAt")?;
            let l = list_of_list(&t)?;
            let items = l.borrow();
            items
                .get(i as usize)
                .cloned()
                .ok_or_else(|| h::err(format!("elementAt: index out of range {i}")))
        }),
    );
    bc.methods.insert(
        "setRange".into(),
        method(|_v, t, args, _n| {
            let start = h::arg_int(&args, 0, "setRange")?;
            let end = h::arg_int(&args, 1, "setRange")?;
            let src = args
                .get(2)
                .ok_or_else(|| h::err("setRange expects an Iterable"))?;
            let skip = args.get(3).and_then(h::as_int).unwrap_or(0);
            let items = iterable_items(src, _v)?;
            let l = list_of_list(&t)?;
            let mut target = l.borrow_mut();
            let s = (start.max(0) as usize).min(target.len());
            let e = (end.max(0) as usize).min(target.len());
            for (si, i) in (skip.max(0) as usize..).zip(s..e) {
                if si < items.len() {
                    target[i] = items[si].clone();
                }
            }
            Ok(Value::Null)
        }),
    );
    bc.methods.insert(
        "getRange".into(),
        method(|_v, t, args, _n| {
            let start = h::arg_int(&args, 0, "getRange")?;
            let end = h::arg_int(&args, 1, "getRange")?;
            let l = list_of_list(&t)?;
            let items = l.borrow();
            let s = (start.max(0) as usize).min(items.len());
            let e = (end.max(0) as usize).min(items.len());
            if s > e {
                return Err(h::err("getRange: invalid range"));
            }
            Ok(list_value(items[s..e].to_vec()))
        }),
    );
    bc.getters.insert(
        "length".into(),
        getter(|_v, t| Ok(Value::Int(list_of_list(&t)?.borrow().len() as i64))),
    );
    bc.getters.insert(
        "isEmpty".into(),
        getter(|_v, t| Ok(Value::Bool(list_of_list(&t)?.borrow().is_empty()))),
    );
    bc.getters.insert(
        "isNotEmpty".into(),
        getter(|_v, t| Ok(Value::Bool(!list_of_list(&t)?.borrow().is_empty()))),
    );
    bc.getters.insert(
        "first".into(),
        getter(|_v, t| {
            list_of_list(&t)?
                .borrow()
                .first()
                .cloned()
                .ok_or_else(|| h::err("first: empty list"))
        }),
    );
    bc.getters.insert(
        "last".into(),
        getter(|_v, t| {
            list_of_list(&t)?
                .borrow()
                .last()
                .cloned()
                .ok_or_else(|| h::err("last: empty list"))
        }),
    );
    bc.getters.insert(
        "single".into(),
        getter(|_v, t| {
            let l = list_of_list(&t)?;
            let items = l.borrow();
            match items.len() {
                1 => Ok(items[0].clone()),
                0 => Err(h::err("single: empty list")),
                _ => Err(h::err("single: too many elements")),
            }
        }),
    );
    bc.getters.insert(
        "reversed".into(),
        getter(|_v, t| {
            let mut items = list_of_list(&t)?.borrow().clone();
            items.reverse();
            Ok(list_value(items))
        }),
    );
    bc.getters.insert(
        "iterator".into(),
        getter(|_v, t| {
            let items = list_of_list(&t)?.borrow().clone();
            Ok(h::wrap(&h::class("Iterator"), list_value(items)))
        }),
    );
    bc.getters.insert(
        "runtimeType".into(),
        getter(|_v, _t| Ok(Value::Type(crate::rt::RuntimeType::named("List")))),
    );
    bc.getters.insert(
        "hashCode".into(),
        getter(|_v, t| Ok(Value::Int(h::hash_code(&t)))),
    );
    bc.setters.insert(
        "length".into(),
        setter(|_v, t, value| {
            let new_len =
                h::as_int(&value).ok_or_else(|| h::err("length setter expects an int"))?;
            if new_len < 0 {
                return Err(h::err("length setter: negative length"));
            }
            let l = list_of_list(&t)?;
            let mut items = l.borrow_mut();
            let len = items.len();
            let new_len = new_len as usize;
            if new_len < len {
                items.truncate(new_len);
            } else {
                items.resize(new_len, Value::Null);
            }
            Ok(())
        }),
    );
    bc.setters.insert(
        "first".into(),
        setter(|_v, t, value| {
            let l = list_of_list(&t)?;
            let mut items = l.borrow_mut();
            let first = items
                .first_mut()
                .ok_or_else(|| h::err("first setter: empty list"))?;
            *first = value;
            Ok(())
        }),
    );
    bc.setters.insert(
        "last".into(),
        setter(|_v, t, value| {
            let l = list_of_list(&t)?;
            let mut items = l.borrow_mut();
            let last = items
                .last_mut()
                .ok_or_else(|| h::err("last setter: empty list"))?;
            *last = value;
            Ok(())
        }),
    );
    bc
}

fn insertion_sort(
    items: Vec<Value>,
    mut less: impl FnMut(&Value, &Value) -> Result<i64, InterpError>,
) -> Result<Vec<Value>, InterpError> {
    let mut out = items;
    for i in 1..out.len() {
        let key = out[i].clone();
        let mut j = i;
        while j > 0 && less(&out[j - 1], &key)? > 0 {
            out[j] = out[j - 1].clone();
            j -= 1;
        }
        out[j] = key;
    }
    Ok(out)
}

fn sort_native(items: &mut [Value]) -> Result<(), InterpError> {
    for i in 1..items.len() {
        let key = items[i].clone();
        let mut j = i;
        while j > 0 {
            let ord = native_cmp(&items[j - 1], &key)?;
            if ord > 0 {
                items[j] = items[j - 1].clone();
                j -= 1;
            } else {
                break;
            }
        }
        items[j] = key;
    }
    Ok(())
}

fn native_cmp(a: &Value, b: &Value) -> Result<i64, InterpError> {
    if let (Some(x), Some(y)) = (h::as_double(a), h::as_double(b)) {
        return Ok(match x.partial_cmp(&y) {
            Some(std::cmp::Ordering::Less) => -1,
            Some(std::cmp::Ordering::Equal) => 0,
            Some(std::cmp::Ordering::Greater) => 1,
            None => 0,
        });
    }
    if let (Some(x), Some(y)) = (h::as_str(a), h::as_str(b)) {
        return Ok(match x.cmp(y) {
            std::cmp::Ordering::Less => -1,
            std::cmp::Ordering::Equal => 0,
            std::cmp::Ordering::Greater => 1,
        });
    }
    Ok(value_display(a).cmp(&value_display(b)) as i64)
}

struct SimpleRng {
    state: u64,
}

impl SimpleRng {
    fn new() -> SimpleRng {
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0x9E3779B97F4A7C15);
        SimpleRng { state: seed | 1 }
    }

    fn next(&mut self) -> u64 {
        self.state = self
            .state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.state >> 32
    }
}

pub fn map_entry_value(key: Value, value: Value) -> Value {
    h::wrap(&h::class("MapEntry"), list_value(vec![key, value]))
}

fn as_map_entry(v: &Value) -> Option<(Value, Value)> {
    if let Value::Bridged(b) = v
        && b.borrow().bridged_class.name == "MapEntry"
        && let Value::List(l) = &b.borrow().native
    {
        let items = l.borrow();
        if items.len() == 2 {
            return Some((items[0].clone(), items[1].clone()));
        }
    }
    if let Value::List(l) = v {
        let items = l.borrow();
        if items.len() == 2 {
            return Some((items[0].clone(), items[1].clone()));
        }
    }
    None
}

pub fn map_definition() -> BridgedClassDef {
    let mut bc = BridgedClassDef::new("Map");
    bc.native_type = Some("Map".into());
    bc.constructors.insert(
        "".into(),
        ctor(|_v, _a, _n| Ok(Value::Map(Rc::new(RefCell::new(DartMap::new()))))),
    );
    bc.static_methods.insert(
        "from".into(),
        static_method(|_v, args, _n| {
            let src = h::as_map(
                args.first()
                    .ok_or_else(|| h::err("Map.from expects a Map"))?,
            )
            .ok_or_else(|| h::err("Map.from expects a Map"))?;
            let mut out = DartMap::new();
            for (k, v) in src.borrow().entries.clone() {
                out.set(k, v);
            }
            Ok(Value::Map(Rc::new(RefCell::new(out))))
        }),
    );
    bc.static_methods.insert(
        "of".into(),
        static_method(|_v, args, _n| {
            let src = h::as_map(args.first().ok_or_else(|| h::err("Map.of expects a Map"))?)
                .ok_or_else(|| h::err("Map.of expects a Map"))?;
            let mut out = DartMap::new();
            for (k, v) in src.borrow().entries.clone() {
                out.set(k, v);
            }
            Ok(Value::Map(Rc::new(RefCell::new(out))))
        }),
    );
    bc.static_methods.insert(
        "unmodifiable".into(),
        static_method(|_v, args, _n| {
            let src = h::as_map(
                args.first()
                    .ok_or_else(|| h::err("Map.unmodifiable expects a Map"))?,
            )
            .ok_or_else(|| h::err("Map.unmodifiable expects a Map"))?;
            let mut out = DartMap::new();
            for (k, v) in src.borrow().entries.clone() {
                out.set(k, v);
            }
            Ok(Value::Map(Rc::new(RefCell::new(out))))
        }),
    );
    bc.static_methods.insert(
        "identity".into(),
        static_method(|_v, _a, _n| Ok(Value::Map(Rc::new(RefCell::new(DartMap::new()))))),
    );
    bc.static_methods.insert(
        "fromIterable".into(),
        static_method(|_v, args, named| {
            let src = args
                .first()
                .ok_or_else(|| h::err("Map.fromIterable expects an Iterable"))?;
            let key_fn = named.get("key").cloned();
            let value_fn = named.get("value").cloned();
            let mut out = DartMap::new();
            for item in iterable_items(src, _v)? {
                let k = match &key_fn {
                    Some(f) => h::call_closure(_v, f, vec![item.clone()])?,
                    None => item.clone(),
                };
                let v = match &value_fn {
                    Some(f) => h::call_closure(_v, f, vec![item.clone()])?,
                    None => item.clone(),
                };
                out.set(k, v);
            }
            Ok(Value::Map(Rc::new(RefCell::new(out))))
        }),
    );
    bc.static_methods.insert(
        "fromIterables".into(),
        static_method(|_v, args, _n| {
            let keys = args
                .first()
                .ok_or_else(|| h::err("Map.fromIterables expects two Iterables"))?;
            let values = args
                .get(1)
                .ok_or_else(|| h::err("Map.fromIterables expects two Iterables"))?;
            let k_items = iterable_items(keys, _v)?;
            let v_items = iterable_items(values, _v)?;
            let mut out = DartMap::new();
            for (k, v) in k_items.into_iter().zip(v_items) {
                out.set(k, v);
            }
            Ok(Value::Map(Rc::new(RefCell::new(out))))
        }),
    );
    bc.static_methods.insert(
        "fromEntries".into(),
        static_method(|_v, args, _n| {
            let entries = args
                .first()
                .ok_or_else(|| h::err("Map.fromEntries expects an Iterable"))?;
            let mut out = DartMap::new();
            for entry in iterable_items(entries, _v)? {
                let (k, v) = as_map_entry(&entry)
                    .ok_or_else(|| h::err("fromEntries expects Iterable<MapEntry>"))?;
                out.set(k, v);
            }
            Ok(Value::Map(Rc::new(RefCell::new(out))))
        }),
    );
    bc.methods.insert(
        "[]".into(),
        method(|_v, t, args, _n| {
            let key = args.first().cloned().unwrap_or(Value::Null);
            let m = h::as_map(&t).ok_or_else(|| h::err("[]: expected a Map"))?;
            Ok(m.borrow().get(&key).unwrap_or(Value::Null))
        }),
    );
    bc.methods.insert(
        "[]=".into(),
        method(|_v, t, args, _n| {
            let key = args.first().cloned().unwrap_or(Value::Null);
            let value = args.get(1).cloned().unwrap_or(Value::Null);
            let m = h::as_map(&t).ok_or_else(|| h::err("[]=: expected a Map"))?;
            m.borrow_mut().set(key, value);
            Ok(Value::Null)
        }),
    );
    bc.methods.insert(
        "addAll".into(),
        method(|_v, t, args, _n| {
            let src = args
                .first()
                .cloned()
                .ok_or_else(|| h::err("addAll expects a Map"))?;
            let entries = h::as_map(&src)
                .ok_or_else(|| h::err("addAll expects a Map"))?
                .borrow()
                .entries
                .clone();
            let m = h::as_map(&t).ok_or_else(|| h::err("addAll: expected a Map"))?;
            let mut m = m.borrow_mut();
            for (k, v) in entries {
                m.set(k, v);
            }
            Ok(Value::Null)
        }),
    );
    bc.methods.insert(
        "addEntries".into(),
        method(|_v, t, args, _n| {
            let entries = args
                .first()
                .ok_or_else(|| h::err("addEntries expects an Iterable"))?;
            let m = h::as_map(&t).ok_or_else(|| h::err("addEntries: expected a Map"))?;
            let mut m = m.borrow_mut();
            for entry in iterable_items(entries, _v)? {
                let (k, v) = as_map_entry(&entry)
                    .ok_or_else(|| h::err("addEntries expects Iterable<MapEntry>"))?;
                m.set(k, v);
            }
            Ok(Value::Null)
        }),
    );
    bc.methods.insert(
        "clear".into(),
        method(|_v, t, _a, _n| {
            h::as_map(&t)
                .ok_or_else(|| h::err("clear: expected a Map"))?
                .borrow_mut()
                .entries
                .clear();
            Ok(Value::Null)
        }),
    );
    bc.methods.insert(
        "containsKey".into(),
        method(|_v, t, args, _n| {
            let key = args.first().cloned().unwrap_or(Value::Null);
            let m = h::as_map(&t).ok_or_else(|| h::err("containsKey: expected a Map"))?;
            Ok(Value::Bool(m.borrow().contains(&key)))
        }),
    );
    bc.methods.insert(
        "containsValue".into(),
        method(|_v, t, args, _n| {
            let needle = args.first().cloned().unwrap_or(Value::Null);
            let m = h::as_map(&t).ok_or_else(|| h::err("containsValue: expected a Map"))?;
            Ok(Value::Bool(
                m.borrow().entries.iter().any(|(_, v)| value_eq(v, &needle)),
            ))
        }),
    );
    bc.methods.insert(
        "forEach".into(),
        method(|_v, t, args, _n| {
            let f = args
                .first()
                .cloned()
                .ok_or_else(|| h::err("forEach expects a function"))?;
            let entries = h::as_map(&t)
                .ok_or_else(|| h::err("forEach: expected a Map"))?
                .borrow()
                .entries
                .clone();
            for (k, v) in entries {
                h::call_closure(_v, &f, vec![k, v])?;
            }
            Ok(Value::Null)
        }),
    );
    bc.methods.insert(
        "putIfAbsent".into(),
        method(|_v, t, args, _n| {
            let key = args.first().cloned().unwrap_or(Value::Null);
            let f = args
                .get(1)
                .cloned()
                .ok_or_else(|| h::err("putIfAbsent expects a function"))?;
            let m = h::as_map(&t).ok_or_else(|| h::err("putIfAbsent: expected a Map"))?;
            if let Some(v) = m.borrow().get(&key) {
                return Ok(v);
            }
            let v = h::call_closure(_v, &f, vec![])?;
            m.borrow_mut().set(key, v.clone());
            Ok(v)
        }),
    );
    bc.methods.insert(
        "remove".into(),
        method(|_v, t, args, _n| {
            let key = args.first().cloned().unwrap_or(Value::Null);
            let m = h::as_map(&t).ok_or_else(|| h::err("remove: expected a Map"))?;
            Ok(m.borrow_mut().remove(&key).unwrap_or(Value::Null))
        }),
    );
    bc.methods.insert(
        "removeWhere".into(),
        method(|_v, t, args, _n| {
            let f = args
                .first()
                .cloned()
                .ok_or_else(|| h::err("removeWhere expects a function"))?;
            let m = h::as_map(&t).ok_or_else(|| h::err("removeWhere: expected a Map"))?;
            let entries = m.borrow().entries.clone();
            for (k, v) in entries {
                let r = h::call_closure(_v, &f, vec![k.clone(), v])?;
                if h::as_bool(&r).unwrap_or(false) {
                    m.borrow_mut().remove(&k);
                }
            }
            Ok(Value::Null)
        }),
    );
    bc.methods.insert(
        "update".into(),
        method(|_v, t, args, named| {
            let key = args.first().cloned().unwrap_or(Value::Null);
            let f = args
                .get(1)
                .cloned()
                .ok_or_else(|| h::err("update expects a function"))?;
            let m = h::as_map(&t).ok_or_else(|| h::err("update: expected a Map"))?;
            let old = m.borrow().get(&key);
            if let Some(old) = old {
                let new = h::call_closure(_v, &f, vec![old])?;
                m.borrow_mut().set(key, new.clone());
                return Ok(new);
            }
            match named.get("ifAbsent") {
                Some(if_absent) => {
                    let v = h::call_closure(_v, if_absent, vec![])?;
                    m.borrow_mut().set(key, v.clone());
                    Ok(v)
                }
                None => Err(h::err("update: key not found")),
            }
        }),
    );
    bc.methods.insert(
        "updateAll".into(),
        method(|_v, t, args, _n| {
            let f = args
                .first()
                .cloned()
                .ok_or_else(|| h::err("updateAll expects a function"))?;
            let m = h::as_map(&t).ok_or_else(|| h::err("updateAll: expected a Map"))?;
            let entries = m.borrow().entries.clone();
            for (k, v) in entries {
                let new = h::call_closure(_v, &f, vec![k.clone(), v])?;
                m.borrow_mut().set(k, new);
            }
            Ok(Value::Null)
        }),
    );
    bc.methods.insert(
        "map".into(),
        method(|_v, t, args, _n| {
            let f = args
                .first()
                .cloned()
                .ok_or_else(|| h::err("map expects a function"))?;
            let entries = h::as_map(&t)
                .ok_or_else(|| h::err("map: expected a Map"))?
                .borrow()
                .entries
                .clone();
            let mut out = DartMap::new();
            for (k, v) in entries {
                let r = h::call_closure(_v, &f, vec![k, v])?;
                let (nk, nv) = as_map_entry(&r)
                    .ok_or_else(|| h::err("Map.map callback must return a MapEntry"))?;
                out.set(nk, nv);
            }
            Ok(Value::Map(Rc::new(RefCell::new(out))))
        }),
    );
    bc.methods
        .insert("cast".into(), method(|_v, t, _a, _n| Ok(t)));
    bc.getters.insert(
        "length".into(),
        getter(|_v, t| {
            Ok(Value::Int(
                h::as_map(&t)
                    .ok_or_else(|| h::err("length: expected a Map"))?
                    .borrow()
                    .len() as i64,
            ))
        }),
    );
    bc.getters.insert(
        "hashCode".into(),
        getter(|_v, t| Ok(Value::Int(h::hash_code(&t)))),
    );
    bc.getters.insert(
        "isEmpty".into(),
        getter(|_v, t| {
            Ok(Value::Bool(
                h::as_map(&t)
                    .ok_or_else(|| h::err("isEmpty: expected a Map"))?
                    .borrow()
                    .is_empty(),
            ))
        }),
    );
    bc.getters.insert(
        "isNotEmpty".into(),
        getter(|_v, t| {
            Ok(Value::Bool(
                !h::as_map(&t)
                    .ok_or_else(|| h::err("isNotEmpty: expected a Map"))?
                    .borrow()
                    .is_empty(),
            ))
        }),
    );
    bc.getters.insert(
        "keys".into(),
        getter(|_v, t| {
            let m = h::as_map(&t).ok_or_else(|| h::err("keys: expected a Map"))?;
            Ok(list_value(m.borrow().keys()))
        }),
    );
    bc.getters.insert(
        "values".into(),
        getter(|_v, t| {
            let m = h::as_map(&t).ok_or_else(|| h::err("values: expected a Map"))?;
            Ok(list_value(
                m.borrow().entries.iter().map(|(_, v)| v.clone()).collect(),
            ))
        }),
    );
    bc.getters.insert(
        "entries".into(),
        getter(|_v, t| {
            let m = h::as_map(&t).ok_or_else(|| h::err("entries: expected a Map"))?;
            Ok(list_value(
                m.borrow()
                    .entries
                    .iter()
                    .map(|(k, v)| map_entry_value(k.clone(), v.clone()))
                    .collect(),
            ))
        }),
    );
    bc.getters.insert(
        "runtimeType".into(),
        getter(|_v, _t| Ok(Value::Type(crate::rt::RuntimeType::named("Map")))),
    );
    bc
}

pub fn map_entry_definition() -> BridgedClassDef {
    let mut bc = BridgedClassDef::new("MapEntry");
    bc.constructors.insert(
        "".into(),
        ctor(|_v, args, _n| {
            if args.len() != 2 {
                return Err(h::err(
                    "MapEntry constructor requires 2 arguments (key, value).",
                ));
            }
            Ok(map_entry_value(args[0].clone(), args[1].clone()))
        }),
    );
    bc.methods.insert(
        "hashCode".into(),
        method(|_v, t, _a, _n| Ok(Value::Int(h::hash_code(&t)))),
    );
    bc.methods.insert(
        "toString".into(),
        method(|_v, t, _a, _n| {
            let (k, v) = as_map_entry(&t).unwrap_or((Value::Null, Value::Null));
            Ok(Value::str(format!(
                "({}: {})",
                value_display(&k),
                value_display(&v)
            )))
        }),
    );
    bc.getters.insert(
        "key".into(),
        getter(|_v, t| Ok(as_map_entry(&t).map(|(k, _)| k).unwrap_or(Value::Null))),
    );
    bc.getters.insert(
        "value".into(),
        getter(|_v, t| Ok(as_map_entry(&t).map(|(_, v)| v).unwrap_or(Value::Null))),
    );
    bc
}

pub fn set_definition() -> BridgedClassDef {
    let mut bc = BridgedClassDef::new("Set");
    bc.native_type = Some("Set".into());
    bc.constructors.insert(
        "".into(),
        ctor(|_v, _a, _n| Ok(Value::Set(Rc::new(RefCell::new(DartSet::new()))))),
    );
    bc.static_methods.insert(
        "from".into(),
        static_method(|_v, args, _n| {
            let src = args
                .first()
                .ok_or_else(|| h::err("Set.from expects an Iterable"))?;
            let mut set = DartSet::new();
            for item in iterable_items(src, _v)? {
                set.insert(item);
            }
            Ok(Value::Set(Rc::new(RefCell::new(set))))
        }),
    );
    bc.static_methods.insert(
        "of".into(),
        static_method(|_v, args, _n| {
            let src = args
                .first()
                .ok_or_else(|| h::err("Set.of expects an Iterable"))?;
            let mut set = DartSet::new();
            for item in iterable_items(src, _v)? {
                set.insert(item);
            }
            Ok(Value::Set(Rc::new(RefCell::new(set))))
        }),
    );
    bc.static_methods.insert(
        "identity".into(),
        static_method(|_v, _a, _n| Ok(Value::Set(Rc::new(RefCell::new(DartSet::new()))))),
    );
    bc.static_methods.insert(
        "unmodifiable".into(),
        static_method(|_v, args, _n| {
            let src = args
                .first()
                .ok_or_else(|| h::err("Set.unmodifiable expects an Iterable"))?;
            let mut set = DartSet::new();
            for item in iterable_items(src, _v)? {
                set.insert(item);
            }
            Ok(Value::Set(Rc::new(RefCell::new(set))))
        }),
    );
    let set_methods: Vec<(&str, M)> = vec![
        (
            "add",
            method(|_v, t, args, _n| {
                let v = args.first().cloned().unwrap_or(Value::Null);
                let s = h::as_set(&t).ok_or_else(|| h::err("add: expected a Set"))?;
                Ok(Value::Bool(s.borrow_mut().insert(v)))
            }),
        ),
        (
            "addAll",
            method(|_v, t, args, _n| {
                let src = args
                    .first()
                    .ok_or_else(|| h::err("addAll expects an Iterable"))?;
                let s = h::as_set(&t).ok_or_else(|| h::err("addAll: expected a Set"))?;
                for item in iterable_items(src, _v)? {
                    s.borrow_mut().insert(item);
                }
                Ok(Value::Null)
            }),
        ),
        (
            "clear",
            method(|_v, t, _a, _n| {
                h::as_set(&t)
                    .ok_or_else(|| h::err("clear: expected a Set"))?
                    .borrow_mut()
                    .items
                    .clear();
                Ok(Value::Null)
            }),
        ),
        (
            "contains",
            method(|_v, t, args, _n| {
                let v = args.first().cloned().unwrap_or(Value::Null);
                let s = h::as_set(&t).ok_or_else(|| h::err("contains: expected a Set"))?;
                Ok(Value::Bool(s.borrow().contains(&v)))
            }),
        ),
        (
            "containsAll",
            method(|_v, t, args, _n| {
                let src = args
                    .first()
                    .ok_or_else(|| h::err("containsAll expects an Iterable"))?;
                let s = h::as_set(&t).ok_or_else(|| h::err("containsAll: expected a Set"))?;
                for item in iterable_items(src, _v)? {
                    if !s.borrow().contains(&item) {
                        return Ok(Value::Bool(false));
                    }
                }
                Ok(Value::Bool(true))
            }),
        ),
        (
            "difference",
            method(|_v, t, args, _n| {
                let other = args
                    .first()
                    .and_then(h::as_set)
                    .ok_or_else(|| h::err("difference expects a Set"))?;
                let s = h::as_set(&t).ok_or_else(|| h::err("difference: expected a Set"))?;
                let mut out = DartSet::new();
                for item in s.borrow().items.clone() {
                    if !other.borrow().contains(&item) {
                        out.insert(item);
                    }
                }
                Ok(Value::Set(Rc::new(RefCell::new(out))))
            }),
        ),
        (
            "intersection",
            method(|_v, t, args, _n| {
                let other = args
                    .first()
                    .and_then(h::as_set)
                    .ok_or_else(|| h::err("intersection expects a Set"))?;
                let s = h::as_set(&t).ok_or_else(|| h::err("intersection: expected a Set"))?;
                let mut out = DartSet::new();
                for item in s.borrow().items.clone() {
                    if other.borrow().contains(&item) {
                        out.insert(item);
                    }
                }
                Ok(Value::Set(Rc::new(RefCell::new(out))))
            }),
        ),
        (
            "lookup",
            method(|_v, t, args, _n| {
                let v = args.first().cloned().unwrap_or(Value::Null);
                let s = h::as_set(&t).ok_or_else(|| h::err("lookup: expected a Set"))?;
                Ok(s.borrow()
                    .items
                    .iter()
                    .find(|i| value_eq(i, &v))
                    .cloned()
                    .unwrap_or(Value::Null))
            }),
        ),
        (
            "remove",
            method(|_v, t, args, _n| {
                let v = args.first().cloned().unwrap_or(Value::Null);
                let s = h::as_set(&t).ok_or_else(|| h::err("remove: expected a Set"))?;
                Ok(Value::Bool(s.borrow_mut().remove(&v)))
            }),
        ),
        (
            "removeAll",
            method(|_v, t, args, _n| {
                let src = args
                    .first()
                    .ok_or_else(|| h::err("removeAll expects an Iterable"))?;
                let s = h::as_set(&t).ok_or_else(|| h::err("removeAll: expected a Set"))?;
                for item in iterable_items(src, _v)? {
                    s.borrow_mut().remove(&item);
                }
                Ok(Value::Null)
            }),
        ),
        (
            "removeWhere",
            method(|_v, t, args, _n| {
                let f = args
                    .first()
                    .cloned()
                    .ok_or_else(|| h::err("removeWhere expects a function"))?;
                let s = h::as_set(&t).ok_or_else(|| h::err("removeWhere: expected a Set"))?;
                let items = s.borrow().items.clone();
                for item in items {
                    let r = h::call_closure(_v, &f, vec![item.clone()])?;
                    if h::as_bool(&r).unwrap_or(false) {
                        s.borrow_mut().remove(&item);
                    }
                }
                Ok(Value::Null)
            }),
        ),
        (
            "retainAll",
            method(|_v, t, args, _n| {
                let src = args
                    .first()
                    .ok_or_else(|| h::err("retainAll expects an Iterable"))?;
                let keep: Vec<Value> = iterable_items(src, _v)?;
                let s = h::as_set(&t).ok_or_else(|| h::err("retainAll: expected a Set"))?;
                let items = s.borrow().items.clone();
                for item in items {
                    if !keep.iter().any(|k| value_eq(k, &item)) {
                        s.borrow_mut().remove(&item);
                    }
                }
                Ok(Value::Null)
            }),
        ),
        (
            "retainWhere",
            method(|_v, t, args, _n| {
                let f = args
                    .first()
                    .cloned()
                    .ok_or_else(|| h::err("retainWhere expects a function"))?;
                let s = h::as_set(&t).ok_or_else(|| h::err("retainWhere: expected a Set"))?;
                let items = s.borrow().items.clone();
                for item in items {
                    let r = h::call_closure(_v, &f, vec![item.clone()])?;
                    if !h::as_bool(&r).unwrap_or(false) {
                        s.borrow_mut().remove(&item);
                    }
                }
                Ok(Value::Null)
            }),
        ),
        (
            "union",
            method(|_v, t, args, _n| {
                let other = args
                    .first()
                    .and_then(h::as_set)
                    .ok_or_else(|| h::err("union expects a Set"))?;
                let s = h::as_set(&t).ok_or_else(|| h::err("union: expected a Set"))?;
                let mut out = DartSet::new();
                for item in s.borrow().items.clone() {
                    out.insert(item);
                }
                for item in other.borrow().items.clone() {
                    out.insert(item);
                }
                Ok(Value::Set(Rc::new(RefCell::new(out))))
            }),
        ),
        (
            "toSet",
            method(|_v, t, _a, _n| {
                let s = h::as_set(&t).ok_or_else(|| h::err("toSet: expected a Set"))?;
                let mut out = DartSet::new();
                for item in s.borrow().items.clone() {
                    out.insert(item);
                }
                Ok(Value::Set(Rc::new(RefCell::new(out))))
            }),
        ),
        (
            "toList",
            method(|_v, t, _a, _n| {
                let s = h::as_set(&t).ok_or_else(|| h::err("toList: expected a Set"))?;
                Ok(list_value(s.borrow().items.clone()))
            }),
        ),
        (
            "forEach",
            method(|_v, t, args, _n| {
                let f = args
                    .first()
                    .cloned()
                    .ok_or_else(|| h::err("forEach expects a function"))?;
                let items = h::as_set(&t)
                    .ok_or_else(|| h::err("forEach: expected a Set"))?
                    .borrow()
                    .items
                    .clone();
                for item in items {
                    h::call_closure(_v, &f, vec![item])?;
                }
                Ok(Value::Null)
            }),
        ),
        (
            "map",
            method(|_v, t, args, _n| {
                let f = args
                    .first()
                    .cloned()
                    .ok_or_else(|| h::err("map expects a function"))?;
                let items = h::as_set(&t)
                    .ok_or_else(|| h::err("map: expected a Set"))?
                    .borrow()
                    .items
                    .clone();
                let mut out = Vec::new();
                for item in items {
                    out.push(h::call_closure(_v, &f, vec![item])?);
                }
                Ok(list_value(out))
            }),
        ),
        (
            "where",
            method(|_v, t, args, _n| {
                let f = args
                    .first()
                    .cloned()
                    .ok_or_else(|| h::err("where expects a function"))?;
                let items = h::as_set(&t)
                    .ok_or_else(|| h::err("where: expected a Set"))?
                    .borrow()
                    .items
                    .clone();
                let mut out = Vec::new();
                for item in items {
                    let r = h::call_closure(_v, &f, vec![item.clone()])?;
                    if h::as_bool(&r).unwrap_or(false) {
                        out.push(item);
                    }
                }
                Ok(list_value(out))
            }),
        ),
        (
            "expand",
            method(|_v, t, args, _n| {
                let f = args
                    .first()
                    .cloned()
                    .ok_or_else(|| h::err("expand expects a function"))?;
                let items = h::as_set(&t)
                    .ok_or_else(|| h::err("expand: expected a Set"))?
                    .borrow()
                    .items
                    .clone();
                let mut out = Vec::new();
                for item in items {
                    let r = h::call_closure(_v, &f, vec![item])?;
                    out.extend(iterable_items(&r, _v)?);
                }
                Ok(list_value(out))
            }),
        ),
        (
            "every",
            method(|_v, t, args, _n| {
                let f = args
                    .first()
                    .cloned()
                    .ok_or_else(|| h::err("every expects a function"))?;
                let items = h::as_set(&t)
                    .ok_or_else(|| h::err("every: expected a Set"))?
                    .borrow()
                    .items
                    .clone();
                for item in items {
                    let r = h::call_closure(_v, &f, vec![item])?;
                    if !h::as_bool(&r).unwrap_or(false) {
                        return Ok(Value::Bool(false));
                    }
                }
                Ok(Value::Bool(true))
            }),
        ),
        (
            "any",
            method(|_v, t, args, _n| {
                let f = args
                    .first()
                    .cloned()
                    .ok_or_else(|| h::err("any expects a function"))?;
                let items = h::as_set(&t)
                    .ok_or_else(|| h::err("any: expected a Set"))?
                    .borrow()
                    .items
                    .clone();
                for item in items {
                    let r = h::call_closure(_v, &f, vec![item])?;
                    if h::as_bool(&r).unwrap_or(false) {
                        return Ok(Value::Bool(true));
                    }
                }
                Ok(Value::Bool(false))
            }),
        ),
        (
            "firstWhere",
            method(|_v, t, args, named| {
                let f = args
                    .first()
                    .cloned()
                    .ok_or_else(|| h::err("firstWhere expects a function"))?;
                let items = h::as_set(&t)
                    .ok_or_else(|| h::err("firstWhere: expected a Set"))?
                    .borrow()
                    .items
                    .clone();
                for item in items {
                    let r = h::call_closure(_v, &f, vec![item.clone()])?;
                    if h::as_bool(&r).unwrap_or(false) {
                        return Ok(item);
                    }
                }
                match named.get("orElse") {
                    Some(or) => h::call_closure(_v, or, vec![]),
                    None => Err(h::err("firstWhere: no element found")),
                }
            }),
        ),
        (
            "lastWhere",
            method(|_v, t, args, named| {
                let f = args
                    .first()
                    .cloned()
                    .ok_or_else(|| h::err("lastWhere expects a function"))?;
                let items = h::as_set(&t)
                    .ok_or_else(|| h::err("lastWhere: expected a Set"))?
                    .borrow()
                    .items
                    .clone();
                let mut found = None;
                for item in items {
                    let r = h::call_closure(_v, &f, vec![item.clone()])?;
                    if h::as_bool(&r).unwrap_or(false) {
                        found = Some(item);
                    }
                }
                match found {
                    Some(item) => Ok(item),
                    None => match named.get("orElse") {
                        Some(or) => h::call_closure(_v, or, vec![]),
                        None => Err(h::err("lastWhere: no element found")),
                    },
                }
            }),
        ),
        (
            "singleWhere",
            method(|_v, t, args, named| {
                let f = args
                    .first()
                    .cloned()
                    .ok_or_else(|| h::err("singleWhere expects a function"))?;
                let items = h::as_set(&t)
                    .ok_or_else(|| h::err("singleWhere: expected a Set"))?
                    .borrow()
                    .items
                    .clone();
                let mut found = None;
                for item in items {
                    let r = h::call_closure(_v, &f, vec![item.clone()])?;
                    if h::as_bool(&r).unwrap_or(false) {
                        if found.is_some() {
                            return Err(h::err("singleWhere: more than one element found"));
                        }
                        found = Some(item);
                    }
                }
                match found {
                    Some(item) => Ok(item),
                    None => match named.get("orElse") {
                        Some(or) => h::call_closure(_v, or, vec![]),
                        None => Err(h::err("singleWhere: no element found")),
                    },
                }
            }),
        ),
        (
            "elementAt",
            method(|_v, t, args, _n| {
                let i = h::arg_int(&args, 0, "elementAt")?;
                let s = h::as_set(&t).ok_or_else(|| h::err("elementAt: expected a Set"))?;
                s.borrow()
                    .items
                    .get(i as usize)
                    .cloned()
                    .ok_or_else(|| h::err(format!("elementAt: index out of range {i}")))
            }),
        ),
        (
            "take",
            method(|_v, t, args, _n| {
                let n = h::arg_int(&args, 0, "take")?;
                let s = h::as_set(&t).ok_or_else(|| h::err("take: expected a Set"))?;
                Ok(list_value(
                    s.borrow()
                        .items
                        .iter()
                        .take(n.max(0) as usize)
                        .cloned()
                        .collect(),
                ))
            }),
        ),
        (
            "skip",
            method(|_v, t, args, _n| {
                let n = h::arg_int(&args, 0, "skip")?;
                let s = h::as_set(&t).ok_or_else(|| h::err("skip: expected a Set"))?;
                Ok(list_value(
                    s.borrow()
                        .items
                        .iter()
                        .skip(n.max(0) as usize)
                        .cloned()
                        .collect(),
                ))
            }),
        ),
        (
            "takeWhile",
            method(|_v, t, args, _n| {
                let f = args
                    .first()
                    .cloned()
                    .ok_or_else(|| h::err("takeWhile expects a function"))?;
                let s = h::as_set(&t).ok_or_else(|| h::err("takeWhile: expected a Set"))?;
                let mut out = Vec::new();
                for item in s.borrow().items.clone() {
                    let r = h::call_closure(_v, &f, vec![item.clone()])?;
                    if !h::as_bool(&r).unwrap_or(false) {
                        break;
                    }
                    out.push(item);
                }
                Ok(list_value(out))
            }),
        ),
        (
            "skipWhile",
            method(|_v, t, args, _n| {
                let f = args
                    .first()
                    .cloned()
                    .ok_or_else(|| h::err("skipWhile expects a function"))?;
                let s = h::as_set(&t).ok_or_else(|| h::err("skipWhile: expected a Set"))?;
                let mut skipped = false;
                let mut out = Vec::new();
                for item in s.borrow().items.clone() {
                    let r = h::call_closure(_v, &f, vec![item.clone()])?;
                    if !skipped && h::as_bool(&r).unwrap_or(false) {
                        continue;
                    }
                    skipped = true;
                    out.push(item);
                }
                Ok(list_value(out))
            }),
        ),
        (
            "fold",
            method(|_v, t, args, _n| {
                let initial = args.first().cloned().unwrap_or(Value::Null);
                let f = args
                    .get(1)
                    .cloned()
                    .ok_or_else(|| h::err("fold expects a function"))?;
                let mut acc = initial;
                for item in h::as_set(&t)
                    .ok_or_else(|| h::err("fold: expected a Set"))?
                    .borrow()
                    .items
                    .clone()
                {
                    acc = h::call_closure(_v, &f, vec![acc, item])?;
                }
                Ok(acc)
            }),
        ),
        (
            "reduce",
            method(|_v, t, args, _n| {
                let f = args
                    .first()
                    .cloned()
                    .ok_or_else(|| h::err("reduce expects a function"))?;
                let mut items = h::as_set(&t)
                    .ok_or_else(|| h::err("reduce: expected a Set"))?
                    .borrow()
                    .items
                    .clone()
                    .into_iter();
                let mut acc = items.next().ok_or_else(|| h::err("reduce: empty set"))?;
                for item in items {
                    acc = h::call_closure(_v, &f, vec![acc, item])?;
                }
                Ok(acc)
            }),
        ),
        (
            "join",
            method(|_v, t, args, _n| {
                let sep = args.first().and_then(h::as_str).unwrap_or("");
                let s = h::as_set(&t).ok_or_else(|| h::err("join: expected a Set"))?;
                Ok(Value::str(
                    s.borrow()
                        .items
                        .iter()
                        .map(value_display)
                        .collect::<Vec<_>>()
                        .join(sep),
                ))
            }),
        ),
        (
            "followedBy",
            method(|_v, t, args, _n| {
                let other = args
                    .first()
                    .ok_or_else(|| h::err("followedBy expects an Iterable"))?;
                let s = h::as_set(&t).ok_or_else(|| h::err("followedBy: expected a Set"))?;
                let mut out = s.borrow().items.clone();
                out.extend(iterable_items(other, _v)?);
                Ok(list_value(out))
            }),
        ),
        ("cast", method(|_v, t, _a, _n| Ok(t))),
    ];
    for (k, m) in set_methods {
        bc.methods.insert(k.into(), m);
    }
    bc.getters.insert(
        "length".into(),
        getter(|_v, t| {
            Ok(Value::Int(
                h::as_set(&t)
                    .ok_or_else(|| h::err("length: expected a Set"))?
                    .borrow()
                    .len() as i64,
            ))
        }),
    );
    bc.getters.insert(
        "isEmpty".into(),
        getter(|_v, t| {
            Ok(Value::Bool(
                h::as_set(&t)
                    .ok_or_else(|| h::err("isEmpty: expected a Set"))?
                    .borrow()
                    .is_empty(),
            ))
        }),
    );
    bc.getters.insert(
        "isNotEmpty".into(),
        getter(|_v, t| {
            Ok(Value::Bool(
                !h::as_set(&t)
                    .ok_or_else(|| h::err("isNotEmpty: expected a Set"))?
                    .borrow()
                    .is_empty(),
            ))
        }),
    );
    bc.getters.insert(
        "first".into(),
        getter(|_v, t| {
            h::as_set(&t)
                .ok_or_else(|| h::err("first: expected a Set"))?
                .borrow()
                .items
                .first()
                .cloned()
                .ok_or_else(|| h::err("first: empty set"))
        }),
    );
    bc.getters.insert(
        "last".into(),
        getter(|_v, t| {
            h::as_set(&t)
                .ok_or_else(|| h::err("last: expected a Set"))?
                .borrow()
                .items
                .last()
                .cloned()
                .ok_or_else(|| h::err("last: empty set"))
        }),
    );
    bc.getters.insert(
        "single".into(),
        getter(|_v, t| {
            let set = h::as_set(&t).ok_or_else(|| h::err("single: expected a Set"))?;
            let items = &set.borrow().items;
            match items.len() {
                1 => Ok(items[0].clone()),
                0 => Err(h::err("single: empty set")),
                _ => Err(h::err("single: too many elements")),
            }
        }),
    );
    bc.getters.insert(
        "iterator".into(),
        getter(|_v, t| {
            let items = h::as_set(&t)
                .ok_or_else(|| h::err("iterator: expected a Set"))?
                .borrow()
                .items
                .clone();
            Ok(h::wrap(&h::class("Iterator"), list_value(items)))
        }),
    );
    bc
}

pub fn iterable_definition() -> BridgedClassDef {
    let mut bc = BridgedClassDef::new("Iterable");
    bc.is_subtype_of_fn = Some(Box::new(|v| {
        matches!(v, Value::List(_) | Value::Set(_) | Value::SyncGen(_))
    }));
    bc.static_methods.insert(
        "generate".into(),
        static_method(|_v, args, _n| {
            let count = h::arg_int(&args, 0, "generate")?;
            let generator = args.get(1).cloned();
            let mut out = Vec::new();
            for i in 0..count {
                match &generator {
                    Some(f) => out.push(h::call_closure(_v, f, vec![Value::Int(i)])?),
                    None => out.push(Value::Null),
                }
            }
            Ok(list_value(out))
        }),
    );
    bc.static_methods.insert(
        "empty".into(),
        static_method(|_v, _a, _n| Ok(list_value(vec![]))),
    );
    let iterable_methods: Vec<(&str, M)> = vec![
        (
            "map",
            method(|_v, t, args, _n| {
                let f = args
                    .first()
                    .cloned()
                    .ok_or_else(|| h::err("map expects a function"))?;
                let mut out = Vec::new();
                for item in iterable_items(&t, _v)? {
                    out.push(h::call_closure(_v, &f, vec![item])?);
                }
                Ok(list_value(out))
            }),
        ),
        (
            "where",
            method(|_v, t, args, _n| {
                let f = args
                    .first()
                    .cloned()
                    .ok_or_else(|| h::err("where expects a function"))?;
                let mut out = Vec::new();
                for item in iterable_items(&t, _v)? {
                    let r = h::call_closure(_v, &f, vec![item.clone()])?;
                    if h::as_bool(&r).unwrap_or(false) {
                        out.push(item);
                    }
                }
                Ok(list_value(out))
            }),
        ),
        (
            "expand",
            method(|_v, t, args, _n| {
                let f = args
                    .first()
                    .cloned()
                    .ok_or_else(|| h::err("expand expects a function"))?;
                let mut out = Vec::new();
                for item in iterable_items(&t, _v)? {
                    let r = h::call_closure(_v, &f, vec![item])?;
                    out.extend(iterable_items(&r, _v)?);
                }
                Ok(list_value(out))
            }),
        ),
        (
            "contains",
            method(|_v, t, args, _n| {
                let needle = args.first().cloned().unwrap_or(Value::Null);
                Ok(Value::Bool(
                    iterable_items(&t, _v)?.iter().any(|i| value_eq(i, &needle)),
                ))
            }),
        ),
        (
            "forEach",
            method(|_v, t, args, _n| {
                let f = args
                    .first()
                    .cloned()
                    .ok_or_else(|| h::err("forEach expects a function"))?;
                for item in iterable_items(&t, _v)? {
                    h::call_closure(_v, &f, vec![item])?;
                }
                Ok(Value::Null)
            }),
        ),
        (
            "reduce",
            method(|_v, t, args, _n| {
                let f = args
                    .first()
                    .cloned()
                    .ok_or_else(|| h::err("reduce expects a function"))?;
                let mut items = iterable_items(&t, _v)?.into_iter();
                let mut acc = items
                    .next()
                    .ok_or_else(|| h::err("reduce: empty iterable"))?;
                for item in items {
                    acc = h::call_closure(_v, &f, vec![acc, item])?;
                }
                Ok(acc)
            }),
        ),
        (
            "fold",
            method(|_v, t, args, _n| {
                let initial = args.first().cloned().unwrap_or(Value::Null);
                let f = args
                    .get(1)
                    .cloned()
                    .ok_or_else(|| h::err("fold expects a function"))?;
                let mut acc = initial;
                for item in iterable_items(&t, _v)? {
                    acc = h::call_closure(_v, &f, vec![acc, item])?;
                }
                Ok(acc)
            }),
        ),
        (
            "every",
            method(|_v, t, args, _n| {
                let f = args
                    .first()
                    .cloned()
                    .ok_or_else(|| h::err("every expects a function"))?;
                for item in iterable_items(&t, _v)? {
                    let r = h::call_closure(_v, &f, vec![item])?;
                    if !h::as_bool(&r).unwrap_or(false) {
                        return Ok(Value::Bool(false));
                    }
                }
                Ok(Value::Bool(true))
            }),
        ),
        (
            "join",
            method(|_v, t, args, _n| {
                let sep = args.first().and_then(h::as_str).unwrap_or("");
                Ok(Value::str(
                    iterable_items(&t, _v)?
                        .iter()
                        .map(value_display)
                        .collect::<Vec<_>>()
                        .join(sep),
                ))
            }),
        ),
        (
            "any",
            method(|_v, t, args, _n| {
                let f = args
                    .first()
                    .cloned()
                    .ok_or_else(|| h::err("any expects a function"))?;
                for item in iterable_items(&t, _v)? {
                    let r = h::call_closure(_v, &f, vec![item])?;
                    if h::as_bool(&r).unwrap_or(false) {
                        return Ok(Value::Bool(true));
                    }
                }
                Ok(Value::Bool(false))
            }),
        ),
        (
            "toList",
            method(|_v, t, _a, _n| Ok(list_value(iterable_items(&t, _v)?))),
        ),
        (
            "toSet",
            method(|_v, t, _a, _n| {
                let mut set = DartSet::new();
                for item in iterable_items(&t, _v)? {
                    set.insert(item);
                }
                Ok(Value::Set(Rc::new(RefCell::new(set))))
            }),
        ),
        (
            "take",
            method(|_v, t, args, _n| {
                let n = h::arg_int(&args, 0, "take")?;
                Ok(list_value(
                    iterable_items(&t, _v)?
                        .into_iter()
                        .take(n.max(0) as usize)
                        .collect(),
                ))
            }),
        ),
        (
            "takeWhile",
            method(|_v, t, args, _n| {
                let f = args
                    .first()
                    .cloned()
                    .ok_or_else(|| h::err("takeWhile expects a function"))?;
                let mut out = Vec::new();
                for item in iterable_items(&t, _v)? {
                    let r = h::call_closure(_v, &f, vec![item.clone()])?;
                    if !h::as_bool(&r).unwrap_or(false) {
                        break;
                    }
                    out.push(item);
                }
                Ok(list_value(out))
            }),
        ),
        (
            "skip",
            method(|_v, t, args, _n| {
                let n = h::arg_int(&args, 0, "skip")?;
                Ok(list_value(
                    iterable_items(&t, _v)?
                        .into_iter()
                        .skip(n.max(0) as usize)
                        .collect(),
                ))
            }),
        ),
        (
            "skipWhile",
            method(|_v, t, args, _n| {
                let f = args
                    .first()
                    .cloned()
                    .ok_or_else(|| h::err("skipWhile expects a function"))?;
                let mut skipped = false;
                let mut out = Vec::new();
                for item in iterable_items(&t, _v)? {
                    let r = h::call_closure(_v, &f, vec![item.clone()])?;
                    if !skipped && h::as_bool(&r).unwrap_or(false) {
                        continue;
                    }
                    skipped = true;
                    out.push(item);
                }
                Ok(list_value(out))
            }),
        ),
        (
            "firstWhere",
            method(|_v, t, args, named| {
                let f = args
                    .first()
                    .cloned()
                    .ok_or_else(|| h::err("firstWhere expects a function"))?;
                for item in iterable_items(&t, _v)? {
                    let r = h::call_closure(_v, &f, vec![item.clone()])?;
                    if h::as_bool(&r).unwrap_or(false) {
                        return Ok(item);
                    }
                }
                match named.get("orElse") {
                    Some(or) => h::call_closure(_v, or, vec![]),
                    None => Err(h::err("firstWhere: no element found")),
                }
            }),
        ),
        (
            "lastWhere",
            method(|_v, t, args, named| {
                let f = args
                    .first()
                    .cloned()
                    .ok_or_else(|| h::err("lastWhere expects a function"))?;
                let mut found = None;
                for item in iterable_items(&t, _v)? {
                    let r = h::call_closure(_v, &f, vec![item.clone()])?;
                    if h::as_bool(&r).unwrap_or(false) {
                        found = Some(item);
                    }
                }
                match found {
                    Some(item) => Ok(item),
                    None => match named.get("orElse") {
                        Some(or) => h::call_closure(_v, or, vec![]),
                        None => Err(h::err("lastWhere: no element found")),
                    },
                }
            }),
        ),
        (
            "singleWhere",
            method(|_v, t, args, named| {
                let f = args
                    .first()
                    .cloned()
                    .ok_or_else(|| h::err("singleWhere expects a function"))?;
                let mut found = None;
                for item in iterable_items(&t, _v)? {
                    let r = h::call_closure(_v, &f, vec![item.clone()])?;
                    if h::as_bool(&r).unwrap_or(false) {
                        if found.is_some() {
                            return Err(h::err("singleWhere: more than one element found"));
                        }
                        found = Some(item);
                    }
                }
                match found {
                    Some(item) => Ok(item),
                    None => match named.get("orElse") {
                        Some(or) => h::call_closure(_v, or, vec![]),
                        None => Err(h::err("singleWhere: no element found")),
                    },
                }
            }),
        ),
        (
            "elementAt",
            method(|_v, t, args, _n| {
                let i = h::arg_int(&args, 0, "elementAt")?;
                iterable_items(&t, _v)?
                    .get(i as usize)
                    .cloned()
                    .ok_or_else(|| h::err(format!("elementAt: index out of range {i}")))
            }),
        ),
        (
            "followedBy",
            method(|_v, t, args, _n| {
                let other = args
                    .first()
                    .ok_or_else(|| h::err("followedBy expects an Iterable"))?;
                let mut out = iterable_items(&t, _v)?;
                out.extend(iterable_items(other, _v)?);
                Ok(list_value(out))
            }),
        ),
        (
            "whereType",
            method(|_v, t, _a, _n| Ok(list_value(iterable_items(&t, _v)?))),
        ),
        ("cast", method(|_v, t, _a, _n| Ok(t))),
    ];
    for (k, m) in iterable_methods {
        bc.methods.insert(k.into(), m);
    }
    bc.getters.insert(
        "length".into(),
        getter(|_v, t| Ok(Value::Int(iterable_items(&t, _v)?.len() as i64))),
    );
    bc.getters.insert(
        "isEmpty".into(),
        getter(|_v, t| Ok(Value::Bool(iterable_items(&t, _v)?.is_empty()))),
    );
    bc.getters.insert(
        "isNotEmpty".into(),
        getter(|_v, t| Ok(Value::Bool(!iterable_items(&t, _v)?.is_empty()))),
    );
    bc.getters.insert(
        "first".into(),
        getter(|_v, t| {
            iterable_items(&t, _v)?
                .first()
                .cloned()
                .ok_or_else(|| h::err("first: empty iterable"))
        }),
    );
    bc.getters.insert(
        "last".into(),
        getter(|_v, t| {
            iterable_items(&t, _v)?
                .last()
                .cloned()
                .ok_or_else(|| h::err("last: empty iterable"))
        }),
    );
    bc.getters.insert(
        "single".into(),
        getter(|_v, t| {
            let items = iterable_items(&t, _v)?;
            match items.len() {
                1 => Ok(items[0].clone()),
                0 => Err(h::err("single: empty iterable")),
                _ => Err(h::err("single: too many elements")),
            }
        }),
    );
    bc.getters.insert(
        "iterator".into(),
        getter(|_v, t| {
            let items = iterable_items(&t, _v)?;
            Ok(h::wrap(&h::class("Iterator"), list_value(items)))
        }),
    );
    bc
}

pub fn iterator_definition() -> BridgedClassDef {
    let mut bc = BridgedClassDef::new("Iterator");
    bc.methods.insert(
        "moveNext".into(),
        method(|_v, t, _a, _n| {
            let l = h::as_list(&t).ok_or_else(|| h::err("moveNext: not an Iterator"))?;
            let mut items = l.borrow_mut();
            if items.is_empty() {
                Ok(Value::Bool(false))
            } else {
                items.remove(0);
                Ok(Value::Bool(true))
            }
        }),
    );
    bc.getters.insert(
        "current".into(),
        getter(|_v, t| {
            let l = h::as_list(&t).ok_or_else(|| h::err("current: not an Iterator"))?;
            Ok(l.borrow().first().cloned().unwrap_or(Value::Null))
        }),
    );
    bc.getters.insert(
        "hashCode".into(),
        getter(|_v, t| Ok(Value::Int(h::hash_code(&t)))),
    );
    bc.getters.insert(
        "runtimeType".into(),
        getter(|_v, _t| Ok(Value::Type(crate::rt::RuntimeType::named("Iterator")))),
    );
    bc
}
