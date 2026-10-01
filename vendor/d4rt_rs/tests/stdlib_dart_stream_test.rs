use d4rt_rs::Context;
use d4rt_rs::value::Value;

fn ev(c: &mut Context, body: &str) -> Value {
    ev_imports(c, "import 'dart:async';", body)
}

fn ev_imports(c: &mut Context, imports: &str, body: &str) -> Value {
    let src = format!("{imports}\nFuture<dynamic> main() async {{ {body} }}");
    c.eval(&src)
        .unwrap_or_else(|e| panic!("eval failed: {:?}", e.message()))
}

fn num(v: &Value) -> i64 {
    match v {
        Value::Int(i) => *i,
        other => panic!("expected int, got {other:?}"),
    }
}

fn boolean(v: &Value) -> bool {
    match v {
        Value::Bool(b) => *b,
        other => panic!("expected bool, got {other:?}"),
    }
}

fn str(v: &Value) -> String {
    match v {
        Value::Str(s) => s.to_string(),
        other => panic!("expected string, got {other:?}"),
    }
}

fn nums(v: &Value) -> Vec<i64> {
    match v {
        Value::List(l) => l.borrow().iter().map(num).collect(),
        other => panic!("expected list, got {other:?}"),
    }
}

fn strs(v: &Value) -> Vec<String> {
    match v {
        Value::List(l) => l.borrow().iter().map(str).collect(),
        other => panic!("expected list, got {other:?}"),
    }
}

#[test]
fn stream_value_first() {
    let mut c = Context::new();
    assert_eq!(
        num(&ev(&mut c, "return await Stream.value(123).first;")),
        123
    );
}

#[test]
fn stream_value_single() {
    let mut c = Context::new();
    assert_eq!(
        num(&ev(&mut c, "return await Stream.value(42).single;")),
        42
    );
}

#[test]
fn stream_error_listen() {
    let mut c = Context::new();
    let v = ev(
        &mut c,
        "try { await Stream.error(Exception('Boom')).toList(); return 0; } catch (e) { return e.toString(); }",
    );
    assert!(str(&v).contains("Boom"));
}

#[test]
fn stream_empty() {
    let mut c = Context::new();
    assert!(boolean(&ev(&mut c, "return await Stream.empty().isEmpty;")));
    assert!(!boolean(&ev(
        &mut c,
        "return await Stream.empty().contains(1);"
    )));
}

#[test]
fn stream_from_iterable() {
    let mut c = Context::new();
    assert_eq!(
        nums(&ev(
            &mut c,
            "return await Stream.fromIterable([1, 2, 3]).toList();"
        )),
        [1, 2, 3]
    );
}

#[test]
fn stream_periodic() {
    let mut c = Context::new();
    assert_eq!(
        nums(&ev(
            &mut c,
            "return await Stream.periodic(Duration(milliseconds: 1), (i) => i).take(3).toList();"
        )),
        [0, 1, 2]
    );
}

#[test]
fn stream_from_future() {
    let mut c = Context::new();
    assert_eq!(
        strs(&ev(
            &mut c,
            "return await Stream.fromFuture(Future.value('done')).toList();"
        )),
        ["done"]
    );
}

#[test]
fn stream_map() {
    let mut c = Context::new();
    assert_eq!(
        nums(&ev(
            &mut c,
            "return await Stream.fromIterable([1, 2, 3]).map((x) => x * 2).toList();"
        )),
        [2, 4, 6]
    );
}

#[test]
fn stream_where() {
    let mut c = Context::new();
    assert_eq!(
        nums(&ev(
            &mut c,
            "return await Stream.fromIterable([1, 2, 3, 4]).where((x) => x % 2 == 0).toList();"
        )),
        [2, 4]
    );
}

#[test]
fn stream_expand() {
    let mut c = Context::new();
    assert_eq!(
        nums(&ev(
            &mut c,
            "return await Stream.fromIterable([1, 2]).expand((x) => [x, x * 10]).toList();"
        )),
        [1, 10, 2, 20]
    );
}

#[test]
fn stream_take_skip() {
    let mut c = Context::new();
    assert_eq!(
        nums(&ev(
            &mut c,
            "return await Stream.fromIterable([1, 2, 3, 4]).take(2).toList();"
        )),
        [1, 2]
    );
    assert_eq!(
        nums(&ev(
            &mut c,
            "return await Stream.fromIterable([1, 2, 3, 4]).skip(2).toList();"
        )),
        [3, 4]
    );
}

#[test]
fn stream_take_while_skip_while() {
    let mut c = Context::new();
    assert_eq!(
        nums(&ev(
            &mut c,
            "return await Stream.fromIterable([1, 2, 3, 4]).takeWhile((x) => x < 3).toList();"
        )),
        [1, 2]
    );
    assert_eq!(
        nums(&ev(
            &mut c,
            "return await Stream.fromIterable([1, 2, 3, 4]).skipWhile((x) => x < 3).toList();"
        )),
        [3, 4]
    );
}

#[test]
fn stream_distinct() {
    let mut c = Context::new();
    assert_eq!(
        nums(&ev(
            &mut c,
            "return await Stream.fromIterable([1, 1, 2, 2, 3]).distinct().toList();"
        )),
        [1, 2, 3]
    );
}

#[test]
fn stream_async_map() {
    let mut c = Context::new();
    assert_eq!(
        nums(&ev(
            &mut c,
            "return await Stream.fromIterable([1, 2, 3]).asyncMap((x) async => x * 2).toList();"
        )),
        [2, 4, 6]
    );
}

#[test]
fn stream_async_expand() {
    let mut c = Context::new();
    assert_eq!(
        nums(&ev(
            &mut c,
            "return await Stream.fromIterable([1, 2]).asyncExpand((x) async => Stream.fromIterable([x, x * 10])).toList();"
        )),
        [1, 10, 2, 20]
    );
}

#[test]
fn stream_to_list_join() {
    let mut c = Context::new();
    assert_eq!(
        str(&ev(
            &mut c,
            "return await Stream.fromIterable(['a', 'b', 'c']).join('-');"
        )),
        "a-b-c"
    );
}

#[test]
fn stream_any_every_contains() {
    let mut c = Context::new();
    assert!(boolean(&ev(
        &mut c,
        "return await Stream.fromIterable([1, 2, 3]).any((x) => x == 2);"
    )));
    assert!(boolean(&ev(
        &mut c,
        "return await Stream.fromIterable([1, 2, 3]).every((x) => x > 0);"
    )));
    assert!(boolean(&ev(
        &mut c,
        "return await Stream.fromIterable([1, 2, 3]).contains(2);"
    )));
}

#[test]
fn stream_fold_reduce() {
    let mut c = Context::new();
    assert_eq!(
        num(&ev(
            &mut c,
            "return await Stream.fromIterable([1, 2, 3, 4]).fold(0, (a, b) => a + b);"
        )),
        10
    );
    assert_eq!(
        num(&ev(
            &mut c,
            "return await Stream.fromIterable([1, 2, 3, 4]).reduce((a, b) => a + b);"
        )),
        10
    );
}

#[test]
fn stream_foreach() {
    let mut c = Context::new();
    assert_eq!(
        nums(&ev(
            &mut c,
            "var r = <int>[]; await Stream.fromIterable([1, 2, 3]).forEach((x) { r.add(x * 10); }); return r;"
        )),
        [10, 20, 30]
    );
}

#[test]
fn stream_last_length() {
    let mut c = Context::new();
    assert_eq!(
        num(&ev(
            &mut c,
            "return await Stream.fromIterable([1, 2, 3]).last;"
        )),
        3
    );
    assert_eq!(
        num(&ev(
            &mut c,
            "return await Stream.fromIterable([1, 2, 3]).length;"
        )),
        3
    );
}

#[test]
fn stream_first_where() {
    let mut c = Context::new();
    assert_eq!(
        num(&ev(
            &mut c,
            "return await Stream.fromIterable([1, 2, 3, 4]).firstWhere((x) => x % 3 == 0);"
        )),
        3
    );
}

#[test]
fn stream_last_where() {
    let mut c = Context::new();
    assert_eq!(
        num(&ev(
            &mut c,
            "return await Stream.fromIterable([1, 2, 3, 4]).lastWhere((x) => x % 2 == 0);"
        )),
        4
    );
}

#[test]
fn stream_single_where() {
    let mut c = Context::new();
    assert_eq!(
        num(&ev(
            &mut c,
            "return await Stream.fromIterable([1, 2, 3, 4]).singleWhere((x) => x == 3);"
        )),
        3
    );
}

#[test]
fn stream_element_at() {
    let mut c = Context::new();
    assert_eq!(
        str(&ev(
            &mut c,
            "return await Stream.fromIterable(['a', 'b', 'c']).elementAt(1);"
        )),
        "b"
    );
}

#[test]
fn stream_controller_add_close() {
    let mut c = Context::new();
    assert_eq!(
        nums(&ev(
            &mut c,
            "var c = StreamController<int>(); c.add(1); c.add(2); c.add(3); var s = c.stream; await c.close(); return await s.toList();"
        )),
        [1, 2, 3]
    );
}

#[test]
fn stream_controller_add_error() {
    let mut c = Context::new();
    let v = ev(
        &mut c,
        "var c = StreamController<int>(); c.add(1); c.addError(Exception('Bad')); c.add(2); var r = <String>[]; c.stream.listen((x) => r.add('d:' + x.toString()), onError: (e) => r.add('e:' + e.toString()), onDone: () {}); await c.close(); return r;",
    );
    assert_eq!(strs(&v), ["d:1", "e:Exception: Bad", "d:2"]);
}

#[test]
fn stream_controller_sink() {
    let mut c = Context::new();
    assert_eq!(
        nums(&ev(
            &mut c,
            "var c = StreamController<int>(); c.sink.add(5); var s = c.stream; await c.close(); return await s.toList();"
        )),
        [5]
    );
}

#[test]
fn stream_controller_broadcast() {
    let mut c = Context::new();
    let v = ev(
        &mut c,
        "var c = StreamController<int>.broadcast(); var a = <int>[]; var b = <int>[]; c.stream.listen((x) => a.add(x)); c.stream.listen((x) => b.add(x)); c.add(1); c.add(2); await c.close(); return [a.length, b.length, a[0], b[0], a[1], b[1]];",
    );
    assert_eq!(nums(&v), [2, 2, 1, 1, 2, 2]);
}

#[test]
fn stream_listen_pause_resume() {
    let mut c = Context::new();
    assert_eq!(
        nums(&ev(
            &mut c,
            "var c = StreamController<int>(); var r = <int>[]; var s = c.stream.listen((x) => r.add(x)); c.add(1); await Future.delayed(Duration(milliseconds: 2)); s.pause(); c.add(2); await Future.delayed(Duration(milliseconds: 2)); var before = r.length; s.resume(); await Future.delayed(Duration(milliseconds: 2)); var after = r.length; return [before, after];"
        )),
        [1, 2]
    );
}

#[test]
fn stream_handle_error_one_arg() {
    let mut c = Context::new();
    assert_eq!(
        nums(&ev(
            &mut c,
            "var c = StreamController<int>(); var r = <int>[]; c.stream.handleError((e) { r.add(99); }).listen((x) => r.add(x)); c.add(1); c.addError(Exception('x')); c.add(2); await c.close(); return r;"
        )),
        [1, 99, 2]
    );
}

#[test]
fn stream_handle_error_two_args() {
    let mut c = Context::new();
    assert_eq!(
        nums(&ev(
            &mut c,
            "var c = StreamController<int>(); var r = <int>[]; c.stream.handleError((e, st) { r.add(77); }).listen((x) => r.add(x)); c.add(1); c.addError(Exception('x')); c.add(2); await c.close(); return r;"
        )),
        [1, 77, 2]
    );
}

#[test]
fn stream_transformer_from_handlers_data() {
    let mut c = Context::new();
    assert_eq!(
        nums(&ev(
            &mut c,
            "var c = StreamController<int>(); var t = StreamTransformer<int, int>.fromHandlers(handleData: (data, sink) { sink.add(data * 2); }); var s = c.stream.transform(t); var r = <int>[]; s.listen((x) => r.add(x)); c.add(1); c.add(2); c.add(3); await c.close(); return r;"
        )),
        [2, 4, 6]
    );
}

#[test]
fn stream_transformer_from_handlers_error() {
    let mut c = Context::new();
    let v = ev(
        &mut c,
        "var c = StreamController<int>(); var t = StreamTransformer<int, String>.fromHandlers(handleData: (data, sink) { if (data < 0) { throw Exception('Negative'); } sink.add('Value: ' + data.toString()); }, handleError: (error, stackTrace, sink) { sink.add('Error caught: ' + error.toString()); }); var s = c.stream.transform(t); var r = <String>[]; s.listen((x) => r.add(x)); c.add(1); c.addError(Exception('Test error')); c.add(2); await c.close(); return r.length >= 2;",
    );
    assert!(boolean(&v));
}

#[test]
fn stream_transformer_from_handlers_done() {
    let mut c = Context::new();
    assert!(boolean(&ev(
        &mut c,
        "var c = StreamController<int>(); bool done = false; var t = StreamTransformer<int, int>.fromHandlers(handleData: (data, sink) { sink.add(data); }, handleDone: (sink) { done = true; sink.close(); }); var s = c.stream.transform(t); s.listen((x) {}); c.add(1); await c.close(); return done;"
    )));
}

#[test]
fn stream_transformer_from_bind() {
    let mut c = Context::new();
    assert_eq!(
        nums(&ev(
            &mut c,
            "var c = StreamController<int>(); var t = StreamTransformer<int, int>.fromBind((s) { var r = StreamController<int>(); s.listen((x) => r.add(x * 3)); return r.stream; }); var r = <int>[]; c.stream.transform(t).listen((x) => r.add(x)); c.add(1); c.add(2); await c.close(); return r;"
        )),
        [3, 6]
    );
}

#[test]
fn stream_chained_transforms() {
    let mut c = Context::new();
    assert_eq!(
        nums(&ev(
            &mut c,
            "return await Stream.fromIterable([1, 2, 3, 4]).where((x) => x.isEven).map((x) => x * 10).toList();"
        )),
        [20, 40]
    );
}

#[test]
fn stream_timeout_error_when_silent() {
    let mut c = Context::new();
    let v = ev(
        &mut c,
        "var c = StreamController<int>(); c.add(1); var r = <int>[]; try { await c.stream.timeout(Duration(milliseconds: 5)).forEach((x) => r.add(x)); return 0; } catch (e) { return e.toString().contains('TimeoutException') ? 1 : 2; }",
    );
    assert_eq!(num(&v), 1);
}

#[test]
fn stream_timeout_event_resets_timer() {
    let mut c = Context::new();
    let v = ev(
        &mut c,
        "var c = StreamController<int>(); var r = <int>[]; var f = c.stream.timeout(Duration(milliseconds: 3)).listen((x) => r.add(x)); c.add(1); await Future.delayed(Duration(milliseconds: 1)); c.add(2); await Future.delayed(Duration(milliseconds: 1)); c.add(3); await c.close(); return r;",
    );
    assert_eq!(nums(&v), [1, 2, 3]);
}

#[test]
fn stream_timeout_on_timeout_callback() {
    let mut c = Context::new();
    assert_eq!(
        nums(&ev(
            &mut c,
            "var c = StreamController<int>(); var r = <int>[]; c.stream.timeout(Duration(milliseconds: 3), onTimeout: (sink) { sink.add(42); }).listen((x) => r.add(x)); await Future.delayed(Duration(milliseconds: 10)); await c.close(); return r;"
        )),
        [42]
    );
}

#[test]
fn stream_as_future_last_event() {
    let mut c = Context::new();
    assert_eq!(
        num(&ev(
            &mut c,
            "return await Stream.fromIterable([1, 2, 3]).asFuture();"
        )),
        3
    );
}

#[test]
fn stream_as_future_seed() {
    let mut c = Context::new();
    assert_eq!(
        num(&ev(&mut c, "return await Stream<int>.empty().asFuture(7);")),
        7
    );
}

#[test]
fn stream_as_future_error() {
    let mut c = Context::new();
    let v = ev(
        &mut c,
        "try { await Stream.error(Exception('bad')).asFuture(); return 0; } catch (e) { return e.toString().contains('bad') ? 1 : 2; }",
    );
    assert_eq!(num(&v), 1);
}

#[test]
fn stream_listen_cancel_on_error() {
    let mut c = Context::new();
    assert_eq!(
        nums(&ev(
            &mut c,
            "var c = StreamController<int>(); var r = <int>[]; var s = c.stream.listen((x) => r.add(x), cancelOnError: true); c.add(1); c.addError(Exception('x')); c.add(2); await Future.delayed(Duration(milliseconds: 5)); return r;"
        )),
        [1]
    );
}

#[test]
fn stream_first_where_or_else() {
    let mut c = Context::new();
    assert_eq!(
        num(&ev(
            &mut c,
            "return await Stream.fromIterable([1, 2, 3]).firstWhere((x) => x > 5, orElse: () => 99);"
        )),
        99
    );
}

#[test]
fn stream_last_where_or_else() {
    let mut c = Context::new();
    assert_eq!(
        num(&ev(
            &mut c,
            "return await Stream.fromIterable([1, 2, 3]).lastWhere((x) => x > 5, orElse: () => 88);"
        )),
        88
    );
}

#[test]
fn stream_single_where_or_else() {
    let mut c = Context::new();
    assert_eq!(
        num(&ev(
            &mut c,
            "return await Stream.fromIterable([1, 2, 3]).singleWhere((x) => x > 5, orElse: () => 77);"
        )),
        77
    );
}

#[test]
fn stream_pipe_forwards_to_controller() {
    let mut c = Context::new();
    assert_eq!(
        num(&ev(
            &mut c,
            "var c = StreamController<int>(); var r = <int>[]; c.stream.listen((x) => r.add(x)); await Stream.fromIterable([1, 2, 3]).pipe(c.sink); await Future.delayed(Duration(milliseconds: 1)); return r.length;"
        )),
        3
    );
    assert_eq!(
        num(&ev(
            &mut c,
            "var c = StreamController<int>(); var r = <int>[]; c.stream.listen((x) => r.add(x)); await Stream.fromIterable([1, 2, 3]).pipe(c.sink); await Future.delayed(Duration(milliseconds: 1)); return r[0] + r[1] + r[2];"
        )),
        6
    );
}

#[test]
fn stream_pipe_completes_when_sink_closed() {
    let mut c = Context::new();
    assert_eq!(
        num(&ev(
            &mut c,
            "var c = StreamController<int>(); var r = <int>[]; c.stream.listen((x) => r.add(x)); await Stream.fromIterable([1, 2, 3]).pipe(c.sink); return c.isClosed ? 1 : 0;"
        )),
        1
    );
}

#[test]
fn stream_pipe_forwards_errors() {
    let mut c = Context::new();
    assert_eq!(
        num(&ev(
            &mut c,
            "var c = StreamController<int>(); var errs = <String>[]; c.stream.listen((x) {}, onError: (e) => errs.add(e.toString())); await Stream.error('boom').pipe(c.sink); await Future.delayed(Duration(milliseconds: 1)); return errs.length;"
        )),
        1
    );
}

#[test]
fn stream_controller_add_stream_merges_events() {
    let mut c = Context::new();
    assert_eq!(
        num(&ev(
            &mut c,
            "var c = StreamController<int>(); var r = <int>[]; c.stream.listen((x) => r.add(x)); await c.addStream(Stream.fromIterable([1, 2, 3])); c.add(4); c.close(); await Future.delayed(Duration(milliseconds: 1)); return r.length;"
        )),
        4
    );
    assert_eq!(
        num(&ev(
            &mut c,
            "var c = StreamController<int>(); var r = <int>[]; c.stream.listen((x) => r.add(x)); await c.addStream(Stream.fromIterable([1, 2, 3])); c.add(4); c.close(); await Future.delayed(Duration(milliseconds: 1)); return r[3];"
        )),
        4
    );
}

#[test]
fn stream_controller_add_stream_errors_forwarded() {
    let mut c = Context::new();
    assert_eq!(
        num(&ev(
            &mut c,
            "var c = StreamController<int>(); var errs = <int>[]; c.stream.listen((x) {}, onError: (e) => errs.add(1)); await c.addStream(Stream.error('x')); await Future.delayed(Duration(milliseconds: 1)); return errs.length;"
        )),
        1
    );
}

#[test]
fn stream_controller_add_stream_cancel_on_error() {
    let mut c = Context::new();
    assert_eq!(
        num(&ev(
            &mut c,
            "var c = StreamController<int>(); var got = <int>[]; c.stream.listen((x) => got.add(x)); await c.addStream(Stream.fromIterable([1, 2, 3]), cancelOnError: true); await Future.delayed(Duration(milliseconds: 1)); return got.length;"
        )),
        3
    );
}

#[test]
fn stream_transform_utf8_decoder_decodes_chunks() {
    let mut c = Context::new();
    assert_eq!(
        str(&ev_imports(
            &mut c,
            "import 'dart:async';\nimport 'dart:convert';",
            "final c = StreamController<List<int>>(); final out = <String>[]; c.stream.transform(utf8.decoder).listen(out.add); c.add([104, 105]); c.add([32, 195, 170]); await c.close(); return out.join('|');"
        )),
        "hi| ê"
    );
}

#[test]
fn stream_transform_utf8_decoder_empty_stream() {
    let mut c = Context::new();
    assert_eq!(
        str(&ev_imports(
            &mut c,
            "import 'dart:async';\nimport 'dart:convert';",
            "final c = StreamController<List<int>>(); final out = <String>[]; c.stream.transform(utf8.decoder).listen(out.add); await c.close(); return out.join('|');"
        )),
        ""
    );
}
