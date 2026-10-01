use super::super::helpers as h;
use crate::class::BridgedClassDef;
use crate::error::InterpError;
use crate::value::Value;
use regex::Regex;
use std::cell::RefCell;
use std::rc::Rc;

fn list(items: Vec<Value>) -> Value {
    Value::List(Rc::new(RefCell::new(items)))
}

fn re_match_value(
    input: &str,
    start: usize,
    end: usize,
    groups: Vec<Value>,
    regexp_native: Rc<RefCell<Vec<Value>>>,
) -> Value {
    h::wrap(
        &h::class("RegExpMatch"),
        Value::List(Rc::new(RefCell::new(vec![
            Value::Int(start as i64),
            Value::Int(end as i64),
            Value::str(input),
            Value::List(Rc::new(RefCell::new(groups))),
            Value::List(regexp_native),
        ]))),
    )
}

fn flags_of(v: &Value) -> (String, bool, bool, bool, bool) {
    let l = h::as_list(v).unwrap_or_else(|| Rc::new(RefCell::new(vec![])));
    let items = l.borrow();
    let pattern = items
        .first()
        .and_then(h::as_str)
        .unwrap_or_default()
        .to_string();
    let multi_line = items.get(1).and_then(h::as_bool).unwrap_or(false);
    let case_sensitive = items.get(2).and_then(h::as_bool).unwrap_or(true);
    let unicode = items.get(3).and_then(h::as_bool).unwrap_or(false);
    let dot_all = items.get(4).and_then(h::as_bool).unwrap_or(false);
    (pattern, multi_line, case_sensitive, unicode, dot_all)
}

fn group_names_of(v: &Value) -> Vec<String> {
    let l = h::as_list(v).unwrap_or_else(|| Rc::new(RefCell::new(vec![])));
    let items = l.borrow();
    if let Some(Value::List(names)) = items.get(5) {
        return names
            .borrow()
            .iter()
            .filter_map(h::as_str)
            .map(|s| s.to_string())
            .collect();
    }
    vec![]
}

fn compile_regex(v: &Value) -> Result<Regex, InterpError> {
    let (pattern, multi_line, case_sensitive, _unicode, dot_all) = flags_of(v);
    let mut builder = regex::RegexBuilder::new(&pattern);
    builder.multi_line(multi_line);
    builder.case_insensitive(!case_sensitive);
    builder.dot_matches_new_line(dot_all);
    builder.build().map_err(|e| {
        h::format_exception(format!("Error parsing regular expression '{pattern}': {e}"))
    })
}

fn byte_offset(s: &str, char_idx: usize) -> usize {
    s.char_indices()
        .nth(char_idx)
        .map(|(b, _)| b)
        .unwrap_or(s.len())
}

fn char_index(s: &str, byte_idx: usize) -> usize {
    s[..byte_idx.min(s.len())].chars().count()
}

fn groups_of(v: &Value) -> Option<Rc<RefCell<Vec<Value>>>> {
    let l = h::as_list(v)?;
    let items = l.borrow();
    if let Some(Value::List(g)) = items.get(3) {
        Some(g.clone())
    } else {
        None
    }
}

fn count_groups(pattern: &str) -> i64 {
    let chars: Vec<char> = pattern.chars().collect();
    let mut count = 0i64;
    let mut i = 0;
    while i < chars.len() {
        match chars[i] {
            '\\' => i += 2,
            '(' => {
                let next = chars.get(i + 1).copied();
                match next {
                    Some('?') => {
                        let after = chars.get(i + 2).copied();
                        match after {
                            Some('<') => {
                                let look = chars.get(i + 3).copied();
                                if look == Some('=') || look == Some('!') {
                                    i += 4;
                                } else {
                                    count += 1;
                                    while i < chars.len() && chars[i] != '>' {
                                        i += 1;
                                    }
                                    i += 1;
                                }
                            }
                            Some('=') | Some('!') | Some(':') => i += 3,
                            _ => {
                                count += 1;
                                i += 2;
                            }
                        }
                    }
                    _ => {
                        count += 1;
                        i += 1;
                    }
                }
            }
            _ => i += 1,
        }
    }
    count
}

pub fn regexp_definition() -> BridgedClassDef {
    let mut bc = BridgedClassDef::new("RegExp");
    bc.constructors.insert(
        "".into(),
        h::ctor(|_v, args, named| {
            let source = h::arg_str(&args, 0, "RegExp")?;
            let multi_line = named.get("multiLine").and_then(h::as_bool).unwrap_or(false);
            let case_sensitive = named
                .get("caseSensitive")
                .and_then(h::as_bool)
                .unwrap_or(true);
            let unicode = named.get("unicode").and_then(h::as_bool).unwrap_or(false);
            let dot_all = named.get("dotAll").and_then(h::as_bool).unwrap_or(false);
            let group_names: Vec<Value> = regex::RegexBuilder::new(source)
                .multi_line(multi_line)
                .case_insensitive(!case_sensitive)
                .dot_matches_new_line(dot_all)
                .build()
                .map(|r| r.capture_names().flatten().map(Value::str).collect())
                .map_err(|e| {
                    h::format_exception(format!("Error parsing regular expression '{source}': {e}"))
                })?;
            let native = list(vec![
                Value::str(source),
                Value::Bool(multi_line),
                Value::Bool(case_sensitive),
                Value::Bool(unicode),
                Value::Bool(dot_all),
                list(group_names),
            ]);
            Ok(h::wrap(&h::class("RegExp"), native))
        }),
    );
    bc.static_methods.insert(
        "escape".into(),
        h::static_method(|_v, args, _n| {
            let s = h::arg_str(&args, 0, "escape")?;
            let mut out = String::new();
            for c in s.chars() {
                if c.is_ascii_alphanumeric() || c == '_' {
                    out.push(c);
                } else {
                    out.push('\\');
                    out.push(c);
                }
            }
            Ok(Value::str(out))
        }),
    );
    bc.methods.insert(
        "hasMatch".into(),
        h::method(|_v, t, args, _n| {
            let input = h::arg_str(&args, 0, "hasMatch")?;
            let re = compile_regex(&t)?;
            Ok(Value::Bool(re.is_match(input)))
        }),
    );
    bc.methods.insert(
        "firstMatch".into(),
        h::method(|_v, t, args, _n| {
            let input = h::arg_str(&args, 0, "firstMatch")?;
            let re = compile_regex(&t)?;
            let native = h::as_list(&t).unwrap_or_else(|| Rc::new(RefCell::new(vec![])));
            match re.captures(input) {
                Some(caps) => {
                    let start = caps
                        .get(0)
                        .map(|m| char_index(input, m.start()))
                        .unwrap_or(0);
                    let end = caps.get(0).map(|m| char_index(input, m.end())).unwrap_or(0);
                    let mut groups = Vec::new();
                    for i in 0..caps.len() {
                        groups.push(
                            caps.get(i)
                                .map(|m| Value::str(m.as_str()))
                                .unwrap_or(Value::Null),
                        );
                    }
                    Ok(re_match_value(input, start, end, groups, native))
                }
                None => Ok(Value::Null),
            }
        }),
    );
    bc.methods.insert(
        "allMatches".into(),
        h::method(|_v, t, args, _n| {
            let input = h::arg_str(&args, 0, "allMatches")?;
            let start = args.get(1).and_then(h::as_int).unwrap_or(0).max(0) as usize;
            let re = compile_regex(&t)?;
            let native = h::as_list(&t).unwrap_or_else(|| Rc::new(RefCell::new(vec![])));
            let rest = &input[byte_offset(input, start)..];
            let mut out = Vec::new();
            for caps in re.captures_iter(rest) {
                let s0 = caps
                    .get(0)
                    .map(|m| char_index(rest, m.start()))
                    .unwrap_or(0);
                let e0 = caps.get(0).map(|m| char_index(rest, m.end())).unwrap_or(0);
                let mut groups = Vec::new();
                for i in 0..caps.len() {
                    groups.push(
                        caps.get(i)
                            .map(|m| Value::str(m.as_str()))
                            .unwrap_or(Value::Null),
                    );
                }
                out.push(re_match_value(
                    input,
                    start + s0,
                    start + e0,
                    groups,
                    native.clone(),
                ));
            }
            Ok(list(out))
        }),
    );
    bc.methods.insert(
        "stringMatch".into(),
        h::method(|_v, t, args, _n| {
            let input = h::arg_str(&args, 0, "stringMatch")?;
            let re = compile_regex(&t)?;
            Ok(match re.find(input) {
                Some(m) => Value::str(m.as_str()),
                None => Value::str(""),
            })
        }),
    );
    bc.methods.insert(
        "matchAsPrefix".into(),
        h::method(|_v, t, args, _n| {
            let input = h::arg_str(&args, 0, "matchAsPrefix")?;
            let start = args.get(1).and_then(h::as_int).unwrap_or(0).max(0) as usize;
            let re = compile_regex(&t)?;
            let native = h::as_list(&t).unwrap_or_else(|| Rc::new(RefCell::new(vec![])));
            let rest = &input[byte_offset(input, start)..];
            match re.captures(rest) {
                Some(caps) if caps.get(0).is_some_and(|m| m.start() == 0) => {
                    let s0 = 0;
                    let e0 = caps.get(0).map(|m| char_index(rest, m.end())).unwrap_or(0);
                    let mut groups = Vec::new();
                    for i in 0..caps.len() {
                        groups.push(
                            caps.get(i)
                                .map(|m| Value::str(m.as_str()))
                                .unwrap_or(Value::Null),
                        );
                    }
                    Ok(re_match_value(
                        input,
                        start + s0,
                        start + e0,
                        groups,
                        native,
                    ))
                }
                _ => Ok(Value::Null),
            }
        }),
    );
    bc.methods.insert(
        "toString".into(),
        h::method(|_v, t, _a, _n| {
            let (pattern, multi_line, case_sensitive, unicode, dot_all) = flags_of(&t);
            let mut out = String::from("/");
            out.push_str(&pattern);
            out.push('/');
            if !case_sensitive {
                out.push('i');
            }
            if multi_line {
                out.push('m');
            }
            if unicode {
                out.push('u');
            }
            if dot_all {
                out.push('s');
            }
            Ok(Value::str(out))
        }),
    );
    bc.methods.insert(
        "==".into(),
        h::method(|_v, t, args, _n| {
            let other = args.first().cloned().unwrap_or(Value::Null);
            let (pa, ma, ca, ua, da) = flags_of(&t);
            let (pb, mb, cb, ub, db) = flags_of(&other);
            Ok(Value::Bool(
                pa == pb && ma == mb && ca == cb && ua == ub && da == db,
            ))
        }),
    );
    bc.getters.insert(
        "pattern".into(),
        h::getter(|_v, t| Ok(Value::str(flags_of(&t).0))),
    );
    bc.getters.insert(
        "isMultiLine".into(),
        h::getter(|_v, t| Ok(Value::Bool(flags_of(&t).1))),
    );
    bc.getters.insert(
        "isCaseSensitive".into(),
        h::getter(|_v, t| Ok(Value::Bool(flags_of(&t).2))),
    );
    bc.getters.insert(
        "isUnicode".into(),
        h::getter(|_v, t| Ok(Value::Bool(flags_of(&t).3))),
    );
    bc.getters.insert(
        "isDotAll".into(),
        h::getter(|_v, t| Ok(Value::Bool(flags_of(&t).4))),
    );
    bc.getters.insert(
        "groupCount".into(),
        h::getter(|_v, t| Ok(Value::Int(count_groups(&flags_of(&t).0)))),
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

pub fn regexp_match_definition() -> BridgedClassDef {
    let mut bc = BridgedClassDef::new("RegExpMatch");
    bc.methods.insert(
        "group".into(),
        h::method(|_v, t, args, _n| {
            let i = h::arg_int(&args, 0, "group")?;
            let g = groups_of(&t).ok_or_else(|| h::err("RegExpMatch.group: bad match"))?;
            Ok(g.borrow().get(i as usize).cloned().unwrap_or(Value::Null))
        }),
    );
    bc.methods.insert(
        "groups".into(),
        h::method(|_v, t, args, _n| {
            let g = groups_of(&t).ok_or_else(|| h::err("RegExpMatch.groups: bad match"))?;
            let indices: Vec<i64> = h::as_list(&args.first().cloned().unwrap_or(Value::Null))
                .map(|l| l.borrow().iter().filter_map(h::as_int).collect())
                .unwrap_or_default();
            let g = g.borrow();
            let mut out = Vec::new();
            for i in indices {
                out.push(g.get(i as usize).cloned().unwrap_or(Value::Null));
            }
            Ok(list(out))
        }),
    );
    bc.methods.insert(
        "[]".into(),
        h::method(|_v, t, args, _n| {
            if args.len() != 1 {
                return Err(h::err(
                    "RegExpMatch [] requires one integer argument (group index).",
                ));
            }
            let i = h::arg_int(&args, 0, "[]")?;
            let g = groups_of(&t).ok_or_else(|| h::err("RegExpMatch []: bad match"))?;
            Ok(g.borrow().get(i as usize).cloned().unwrap_or(Value::Null))
        }),
    );
    bc.methods.insert(
        "namedGroup".into(),
        h::method(|_v, t, args, _n| {
            let name = h::arg_str(&args, 0, "namedGroup")?;
            let l = h::as_list(&t).ok_or_else(|| h::err("RegExpMatch.namedGroup: bad match"))?;
            let items = l.borrow();
            let regexp_native = items.get(4).cloned().unwrap_or(Value::Null);
            let names = group_names_of(&regexp_native);
            let g = groups_of(&t).ok_or_else(|| h::err("RegExpMatch.namedGroup: bad match"))?;
            let index = names.iter().position(|n| n == name);
            Ok(match index {
                Some(i) => g.borrow().get(i + 1).cloned().unwrap_or(Value::Null),
                None => Value::Null,
            })
        }),
    );
    bc.methods.insert(
        "toString".into(),
        h::method(|_v, t, _a, _n| {
            let g = groups_of(&t).ok_or_else(|| h::err("RegExpMatch.toString: bad match"))?;
            Ok(g.borrow().first().cloned().unwrap_or(Value::Null))
        }),
    );
    bc.methods.insert(
        "hashCode".into(),
        h::method(|_v, t, _a, _n| Ok(Value::Int(h::hash_code(&t)))),
    );
    bc.getters.insert(
        "start".into(),
        h::getter(|_v, t| {
            let l = h::as_list(&t).ok_or_else(|| h::err("RegExpMatch.start: bad match"))?;
            Ok(l.borrow().first().cloned().unwrap_or(Value::Null))
        }),
    );
    bc.getters.insert(
        "end".into(),
        h::getter(|_v, t| {
            let l = h::as_list(&t).ok_or_else(|| h::err("RegExpMatch.end: bad match"))?;
            Ok(l.borrow().get(1).cloned().unwrap_or(Value::Null))
        }),
    );
    bc.getters.insert(
        "input".into(),
        h::getter(|_v, t| {
            let l = h::as_list(&t).ok_or_else(|| h::err("RegExpMatch.input: bad match"))?;
            Ok(l.borrow().get(2).cloned().unwrap_or(Value::Null))
        }),
    );
    bc.getters.insert(
        "groupCount".into(),
        h::getter(|_v, t| {
            let g = groups_of(&t).ok_or_else(|| h::err("RegExpMatch.groupCount: bad match"))?;
            Ok(Value::Int(g.borrow().len().saturating_sub(1) as i64))
        }),
    );
    bc.getters.insert(
        "pattern".into(),
        h::getter(|_v, t| {
            let l = h::as_list(&t).ok_or_else(|| h::err("RegExpMatch.pattern: bad match"))?;
            let regexp_native = l.borrow().get(4).cloned().unwrap_or(Value::Null);
            Ok(h::wrap(&h::class("RegExp"), regexp_native))
        }),
    );
    bc.getters.insert(
        "groupNames".into(),
        h::getter(|_v, t| {
            let l = h::as_list(&t).ok_or_else(|| h::err("RegExpMatch.groupNames: bad match"))?;
            let regexp_native = l.borrow().get(4).cloned().unwrap_or(Value::Null);
            Ok(list(
                group_names_of(&regexp_native)
                    .into_iter()
                    .map(Value::str)
                    .collect(),
            ))
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
