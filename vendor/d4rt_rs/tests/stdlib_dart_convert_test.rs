use d4rt_rs::Context;
use d4rt_rs::value::Value;

fn ev(c: &mut Context, body: &str, ret: &str) -> Value {
    let src = format!("import 'dart:convert';\nvoid main() {{ {body} return {ret}; }}");
    c.eval(&src)
        .unwrap_or_else(|e| panic!("{body} {ret}: {:?}", e.message()))
}

fn assert_list(v: Value, expected: Vec<i64>) {
    match v {
        Value::List(l) => {
            let got: Vec<i64> = l
                .borrow()
                .iter()
                .map(|x| match x {
                    Value::Int(i) => *i,
                    other => panic!("expected int, got {other:?}"),
                })
                .collect();
            assert_eq!(got, expected, "list mismatch");
        }
        other => panic!("expected list, got {other:?}"),
    }
}

fn assert_str(v: Value, expected: &str) {
    match v {
        Value::Str(s) => assert_eq!(&*s, expected),
        other => panic!("expected string, got {other:?}"),
    }
}

#[test]
fn json_codec_roundtrip() {
    let mut c = Context::new();
    let v = ev(
        &mut c,
        r#"JsonCodec codec = JsonCodec();
           Map<String, dynamic> data = {"key": "value", "number": 42};
           String encoded = codec.encode(data);
           Map<String, dynamic> decoded = codec.decode(encoded);
           return [encoded, decoded];"#,
        "",
    );
    match v {
        Value::List(l) => {
            let l = l.borrow();
            assert_str(l[0].clone(), r#"{"key":"value","number":42}"#);
            match &l[1] {
                Value::Map(m) => {
                    let m = m.borrow();
                    assert_eq!(m.entries.len(), 2);
                }
                other => panic!("expected map, got {other:?}"),
            }
        }
        other => panic!("expected list, got {other:?}"),
    }
}

#[test]
fn json_encoder_decoder_convert() {
    let mut c = Context::new();
    assert_str(
        ev(
            &mut c,
            r#"JsonEncoder encoder = JsonEncoder();
        Map<String, dynamic> data = {"key": "value", "number": 42};
        String encoded = encoder.convert(data);"#,
            "encoded",
        ),
        r#"{"key":"value","number":42}"#,
    );
    match ev(
        &mut c,
        r#"JsonDecoder decoder = JsonDecoder();
        String json = '{"key":"value","number":42}';
        Map<String, dynamic> decoded = decoder.convert(json);"#,
        "decoded['key']",
    ) {
        Value::Str(s) => assert_eq!(&*s, "value"),
        other => panic!("expected string, got {other:?}"),
    }
}

#[test]
fn json_top_level_functions() {
    let mut c = Context::new();
    assert_str(
        ev(
            &mut c,
            r#"Map<String, dynamic> data = {"key": "value", "number": 42};
        String encoded = jsonEncode(data);"#,
            "encoded",
        ),
        r#"{"key":"value","number":42}"#,
    );
    match ev(
        &mut c,
        r#"String json = '{"key":"value","number":42}';
        Map<String, dynamic> decoded = jsonDecode(json);"#,
        "decoded['number']",
    ) {
        Value::Int(i) => assert_eq!(i, 42),
        other => panic!("expected int, got {other:?}"),
    }
}

#[test]
fn json_arrays_primitives_and_nesting() {
    let mut c = Context::new();
    assert_str(
        ev(
            &mut c,
            "var x = [1, 2.5, true, null, 'a\\nb', [1], {'k': false}];",
            "jsonEncode(x)",
        ),
        r#"[1,2.5,true,null,"a\nb",[1],{"k":false}]"#,
    );
    match ev(
        &mut c,
        "var x = jsonDecode('{\"a\":[1,2],\"b\":{\"c\":null}}');",
        "x['b']['c']",
    ) {
        Value::Null => {}
        other => panic!("expected null, got {other:?}"),
    }
    match ev(
        &mut c,
        "var x = jsonDecode('{\"a\":[1,2],\"b\":{\"c\":null}}');",
        "x['a'][1]",
    ) {
        Value::Int(i) => assert_eq!(i, 2),
        other => panic!("expected int, got {other:?}"),
    }
}

#[test]
fn json_decode_trailing_whitespace() {
    // Regression: response bodies ending with a newline used to fail jsonDecode
    // with "Unexpected character at end of JSON".
    let mut c = Context::new();
    assert_str(
        ev(
            &mut c,
            r#"var x = jsonDecode('{"a":1}\n');"#,
            "x['a'].toString()",
        ),
        "1",
    );
    assert_str(
        ev(
            &mut c,
            r#"var x = jsonDecode('{"a":1}\n\n');"#,
            "x['a'].toString()",
        ),
        "1",
    );
    assert_str(
        ev(
            &mut c,
            r#"var x = jsonDecode(' [1, 2] ');"#,
            "x.length.toString()",
        ),
        "2",
    );
}

#[test]
fn json_reviver_and_to_encodable() {
    let mut c = Context::new();
    match ev(
        &mut c,
        r#"String json = '{"key":"value","date":"2023-01-01"}';
        Map<String, dynamic> decoded = jsonDecode(json, reviver: (key, value) {
          if (key == "date") { return "REVIVED"; }
          return value;
        });"#,
        "decoded['date']",
    ) {
        Value::Str(s) => assert_eq!(&*s, "REVIVED"),
        other => panic!("expected string, got {other:?}"),
    }
    assert_str(
        ev(
            &mut c,
            r#"Map<String, dynamic> data = {"key": "value", "n": 1};
        String encoded = jsonEncode(data, toEncodable: (object) => object.toString());"#,
            "encoded",
        ),
        r#"{"key":"value","n":1}"#,
    );
}

#[test]
fn json_codec_getters() {
    let mut c = Context::new();
    assert_str(
        ev(
            &mut c,
            "JsonCodec codec = JsonCodec();",
            "codec.encoder.runtimeType.toString()",
        ),
        "JsonEncoder",
    );
    assert_str(
        ev(
            &mut c,
            "JsonCodec codec = JsonCodec();",
            "codec.decoder.runtimeType.toString()",
        ),
        "JsonDecoder",
    );
}

#[test]
fn utf8_codec_roundtrip() {
    let mut c = Context::new();
    let v = ev(
        &mut c,
        r#"Utf8Codec codec = Utf8Codec();
           String original = "Hello, UTF-8!";
           List<int> encoded = codec.encode(original);
           String decoded = codec.decode(encoded);"#,
        "encoded",
    );
    assert_list(
        v,
        vec![72, 101, 108, 108, 111, 44, 32, 85, 84, 70, 45, 56, 33],
    );
    assert_str(
        ev(
            &mut c,
            r#"Utf8Codec codec = Utf8Codec();
        String original = "Hello, UTF-8!";
        List<int> encoded = codec.encode(original);"#,
            "codec.decode(encoded)",
        ),
        "Hello, UTF-8!",
    );
}

#[test]
fn utf8_encoder_decoder_convert() {
    let mut c = Context::new();
    assert_list(
        ev(
            &mut c,
            r#"Utf8Encoder encoder = Utf8Encoder();
        String original = "Encode this!";
        List<int> encoded = encoder.convert(original);"#,
            "encoded",
        ),
        vec![69, 110, 99, 111, 100, 101, 32, 116, 104, 105, 115, 33],
    );
    assert_str(
        ev(
            &mut c,
            r#"Utf8Decoder decoder = Utf8Decoder();
        List<int> encoded = [68, 101, 99, 111, 100, 101, 32, 116, 104, 105, 115, 33];"#,
            "decoder.convert(encoded)",
        ),
        "Decode this!",
    );
}

#[test]
fn base64_codec_roundtrip() {
    let mut c = Context::new();
    assert_str(
        ev(
            &mut c,
            "Base64Codec codec = Base64Codec();",
            "codec.encode([104, 101, 108, 108, 111])",
        ),
        "aGVsbG8=",
    );
    assert_list(
        ev(
            &mut c,
            "Base64Codec codec = Base64Codec();",
            "codec.decode('aGVsbG8=')",
        ),
        vec![104, 101, 108, 108, 111],
    );
    assert_str(
        ev(
            &mut c,
            "Base64Encoder encoder = Base64Encoder();",
            "encoder.convert([104, 101, 108, 108, 111])",
        ),
        "aGVsbG8=",
    );
    assert_list(
        ev(
            &mut c,
            "Base64Decoder decoder = Base64Decoder();",
            "decoder.convert('aGVsbG8=')",
        ),
        vec![104, 101, 108, 108, 111],
    );
    assert_str(
        ev(
            &mut c,
            "Base64Codec codec = Base64Codec();",
            "codec.normalize('aGVsbG8=')",
        ),
        "aGVsbG8=",
    );
}

#[test]
fn base64_top_level_functions() {
    let mut c = Context::new();
    assert_str(
        ev(&mut c, "", "base64Encode([104, 101, 108, 108, 111])"),
        "aGVsbG8=",
    );
    assert_str(
        ev(&mut c, "", "base64UrlEncode([104, 101, 108, 108, 111])"),
        "aGVsbG8=",
    );
    assert_list(
        ev(&mut c, "", "base64Decode('aGVsbG8=')"),
        vec![104, 101, 108, 108, 111],
    );
    assert_str(ev(&mut c, "", "base64Url.encode([251, 239])"), "--8=");
}

#[test]
fn codec_fuse() {
    let mut c = Context::new();
    let v = ev(
        &mut c,
        r#"Codec<String, String> codec = Utf8Codec().fuse(Base64Codec());
           String encoded = codec.encode("hello");
           String decoded = codec.decode(encoded);"#,
        "encoded",
    );
    assert_str(v, "aGVsbG8=");
    assert_str(
        ev(
            &mut c,
            r#"Codec<String, String> codec = Utf8Codec().fuse(Base64Codec());
        String encoded = codec.encode("hello");"#,
            "codec.decode(encoded)",
        ),
        "hello",
    );
}

#[test]
fn codec_encoder_decoder() {
    let mut c = Context::new();
    let v = ev(
        &mut c,
        r#"Codec<String, List<int>> codec = Utf8Codec();
           Converter<String, List<int>> encoder = codec.encoder;
           Converter<List<int>, String> decoder = codec.decoder;
           List<int> encoded = encoder.convert("hello");
           String decoded = decoder.convert(encoded);"#,
        "encoded",
    );
    assert_list(v, vec![104, 101, 108, 108, 111]);
    assert_str(
        ev(
            &mut c,
            r#"Codec<String, List<int>> codec = Utf8Codec();
        Converter<String, List<int>> encoder = codec.encoder;
        Converter<List<int>, String> decoder = codec.decoder;
        List<int> encoded = encoder.convert("hello");"#,
            "decoder.convert(encoded)",
        ),
        "hello",
    );
}

#[test]
fn encoding_constants() {
    let mut c = Context::new();
    let v = ev(
        &mut c,
        r#"Encoding encoding = utf8;
        String original = "Hello, UTF-8!";
        List<int> encoded = encoding.encode(original);
        String decoded = encoding.decode(encoded);"#,
        "encoded",
    );
    assert_list(
        v,
        vec![72, 101, 108, 108, 111, 44, 32, 85, 84, 70, 45, 56, 33],
    );
    assert_str(
        ev(
            &mut c,
            "Encoding encoding = utf8;",
            "encoding.decoder.runtimeType.toString()",
        ),
        "Utf8Decoder",
    );
    assert_str(
        ev(
            &mut c,
            "Encoding encoding = ascii;",
            "encoding.encoder.runtimeType.toString()",
        ),
        "AsciiEncoder",
    );
    assert_str(
        ev(
            &mut c,
            "Encoding encoding = latin1;",
            "encoding.decoder.runtimeType.toString()",
        ),
        "Latin1Decoder",
    );
    assert_list(
        ev(
            &mut c,
            "Encoding encoding = ascii;",
            "encoding.encode('Hello, ASCII!')",
        ),
        vec![72, 101, 108, 108, 111, 44, 32, 65, 83, 67, 73, 73, 33],
    );
    assert_list(
        ev(
            &mut c,
            "Encoding encoding = latin1;",
            "encoding.encode('Hello, Latin1!')",
        ),
        vec![
            72, 101, 108, 108, 111, 44, 32, 76, 97, 116, 105, 110, 49, 33,
        ],
    );
}

#[test]
fn html_escape_unknown_mode_escapes_everything() {
    let mut c = Context::new();
    let v = ev(
        &mut c,
        r#"HtmlEscape escape = HtmlEscape();
           String s = escape.convert("a & b < c > d \" e ' f / g");"#,
        "s",
    );
    assert_str(v, "a &amp; b &lt; c &gt; d &quot; e &#39; f &#47; g");
}

#[test]
fn html_escape_element_mode_only_escapes_lt_gt_and_amp() {
    let mut c = Context::new();
    assert_str(
        ev(
            &mut c,
            r#"HtmlEscape escape = HtmlEscape(HtmlEscapeMode.element);
           String s = escape.convert("<tag> \"quoted\" 'apos' /slash &amp");"#,
            "s",
        ),
        "&lt;tag&gt; \"quoted\" 'apos' /slash &amp;amp",
    );
}

#[test]
fn html_escape_attribute_mode_escapes_quot() {
    let mut c = Context::new();
    assert_str(
        ev(
            &mut c,
            r#"HtmlEscape escape = HtmlEscape(HtmlEscapeMode.attribute);
           String s = escape.convert("<a href=\"x\"> 'y' /z &");"#,
            "s",
        ),
        "&lt;a href=&quot;x&quot;&gt; 'y' /z &amp;",
    );
}

#[test]
fn html_escape_custom_mode() {
    let mut c = Context::new();
    assert_str(
        ev(
            &mut c,
            r#"HtmlEscapeMode mode = HtmlEscapeMode(name: 'custom', escapeSlash: true);
           HtmlEscape escape = HtmlEscape(mode);
           String s = escape.convert("/a/b & < >");"#,
            "s",
        ),
        "&#47;a&#47;b &amp; < >",
    );
}

#[test]
fn html_escape_global_constant() {
    let mut c = Context::new();
    assert_str(
        ev(&mut c, r#"String s = htmlEscape.convert("a & b");"#, "s"),
        "a &amp; b",
    );
}

#[test]
fn html_escape_mode_static_getters() {
    let mut c = Context::new();
    assert_str(
        ev(&mut c, "var m = HtmlEscapeMode.unknown;", "m.toString()"),
        "unknown",
    );
    assert_str(
        ev(&mut c, "var m = HtmlEscapeMode.element;", "m.toString()"),
        "element",
    );
    assert_str(
        ev(&mut c, "var m = HtmlEscapeMode.attribute;", "m.toString()"),
        "attribute",
    );
    assert_str(
        ev(
            &mut c,
            "var m = HtmlEscapeMode.sqAttribute;",
            "m.toString()",
        ),
        "attribute",
    );
}

#[test]
fn line_splitter_convert() {
    let mut c = Context::new();
    let v = ev(
        &mut c,
        r#"LineSplitter splitter = LineSplitter();
           List<String> lines = splitter.convert("one\ntwo\rthree\r\nfour");"#,
        "lines",
    );
    match v {
        Value::List(l) => {
            let got: Vec<String> = l
                .borrow()
                .iter()
                .map(|x| x.as_str().unwrap_or_default().to_string())
                .collect();
            assert_eq!(got, vec!["one", "two", "three", "four"]);
        }
        other => panic!("expected list, got {other:?}"),
    }
}

#[test]
fn line_splitter_convert_edge_cases() {
    let mut c = Context::new();
    let lines = |c: &mut Context, src: &str| -> Vec<String> {
        match ev(
            c,
            &format!(
                r#"LineSplitter splitter = LineSplitter(); List<String> l = splitter.convert("{src}");"#
            ),
            "l",
        ) {
            Value::List(l) => l
                .borrow()
                .iter()
                .map(|x| x.as_str().unwrap_or_default().to_string())
                .collect(),
            other => panic!("expected list, got {other:?}"),
        }
    };
    assert_eq!(lines(&mut c, "a\\nb"), vec!["a", "b"]);
    assert_eq!(lines(&mut c, "a\\rb"), vec!["a", "b"]);
    assert_eq!(lines(&mut c, "a\\r\\nb"), vec!["a", "b"]);
    assert_eq!(lines(&mut c, "a\\n"), vec!["a"]);
    assert_eq!(lines(&mut c, "\\r\\n"), vec![""]);
    assert_eq!(lines(&mut c, ""), Vec::<String>::new());
    assert_eq!(lines(&mut c, "no-newline"), vec!["no-newline"]);
}

#[test]
fn line_splitter_static_split() {
    let mut c = Context::new();
    let v = ev(
        &mut c,
        r#"List<String> l = LineSplitter.split('a\nb\nc');"#,
        "l",
    );
    match v {
        Value::List(l) => {
            let got: Vec<String> = l
                .borrow()
                .iter()
                .map(|x| x.as_str().unwrap_or_default().to_string())
                .collect();
            assert_eq!(got, vec!["a", "b", "c"]);
        }
        other => panic!("expected list, got {other:?}"),
    }
}

#[test]
fn line_splitter_chunked_conversion() {
    let mut c = Context::new();
    let v = ev(
        &mut c,
        r#"
        List lines = [];
        var sink = StringConversionSink.withCallback((List data) {
          lines = data;
        });
        var chunked = LineSplitter().startChunkedConversion(sink);
        chunked.add("hello\nwor");
        chunked.add("ld\n");
        chunked.add("tail");
        chunked.close();
        String out = lines.join(",");
    "#,
        "out",
    );
    assert_str(v, "hello,world,tail");
}

#[test]
fn string_conversion_sink_from() {
    let mut c = Context::new();
    let v = ev(
        &mut c,
        r#"
        List got = [];
        var inner = ChunkedConversionSink.withCallback((List data) {
          got = data;
        });
        var sink = StringConversionSink.from(inner);
        sink.add("a");
        sink.add("b");
        sink.close();
        String out = got.join("|");
    "#,
        "out",
    );
    assert_str(v, "a|b");
}

#[test]
fn string_conversion_sink_add_slice() {
    let mut c = Context::new();
    let v = ev(
        &mut c,
        r#"
        List got = [];
        var sink = StringConversionSink.withCallback((List data) {
          got = data;
        });
        sink.addSlice("hello world", 6, 11, false);
        sink.addSlice("! tail", 0, 1, true);
        String out = got.join("|");
    "#,
        "out",
    );
    assert_str(v, "world|!");
}

#[test]
fn chunked_conversion_sink_with_callback() {
    let mut c = Context::new();
    let v = ev(
        &mut c,
        r#"
        List chunks = [];
        var sink = ChunkedConversionSink.withCallback((List data) {
          chunks = data;
        });
        sink.add(1);
        sink.add("two");
        sink.close();
        String out = chunks.join(",");
    "#,
        "out",
    );
    assert_str(v, "1,two");
}

#[test]
fn byte_conversion_sink_with_callback() {
    let mut c = Context::new();
    let v = ev(
        &mut c,
        "var out = [];
         var s = ByteConversionSink.withCallback((b) { out.add(b); });
         s.add([1, 2]);
         s.add([3]);
         s.close();",
        "[out.length, out[0][0], out[0][1], out[0][2]].join(',')",
    );
    assert_str(v, "1,1,2,3");
}

#[test]
fn byte_conversion_sink_add_slice() {
    let mut c = Context::new();
    let v = ev(
        &mut c,
        "var out = [];
         var s = ByteConversionSink.withCallback((b) { out.add(b); });
         s.addSlice([1, 2, 3, 4, 5], 1, 4, false);
         s.close();",
        "[out[0][0], out[0][1], out[0][2]].join(',')",
    );
    assert_str(v, "2,3,4");
}

#[test]
fn converter_cast_and_dispatch() {
    let mut c = Context::new();
    let v = ev(
        &mut c,
        "final c = utf8.encoder as Converter<dynamic, dynamic>;
         final d = utf8.decoder as Converter<dynamic, dynamic>;
         final s = c.convert('hi');
         d.convert(s);",
        "d.convert(s)",
    );
    assert_str(v, "hi");
}

#[test]
fn encoding_get_by_name() {
    let mut c = Context::new();
    assert_str(
        ev(
            &mut c,
            "final e = Encoding.getByName('utf-8');
             final bytes = e.encode('hï');
             final s = utf8.decode(bytes);
             s;",
            "s",
        ),
        "hï",
    );
    assert_eq!(
        num_of(ev(
            &mut c,
            "final e = Encoding.getByName('nope');",
            "e == null ? 1 : 0"
        )),
        1
    );
}

fn num_of(v: Value) -> i64 {
    match v {
        Value::Int(i) => i,
        other => panic!("expected int, got {other:?}"),
    }
}

#[test]
fn base64_encoder_url_safe() {
    let mut c = Context::new();
    assert_str(
        ev(
            &mut c,
            "",
            "Base64Encoder(urlSafe: true).convert([251, 255])",
        ),
        "-_8=",
    );
    assert_str(
        ev(&mut c, "", "Base64Encoder().convert([251, 255])"),
        "+/8=",
    );
}
