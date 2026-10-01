//! End-to-end test: runs a MangaYomi-style Dart extension (a subclass of a
//! bridged `MProvider`) through the d4rt_rs bridge surface, mirroring what
//! MangaYomi's `DartExtensionService` does:
//!
//! 1. register bridged classes/enums + top-level functions
//! 2. `execute(source, "main", [MSource])` -> provider instance
//! 3. `invoke("getPopular" | "getDetail" | "getFilterList" | ...)` on it
//!
//! The DOM + HTTP are fakes: a tiny hand-rolled HTML tree and a mock Client
//! whose responses come from a closure. The real implementations live in the
//! RakuYomi backend; this test pins the interpreter semantics.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use d4rt_rs::class::BridgedInstanceData;
use d4rt_rs::class::{BridgedClassDef, BridgedEnumDef};
use d4rt_rs::interp::InterpreterVisitor;
use d4rt_rs::value::DartMap;
use d4rt_rs::{Context, InterpError, Value};

// ---------------------------------------------------------------------------
// Native model helpers (Value-encoded, same pattern as the real bridge)
// ---------------------------------------------------------------------------

fn nmap() -> Value {
    Value::Map(Rc::new(RefCell::new(DartMap::new())))
}

fn map_get(v: &Value, key: &str) -> Value {
    match v {
        Value::Map(m) => m.borrow().get(&Value::str(key)).unwrap_or(Value::Null),
        _ => Value::Null,
    }
}

fn map_set(v: &Value, key: &str, value: Value) {
    if let Value::Map(m) = v {
        m.borrow_mut().set(Value::str(key), value);
    }
}

fn map_str(v: &Value, key: &str) -> String {
    match map_get(v, key) {
        Value::Str(s) => s.to_string(),
        _ => String::new(),
    }
}

fn wrap(bc: &Arc<BridgedClassDef>, native: Value) -> Value {
    Value::Bridged(Rc::new(RefCell::new(BridgedInstanceData {
        bridged_class: bc.clone(),
        native,
        type_arguments: vec![],
        bridged_super_object: None,
    })))
}

// ---------------------------------------------------------------------------
// Minimal HTML tree (tag, attrs, text, children)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct MiniElem {
    tag: String,
    class: String,
    attrs: Vec<(String, String)>,
    text: String,
    children: Vec<MiniElem>,
}

fn mini_elem_to_value(e: &MiniElem) -> Value {
    let v = nmap();
    map_set(&v, "tag", Value::str(&e.tag));
    map_set(&v, "class", Value::str(&e.class));
    map_set(&v, "text", Value::str(&e.text));
    let attrs = Value::Map(Rc::new(RefCell::new(DartMap::new())));
    if let Value::Map(a) = &attrs {
        for (k, val) in &e.attrs {
            a.borrow_mut().set(Value::str(k), Value::str(val));
            if matches!(k.as_str(), "href" | "src" | "data-src") {
                map_set(&v, k, Value::str(val));
            }
        }
    }
    map_set(&v, "attrs", attrs);
    map_set(
        &v,
        "children",
        Value::List(Rc::new(RefCell::new(
            e.children.iter().map(mini_elem_to_value).collect(),
        ))),
    );
    v
}

/// Ultra-minimal HTML parser: tags `<t ...>`, attributes `name="value"` or
/// `name=value`, self-closing or closed with the same tag. Text goes into
/// the innermost open element.
fn parse_mini_html(html: &str) -> Vec<MiniElem> {
    let mut stack: Vec<MiniElem> = Vec::new();
    let mut result: Vec<MiniElem> = Vec::new();
    let bytes = html.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'<' {
            let close = html[i + 1..].find('>').map(|p| i + 1 + p);
            let Some(end) = close else { break };
            let raw = &html[i + 1..end];
            i = end + 1;
            if raw.is_empty() {
                continue;
            }
            if raw.starts_with('/') {
                if let Some(open) = stack.pop() {
                    match stack.last_mut() {
                        Some(parent) => parent.children.push(open),
                        None => result.push(open),
                    }
                }
                continue;
            }
            let parts: Vec<&str> = raw.split_whitespace().collect();
            let tag = parts[0].to_string();
            let mut attrs: Vec<(String, String)> = Vec::new();
            let mut class = String::new();
            for part in &parts[1..] {
                let (k, v) = match part.split_once('=') {
                    Some((k, v)) => (k.to_string(), v.trim_matches('"').to_string()),
                    None => (part.to_string(), String::new()),
                };
                if k == "class" {
                    class = v.clone();
                }
                attrs.push((k, v));
            }
            let self_closing = raw.ends_with('/') || matches!(tag.as_str(), "img" | "br" | "meta");
            let elem = MiniElem {
                tag,
                class,
                attrs,
                text: String::new(),
                children: vec![],
            };
            if self_closing {
                match stack.last_mut() {
                    Some(parent) => parent.children.push(elem),
                    None => result.push(elem),
                }
            } else {
                stack.push(elem);
            }
        } else {
            // text run until next '<'
            let next = html[i..].find('<').map(|p| i + p).unwrap_or(bytes.len());
            let text = html[i..next].to_string();
            if let Some(open) = stack.last_mut() {
                open.text.push_str(text.trim());
            }
            i = next;
        }
    }
    result
}

fn match_selector(e: &MiniElem, selector: &str) -> bool {
    match selector.split_once('.') {
        Some((tag, class)) => {
            (tag.is_empty() || e.tag == tag) && e.class.split_whitespace().any(|c| c == class)
        }
        None => e.tag == selector,
    }
}

fn select_in(e: &MiniElem, selector: &str, out: &mut Vec<MiniElem>) {
    if selector.contains(' ') {
        let parts: Vec<&str> = selector.split_whitespace().collect();
        let mut scope = vec![e.clone()];
        for (i, part) in parts.iter().enumerate() {
            let mut next = Vec::new();
            for s in &scope {
                collect_matching(s, part, &mut next);
            }
            if i == parts.len() - 1 {
                out.extend(next);
                return;
            }
            scope = next;
            if scope.is_empty() {
                return;
            }
        }
        return;
    }
    if match_selector(e, selector) {
        out.push(e.clone());
    }
    for c in &e.children {
        select_in(c, selector, out);
    }
}

fn collect_matching(e: &MiniElem, selector: &str, out: &mut Vec<MiniElem>) {
    if match_selector(e, selector) {
        out.push(e.clone());
    }
    for c in &e.children {
        collect_matching(c, selector, out);
    }
}

// ---------------------------------------------------------------------------
// Bridge setup
// ---------------------------------------------------------------------------

/// Generic bridged class whose native is a plain map with the given keys,
/// constructor fills keys from positional/named args.
fn make_map_class(name: &str, keys: &[&str]) -> Arc<BridgedClassDef> {
    let keys: Vec<String> = keys.iter().map(|s| s.to_string()).collect();
    let name_owned = name.to_string();
    let mut bc = BridgedClassDef::new(name);
    let kc = keys.clone();
    let n2 = name_owned.clone();
    bc.constructors.insert(
        "".to_string(),
        Box::new(move |_v, positional, named| {
            let n = nmap();
            for (i, k) in kc.iter().enumerate() {
                let v = positional
                    .get(i)
                    .cloned()
                    .or_else(|| named.get(k).cloned())
                    .unwrap_or(Value::Null);
                map_set(&n, k, v);
            }
            Ok(wrap(&classes_arc(&n2), n))
        }),
    );
    for k in keys {
        let kg = k.clone();
        bc.getters.insert(
            k.clone(),
            Box::new(move |_v, target| Ok(map_get(&target, &kg))),
        );
    }
    Arc::new(bc)
}

type CtorAdapter = Box<
    dyn Fn(
            &mut InterpreterVisitor,
            Vec<Value>,
            HashMap<String, Value>,
        ) -> Result<Value, InterpError>
        + Send
        + Sync,
>;

fn register_bridge(ctx: &mut Context) -> Rc<BridgeMap> {
    let mut classes: BridgeMap = HashMap::new();

    let mut msource = BridgedClassDef::new("MSource");
    let ctor: CtorAdapter = Box::new(|_v, _p, named| {
        let n = nmap();
        for key in [
            "id",
            "name",
            "baseUrl",
            "lang",
            "isFullData",
            "hasCloudflare",
            "dateFormat",
            "dateFormatLocale",
            "apiUrl",
            "additionalParams",
            "notes",
        ] {
            map_set(&n, key, named.get(key).cloned().unwrap_or(Value::Null));
        }
        Ok(n)
    });
    msource.constructors.insert("".to_string(), ctor);
    for key in [
        "id",
        "name",
        "baseUrl",
        "lang",
        "isFullData",
        "hasCloudflare",
        "dateFormat",
        "dateFormatLocale",
        "apiUrl",
        "additionalParams",
        "notes",
    ] {
        let k = key.to_string();
        let kk = k.clone();
        msource
            .getters
            .insert(k, Box::new(move |_v, target| Ok(map_get(&target, &kk))));
    }
    let msource = Arc::new(msource);
    classes.insert("MSource".to_string(), msource.clone());

    let mut mchapter = BridgedClassDef::new("MChapter");
    mchapter.constructors.insert(
        "".to_string(),
        Box::new(|_v, _p, named| {
            let n = nmap();
            for key in [
                "name",
                "url",
                "dateUpload",
                "scanlator",
                "isFiller",
                "thumbnailUrl",
                "description",
                "downloadSize",
                "duration",
            ] {
                map_set(&n, key, named.get(key).cloned().unwrap_or(Value::Null));
            }
            Ok(wrap(&classes_arc("MChapter"), n))
        }),
    );
    for key in [
        "name",
        "url",
        "dateUpload",
        "scanlator",
        "isFiller",
        "thumbnailUrl",
        "description",
        "downloadSize",
        "duration",
    ] {
        let k = key.to_string();
        let kg = k.clone();
        mchapter.getters.insert(
            k.clone(),
            Box::new(move |_v, target| Ok(map_get(&target, &kg))),
        );
        let ks = k.clone();
        mchapter.setters.insert(
            k,
            Box::new(move |_v, target, value| {
                map_set(&target, &ks, value);
                Ok(())
            }),
        );
    }
    let mchapter = Arc::new(mchapter);
    classes.insert("MChapter".to_string(), mchapter.clone());

    let mut mmanga = BridgedClassDef::new("MManga");
    mmanga.constructors.insert(
        "".to_string(),
        Box::new(|_v, _p, named| {
            let n = nmap();
            for key in [
                "name",
                "artist",
                "author",
                "description",
                "genre",
                "status",
                "imageUrl",
                "link",
                "chapters",
            ] {
                map_set(&n, key, named.get(key).cloned().unwrap_or(Value::Null));
            }
            Ok(wrap(&classes_arc("MManga"), n))
        }),
    );
    for key in [
        "name",
        "artist",
        "author",
        "description",
        "genre",
        "status",
        "imageUrl",
        "link",
        "chapters",
    ] {
        let k = key.to_string();
        let kg = k.clone();
        mmanga.getters.insert(
            k.clone(),
            Box::new(move |_v, target| Ok(map_get(&target, &kg))),
        );
        let ks = k.clone();
        mmanga.setters.insert(
            k,
            Box::new(move |_v, target, value| {
                map_set(&target, &ks, value);
                Ok(())
            }),
        );
    }
    let mmanga = Arc::new(mmanga);
    classes.insert("MManga".to_string(), mmanga.clone());

    let mut mpages = BridgedClassDef::new("MPages");
    mpages.constructors.insert(
        "".to_string(),
        Box::new(move |_v, positional, _named| {
            let native = Value::List(Rc::new(RefCell::new(vec![
                positional.first().cloned().unwrap_or(Value::Null),
                positional.get(1).cloned().unwrap_or(Value::Bool(false)),
            ])));
            Ok(wrap(&classes_arc("MPages"), native))
        }),
    );
    mpages.getters.insert(
        "list".to_string(),
        Box::new(|_v, target| {
            Ok(match &target {
                Value::List(l) => l.borrow().first().cloned().unwrap_or(Value::Null),
                _ => Value::Null,
            })
        }),
    );
    mpages.getters.insert(
        "hasNextPage".to_string(),
        Box::new(|_v, target| {
            Ok(match &target {
                Value::List(l) => l.borrow().get(1).cloned().unwrap_or(Value::Bool(false)),
                _ => Value::Bool(false),
            })
        }),
    );
    mpages.setters.insert(
        "list".to_string(),
        Box::new(|_v, target, value| {
            if let Value::List(l) = &target {
                l.borrow_mut()[0] = value;
            }
            Ok(())
        }),
    );
    mpages.setters.insert(
        "hasNextPage".to_string(),
        Box::new(|_v, target, value| {
            if let Value::List(l) = &target {
                l.borrow_mut()[1] = value;
            }
            Ok(())
        }),
    );
    let mpages = Arc::new(mpages);
    classes.insert("MPages".to_string(), mpages.clone());

    // Client: mock HTTP. Native = Value::Null; responses come from a closure.
    let mut client = BridgedClassDef::new("Client");
    client.constructors.insert(
        "".to_string(),
        Box::new(|_v, _p, _named| Ok(wrap(&classes_arc("Client"), nmap()))),
    );
    client.methods.insert(
        "get".to_string(),
        Box::new(|_v, _target, positional, _named| {
            let url = match positional.first() {
                Some(Value::Bridged(b)) => match &b.borrow().native {
                    Value::Str(s) => s.to_string(),
                    other => d4rt_rs::value::value_display(other),
                },
                Some(other) => d4rt_rs::value::value_display(other),
                None => String::new(),
            };
            let body = MOCK_HTTP.with(|m| m.borrow_mut().call(&url));
            let resp = nmap();
            map_set(&resp, "statusCode", Value::Int(200));
            map_set(&resp, "body", Value::str(body));
            Ok(Value::Future(Rc::new(RefCell::new(
                d4rt_rs::async_state::FutureState {
                    completed: true,
                    value: Some(wrap(&classes_arc("Response"), resp)),
                    error: None,
                    continuations: vec![],
                },
            ))))
        }),
    );
    let client = Arc::new(client);
    classes.insert("Client".to_string(), client.clone());

    let mut response = BridgedClassDef::new("Response");
    response.getters.insert(
        "statusCode".to_string(),
        Box::new(|_v, target| Ok(map_get(&target, "statusCode"))),
    );
    response.getters.insert(
        "body".to_string(),
        Box::new(|_v, target| Ok(map_get(&target, "body"))),
    );
    let response = Arc::new(response);
    classes.insert("Response".to_string(), response.clone());

    // MElement / MDocument
    let mut element = BridgedClassDef::new("MElement");
    element.getters.insert(
        "text".to_string(),
        Box::new(|_v, target| Ok(map_get(&target, "text"))),
    );
    element.getters.insert(
        "getHref".to_string(),
        Box::new(|_v, target| Ok(Value::str(map_str(&target, "href")))),
    );
    element.getters.insert(
        "getSrc".to_string(),
        Box::new(|_v, target| Ok(Value::str(map_str(&target, "src")))),
    );
    element.getters.insert(
        "getDataSrc".to_string(),
        Box::new(|_v, target| Ok(Value::str(map_str(&target, "data-src")))),
    );
    element.methods.insert(
        "attr".to_string(),
        Box::new(|_v, target, positional, _named| {
            let name = match positional.first() {
                Some(Value::Str(s)) => s.to_string(),
                _ => String::new(),
            };
            let v = map_get(&target, &name);
            Ok(match v {
                Value::Str(s) if s.is_empty() => Value::Null,
                other => other,
            })
        }),
    );
    element.methods.insert(
        "select".to_string(),
        Box::new(|_v, target, positional, _named| {
            let sel = match positional.first() {
                Some(Value::Str(s)) => s.to_string(),
                _ => String::new(),
            };
            let mut out = Vec::new();
            let children = match map_get(&target, "children") {
                Value::List(l) => l.borrow().clone(),
                _ => vec![],
            };
            for child in &children {
                let elem = mini_elem_from_value(child);
                if let Some(e) = elem {
                    select_in(&e, &sel, &mut out);
                }
            }
            Ok(Value::List(Rc::new(RefCell::new(
                out.iter()
                    .map(|e| wrap(&classes_arc("MElement"), mini_elem_to_value(e)))
                    .collect(),
            ))))
        }),
    );
    element.methods.insert(
        "selectFirst".to_string(),
        Box::new(|_v, target, positional, _named| {
            let sel = match positional.first() {
                Some(Value::Str(s)) => s.to_string(),
                _ => String::new(),
            };
            let mut out = Vec::new();
            let children = match map_get(&target, "children") {
                Value::List(l) => l.borrow().clone(),
                _ => vec![],
            };
            for child in &children {
                let elem = mini_elem_from_value(child);
                if let Some(e) = elem {
                    select_in(&e, &sel, &mut out);
                }
            }
            match out.first() {
                Some(e) => Ok(wrap(&classes_arc("MElement"), mini_elem_to_value(e))),
                None => Ok(Value::Null),
            }
        }),
    );
    let element = Arc::new(element);
    classes.insert("MElement".to_string(), element.clone());

    let mut document = BridgedClassDef::new("MDocument");
    document.getters.insert(
        "body".to_string(),
        Box::new(|_v, target| Ok(map_get(&target, "body"))),
    );
    document.methods.insert(
        "select".to_string(),
        Box::new(|_v, target, positional, _named| {
            let sel = match positional.first() {
                Some(Value::Str(s)) => s.to_string(),
                _ => String::new(),
            };
            let mut out = Vec::new();
            let body = match map_get(&target, "body") {
                Value::Bridged(b) => mini_elem_from_value(&b.borrow().native),
                _ => None,
            };
            if let Some(body) = body {
                select_in(&body, &sel, &mut out);
            }
            Ok(Value::List(Rc::new(RefCell::new(
                out.iter()
                    .map(|e| wrap(&classes_arc("MElement"), mini_elem_to_value(e)))
                    .collect(),
            ))))
        }),
    );
    document.methods.insert(
        "selectFirst".to_string(),
        Box::new(|_v, target, positional, _named| {
            let sel = match positional.first() {
                Some(Value::Str(s)) => s.to_string(),
                _ => String::new(),
            };
            let mut out = Vec::new();
            let body = match map_get(&target, "body") {
                Value::Bridged(b) => mini_elem_from_value(&b.borrow().native),
                _ => None,
            };
            if let Some(body) = body {
                select_in(&body, &sel, &mut out);
            }
            match out.first() {
                Some(e) => Ok(wrap(&classes_arc("MElement"), mini_elem_to_value(e))),
                None => Ok(Value::Null),
            }
        }),
    );
    let document = Arc::new(document);
    classes.insert("MDocument".to_string(), document.clone());

    // Filter model classes used by getFilterList
    let text_filter = make_map_class("TextFilter", &["type", "name", "state", "typeName"]);
    classes.insert("TextFilter".to_string(), text_filter);
    let select_filter = make_map_class(
        "SelectFilter",
        &["type", "name", "state", "options", "typeName"],
    );
    classes.insert("SelectFilter".to_string(), select_filter);
    let select_option = make_map_class("SelectFilterOption", &["name", "value", "typeName"]);
    classes.insert("SelectFilterOption".to_string(), select_option);

    // MProvider: bridged superclass of the extension class.
    let mut provider = BridgedClassDef::new("MProvider");
    provider
        .constructors
        .insert("".to_string(), Box::new(|_v, _p, _named| Ok(Value::Null)));
    provider.getters.insert(
        "supportsLatest".to_string(),
        Box::new(|_v, _target| Ok(Value::Bool(true))),
    );
    provider.getters.insert(
        "baseUrl".to_string(),
        Box::new(|_v, _target| Ok(Value::str("https://example.com"))),
    );
    provider
        .getters
        .insert("headers".to_string(), Box::new(|_v, _target| Ok(nmap())));
    let provider = Arc::new(provider);
    classes.insert("MProvider".to_string(), provider.clone());

    // MStatus bridged enum
    let status_def = BridgedEnumDef::new(
        "MStatus",
        vec![
            ("ongoing", Value::Int(0)),
            ("completed", Value::Int(1)),
            ("unknown", Value::Int(2)),
        ],
    );
    STATUS_ENUM.with(|s| *s.borrow_mut() = Some(status_def.clone()));

    let classes_rc = Rc::new(classes);
    for c in classes_rc.values() {
        ctx.register_bridged_class(c.clone());
    }
    ctx.register_package_library("package:mangayomi/bridge_lib.dart", {
        let mut v: Vec<Arc<BridgedClassDef>> = classes_rc.values().cloned().collect();
        v.sort_by(|a, b| a.name.cmp(&b.name));
        v
    });
    ctx.register_bridged_enum(status_def);

    // Top-level functions
    let classes2 = classes_rc.clone();
    ctx.set(
        "parseHtml",
        Value::NativeFn(Rc::new(d4rt_rs::callable::NativeFunction {
            name: Some("parseHtml".to_string()),
            min_arity: 1,
            max_arity: Some(1),
            is_async: false,
            is_generator: false,
            closure: Box::new(move |_v, positional, _named| {
                let html = match positional.first() {
                    Some(Value::Str(s)) => s.to_string(),
                    _ => String::new(),
                };
                let tree = parse_mini_html(&html);
                let doc = nmap();
                map_set(
                    &doc,
                    "body",
                    wrap(
                        &classes2.get("MElement").unwrap().clone(),
                        mini_elem_to_value(&MiniElem {
                            tag: "body".to_string(),
                            class: String::new(),
                            attrs: vec![],
                            text: String::new(),
                            children: tree,
                        }),
                    ),
                );
                Ok(wrap(&classes2.get("MDocument").unwrap().clone(), doc))
            }),
            callable_runtime_type: d4rt_rs::rt::FunctionRuntimeType::untyped(),
        })),
    );

    let classes3 = classes_rc.clone();
    ctx.set(
        "parseStatus",
        Value::NativeFn(Rc::new(d4rt_rs::callable::NativeFunction {
            name: Some("parseStatus".to_string()),
            min_arity: 2,
            max_arity: Some(2),
            is_async: false,
            is_generator: false,
            closure: Box::new(move |_v, positional, _named| {
                let text = match positional.first() {
                    Some(Value::Str(s)) => s.to_string().to_lowercase(),
                    _ => String::new(),
                };
                let _ = classes3;
                let status_rc: Rc<BridgedEnumDef> =
                    STATUS_ENUM.with(|s| s.borrow().as_ref().expect("status enum").clone());
                let values: Vec<_> = status_rc
                    .values
                    .borrow()
                    .iter()
                    .map(|v| v.as_ref().clone())
                    .collect();
                let hit = values.iter().find(|v| match &v.native_value {
                    Value::Int(i) => {
                        let label = match i {
                            0 => "ongoing",
                            1 => "completed",
                            _ => "unknown",
                        };
                        text.contains(label) || (label == "ongoing" && text.contains("on-going"))
                    }
                    _ => false,
                });
                Ok(match hit {
                    Some(v) => Value::BridgedEnumValue(Rc::new(RefCell::new(v.clone()))),
                    None => {
                        let unknown = values
                            .iter()
                            .find(|v| v.name == "unknown")
                            .cloned()
                            .expect("MStatus.unknown must exist");
                        Value::BridgedEnumValue(Rc::new(RefCell::new(unknown)))
                    }
                })
            }),
            callable_runtime_type: d4rt_rs::rt::FunctionRuntimeType::untyped(),
        })),
    );

    let classes4 = classes_rc.clone();
    ctx.set(
        "substringBefore",
        Value::NativeFn(Rc::new(d4rt_rs::callable::NativeFunction {
            name: Some("substringBefore".to_string()),
            min_arity: 2,
            max_arity: Some(2),
            is_async: false,
            is_generator: false,
            closure: Box::new(move |_v, positional, _named| {
                let s = match positional.first() {
                    Some(Value::Str(v)) => v.to_string(),
                    _ => String::new(),
                };
                let delim = match positional.get(1) {
                    Some(Value::Str(v)) => v.to_string(),
                    _ => String::new(),
                };
                let out = s.split(&delim).next().unwrap_or("").to_string();
                let _ = classes4;
                Ok(Value::str(out))
            }),
            callable_runtime_type: d4rt_rs::rt::FunctionRuntimeType::untyped(),
        })),
    );

    ctx.set(
        "getPreferenceValue",
        Value::NativeFn(Rc::new(d4rt_rs::callable::NativeFunction {
            name: Some("getPreferenceValue".to_string()),
            min_arity: 2,
            max_arity: Some(2),
            is_async: false,
            is_generator: false,
            closure: Box::new(|_v, _positional, _named| Ok(Value::Null)),
            callable_runtime_type: d4rt_rs::rt::FunctionRuntimeType::untyped(),
        })),
    );

    classes_rc
}

thread_local! {
    static MOCK_HTTP: RefCell<MockHttp> = RefCell::new(MockHttp { routes: HashMap::new() });
}

#[derive(Default)]
struct MockHttp {
    routes: HashMap<String, String>,
}

impl MockHttp {
    fn call(&mut self, url: &str) -> String {
        self.routes
            .get(url)
            .cloned()
            .unwrap_or_else(|| "<html><body></body></html>".to_string())
    }
}

type BridgeMap = HashMap<String, Arc<BridgedClassDef>>;

thread_local! {
    static CLASSES: RefCell<Option<Rc<BridgeMap>>> = const { RefCell::new(None) };
}

thread_local! {
    static STATUS_ENUM: RefCell<Option<Rc<BridgedEnumDef>>> = const { RefCell::new(None) };
}

fn classes_arc(name: &str) -> Arc<BridgedClassDef> {
    CLASSES.with(|c| {
        let map = c.borrow();
        let map = map.as_ref().expect("bridge registered");
        map.get(name)
            .unwrap_or_else(|| panic!("missing class: {name}"))
            .clone()
    })
}

fn mini_elem_from_value(v: &Value) -> Option<MiniElem> {
    let n = match v {
        Value::Bridged(b) => &b.borrow().native,
        Value::Map(_) => v,
        _ => return None,
    };
    let mut attrs: Vec<(String, String)> = match map_get(n, "attrs") {
        Value::Map(m) => {
            let m = m.borrow();
            m.entries
                .iter()
                .filter_map(|(k, v)| match (k, v) {
                    (Value::Str(k), Value::Str(v)) => Some((k.to_string(), v.to_string())),
                    _ => None,
                })
                .collect()
        }
        _ => vec![],
    };
    for k in ["href", "src", "data-src"] {
        let top = map_get(n, k);
        if let Value::Str(s) = &top
            && !s.is_empty()
            && !attrs.iter().any(|(ak, _)| ak == k)
        {
            attrs.push((k.to_string(), s.to_string()));
        }
    }
    Some(MiniElem {
        tag: map_str(n, "tag"),
        class: map_str(n, "class"),
        attrs,
        text: map_str(n, "text"),
        children: match map_get(n, "children") {
            Value::List(l) => l.borrow().iter().filter_map(mini_elem_from_value).collect(),
            _ => vec![],
        },
    })
}

// ---------------------------------------------------------------------------
// The extension under test (mirrors mangayomi-extensions madara.dart shape)
// ---------------------------------------------------------------------------

const EXTENSION: &str = r#"
import 'package:mangayomi/bridge_lib.dart';

class Madara extends MProvider {
  Madara({required this.source});
  MSource source;
  final Client client = Client();

  @override
  Future<MPages> getPopular(int page) async {
    final res = (await client.get(Uri.parse("${source.baseUrl}/page/$page"))).body;
    final document = parseHtml(res);
    List<MManga> mangaList = [];
    for (final el in document.select("div.item")) {
      MManga manga = MManga();
      manga.name = el.selectFirst("a.title").text;
      manga.link = el.selectFirst("a.title").getHref;
      manga.imageUrl = el.selectFirst("img").getSrc;
      mangaList.add(manga);
    }
    return MPages(mangaList, true);
  }

  @override
  Future<MManga> getDetail(String url) async {
    final res = (await client.get(Uri.parse(url))).body;
    final document = parseHtml(res);
    MManga manga = MManga();
    manga.name = document.selectFirst("h1.post-title").text;
    manga.description = document.selectFirst("div.summary__content").text;
    manga.status = parseStatus(document.selectFirst("div.summary-content").text, []);
    final imageElement = document.selectFirst("div.summary_image img");
    manga.imageUrl = imageElement.getSrc;
    manga.genre = document.select("div.genres-content a").map((e) => e.text).toList();
    List<MChapter> chapters = [];
    for (final element in document.select("li.wp-manga-chapter")) {
      var ch = element.selectFirst("a");
      if (ch != null) {
        var chapter = MChapter();
        chapter.url = ch.getHref;
        chapter.name = ch.text;
        chapters.add(chapter);
      }
    }
    manga.chapters = chapters;
    return manga;
  }

  @override
  Future<List<String>> getPageList(String url) async {
    final res = (await client.get(Uri.parse(url))).body;
    final document = parseHtml(res);
    return document.select("div.page-break img").map((e) => e.getSrc).toList();
  }

  @override
  List<dynamic> getFilterList() {
    return [
      TextFilter("GenreListFilter", "Genre", ""),
      SelectFilter("OrderByFilter", "Order By", 0, [
        SelectFilterOption("Relevance", ""),
        SelectFilterOption("Latest", "latest"),
      ]),
    ];
  }

  @override
  bool get supportsLatest => true;
}

Madara main(MSource source) => Madara(source: source);
"#;

const POPULAR_HTML: &str = r#"
<html><body>
  <div class="item">
    <a class="title" href="/manga/one">One Piece</a>
    <img src="https://cdn.example.com/one.jpg">
  </div>
  <div class="item">
    <a class="title" href="/manga/two">Two</a>
    <img src="https://cdn.example.com/two.jpg">
  </div>
</body></html>
"#;

const DETAIL_HTML: &str = r#"
<html><body>
  <h1 class="post-title">One Piece</h1>
  <div class="summary__content">Pirate adventure story.</div>
  <div class="summary-content">OnGoing</div>
  <div class="summary_image"><img src="https://cdn.example.com/cover.jpg"></div>
  <div class="genres-content"><a>Action</a><a>Adventure</a></div>
  <li class="wp-manga-chapter"><a href="/manga/one/ch/1">Chapter 1</a></li>
  <li class="wp-manga-chapter"><a href="/manga/one/ch/2">Chapter 2</a></li>
</body></html>
"#;

const PAGES_HTML: &str = r#"
<html><body>
  <div class="page-break"><img src="https://cdn.example.com/p1.jpg"></div>
  <div class="page-break"><img src="https://cdn.example.com/p2.jpg"></div>
</body></html>
"#;

#[test]
fn mangayomi_style_extension_end_to_end() {
    MOCK_HTTP.with(|m| {
        let mut m = m.borrow_mut();
        m.routes.insert(
            "https://example.com/page/1".to_string(),
            POPULAR_HTML.to_string(),
        );
        m.routes
            .insert("/manga/one".to_string(), DETAIL_HTML.to_string());
        m.routes
            .insert("/manga/one/ch/1".to_string(), PAGES_HTML.to_string());
    });

    let mut ctx = Context::new();
    ctx.grant(d4rt_rs::permission::Permission::Filesystem(
        d4rt_rs::permission::FilesystemPermission::Any,
    ));
    ctx.grant(d4rt_rs::permission::Permission::Network(
        d4rt_rs::permission::NetworkPermission::Any,
    ));
    let classes = register_bridge(&mut ctx);
    CLASSES.with(|c| *c.borrow_mut() = Some(classes));

    // main(MSource) -> provider instance
    let msource = wrap(&classes_arc("MSource"), {
        let n = nmap();
        map_set(&n, "name", Value::str("Madara"));
        map_set(&n, "baseUrl", Value::str("https://example.com"));
        map_set(&n, "lang", Value::str("en"));
        n
    });
    let result = ctx
        .execute(EXTENSION, "main", vec![msource], HashMap::new())
        .expect("execute main");
    assert!(
        matches!(result, Value::Instance(_)),
        "main returned {result:?}"
    );

    // bridged-superclass getters via invoke
    assert_eq!(
        ctx.invoke("supportsLatest", vec![], HashMap::new())
            .unwrap(),
        Value::Bool(true)
    );

    // async method: getPopular(1)
    let pages = ctx
        .invoke("getPopular", vec![Value::Int(1)], HashMap::new())
        .expect("getPopular");
    let pages = ctx.pump_to_completion(pages).expect("await getPopular");
    {
        let Value::Bridged(pages_b) = &pages else {
            panic!("getPopular returned {pages:?}");
        };
        let pages_native = &pages_b.borrow().native;
        let Value::List(pages_list) = pages_native else {
            panic!("MPages native {pages_native:?}");
        };
        let items = pages_list.borrow();
        let Value::List(manga_list) = &items[0] else {
            panic!("MPages.list {items:?}");
        };
        let manga_list = manga_list.borrow();
        assert_eq!(manga_list.len(), 2);
        let Value::Bridged(m1) = &manga_list[0] else {
            panic!("not bridged")
        };
        let m1 = &m1.borrow().native;
        assert_eq!(map_str(m1, "name"), "One Piece");
        assert_eq!(map_str(m1, "link"), "/manga/one");
        assert_eq!(map_str(m1, "imageUrl"), "https://cdn.example.com/one.jpg");
        let Value::Bool(has_next) = &items[1] else {
            panic!("hasNextPage")
        };
        assert!(*has_next);
    }

    // getDetail: status bridged enum + chapters
    let detail = ctx
        .invoke("getDetail", vec![Value::str("/manga/one")], HashMap::new())
        .expect("getDetail");
    let detail = ctx.pump_to_completion(detail).expect("await getDetail");
    {
        let Value::Bridged(detail_b) = &detail else {
            panic!("detail {detail:?}")
        };
        let detail_n = &detail_b.borrow().native;
        assert_eq!(map_str(detail_n, "name"), "One Piece");
        assert_eq!(map_str(detail_n, "description"), "Pirate adventure story.");
        assert_eq!(
            map_str(detail_n, "imageUrl"),
            "https://cdn.example.com/cover.jpg"
        );
        // status: bridged enum value with native_value Int(0) (ongoing)
        match map_get(detail_n, "status") {
            Value::BridgedEnumValue(v) => {
                let v = v.borrow();
                assert_eq!(v.name, "ongoing");
                assert_eq!(v.native_value, Value::Int(0));
            }
            other => panic!("status {other:?}"),
        }
        // genre list
        match map_get(detail_n, "genre") {
            Value::List(l) => {
                let l = l.borrow();
                assert_eq!(l.len(), 2);
                assert_eq!(l[0], Value::str("Action"));
                assert_eq!(l[1], Value::str("Adventure"));
            }
            other => panic!("genre {other:?}"),
        }
        // chapters
        match map_get(detail_n, "chapters") {
            Value::List(l) => {
                let l = l.borrow();
                assert_eq!(l.len(), 2);
                let Value::Bridged(c1) = &l[0] else {
                    panic!("chapter")
                };
                let c1 = &c1.borrow().native;
                assert_eq!(map_str(c1, "name"), "Chapter 1");
                assert_eq!(map_str(c1, "url"), "/manga/one/ch/1");
            }
            other => panic!("chapters {other:?}"),
        }
    }

    // getPageList
    let pages = ctx
        .invoke(
            "getPageList",
            vec![Value::str("/manga/one/ch/1")],
            HashMap::new(),
        )
        .expect("getPageList");
    let pages = ctx.pump_to_completion(pages).expect("await getPageList");
    match pages {
        Value::List(l) => {
            let l = l.borrow();
            assert_eq!(
                l.as_slice(),
                &[
                    Value::str("https://cdn.example.com/p1.jpg"),
                    Value::str("https://cdn.example.com/p2.jpg")
                ]
            );
        }
        other => panic!("pageList {other:?}"),
    }

    // getFilterList (sync)
    let filters = ctx.invoke("getFilterList", vec![], HashMap::new()).unwrap();
    match filters {
        Value::List(l) => {
            let l = l.borrow();
            assert_eq!(l.len(), 2);
            let Value::Bridged(f) = &l[0] else {
                panic!("filter")
            };
            let f = &f.borrow().native;
            assert_eq!(map_str(f, "type"), "GenreListFilter");
            assert_eq!(map_str(f, "name"), "Genre");
        }
        other => panic!("filters {other:?}"),
    }

    // interpreted instance field access via invoke
    let src_value = ctx.invoke("source", vec![], HashMap::new()).unwrap();
    let Value::Bridged(src_b) = &src_value else {
        panic!("source field {src_value:?}")
    };
    let src_n = &src_b.borrow().native;
    assert_eq!(map_str(src_n, "name"), "Madara");
    assert_eq!(map_str(src_n, "baseUrl"), "https://example.com");
}
