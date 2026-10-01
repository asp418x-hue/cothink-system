use super::super::helpers as h;
use crate::class::BridgedClassDef;
use crate::error::InterpError;
use crate::interp::InterpreterVisitor;
use crate::value::{Value, value_display};
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

pub fn chars_of(v: &Value) -> Vec<char> {
    h::as_str(v).unwrap_or_default().chars().collect()
}

pub enum Pattern {
    Literal(String),
    Regex(regex::Regex),
}

pub fn as_pattern_obj(v: &Value) -> Option<Pattern> {
    match v {
        Value::Str(s) => Some(Pattern::Literal(s.to_string())),
        Value::Bridged(b) => {
            let native = b.borrow().native.clone();
            let l = h::as_list(&native)?;
            let items = l.borrow();
            let source = items.first()?.as_str()?.to_string();
            let multi_line = items.get(1).and_then(h::as_bool).unwrap_or(false);
            let case_sensitive = items.get(2).and_then(h::as_bool).unwrap_or(true);
            let dot_all = items.get(4).and_then(h::as_bool).unwrap_or(false);
            let re = regex::RegexBuilder::new(&source)
                .multi_line(multi_line)
                .case_insensitive(!case_sensitive)
                .dot_matches_new_line(dot_all)
                .build()
                .ok()?;
            Some(Pattern::Regex(re))
        }
        _ => None,
    }
}

fn pattern_source(p: &Pattern) -> String {
    match p {
        Pattern::Literal(s) => s.clone(),
        Pattern::Regex(re) => re.as_str().to_string(),
    }
}

fn pattern_is_empty(p: &Pattern) -> bool {
    matches!(p, Pattern::Literal(s) if s.is_empty())
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

/// Leftmost, non-overlapping matches of `pattern` in `input` starting at char
/// index `start`. Returns (start, end) char-index pairs.
pub fn pattern_matches(input: &str, pattern: &Pattern, start: usize) -> Vec<(usize, usize)> {
    match pattern {
        Pattern::Literal(p) => literal_matches(input, p, start),
        Pattern::Regex(re) => {
            let byte_start = byte_offset(input, start);
            let mut out = Vec::new();
            for m in re.find_iter(&input[byte_start..]) {
                let s = char_index(input, byte_start + m.start());
                let e = char_index(input, byte_start + m.end());
                out.push((s, e));
            }
            out
        }
    }
}

/// Leftmost, non-overlapping string matches of `pattern` in `input` starting
/// at char index `start`. Returns (start, end) char-index pairs.
fn literal_matches(input: &str, pattern: &str, start: usize) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    if pattern.is_empty() {
        let count = input.chars().count();
        for i in start.min(count)..=count {
            out.push((i, i));
        }
        return out;
    }
    let pchars: Vec<char> = pattern.chars().collect();
    let chars: Vec<char> = input.chars().collect();
    let mut i = start.min(chars.len());
    while i + pchars.len() <= chars.len() {
        if chars[i..i + pchars.len()] == pchars[..] {
            out.push((i, i + pchars.len()));
            i += pchars.len();
        } else {
            i += 1;
        }
    }
    out
}

pub fn match_value(input: &str, pattern: &str, start: usize, end: usize) -> Value {
    let input_chars: Vec<char> = input.chars().collect();
    let matched: String = input_chars[start..end].iter().collect();
    let groups = vec![Value::str(matched.clone())];
    h::wrap(
        &h::class("Match"),
        Value::List(Rc::new(RefCell::new(vec![
            Value::Int(start as i64),
            Value::Int(end as i64),
            Value::str(input),
            Value::str(matched),
            Value::List(Rc::new(RefCell::new(groups))),
            Value::str(pattern),
        ]))),
    )
}

fn match_matched(v: &Value) -> String {
    if let Value::Bridged(b) = v
        && let Value::List(l) = &b.borrow().native
    {
        let items = l.borrow();
        if let Some(Value::Str(s)) = items.get(3) {
            return s.to_string();
        }
    }
    String::new()
}

fn native_display(v: &Value) -> String {
    match v {
        Value::Bridged(b) => native_display(&b.borrow().native),
        other => value_display(other),
    }
}

fn string_list(items: Vec<Value>) -> Value {
    Value::List(Rc::new(RefCell::new(items)))
}

pub fn string_definition() -> BridgedClassDef {
    let mut bc = BridgedClassDef::new("String");
    bc.native_type = Some("String".into());
    bc.static_methods.insert(
        "fromCharCode".into(),
        static_method(|_v, args, _n| {
            let code = h::arg_int(&args, 0, "fromCharCode")?;
            let c = char::from_u32(code as u32)
                .ok_or_else(|| h::err(format!("fromCharCode: invalid code point {code}")))?;
            Ok(Value::str(c.to_string()))
        }),
    );
    bc.static_methods.insert(
        "fromCharCodes".into(),
        static_method(|_v, args, _n| {
            let codes = args
                .first()
                .and_then(h::as_list)
                .ok_or_else(|| h::err("fromCharCodes expects a List of ints"))?;
            let start = args.get(1).and_then(h::as_int).unwrap_or(0);
            let end = args.get(2).and_then(h::as_int);
            let items = codes.borrow();
            let end = end.map(|e| e as usize).unwrap_or(items.len());
            let mut out = String::new();
            for c in &items[start.max(0) as usize..end] {
                let code =
                    h::as_int(c).ok_or_else(|| h::err("fromCharCodes expects a List of ints"))?;
                out.push(
                    char::from_u32(code as u32).ok_or_else(|| {
                        h::err(format!("fromCharCodes: invalid code point {code}"))
                    })?,
                );
            }
            Ok(Value::str(out))
        }),
    );
    bc.static_methods.insert(
        "fromEnvironment".into(),
        static_method(|_v, args, named| {
            let name = h::arg_str(&args, 0, "fromEnvironment")?;
            let default = named.get("defaultValue").and_then(h::as_str).unwrap_or("");
            Ok(Value::str(
                std::env::var(name).unwrap_or_else(|_| default.to_string()),
            ))
        }),
    );
    bc.methods.insert(
        "[]".into(),
        method(|_v, t, args, _n| {
            let i = h::arg_int(&args, 0, "[]")?;
            let c = chars_of(&t)
                .get(i as usize)
                .copied()
                .ok_or_else(|| h::err(format!("String index out of range: {i}")))?;
            Ok(Value::str(c.to_string()))
        }),
    );
    bc.methods.insert(
        "substring".into(),
        method(|_v, t, args, _n| {
            let start = h::arg_int(&args, 0, "substring")?;
            let end = args.get(1).and_then(h::as_int);
            let chars = chars_of(&t);
            let s = start.max(0) as usize;
            let e = end.map(|e| e.max(0) as usize).unwrap_or(chars.len());
            if s > chars.len() || e > chars.len() || s > e {
                return Err(h::err(format!(
                    "substring: invalid range {start}..{} (length {})",
                    end.unwrap_or(chars.len() as i64),
                    chars.len()
                )));
            }
            Ok(Value::str(chars[s..e].iter().collect::<String>()))
        }),
    );
    bc.methods.insert(
        "elementAt".into(),
        method(|_v, t, args, _n| {
            let i = h::arg_int(&args, 0, "elementAt")?;
            let c = chars_of(&t)
                .get(i as usize)
                .copied()
                .ok_or_else(|| h::err(format!("elementAt: index out of range {i}")))?;
            Ok(Value::str(c.to_string()))
        }),
    );
    bc.methods.insert(
        "map".into(),
        method(|_v, t, args, _n| {
            let f = args
                .first()
                .cloned()
                .ok_or_else(|| h::err("map expects a function"))?;
            let mut out = Vec::new();
            for c in chars_of(&t) {
                let r = h::call_closure(_v, &f, vec![Value::str(c.to_string())])?;
                out.push(r);
            }
            Ok(string_list(out))
        }),
    );
    bc.methods.insert(
        "where".into(),
        method(|_v, t, args, _n| {
            let f = args
                .first()
                .cloned()
                .ok_or_else(|| h::err("where expects a function"))?;
            let mut out = Vec::new();
            for c in chars_of(&t) {
                let r = h::call_closure(_v, &f, vec![Value::str(c.to_string())])?;
                if h::as_bool(&r).unwrap_or(false) {
                    out.push(Value::str(c.to_string()));
                }
            }
            Ok(string_list(out))
        }),
    );
    bc.methods.insert(
        "join".into(),
        method(|_v, t, args, _n| {
            let sep = args.first().and_then(h::as_str).unwrap_or("");
            let joined = chars_of(&t)
                .iter()
                .map(|c| c.to_string())
                .collect::<Vec<_>>()
                .join(sep);
            Ok(Value::str(joined))
        }),
    );
    bc.methods.insert(
        "toList".into(),
        method(|_v, t, _a, _n| {
            let items: Vec<Value> = chars_of(&t)
                .into_iter()
                .map(|c| Value::str(c.to_string()))
                .collect();
            Ok(Value::List(Rc::new(RefCell::new(items))))
        }),
    );
    bc.methods.insert(
        "toUpperCase".into(),
        method(|_v, t, _a, _n| Ok(Value::str(h::as_str(&t).unwrap_or_default().to_uppercase()))),
    );
    bc.methods.insert(
        "toLowerCase".into(),
        method(|_v, t, _a, _n| Ok(Value::str(h::as_str(&t).unwrap_or_default().to_lowercase()))),
    );
    bc.methods.insert(
        "contains".into(),
        method(|_v, t, args, _n| {
            let pat = args
                .first()
                .and_then(as_pattern_obj)
                .ok_or_else(|| h::err("contains expects a Pattern argument"))?;
            let start = args.get(1).and_then(h::as_int).unwrap_or(0);
            let chars = chars_of(&t);
            let text = h::as_str(&t).unwrap_or_default();
            let count = chars.len();
            let start = (start.max(0) as usize).min(count);
            Ok(Value::Bool(!pattern_matches(text, &pat, start).is_empty()))
        }),
    );
    bc.methods.insert(
        "startsWith".into(),
        method(|_v, t, args, _n| {
            let pat = args
                .first()
                .and_then(as_pattern_obj)
                .ok_or_else(|| h::err("startsWith expects a Pattern argument"))?;
            let start = args.get(1).and_then(h::as_int).unwrap_or(0);
            let text = h::as_str(&t).unwrap_or_default();
            let chars = chars_of(&t);
            let start = (start.max(0) as usize).min(chars.len());
            let matches = pattern_matches(text, &pat, start);
            Ok(Value::Bool(
                matches.first().is_some_and(|(s, _)| *s == start),
            ))
        }),
    );
    bc.methods.insert(
        "endsWith".into(),
        method(|_v, t, args, _n| {
            let other = args
                .first()
                .and_then(h::as_str)
                .ok_or_else(|| h::err("endsWith expects a String argument"))?;
            Ok(Value::Bool(
                h::as_str(&t).unwrap_or_default().ends_with(other),
            ))
        }),
    );
    bc.methods.insert(
        "indexOf".into(),
        method(|_v, t, args, _n| {
            let pat = args
                .first()
                .and_then(as_pattern_obj)
                .ok_or_else(|| h::err("indexOf expects a Pattern argument"))?;
            let start = args.get(1).and_then(h::as_int).unwrap_or(0);
            let text = h::as_str(&t).unwrap_or_default();
            let chars = chars_of(&t);
            let start = (start.max(0) as usize).min(chars.len());
            Ok(Value::Int(
                pattern_matches(text, &pat, start)
                    .first()
                    .map(|(s, _)| *s as i64)
                    .unwrap_or(-1),
            ))
        }),
    );
    bc.methods.insert(
        "lastIndexOf".into(),
        method(|_v, t, args, _n| {
            let pat = args
                .first()
                .and_then(as_pattern_obj)
                .ok_or_else(|| h::err("lastIndexOf expects a Pattern argument"))?;
            let end = args.get(1).and_then(h::as_int);
            let text = h::as_str(&t).unwrap_or_default();
            let chars = chars_of(&t);
            let count = chars.len();
            let end = end.map(|e| (e.max(0) as usize).min(count)).unwrap_or(count);
            let start = if pattern_is_empty(&pat) { end } else { 0 };
            let matches = pattern_matches(text, &pat, start);
            let last = matches
                .iter()
                .rfind(|(s, _)| *s <= end)
                .map(|(s, _)| *s as i64)
                .unwrap_or(-1);
            Ok(Value::Int(last))
        }),
    );
    bc.methods.insert(
        "trim".into(),
        method(|_v, t, _a, _n| Ok(Value::str(h::as_str(&t).unwrap_or_default().trim()))),
    );
    bc.methods.insert(
        "trimLeft".into(),
        method(|_v, t, _a, _n| Ok(Value::str(h::as_str(&t).unwrap_or_default().trim_start()))),
    );
    bc.methods.insert(
        "trimRight".into(),
        method(|_v, t, _a, _n| Ok(Value::str(h::as_str(&t).unwrap_or_default().trim_end()))),
    );
    bc.methods.insert(
        "replaceAll".into(),
        method(|_v, t, args, _n| {
            let pat = args
                .first()
                .and_then(as_pattern_obj)
                .ok_or_else(|| h::err("replaceAll expects a Pattern argument"))?;
            let repl = args
                .get(1)
                .and_then(h::as_str)
                .ok_or_else(|| h::err("replaceAll expects a String replacement"))?;
            let text = h::as_str(&t).unwrap_or_default();
            if pattern_is_empty(&pat) {
                return Ok(Value::str(text.to_string()));
            }
            match &pat {
                Pattern::Literal(p) => Ok(Value::str(text.replace(p.as_str(), repl))),
                Pattern::Regex(_) => {
                    let matches = pattern_matches(text, &pat, 0);
                    if matches.is_empty() {
                        return Ok(Value::str(text));
                    }
                    let mut out = String::new();
                    let mut last = 0usize;
                    for (s, e) in matches {
                        out.push_str(&text_chars(text, last, s));
                        out.push_str(repl);
                        last = e;
                    }
                    out.push_str(&text_chars(text, last, chars_count(text)));
                    Ok(Value::str(out))
                }
            }
        }),
    );
    bc.methods.insert(
        "split".into(),
        method(|_v, t, args, _n| {
            let pat = args
                .first()
                .and_then(as_pattern_obj)
                .ok_or_else(|| h::err("split expects a Pattern argument"))?;
            let text = h::as_str(&t).unwrap_or_default();
            if pattern_is_empty(&pat) {
                return Ok(string_list(
                    text.chars().map(|c| Value::str(c.to_string())).collect(),
                ));
            }
            match &pat {
                Pattern::Literal(p) => Ok(string_list(
                    text.split(p.as_str()).map(Value::str).collect(),
                )),
                Pattern::Regex(_) => {
                    let matches = pattern_matches(text, &pat, 0);
                    let mut out = Vec::new();
                    let mut last = 0usize;
                    for (s, e) in matches {
                        out.push(Value::str(text_chars(text, last, s)));
                        last = e;
                    }
                    out.push(Value::str(text_chars(text, last, chars_count(text))));
                    Ok(string_list(out))
                }
            }
        }),
    );
    bc.methods.insert(
        "splitMapJoin".into(),
        method(|_v, t, args, named| {
            let pat = args
                .first()
                .and_then(as_pattern_obj)
                .ok_or_else(|| h::err("splitMapJoin expects a Pattern argument"))?;
            let on_match = named.get("onMatch").cloned();
            let on_non_match = named.get("onNonMatch").cloned();
            let text = h::as_str(&t).unwrap_or_default();
            let matches = pattern_matches(text, &pat, 0);
            let mut out = String::new();
            let mut last = 0usize;
            for (s, e) in matches {
                let chunk = &text_chars(text, last, s);
                if let Some(f) = &on_non_match {
                    let r = h::call_closure(_v, f, vec![Value::str(chunk)])?;
                    out.push_str(h::as_str(&r).unwrap_or_default());
                } else {
                    out.push_str(chunk);
                }
                let mv = match_value(text, &pattern_source(&pat), s, e);
                if let Some(f) = &on_match {
                    let r = h::call_closure(_v, f, vec![mv])?;
                    out.push_str(h::as_str(&r).unwrap_or_default());
                } else {
                    out.push_str(&match_matched(&mv));
                }
                last = e;
            }
            let tail = text_chars(text, last, chars_count(text));
            if let Some(f) = &on_non_match {
                let r = h::call_closure(_v, f, vec![Value::str(tail)])?;
                out.push_str(h::as_str(&r).unwrap_or_default());
            } else {
                out.push_str(&tail);
            }
            Ok(Value::str(out))
        }),
    );
    bc.methods.insert(
        "padLeft".into(),
        method(|_v, t, args, _n| {
            let width = h::arg_int(&args, 0, "padLeft")?;
            let padding = args.get(1).and_then(h::as_str).unwrap_or(" ");
            Ok(Value::str(pad(chars_of(&t), width, padding, true)))
        }),
    );
    bc.methods.insert(
        "padRight".into(),
        method(|_v, t, args, _n| {
            let width = h::arg_int(&args, 0, "padRight")?;
            let padding = args.get(1).and_then(h::as_str).unwrap_or(" ");
            Ok(Value::str(pad(chars_of(&t), width, padding, false)))
        }),
    );
    bc.methods.insert(
        "replaceFirst".into(),
        method(|_v, t, args, _n| {
            let pat = args
                .first()
                .and_then(as_pattern_obj)
                .ok_or_else(|| h::err("replaceFirst expects a Pattern argument"))?;
            let repl = args
                .get(1)
                .and_then(h::as_str)
                .ok_or_else(|| h::err("replaceFirst expects a String replacement"))?;
            let start = args.get(2).and_then(h::as_int).unwrap_or(0);
            let text = h::as_str(&t).unwrap_or_default();
            let count = chars_count(text);
            let start = (start.max(0) as usize).min(count);
            if pattern_is_empty(&pat) {
                let mut chars: Vec<char> = text.chars().collect();
                let insert_at = start.min(chars.len());
                chars.splice(insert_at..insert_at, repl.chars());
                return Ok(Value::str(chars.into_iter().collect::<String>()));
            }
            match pattern_matches(text, &pat, start).first() {
                Some((s, e)) => {
                    let mut out = String::new();
                    for (i, c) in text.chars().enumerate() {
                        if i == *s {
                            out.push_str(repl);
                        }
                        if !(i >= *s && i < *e) {
                            out.push(c);
                        }
                    }
                    Ok(Value::str(out))
                }
                None => Ok(Value::str(text)),
            }
        }),
    );
    bc.methods.insert(
        "replaceRange".into(),
        method(|_v, t, args, _n| {
            let start = h::arg_int(&args, 0, "replaceRange")?;
            let end = args
                .get(1)
                .and_then(h::as_int)
                .ok_or_else(|| h::err("replaceRange expects start and end"))?;
            let repl = args
                .get(2)
                .and_then(h::as_str)
                .ok_or_else(|| h::err("replaceRange expects a String replacement"))?;
            let chars = chars_of(&t);
            let s = (start.max(0) as usize).min(chars.len());
            let e = (end.max(0) as usize).min(chars.len());
            let mut out: String = chars[..s].iter().collect();
            out.push_str(repl);
            out.extend(chars[e..].iter());
            Ok(Value::str(out))
        }),
    );
    bc.methods.insert(
        "codeUnitAt".into(),
        method(|_v, t, args, _n| {
            let i = h::arg_int(&args, 0, "codeUnitAt")?;
            let c = chars_of(&t)
                .get(i as usize)
                .copied()
                .ok_or_else(|| h::err(format!("codeUnitAt: index out of range {i}")))?;
            Ok(Value::Int(c as i64))
        }),
    );
    bc.methods
        .insert("toString".into(), method(|_v, t, _a, _n| Ok(t)));
    bc.methods.insert(
        "compareTo".into(),
        method(|_v, t, args, _n| {
            let other = args
                .first()
                .and_then(h::as_str)
                .ok_or_else(|| h::err("compareTo expects a String argument"))?;
            Ok(Value::Int(
                match h::as_str(&t).unwrap_or_default().cmp(other) {
                    std::cmp::Ordering::Less => -1,
                    std::cmp::Ordering::Equal => 0,
                    std::cmp::Ordering::Greater => 1,
                },
            ))
        }),
    );
    bc.methods.insert(
        "allMatches".into(),
        method(|_v, t, args, _n| {
            let pat = args
                .first()
                .and_then(as_pattern_obj)
                .ok_or_else(|| h::err("allMatches expects a Pattern argument"))?;
            let start = args.get(1).and_then(h::as_int).unwrap_or(0);
            let text = h::as_str(&t).unwrap_or_default();
            let matches = pattern_matches(text, &pat, (start.max(0)) as usize);
            Ok(string_list(
                matches
                    .into_iter()
                    .map(|(s, e)| match_value(text, &pattern_source(&pat), s, e))
                    .collect(),
            ))
        }),
    );
    bc.methods.insert(
        "matchAsPrefix".into(),
        method(|_v, t, args, _n| {
            let pat = args
                .first()
                .and_then(as_pattern_obj)
                .ok_or_else(|| h::err("matchAsPrefix expects a Pattern argument"))?;
            let start = args.get(1).and_then(h::as_int).unwrap_or(0);
            let text = h::as_str(&t).unwrap_or_default();
            let chars = chars_of(&t);
            let start = (start.max(0) as usize).min(chars.len());
            if pattern_is_empty(&pat) {
                return Ok(match_value(text, &pattern_source(&pat), start, start));
            }
            match pattern_matches(text, &pat, start).first() {
                Some((s, e)) if *s == start => Ok(match_value(text, &pattern_source(&pat), *s, *e)),
                _ => Ok(Value::Null),
            }
        }),
    );
    bc.methods.insert(
        "replaceAllMapped".into(),
        method(|_v, t, args, _n| {
            let pat = args
                .first()
                .and_then(as_pattern_obj)
                .ok_or_else(|| h::err("replaceAllMapped expects a Pattern argument"))?;
            let repl = args
                .get(1)
                .cloned()
                .ok_or_else(|| h::err("replaceAllMapped expects a replace function"))?;
            let text = h::as_str(&t).unwrap_or_default();
            let matches = pattern_matches(text, &pat, 0);
            if matches.is_empty() {
                return Ok(Value::str(text));
            }
            if pattern_is_empty(&pat) {
                return Err(h::err("replaceAllMapped: empty pattern is not supported"));
            }
            let mut out = String::new();
            let mut last = 0usize;
            for (s, e) in matches {
                for c in text_chars(text, last, s).chars() {
                    out.push(c);
                }
                let mv = match_value(text, &pattern_source(&pat), s, e);
                let r = h::call_closure(_v, &repl, vec![mv])?;
                out.push_str(h::as_str(&r).unwrap_or_default());
                last = e;
            }
            for c in text_chars(text, last, chars_count(text)).chars() {
                out.push(c);
            }
            Ok(Value::str(out))
        }),
    );
    bc.methods.insert(
        "replaceFirstMapped".into(),
        method(|_v, t, args, _n| {
            let pat = args
                .first()
                .and_then(as_pattern_obj)
                .ok_or_else(|| h::err("replaceFirstMapped expects a Pattern argument"))?;
            let repl = args
                .get(1)
                .cloned()
                .ok_or_else(|| h::err("replaceFirstMapped expects a replace function"))?;
            let start = args.get(2).and_then(h::as_int).unwrap_or(0);
            let text = h::as_str(&t).unwrap_or_default();
            let matches = pattern_matches(text, &pat, (start.max(0)) as usize);
            let Some((s, e)) = matches.first() else {
                return Ok(Value::str(text));
            };
            let (s, e) = (*s, *e);
            let mut out = String::new();
            for (i, c) in text.chars().enumerate() {
                if i == s {
                    let mv = match_value(text, &pattern_source(&pat), s, e);
                    let r = h::call_closure(_v, &repl, vec![mv])?;
                    out.push_str(h::as_str(&r).unwrap_or_default());
                }
                if !(i >= s && i < e) {
                    out.push(c);
                }
            }
            Ok(Value::str(out))
        }),
    );
    bc.getters.insert(
        "isEmpty".into(),
        getter(|_v, t| Ok(Value::Bool(h::as_str(&t).unwrap_or_default().is_empty()))),
    );
    bc.getters.insert(
        "isNotEmpty".into(),
        getter(|_v, t| Ok(Value::Bool(!h::as_str(&t).unwrap_or_default().is_empty()))),
    );
    bc.getters.insert(
        "length".into(),
        getter(|_v, t| {
            Ok(Value::Int(
                chars_count(h::as_str(&t).unwrap_or_default()) as i64
            ))
        }),
    );
    bc.getters.insert(
        "codeUnits".into(),
        getter(|_v, t| {
            Ok(string_list(
                chars_of(&t)
                    .into_iter()
                    .map(|c| Value::Int(c as i64))
                    .collect(),
            ))
        }),
    );
    bc.getters.insert(
        "runes".into(),
        getter(|_v, t| Ok(h::wrap(&h::class("Runes"), t))),
    );
    bc.getters.insert(
        "hashCode".into(),
        getter(|_v, t| Ok(Value::Int(h::hash_code(&t)))),
    );
    bc.getters.insert(
        "runtimeType".into(),
        getter(|_v, _t| Ok(Value::Type(crate::rt::RuntimeType::named("String")))),
    );
    bc
}

pub fn chars_count(s: &str) -> usize {
    s.chars().count()
}

fn text_chars(s: &str, start: usize, end: usize) -> String {
    s.chars()
        .skip(start)
        .take(end.saturating_sub(start))
        .collect()
}

fn pad(chars: Vec<char>, width: i64, padding: &str, left: bool) -> String {
    if width <= chars.len() as i64 || padding.is_empty() {
        return chars.into_iter().collect();
    }
    let pad_chars: Vec<char> = padding.chars().collect();
    let mut padding_out = String::new();
    let mut n = width as usize - chars.len();
    while n > 0 {
        padding_out.push(pad_chars[0]);
        n -= 1;
    }
    if left {
        format!("{padding_out}{}", chars.into_iter().collect::<String>())
    } else {
        format!("{}{padding_out}", chars.into_iter().collect::<String>())
    }
}

pub fn string_buffer_definition() -> BridgedClassDef {
    let mut bc = BridgedClassDef::new("StringBuffer");
    bc.native_type = Some("StringBuffer".into());
    bc.constructors.insert(
        "".into(),
        ctor(|_v, args, _n| {
            let initial = args.first().map(native_display).unwrap_or_default();
            let native = Value::StringBuffer(Rc::new(RefCell::new(initial)));
            Ok(h::wrap(&h::class("StringBuffer"), native))
        }),
    );
    bc.methods.insert(
        "write".into(),
        method(|_v, t, args, _n| {
            let sb = h::as_strbuf(&t).ok_or_else(|| h::err("write: not a StringBuffer"))?;
            let obj = args.first().cloned().unwrap_or(Value::Null);
            sb.borrow_mut().push_str(&native_display(&obj));
            Ok(Value::Null)
        }),
    );
    bc.methods.insert(
        "writeAll".into(),
        method(|_v, t, args, _n| {
            let sb = h::as_strbuf(&t).ok_or_else(|| h::err("writeAll: not a StringBuffer"))?;
            let objects = args
                .first()
                .ok_or_else(|| h::err("writeAll expects an Iterable"))?;
            let separator = args.get(1).and_then(h::as_str).unwrap_or("");
            let items = iterable_items(objects, _v)?;
            let joined = items
                .iter()
                .map(native_display)
                .collect::<Vec<_>>()
                .join(separator);
            sb.borrow_mut().push_str(&joined);
            Ok(Value::Null)
        }),
    );
    bc.methods.insert(
        "writeln".into(),
        method(|_v, t, args, _n| {
            let sb = h::as_strbuf(&t).ok_or_else(|| h::err("writeln: not a StringBuffer"))?;
            let obj = args.first().map(native_display).unwrap_or_default();
            sb.borrow_mut().push_str(&obj);
            sb.borrow_mut().push('\n');
            Ok(Value::Null)
        }),
    );
    bc.methods.insert(
        "writeCharCode".into(),
        method(|_v, t, args, _n| {
            let sb = h::as_strbuf(&t).ok_or_else(|| h::err("writeCharCode: not a StringBuffer"))?;
            let code = h::arg_int(&args, 0, "writeCharCode")?;
            let c = char::from_u32(code as u32)
                .ok_or_else(|| h::err(format!("writeCharCode: invalid code point {code}")))?;
            sb.borrow_mut().push(c);
            Ok(Value::Null)
        }),
    );
    bc.methods.insert(
        "clear".into(),
        method(|_v, t, _a, _n| {
            let sb = h::as_strbuf(&t).ok_or_else(|| h::err("clear: not a StringBuffer"))?;
            sb.borrow_mut().clear();
            Ok(Value::Null)
        }),
    );
    bc.methods.insert(
        "toString".into(),
        method(|_v, t, _a, _n| {
            let sb = h::as_strbuf(&t).ok_or_else(|| h::err("toString: not a StringBuffer"))?;
            Ok(Value::str(sb.borrow().clone()))
        }),
    );
    bc.getters.insert(
        "length".into(),
        getter(|_v, t| {
            Ok(Value::Int(
                h::as_strbuf(&t)
                    .map(|s| s.borrow().chars().count() as i64)
                    .unwrap_or(0),
            ))
        }),
    );
    bc.getters.insert(
        "isEmpty".into(),
        getter(|_v, t| {
            Ok(Value::Bool(
                h::as_strbuf(&t)
                    .map(|s| s.borrow().is_empty())
                    .unwrap_or(true),
            ))
        }),
    );
    bc.getters.insert(
        "isNotEmpty".into(),
        getter(|_v, t| {
            Ok(Value::Bool(
                h::as_strbuf(&t)
                    .map(|s| !s.borrow().is_empty())
                    .unwrap_or(false),
            ))
        }),
    );
    bc.getters.insert(
        "hashCode".into(),
        getter(|_v, t| Ok(Value::Int(h::hash_code(&t)))),
    );
    bc.getters.insert(
        "runtimeType".into(),
        getter(|_v, _t| Ok(Value::Type(crate::rt::RuntimeType::named("StringBuffer")))),
    );
    bc
}

pub fn iterable_items(v: &Value, vst: &mut InterpreterVisitor) -> Result<Vec<Value>, InterpError> {
    let items: Vec<Value>;
    match v {
        Value::List(l) => items = l.borrow().clone(),
        Value::Set(s) => items = s.borrow().items.clone(),
        Value::Str(text) => items = text.chars().map(|c| Value::str(c.to_string())).collect(),
        Value::SyncGen(g) => {
            let mut acc = Vec::new();
            while let Some(v) = vst.run_sync_generator(g)? {
                acc.push(v);
            }
            items = acc;
        }
        Value::Bridged(b) => {
            let native = &b.borrow().native;
            match native {
                Value::List(l) => items = l.borrow().clone(),
                Value::Set(s) => items = s.borrow().items.clone(),
                Value::Map(m) => items = m.borrow().keys(),
                Value::Str(text) => {
                    items = text.chars().map(|c| Value::str(c.to_string())).collect()
                }
                other => {
                    return Err(h::err(format!(
                        "Expected an Iterable, got {}",
                        value_display(other)
                    )));
                }
            }
        }
        other => {
            return Err(h::err(format!(
                "Expected an Iterable, got {}",
                value_display(other)
            )));
        }
    }
    Ok(items)
}

pub fn string_sink_definition() -> BridgedClassDef {
    let mut bc = BridgedClassDef::new("StringSink");
    bc.methods.insert(
        "write".into(),
        method(|_v, t, args, _n| {
            if args.len() != 1 {
                return Err(h::err("StringSink.write requires exactly one argument."));
            }
            let sb = h::as_strbuf(&t).ok_or_else(|| h::err("write: not a StringSink"))?;
            sb.borrow_mut().push_str(&native_display(&args[0]));
            Ok(Value::Null)
        }),
    );
    bc.methods.insert(
        "writeln".into(),
        method(|_v, t, args, _n| {
            if args.len() > 1 {
                return Err(h::err("StringSink.writeln takes at most one argument."));
            }
            let sb = h::as_strbuf(&t).ok_or_else(|| h::err("writeln: not a StringSink"))?;
            if let Some(a) = args.first() {
                sb.borrow_mut().push_str(&native_display(a));
            }
            sb.borrow_mut().push('\n');
            Ok(Value::Null)
        }),
    );
    bc.methods.insert(
        "writeAll".into(),
        method(|_v, t, args, _n| {
            if args.is_empty() || args.len() > 2 {
                return Err(h::err(
                    "StringSink.writeAll requires an Iterable and an optional separator String.",
                ));
            }
            let sb = h::as_strbuf(&t).ok_or_else(|| h::err("writeAll: not a StringSink"))?;
            let items = iterable_items(&args[0], _v)?;
            let separator = args.get(1).and_then(h::as_str).unwrap_or("");
            let joined = items
                .iter()
                .map(native_display)
                .collect::<Vec<_>>()
                .join(separator);
            sb.borrow_mut().push_str(&joined);
            Ok(Value::Null)
        }),
    );
    bc.methods.insert(
        "writeCharCode".into(),
        method(|_v, t, args, _n| {
            if args.len() != 1 {
                return Err(h::err(
                    "StringSink.writeCharCode requires one integer argument.",
                ));
            }
            let sb = h::as_strbuf(&t).ok_or_else(|| h::err("writeCharCode: not a StringSink"))?;
            let code = h::arg_int(&args, 0, "writeCharCode")?;
            let c = char::from_u32(code as u32)
                .ok_or_else(|| h::err(format!("writeCharCode: invalid code point {code}")))?;
            sb.borrow_mut().push(c);
            Ok(Value::Null)
        }),
    );
    bc.methods.insert(
        "hashCode".into(),
        method(|_v, t, _a, _n| Ok(Value::Int(h::hash_code(&t)))),
    );
    bc.methods.insert(
        "toString".into(),
        method(|_v, t, _a, _n| {
            let sb = h::as_strbuf(&t).ok_or_else(|| h::err("toString: not a StringSink"))?;
            Ok(Value::str(sb.borrow().clone()))
        }),
    );
    bc.getters.insert(
        "hashCode".into(),
        getter(|_v, t| Ok(Value::Int(h::hash_code(&t)))),
    );
    bc.getters.insert(
        "runtimeType".into(),
        getter(|_v, _t| Ok(Value::Type(crate::rt::RuntimeType::named("StringSink")))),
    );
    bc
}

pub fn sink_definition() -> BridgedClassDef {
    let mut bc = BridgedClassDef::new("Sink");
    bc.methods.insert(
        "add".into(),
        method(|_v, t, args, _n| {
            if args.len() != 1 {
                return Err(h::err("Sink.add requires exactly one argument."));
            }
            let sb = h::as_strbuf(&t).ok_or_else(|| h::err("add: not a Sink"))?;
            sb.borrow_mut().push_str(&native_display(&args[0]));
            Ok(Value::Null)
        }),
    );
    bc.methods
        .insert("close".into(), method(|_v, _t, _a, _n| Ok(Value::Null)));
    bc.methods.insert(
        "hashCode".into(),
        method(|_v, t, _a, _n| Ok(Value::Int(h::hash_code(&t)))),
    );
    bc.methods.insert(
        "toString".into(),
        method(|_v, t, _a, _n| Ok(Value::str(value_display(&t)))),
    );
    bc.getters.insert(
        "hashCode".into(),
        getter(|_v, t| Ok(Value::Int(h::hash_code(&t)))),
    );
    bc.getters.insert(
        "runtimeType".into(),
        getter(|_v, _t| Ok(Value::Type(crate::rt::RuntimeType::named("Sink")))),
    );
    bc
}

pub fn runes_definition() -> BridgedClassDef {
    let mut bc = BridgedClassDef::new("Runes");
    bc.constructors.insert(
        "".into(),
        ctor(|_v, args, _n| {
            let s = h::arg_str(&args, 0, "Runes")?;
            Ok(h::wrap(&h::class("Runes"), Value::str(s)))
        }),
    );
    bc.methods.insert(
        "iterator".into(),
        method(|_v, t, _a, _n| {
            let items: Vec<Value> = chars_of(&t)
                .into_iter()
                .map(|c| Value::Int(c as i64))
                .collect();
            let native = Value::List(Rc::new(RefCell::new(items)));
            Ok(h::wrap(&h::class("Iterator"), native))
        }),
    );
    bc.methods.insert(
        "contains".into(),
        method(|_v, t, args, _n| {
            let other = args.first().cloned().unwrap_or(Value::Null);
            Ok(Value::Bool(chars_of(&t).iter().any(|c| {
                *c as i64 == h::as_int(&other).unwrap_or(i64::MAX)
            })))
        }),
    );
    bc.methods.insert(
        "elementAt".into(),
        method(|_v, t, args, _n| {
            let i = h::arg_int(&args, 0, "elementAt")?;
            let c = chars_of(&t)
                .get(i as usize)
                .copied()
                .ok_or_else(|| h::err(format!("elementAt: index out of range {i}")))?;
            Ok(Value::Int(c as i64))
        }),
    );
    bc.methods.insert(
        "every".into(),
        method(|_v, t, args, _n| {
            let f = args
                .first()
                .cloned()
                .ok_or_else(|| h::err("every expects a function"))?;
            for c in chars_of(&t) {
                let r = h::call_closure(_v, &f, vec![Value::Int(c as i64)])?;
                if !h::as_bool(&r).unwrap_or(false) {
                    return Ok(Value::Bool(false));
                }
            }
            Ok(Value::Bool(true))
        }),
    );
    bc.methods.insert(
        "any".into(),
        method(|_v, t, args, _n| {
            let f = args
                .first()
                .cloned()
                .ok_or_else(|| h::err("any expects a function"))?;
            for c in chars_of(&t) {
                let r = h::call_closure(_v, &f, vec![Value::Int(c as i64)])?;
                if h::as_bool(&r).unwrap_or(false) {
                    return Ok(Value::Bool(true));
                }
            }
            Ok(Value::Bool(false))
        }),
    );
    bc.methods.insert(
        "expand".into(),
        method(|_v, t, args, _n| {
            let f = args
                .first()
                .cloned()
                .ok_or_else(|| h::err("expand expects a function"))?;
            let mut out = Vec::new();
            for c in chars_of(&t) {
                let r = h::call_closure(_v, &f, vec![Value::Int(c as i64)])?;
                out.extend(iterable_items(&r, _v)?);
            }
            Ok(string_list(out))
        }),
    );
    bc.methods.insert(
        "firstWhere".into(),
        method(|_v, t, args, named| {
            let f = args
                .first()
                .cloned()
                .ok_or_else(|| h::err("firstWhere expects a function"))?;
            for c in chars_of(&t) {
                let r = h::call_closure(_v, &f, vec![Value::Int(c as i64)])?;
                if h::as_bool(&r).unwrap_or(false) {
                    return Ok(Value::Int(c as i64));
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
            for c in chars_of(&t) {
                let r = h::call_closure(_v, &f, vec![Value::Int(c as i64)])?;
                if h::as_bool(&r).unwrap_or(false) {
                    found = Some(c);
                }
            }
            match found {
                Some(c) => Ok(Value::Int(c as i64)),
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
            let mut found: Option<i64> = None;
            for c in chars_of(&t) {
                let r = h::call_closure(_v, &f, vec![Value::Int(c as i64)])?;
                if h::as_bool(&r).unwrap_or(false) {
                    if found.is_some() {
                        return Err(h::err("singleWhere: more than one element found"));
                    }
                    found = Some(c as i64);
                }
            }
            match found {
                Some(c) => Ok(Value::Int(c)),
                None => match named.get("orElse") {
                    Some(or) => h::call_closure(_v, or, vec![]),
                    None => Err(h::err("singleWhere: no element found")),
                },
            }
        }),
    );
    bc.methods.insert(
        "forEach".into(),
        method(|_v, t, args, _n| {
            let f = args
                .first()
                .cloned()
                .ok_or_else(|| h::err("forEach expects a function"))?;
            for c in chars_of(&t) {
                h::call_closure(_v, &f, vec![Value::Int(c as i64)])?;
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
            let mut out = Vec::new();
            for c in chars_of(&t) {
                out.push(h::call_closure(_v, &f, vec![Value::Int(c as i64)])?);
            }
            Ok(string_list(out))
        }),
    );
    bc.methods.insert(
        "where".into(),
        method(|_v, t, args, _n| {
            let f = args
                .first()
                .cloned()
                .ok_or_else(|| h::err("where expects a function"))?;
            let mut out = Vec::new();
            for c in chars_of(&t) {
                let r = h::call_closure(_v, &f, vec![Value::Int(c as i64)])?;
                if h::as_bool(&r).unwrap_or(false) {
                    out.push(Value::Int(c as i64));
                }
            }
            Ok(string_list(out))
        }),
    );
    bc.methods.insert(
        "whereType".into(),
        method(|_v, t, _a, _n| {
            Ok(string_list(
                chars_of(&t)
                    .into_iter()
                    .map(|c| Value::Int(c as i64))
                    .collect(),
            ))
        }),
    );
    bc.methods.insert(
        "skip".into(),
        method(|_v, t, args, _n| {
            let n = h::arg_int(&args, 0, "skip")?;
            Ok(string_list(
                chars_of(&t)
                    .into_iter()
                    .skip(n.max(0) as usize)
                    .map(|c| Value::Int(c as i64))
                    .collect(),
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
            let mut skipped = false;
            let mut out = Vec::new();
            for c in chars_of(&t) {
                let r = h::call_closure(_v, &f, vec![Value::Int(c as i64)])?;
                if !skipped && h::as_bool(&r).unwrap_or(false) {
                    continue;
                }
                skipped = true;
                out.push(Value::Int(c as i64));
            }
            Ok(string_list(out))
        }),
    );
    bc.methods.insert(
        "take".into(),
        method(|_v, t, args, _n| {
            let n = h::arg_int(&args, 0, "take")?;
            Ok(string_list(
                chars_of(&t)
                    .into_iter()
                    .take(n.max(0) as usize)
                    .map(|c| Value::Int(c as i64))
                    .collect(),
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
            let mut out = Vec::new();
            for c in chars_of(&t) {
                let r = h::call_closure(_v, &f, vec![Value::Int(c as i64)])?;
                if !h::as_bool(&r).unwrap_or(false) {
                    break;
                }
                out.push(Value::Int(c as i64));
            }
            Ok(string_list(out))
        }),
    );
    bc.methods.insert(
        "toList".into(),
        method(|_v, t, _a, _n| {
            Ok(string_list(
                chars_of(&t)
                    .into_iter()
                    .map(|c| Value::Int(c as i64))
                    .collect(),
            ))
        }),
    );
    bc.methods.insert(
        "toSet".into(),
        method(|_v, t, _a, _n| {
            let mut s = crate::value::DartSet::new();
            for c in chars_of(&t) {
                s.insert(Value::Int(c as i64));
            }
            Ok(Value::Set(Rc::new(RefCell::new(s))))
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
            for c in chars_of(&t) {
                acc = h::call_closure(_v, &f, vec![acc, Value::Int(c as i64)])?;
            }
            Ok(acc)
        }),
    );
    bc.methods.insert(
        "reduce".into(),
        method(|_v, t, args, _n| {
            let f = args
                .first()
                .cloned()
                .ok_or_else(|| h::err("reduce expects a function"))?;
            let mut iter = chars_of(&t).into_iter();
            let mut acc = match iter.next() {
                Some(c) => Value::Int(c as i64),
                None => return Err(h::err("reduce: empty iterable")),
            };
            for c in iter {
                acc = h::call_closure(_v, &f, vec![acc, Value::Int(c as i64)])?;
            }
            Ok(acc)
        }),
    );
    bc.methods.insert(
        "join".into(),
        method(|_v, t, args, _n| {
            let sep = args.first().and_then(h::as_str).unwrap_or("");
            let joined = chars_of(&t)
                .iter()
                .map(|c| c.to_string())
                .collect::<Vec<_>>()
                .join(sep);
            Ok(Value::str(joined))
        }),
    );
    bc.methods
        .insert("toString".into(), method(|_v, t, _a, _n| Ok(t)));
    bc.methods
        .insert("cast".into(), method(|_v, t, _a, _n| Ok(t)));
    bc.methods.insert(
        "followedBy".into(),
        method(|_v, t, args, _n| {
            let other = args
                .first()
                .cloned()
                .ok_or_else(|| h::err("followedBy expects an Iterable"))?;
            let mut out: Vec<Value> = chars_of(&t)
                .into_iter()
                .map(|c| Value::Int(c as i64))
                .collect();
            out.extend(iterable_items(&other, _v)?);
            Ok(string_list(out))
        }),
    );
    bc.getters.insert(
        "length".into(),
        getter(|_v, t| {
            Ok(Value::Int(
                chars_count(h::as_str(&t).unwrap_or_default()) as i64
            ))
        }),
    );
    bc.getters.insert(
        "isEmpty".into(),
        getter(|_v, t| Ok(Value::Bool(h::as_str(&t).unwrap_or_default().is_empty()))),
    );
    bc.getters.insert(
        "isNotEmpty".into(),
        getter(|_v, t| Ok(Value::Bool(!h::as_str(&t).unwrap_or_default().is_empty()))),
    );
    bc.getters.insert(
        "first".into(),
        getter(|_v, t| {
            chars_of(&t)
                .first()
                .copied()
                .map(|c| Ok(Value::Int(c as i64)))
                .unwrap_or_else(|| Err(h::err("first: empty runes")))
        }),
    );
    bc.getters.insert(
        "last".into(),
        getter(|_v, t| {
            chars_of(&t)
                .last()
                .copied()
                .map(|c| Ok(Value::Int(c as i64)))
                .unwrap_or_else(|| Err(h::err("last: empty runes")))
        }),
    );
    bc.getters.insert(
        "single".into(),
        getter(|_v, t| {
            let chars = chars_of(&t);
            match chars.len() {
                1 => Ok(Value::Int(chars[0] as i64)),
                0 => Err(h::err("single: empty runes")),
                _ => Err(h::err("single: too many elements")),
            }
        }),
    );
    bc.getters.insert("string".into(), getter(|_v, t| Ok(t)));
    bc.getters.insert(
        "hashCode".into(),
        getter(|_v, t| Ok(Value::Int(h::hash_code(&t)))),
    );
    bc.getters.insert(
        "runtimeType".into(),
        getter(|_v, _t| Ok(Value::Type(crate::rt::RuntimeType::named("Runes")))),
    );
    bc
}
