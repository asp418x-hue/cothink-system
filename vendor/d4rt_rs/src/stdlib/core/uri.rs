use super::super::helpers as h;
use crate::class::BridgedClassDef;
use crate::error::InterpError;
use crate::value::{DartMap, Value};
use std::cell::RefCell;
use std::rc::Rc;

fn str(v: impl Into<String>) -> Value {
    Value::str(v)
}

fn list(items: Vec<Value>) -> Value {
    Value::List(Rc::new(RefCell::new(items)))
}

#[derive(Default, Clone)]
struct ParsedUri {
    scheme: String,
    user_info: String,
    host: String,
    port: i64,
    path: String,
    query: String,
    fragment: String,
}

fn is_scheme_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.')
}

fn parse_uri(s: &str) -> ParsedUri {
    let mut out = ParsedUri::default();
    let mut rest = s;
    if let Some(idx) = s.find(':') {
        let scheme = &s[..idx];
        if !scheme.is_empty()
            && scheme
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_alphabetic())
            && scheme.chars().all(is_scheme_char)
        {
            out.scheme = scheme.to_string();
            rest = &s[idx + 1..];
        }
    }
    if rest.starts_with("//") {
        let after = &rest[2..];
        let end = after.find(['/', '?', '#']).unwrap_or(after.len());
        let authority = &after[..end];
        rest = &after[end..];
        let (user_info, host_port) = match authority.rfind('@') {
            Some(i) => (&authority[..i], &authority[i + 1..]),
            None => ("", authority),
        };
        out.user_info = user_info.to_string();
        let (host, port) = if host_port.starts_with('[') {
            match host_port.find(']') {
                Some(i) => {
                    let host = &host_port[1..i];
                    let after = &host_port[i + 1..];
                    let port = after
                        .strip_prefix(':')
                        .and_then(|p| p.parse::<i64>().ok())
                        .unwrap_or(0);
                    (host.to_string(), port)
                }
                None => (host_port.to_string(), 0),
            }
        } else {
            match host_port.rfind(':') {
                Some(i)
                    if host_port[i + 1..].chars().all(|c| c.is_ascii_digit())
                        && !host_port[i + 1..].is_empty() =>
                {
                    let port = host_port[i + 1..].parse::<i64>().unwrap_or(0);
                    (host_port[..i].to_string(), port)
                }
                _ => (host_port.to_string(), 0),
            }
        };
        out.host = host;
        out.port = port;
    }
    let end = rest.find(['?', '#']).unwrap_or(rest.len());
    out.path = rest[..end].to_string();
    rest = &rest[end..];
    if let Some(idx) = rest.find('#') {
        out.query = if idx > 1 {
            rest[1..idx].to_string()
        } else {
            String::new()
        };
        out.fragment = rest[idx + 1..].to_string();
    } else {
        out.query = rest.strip_prefix('?').unwrap_or("").to_string();
    }
    out
}

fn is_unreserved(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '-' | '.' | '_' | '~')
}

fn is_reserved(c: char) -> bool {
    matches!(
        c,
        ':' | '/'
            | '?'
            | '#'
            | '['
            | ']'
            | '@'
            | '!'
            | '$'
            | '&'
            | '\''
            | '('
            | ')'
            | '*'
            | '+'
            | ','
            | ';'
            | '='
    )
}

fn encode_utf8(s: &str, allowed: impl Fn(char) -> bool, space_to_plus: bool) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        let c = b as char;
        if c.is_ascii() {
            if c == ' ' && space_to_plus {
                out.push('+');
            } else if allowed(c) {
                out.push(c);
            } else {
                out.push_str(&format!("%{b:02X}"));
            }
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

fn decode_percent(s: &str, plus_to_space: bool, strict: bool) -> Result<String, InterpError> {
    let bytes = s.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(s.len());
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'+' && plus_to_space {
            out.push(b' ');
            i += 1;
        } else if b == b'%' && i + 2 < bytes.len() {
            let hex = &s[i + 1..i + 3];
            match u8::from_str_radix(hex, 16) {
                Ok(v) => {
                    out.push(v);
                    i += 3;
                }
                Err(_) => {
                    if strict {
                        return Err(h::format_exception(format!(
                            "Invalid percent-encoding at offset {i}"
                        )));
                    }
                    out.push(b'%');
                    i += 1;
                }
            }
        } else {
            out.push(b);
            i += 1;
        }
    }
    let decoded = String::from_utf8(out);
    match decoded {
        Ok(s) => Ok(s),
        Err(e) => {
            if strict {
                Err(h::format_exception(
                    "Invalid UTF-8 in percent-decoded string".to_string(),
                ))
            } else {
                Ok(String::from_utf8_lossy(e.as_bytes()).into_owned())
            }
        }
    }
}

fn encode_component(s: &str) -> String {
    encode_utf8(s, is_unreserved, false)
}

fn encode_query_component(s: &str) -> String {
    encode_utf8(s, is_unreserved, true)
}

fn encode_full(s: &str) -> String {
    encode_utf8(s, |c| is_unreserved(c) || is_reserved(c), false)
}

fn decode_component(s: &str) -> Result<String, InterpError> {
    decode_percent(s, false, true)
}

fn decode_query_component(s: &str) -> Result<String, InterpError> {
    decode_percent(s, true, true)
}

fn decode_full(s: &str) -> String {
    decode_percent(s, false, false).unwrap_or_else(|_| s.to_string())
}

fn query_pairs(query: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for part in query.split('&') {
        if part.is_empty() {
            continue;
        }
        let (k, v) = match part.find('=') {
            Some(i) => (&part[..i], &part[i + 1..]),
            None => (part, ""),
        };
        let k = decode_query_component(k).unwrap_or_else(|_| k.to_string());
        let v = decode_query_component(v).unwrap_or_else(|_| v.to_string());
        out.push((k, v));
    }
    out
}

#[allow(clippy::too_many_arguments)]
fn build_uri(
    scheme: Option<&str>,
    user_info: Option<&str>,
    host: Option<&str>,
    port: Option<i64>,
    path: Option<&str>,
    path_segments: Option<&[String]>,
    query: Option<&str>,
    query_parameters: Option<&crate::value::DartMap>,
    fragment: Option<&str>,
) -> Result<String, InterpError> {
    let mut out = String::new();
    if let Some(s) = scheme {
        out.push_str(s);
        out.push(':');
    }
    if host.is_some() {
        out.push_str("//");
        if let Some(ui) = user_info
            && !ui.is_empty()
        {
            out.push_str(&encode_component(ui));
            out.push('@');
        }
        let host = host.unwrap_or_default();
        if host.contains(':') && !host.starts_with('[') {
            out.push('[');
            out.push_str(host);
            out.push(']');
        } else {
            out.push_str(host);
        }
        if let Some(p) = port {
            out.push(':');
            out.push_str(&p.to_string());
        }
    }
    if let Some(segments) = path_segments {
        let joined: Vec<String> = segments.iter().map(|s| encode_component(s)).collect();
        out.push_str(&joined.join("/"));
    } else if let Some(p) = path
        && !p.is_empty()
    {
        let encoded: Vec<String> = p.split('/').map(encode_component).collect();
        out.push_str(&encoded.join("/"));
    }
    if let Some(q) = query {
        if !q.is_empty() {
            out.push('?');
            out.push_str(&encode_query_component(q).replace('+', "%20"));
        }
    } else if let Some(params) = query_parameters {
        let mut parts = Vec::new();
        for (k, v) in &params.entries {
            let k = h::as_str(k).unwrap_or_default();
            let v = h::as_str(v).unwrap_or_default();
            parts.push(format!(
                "{}={}",
                encode_query_component(k),
                encode_query_component(v)
            ));
        }
        if !parts.is_empty() {
            out.push('?');
            out.push_str(&parts.join("&"));
        }
    }
    if let Some(f) = fragment
        && !f.is_empty()
    {
        out.push('#');
        out.push_str(&encode_component(f));
    }
    Ok(out)
}

fn normalize_path(path: &str) -> String {
    let mut segments: Vec<&str> = Vec::new();
    let leading_slash = path.starts_with('/');
    for seg in path.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                if !segments.is_empty() && segments.last() != Some(&"..") {
                    segments.pop();
                } else if !leading_slash {
                    segments.push("..");
                }
            }
            s => segments.push(s),
        }
    }
    let mut out = if leading_slash {
        String::from("/")
    } else {
        String::new()
    };
    out.push_str(&segments.join("/"));
    out
}

fn resolve_uri(base: &ParsedUri, relative: &str) -> String {
    let rel = parse_uri(relative);
    if !rel.scheme.is_empty() {
        return relative.to_string();
    }
    let mut out = String::new();
    if !base.scheme.is_empty() {
        out.push_str(&base.scheme);
        out.push(':');
    }
    if !rel.host.is_empty() {
        out.push_str("//");
        if !rel.user_info.is_empty() {
            out.push_str(&rel.user_info);
            out.push('@');
        }
        out.push_str(&rel.host);
        if rel.port != 0 {
            out.push(':');
            out.push_str(&rel.port.to_string());
        }
        let path = if rel.path.is_empty() {
            "/".to_string()
        } else {
            rel.path.clone()
        };
        out.push_str(&normalize_path(&path));
    } else if !base.host.is_empty() {
        out.push_str("//");
        if !base.user_info.is_empty() {
            out.push_str(&base.user_info);
            out.push('@');
        }
        out.push_str(&base.host);
        if base.port != 0 {
            out.push(':');
            out.push_str(&base.port.to_string());
        }
        let path = if rel.path.starts_with('/') {
            normalize_path(&rel.path)
        } else if rel.path.is_empty() {
            base.path.clone()
        } else {
            let merged = merge_paths(&base.path, &rel.path);
            normalize_path(&merged)
        };
        out.push_str(&path);
    } else {
        let path = if rel.path.starts_with('/') {
            normalize_path(&rel.path)
        } else if rel.path.is_empty() {
            base.path.clone()
        } else {
            normalize_path(&merge_paths(&base.path, &rel.path))
        };
        out.push_str(&path);
    }
    if !rel.query.is_empty() {
        out.push('?');
        out.push_str(&rel.query);
    }
    if !rel.fragment.is_empty() {
        out.push('#');
        out.push_str(&rel.fragment);
    }
    out
}

fn merge_paths(base: &str, relative: &str) -> String {
    match base.rfind('/') {
        Some(i) => format!("{}{}", &base[..i + 1], relative),
        None => relative.to_string(),
    }
}

fn is_valid_uri(s: &str) -> bool {
    !s.is_empty()
        && !s.chars().any(|c| {
            c.is_whitespace() || matches!(c, '<' | '>' | '"' | '{' | '}' | '|' | '\\' | '^' | '`')
        })
}

fn parse_ipv6_groups(s: &str) -> Option<Vec<u16>> {
    if s == "::" {
        return Some(vec![0; 8]);
    }
    if s.is_empty() || s.matches("::").count() > 1 {
        return None;
    }
    let (head, tail) = match s.find("::") {
        Some(i) => (Some(&s[..i]), Some(&s[i + 2..])),
        None => (Some(s), None),
    };
    let mut groups: Vec<u16> = Vec::new();
    if let Some(h) = head
        && !h.is_empty()
    {
        for g in h.split(':') {
            if g.is_empty() {
                return None;
            }
            groups.push(u16::from_str_radix(g, 16).ok()?);
        }
    }
    if let Some(t) = tail {
        let mut tail_groups = Vec::new();
        if !t.is_empty() {
            for g in t.split(':') {
                if g.is_empty() {
                    return None;
                }
                tail_groups.push(u16::from_str_radix(g, 16).ok()?);
            }
        }
        let total = groups.len() + tail_groups.len();
        if total >= 8 {
            return None;
        }
        groups.extend(std::iter::repeat_n(0, 8 - total));
        groups.extend(tail_groups);
    }
    (groups.len() == 8).then_some(groups)
}

const B64_ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn base64_encode(bytes: &[u8]) -> String {
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = chunk.get(1).copied().unwrap_or(0) as u32;
        let b2 = chunk.get(2).copied().unwrap_or(0) as u32;
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(B64_ALPHABET[(n >> 18) as usize & 63] as char);
        out.push(B64_ALPHABET[(n >> 12) as usize & 63] as char);
        if chunk.len() > 1 {
            out.push(B64_ALPHABET[(n >> 6) as usize & 63] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(B64_ALPHABET[n as usize & 63] as char);
        } else {
            out.push('=');
        }
    }
    out
}

pub fn uri_definition() -> BridgedClassDef {
    let mut bc = BridgedClassDef::new("Uri");
    bc.constructors.insert(
        "".into(),
        h::ctor(|_v, _args, named| {
            let scheme = named
                .get("scheme")
                .and_then(h::as_str)
                .map(|s| s.to_string());
            let user_info = named
                .get("userInfo")
                .and_then(h::as_str)
                .map(|s| s.to_string());
            let host = named.get("host").and_then(h::as_str).map(|s| s.to_string());
            let port = named.get("port").and_then(h::as_int);
            let path = named.get("path").and_then(h::as_str).map(|s| s.to_string());
            let path_segments = named.get("pathSegments").and_then(h::as_list).map(|l| {
                l.borrow()
                    .iter()
                    .filter_map(h::as_str)
                    .map(|s| s.to_string())
                    .collect::<Vec<_>>()
            });
            let query = named
                .get("query")
                .and_then(h::as_str)
                .map(|s| s.to_string());
            let query_parameters =
                named
                    .get("queryParameters")
                    .and_then(h::as_map)
                    .map(|m| DartMap {
                        entries: m.borrow().entries.clone(),
                    });
            let fragment = named
                .get("fragment")
                .and_then(h::as_str)
                .map(|s| s.to_string());
            let built = build_uri(
                scheme.as_deref(),
                user_info.as_deref(),
                host.as_deref(),
                port,
                path.as_deref(),
                path_segments.as_deref(),
                query.as_deref(),
                query_parameters.as_ref(),
                fragment.as_deref(),
            )?;
            Ok(h::wrap(&h::class("Uri"), str(built)))
        }),
    );
    bc.constructors.insert(
        "http".into(),
        h::ctor(|_v, args, _n| {
            let host = h::arg_str(&args, 0, "Uri.http")?;
            let path = args.get(1).and_then(h::as_str).unwrap_or_default();
            let query = args
                .get(2)
                .and_then(h::as_map)
                .map(|m| {
                    let m = m.borrow();
                    let mut parts = Vec::new();
                    for (k, v) in &m.entries {
                        parts.push(format!(
                            "{}={}",
                            encode_query_component(h::as_str(k).unwrap_or_default()),
                            encode_query_component(h::as_str(v).unwrap_or_default())
                        ));
                    }
                    parts.join("&")
                })
                .unwrap_or_default();
            let mut out = format!("http://{host}{path}");
            if !query.is_empty() {
                out.push('?');
                out.push_str(&query);
            }
            Ok(h::wrap(&h::class("Uri"), str(out)))
        }),
    );
    bc.constructors.insert(
        "https".into(),
        h::ctor(|_v, args, _n| {
            let host = h::arg_str(&args, 0, "Uri.https")?;
            let path = args.get(1).and_then(h::as_str).unwrap_or_default();
            let query = args
                .get(2)
                .and_then(h::as_map)
                .map(|m| {
                    let m = m.borrow();
                    let mut parts = Vec::new();
                    for (k, v) in &m.entries {
                        parts.push(format!(
                            "{}={}",
                            encode_query_component(h::as_str(k).unwrap_or_default()),
                            encode_query_component(h::as_str(v).unwrap_or_default())
                        ));
                    }
                    parts.join("&")
                })
                .unwrap_or_default();
            let mut out = format!("https://{host}{path}");
            if !query.is_empty() {
                out.push('?');
                out.push_str(&query);
            }
            Ok(h::wrap(&h::class("Uri"), str(out)))
        }),
    );
    bc.constructors.insert(
        "file".into(),
        h::ctor(|_v, args, _n| {
            let path = h::arg_str(&args, 0, "Uri.file")?;
            let out = if path.starts_with('/') {
                format!("file://{path}")
            } else {
                format!("file:///{path}")
            };
            Ok(h::wrap(&h::class("Uri"), str(out)))
        }),
    );
    bc.constructors.insert(
        "directory".into(),
        h::ctor(|_v, args, _n| {
            let path = h::arg_str(&args, 0, "Uri.directory")?;
            let mut out = if path.starts_with('/') {
                format!("file://{path}")
            } else {
                format!("file:///{path}")
            };
            if !out.ends_with('/') {
                out.push('/');
            }
            Ok(h::wrap(&h::class("Uri"), str(out)))
        }),
    );
    bc.constructors.insert(
        "dataFromBytes".into(),
        h::ctor(|_v, args, _n| {
            let bytes: Vec<u8> = h::as_list(&args.first().cloned().unwrap_or(Value::Null))
                .map(|l| {
                    l.borrow()
                        .iter()
                        .filter_map(h::as_int)
                        .map(|i| i as u8)
                        .collect()
                })
                .unwrap_or_default();
            let mime_type = args
                .get(1)
                .and_then(h::as_str)
                .unwrap_or("application/octet-stream");
            let parameters = args.get(2).and_then(h::as_map);
            let percent_encoded = args.get(3).and_then(h::as_bool).unwrap_or(false);
            let mut out = format!("data:{mime_type}");
            if let Some(p) = parameters {
                let p = p.borrow();
                let mut parts = Vec::new();
                for (k, v) in &p.entries {
                    parts.push(format!(
                        "{}={}",
                        encode_component(h::as_str(k).unwrap_or_default()),
                        encode_component(h::as_str(v).unwrap_or_default())
                    ));
                }
                out.push(';');
                out.push_str(&parts.join(";"));
            }
            out.push(',');
            if percent_encoded {
                for b in bytes {
                    out.push_str(&format!("%{b:02X}"));
                }
            } else {
                out.push_str(&base64_encode(&bytes));
            }
            Ok(h::wrap(&h::class("Uri"), str(out)))
        }),
    );
    bc.constructors.insert(
        "dataFromString".into(),
        h::ctor(|_v, args, named| {
            let s = h::arg_str(&args, 0, "dataFromString")?;
            let mime_type = named
                .get("mimeType")
                .and_then(h::as_str)
                .unwrap_or("text/plain");
            let parameters = named.get("parameters").and_then(h::as_map);
            let base64 = named.get("base64").and_then(h::as_bool).unwrap_or(false);
            let mut out = format!("data:{mime_type}");
            if let Some(p) = parameters {
                let p = p.borrow();
                let mut parts = Vec::new();
                for (k, v) in &p.entries {
                    parts.push(format!(
                        "{}={}",
                        encode_component(h::as_str(k).unwrap_or_default()),
                        encode_component(h::as_str(v).unwrap_or_default())
                    ));
                }
                out.push(';');
                out.push_str(&parts.join(";"));
            }
            out.push(',');
            if base64 {
                out.push_str(&base64_encode(s.as_bytes()));
            } else {
                for b in s.as_bytes() {
                    if *b == b' ' {
                        out.push('%');
                        out.push_str("20");
                    } else {
                        out.push(*b as char);
                    }
                }
            }
            Ok(h::wrap(&h::class("Uri"), str(out)))
        }),
    );
    bc.static_methods.insert(
        "parse".into(),
        h::static_method(|_v, args, _n| {
            let s = h::arg_str(&args, 0, "parse")?;
            if !is_valid_uri(s) {
                return Err(h::format_exception(format!("Invalid URI: {s}")));
            }
            Ok(h::wrap(&h::class("Uri"), str(s)))
        }),
    );
    bc.static_methods.insert(
        "tryParse".into(),
        h::static_method(|_v, args, _n| {
            let s = h::arg_str(&args, 0, "tryParse")?;
            if !is_valid_uri(s) {
                return Ok(Value::Null);
            }
            Ok(h::wrap(&h::class("Uri"), str(s)))
        }),
    );
    bc.static_methods.insert(
        "parseIPv4Address".into(),
        h::static_method(|_v, args, _n| {
            let s = h::arg_str(&args, 0, "parseIPv4Address")?;
            let parts: Vec<&str> = s.split('.').collect();
            if parts.len() != 4 {
                return Err(h::format_exception(format!("Invalid IPv4 address: {s}")));
            }
            let mut octets = Vec::new();
            for p in parts {
                match p.parse::<u8>() {
                    Ok(o) => octets.push(Value::Int(o as i64)),
                    Err(_) => {
                        return Err(h::format_exception(format!("Invalid IPv4 address: {s}")));
                    }
                }
            }
            Ok(list(octets))
        }),
    );
    bc.static_methods.insert(
        "parseIPv6Address".into(),
        h::static_method(|_v, args, _n| {
            let s = h::arg_str(&args, 0, "parseIPv6Address")?;
            let groups = parse_ipv6_groups(s)
                .ok_or_else(|| h::format_exception(format!("Invalid IPv6 address: {s}")))?;
            let mut bytes = Vec::new();
            for g in groups {
                bytes.push(Value::Int((g >> 8) as i64));
                bytes.push(Value::Int((g & 0xff) as i64));
            }
            Ok(list(bytes))
        }),
    );
    bc.static_methods.insert(
        "encodeComponent".into(),
        h::static_method(|_v, args, _n| {
            let s = h::arg_str(&args, 0, "encodeComponent")?;
            Ok(str(encode_component(s)))
        }),
    );
    bc.static_methods.insert(
        "encodeQueryComponent".into(),
        h::static_method(|_v, args, _n| {
            let s = h::arg_str(&args, 0, "encodeQueryComponent")?;
            Ok(str(encode_query_component(s)))
        }),
    );
    bc.static_methods.insert(
        "decodeComponent".into(),
        h::static_method(|_v, args, _n| {
            let s = h::arg_str(&args, 0, "decodeComponent")?;
            Ok(str(decode_component(s)?))
        }),
    );
    bc.static_methods.insert(
        "decodeQueryComponent".into(),
        h::static_method(|_v, args, _n| {
            let s = h::arg_str(&args, 0, "decodeQueryComponent")?;
            Ok(str(decode_query_component(s)?))
        }),
    );
    bc.static_methods.insert(
        "encodeFull".into(),
        h::static_method(|_v, args, _n| {
            let s = h::arg_str(&args, 0, "encodeFull")?;
            Ok(str(encode_full(s)))
        }),
    );
    bc.static_methods.insert(
        "decodeFull".into(),
        h::static_method(|_v, args, _n| {
            let s = h::arg_str(&args, 0, "decodeFull")?;
            Ok(str(decode_full(s)))
        }),
    );
    bc.static_methods.insert(
        "splitQueryString".into(),
        h::static_method(|_v, args, _n| {
            let s = h::arg_str(&args, 0, "splitQueryString")?;
            let mut entries = Vec::new();
            for (k, v) in query_pairs(s) {
                entries.push((Value::str(k), Value::str(v)));
            }
            Ok(Value::Map(Rc::new(RefCell::new(crate::value::DartMap {
                entries,
            }))))
        }),
    );
    bc.static_methods.insert(
        "normalizePath".into(),
        h::static_method(|_v, args, _n| {
            let path = h::arg_str(&args, 0, "normalizePath")?;
            Ok(str(normalize_path(path)))
        }),
    );
    bc.methods.insert(
        "replace".into(),
        h::method(|_v, t, _a, named| {
            let p = parse_uri(h::as_str(&t).unwrap_or_default());
            let scheme = named
                .get("scheme")
                .and_then(h::as_str)
                .map(|s| s.to_string());
            let user_info = named
                .get("userInfo")
                .and_then(h::as_str)
                .map(|s| s.to_string());
            let host = named.get("host").and_then(h::as_str).map(|s| s.to_string());
            let port = named.get("port").and_then(h::as_int);
            let path = named.get("path").and_then(h::as_str).map(|s| s.to_string());
            let path_segments = named.get("pathSegments").and_then(h::as_list).map(|l| {
                l.borrow()
                    .iter()
                    .filter_map(h::as_str)
                    .map(|s| s.to_string())
                    .collect::<Vec<_>>()
            });
            let query = named
                .get("query")
                .and_then(h::as_str)
                .map(|s| s.to_string());
            let query_parameters =
                named
                    .get("queryParameters")
                    .and_then(h::as_map)
                    .map(|m| DartMap {
                        entries: m.borrow().entries.clone(),
                    });
            let fragment = named
                .get("fragment")
                .and_then(h::as_str)
                .map(|s| s.to_string());
            let built = build_uri(
                scheme.as_deref().or(Some(&p.scheme)),
                user_info.as_deref().or(Some(&p.user_info)),
                host.as_deref().or(Some(&p.host)),
                port.or((p.port != 0).then_some(p.port)),
                path.as_deref().or(Some(&p.path)),
                path_segments.as_deref(),
                query
                    .as_deref()
                    .or((!p.query.is_empty()).then_some(p.query.as_str())),
                query_parameters.as_ref(),
                fragment.as_deref().or(Some(&p.fragment)),
            )?;
            Ok(h::wrap(&h::class("Uri"), str(built)))
        }),
    );
    bc.methods.insert(
        "removeFragment".into(),
        h::method(|_v, t, _a, _n| {
            let s = h::as_str(&t).unwrap_or_default();
            let s = match s.find('#') {
                Some(i) => &s[..i],
                None => s,
            };
            Ok(h::wrap(&h::class("Uri"), str(s)))
        }),
    );
    bc.methods.insert(
        "resolve".into(),
        h::method(|_v, t, args, _n| {
            let base = parse_uri(h::as_str(&t).unwrap_or_default());
            let relative = h::arg_str(&args, 0, "resolve")?;
            Ok(h::wrap(&h::class("Uri"), str(resolve_uri(&base, relative))))
        }),
    );
    bc.methods.insert(
        "resolveUri".into(),
        h::method(|_v, t, args, _n| {
            let base = parse_uri(h::as_str(&t).unwrap_or_default());
            let first = args.first().cloned().unwrap_or(Value::Null);
            let relative = h::as_str(&first).unwrap_or_default();
            Ok(h::wrap(&h::class("Uri"), str(resolve_uri(&base, relative))))
        }),
    );
    bc.methods.insert(
        "toFilePath".into(),
        h::method(|_v, t, _a, _n| {
            let p = parse_uri(h::as_str(&t).unwrap_or_default());
            let mut path = p.path.clone();
            path = decode_component(&path).unwrap_or(path);
            Ok(str(path))
        }),
    );
    bc.methods.insert(
        "toString".into(),
        h::method(|_v, t, _a, _n| Ok(str(h::as_str(&t).unwrap_or_default()))),
    );
    bc.methods.insert(
        "normalizePath".into(),
        h::method(|_v, t, _a, _n| {
            let p = parse_uri(h::as_str(&t).unwrap_or_default());
            let mut out = String::new();
            if !p.scheme.is_empty() {
                out.push_str(&p.scheme);
                out.push(':');
            }
            if !p.host.is_empty() {
                out.push_str("//");
                out.push_str(&p.host);
            }
            out.push_str(&normalize_path(&p.path));
            if !p.query.is_empty() {
                out.push('?');
                out.push_str(&p.query);
            }
            if !p.fragment.is_empty() {
                out.push('#');
                out.push_str(&p.fragment);
            }
            Ok(h::wrap(&h::class("Uri"), str(out)))
        }),
    );
    bc.methods.insert(
        "isScheme".into(),
        h::method(|_v, t, args, _n| {
            let p = parse_uri(h::as_str(&t).unwrap_or_default());
            let name = h::arg_str(&args, 0, "isScheme")?;
            Ok(Value::Bool(p.scheme == name))
        }),
    );
    bc.getters.insert(
        "scheme".into(),
        h::getter(|_v, t| Ok(str(parse_uri(h::as_str(&t).unwrap_or_default()).scheme))),
    );
    bc.getters.insert(
        "userInfo".into(),
        h::getter(|_v, t| Ok(str(parse_uri(h::as_str(&t).unwrap_or_default()).user_info))),
    );
    bc.getters.insert(
        "host".into(),
        h::getter(|_v, t| Ok(str(parse_uri(h::as_str(&t).unwrap_or_default()).host))),
    );
    bc.getters.insert(
        "port".into(),
        h::getter(|_v, t| {
            Ok(Value::Int(
                parse_uri(h::as_str(&t).unwrap_or_default()).port,
            ))
        }),
    );
    bc.getters.insert(
        "path".into(),
        h::getter(|_v, t| Ok(str(parse_uri(h::as_str(&t).unwrap_or_default()).path))),
    );
    bc.getters.insert(
        "query".into(),
        h::getter(|_v, t| Ok(str(parse_uri(h::as_str(&t).unwrap_or_default()).query))),
    );
    bc.getters.insert(
        "fragment".into(),
        h::getter(|_v, t| Ok(str(parse_uri(h::as_str(&t).unwrap_or_default()).fragment))),
    );
    bc.getters.insert(
        "authority".into(),
        h::getter(|_v, t| {
            let p = parse_uri(h::as_str(&t).unwrap_or_default());
            let mut out = String::new();
            if !p.user_info.is_empty() {
                out.push_str(&p.user_info);
                out.push('@');
            }
            out.push_str(&p.host);
            if p.port != 0 {
                out.push(':');
                out.push_str(&p.port.to_string());
            }
            Ok(str(out))
        }),
    );
    bc.getters.insert(
        "pathSegments".into(),
        h::getter(|_v, t| {
            let p = parse_uri(h::as_str(&t).unwrap_or_default());
            let segments: Vec<Value> = p
                .path
                .split('/')
                .filter(|s| !s.is_empty())
                .map(|s| str(decode_component(s).unwrap_or_else(|_| s.to_string())))
                .collect();
            Ok(list(segments))
        }),
    );
    bc.getters.insert(
        "queryParameters".into(),
        h::getter(|_v, t| {
            let p = parse_uri(h::as_str(&t).unwrap_or_default());
            let mut entries = Vec::new();
            for (k, v) in query_pairs(&p.query) {
                entries.push((Value::str(k), Value::str(v)));
            }
            Ok(Value::Map(Rc::new(RefCell::new(crate::value::DartMap {
                entries,
            }))))
        }),
    );
    bc.getters.insert(
        "queryParametersAll".into(),
        h::getter(|_v, t| {
            let p = parse_uri(h::as_str(&t).unwrap_or_default());
            let mut entries: Vec<(Value, Value)> = Vec::new();
            for (k, v) in query_pairs(&p.query) {
                match entries
                    .iter_mut()
                    .find(|(ek, _)| h::as_str(ek) == Some(k.as_str()))
                {
                    Some((_, existing)) => {
                        if let Value::List(l) = existing {
                            l.borrow_mut().push(Value::str(v));
                        }
                    }
                    None => {
                        entries.push((Value::str(k), list(vec![Value::str(v)])));
                    }
                }
            }
            Ok(Value::Map(Rc::new(RefCell::new(crate::value::DartMap {
                entries,
            }))))
        }),
    );
    bc.getters.insert(
        "isAbsolute".into(),
        h::getter(|_v, t| {
            Ok(Value::Bool(
                !parse_uri(h::as_str(&t).unwrap_or_default())
                    .scheme
                    .is_empty(),
            ))
        }),
    );
    bc.getters.insert(
        "hasScheme".into(),
        h::getter(|_v, t| {
            Ok(Value::Bool(
                !parse_uri(h::as_str(&t).unwrap_or_default())
                    .scheme
                    .is_empty(),
            ))
        }),
    );
    bc.getters.insert(
        "hasAuthority".into(),
        h::getter(|_v, t| {
            let p = parse_uri(h::as_str(&t).unwrap_or_default());
            Ok(Value::Bool(!p.host.is_empty()))
        }),
    );
    bc.getters.insert(
        "hasPort".into(),
        h::getter(|_v, t| {
            Ok(Value::Bool(
                parse_uri(h::as_str(&t).unwrap_or_default()).port != 0,
            ))
        }),
    );
    bc.getters.insert(
        "hasQuery".into(),
        h::getter(|_v, t| {
            Ok(Value::Bool(
                !parse_uri(h::as_str(&t).unwrap_or_default())
                    .query
                    .is_empty(),
            ))
        }),
    );
    bc.getters.insert(
        "hasFragment".into(),
        h::getter(|_v, t| {
            Ok(Value::Bool(
                !parse_uri(h::as_str(&t).unwrap_or_default())
                    .fragment
                    .is_empty(),
            ))
        }),
    );
    bc.getters.insert(
        "hasEmptyPath".into(),
        h::getter(|_v, t| {
            Ok(Value::Bool(
                parse_uri(h::as_str(&t).unwrap_or_default()).path.is_empty(),
            ))
        }),
    );
    bc.getters.insert(
        "hasAbsolutePath".into(),
        h::getter(|_v, t| {
            Ok(Value::Bool(
                parse_uri(h::as_str(&t).unwrap_or_default())
                    .path
                    .starts_with('/'),
            ))
        }),
    );
    bc.getters.insert(
        "origin".into(),
        h::getter(|_v, t| {
            let p = parse_uri(h::as_str(&t).unwrap_or_default());
            if p.scheme != "http" && p.scheme != "https" {
                return Ok(str(""));
            }
            let mut out = format!("{}://{}", p.scheme, p.host);
            if p.port != 0 {
                out.push(':');
                out.push_str(&p.port.to_string());
            }
            Ok(str(out))
        }),
    );
    bc.getters.insert(
        "directory".into(),
        h::getter(|_v, t| {
            let p = parse_uri(h::as_str(&t).unwrap_or_default());
            let path = match p.path.rfind('/') {
                Some(0) => "/".to_string(),
                Some(i) => p.path[..i].to_string(),
                None => String::new(),
            };
            let mut out = String::new();
            if !p.scheme.is_empty() {
                out.push_str(&p.scheme);
                out.push(':');
            }
            if !p.host.is_empty() {
                out.push_str("//");
                out.push_str(&p.host);
                if p.port != 0 {
                    out.push(':');
                    out.push_str(&p.port.to_string());
                }
            }
            out.push_str(&path);
            Ok(str(out))
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
