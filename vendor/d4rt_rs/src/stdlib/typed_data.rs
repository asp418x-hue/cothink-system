//! dart:typed_data: ByteData, ByteBuffer, Uint8List and Endian.
//!
//! Storage is a shared `Rc<RefCell<Vec<Value>>>` of byte values:
//! - `ByteBuffer` native: the byte list itself
//! - `Uint8List` native: `[bytes, offset, length]` wrapper list (views
//!   share the underlying bytes)
//! - `ByteData` native: `[bytes, offset]` wrapper list

use super::helpers as h;
use crate::InterpError;
use crate::class::BridgedClassDef;
use crate::env::Environment;
use crate::value::Value;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

fn define_bridge(env: &Rc<RefCell<Environment>>, bc: BridgedClassDef) {
    let bc = Arc::new(bc);
    h::register_class(bc.clone());
    Environment::define_bridge(env, bc);
}

pub fn load(env: &Rc<RefCell<Environment>>) {
    define_bridge(env, endian_definition());
    define_bridge(env, byte_buffer_definition());
    define_bridge(env, byte_data_definition());
    define_bridge(env, uint8_list_definition());
    define_bridge(env, int16_list_definition());
    define_bridge(env, float32_list_definition());
}

fn wrap_cls(name: &str, native: Value) -> Value {
    h::wrap(&h::class(name), native)
}

fn bytes_cell(len: usize) -> Rc<RefCell<Vec<Value>>> {
    Rc::new(RefCell::new(vec![Value::Int(0); len]))
}

fn wrap_bytes(bytes: Rc<RefCell<Vec<Value>>>) -> Value {
    Value::List(bytes)
}

fn unwrap_bytes(v: &Value) -> Result<Rc<RefCell<Vec<Value>>>, InterpError> {
    h::as_list(v).ok_or_else(|| h::err("Not a byte buffer"))
}

/// Extracts (bytes, offset) from a Uint8List/ByteData native wrapper.
type ByteBuffer = Rc<RefCell<Vec<Value>>>;
type View = (ByteBuffer, usize);
type U8View = (ByteBuffer, usize, usize);

fn unwrap_view(t: &Value) -> Result<View, InterpError> {
    let w = h::as_list(t).ok_or_else(|| h::err("Not a typed data view"))?;
    let w = w.borrow();
    if w.len() < 2 {
        return Err(h::err("Corrupt typed data view"));
    }
    let bytes = unwrap_bytes(&w[0])?;
    let offset = h::as_int(&w[1]).ok_or_else(|| h::err("Corrupt typed data offset"))?;
    Ok((bytes, offset as usize))
}

/// Extracts (bytes, offset, length) from a Uint8List native wrapper.
fn unwrap_u8_view(t: &Value) -> Result<U8View, InterpError> {
    let (bytes, offset) = unwrap_view(t)?;
    let w = h::as_list(t).ok_or_else(|| h::err("Not a typed data view"))?;
    let w = w.borrow();
    let len = h::as_int(&w[2]).ok_or_else(|| h::err("Corrupt typed data length"))?;
    Ok((bytes, offset, len as usize))
}

fn u8_wrapper(bytes: Rc<RefCell<Vec<Value>>>, offset: usize, len: usize) -> Value {
    Value::List(Rc::new(RefCell::new(vec![
        wrap_bytes(bytes),
        Value::Int(offset as i64),
        Value::Int(len as i64),
    ])))
}

fn byte_data_wrapper(bytes: Rc<RefCell<Vec<Value>>>, offset: usize, len: usize) -> Value {
    Value::List(Rc::new(RefCell::new(vec![
        wrap_bytes(bytes),
        Value::Int(offset as i64),
        Value::Int(len as i64),
    ])))
}

fn byte_at(bytes: &Rc<RefCell<Vec<Value>>>, offset: usize, i: usize) -> Result<Value, InterpError> {
    let bytes = bytes.borrow();
    bytes
        .get(offset + i)
        .cloned()
        .ok_or_else(|| h::err("Index out of range"))
}

fn set_byte(
    bytes: &Rc<RefCell<Vec<Value>>>,
    offset: usize,
    i: usize,
    b: i64,
) -> Result<(), InterpError> {
    if !(0..=255).contains(&b) {
        return Err(h::err("Byte value out of range"));
    }
    let mut bytes = bytes.borrow_mut();
    let target = bytes
        .get_mut(offset + i)
        .ok_or_else(|| h::err("Index out of range"))?;
    *target = Value::Int(b);
    Ok(())
}

fn is_little(v: &Value) -> bool {
    match v {
        Value::Str(s) => &**s == "little",
        _ => false,
    }
}

// ---------------------------------------------------------------------------
// Endian
// ---------------------------------------------------------------------------

fn endian_definition() -> BridgedClassDef {
    let mut bc = BridgedClassDef::new("Endian");
    bc.native_type = Some("d4rt::typed_data::Endian".to_string());
    bc.static_getters
        .insert("big".into(), h::static_getter(|_v| Ok(Value::str("big"))));
    bc.static_getters.insert(
        "little".into(),
        h::static_getter(|_v| Ok(Value::str("little"))),
    );
    let host = if cfg!(target_endian = "big") {
        "big"
    } else {
        "little"
    };
    bc.static_getters.insert(
        "host".into(),
        h::static_getter(move |_v| Ok(Value::str(host))),
    );
    bc
}

// ---------------------------------------------------------------------------
// ByteBuffer
// ---------------------------------------------------------------------------

fn byte_buffer_definition() -> BridgedClassDef {
    let mut bc = BridgedClassDef::new("ByteBuffer");
    bc.native_type = Some("d4rt::typed_data::ByteBuffer".to_string());
    bc.getters.insert(
        "lengthInBytes".into(),
        h::getter(|_v, t| {
            let bytes = unwrap_bytes(&t)?;
            Ok(Value::Int(bytes.borrow().len() as i64))
        }),
    );
    bc.methods.insert(
        "asUint8List".into(),
        h::method(|_v, t, args, _n| {
            let bytes = unwrap_bytes(&t)?;
            let offset = match args.first() {
                Some(a) => h::as_int(a).ok_or_else(|| h::err("offset must be an int"))? as usize,
                None => 0,
            };
            let len = match args.get(1) {
                Some(a) => h::as_int(a).ok_or_else(|| h::err("length must be an int"))? as usize,
                None => bytes.borrow().len().saturating_sub(offset),
            };
            if offset + len > bytes.borrow().len() {
                return Err(h::err("View out of range"));
            }
            Ok(wrap_cls("Uint8List", u8_wrapper(bytes, offset, len)))
        }),
    );
    bc
}

// ---------------------------------------------------------------------------
// ByteData
// ---------------------------------------------------------------------------

fn byte_data_definition() -> BridgedClassDef {
    let mut bc = BridgedClassDef::new("ByteData");
    bc.native_type = Some("d4rt::typed_data::ByteData".to_string());
    {
        bc.constructors.insert(
            "".into(),
            h::ctor(|_v, args, _n| {
                let len = h::arg_int(&args, 0, "ByteData")?;
                if len < 0 {
                    return Err(h::err("Length must be non-negative"));
                }
                let len = len as usize;
                Ok(wrap_cls(
                    "ByteData",
                    byte_data_wrapper(bytes_cell(len), 0, len),
                ))
            }),
        );
    }
    bc.static_methods.insert(
        "view".into(),
        h::static_method(|_v, args, _n| {
            h::require_args(&args, 1, "ByteData.view")?;
            let bytes = unwrap_bytes(&args[0])?;
            let total = bytes.borrow().len();
            let offset = args.get(1).and_then(h::as_int).unwrap_or(0).max(0) as usize;
            let len = match args.get(2).and_then(h::as_int) {
                Some(l) => l.max(0) as usize,
                None => total.saturating_sub(offset),
            };
            if offset + len > total {
                return Err(h::err("View out of range"));
            }
            Ok(wrap_cls("ByteData", byte_data_wrapper(bytes, offset, len)))
        }),
    );
    bc.static_methods.insert(
        "sublistView".into(),
        h::static_method(|_v, args, _n| {
            h::require_args(&args, 1, "ByteData.sublistView")?;
            let (bytes, offset, len) = unwrap_u8_view(&args[0])?;
            let start = args.get(1).and_then(h::as_int).unwrap_or(0).max(0) as usize;
            let end = match args.get(2).and_then(h::as_int) {
                Some(e) => e as usize,
                None => len,
            };
            if start > end || end > len {
                return Err(h::err("Range out of bounds"));
            }
            Ok(wrap_cls(
                "ByteData",
                byte_data_wrapper(bytes, offset + start, end - start),
            ))
        }),
    );
    bc.getters.insert(
        "lengthInBytes".into(),
        h::getter(|_v, t| {
            let (_, _, len) = unwrap_u8_view(&t)?;
            Ok(Value::Int(len as i64))
        }),
    );
    bc.getters.insert(
        "elementSizeInBytes".into(),
        h::getter(|_v, _t| Ok(Value::Int(1))),
    );
    bc.getters.insert(
        "buffer".into(),
        h::getter(|_v, t| {
            let (bytes, _offset) = unwrap_view(&t)?;
            Ok(wrap_cls("ByteBuffer", wrap_bytes(bytes)))
        }),
    );
    // Integer accessors: (get|set)(Int8|Uint8|Int16|Uint16|Int32|Uint32|Int64|Uint64)
    for (name, width, signed) in [
        ("Int8", 1, true),
        ("Uint8", 1, false),
        ("Int16", 2, true),
        ("Uint16", 2, false),
        ("Int32", 4, true),
        ("Uint32", 4, false),
        ("Int64", 8, true),
        ("Uint64", 8, false),
    ] {
        let get_name = format!("get{name}");
        let set_name = format!("set{name}");
        let w = width;
        let sg = signed;
        let gn = get_name.clone();
        bc.methods.insert(
            get_name,
            h::method(move |_v, t, args, _n| {
                h::require_args(&args, 1, &gn)?;
                let (bytes, offset) = unwrap_view(&t)?;
                let i = h::arg_int(&args, 0, &gn)? as usize;
                let little = args.get(1).map(is_little).unwrap_or(false);
                let mut value: u64 = 0;
                for k in 0..w {
                    let b = byte_at(&bytes, offset, i + k)?;
                    let b = h::as_int(&b).ok_or_else(|| h::err("Bad byte"))? as u64;
                    let shift = if little { k } else { w - 1 - k };
                    value |= b << (shift * 8);
                }
                if sg && w < 8 {
                    let bits = w * 8;
                    let sign_bit = 1u64 << (bits - 1);
                    if value & sign_bit != 0 {
                        value |= !((1u64 << bits) - 1);
                    }
                    Ok(Value::Int(value as i64))
                } else {
                    Ok(Value::Int(value as i64))
                }
            }),
        );
        let w = width;
        let sg = signed;
        let sn = set_name.clone();
        bc.methods.insert(
            set_name,
            h::method(move |_v, t, args, _n| {
                h::require_args(&args, 2, &sn)?;
                let (bytes, offset) = unwrap_view(&t)?;
                let i = h::arg_int(&args, 0, &sn)? as usize;
                let mut value = h::arg_int(&args, 1, &sn)? as u64;
                let little = args.get(2).map(is_little).unwrap_or(false);
                if !sg {
                    value &= (1u64 << (w * 8)) - 1;
                }
                for k in 0..w {
                    let shift = if little { k } else { w - 1 - k };
                    set_byte(
                        &bytes,
                        offset,
                        i + k,
                        ((value >> (shift * 8)) & 0xFF) as i64,
                    )?;
                }
                Ok(Value::Null)
            }),
        );
    }
    // Float accessors
    for (name, width) in [("Float32", 4), ("Float64", 8)] {
        let get_name = format!("get{name}");
        let set_name = format!("set{name}");
        let w = width;
        let gn = get_name.clone();
        bc.methods.insert(
            get_name,
            h::method(move |_v, t, args, _n| {
                h::require_args(&args, 1, &gn)?;
                let (bytes, offset) = unwrap_view(&t)?;
                let i = h::arg_int(&args, 0, &gn)? as usize;
                let little = args.get(1).map(is_little).unwrap_or(false);
                let mut value: u64 = 0;
                for k in 0..w {
                    let b = byte_at(&bytes, offset, i + k)?;
                    let b = h::as_int(&b).ok_or_else(|| h::err("Bad byte"))? as u64;
                    let shift = if little { k } else { w - 1 - k };
                    value |= b << (shift * 8);
                }
                if w == 4 {
                    Ok(Value::Double(f32::from_bits(value as u32) as f64))
                } else {
                    Ok(Value::Double(f64::from_bits(value)))
                }
            }),
        );
        let w = width;
        let sn = set_name.clone();
        bc.methods.insert(
            set_name,
            h::method(move |_v, t, args, _n| {
                h::require_args(&args, 2, &sn)?;
                let (bytes, offset) = unwrap_view(&t)?;
                let i = h::arg_int(&args, 0, &sn)? as usize;
                let f = h::arg_double(&args, 1, &sn)?;
                let little = args.get(2).map(is_little).unwrap_or(false);
                let value: u64 = if w == 4 {
                    (f as f32).to_bits() as u64
                } else {
                    f.to_bits()
                };
                for k in 0..w {
                    let shift = if little { k } else { w - 1 - k };
                    set_byte(
                        &bytes,
                        offset,
                        i + k,
                        ((value >> (shift * 8)) & 0xFF) as i64,
                    )?;
                }
                Ok(Value::Null)
            }),
        );
    }
    bc
}

// ---------------------------------------------------------------------------
// Uint8List
// ---------------------------------------------------------------------------

fn uint8_list_definition() -> BridgedClassDef {
    let mut bc = BridgedClassDef::new("Uint8List");
    bc.native_type = Some("d4rt::typed_data::Uint8List".to_string());
    {
        bc.constructors.insert(
            "".into(),
            h::ctor(|_v, args, _n| {
                let len = h::arg_int(&args, 0, "Uint8List")?;
                if len < 0 {
                    return Err(h::err("Length must be non-negative"));
                }
                Ok(wrap_cls(
                    "Uint8List",
                    u8_wrapper(bytes_cell(len as usize), 0, len as usize),
                ))
            }),
        );
    }
    bc.static_methods.insert(
        "fromList".into(),
        h::static_method(|_v, args, _n| {
            let src = h::as_list(&args[0]).ok_or_else(|| h::err("fromList expects a List<int>"))?;
            let bytes = Rc::new(RefCell::new(Vec::with_capacity(src.borrow().len())));
            for item in src.borrow().iter() {
                let b = h::as_int(item).ok_or_else(|| h::err("fromList expects a List<int>"))?;
                if !(0..=255).contains(&b) {
                    return Err(h::err("Byte value out of range"));
                }
                bytes.borrow_mut().push(Value::Int(b));
            }
            let len = bytes.borrow().len();
            Ok(wrap_cls("Uint8List", u8_wrapper(bytes, 0, len)))
        }),
    );
    bc.static_methods.insert(
        "view".into(),
        h::static_method(|_v, args, _n| {
            h::require_args(&args, 1, "Uint8List.view")?;
            let bytes = unwrap_bytes(&args[0])?;
            let total = bytes.borrow().len();
            let offset = args.get(1).and_then(h::as_int).unwrap_or(0).max(0) as usize;
            let len = match args.get(2).and_then(h::as_int) {
                Some(l) => l.max(0) as usize,
                None => total.saturating_sub(offset),
            };
            if offset + len > total {
                return Err(h::err("View out of range"));
            }
            Ok(wrap_cls("Uint8List", u8_wrapper(bytes, offset, len)))
        }),
    );
    bc.static_methods.insert(
        "sublistView".into(),
        h::static_method(|_v, args, _n| {
            h::require_args(&args, 1, "Uint8List.sublistView")?;
            let (bytes, offset, len) = unwrap_u8_view(&args[0])?;
            let start = args.get(1).and_then(h::as_int).unwrap_or(0).max(0) as usize;
            let end = match args.get(2).and_then(h::as_int) {
                Some(e) => e as usize,
                None => len,
            };
            if start > end || end > len {
                return Err(h::err("Range out of bounds"));
            }
            Ok(wrap_cls(
                "Uint8List",
                u8_wrapper(bytes, offset + start, end - start),
            ))
        }),
    );
    bc.getters.insert(
        "length".into(),
        h::getter(|_v, t| {
            let (_, _, len) = unwrap_u8_view(&t)?;
            Ok(Value::Int(len as i64))
        }),
    );
    bc.getters.insert(
        "elementSizeInBytes".into(),
        h::getter(|_v, _t| Ok(Value::Int(1))),
    );
    bc.getters.insert(
        "buffer".into(),
        h::getter(|_v, t| {
            let (bytes, _offset, _len) = unwrap_u8_view(&t)?;
            Ok(wrap_cls("ByteBuffer", wrap_bytes(bytes)))
        }),
    );
    bc.methods.insert(
        "[]".into(),
        h::method(|_v, t, args, _n| {
            h::require_args(&args, 1, "[]")?;
            let (bytes, offset, len) = unwrap_u8_view(&t)?;
            let i = h::arg_int(&args, 0, "[]")?;
            if i < 0 || i as usize >= len {
                return Err(h::err("Index out of range"));
            }
            byte_at(&bytes, offset, i as usize)
        }),
    );
    bc.methods.insert(
        "[]=".into(),
        h::method(|_v, t, args, _n| {
            h::require_args(&args, 2, "[]=")?;
            let (bytes, offset, len) = unwrap_u8_view(&t)?;
            let i = h::arg_int(&args, 0, "[]=")?;
            if i < 0 || i as usize >= len {
                return Err(h::err("Index out of range"));
            }
            let b = h::arg_int(&args, 1, "[]=")?;
            set_byte(&bytes, offset, i as usize, b)?;
            Ok(Value::Null)
        }),
    );
    bc.methods.insert(
        "sublist".into(),
        h::method(|_v, t, args, _n| {
            h::require_args(&args, 1, "sublist")?;
            let (bytes, offset, len) = unwrap_u8_view(&t)?;
            let start = h::arg_int(&args, 0, "sublist")? as usize;
            let end = match args.get(1) {
                Some(a) => h::as_int(a).ok_or_else(|| h::err("end must be an int"))? as usize,
                None => len,
            };
            if start > end || end > len {
                return Err(h::err("Range out of bounds"));
            }
            Ok(wrap_cls(
                "Uint8List",
                u8_wrapper(bytes, offset + start, end - start),
            ))
        }),
    );
    bc
}

// ---------------------------------------------------------------------------
// Generic typed element list (Int16List, Float32List, ...)
// ---------------------------------------------------------------------------

/// Builds a typed list bridge. Elements are stored as `Value`s inside the
/// shared byte cell; `coerce` validates/converts an incoming Dart value,
/// `zero` is the initial element value, `to_bytes` synthesizes the raw byte
/// buffer backing `buffer`/`asUint8ListView`.
fn typed_list_definition(
    name: &str,
    elem_size: i64,
    zero: fn() -> Value,
    coerce: fn(&Value) -> Result<Value, InterpError>,
    to_bytes: fn(&[Value]) -> Vec<Value>,
) -> BridgedClassDef {
    let mut bc = BridgedClassDef::new(name);
    bc.native_type = Some(format!("d4rt::typed_data::{name}"));
    let name_c = name.to_string();
    let name_c2 = name_c.clone();
    bc.constructors.insert(
        "".into(),
        h::ctor(move |_v, args, _n| {
            let len = h::arg_int(&args, 0, &name_c2)?;
            if len < 0 {
                return Err(h::err("Length must be non-negative"));
            }
            Ok(wrap_cls(
                &name_c2,
                u8_wrapper(bytes_cell(len as usize), 0, len as usize),
            ))
        }),
    );
    let nm = name_c.clone();
    let es = elem_size;
    bc.static_methods.insert(
        "fromList".into(),
        h::static_method(move |_v, args, _n| {
            h::require_args(&args, 1, &nm)?;
            let src =
                h::as_list(&args[0]).ok_or_else(|| h::err("fromList expects a List of numbers"))?;
            let cells = Rc::new(RefCell::new(Vec::with_capacity(src.borrow().len())));
            for item in src.borrow().iter() {
                cells.borrow_mut().push(coerce(item)?);
            }
            let len = cells.borrow().len();
            Ok(wrap_cls(&nm, u8_wrapper(cells, 0, len)))
        }),
    );
    let nm = name_c.clone();
    bc.static_methods.insert(
        "view".into(),
        h::static_method(move |_v, args, _n| {
            h::require_args(&args, 1, &nm)?;
            let bytes = unwrap_bytes(&args[0])?;
            let total_elements = bytes.borrow().len() / es as usize;
            let off_elements =
                args.get(1).and_then(h::as_int).unwrap_or(0).max(0) as usize / es as usize;
            let len_elements = match args.get(2).and_then(h::as_int) {
                Some(l) => (l.max(0) as usize) / es as usize,
                None => total_elements.saturating_sub(off_elements),
            };
            Ok(wrap_cls(
                &nm,
                u8_wrapper(
                    bytes,
                    off_elements,
                    len_elements.min(total_elements.saturating_sub(off_elements)),
                ),
            ))
        }),
    );
    let nm = name_c.clone();
    bc.static_methods.insert(
        "sublistView".into(),
        h::static_method(move |_v, args, _n| {
            h::require_args(&args, 1, &nm)?;
            let (bytes, offset, len) = unwrap_u8_view(&args[0])?;
            let start = args.get(1).and_then(h::as_int).unwrap_or(0).max(0) as usize;
            let end = match args.get(2).and_then(h::as_int) {
                Some(e) => e as usize,
                None => len,
            };
            if start > end || end > len {
                return Err(h::err("Range out of bounds"));
            }
            Ok(wrap_cls(
                &nm,
                u8_wrapper(bytes, offset + start, end - start),
            ))
        }),
    );
    bc.getters.insert(
        "length".into(),
        h::getter(move |_v, t| {
            let (_, _, len) = unwrap_u8_view(&t)?;
            Ok(Value::Int(len as i64))
        }),
    );
    let es = elem_size;
    bc.getters.insert(
        "lengthInBytes".into(),
        h::getter(move |_v, t| {
            let (_, _, len) = unwrap_u8_view(&t)?;
            Ok(Value::Int(len as i64 * es))
        }),
    );
    let es = elem_size;
    bc.getters.insert(
        "elementSizeInBytes".into(),
        h::getter(move |_v, _t| Ok(Value::Int(es))),
    );
    let es = elem_size;
    bc.getters.insert(
        "offsetInBytes".into(),
        h::getter(move |_v, t| {
            let (_, offset, _) = unwrap_u8_view(&t)?;
            Ok(Value::Int(offset as i64 * es))
        }),
    );
    bc.getters.insert(
        "buffer".into(),
        h::getter(move |_v, t| {
            let (bytes, _, len) = unwrap_u8_view(&t)?;
            let raw = to_bytes(&bytes.borrow()[..len]);
            Ok(wrap_cls(
                "ByteBuffer",
                wrap_bytes(Rc::new(RefCell::new(raw))),
            ))
        }),
    );
    bc.getters.insert(
        "first".into(),
        h::getter(move |_v, t| {
            let (bytes, offset, len) = unwrap_u8_view(&t)?;
            if len == 0 {
                return Err(h::err("first on empty list"));
            }
            byte_at(&bytes, offset, 0)
        }),
    );
    bc.getters.insert(
        "last".into(),
        h::getter(move |_v, t| {
            let (bytes, offset, len) = unwrap_u8_view(&t)?;
            if len == 0 {
                return Err(h::err("last on empty list"));
            }
            byte_at(&bytes, offset, len - 1)
        }),
    );
    bc.getters.insert(
        "isEmpty".into(),
        h::getter(move |_v, t| {
            let (_, _, len) = unwrap_u8_view(&t)?;
            Ok(Value::Bool(len == 0))
        }),
    );
    bc.getters.insert(
        "isNotEmpty".into(),
        h::getter(move |_v, t| {
            let (_, _, len) = unwrap_u8_view(&t)?;
            Ok(Value::Bool(len != 0))
        }),
    );
    bc.getters.insert(
        "hashCode".into(),
        h::getter(move |_v, t| Ok(Value::Int(h::hash_code(&t)))),
    );
    bc.getters.insert(
        "runtimeType".into(),
        h::getter(move |_v, t| Ok(h::runtime_type(&t))),
    );
    bc.methods.insert(
        "[]".into(),
        h::method(move |_v, t, args, _n| {
            h::require_args(&args, 1, "[]")?;
            let (bytes, offset, len) = unwrap_u8_view(&t)?;
            let i = h::arg_int(&args, 0, "[]")?;
            if i < 0 || i as usize >= len {
                return Err(h::err("Index out of range"));
            }
            byte_at(&bytes, offset, i as usize)
        }),
    );
    bc.methods.insert(
        "[]=".into(),
        h::method(move |_v, t, args, _n| {
            h::require_args(&args, 2, "[]=")?;
            let (bytes, offset, len) = unwrap_u8_view(&t)?;
            let i = h::arg_int(&args, 0, "[]=")?;
            if i < 0 || i as usize >= len {
                return Err(h::err("Index out of range"));
            }
            let val = coerce(&args[1])?;
            bytes.borrow_mut()[offset + i as usize] = val;
            Ok(Value::Null)
        }),
    );
    let nm = name_c.clone();
    bc.methods.insert(
        "sublist".into(),
        h::method(move |_v, t, args, _n| {
            h::require_args(&args, 1, "sublist")?;
            let (bytes, offset, len) = unwrap_u8_view(&t)?;
            let start = h::arg_int(&args, 0, "sublist")? as usize;
            let end = match args.get(1) {
                Some(a) => h::as_int(a).ok_or_else(|| h::err("end must be an int"))? as usize,
                None => len,
            };
            if start > end || end > len {
                return Err(h::err("Range out of bounds"));
            }
            Ok(wrap_cls(
                &nm,
                u8_wrapper(bytes, offset + start, end - start),
            ))
        }),
    );
    bc.methods.insert(
        "getRange".into(),
        h::method(move |_v, t, args, _n| {
            h::require_args(&args, 2, "getRange")?;
            let (bytes, offset, len) = unwrap_u8_view(&t)?;
            let start = h::arg_int(&args, 0, "getRange")? as usize;
            let end = h::arg_int(&args, 1, "getRange")? as usize;
            if start > end || end > len {
                return Err(h::err("Range out of bounds"));
            }
            let b = bytes.borrow();
            let mut out = Vec::with_capacity(end - start);
            for i in start..end {
                out.push(b[offset + i].clone());
            }
            Ok(Value::List(Rc::new(RefCell::new(out))))
        }),
    );
    bc.methods.insert(
        "setRange".into(),
        h::method(move |_v, t, args, _n| {
            h::require_args(&args, 3, "setRange")?;
            let (bytes, offset, len) = unwrap_u8_view(&t)?;
            let start = h::arg_int(&args, 0, "setRange")? as usize;
            let end = h::arg_int(&args, 1, "setRange")? as usize;
            if start > end || end > len {
                return Err(h::err("Range out of bounds"));
            }
            let src = h::as_list(&args[2]).ok_or_else(|| h::err("setRange expects an Iterable"))?;
            let skip = args.get(3).and_then(h::as_int).unwrap_or(0).max(0) as usize;
            let mut b = bytes.borrow_mut();
            let srcb = src.borrow();
            for (i, item) in srcb.iter().skip(skip).take(end - start).enumerate() {
                b[offset + start + i] = coerce(item)?;
            }
            Ok(Value::Null)
        }),
    );
    bc.methods.insert(
        "setAll".into(),
        h::method(move |_v, t, args, _n| {
            h::require_args(&args, 2, "setAll")?;
            let (bytes, offset, len) = unwrap_u8_view(&t)?;
            let at = h::arg_int(&args, 0, "setAll")? as usize;
            if at > len {
                return Err(h::err("Index out of range"));
            }
            let src = h::as_list(&args[1]).ok_or_else(|| h::err("setAll expects an Iterable"))?;
            let mut b = bytes.borrow_mut();
            let srcb = src.borrow();
            for (i, item) in srcb.iter().enumerate() {
                if at + i >= len {
                    break;
                }
                b[offset + at + i] = coerce(item)?;
            }
            Ok(Value::Null)
        }),
    );
    bc.methods.insert(
        "fillRange".into(),
        h::method(move |_v, t, args, _n| {
            h::require_args(&args, 2, "fillRange")?;
            let (bytes, offset, len) = unwrap_u8_view(&t)?;
            let start = h::arg_int(&args, 0, "fillRange")? as usize;
            let end = match args.get(1).and_then(h::as_int) {
                Some(e) => e as usize,
                None => len,
            };
            if start > end || end > len {
                return Err(h::err("Range out of bounds"));
            }
            let fill = match args.get(2) {
                Some(v) => coerce(v)?,
                None => zero(),
            };
            let mut b = bytes.borrow_mut();
            for i in start..end {
                b[offset + i] = fill.clone();
            }
            Ok(Value::Null)
        }),
    );
    bc.methods.insert(
        "asUint8ListView".into(),
        h::method(move |_v, t, args, _n| {
            let (bytes, _offset, len) = unwrap_u8_view(&t)?;
            let raw = to_bytes(&bytes.borrow()[..len]);
            let total = raw.len();
            let byte_off = args.first().and_then(h::as_int).unwrap_or(0).max(0) as usize;
            let byte_len = match args.get(1).and_then(h::as_int) {
                Some(l) => l.max(0) as usize,
                None => total.saturating_sub(byte_off),
            };
            let byte_len = byte_len.min(total.saturating_sub(byte_off));
            Ok(wrap_cls(
                "Uint8List",
                u8_wrapper(Rc::new(RefCell::new(raw)), byte_off, byte_len),
            ))
        }),
    );
    bc.methods.insert(
        "toString".into(),
        h::method(move |_v, t, _a, _n| {
            let (bytes, offset, len) = unwrap_u8_view(&t)?;
            let b = bytes.borrow();
            let items: Vec<String> = b[offset..offset + len].iter().map(h::display).collect();
            Ok(Value::str(format!("[{}]", items.join(", "))))
        }),
    );
    bc.methods.insert(
        "==".into(),
        h::method(move |_v, t, args, _n| {
            h::require_args(&args, 1, "==")?;
            let other = args[0].clone();
            let Ok((bytes, offset, len)) = unwrap_u8_view(&t) else {
                return Ok(Value::Bool(false));
            };
            let Ok((ob, oo, ol)) = unwrap_u8_view(&other) else {
                return Ok(Value::Bool(false));
            };
            if len != ol {
                return Ok(Value::Bool(false));
            }
            let b = bytes.borrow();
            let ob = ob.borrow();
            for i in 0..len {
                if b[offset + i] != ob[oo + i] {
                    return Ok(Value::Bool(false));
                }
            }
            Ok(Value::Bool(true))
        }),
    );
    bc
}

fn int16_coerce(v: &Value) -> Result<Value, InterpError> {
    let n = h::as_int(v).ok_or_else(|| h::err("Expected an int"))?;
    if !(-32768..=32767).contains(&n) {
        return Err(h::err("Int16 value out of range"));
    }
    Ok(Value::Int(n))
}

fn float32_coerce(v: &Value) -> Result<Value, InterpError> {
    let d = h::as_double(v).ok_or_else(|| h::err("Expected a num"))?;
    Ok(Value::Double(d))
}

fn int16_to_bytes(cells: &[Value]) -> Vec<Value> {
    let mut out = Vec::with_capacity(cells.len() * 2);
    for c in cells {
        let n = h::as_int(c).unwrap_or(0) as i16;
        out.push(Value::Int(n as i64 & 0xFF));
        out.push(Value::Int((n >> 8) as i64 & 0xFF));
    }
    out
}

fn float32_to_bytes(cells: &[Value]) -> Vec<Value> {
    let mut out = Vec::with_capacity(cells.len() * 4);
    for c in cells {
        let d = h::as_double(c).unwrap_or(0.0) as f32;
        for b in d.to_le_bytes() {
            out.push(Value::Int(b as i64));
        }
    }
    out
}

fn int16_list_definition() -> BridgedClassDef {
    typed_list_definition(
        "Int16List",
        2,
        || Value::Int(0),
        int16_coerce,
        int16_to_bytes,
    )
}

fn float32_list_definition() -> BridgedClassDef {
    typed_list_definition(
        "Float32List",
        4,
        || Value::Double(0.0),
        float32_coerce,
        float32_to_bytes,
    )
}
