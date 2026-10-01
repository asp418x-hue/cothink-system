use d4rt_rs::Context;
use d4rt_rs::value::Value;

fn nums(v: &Value) -> Vec<i64> {
    match v {
        Value::List(l) => l
            .borrow()
            .iter()
            .map(|x| match x {
                Value::Int(i) => *i,
                other => panic!("expected int, got {other:?}"),
            })
            .collect(),
        other => panic!("expected list, got {other:?}"),
    }
}

fn strs(v: &Value) -> Vec<String> {
    match v {
        Value::List(l) => l
            .borrow()
            .iter()
            .map(|x| match x {
                Value::Str(s) => s.to_string(),
                other => panic!("expected string, got {other:?}"),
            })
            .collect(),
        other => panic!("expected list, got {other:?}"),
    }
}

#[test]
fn async_star_basic_yield() {
    let mut c = Context::new();
    let src = r#"
import 'dart:async';
Stream<int> countAsync() async* {
  yield 1;
  yield 2;
  yield 3;
}
Future<dynamic> main() async {
  List<int> results = [];
  await for (var value in countAsync()) {
    results.add(value);
  }
  return results;
}
"#;
    let v = c.eval(src).unwrap();
    assert_eq!(nums(&v), vec![1, 2, 3]);
}

#[test]
fn async_star_with_await() {
    let mut c = Context::new();
    let src = r#"
import 'dart:async';
Stream<int> doubleGen() async* {
  for (int i = 1; i <= 3; i++) {
    await Future<void>.delayed(Duration(milliseconds: 1));
    yield i * 2;
  }
}
Future<dynamic> main() async {
  List<int> results = [];
  await for (var value in doubleGen()) {
    results.add(value);
  }
  return results;
}
"#;
    let v = c.eval(src).unwrap();
    assert_eq!(nums(&v), vec![2, 4, 6]);
}

#[test]
fn async_star_yield_star() {
    let mut c = Context::new();
    let src = r#"
import 'dart:async';
Stream<int> countAsync() async* {
  yield 1;
  yield 2;
}
Stream<int> withYieldStar() async* {
  yield 0;
  yield* countAsync();
  yield 4;
}
Future<dynamic> main() async {
  List<int> results = [];
  await for (var value in withYieldStar()) {
    results.add(value);
  }
  return results;
}
"#;
    let v = c.eval(src).unwrap();
    assert_eq!(nums(&v), vec![0, 1, 2, 4]);
}

#[test]
fn async_star_stream_machinery() {
    let mut c = Context::new();
    let src = r#"
import 'dart:async';
Stream<int> countAsync() async* {
  yield 1;
  yield 2;
  yield 3;
}
Future<dynamic> main() async {
  return await countAsync().map((x) => x * 10).toList();
}
"#;
    let v = c.eval(src).unwrap();
    assert_eq!(nums(&v), vec![10, 20, 30]);
}

#[test]
fn async_star_early_return() {
    let mut c = Context::new();
    let src = r#"
import 'dart:async';
Stream<int> gen(bool go) async* {
  yield 1;
  if (!go) return;
  yield 2;
}
Future<dynamic> main() async {
  List<int> a = [];
  await for (var v in gen(true)) { a.add(v); }
  List<int> b = [];
  await for (var v in gen(false)) { b.add(v); }
  return [a, b];
}
"#;
    let v = c.eval(src).unwrap();
    let l = match &v {
        Value::List(l) => l,
        other => panic!("expected list, got {other:?}"),
    };
    let l = l.borrow();
    let a = nums(&l[0]);
    let b = nums(&l[1]);
    assert_eq!(a, vec![1, 2]);
    assert_eq!(b, vec![1]);
}

#[test]
fn async_star_error_propagation() {
    let mut c = Context::new();
    let src = r#"
import 'dart:async';
Stream<int> gen() async* {
  yield 1;
  throw Exception('Boom');
}
Future<dynamic> main() async {
  List<dynamic> events = [];
  try {
    await for (var v in gen()) { events.add('$v'); }
    return 'no-error';
  } catch (e) {
    events.add('caught:${e.toString().contains('Boom')}');
    return events;
  }
}
"#;
    let v = c.eval(src).unwrap();
    assert_eq!(strs(&v), vec!["1", "caught:true"]);
}

#[test]
fn async_star_yield_star_error() {
    let mut c = Context::new();
    let src = r#"
import 'dart:async';
Stream<int> bad() async* {
  yield 1;
  throw Exception('Inner');
}
Stream<int> outer() async* {
  yield 0;
  try {
    yield* bad();
  } catch (e) {
    yield 9;
  }
  yield 5;
}
Future<dynamic> main() async {
  List<int> results = [];
  await for (var v in outer()) { results.add(v); }
  return results;
}
"#;
    let v = c.eval(src).unwrap();
    assert_eq!(nums(&v), vec![0, 1, 9, 5]);
}

#[test]
fn async_star_break_await_for() {
    let mut c = Context::new();
    let src = r#"
import 'dart:async';
Stream<int> gen() async* {
  for (int i = 1; i <= 10; i++) {
    yield i;
  }
}
Future<dynamic> main() async {
  List<int> results = [];
  await for (var v in gen()) {
    if (v == 3) break;
    results.add(v);
  }
  return results;
}
"#;
    let v = c.eval(src).unwrap();
    assert_eq!(nums(&v), vec![1, 2]);
}

#[test]
fn async_star_continue_await_for() {
    let mut c = Context::new();
    let src = r#"
import 'dart:async';
Stream<int> gen() async* {
  for (int i = 1; i <= 5; i++) {
    yield i;
  }
}
Future<dynamic> main() async {
  List<int> results = [];
  await for (var v in gen()) {
    if (v == 2) continue;
    results.add(v);
  }
  return results;
}
"#;
    let v = c.eval(src).unwrap();
    assert_eq!(nums(&v), vec![1, 3, 4, 5]);
}

#[test]
fn async_star_concurrent_wait() {
    let mut c = Context::new();
    let src = r#"
import 'dart:async';
Stream<int> a() async* {
  await Future<void>.delayed(Duration(milliseconds: 2));
  yield 1;
  yield 2;
}
Stream<int> b() async* {
  await Future<void>.delayed(Duration(milliseconds: 1));
  yield 3;
  yield 4;
}
Future<dynamic> main() async {
  List<Future<List<int>>> futures = [a().toList(), b().toList()];
  List<List<int>> results = await Future.wait(futures);
  return results[0] + results[1];
}
"#;
    let v = c.eval(src).unwrap();
    assert_eq!(nums(&v), vec![1, 2, 3, 4]);
}
