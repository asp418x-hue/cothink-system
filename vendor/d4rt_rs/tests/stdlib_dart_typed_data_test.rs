use d4rt_rs::Context;
use d4rt_rs::value::Value;

fn ev(c: &mut Context, body: &str, ret: &str) -> Value {
    let src = format!("import 'dart:typed_data';\nvoid main() {{ {body} return {ret}; }}");
    c.eval(&src)
        .unwrap_or_else(|e| panic!("{body} {ret}: {:?}", e.message()))
}

fn str_(v: &Value) -> String {
    match v {
        Value::Str(s) => s.to_string(),
        other => panic!("expected string, got {other:?}"),
    }
}

fn num(v: &Value) -> i64 {
    match v {
        Value::Int(i) => *i,
        other => panic!("expected int, got {other:?}"),
    }
}

#[test]
fn endian_constants() {
    let mut c = Context::new();
    assert!(matches!(ev(&mut c, "", "Endian.big"), Value::Str(_)));
    let host_is_little = matches!(ev(&mut c, "", "Endian.host"), Value::Str(s) if &*s == "little");
    assert_eq!(host_is_little, cfg!(target_endian = "little"));
    assert_eq!(
        num(&ev(&mut c, "", "Endian.big == Endian.little ? 1 : 0")),
        0
    );
}

#[test]
fn byte_data_basics() {
    let mut c = Context::new();
    assert_eq!(
        num(&ev(
            &mut c,
            "var bd = new ByteData(10);",
            "bd.lengthInBytes"
        )),
        10
    );
    assert_eq!(
        num(&ev(
            &mut c,
            "var bd = new ByteData(10);",
            "bd.elementSizeInBytes"
        )),
        1
    );
    assert_eq!(
        num(&ev(
            &mut c,
            "var bd = new ByteData(2); bd.setInt8(0, -5); bd.setInt8(1, 127);",
            "bd.getInt8(0)"
        )),
        -5
    );
    assert_eq!(
        num(&ev(
            &mut c,
            "var bd = new ByteData(2); bd.setInt8(0, -5); bd.setInt8(1, 127);",
            "bd.getInt8(1)"
        )),
        127
    );
    assert_eq!(
        num(&ev(
            &mut c,
            "var bd = new ByteData(2); bd.setUint8(0, 0x34); bd.setUint8(1, 0x12);",
            "bd.getUint16(0)"
        )),
        0x3412
    );
    assert_eq!(
        num(&ev(
            &mut c,
            "var bd = new ByteData(2); bd.setUint8(0, 0x34); bd.setUint8(1, 0x12);",
            "bd.getUint16(0, Endian.little)"
        )),
        0x1234
    );
}

#[test]
fn byte_data_set_uint16_endianness() {
    let mut c = Context::new();
    let v = ev(
        &mut c,
        r#"var bd = new ByteData(4);
           bd.setUint16(0, 0xABCD);
           bd.setUint16(2, 0x1234, Endian.little);"#,
        "bd.buffer.asUint8List()[0]",
    );
    assert_eq!(num(&v), 0xAB);
    assert_eq!(
        num(&ev(
            &mut c,
            r#"var bd = new ByteData(4);
           bd.setUint16(0, 0xABCD);
           bd.setUint16(2, 0x1234, Endian.little);"#,
            "bd.buffer.asUint8List()[1]"
        )),
        0xCD
    );
    assert_eq!(
        num(&ev(
            &mut c,
            r#"var bd = new ByteData(4);
           bd.setUint16(0, 0xABCD);
           bd.setUint16(2, 0x1234, Endian.little);"#,
            "bd.buffer.asUint8List()[2]"
        )),
        0x34
    );
    assert_eq!(
        num(&ev(
            &mut c,
            r#"var bd = new ByteData(4);
           bd.setUint16(0, 0xABCD);
           bd.setUint16(2, 0x1234, Endian.little);"#,
            "bd.buffer.asUint8List()[3]"
        )),
        0x12
    );
    assert_eq!(
        num(&ev(
            &mut c,
            r#"var bd = new ByteData(4);
           bd.setUint16(0, 0xABCD);
           bd.setUint16(2, 0x1234, Endian.little);"#,
            "bd.getUint16(0)"
        )),
        0xABCD
    );
    assert_eq!(
        num(&ev(
            &mut c,
            r#"var bd = new ByteData(4);
           bd.setUint16(0, 0xABCD);
           bd.setUint16(2, 0x1234, Endian.little);"#,
            "bd.getUint16(2, Endian.little)"
        )),
        0x1234
    );
}

#[test]
fn byte_data_int32_and_floats() {
    let mut c = Context::new();
    assert_eq!(
        num(&ev(
            &mut c,
            "var bd = new ByteData(4); bd.setInt32(0, -123456);",
            "bd.getInt32(0)"
        )),
        -123456
    );
    assert_eq!(
        num(&ev(
            &mut c,
            "var bd = new ByteData(8); bd.setUint32(0, 0xDEADBEEF);",
            "bd.getUint32(0)"
        )),
        0xDEADBEEF
    );
    assert_eq!(
        num(&ev(
            &mut c,
            "var bd = new ByteData(4); bd.setInt32(0, -1, Endian.little);",
            "bd.getInt32(0, Endian.little)"
        )),
        -1
    );
    assert_eq!(
        num(&ev(
            &mut c,
            "var bd = new ByteData(4); bd.setUint32(0, 0x12345678, Endian.little);",
            "bd.getUint32(0, Endian.little)"
        )),
        0x12345678
    );
    let f = ev(
        &mut c,
        "var bd = new ByteData(4); bd.setFloat32(0, 1.5);",
        "bd.getFloat32(0)",
    );
    match f {
        Value::Double(x) => assert!((x - 1.5).abs() < 1e-9),
        other => panic!("expected double, got {other:?}"),
    }
    let d = ev(
        &mut c,
        "var bd = new ByteData(8); bd.setFloat64(0, 3.25);",
        "bd.getFloat64(0)",
    );
    match d {
        Value::Double(x) => assert!((x - 3.25).abs() < 1e-9),
        other => panic!("expected double, got {other:?}"),
    }
}

#[test]
fn byte_data_out_of_bounds() {
    let mut c = Context::new();
    let src = "import 'dart:typed_data';\nvoid main() { var bd = new ByteData(1); bd.getInt8(1); return 0; }".to_string();
    assert!(c.eval(&src).is_err());
    let src = "import 'dart:typed_data';\nvoid main() { var bd = new ByteData(1); bd.setInt8(1, 0); return 0; }".to_string();
    assert!(c.eval(&src).is_err());
    let src = "import 'dart:typed_data';\nvoid main() { var bd = new ByteData(1); bd.getUint16(0); return 0; }".to_string();
    assert!(c.eval(&src).is_err());
}

#[test]
fn byte_buffer_views() {
    let mut c = Context::new();
    assert_eq!(
        num(&ev(
            &mut c,
            "var list = new Uint8List(10);",
            "list.buffer.lengthInBytes"
        )),
        10
    );
    let v = ev(
        &mut c,
        r#"var list = new Uint8List(5);
           for (var i = 0; i < 5; i++) { list[i] = i * 10; }
           var buffer = list.buffer;
           var view = buffer.asUint8List();"#,
        "view.runtimeType.toString()",
    );
    match v {
        Value::Str(s) => assert_eq!(&*s, "Uint8List"),
        other => panic!("expected string, got {other:?}"),
    }
    assert_eq!(
        num(&ev(
            &mut c,
            r#"var list = new Uint8List(10);
           for (var i = 0; i < 10; i++) { list[i] = i; }
           var buffer = list.buffer;
           var view = buffer.asUint8List(2, 3);
           var sum = 0;
           for (var i = 0; i < view.length; i++) { sum += view[i]; }"#,
            "sum"
        )),
        2 + 3 + 4
    );
    // View shares the underlying buffer: mutation through the view is visible.
    assert_eq!(
        num(&ev(
            &mut c,
            r#"var list = new Uint8List(4);
           list[1] = 7;
           var view = list.buffer.asUint8List(1, 2);
           view[0] = 99;"#,
            "list[1]"
        )),
        99
    );
}

#[test]
fn uint8_list_basics() {
    let mut c = Context::new();
    assert_eq!(
        num(&ev(&mut c, "var list = Uint8List(5);", "list.length")),
        5
    );
    assert_eq!(
        num(&ev(
            &mut c,
            "var list = Uint8List(5);",
            "list.elementSizeInBytes"
        )),
        1
    );
    assert_eq!(num(&ev(&mut c, "var list = Uint8List(5);", "list[0]")), 0);
    assert_eq!(
        num(&ev(
            &mut c,
            "var list = Uint8List.fromList([10, 20, 30]);",
            "list.length"
        )),
        3
    );
    assert_eq!(
        num(&ev(
            &mut c,
            "var list = Uint8List.fromList([10, 20, 30]);",
            "list[2]"
        )),
        30
    );
    assert_eq!(
        num(&ev(
            &mut c,
            "var list = Uint8List(3); list[0] = 255; list[1] = 128; list[2] = list[0] - list[1];",
            "list[2]"
        )),
        127
    );
}

#[test]
fn uint8_list_sublist() {
    let mut c = Context::new();
    let v = ev(
        &mut c,
        r#"var list = Uint8List.fromList([0, 1, 2, 3, 4, 5]);
           var sub = list.sublist(2, 5);
           var values = [];
           for (var i = 0; i < sub.length; i++) { values.add(sub[i]); }"#,
        "values",
    );
    match v {
        Value::List(l) => {
            let got: Vec<i64> = l.borrow().iter().map(num).collect();
            assert_eq!(got, vec![2, 3, 4]);
        }
        other => panic!("expected list, got {other:?}"),
    }
    assert_eq!(
        num(&ev(
            &mut c,
            "var list = Uint8List.fromList([0, 1, 2, 3, 4, 5]); var sub = list.sublist(2, 5);",
            "list.length"
        )),
        6
    );
}

#[test]
fn uint8_list_errors() {
    let mut c = Context::new();
    let src =
        "import 'dart:typed_data';\nvoid main() { Uint8List.fromList([1, 'a', 3]); return 0; }"
            .to_string();
    assert!(c.eval(&src).is_err());
    let src = "import 'dart:typed_data';\nvoid main() { var list = Uint8List(2); list[2] = 0; return 0; }".to_string();
    assert!(c.eval(&src).is_err());
    let src =
        "import 'dart:typed_data';\nvoid main() { var list = Uint8List(2); return list[-1]; }"
            .to_string();
    assert!(c.eval(&src).is_err());
}

#[test]
fn int16_list_basics() {
    let mut c = Context::new();
    let v = ev(
        &mut c,
        "var l = Int16List(3); l[0] = 1; l[1] = -2; l[2] = 32767;",
        "[l[0], l[1], l[2], l.length, l.elementSizeInBytes, l.lengthInBytes, l.offsetInBytes].join(',')",
    );
    assert_eq!(str_(&v), "1,-2,32767,3,2,6,0");
    assert_eq!(
        str_(&ev(
            &mut c,
            "",
            "Int16List.fromList([100, -300, 5]).toString()"
        )),
        "[100, -300, 5]"
    );
    assert_eq!(
        num(&ev(
            &mut c,
            "var l = Int16List.fromList([1, 2, 3, 4]);",
            "l[0] + l[1] + l[3]"
        )),
        7
    );
}

#[test]
fn int16_list_views_and_ranges() {
    let mut c = Context::new();
    let v = ev(
        &mut c,
        "var sub = Int16List.sublistView(Int16List.fromList([10, 20, 30, 40, 50]), 1, 3);",
        "[sub.length, sub[0], sub[1]].join(',')",
    );
    assert_eq!(str_(&v), "2,20,30");
    let v = ev(
        &mut c,
        "var l = Int16List.fromList([1, 2, 3, 4]);
         l.setAll(1, [7, 8]);
         l.fillRange(3, 4, -1);
         l.setRange(0, 2, [9, 10]);
         var r = l.getRange(0, 4);",
        "[l[0], l[1], l[2], l[3], r.length].join(',')",
    );
    assert_eq!(str_(&v), "9,10,8,-1,4");
    assert_eq!(
        num(&ev(
            &mut c,
            "var l = Int16List.fromList([1, 2]);",
            "l.buffer.lengthInBytes"
        )),
        4
    );
    assert_eq!(
        num(&ev(
            &mut c,
            "var l = Int16List.fromList([1, 256]); var b = l.asUint8ListView();",
            "b[0] + b[1] * 10 + b[2] * 100 + b[3] * 1000"
        )),
        1001
    );
}

#[test]
fn int16_list_errors() {
    let mut c = Context::new();
    let src =
        "import 'dart:typed_data';\nvoid main() { Int16List.fromList([1, 40000]); return 0; }";
    assert!(c.eval(src).is_err());
    let src =
        "import 'dart:typed_data';\nvoid main() { var l = Int16List(2); l[5] = 0; return 0; }";
    assert!(c.eval(src).is_err());
}

#[test]
fn float32_list_basics() {
    let mut c = Context::new();
    let v = ev(
        &mut c,
        "var f = Float32List(2); f[0] = 1.5; f[1] = -2.25;",
        "[f[0], f[1], f.length, f.elementSizeInBytes, f.lengthInBytes].join(',')",
    );
    assert_eq!(str_(&v), "1.5,-2.25,2,4,8");
    assert_eq!(
        str_(&ev(
            &mut c,
            "",
            "Float32List.fromList([1.5, 2.5, 3.75]).toString()"
        )),
        "[1.5, 2.5, 3.75]"
    );
    assert_eq!(
        num(&ev(
            &mut c,
            "var f = Float32List.fromList([1.5]);",
            "f[0] > 1 ? 1 : 0"
        )),
        1
    );
    assert_eq!(
        num(&ev(
            &mut c,
            "var f = Float32List(3); f.fillRange(0, 3, 2.5);",
            "f[2] > 2 ? 1 : 0"
        )),
        1
    );
    let v = ev(
        &mut c,
        "var f = Float32List.fromList([1.5, -0.5]);",
        "[f.first, f.last, f.isEmpty ? 1 : 0].join(',')",
    );
    assert_eq!(str_(&v), "1.5,-0.5,0");
}

#[test]
fn float32_list_view_and_bytes() {
    let mut c = Context::new();
    let v = ev(
        &mut c,
        "var l = Float32List.fromList([1.0, 2.0, 3.0]);",
        "[l.buffer.lengthInBytes, l.asUint8ListView(4, 4).length].join(',')",
    );
    assert_eq!(str_(&v), "12,4");
}

#[test]
fn byte_data_view_and_sublist_view() {
    let mut c = Context::new();
    let v = ev(
        &mut c,
        "final u = Uint8List.fromList([10, 20, 30, 40]);
         final bd = ByteData.view(u.buffer, 1, 2);
         final w = ByteData.sublistView(u, 1, 4);
         [bd.lengthInBytes, bd.getUint8(0), bd.getUint8(1), w.lengthInBytes, w.getUint8(0)].join(',');",
        "[bd.lengthInBytes, bd.getUint8(0), bd.getUint8(1), w.lengthInBytes, w.getUint8(0)].join(',')",
    );
    assert_eq!(str_(&v), "2,20,30,3,20");
}

#[test]
fn uint8_list_view_and_sublist_view() {
    let mut c = Context::new();
    let v = ev(
        &mut c,
        "final u = Uint8List.fromList([1, 2, 3, 4, 5]);
         final v = Uint8List.view(u.buffer, 1, 3);
         v[0] = 99;
         final w = Uint8List.sublistView(u, 2, 4);
         [v.length, v[0], v[1], u[1], w[0], w[1]].join(',');",
        "[v.length, v[0], v[1], u[1], w[0], w[1]].join(',')",
    );
    assert_eq!(str_(&v), "3,99,3,99,3,4");
}
