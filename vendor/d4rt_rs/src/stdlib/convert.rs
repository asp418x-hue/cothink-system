//! dart:convert: JSON, UTF-8, ASCII, Latin1, Base64 codecs and helpers.
//!
//! Encoders/decoders are bridged classes wrapping `Value` data; the
//! per-instance behaviour of fused codecs is carried in the wrapped
//! native value (a `Value::List` of the two inner codec instances).

use super::helpers as h;
use crate::callable::NativeFunction;
use crate::class::BridgedClassDef;
use crate::env::Environment;
use crate::interp::InterpreterVisitor;
use crate::rt::FunctionRuntimeType;
use crate::value::{DartMap, Value};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

fn define_bridge(env: &Rc<RefCell<Environment>>, bc: BridgedClassDef) {
    let bc = Arc::new(bc);
    h::register_class(bc.clone());
    Environment::define_bridge(env, bc);
}

fn define_fn(env: &Rc<RefCell<Environment>>, name: &str, f: crate::callable::NativeFn) {
    let callable = NativeFunction {
        name: Some(name.to_string()),
        min_arity: 1,
        max_arity: Some(1),
        is_async: false,
        is_generator: false,
        closure: f,
        callable_runtime_type: FunctionRuntimeType::untyped(),
    };
    Environment::define(env, name, Value::NativeFn(Rc::new(callable)));
}

pub fn load(env: &Rc<RefCell<Environment>>) {
    // Classes
    define_bridge(env, codec_definition());
    define_bridge(env, fused_codec_definition());
    define_bridge(env, encoding_definition());
    define_bridge(env, utf8_codec_definition());
    define_bridge(env, utf8_encoder_definition());
    define_bridge(env, utf8_decoder_definition());
    define_bridge(env, ascii_codec_definition());
    define_bridge(env, ascii_encoder_definition());
    define_bridge(env, ascii_decoder_definition());
    define_bridge(env, latin1_codec_definition());
    define_bridge(env, latin1_encoder_definition());
    define_bridge(env, latin1_decoder_definition());
    define_bridge(env, base64_codec_definition());
    define_bridge(env, base64_encoder_definition());
    define_bridge(env, base64_decoder_definition());
    define_bridge(env, json_codec_definition());
    define_bridge(env, json_encoder_definition());
    define_bridge(env, json_decoder_definition());
    define_bridge(env, html_escape_definition());
    define_bridge(env, html_escape_mode_definition());
    define_bridge(env, line_splitter_definition());
    define_bridge(env, line_splitter_sink_definition());
    define_bridge(env, chunked_conversion_sink_definition());
    define_bridge(env, string_conversion_sink_definition());
    define_bridge(env, converter_definition());
    define_bridge(env, byte_conversion_sink_definition());

    // Global functions
    let json_encode = |v: &mut InterpreterVisitor,
                       args: Vec<Value>,
                       named: std::collections::HashMap<String, Value>|
     -> Result<Value, crate::InterpError> {
        let obj = args
            .into_iter()
            .next()
            .ok_or_else(|| h::err("jsonEncode requires one positional argument (object)."))?;
        let to_encodable = named.get("toEncodable").cloned();
        json_encoder(v, &obj, &to_encodable).map(h::str)
    };
    let json_decode = |v: &mut InterpreterVisitor,
                       args: Vec<Value>,
                       named: std::collections::HashMap<String, Value>|
     -> Result<Value, crate::InterpError> {
        let src = args.into_iter().next().ok_or_else(|| {
            h::err("jsonDecode requires one positional argument (String source).")
        })?;
        let src = h::as_str(&src).ok_or_else(|| {
            h::err("jsonDecode requires one positional argument (String source).")
        })?;
        let reviver = named.get("reviver").cloned();
        json_decoder(v, src, &reviver)
    };
    define_fn(env, "jsonEncode", Box::new(json_encode));
    define_fn(env, "jsonDecode", Box::new(json_decode));
    let b64 = |v: &mut InterpreterVisitor,
               args: Vec<Value>,
               _named: std::collections::HashMap<String, Value>|
     -> Result<Value, crate::InterpError> {
        let _ = v;
        let src = args.into_iter().next().ok_or_else(|| {
            h::err("base64Encode requires one positional argument (List<int> source).")
        })?;
        let bytes = value_bytes(&src)?;
        Ok(h::str(base64_encode(&bytes, false)))
    };
    let b64url = |v: &mut InterpreterVisitor,
                  args: Vec<Value>,
                  _named: std::collections::HashMap<String, Value>|
     -> Result<Value, crate::InterpError> {
        let _ = v;
        let src = args.into_iter().next().ok_or_else(|| {
            h::err("base64UrlEncode requires one positional argument (List<int> source).")
        })?;
        let bytes = value_bytes(&src)?;
        Ok(h::str(base64_encode(&bytes, true)))
    };
    let b64d = |v: &mut InterpreterVisitor,
                args: Vec<Value>,
                _named: std::collections::HashMap<String, Value>|
     -> Result<Value, crate::InterpError> {
        let _ = v;
        let src = args.into_iter().next().ok_or_else(|| {
            h::err("base64Decode requires one positional argument (String source).")
        })?;
        let s = h::as_str(&src).ok_or_else(|| h::err("base64Decode requires a String source."))?;
        Ok(bytes_value(&base64_decode(s)?))
    };
    define_fn(env, "base64Encode", Box::new(b64));
    define_fn(env, "base64UrlEncode", Box::new(b64url));
    define_fn(env, "base64Decode", Box::new(b64d));

    // Global instances
    Environment::define(env, "json", wrap_cls("JsonCodec", Value::Null));
    Environment::define(env, "base64", wrap_cls("Base64Codec", Value::Bool(false)));
    Environment::define(env, "base64Url", wrap_cls("Base64Codec", Value::Bool(true)));
    Environment::define(env, "ascii", wrap_cls("AsciiCodec", Value::Null));
    Environment::define(env, "utf8", wrap_cls("Utf8Codec", Value::Null));
    Environment::define(env, "latin1", wrap_cls("Latin1Codec", Value::Null));
    Environment::define(env, "htmlEscape", wrap_cls("HtmlEscape", Value::Null));
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn wrap_cls(name: &str, native: Value) -> Value {
    h::wrap(&h::class(name), native)
}

fn value_bytes(v: &Value) -> Result<Vec<u8>, crate::InterpError> {
    let list = h::as_list(v).ok_or_else(|| h::err("Expected a List<int>"))?;
    let mut out = Vec::with_capacity(list.borrow().len());
    for item in list.borrow().iter() {
        let b = h::as_int(item).ok_or_else(|| h::err("Expected a List<int>"))?;
        if !(0..=255).contains(&b) {
            return Err(h::err("Byte value out of range"));
        }
        out.push(b as u8);
    }
    Ok(out)
}

fn bytes_value(bytes: &[u8]) -> Value {
    Value::List(Rc::new(RefCell::new(
        bytes.iter().map(|b| Value::Int(*b as i64)).collect(),
    )))
}

// ---------------------------------------------------------------------------
// JSON
// ---------------------------------------------------------------------------

pub fn json_encoder(
    v: &mut InterpreterVisitor,
    value: &Value,
    to_encodable: &Option<Value>,
) -> Result<String, crate::InterpError> {
    let mut out = String::new();
    encode_json_value(v, value, to_encodable, &mut out)?;
    Ok(out)
}

fn encode_json_value(
    v: &mut InterpreterVisitor,
    value: &Value,
    to_encodable: &Option<Value>,
    out: &mut String,
) -> Result<(), crate::InterpError> {
    match value {
        Value::Null => {
            out.push_str("null");
            Ok(())
        }
        Value::Bool(b) => {
            out.push_str(if *b { "true" } else { "false" });
            Ok(())
        }
        Value::Int(i) => {
            out.push_str(&i.to_string());
            Ok(())
        }
        Value::Double(f) => {
            if f.is_finite() && f.fract() == 0.0 {
                out.push_str(&format!("{:.1}", f));
            } else {
                out.push_str(&f.to_string());
            }
            Ok(())
        }
        Value::Str(s) => {
            encode_json_string(s, out);
            Ok(())
        }
        Value::BigInt(b) => {
            out.push_str(&b.to_string());
            Ok(())
        }
        Value::List(l) => {
            out.push('[');
            let items = l.borrow();
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                encode_json_value(v, item, to_encodable, out)?;
            }
            drop(items);
            out.push(']');
            Ok(())
        }
        Value::Map(m) => {
            out.push('{');
            let entries = m.borrow();
            let mut first = true;
            for (k, val) in entries.entries.iter() {
                if !first {
                    out.push(',');
                }
                first = false;
                match k {
                    Value::Str(s) => encode_json_string(s, out),
                    other => encode_json_string(&h::display(other), out),
                }
                out.push(':');
                encode_json_value(v, val, to_encodable, out)?;
            }
            drop(entries);
            out.push('}');
            Ok(())
        }
        other => match to_encodable {
            Some(f) => {
                let converted =
                    v.call_value(f, vec![other.clone()], std::collections::HashMap::new())?;
                encode_json_value(v, &converted, to_encodable, out)
            }
            None => Err(h::err(format!(
                "Converting object to an encodable object failed: {}",
                h::display(other)
            ))),
        },
    }
}

fn encode_json_string(s: &str, out: &mut String) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{08}' => out.push_str("\\b"),
            '\u{0c}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
}

fn json_revive(
    v: &mut InterpreterVisitor,
    value: Value,
    key: &Value,
    reviver: &Value,
) -> Result<Value, crate::InterpError> {
    let new_value = match value {
        Value::List(l) => {
            let items = l.borrow();
            let mut new_items = Vec::with_capacity(items.len());
            for (i, item) in items.iter().enumerate() {
                let revived = json_revive(v, item.clone(), &Value::Int(i as i64), reviver)?;
                new_items.push(revived);
            }
            drop(items);
            Value::List(Rc::new(RefCell::new(new_items)))
        }
        Value::Map(m) => {
            let entries = m.borrow();
            let mut new_map = DartMap::new();
            for (k, val) in entries.entries.iter() {
                let revived = json_revive(v, val.clone(), k, reviver)?;
                new_map.entries.push((k.clone(), revived));
            }
            drop(entries);
            Value::Map(Rc::new(RefCell::new(new_map)))
        }
        other => other,
    };
    v.call_value(
        reviver,
        vec![key.clone(), new_value.clone()],
        std::collections::HashMap::new(),
    )
}

pub fn json_decoder(
    v: &mut InterpreterVisitor,
    source: &str,
    reviver: &Option<Value>,
) -> Result<Value, crate::InterpError> {
    let (value, rest) = parse_json(source, 0)?;
    // Trailing whitespace (e.g. a final newline in a response body) is
    // allowed, mirroring dart:convert's lenient behaviour.
    if source[rest..].trim() != "" {
        return Err(h::err("Unexpected character at end of JSON"));
    }
    if let Some(r) = reviver {
        json_revive(v, value, &h::str(""), r)
    } else {
        Ok(value)
    }
}

pub fn parse_json(src: &str, pos: usize) -> Result<(Value, usize), crate::InterpError> {
    let b = src.as_bytes();
    let mut i = skip_ws(src, pos);
    if i >= b.len() {
        return Err(h::err("Unexpected end of JSON"));
    }
    match b[i] {
        b'{' => {
            i += 1;
            let mut map = DartMap::new();
            i = skip_ws(src, i);
            if i < b.len() && b[i] == b'}' {
                return Ok((Value::Map(Rc::new(RefCell::new(map))), i + 1));
            }
            loop {
                i = skip_ws(src, i);
                if i >= b.len() || b[i] != b'"' {
                    return Err(h::err("Expected JSON string key"));
                }
                let (k, j) = parse_json_string(src, i)?;
                i = skip_ws(src, j);
                if i >= b.len() || b[i] != b':' {
                    return Err(h::err("Expected ':' in JSON object"));
                }
                i += 1;
                let (val, j) = parse_json(src, i)?;
                map.entries.push((h::str(k), val));
                i = skip_ws(src, j);
                if i < b.len() && b[i] == b',' {
                    i += 1;
                    continue;
                }
                if i < b.len() && b[i] == b'}' {
                    return Ok((Value::Map(Rc::new(RefCell::new(map))), i + 1));
                }
                return Err(h::err("Expected ',' or '}' in JSON object"));
            }
        }
        b'[' => {
            i += 1;
            let mut items = Vec::new();
            i = skip_ws(src, i);
            if i < b.len() && b[i] == b']' {
                return Ok((Value::List(Rc::new(RefCell::new(items))), i + 1));
            }
            loop {
                let (val, j) = parse_json(src, i)?;
                items.push(val);
                i = skip_ws(src, j);
                if i < b.len() && b[i] == b',' {
                    i += 1;
                    continue;
                }
                if i < b.len() && b[i] == b']' {
                    return Ok((Value::List(Rc::new(RefCell::new(items))), i + 1));
                }
                return Err(h::err("Expected ',' or ']' in JSON array"));
            }
        }
        b'"' => {
            let (s, j) = parse_json_string(src, i)?;
            Ok((h::str(s), j))
        }
        b't' if src[i..].starts_with("true") => Ok((Value::Bool(true), i + 4)),
        b'f' if src[i..].starts_with("false") => Ok((Value::Bool(false), i + 5)),
        b'n' if src[i..].starts_with("null") => Ok((Value::Null, i + 4)),
        b'-' | b'0'..=b'9' => parse_json_number(src, i),
        _ => Err(h::err(format!(
            "Unexpected character in JSON: {:?}",
            b[i] as char
        ))),
    }
}

fn parse_json_number(src: &str, pos: usize) -> Result<(Value, usize), crate::InterpError> {
    let b = src.as_bytes();
    let start = pos;
    let mut i = pos;
    if i < b.len() && b[i] == b'-' {
        i += 1;
    }
    while i < b.len() && b[i].is_ascii_digit() {
        i += 1;
    }
    let mut is_float = false;
    if i < b.len() && (b[i] == b'.' || b[i] == b'e' || b[i] == b'E') {
        is_float = true;
        if i < b.len() && b[i] == b'.' {
            i += 1;
            while i < b.len() && b[i].is_ascii_digit() {
                i += 1;
            }
        }
        if i < b.len() && (b[i] == b'e' || b[i] == b'E') {
            i += 1;
            if i < b.len() && (b[i] == b'+' || b[i] == b'-') {
                i += 1;
            }
            while i < b.len() && b[i].is_ascii_digit() {
                i += 1;
            }
        }
    }
    let text = &src[start..i];
    if is_float {
        let f: f64 = text.parse().map_err(|_| h::err("Invalid JSON number"))?;
        Ok((Value::Double(f), i))
    } else {
        let n: i64 = text.parse().map_err(|_| h::err("Invalid JSON number"))?;
        Ok((Value::Int(n), i))
    }
}

fn parse_json_string(src: &str, pos: usize) -> Result<(String, usize), crate::InterpError> {
    let b = src.as_bytes();
    let mut i = pos + 1;
    let mut out = String::new();
    loop {
        if i >= b.len() {
            return Err(h::err("Unterminated JSON string"));
        }
        match b[i] {
            b'"' => return Ok((out, i + 1)),
            b'\\' => {
                i += 1;
                if i >= b.len() {
                    return Err(h::err("Unterminated JSON escape"));
                }
                match b[i] {
                    b'"' => out.push('"'),
                    b'\\' => out.push('\\'),
                    b'/' => out.push('/'),
                    b'b' => out.push('\u{08}'),
                    b'f' => out.push('\u{0c}'),
                    b'n' => out.push('\n'),
                    b'r' => out.push('\r'),
                    b't' => out.push('\t'),
                    b'u' => {
                        if i + 4 >= b.len() {
                            return Err(h::err("Invalid JSON unicode escape"));
                        }
                        let hex = &src[i + 1..i + 5];
                        let cp = u32::from_str_radix(hex, 16)
                            .map_err(|_| h::err("Invalid JSON unicode escape"))?;
                        out.push(char::from_u32(cp).unwrap_or('\u{fffd}'));
                        i += 4;
                    }
                    _ => return Err(h::err("Invalid JSON escape")),
                }
                i += 1;
            }
            _ => {
                let ch = src[i..].chars().next().unwrap();
                out.push(ch);
                i += ch.len_utf8();
            }
        }
    }
}

fn skip_ws(src: &str, mut i: usize) -> usize {
    let b = src.as_bytes();
    while i < b.len() && (b[i] == b' ' || b[i] == b'\t' || b[i] == b'\n' || b[i] == b'\r') {
        i += 1;
    }
    i
}

// ---------------------------------------------------------------------------
// Base64
// ---------------------------------------------------------------------------

const B64_ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

pub fn base64_encode(bytes: &[u8], url_safe: bool) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = chunk.get(1).copied().unwrap_or(0) as u32;
        let b2 = chunk.get(2).copied().unwrap_or(0) as u32;
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(alphabet((n >> 18) & 63, url_safe));
        out.push(alphabet((n >> 12) & 63, url_safe));
        out.push(if chunk.len() > 1 {
            alphabet((n >> 6) & 63, url_safe)
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            alphabet(n & 63, url_safe)
        } else {
            '='
        });
    }
    out
}

fn alphabet(i: u32, url_safe: bool) -> char {
    let c = B64_ALPHABET[i as usize];
    if url_safe {
        match c {
            b'+' => '-',
            b'/' => '_',
            c => c as char,
        }
    } else {
        c as char
    }
}

fn b64_value(c: char, url_safe: bool) -> Option<u32> {
    let c = if url_safe && c == '-' {
        '+'
    } else if url_safe && c == '_' {
        '/'
    } else {
        c
    };
    B64_ALPHABET
        .iter()
        .position(|&x| x as char == c)
        .map(|p| p as u32)
}

pub fn base64_decode(s: &str) -> Result<Vec<u8>, crate::InterpError> {
    let url_safe = s.contains('-') || s.contains('_');
    let mut out = Vec::with_capacity(s.len() / 4 * 3);
    let chars: Vec<char> = s.chars().filter(|c| *c != '=').collect();
    let mut i = 0;
    while i < chars.len() {
        let a = b64_value(chars[i], url_safe).ok_or_else(|| h::err("Invalid base64 data"))?;
        let b = if i + 1 < chars.len() {
            b64_value(chars[i + 1], url_safe).ok_or_else(|| h::err("Invalid base64 data"))?
        } else {
            0
        };
        let c = if i + 2 < chars.len() {
            b64_value(chars[i + 2], url_safe).ok_or_else(|| h::err("Invalid base64 data"))?
        } else {
            0
        };
        let d = if i + 3 < chars.len() {
            b64_value(chars[i + 3], url_safe).ok_or_else(|| h::err("Invalid base64 data"))?
        } else {
            0
        };
        let n = (a << 18) | (b << 12) | (c << 6) | d;
        out.push((n >> 16) as u8);
        if i + 2 < chars.len() {
            out.push((n >> 8) as u8);
        }
        if i + 3 < chars.len() {
            out.push(n as u8);
        }
        i += 4;
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Byte<->String encodings
// ---------------------------------------------------------------------------

fn utf8_decode(v: &Value) -> Result<String, crate::InterpError> {
    let bytes = value_bytes(v)?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

fn utf8_encode(s: &str) -> Value {
    bytes_value(s.as_bytes())
}

fn ascii_encode(s: &str) -> Result<Value, crate::InterpError> {
    if s.bytes().any(|b| b > 127) {
        return Err(h::err("Invalid ASCII: non-ASCII character"));
    }
    Ok(bytes_value(s.as_bytes()))
}

fn ascii_decode(v: &Value) -> Result<String, crate::InterpError> {
    let bytes = value_bytes(v)?;
    Ok(bytes.iter().map(|b| *b as char).collect())
}

fn latin1_encode(s: &str) -> Result<Value, crate::InterpError> {
    let mut out = Vec::with_capacity(s.len());
    for c in s.chars() {
        if (c as u32) > 0xFF {
            return Err(h::err("Invalid Latin-1: character outside range"));
        }
        out.push(c as u8);
    }
    Ok(bytes_value(&out))
}

fn latin1_decode(v: &Value) -> Result<String, crate::InterpError> {
    let bytes = value_bytes(v)?;
    Ok(bytes.iter().map(|b| *b as char).collect())
}

// ---------------------------------------------------------------------------
// Class definitions
// ---------------------------------------------------------------------------

fn codec_definition() -> BridgedClassDef {
    // Base class; used for type annotations and dynamic dispatch on
    // fused codecs. encode/decode are provided by subclasses' instances,
    // so this class only carries the shared surface.
    let mut bc = BridgedClassDef::new("Codec");
    bc.native_type = Some("d4rt::convert::Codec".to_string());
    bc
}

fn fused_codec_definition() -> BridgedClassDef {
    // Native value: Value::List([inner_codec_a, inner_codec_b]).
    // encode(x) = b.encode(a.encode(x)); decode(x) = a.decode(b.decode(x)).
    let mut bc = BridgedClassDef::new("_FusedCodec");
    bc.native_type = Some("d4rt::convert::_FusedCodec".to_string());
    bc.methods.insert(
        "encode".into(),
        h::method(|v, t, args, _n| {
            h::require_args(&args, 1, "encode")?;
            let inners = h::as_list(&t).ok_or_else(|| h::err("Not a fused codec"))?;
            let inner = inners.borrow();
            let (a, b) = (inner[0].clone(), inner[1].clone());
            let a_enc = invoke_method(v, &a, "encode", vec![args[0].clone()])?;
            invoke_method(v, &b, "encode", vec![a_enc])
        }),
    );
    bc.methods.insert(
        "decode".into(),
        h::method(|v, t, args, _n| {
            h::require_args(&args, 1, "decode")?;
            let inners = h::as_list(&t).ok_or_else(|| h::err("Not a fused codec"))?;
            let inner = inners.borrow();
            let (a, b) = (inner[0].clone(), inner[1].clone());
            let b_dec = invoke_method(v, &b, "decode", vec![args[0].clone()])?;
            invoke_method(v, &a, "decode", vec![b_dec])
        }),
    );
    bc.getters.insert(
        "encoder".into(),
        h::getter(|v, t| {
            let inners = h::as_list(&t).ok_or_else(|| h::err("Not a fused codec"))?;
            let inner = inners.borrow();
            let (a, b) = (inner[0].clone(), inner[1].clone());
            let a_enc = invoke_method(v, &a, "encoder", vec![])?;
            invoke_method(v, &b, "encoder", vec![a_enc])
        }),
    );
    bc.getters.insert(
        "decoder".into(),
        h::getter(|v, t| {
            let inners = h::as_list(&t).ok_or_else(|| h::err("Not a fused codec"))?;
            let inner = inners.borrow();
            let (a, b) = (inner[0].clone(), inner[1].clone());
            let b_dec = invoke_method(v, &b, "decoder", vec![])?;
            invoke_method(v, &a, "decoder", vec![b_dec])
        }),
    );
    bc
}

fn invoke_method(
    v: &mut InterpreterVisitor,
    target: &Value,
    name: &str,
    args: Vec<Value>,
) -> Result<Value, crate::InterpError> {
    let Value::Bridged(b) = target else {
        return Err(h::err(format!(
            "Cannot call method {name} on non-bridged value"
        )));
    };
    let bc = b.borrow().bridged_class.clone();
    let native = b.borrow().native.clone();
    if let Some(m) = bc.find_instance_method_adapter(name) {
        return m(v, native, args, std::collections::HashMap::new());
    }
    if let Some(g) = bc.find_instance_getter_adapter(name) {
        return g(v, native);
    }
    Err(h::err(format!("No method {name} on {}", bc.name)))
}

fn encoding_definition() -> BridgedClassDef {
    let mut bc = BridgedClassDef::new("Encoding");
    bc.native_type = Some("d4rt::convert::Encoding".to_string());
    bc.static_methods.insert(
        "getByName".to_string(),
        h::static_method(|_v, args, _n| {
            let name = args
                .first()
                .and_then(|v| h::as_str(v))
                .unwrap_or("")
                .to_ascii_lowercase();
            let codec = match name.as_str() {
                "utf-8" | "utf8" => Some("Utf8Codec"),
                "latin-1" | "latin1" | "iso-8859-1" => Some("Latin1Codec"),
                "ascii" | "us-ascii" => Some("AsciiCodec"),
                _ => None,
            };
            Ok(codec
                .map(|c| wrap_cls(c, Value::Null))
                .unwrap_or(Value::Null))
        }),
    );
    bc
}

fn single_byte_codec(
    name: &str,
    enc: fn(&str) -> Result<Value, crate::InterpError>,
    dec: fn(&Value) -> Result<String, crate::InterpError>,
    encoder_cls: &'static str,
    decoder_cls: &'static str,
) -> BridgedClassDef {
    let name = name.to_string();
    let mut bc = BridgedClassDef::new(&name);
    bc.native_type = Some(format!("d4rt::convert::{name}"));
    let e = encoder_cls;
    let d = decoder_cls;
    let n = name.clone();
    bc.constructors.insert(
        "".into(),
        h::ctor(move |_v, _a, _n| Ok(wrap_cls(&n, Value::Null))),
    );
    bc.methods.insert(
        "encode".into(),
        h::method(move |_v, _t, args, _n| {
            h::require_args(&args, 1, "encode")?;
            let s = h::as_str(&args[0]).ok_or_else(|| h::err("encode expects a String"))?;
            enc(s)
        }),
    );
    bc.methods.insert(
        "decode".into(),
        h::method(move |_v, _t, args, _n| {
            h::require_args(&args, 1, "decode")?;
            let bytes = dec(&args[0])?;
            Ok(h::str(bytes))
        }),
    );
    let e2 = e;
    bc.getters.insert(
        "encoder".into(),
        h::getter(move |_v, _t| Ok(wrap_cls(e2, Value::Null))),
    );
    let d2 = d;
    bc.getters.insert(
        "decoder".into(),
        h::getter(move |_v, _t| Ok(wrap_cls(d2, decoder_native(d2, dec)))),
    );
    let fuse_name = name.clone();
    bc.methods.insert(
        "fuse".into(),
        h::method(move |v, t, args, _n| {
            h::require_args(&args, 1, "fuse")?;
            fused(wrap_cls(&fuse_name, t), args[0].clone(), v)
        }),
    );
    bc
}

fn fused(
    inner_a: Value,
    inner_b: Value,
    _v: &mut InterpreterVisitor,
) -> Result<Value, crate::InterpError> {
    Ok(wrap_cls(
        "_FusedCodec",
        Value::List(Rc::new(RefCell::new(vec![inner_a, inner_b]))),
    ))
}

fn utf8_codec_definition() -> BridgedClassDef {
    single_byte_codec(
        "Utf8Codec",
        |s| Ok(utf8_encode(s)),
        utf8_decode,
        "Utf8Encoder",
        "Utf8Decoder",
    )
}

fn ascii_codec_definition() -> BridgedClassDef {
    single_byte_codec(
        "AsciiCodec",
        ascii_encode,
        ascii_decode,
        "AsciiEncoder",
        "AsciiDecoder",
    )
}

fn latin1_codec_definition() -> BridgedClassDef {
    single_byte_codec(
        "Latin1Codec",
        latin1_encode,
        latin1_decode,
        "Latin1Encoder",
        "Latin1Decoder",
    )
}

/// Native payload for a decoder: `[inner, bind, [hd, he, hd_done]]` so
/// `Stream.transform(decoder)` decodes byte chunks via the K_HANDLER
/// pipeline; the hd pushes decoded strings into the EventSink.
fn decoder_native(name: &str, dec: fn(&Value) -> Result<String, crate::InterpError>) -> Value {
    let name = name.to_string();
    let hd = NativeFunction {
        name: Some(format!("<{name} stream hd>")),
        min_arity: 1,
        max_arity: Some(2),
        is_async: false,
        is_generator: false,
        closure: Box::new(move |visitor, args, _n| {
            let event = args.first().cloned().unwrap_or(Value::Null);
            let sink = args.get(1).cloned().unwrap_or(Value::Null);
            match dec(&event) {
                Ok(s) => {
                    crate::stdlib::stream_lib::sink_add_value(visitor, &sink, Value::str(s));
                }
                Err(e) => {
                    crate::stdlib::stream_lib::sink_add_error_value(
                        visitor,
                        &sink,
                        crate::async_state::exception_value(e),
                    );
                }
            }
            Ok(Value::Null)
        }),
        callable_runtime_type: FunctionRuntimeType::untyped(),
    };
    let done_hd = NativeFunction {
        name: Some(format!("<{name} stream done>")),
        min_arity: 1,
        max_arity: Some(1),
        is_async: false,
        is_generator: false,
        closure: Box::new(move |visitor, args, _n| {
            let sink = args.first().cloned().unwrap_or(Value::Null);
            crate::stdlib::stream_lib::sink_close_value(visitor, &sink);
            Ok(Value::Null)
        }),
        callable_runtime_type: FunctionRuntimeType::untyped(),
    };
    Value::List(Rc::new(RefCell::new(vec![
        Value::Null,
        Value::Null,
        Value::List(Rc::new(RefCell::new(vec![
            Value::NativeFn(Rc::new(hd)),
            Value::Null,
            Value::NativeFn(Rc::new(done_hd)),
        ]))),
    ])))
}

fn single_byte_converter(
    name: &str,
    enc: fn(&str) -> Result<Value, crate::InterpError>,
    dec: fn(&Value) -> Result<String, crate::InterpError>,
    is_encoder: bool,
) -> BridgedClassDef {
    let name = name.to_string();
    let mut bc = BridgedClassDef::new(&name);
    bc.native_type = Some(format!("d4rt::convert::{name}"));
    let n = name.clone();
    bc.constructors.insert(
        "".into(),
        h::ctor(move |_v, _a, _n| {
            let decoder = if is_encoder {
                Value::Null
            } else {
                decoder_native(&n, dec)
            };
            Ok(wrap_cls(&n, decoder))
        }),
    );
    bc.methods.insert(
        "convert".into(),
        h::method(move |_v, _t, args, _n| {
            h::require_args(&args, 1, "convert")?;
            if is_encoder {
                let s = h::as_str(&args[0]).ok_or_else(|| h::err("convert expects a String"))?;
                enc(s)
            } else {
                let s = dec(&args[0])?;
                Ok(h::str(s))
            }
        }),
    );
    bc.methods.insert(
        "fuse".into(),
        h::method(|v, t, args, _n| {
            h::require_args(&args, 1, "fuse")?;
            fused(t.clone(), args[0].clone(), v)
        }),
    );
    bc
}

fn utf8_encoder_definition() -> BridgedClassDef {
    single_byte_converter("Utf8Encoder", |s| Ok(utf8_encode(s)), utf8_decode, true)
}

fn utf8_decoder_definition() -> BridgedClassDef {
    single_byte_converter("Utf8Decoder", |s| Ok(utf8_encode(s)), utf8_decode, false)
}

fn ascii_encoder_definition() -> BridgedClassDef {
    single_byte_converter("AsciiEncoder", ascii_encode, ascii_decode, true)
}

fn ascii_decoder_definition() -> BridgedClassDef {
    single_byte_converter("AsciiDecoder", ascii_encode, ascii_decode, false)
}

fn latin1_encoder_definition() -> BridgedClassDef {
    single_byte_converter("Latin1Encoder", latin1_encode, latin1_decode, true)
}

fn latin1_decoder_definition() -> BridgedClassDef {
    single_byte_converter("Latin1Decoder", latin1_encode, latin1_decode, false)
}

fn base64_codec_definition() -> BridgedClassDef {
    // Native value: Value::Bool(urlSafe) or Null for default.
    let mut bc = BridgedClassDef::new("Base64Codec");
    bc.native_type = Some("d4rt::convert::Base64Codec".to_string());
    bc.constructors.insert(
        "".into(),
        h::ctor(|_v, _a, named| {
            let url_safe = matches!(named.get("urlSafe"), Some(Value::Bool(true)));
            Ok(wrap_cls("Base64Codec", Value::Bool(url_safe)))
        }),
    );
    bc.methods.insert(
        "encode".into(),
        h::method(|_v, t, args, _n| {
            h::require_args(&args, 1, "encode")?;
            let bytes = value_bytes(&args[0])?;
            Ok(h::str(base64_encode(&bytes, url_safe_of(&t))))
        }),
    );
    bc.methods.insert(
        "decode".into(),
        h::method(|_v, t, args, _n| {
            h::require_args(&args, 1, "decode")?;
            let s = h::as_str(&args[0]).ok_or_else(|| h::err("decode expects a String"))?;
            let _ = url_safe_of(&t);
            Ok(bytes_value(&base64_decode(s)?))
        }),
    );
    bc.methods.insert(
        "normalize".into(),
        h::method(|_v, t, args, _n| {
            h::require_args(&args, 1, "normalize")?;
            let s = h::as_str(&args[0]).ok_or_else(|| h::err("normalize expects a String"))?;
            let _ = url_safe_of(&t);
            let bytes = base64_decode(s)?;
            Ok(h::str(base64_encode(&bytes, false)))
        }),
    );
    bc.getters.insert(
        "encoder".into(),
        h::getter(|_v, t| Ok(wrap_cls("Base64Encoder", t.clone()))),
    );
    bc.getters.insert(
        "decoder".into(),
        h::getter(|_v, t| Ok(wrap_cls("Base64Decoder", t.clone()))),
    );
    bc.methods.insert(
        "fuse".into(),
        h::method(|v, t, args, _n| {
            h::require_args(&args, 1, "fuse")?;
            fused(wrap_cls("Base64Codec", t), args[0].clone(), v)
        }),
    );
    bc
}

fn url_safe_of(t: &Value) -> bool {
    matches!(t, Value::Bool(true))
}

fn base64_encoder_definition() -> BridgedClassDef {
    let mut bc = BridgedClassDef::new("Base64Encoder");
    bc.native_type = Some("d4rt::convert::Base64Encoder".to_string());
    bc.constructors.insert(
        "".into(),
        h::ctor(|_v, _a, named| {
            let url_safe = matches!(named.get("urlSafe"), Some(Value::Bool(true)));
            Ok(wrap_cls("Base64Encoder", Value::Bool(url_safe)))
        }),
    );
    bc.methods.insert(
        "convert".into(),
        h::method(|_v, t, args, _n| {
            h::require_args(&args, 1, "convert")?;
            let bytes = value_bytes(&args[0])?;
            Ok(h::str(base64_encode(&bytes, url_safe_of(&t))))
        }),
    );
    bc
}

fn base64_decoder_definition() -> BridgedClassDef {
    let mut bc = BridgedClassDef::new("Base64Decoder");
    bc.native_type = Some("d4rt::convert::Base64Decoder".to_string());
    bc.constructors.insert(
        "".into(),
        h::ctor(|_v, _a, _n| Ok(wrap_cls("Base64Decoder", Value::Null))),
    );
    bc.methods.insert(
        "convert".into(),
        h::method(|_v, _t, args, _n| {
            h::require_args(&args, 1, "convert")?;
            let s = h::as_str(&args[0]).ok_or_else(|| h::err("convert expects a String"))?;
            Ok(bytes_value(&base64_decode(s)?))
        }),
    );
    bc
}

fn json_codec_definition() -> BridgedClassDef {
    // Native value: Value::List([toEncodable or Null, reviver or Null]).
    let mut bc = BridgedClassDef::new("JsonCodec");
    bc.native_type = Some("d4rt::convert::JsonCodec".to_string());
    bc.constructors.insert(
        "".into(),
        h::ctor(|_v, _a, named| {
            let to_encodable = named.get("toEncodable").cloned().unwrap_or(Value::Null);
            let reviver = named.get("reviver").cloned().unwrap_or(Value::Null);
            Ok(wrap_cls(
                "JsonCodec",
                Value::List(Rc::new(RefCell::new(vec![to_encodable, reviver]))),
            ))
        }),
    );
    bc.methods.insert(
        "encode".into(),
        h::method(|v, t, args, _n| {
            h::require_args(&args, 1, "encode")?;
            let conf = h::as_list(&t).ok_or_else(|| h::err("Not a JsonCodec"))?;
            let conf = conf.borrow();
            let to_encodable = if matches!(conf[0], Value::Null) {
                None
            } else {
                Some(conf[0].clone())
            };
            let out = json_encoder(v, &args[0], &to_encodable)?;
            Ok(h::str(out))
        }),
    );
    bc.methods.insert(
        "decode".into(),
        h::method(|v, t, args, named| {
            h::require_args(&args, 1, "decode")?;
            let s = h::as_str(&args[0]).ok_or_else(|| h::err("decode expects a String"))?;
            let conf = h::as_list(&t).ok_or_else(|| h::err("Not a JsonCodec"))?;
            let conf = conf.borrow();
            let mut reviver = if matches!(conf[1], Value::Null) {
                None
            } else {
                Some(conf[1].clone())
            };
            if let Some(r) = named.get("reviver") {
                reviver = Some(r.clone());
            }
            json_decoder(v, s, &reviver)
        }),
    );
    bc.getters.insert(
        "encoder".into(),
        h::getter(|_v, t| {
            let conf = h::as_list(&t).ok_or_else(|| h::err("Not a JsonCodec"))?;
            let conf = conf.borrow();
            let to_encodable = if matches!(conf[0], Value::Null) {
                None
            } else {
                Some(conf[0].clone())
            };
            Ok(wrap_cls("JsonEncoder", conf_to_value(&to_encodable)))
        }),
    );
    bc.getters.insert(
        "decoder".into(),
        h::getter(|_v, t| {
            let conf = h::as_list(&t).ok_or_else(|| h::err("Not a JsonCodec"))?;
            let conf = conf.borrow();
            let reviver = if matches!(conf[1], Value::Null) {
                None
            } else {
                Some(conf[1].clone())
            };
            Ok(wrap_cls("JsonDecoder", conf_to_value(&reviver)))
        }),
    );
    bc
}

fn conf_to_value(conf: &Option<Value>) -> Value {
    match conf {
        Some(f) => f.clone(),
        None => Value::Null,
    }
}

fn json_encoder_definition() -> BridgedClassDef {
    // Native value: toEncodable closure or Null.
    let mut bc = BridgedClassDef::new("JsonEncoder");
    bc.native_type = Some("d4rt::convert::JsonEncoder".to_string());
    bc.constructors.insert(
        "".into(),
        h::ctor(|_v, args, _n| {
            let to_encodable = args.first().cloned().unwrap_or(Value::Null);
            Ok(wrap_cls("JsonEncoder", to_encodable))
        }),
    );
    bc.methods.insert(
        "convert".into(),
        h::method(|v, t, args, _n| {
            h::require_args(&args, 1, "convert")?;
            let to_encodable = if matches!(t, Value::Null) {
                None
            } else {
                Some(t.clone())
            };
            let out = json_encoder(v, &args[0], &to_encodable)?;
            Ok(h::str(out))
        }),
    );
    bc.methods.insert(
        "fuse".into(),
        h::method(|v, t, args, _n| {
            h::require_args(&args, 1, "fuse")?;
            fused(wrap_cls("JsonEncoder", t), args[0].clone(), v)
        }),
    );
    bc
}

fn json_decoder_definition() -> BridgedClassDef {
    // Native value: reviver closure or Null.
    let mut bc = BridgedClassDef::new("JsonDecoder");
    bc.native_type = Some("d4rt::convert::JsonDecoder".to_string());
    bc.constructors.insert(
        "".into(),
        h::ctor(|_v, args, _n| {
            let reviver = args.first().cloned().unwrap_or(Value::Null);
            Ok(wrap_cls("JsonDecoder", reviver))
        }),
    );
    bc.methods.insert(
        "convert".into(),
        h::method(|v, t, args, _n| {
            h::require_args(&args, 1, "convert")?;
            let s = h::as_str(&args[0]).ok_or_else(|| h::err("convert expects a String"))?;
            let reviver = if matches!(t, Value::Null) {
                None
            } else {
                Some(t.clone())
            };
            json_decoder(v, s, &reviver)
        }),
    );
    bc
}

// ---------------------------------------------------------------------------
// HtmlEscape / HtmlEscapeMode
// ---------------------------------------------------------------------------

// HtmlEscapeMode native: List([name, escapeLtGt, escapeQuot, escapeApos,
// escapeSlash]). HtmlEscape native: List([escapeQuot, escapeApos, escapeLtGt,
// escapeSlash]); a Null native means the default "unknown" mode.

fn mode_native(name: &str, ltgt: bool, quot: bool, apos: bool, slash: bool) -> Value {
    Value::List(Rc::new(RefCell::new(vec![
        Value::str(name),
        Value::Bool(ltgt),
        Value::Bool(quot),
        Value::Bool(apos),
        Value::Bool(slash),
    ])))
}

fn mode_flags(t: &Value) -> (bool, bool, bool, bool) {
    match t {
        Value::Null => (true, true, true, true),
        other => {
            let l = h::as_list(other).unwrap_or_else(|| Rc::new(RefCell::new(vec![])));
            let l = l.borrow();
            (
                l.get(1).and_then(h::as_bool).unwrap_or(true),
                l.get(2).and_then(h::as_bool).unwrap_or(true),
                l.get(3).and_then(h::as_bool).unwrap_or(true),
                l.get(4).and_then(h::as_bool).unwrap_or(true),
            )
        }
    }
}

/// Flags for the HtmlEscape native layout: List([ltgt, quot, apos, slash]).
/// A Null native means the default "unknown" mode.
fn escape_flags(t: &Value) -> (bool, bool, bool, bool) {
    match t {
        Value::Null => (true, true, true, true),
        other => {
            let l = h::as_list(other).unwrap_or_else(|| Rc::new(RefCell::new(vec![])));
            let l = l.borrow();
            (
                l.first().and_then(h::as_bool).unwrap_or(true),
                l.get(1).and_then(h::as_bool).unwrap_or(true),
                l.get(2).and_then(h::as_bool).unwrap_or(true),
                l.get(3).and_then(h::as_bool).unwrap_or(true),
            )
        }
    }
}

fn escape_html(s: &str, ltgt: bool, quot: bool, apos: bool, slash: bool) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '"' if quot => out.push_str("&quot;"),
            '\'' if apos => out.push_str("&#39;"),
            '<' if ltgt => out.push_str("&lt;"),
            '>' if ltgt => out.push_str("&gt;"),
            '/' if slash => out.push_str("&#47;"),
            other => out.push(other),
        }
    }
    out
}

fn html_escape_mode_definition() -> BridgedClassDef {
    let mut bc = BridgedClassDef::new("HtmlEscapeMode");
    bc.native_type = Some("d4rt::convert::HtmlEscapeMode".to_string());
    bc.constructors.insert(
        "".into(),
        h::ctor(|_v, _args, named| {
            let name = named.get("name").and_then(h::as_str).unwrap_or("custom");
            let ltgt = named
                .get("escapeLtGt")
                .and_then(h::as_bool)
                .unwrap_or(false);
            let quot = named
                .get("escapeQuot")
                .and_then(h::as_bool)
                .unwrap_or(false);
            let apos = named
                .get("escapeApos")
                .and_then(h::as_bool)
                .unwrap_or(false);
            let slash = named
                .get("escapeSlash")
                .and_then(h::as_bool)
                .unwrap_or(false);
            Ok(wrap_cls(
                "HtmlEscapeMode",
                mode_native(name, ltgt, quot, apos, slash),
            ))
        }),
    );
    bc.static_getters.insert(
        "unknown".into(),
        h::static_getter(|_v| {
            Ok(wrap_cls(
                "HtmlEscapeMode",
                mode_native("unknown", true, true, true, true),
            ))
        }),
    );
    bc.static_getters.insert(
        "attribute".into(),
        h::static_getter(|_v| {
            Ok(wrap_cls(
                "HtmlEscapeMode",
                mode_native("attribute", true, true, false, false),
            ))
        }),
    );
    bc.static_getters.insert(
        "sqAttribute".into(),
        h::static_getter(|_v| {
            Ok(wrap_cls(
                "HtmlEscapeMode",
                mode_native("attribute", true, false, true, false),
            ))
        }),
    );
    bc.static_getters.insert(
        "element".into(),
        h::static_getter(|_v| {
            Ok(wrap_cls(
                "HtmlEscapeMode",
                mode_native("element", true, false, false, false),
            ))
        }),
    );
    bc.methods.insert(
        "toString".into(),
        h::method(|_v, t, _args, _n| {
            let l = h::as_list(&t).ok_or_else(|| h::err("Not an HtmlEscapeMode"))?;
            let name = l
                .borrow()
                .first()
                .and_then(h::as_str)
                .map(|s| s.to_string())
                .unwrap_or_else(|| "custom".to_string());
            Ok(h::str(name))
        }),
    );
    bc
}

fn html_escape_definition() -> BridgedClassDef {
    let mut bc = BridgedClassDef::new("HtmlEscape");
    bc.native_type = Some("d4rt::convert::HtmlEscape".to_string());
    bc.constructors.insert(
        "".into(),
        h::ctor(|_v, args, _n| {
            let mode = args.first().cloned().unwrap_or(Value::Null);
            let (ltgt, quot, apos, slash) = mode_flags(&mode);
            Ok(wrap_cls(
                "HtmlEscape",
                Value::List(Rc::new(RefCell::new(vec![
                    Value::Bool(ltgt),
                    Value::Bool(quot),
                    Value::Bool(apos),
                    Value::Bool(slash),
                ]))),
            ))
        }),
    );
    bc.methods.insert(
        "convert".into(),
        h::method(|_v, t, args, _n| {
            h::require_args(&args, 1, "convert")?;
            let s = h::as_str(&args[0])
                .ok_or_else(|| h::err("HtmlEscape.convert requires a String argument."))?;
            let (ltgt, quot, apos, slash) = escape_flags(&t);
            Ok(h::str(escape_html(s, ltgt, quot, apos, slash)))
        }),
    );
    bc
}

// ---------------------------------------------------------------------------
// LineSplitter
// ---------------------------------------------------------------------------

fn split_lines(data: &str) -> Vec<Value> {
    let end = data.len();
    let mut lines = vec![];
    let mut slice_start = 0;
    let mut previous_char = '\0';
    for (i, c) in data.char_indices() {
        if c == '\n' && previous_char == '\r' {
            slice_start = i + 1;
            previous_char = c;
            continue;
        }
        if c != '\r' && c != '\n' {
            previous_char = c;
            continue;
        }
        lines.push(Value::str(&data[slice_start..i]));
        slice_start = i + 1;
        previous_char = c;
    }
    if slice_start < end {
        lines.push(Value::str(&data[slice_start..end]));
    }
    lines
}

fn line_splitter_definition() -> BridgedClassDef {
    let mut bc = BridgedClassDef::new("LineSplitter");
    bc.native_type = Some("d4rt::convert::LineSplitter".to_string());
    bc.constructors.insert(
        "".into(),
        h::ctor(|_v, _args, _n| Ok(wrap_cls("LineSplitter", Value::Null))),
    );
    bc.static_methods.insert(
        "split".into(),
        h::static_method(|_v, args, _n| {
            h::require_args(&args, 1, "split")?;
            let s = h::as_str(&args[0]).ok_or_else(|| h::err("split requires a String"))?;
            let start = args.get(1).and_then(h::as_int).unwrap_or(0).max(0) as usize;
            let end = args
                .get(2)
                .and_then(h::as_int)
                .map(|e| e as usize)
                .unwrap_or(s.len());
            let end = end.min(s.len());
            if start > end {
                return Err(h::err("Invalid start/end range"));
            }
            Ok(Value::List(Rc::new(RefCell::new(split_lines(
                &s[start..end],
            )))))
        }),
    );
    bc.methods.insert(
        "convert".into(),
        h::method(|_v, _t, args, _n| {
            h::require_args(&args, 1, "convert")?;
            let s = h::as_str(&args[0])
                .ok_or_else(|| h::err("LineSplitter.convert requires one String argument."))?;
            Ok(Value::List(Rc::new(RefCell::new(split_lines(s)))))
        }),
    );
    bc.methods.insert(
        "startChunkedConversion".into(),
        h::method(|_v, _t, args, _n| {
            h::require_args(&args, 1, "startChunkedConversion")?;
            let sink = args[0].clone();
            Ok(wrap_cls(
                "_LineSplitterSink",
                Value::List(Rc::new(RefCell::new(vec![sink, Value::str("")]))),
            ))
        }),
    );
    bc
}

fn line_splitter_sink_definition() -> BridgedClassDef {
    // Native: List([sink, carry]); on close, the remaining carry is emitted.
    let mut bc = BridgedClassDef::new("_LineSplitterSink");
    bc.native_type = Some("d4rt::convert::_LineSplitterSink".to_string());
    bc.methods.insert(
        "add".into(),
        h::method(|v, t, args, _n| {
            h::require_args(&args, 1, "add")?;
            let s = h::as_str(&args[0]).ok_or_else(|| h::err("add requires a String"))?;
            let cells = h::as_list(&t).ok_or_else(|| h::err("Not a _LineSplitterSink"))?;
            let (sink, full) = {
                let c = cells.borrow_mut();
                let carry = c[1].as_str().map(|x| x.to_string()).unwrap_or_default();
                (c[0].clone(), format!("{carry}{s}"))
            };
            let lines = split_lines(&full);
            let (completed, new_carry): (Vec<Value>, String) = if lines.is_empty() {
                (vec![], full)
            } else if full.ends_with('\n') || full.ends_with('\r') {
                (lines, String::new())
            } else {
                let mut completed = lines;
                let last = completed
                    .pop()
                    .map(|v| v.as_str().map(|x| x.to_string()).unwrap_or_default())
                    .unwrap_or_default();
                (completed, last)
            };
            {
                let mut c = cells.borrow_mut();
                c[1] = Value::str(new_carry);
            }
            for line in completed {
                invoke_method(v, &sink, "add", vec![line])?;
            }
            Ok(Value::Null)
        }),
    );
    bc.methods.insert(
        "close".into(),
        h::method(|v, t, _args, _n| {
            let cells = h::as_list(&t).ok_or_else(|| h::err("Not a _LineSplitterSink"))?;
            let sink = {
                let c = cells.borrow();
                c[0].clone()
            };
            let carry = {
                let mut c = cells.borrow_mut();
                std::mem::replace(&mut c[1], Value::Null)
            };
            if let Some(carry) = carry.as_str()
                && !carry.is_empty()
            {
                invoke_method(v, &sink, "add", vec![Value::str(carry)])?;
            }
            invoke_method(v, &sink, "close", vec![])?;
            Ok(Value::Null)
        }),
    );
    bc
}

// ---------------------------------------------------------------------------
// ChunkedConversionSink
// ---------------------------------------------------------------------------

fn chunked_conversion_sink_definition() -> BridgedClassDef {
    // Native: List([chunks, callback]); close() invokes callback(chunks).
    let mut bc = BridgedClassDef::new("ChunkedConversionSink");
    bc.native_type = Some("d4rt::convert::ChunkedConversionSink".to_string());
    bc.static_methods.insert(
        "withCallback".into(),
        h::static_method(|_v, args, _n| {
            h::require_args(&args, 1, "withCallback")?;
            Ok(wrap_cls(
                "ChunkedConversionSink",
                Value::List(Rc::new(RefCell::new(vec![
                    Value::List(Rc::new(RefCell::new(vec![]))),
                    args[0].clone(),
                ]))),
            ))
        }),
    );
    bc.methods.insert(
        "add".into(),
        h::method(|_v, t, args, _n| {
            h::require_args(&args, 1, "add")?;
            let cells = h::as_list(&t).ok_or_else(|| h::err("Not a ChunkedConversionSink"))?;
            let chunks = h::as_list(&cells.borrow()[0])
                .ok_or_else(|| h::err("Not a ChunkedConversionSink"))?;
            chunks.borrow_mut().push(args[0].clone());
            Ok(Value::Null)
        }),
    );
    bc.methods.insert(
        "close".into(),
        h::method(|v, t, _args, _n| {
            let cells = h::as_list(&t).ok_or_else(|| h::err("Not a ChunkedConversionSink"))?;
            let (chunks, callback) = {
                let c = cells.borrow();
                (c[0].clone(), c[1].clone())
            };
            v.call_value(&callback, vec![chunks], std::collections::HashMap::new())?;
            Ok(Value::Null)
        }),
    );
    bc
}

fn string_conversion_sink_close(
    v: &mut InterpreterVisitor,
    t: &Value,
) -> Result<Value, crate::InterpError> {
    let cells = h::as_list(t).ok_or_else(|| h::err("Not a StringConversionSink"))?;
    let kind = cells.borrow()[0].as_int().unwrap_or(1);
    if kind == 0 {
        let (callback, acc) = {
            let c = cells.borrow();
            (c[1].clone(), c[2].clone())
        };
        v.call_value(&callback, vec![acc], std::collections::HashMap::new())?;
    } else {
        let inner = cells.borrow()[1].clone();
        invoke_method(v, &inner, "close", vec![])?;
    }
    Ok(Value::Null)
}

fn string_conversion_sink_definition() -> BridgedClassDef {
    // Native: List([kind, inner, acc]) with kind 0 = withCallback
    // (inner = callback, acc = chunk list) and kind 1 = from (inner = sink).
    let mut bc = BridgedClassDef::new("StringConversionSink");
    bc.native_type = Some("d4rt::convert::StringConversionSink".to_string());
    bc.static_methods.insert(
        "withCallback".into(),
        h::static_method(|_v, args, _n| {
            h::require_args(&args, 1, "withCallback")?;
            Ok(wrap_cls(
                "StringConversionSink",
                Value::List(Rc::new(RefCell::new(vec![
                    Value::Int(0),
                    args[0].clone(),
                    Value::List(Rc::new(RefCell::new(vec![]))),
                ]))),
            ))
        }),
    );
    bc.static_methods.insert(
        "from".into(),
        h::static_method(|_v, args, _n| {
            h::require_args(&args, 1, "from")?;
            Ok(wrap_cls(
                "StringConversionSink",
                Value::List(Rc::new(RefCell::new(vec![
                    Value::Int(1),
                    args[0].clone(),
                    Value::Null,
                ]))),
            ))
        }),
    );
    bc.methods.insert(
        "add".into(),
        h::method(|v, t, args, _n| {
            h::require_args(&args, 1, "add")?;
            let s = h::as_str(&args[0]).ok_or_else(|| h::err("add requires a String argument."))?;
            let cells = h::as_list(&t).ok_or_else(|| h::err("Not a StringConversionSink"))?;
            let (kind, inner) = {
                let c = cells.borrow();
                (c[0].as_int().unwrap_or(1), c[1].clone())
            };
            if kind == 0 {
                let acc = h::as_list(&cells.borrow()[2]).ok_or_else(|| h::err("corrupt sink"))?;
                acc.borrow_mut().push(h::str(s));
                Ok(Value::Null)
            } else {
                invoke_method(v, &inner, "add", vec![h::str(s)])
            }
        }),
    );
    bc.methods.insert(
        "addSlice".into(),
        h::method(|v, t, args, _n| {
            h::require_args(&args, 4, "addSlice")?;
            let s = h::as_str(&args[0])
                .ok_or_else(|| h::err("addSlice requires a String argument."))?;
            let start = h::arg_int(&args, 1, "addSlice")?.max(0) as usize;
            let mut end = h::arg_int(&args, 2, "addSlice")?.max(0) as usize;
            end = end.min(s.len());
            let is_last = h::arg_bool(&args, 3, "addSlice")?;
            let cells = h::as_list(&t).ok_or_else(|| h::err("Not a StringConversionSink"))?;
            let (kind, inner) = {
                let c = cells.borrow();
                (c[0].as_int().unwrap_or(1), c[1].clone())
            };
            let sub = h::str(&s[start..end]);
            if kind == 0 {
                let acc = h::as_list(&cells.borrow()[2]).ok_or_else(|| h::err("corrupt sink"))?;
                acc.borrow_mut().push(sub);
            } else {
                invoke_method(v, &inner, "add", vec![sub])?;
            }
            if is_last {
                string_conversion_sink_close(v, &t)?;
            }
            Ok(Value::Null)
        }),
    );
    bc.methods.insert(
        "close".into(),
        h::method(|v, t, _args, _n| string_conversion_sink_close(v, &t)),
    );
    bc
}

// ---------------------------------------------------------------------------
// Converter (abstract base) and ByteConversionSink
// ---------------------------------------------------------------------------

/// Converter<S, T>: the native payload is the wrapped converter value itself,
/// so instance methods dispatch to the concrete converter's class.
fn converter_definition() -> BridgedClassDef {
    let mut bc = BridgedClassDef::new("Converter");
    bc.native_type = Some("d4rt::convert::Converter".to_string());
    bc.is_subtype_of_fn = Some(Box::new(|v| {
        matches!(v, Value::Bridged(b) if b
            .borrow()
            .bridged_class
            .native_type
            .as_deref()
            .is_some_and(|t| t.starts_with("d4rt::convert::")))
    }));
    bc.methods.insert(
        "convert".into(),
        h::method(|v, t, args, _n| {
            h::require_args(&args, 1, "convert")?;
            invoke_method(v, &t, "convert", vec![args[0].clone()])
        }),
    );
    bc.methods.insert(
        "startChunkedConversion".into(),
        h::method(|v, t, args, _n| {
            h::require_args(&args, 1, "startChunkedConversion")?;
            invoke_method(v, &t, "startChunkedConversion", vec![args[0].clone()])
        }),
    );
    bc.methods.insert(
        "bind".into(),
        h::method(|v, t, args, _n| {
            h::require_args(&args, 1, "bind")?;
            invoke_method(v, &t, "bind", vec![args[0].clone()])
        }),
    );
    bc.methods.insert(
        "fuse".into(),
        h::method(|v, t, args, _n| {
            h::require_args(&args, 1, "fuse")?;
            invoke_method(v, &t, "fuse", vec![args[0].clone()])
        }),
    );
    bc.methods.insert(
        "cast".into(),
        h::method(|_v, t, _args, _n| Ok(wrap_cls("Converter", t.clone()))),
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

/// ByteConversionSink: native `[kind, sink|callback, acc]` where kind 0 =
/// wrapping a Sink<List<int>> (from), kind 1 = accumulating callback sink
/// (withCallback); `acc` holds accumulated byte chunks as a List.
fn byte_conversion_sink_definition() -> BridgedClassDef {
    let mut bc = BridgedClassDef::new("ByteConversionSink");
    bc.native_type = Some("d4rt::convert::ByteConversionSink".to_string());
    bc.static_methods.insert(
        "from".into(),
        h::static_method(|_v, args, _n| {
            h::require_args(&args, 1, "from")?;
            Ok(wrap_cls(
                "ByteConversionSink",
                Value::List(Rc::new(RefCell::new(vec![
                    Value::Int(0),
                    args[0].clone(),
                    Value::List(Rc::new(RefCell::new(vec![]))),
                ]))),
            ))
        }),
    );
    bc.static_methods.insert(
        "withCallback".into(),
        h::static_method(|_v, args, _n| {
            h::require_args(&args, 1, "withCallback")?;
            Ok(wrap_cls(
                "ByteConversionSink",
                Value::List(Rc::new(RefCell::new(vec![
                    Value::Int(1),
                    args[0].clone(),
                    Value::List(Rc::new(RefCell::new(vec![]))),
                ]))),
            ))
        }),
    );
    bc.methods.insert(
        "add".into(),
        h::method(|v, t, args, _n| {
            h::require_args(&args, 1, "add")?;
            let cells = h::as_list(&t).ok_or_else(|| h::err("Not a ByteConversionSink"))?;
            let kind = cells.borrow()[0].as_int().unwrap_or(0);
            let bytes = h::as_list(&args[0]).ok_or_else(|| h::err("add expects a List<int>"))?;
            if kind == 0 {
                let sink = cells.borrow()[1].clone();
                invoke_method(v, &sink, "add", vec![args[0].clone()])?;
            } else {
                let acc = h::as_list(&cells.borrow()[2])
                    .ok_or_else(|| h::err("Corrupt ByteConversionSink"))?;
                acc.borrow_mut().extend(bytes.borrow().iter().cloned());
            }
            Ok(Value::Null)
        }),
    );
    bc.methods.insert(
        "addSlice".into(),
        h::method(|v, t, args, _n| {
            h::require_args(&args, 4, "addSlice")?;
            let cells = h::as_list(&t).ok_or_else(|| h::err("Not a ByteConversionSink"))?;
            let kind = cells.borrow()[0].as_int().unwrap_or(0);
            let src = h::as_list(&args[0]).ok_or_else(|| h::err("addSlice expects a List<int>"))?;
            let start = h::arg_int(&args, 1, "addSlice")?.max(0) as usize;
            let end = h::arg_int(&args, 2, "addSlice")?.max(0) as usize;
            let _is_last = h::arg_bool(&args, 3, "addSlice").unwrap_or(true);
            let srcb = src.borrow();
            let slice = srcb
                .iter()
                .skip(start)
                .take(end.saturating_sub(start))
                .cloned()
                .collect::<Vec<_>>();
            let slice_val = Value::List(Rc::new(RefCell::new(slice)));
            if kind == 0 {
                let sink = cells.borrow()[1].clone();
                invoke_method(v, &sink, "add", vec![slice_val])?;
            } else {
                let acc = h::as_list(&cells.borrow()[2])
                    .ok_or_else(|| h::err("Corrupt ByteConversionSink"))?;
                if let Some(a) = h::as_list(&slice_val) {
                    acc.borrow_mut().extend(a.borrow().iter().cloned());
                }
            }
            Ok(Value::Null)
        }),
    );
    bc.methods.insert(
        "close".into(),
        h::method(|v, t, _args, _n| {
            let cells = h::as_list(&t).ok_or_else(|| h::err("Not a ByteConversionSink"))?;
            let kind = cells.borrow()[0].as_int().unwrap_or(0);
            if kind == 0 {
                let sink = cells.borrow()[1].clone();
                invoke_method(v, &sink, "close", vec![])?;
            } else {
                let (callback, acc) = {
                    let c = cells.borrow();
                    (c[1].clone(), c[2].clone())
                };
                v.call_value(&callback, vec![acc], std::collections::HashMap::new())?;
            }
            Ok(Value::Null)
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
