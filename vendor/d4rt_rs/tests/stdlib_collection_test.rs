//! dart:core List, Map, Set coverage.

use d4rt_rs::{Context, Value};

fn expr(src: &str) -> Value {
    Context::new().eval_expression(src).unwrap()
}

fn run(src: &str) -> Value {
    Context::new().eval(src).unwrap()
}

// ---------------------------------------------------------------------------
// List basics
// ---------------------------------------------------------------------------

#[test]
fn list_basics() {
    assert_eq!(expr("[1, 2, 3].length"), Value::Int(3));
    assert_eq!(expr("[].length"), Value::Int(0));
    assert_eq!(expr("['a'].isEmpty"), Value::Bool(false));
    assert_eq!(expr("[].isEmpty"), Value::Bool(true));
    assert_eq!(expr("['a'].isNotEmpty"), Value::Bool(true));
    assert_eq!(expr("[1, 2, 3].first"), Value::Int(1));
    assert_eq!(expr("[1, 2, 3].last"), Value::Int(3));
    assert_eq!(expr("[7].single"), Value::Int(7));
    assert_eq!(expr("[1, 2, 3].elementAt(2)"), Value::Int(3));
    assert_eq!(expr("[1, 2, 3].reversed.toList()[0]"), Value::Int(3));
    assert_eq!(expr("[1, 2, 3].reversed.toList().length"), Value::Int(3));
}

#[test]
fn list_index_and_assign() {
    assert_eq!(
        run("void main() { var l = [1, 2, 3]; return l[1]; }"),
        Value::Int(2)
    );
    assert_eq!(
        run("void main() { var l = [1, 2, 3]; l[0] = 9; return l[0]; }"),
        Value::Int(9)
    );
    assert_eq!(
        run("void main() { var l = [1]; l[0] = 2; return l[0]; }"),
        Value::Int(2)
    );
    assert_eq!(
        run("void main() { var l = [1, 2, 3]; l.length = 5; return l.length; }"),
        Value::Int(5)
    );
}

#[test]
fn list_mutation() {
    assert_eq!(
        run("void main() { var l = []; l.add(1); l.add(2); return l.length; }"),
        Value::Int(2)
    );
    assert_eq!(
        run("void main() { var l = [1]; l.addAll([2, 3]); return l.join(','); }"),
        Value::str("1,2,3")
    );
    assert_eq!(
        run("void main() { var l = [1, 3]; l.insert(1, 2); return l.join(','); }"),
        Value::str("1,2,3")
    );
    assert_eq!(
        run("void main() { var l = [1, 4]; l.insertAll(1, [2, 3]); return l.join(','); }"),
        Value::str("1,2,3,4")
    );
    assert_eq!(
        run("void main() { var l = [1, 2, 3]; l.remove(2); return l.join(','); }"),
        Value::str("1,3")
    );
    assert_eq!(
        run("void main() { var l = [1, 2, 3]; l.removeAt(1); return l.join(','); }"),
        Value::str("1,3")
    );
    assert_eq!(
        run("void main() { var l = [1, 2, 3]; return l.removeLast(); }"),
        Value::Int(3)
    );
    assert_eq!(
        run("void main() { var l = [1, 2, 3]; l.removeRange(1, 3); return l.join(','); }"),
        Value::str("1")
    );
    assert_eq!(
        run(
            "void main() { var l = [1, 2, 3]; l.removeWhere((x) => x.isEven); return l.join(','); }"
        ),
        Value::str("1,3")
    );
    assert_eq!(
        run(
            "void main() { var l = [1, 2, 3, 4]; l.retainWhere((x) => x > 2); return l.join(','); }"
        ),
        Value::str("3,4")
    );
    assert_eq!(
        run("void main() { var l = [1, 2]; l.clear(); return l.length; }"),
        Value::Int(0)
    );
    assert_eq!(
        run("void main() { var l = [1, 2, 3, 4]; l.fillRange(1, 3, 0); return l.join(','); }"),
        Value::str("1,0,0,4")
    );
}

#[test]
fn list_search() {
    assert_eq!(expr("[1, 2, 3].contains(2)"), Value::Bool(true));
    assert_eq!(expr("[1, 2, 3].contains(9)"), Value::Bool(false));
    assert_eq!(expr("['a', 'b'].contains('a')"), Value::Bool(true));
    assert_eq!(expr("[1, 2, 1].indexOf(1)"), Value::Int(0));
    assert_eq!(expr("[1, 2, 1].indexOf(1, 1)"), Value::Int(2));
    assert_eq!(expr("[1, 2, 1].indexOf(9)"), Value::Int(-1));
    assert_eq!(expr("[1, 2, 1].lastIndexOf(1)"), Value::Int(2));
    assert_eq!(expr("[1, 2, 3].indexWhere((x) => x > 1)"), Value::Int(1));
    assert_eq!(
        expr("[1, 2, 3].lastIndexWhere((x) => x > 1)"),
        Value::Int(2)
    );
    assert_eq!(expr("[1, 2, 3].firstWhere((x) => x > 1)"), Value::Int(2));
    assert_eq!(expr("[1, 2, 3].lastWhere((x) => x > 1)"), Value::Int(3));
    assert_eq!(expr("[7].singleWhere((x) => x == 7)"), Value::Int(7));
}

// ---------------------------------------------------------------------------
// List transformation
// ---------------------------------------------------------------------------

#[test]
fn list_transform() {
    assert_eq!(
        expr("[1, 2, 3].map((x) => x * 2).toList().join(',')"),
        Value::str("2,4,6")
    );
    assert_eq!(
        expr("[1, 2, 3, 4].where((x) => x % 2 == 0).toList().join(',')"),
        Value::str("2,4")
    );
    assert_eq!(
        expr("[1, 2, 3].expand((x) => [x, x]).toList().length"),
        Value::Int(6)
    );
    assert_eq!(
        expr("[1, 2, 3].skip(1).toList().join(',')"),
        Value::str("2,3")
    );
    assert_eq!(
        expr("[1, 2, 3].take(2).toList().join(',')"),
        Value::str("1,2")
    );
    assert_eq!(
        expr("[1, 2, 3, 4].skipWhile((x) => x < 3).toList().join(',')"),
        Value::str("3,4")
    );
    assert_eq!(
        expr("[1, 2, 3, 4].takeWhile((x) => x < 3).toList().join(',')"),
        Value::str("1,2")
    );
    assert_eq!(expr("[1, 2, 3].toSet().length"), Value::Int(3));
    assert_eq!(expr("[1, 1, 2].toSet().length"), Value::Int(2));
    assert_eq!(
        expr("[1, 2].followedBy([3]).toList().length"),
        Value::Int(3)
    );
    assert_eq!(expr("[1, 2, 3].join('-')"), Value::str("1-2-3"));
    assert_eq!(expr("[1, 2, 3].fold(10, (a, b) => a + b)"), Value::Int(16));
    assert_eq!(expr("[1, 2, 3].reduce((a, b) => a + b)"), Value::Int(6));
    assert_eq!(expr("[1, 2, 3].any((x) => x == 2)"), Value::Bool(true));
    assert_eq!(expr("[1, 2, 3].every((x) => x > 0)"), Value::Bool(true));
    assert_eq!(expr("[1, 2, 3].every((x) => x > 1)"), Value::Bool(false));
    assert_eq!(expr("[1, 2, 3].asMap()[1]"), Value::Int(2));
    assert_eq!(expr("[1, 2, 3].cast<num>().length"), Value::Int(3));
    assert_eq!(expr("[1, 2, 3].toList().length"), Value::Int(3));
}

#[test]
fn list_sort_shuffle() {
    assert_eq!(
        run("void main() { var l = [3, 1, 2]; l.sort(); return l.join(','); }"),
        Value::str("1,2,3")
    );
    assert_eq!(
        run("void main() { var l = [3, 1, 2]; l.sort((a, b) => b - a); return l.join(','); }"),
        Value::str("3,2,1")
    );
    assert_eq!(
        run("void main() { var l = ['b', 'a']; l.sort(); return l.join(''); }"),
        Value::str("ab")
    );
    assert_eq!(
        run("void main() { var l = [1, 2, 3]; l.shuffle(); return l.length; }"),
        Value::Int(3)
    );
}

#[test]
fn list_ranges() {
    assert_eq!(
        expr("[0, 1, 2, 3, 4].sublist(1).join(',')"),
        Value::str("1,2,3,4")
    );
    assert_eq!(
        expr("[0, 1, 2, 3, 4].sublist(1, 3).join(',')"),
        Value::str("1,2")
    );
    assert_eq!(
        expr("[0, 1, 2, 3, 4].getRange(1, 3).toList().join(',')"),
        Value::str("1,2")
    );
    assert_eq!(
        run("void main() { var l = [0, 0, 0]; l.setRange(1, 3, [5, 6]); return l.join(','); }"),
        Value::str("0,5,6")
    );
    assert_eq!(
        run(
            "void main() { var a = [1, 2]; var b = [9, 9, 9]; a.setAll(1, b); return a.join(','); }"
        ),
        Value::str("1,9,9,9")
    );
    assert_eq!(
        run("void main() { var l = [1, 2, 3]; l.replaceRange(1, 2, [8]); return l.join(','); }"),
        Value::str("1,8,3")
    );
}

// ---------------------------------------------------------------------------
// List constructors
// ---------------------------------------------------------------------------

#[test]
fn list_constructors() {
    assert_eq!(
        expr("List.generate(5, (i) => i * i).join(',')"),
        Value::str("0,1,4,9,16")
    );
    assert_eq!(expr("List.filled(3, 'x').join(',')"), Value::str("x,x,x"));
    assert_eq!(expr("List.from([1, 2]).join(',')"), Value::str("1,2"));
    assert_eq!(expr("List.of({1, 2}).join(',')"), Value::str("1,2"));
    assert_eq!(expr("List.empty().length"), Value::Int(0));
    assert_eq!(expr("List.unmodifiable([1, 2])[1]"), Value::Int(2));
    assert_eq!(expr("List.identity(2).length"), Value::Int(2));
    assert_eq!(
        run("void main() { var l = [1, 2, 3]; l.length = 2; return l.join(','); }"),
        Value::str("1,2")
    );
}

// ---------------------------------------------------------------------------
// Map
// ---------------------------------------------------------------------------

#[test]
fn map_basics() {
    assert_eq!(expr("{'a': 1}.length"), Value::Int(1));
    assert_eq!(expr("{}.isEmpty"), Value::Bool(true));
    assert_eq!(expr("{'a': 1}.isNotEmpty"), Value::Bool(true));
    assert_eq!(
        expr("{'a': 1, 'b': 2}.keys.toList().join(',')"),
        Value::str("a,b")
    );
    assert_eq!(
        expr("{'a': 1, 'b': 2}.values.toList().join(',')"),
        Value::str("1,2")
    );
    assert_eq!(expr("{'a': 1}.containsKey('a')"), Value::Bool(true));
    assert_eq!(expr("{'a': 1}.containsKey('z')"), Value::Bool(false));
    assert_eq!(expr("{'a': 1}.containsValue(1)"), Value::Bool(true));
    assert_eq!(expr("{'a': 1}.containsValue(9)"), Value::Bool(false));
}

#[test]
fn map_access() {
    assert_eq!(expr("{'a': 1}['a']"), Value::Int(1));
    assert_eq!(expr("{'a': 1}['z']"), Value::Null);
    assert_eq!(
        run("void main() { var m = {'a': 1}; m['b'] = 2; return m['a'] + m['b']; }"),
        Value::Int(3)
    );
    assert_eq!(
        run("void main() { var m = {'a': 1}; m['a'] = 9; return m['a']; }"),
        Value::Int(9)
    );
}

#[test]
fn map_mutation() {
    assert_eq!(
        run("void main() { var m = {'a': 1}; m.putIfAbsent('b', () => 2); return m.length; }"),
        Value::Int(2)
    );
    assert_eq!(
        run("void main() { var m = {'a': 1}; m.putIfAbsent('a', () => 99); return m['a']; }"),
        Value::Int(1)
    );
    assert_eq!(
        run("void main() { var m = {'a': 1}; m.update('a', (v) => v + 10); return m['a']; }"),
        Value::Int(11)
    );
    assert_eq!(
        run(
            "void main() { var m = {'a': 1}; m.update('a', (v) => v + 1, ifAbsent: () => 0); return m['a']; }"
        ),
        Value::Int(2)
    );
    assert_eq!(
        run(
            "void main() { var m = {'a': 1, 'b': 2}; m.updateAll((k, v) => v * 10); return m['a'] + m['b']; }"
        ),
        Value::Int(30)
    );
    assert_eq!(
        run("void main() { var m = {'a': 1, 'b': 2}; m.remove('a'); return m.length; }"),
        Value::Int(1)
    );
    assert_eq!(
        run("void main() { var m = {'a': 1, 'b': 2}; m.remove('zz'); return m.length; }"),
        Value::Int(2)
    );
    assert_eq!(
        run("void main() { var m = {'a': 1}; m.clear(); return m.length; }"),
        Value::Int(0)
    );
    assert_eq!(
        run("void main() { var m = {'a': 1}; m.addAll({'b': 2, 'c': 3}); return m.length; }"),
        Value::Int(3)
    );
    assert_eq!(
        run("void main() { var m = {'a': 1}; m.addEntries({'b': 2}.entries); return m['b']; }"),
        Value::Int(2)
    );
}

#[test]
fn map_iteration() {
    assert_eq!(
        run(
            "void main() { var m = {'a': 1, 'b': 2}; var s = 0; m.forEach((k, v) { s += v; }); return s; }"
        ),
        Value::Int(3)
    );
    assert_eq!(
        run(
            "void main() { var m = {'a': 1, 'b': 2}; return m.entries.map((e) => '${e.key}${e.value}').toList().join(''); }"
        ),
        Value::str("a1b2")
    );
    assert_eq!(
        run(
            "void main() { var m = {'a': 1, 'b': 2}; return m.keys.where((k) => k == 'a').length; }"
        ),
        Value::Int(1)
    );
    assert_eq!(
        run("void main() { var m = {'a': 1, 'b': 2}; return m.entries.length; }"),
        Value::Int(2)
    );
    assert_eq!(
        run("void main() { var m = {'a': 1, 'b': 2}; return m.entries.first.key; }"),
        Value::str("a")
    );
    assert_eq!(
        run("void main() { var m = {'a': 1, 'b': 2}; return m.entries.first.value; }"),
        Value::Int(1)
    );
}

#[test]
fn map_constructors() {
    assert_eq!(expr("Map.from({'a': 1})['a']"), Value::Int(1));
    assert_eq!(expr("Map.of({'a': 1}).length"), Value::Int(1));
    assert_eq!(expr("Map.identity().isEmpty"), Value::Bool(true));
    assert_eq!(expr("Map.unmodifiable({'a': 1})['a']"), Value::Int(1));
    assert_eq!(
        expr("Map.fromEntries({'a': 1}.entries)['a']"),
        Value::Int(1)
    );
    assert_eq!(expr("Map.fromIterable([1, 2])[1]"), Value::Int(1));
    assert_eq!(
        expr("Map.fromIterables(['a', 'b'], [1, 2])['b']"),
        Value::Int(2)
    );
    assert_eq!(
        expr("Map.fromIterables(['a', 'b'], [1, 2]).length"),
        Value::Int(2)
    );
}

// ---------------------------------------------------------------------------
// Set
// ---------------------------------------------------------------------------

#[test]
fn set_basics() {
    assert_eq!(expr("{1, 2, 3}.length"), Value::Int(3));
    assert_eq!(expr("{1, 2, 2, 3}.length"), Value::Int(3));
    assert_eq!(expr("{1, 2, 3}.contains(2)"), Value::Bool(true));
    assert_eq!(expr("{1, 2, 3}.contains(9)"), Value::Bool(false));
    assert_eq!(expr("{1}.isEmpty"), Value::Bool(false));
    assert_eq!(expr("Set().isEmpty"), Value::Bool(true));
}

#[test]
fn set_mutation() {
    assert_eq!(
        run("void main() { var s = {1}; s.add(2); return s.length; }"),
        Value::Int(2)
    );
    assert_eq!(
        run("void main() { var s = {1}; s.add(1); return s.length; }"),
        Value::Int(1)
    );
    assert_eq!(
        run("void main() { var s = {1, 2}; s.addAll([2, 3]); return s.length; }"),
        Value::Int(3)
    );
    assert_eq!(
        run("void main() { var s = {1, 2, 3}; s.remove(2); return s.length; }"),
        Value::Int(2)
    );
    assert_eq!(
        run("void main() { var s = {1, 2, 3}; s.remove(9); return s.length; }"),
        Value::Int(3)
    );
    assert_eq!(
        run("void main() { var s = {1, 2, 3}; s.removeWhere((x) => x > 1); return s.length; }"),
        Value::Int(1)
    );
    assert_eq!(
        run(
            "void main() { var s = {1, 2, 3, 4}; s.retainWhere((x) => x.isEven); return s.length; }"
        ),
        Value::Int(2)
    );
    assert_eq!(
        run("void main() { var s = {1, 2}; s.clear(); return s.length; }"),
        Value::Int(0)
    );
}

#[test]
fn set_iteration() {
    assert_eq!(
        expr("{1, 2, 3}.map((x) => x * 2).toList().join(',')"),
        Value::str("2,4,6")
    );
    assert_eq!(
        expr("{1, 2, 3, 4}.where((x) => x % 2 == 0).length"),
        Value::Int(2)
    );
    assert_eq!(expr("{1, 2, 3}.any((x) => x == 3)"), Value::Bool(true));
    assert_eq!(expr("{1, 2, 3}.every((x) => x > 0)"), Value::Bool(true));
    assert_eq!(expr("{1, 2, 3}.fold(0, (a, b) => a + b)"), Value::Int(6));
    assert_eq!(expr("{1, 2, 3}.first"), Value::Int(1));
    assert_eq!(expr("{1, 2, 3}.toList().length"), Value::Int(3));
    assert_eq!(expr("{1, 2, 3}.join('-')"), Value::str("1-2-3"));
    assert_eq!(expr("{1, 2, 3}.contains(2)"), Value::Bool(true));
}

#[test]
fn set_constructors() {
    assert_eq!(expr("Set.from([1, 2, 2]).length"), Value::Int(2));
    assert_eq!(expr("Set.of([1, 2]).length"), Value::Int(2));
    assert_eq!(expr("Set.identity().isEmpty"), Value::Bool(true));
    assert_eq!(expr("Set.unmodifiable([1]).length"), Value::Int(1));
    assert_eq!(
        expr("Set.from([1, 2]).difference({1}).length"),
        Value::Int(1)
    );
}

#[test]
fn iterable_shared() {
    assert_eq!(
        expr("[1, 2, 3].expand((x) => [x, -x]).toList().length"),
        Value::Int(6)
    );
    assert_eq!(
        expr("[1, 2, 3].followedBy([4, 5]).toList().length"),
        Value::Int(5)
    );
    assert_eq!(expr("[1, 2, 3].take(0).length"), Value::Int(0));
    assert_eq!(expr("[1, 2, 3].skip(3).length"), Value::Int(0));
    assert_eq!(expr("[1, 2, 3].cast<num>().toList().length"), Value::Int(3));
}
