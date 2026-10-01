use super::super::helpers as h;
use crate::class::BridgedClassDef;
use crate::stdlib::core::string::{as_pattern_obj, match_value, pattern_matches};
use crate::value::Value;

fn native_list(v: &Value) -> Option<std::rc::Rc<std::cell::RefCell<Vec<Value>>>> {
    match v {
        Value::List(l) => Some(l.clone()),
        Value::Bridged(b) => match &b.borrow().native {
            Value::List(l) => Some(l.clone()),
            _ => None,
        },
        _ => None,
    }
}

pub fn pattern_definition() -> BridgedClassDef {
    let mut bc = BridgedClassDef::new("Pattern");
    bc.is_subtype_of_fn = Some(Box::new(|v| matches!(v, Value::Str(_))));
    bc.methods.insert(
        "allMatches".into(),
        h::method(|_v, t, args, _n| {
            let input = h::arg_str(&args, 0, "allMatches")?;
            let start = args.get(1).and_then(h::as_int).unwrap_or(0);
            let pattern =
                as_pattern_obj(&t).ok_or_else(|| h::err("allMatches expects a Pattern"))?;
            let mut items = Vec::new();
            for (s, e) in pattern_matches(input, &pattern, start.max(0) as usize) {
                items.push(match_value(input, "", s, e));
            }
            Ok(Value::List(std::rc::Rc::new(std::cell::RefCell::new(
                items,
            ))))
        }),
    );
    bc.methods.insert(
        "matchAsPrefix".into(),
        h::method(|_v, t, args, _n| {
            let input = h::arg_str(&args, 0, "matchAsPrefix")?;
            let start = args.get(1).and_then(h::as_int).unwrap_or(0);
            let pattern =
                as_pattern_obj(&t).ok_or_else(|| h::err("matchAsPrefix expects a Pattern"))?;
            let m = pattern_matches(input, &pattern, start.max(0) as usize)
                .first()
                .cloned();
            Ok(match m {
                Some((s, e)) => match_value(input, "", s, e),
                None => Value::Null,
            })
        }),
    );
    bc.methods.insert(
        "hashCode".into(),
        h::method(|_v, t, _a, _n| Ok(Value::Int(h::hash_code(&t)))),
    );
    bc.methods.insert(
        "toString".into(),
        h::method(|_v, t, _a, _n| Ok(Value::str(h::as_str(&t).unwrap_or_default()))),
    );
    bc.getters.insert(
        "hashCode".into(),
        h::getter(|_v, t| Ok(Value::Int(h::hash_code(&t)))),
    );
    bc.getters.insert(
        "runtimeType".into(),
        h::getter(|_v, t| Ok(h::runtime_type(&t))),
    );
    bc
}

pub fn match_definition() -> BridgedClassDef {
    let mut bc = BridgedClassDef::new("Match");
    bc.methods.insert(
        "group".into(),
        h::method(|_v, t, args, _n| {
            let i = h::arg_int(&args, 0, "group")?;
            let l = native_list(&t).ok_or_else(|| h::err("Match.group: bad match"))?;
            let groups = l.borrow();
            if let Some(Value::List(g)) = groups.get(4) {
                return Ok(g.borrow().get(i as usize).cloned().unwrap_or(Value::Null));
            }
            Ok(Value::Null)
        }),
    );
    bc.methods.insert(
        "groups".into(),
        h::method(|_v, t, args, _n| {
            let l = native_list(&t).ok_or_else(|| h::err("Match.groups: bad match"))?;
            let list = h::as_list(&args.first().cloned().unwrap_or(Value::Null))
                .ok_or_else(|| h::err("Match.groups expects a List<int>"))?;
            let indices: Vec<i64> = list.borrow().iter().filter_map(h::as_int).collect();
            let groups = l.borrow();
            let mut out = Vec::new();
            if let Some(Value::List(g)) = groups.get(4) {
                let g = g.borrow();
                for i in indices {
                    out.push(g.get(i as usize).cloned().unwrap_or(Value::Null));
                }
            }
            Ok(Value::List(std::rc::Rc::new(std::cell::RefCell::new(out))))
        }),
    );
    bc.methods.insert(
        "[]".into(),
        h::method(|_v, t, args, _n| {
            if args.len() != 1 {
                return Err(h::err(
                    "Match [] requires one integer argument (group index).",
                ));
            }
            let i = h::arg_int(&args, 0, "[]")?;
            let l = native_list(&t).ok_or_else(|| h::err("Match []: bad match"))?;
            let groups = l.borrow();
            if let Some(Value::List(g)) = groups.get(4) {
                return Ok(g.borrow().get(i as usize).cloned().unwrap_or(Value::Null));
            }
            Ok(Value::Null)
        }),
    );
    bc.methods.insert(
        "toString".into(),
        h::method(|_v, t, _a, _n| {
            let l = native_list(&t).ok_or_else(|| h::err("Match.toString: bad match"))?;
            let groups = l.borrow();
            Ok(groups.get(3).cloned().unwrap_or(Value::Null))
        }),
    );
    bc.methods.insert(
        "hashCode".into(),
        h::method(|_v, t, _a, _n| Ok(Value::Int(h::hash_code(&t)))),
    );
    bc.getters.insert(
        "start".into(),
        h::getter(|_v, t| {
            let l = native_list(&t).ok_or_else(|| h::err("Match.start: bad match"))?;
            Ok(l.borrow().first().cloned().unwrap_or(Value::Null))
        }),
    );
    bc.getters.insert(
        "end".into(),
        h::getter(|_v, t| {
            let l = native_list(&t).ok_or_else(|| h::err("Match.end: bad match"))?;
            Ok(l.borrow().get(1).cloned().unwrap_or(Value::Null))
        }),
    );
    bc.getters.insert(
        "input".into(),
        h::getter(|_v, t| {
            let l = native_list(&t).ok_or_else(|| h::err("Match.input: bad match"))?;
            Ok(l.borrow().get(2).cloned().unwrap_or(Value::Null))
        }),
    );
    bc.getters.insert(
        "pattern".into(),
        h::getter(|_v, t| {
            let l = native_list(&t).ok_or_else(|| h::err("Match.pattern: bad match"))?;
            Ok(l.borrow().get(5).cloned().unwrap_or(Value::Null))
        }),
    );
    bc.getters.insert(
        "groupCount".into(),
        h::getter(|_v, t| {
            let l = native_list(&t).ok_or_else(|| h::err("Match.groupCount: bad match"))?;
            let groups = l.borrow();
            if let Some(Value::List(g)) = groups.get(4) {
                return Ok(Value::Int(g.borrow().len().saturating_sub(1) as i64));
            }
            Ok(Value::Int(0))
        }),
    );
    bc.getters.insert(
        "hashCode".into(),
        h::getter(|_v, t| Ok(Value::Int(h::hash_code(&t)))),
    );
    bc.getters.insert(
        "runtimeType".into(),
        h::getter(|_v, t| Ok(h::runtime_type(&t))),
    );
    bc
}
