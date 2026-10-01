use d4rt_rs::Context;
use d4rt_rs::value::Value;

fn ev(c: &mut Context, body: &str, ret: &str) -> Value {
    let src = format!("import 'dart:async';\nvoid main() {{ {body} return {ret}; }}");
    c.eval(&src)
        .unwrap_or_else(|e| panic!("{body} {ret}: {:?}", e.message()))
}

fn num(v: &Value) -> i64 {
    match v {
        Value::Int(i) => *i,
        other => panic!("expected int, got {other:?}"),
    }
}

fn r#async(c: &mut Context, body: &str, ret: &str) -> Value {
    let src = format!("import 'dart:async';\nFuture<int> main() async {{ {body} return {ret}; }}");
    c.eval(&src)
        .unwrap_or_else(|e| panic!("{body} {ret}: {:?}", e.message()))
}

#[test]
fn duration_construction_and_getters() {
    let mut c = Context::new();
    assert_eq!(
        num(&ev(&mut c, "", "Duration(milliseconds: 10).inMilliseconds")),
        10
    );
    assert_eq!(
        num(&ev(&mut c, "", "Duration(seconds: 1).inMilliseconds")),
        1000
    );
    assert_eq!(num(&ev(&mut c, "", "Duration(minutes: 2).inSeconds")), 120);
    assert_eq!(num(&ev(&mut c, "", "Duration(days: 1).inHours")), 24);
    assert_eq!(
        num(&ev(
            &mut c,
            "",
            "Duration(microseconds: 500).inMicroseconds"
        )),
        500
    );
    assert_eq!(
        num(&ev(&mut c, "", "Duration(milliseconds: 1500).inSeconds")),
        1
    );
    assert_eq!(
        num(&ev(
            &mut c,
            "",
            "(Duration(milliseconds: 5) + Duration(milliseconds: 3)).inMilliseconds"
        )),
        8
    );
    assert_eq!(
        num(&ev(
            &mut c,
            "",
            "(Duration(milliseconds: 5) - Duration(milliseconds: 3)).inMilliseconds"
        )),
        2
    );
    assert_eq!(
        num(&ev(
            &mut c,
            "",
            "Duration(milliseconds: 5) > Duration(milliseconds: 3) ? 1 : 0"
        )),
        1
    );
}

#[test]
fn await_future_value() {
    let mut c = Context::new();
    assert_eq!(
        num(&r#async(
            &mut c,
            "int result = await Future.value(42);",
            "result"
        )),
        42
    );
}

#[test]
fn await_future_delayed() {
    let mut c = Context::new();
    assert!(matches!(
        r#async(
            &mut c,
            "var result = await Future.delayed(Duration(milliseconds: 10));",
            "result"
        ),
        Value::Null
    ));
    assert_eq!(
        num(&r#async(
            &mut c,
            "int result = await Future.delayed(Duration(milliseconds: 10), () => 7);",
            "result"
        )),
        7
    );
}

#[test]
fn multiple_sequential_awaits() {
    let mut c = Context::new();
    assert_eq!(
        num(&r#async(
            &mut c,
            "int a = await Future.value(1); int b = await Future.delayed(Duration(milliseconds: 5), () => 2); int c = a + b;",
            "c"
        )),
        3
    );
}

#[test]
fn await_expression_in_argument() {
    let mut c = Context::new();
    assert_eq!(
        num(&r#async(
            &mut c,
            "int add(int x, int y) => x + y; int r = add(await Future.value(2), await Future.value(3));",
            "r"
        )),
        5
    );
}

#[test]
fn await_result_of_async_function() {
    let mut c = Context::new();
    assert_eq!(
        num(&r#async(
            &mut c,
            "Future<int> inner() async { int x = await Future.delayed(Duration(milliseconds: 5), () => 21); return x * 2; } int r = await inner();",
            "r"
        )),
        42
    );
}

#[test]
fn async_error_handling_try_catch() {
    let mut c = Context::new();
    assert_eq!(
        num(&r#async(
            &mut c,
            "int r = 0; try { await Future.error('boom'); } catch (e) { r = 1; }",
            "r"
        )),
        1
    );
}

#[test]
fn async_error_propagates_out() {
    let mut c = Context::new();
    let src =
        "import 'dart:async';\nFuture<int> main() async { await Future.error('boom'); return 1; }";
    assert!(matches!(
        c.eval(src),
        Err(d4rt_rs::error::InterpError::Throw(_))
    ));
}

#[test]
fn uncaught_throw_in_async_function() {
    let mut c = Context::new();
    let src = "import 'dart:async';\nFuture<int> main() async { throw 'nope'; }";
    assert!(matches!(
        c.eval(src),
        Err(d4rt_rs::error::InterpError::Throw(_))
    ));
}

#[test]
fn then_chain() {
    let mut c = Context::new();
    assert_eq!(
        num(&r#async(
            &mut c,
            "int r = await Future.value(2).then((v) => v * 3);",
            "r"
        )),
        6
    );
    assert_eq!(
        num(&r#async(
            &mut c,
            "int r = await Future.value(2).then((v) => v * 3).then((v) => v + 1);",
            "r"
        )),
        7
    );
}

#[test]
fn then_on_delayed_future() {
    let mut c = Context::new();
    assert_eq!(
        num(&r#async(
            &mut c,
            "int r = await Future.delayed(Duration(milliseconds: 5), () => 10).then((v) => v * 2);",
            "r"
        )),
        20
    );
}

#[test]
fn catch_error_on_delayed_future() {
    let mut c = Context::new();
    assert_eq!(
        num(&r#async(
            &mut c,
            "int r = await Future.delayed(Duration(milliseconds: 5)).then((_) => 99).catchError((e) => 5);",
            "r"
        )),
        99
    );
    assert_eq!(
        num(&r#async(
            &mut c,
            "int r = await Future.error('x').catchError((e) => 5);",
            "r"
        )),
        5
    );
}

#[test]
fn when_complete_runs() {
    let mut c = Context::new();
    assert_eq!(
        num(&r#async(
            &mut c,
            "int ran = 0; int r = await Future.delayed(Duration(milliseconds: 5), () => 1).whenComplete(() { ran = 1; });",
            "ran"
        )),
        1
    );
}

#[test]
fn completer_complete() {
    let mut c = Context::new();
    assert_eq!(
        num(&r#async(
            &mut c,
            "var c = Completer(); c.complete(11); int r = await c.future;",
            "r"
        )),
        11
    );
}

#[test]
fn completer_complete_error() {
    let mut c = Context::new();
    assert_eq!(
        num(&r#async(
            &mut c,
            "int r = 0; var c = Completer(); c.completeError('bad'); try { await c.future; } catch (e) { r = 2; }",
            "r"
        )),
        2
    );
}

#[test]
fn timer_fires() {
    let mut c = Context::new();
    assert_eq!(
        num(&r#async(
            &mut c,
            "int ran = 0; Timer(Duration(milliseconds: 5), () { ran = 1; }); while (ran == 0) { await Future.delayed(Duration(milliseconds: 1)); }",
            "ran"
        )),
        1
    );
}

#[test]
fn loop_with_await_in_condition() {
    let mut c = Context::new();
    assert_eq!(
        num(&r#async(
            &mut c,
            "int i = 0; while (await Future.value(i < 3)) { i = i + 1; }",
            "i"
        )),
        3
    );
}

#[test]
fn await_in_loop_body() {
    let mut c = Context::new();
    assert_eq!(
        num(&r#async(
            &mut c,
            "int total = 0; for (int i = 0; i < 3; i++) { total = total + await Future.value(i); }",
            "total"
        )),
        3
    );
}

#[test]
fn future_microtask() {
    let mut c = Context::new();
    assert_eq!(
        num(&r#async(
            &mut c,
            "int r = await Future.microtask(() => 9);",
            "r"
        )),
        9
    );
}

#[test]
fn future_sync() {
    let mut c = Context::new();
    assert_eq!(
        num(&r#async(&mut c, "int r = await Future.sync(() => 4);", "r")),
        4
    );
}

#[test]
fn chained_async_functions() {
    let mut c = Context::new();
    assert_eq!(
        num(&r#async(&mut c,
            "Future<int> f1() async { return await Future.delayed(Duration(milliseconds: 5), () => 6); }
             Future<int> f2() async { int a = await f1(); return a + 1; }
             int r = await f2();",
            "r")),
        7
    );
}

#[test]
fn future_any_first_value_wins() {
    let mut c = Context::new();
    assert_eq!(
        num(&r#async(
            &mut c,
            "int r = await Future.any([Future.delayed(Duration(milliseconds: 20), () => 1), Future.value(2)]);",
            "r"
        )),
        2
    );
    assert_eq!(
        num(&r#async(
            &mut c,
            "int r = await Future.any([Future.delayed(Duration(milliseconds: 2), () => 3), Future.delayed(Duration(milliseconds: 20), () => 4)]);",
            "r"
        )),
        3
    );
}

#[test]
fn future_any_all_error_last_wins() {
    let mut c = Context::new();
    assert_eq!(
        num(&r#async(
            &mut c,
            "int r = 0; try { await Future.any([Future.error('a'), Future.error('b')]); } catch (e) { r = e == 'b' ? 1 : 0; }",
            "r"
        )),
        1
    );
}

#[test]
fn future_any_empty_errors() {
    let mut c = Context::new();
    assert_eq!(
        num(&r#async(
            &mut c,
            "int r = 0; try { await Future.any([]); } catch (e) { r = 1; }",
            "r"
        )),
        1
    );
}

#[test]
fn future_for_each_awaits_each() {
    let mut c = Context::new();
    assert_eq!(
        num(&r#async(
            &mut c,
            "int total = 0; await Future.forEach([1, 2, 3], (x) async { await Future.delayed(Duration(milliseconds: 1)); total += x; });",
            "total"
        )),
        6
    );
}

#[test]
fn future_for_each_empty() {
    let mut c = Context::new();
    assert_eq!(
        num(&r#async(
            &mut c,
            "int ran = 0; await Future.forEach([], (_) { ran = 1; });",
            "ran"
        )),
        0
    );
}

#[test]
fn future_do_while_runs_until_false() {
    let mut c = Context::new();
    assert_eq!(
        num(&r#async(
            &mut c,
            "int i = 0; await Future.doWhile(() { i++; return Future.value(i < 4); });",
            "i"
        )),
        4
    );
}

#[test]
fn future_do_while_error_propagates() {
    let mut c = Context::new();
    assert_eq!(
        num(&r#async(
            &mut c,
            "int r = 0; try { await Future.doWhile(() { throw 'stop'; }); } catch (e) { r = 1; }",
            "r"
        )),
        1
    );
}

#[test]
fn timer_periodic_fires_multiple() {
    let mut c = Context::new();
    assert_eq!(
        num(&r#async(
            &mut c,
            "int count = 0; Timer.periodic(Duration(milliseconds: 1), (t) { count++; if (count >= 3) { t.cancel(); } }); while (count < 3) { await Future.delayed(Duration(milliseconds: 1)); }",
            "count"
        )),
        3
    );
}

#[test]
fn timer_periodic_cancel_stops() {
    let mut c = Context::new();
    assert_eq!(
        num(&r#async(
            &mut c,
            "int count = 0; var t = Timer.periodic(Duration(milliseconds: 1), (_) { count++; }); t.cancel(); await Future.delayed(Duration(milliseconds: 10));",
            "count"
        )),
        0
    );
}

#[test]
fn future_timeout_fires_on_uncompleted() {
    let mut c = Context::new();
    assert_eq!(
        num(&r#async(
            &mut c,
            "final c = Completer<int>(); try { await c.future.timeout(Duration(milliseconds: 5)); return -1; } catch (e) { return e.message == 'Future not completed within the time limit' ? 1 : 0; }",
            ""
        )),
        1
    );
}

#[test]
fn future_timeout_with_on_timeout_action() {
    let mut c = Context::new();
    assert_eq!(
        num(&r#async(
            &mut c,
            "final c = Completer<int>(); int r = await c.future.timeout(Duration(milliseconds: 5), onTimeout: () => 99);",
            "r"
        )),
        99
    );
}

#[test]
fn future_timeout_no_fire_when_source_wins() {
    let mut c = Context::new();
    assert_eq!(
        num(&r#async(
            &mut c,
            "int r = await Future.delayed(Duration(milliseconds: 5), () => 42).timeout(Duration(milliseconds: 100));",
            "r"
        )),
        42
    );
}

#[test]
fn future_timeout_does_not_timeout_completed_future() {
    let mut c = Context::new();
    assert_eq!(
        num(&r#async(
            &mut c,
            "int r = await Future.value(7).timeout(Duration(milliseconds: 5), onTimeout: () => -1);",
            "r"
        )),
        7
    );
}

#[test]
fn future_as_stream_emits_value() {
    let mut c = Context::new();
    assert_eq!(
        num(&r#async(
            &mut c,
            "var list = await Future.value(7).asStream().toList();",
            "list.length"
        )),
        1
    );
    assert_eq!(
        num(&r#async(
            &mut c,
            "var list = await Future.value(7).asStream().toList();",
            "list[0]"
        )),
        7
    );
}

#[test]
fn future_as_stream_emits_error() {
    let mut c = Context::new();
    assert_eq!(
        num(&r#async(
            &mut c,
            "try { await Future.error('boom').asStream().toList(); return -1; } catch (e) { return e == 'boom' ? 1 : 0; }",
            ""
        )),
        1
    );
}

#[test]
fn future_on_error_handles() {
    let mut c = Context::new();
    assert_eq!(
        num(&r#async(
            &mut c,
            "int r = await Future.error('boom').onError((e) => 5);",
            "r"
        )),
        5
    );
}

#[test]
fn future_on_error_passes_errors_through() {
    let mut c = Context::new();
    assert_eq!(
        num(&r#async(
            &mut c,
            "try { int r = await Future.value(3).onError((e) => 5); if (r != 3) return -1; } catch (e) { return -2; } return 1;",
            ""
        )),
        1
    );
}

#[test]
fn unawaited_ignores_future() {
    let mut c = Context::new();
    assert_eq!(
        num(&ev(
            &mut c,
            "unawaited(Future.value(1)); unawaited(Future.error('x')); return 3;",
            ""
        )),
        3
    );
}

#[test]
fn timer_is_active_and_cancel() {
    let mut c = Context::new();
    assert_eq!(
        num(&ev(
            &mut c,
            "var t = Timer(Duration(milliseconds: 1), () {}); bool a = t.isActive; t.cancel(); bool b = t.isActive; return (a && !b) ? 1 : 0;",
            ""
        )),
        1
    );
}

#[test]
fn timer_is_active_false_after_fire() {
    let mut c = Context::new();
    assert_eq!(
        num(&r#async(
            &mut c,
            "bool ran = false; var t = Timer(Duration(milliseconds: 1), () { ran = true; }); while (ran == false) { await Future.delayed(Duration(milliseconds: 1)); } return t.isActive ? 0 : 1;",
            ""
        )),
        1
    );
}

#[test]
fn timer_canceled_does_not_fire() {
    let mut c = Context::new();
    assert_eq!(
        num(&r#async(
            &mut c,
            "int count = 0; var t = Timer(Duration(milliseconds: 1), () { count++; }); t.cancel(); await Future.delayed(Duration(milliseconds: 5));",
            "count"
        )),
        0
    );
}

#[test]
fn timer_periodic_tick_counts() {
    let mut c = Context::new();
    assert_eq!(
        num(&r#async(
            &mut c,
            "int ticks = 0; var t = Timer.periodic(Duration(milliseconds: 1), (x) { ticks = x.tick; if (x.tick >= 2) { x.cancel(); } }); while (ticks < 2) { await Future.delayed(Duration(milliseconds: 1)); }",
            "ticks"
        )),
        2
    );
}

#[test]
fn completer_is_completed() {
    let mut c = Context::new();
    assert_eq!(
        num(&ev(
            &mut c,
            "var c = Completer<int>(); bool a = c.isCompleted; c.complete(1); bool b = c.isCompleted; return (a == false && b == true) ? 1 : 0;",
            ""
        )),
        1
    );
}

#[test]
fn completer_sync_works() {
    let mut c = Context::new();
    assert_eq!(
        num(&r#async(
            &mut c,
            "var c = Completer<int>.sync(); c.complete(11);",
            "await c.future"
        )),
        11
    );
}

#[test]
fn future_constructor_runs_computation() {
    let mut c = Context::new();
    assert_eq!(
        num(&r#async(&mut c, "int r = await Future(() => 42);", "r")),
        42
    );
}

#[test]
fn stream_iterator_ctor() {
    let mut c = Context::new();
    let v = r#async(
        &mut c,
        "final ctrl = StreamController<int>();
         ctrl.add(1);
         ctrl.add(2);
         ctrl.close();
         final it = StreamIterator(ctrl.stream);
         final a = await it.moveNext();
         final va = it.current;
         final b = await it.moveNext();
         final vb = it.current;
         final d = await it.moveNext();
         '$a:$va|$b:$vb|$d';",
        "'$a:$va|$b:$vb|$d'",
    );
    assert_eq!(str(&v), "true:1|true:2|false");
}

#[test]
fn stream_transformer_cast_from() {
    let mut c = Context::new();
    let v = r#async(
        &mut c,
        "final t = StreamTransformer<int, int>.fromHandlers(
             handleData: (data, sink) { sink.add(data * 2); });
         final cast = StreamTransformer.castFrom(t);
         final out = await Stream.fromIterable([1, 2, 3]).transform(cast).toList();
         out.join(',');",
        "out.join(',')",
    );
    assert_eq!(str(&v), "2,4,6");
}

fn str(v: &Value) -> String {
    match v {
        Value::Str(s) => s.to_string(),
        other => panic!("expected string, got {other:?}"),
    }
}
