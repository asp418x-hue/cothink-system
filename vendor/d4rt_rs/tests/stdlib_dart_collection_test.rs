//! dart:collection coverage: HashMap, LinkedHashMap, SplayTreeMap, HashSet,
//! LinkedHashSet, SplayTreeSet, ListQueue, Queue, UnmodifiableListView.

use d4rt_rs::{Context, Value};

fn ctx() -> Context {
    let mut c = Context::new();
    c.eval("import 'dart:collection'; void main() {}").unwrap();
    c
}

/// Evaluates `body` (statements) and returns the value of `ret`-expression,
/// all inside a generated `main`.
fn ev(c: &mut Context, body: &str, ret: &str) -> Value {
    c.eval(&format!("void main() {{ {body} return {ret}; }}"))
        .unwrap()
}

fn assert_list(v: Value, expected: Vec<i64>) {
    match v {
        Value::List(l) => {
            let items: Vec<i64> = l
                .borrow()
                .iter()
                .map(|x| match x {
                    Value::Int(i) => *i,
                    other => panic!("expected int, got {other:?}"),
                })
                .collect();
            assert_eq!(items, expected);
        }
        other => panic!("expected list, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// Maps
// ---------------------------------------------------------------------------

#[test]
fn hash_map_basics() {
    let mut c = ctx();
    assert_eq!(
        ev(
            &mut c,
            "var m = HashMap(); m['a'] = 1; m['b'] = 2;",
            "m.length"
        ),
        Value::Int(2)
    );
    assert_eq!(
        ev(&mut c, "var m = HashMap(); m['a'] = 1;", "m['a']"),
        Value::Int(1)
    );
    assert_eq!(
        ev(&mut c, "var m = HashMap();", "m.containsKey('x')"),
        Value::Bool(false)
    );
    assert_eq!(
        ev(
            &mut c,
            "var m = HashMap(); m['x'] = 5;",
            "m.containsKey('x')"
        ),
        Value::Bool(true)
    );
    assert_eq!(
        ev(&mut c, "var m = HashMap(); m['x'] = 5;", "m.remove('x')"),
        Value::Int(5)
    );
    assert_eq!(
        ev(
            &mut c,
            "var m = HashMap(); m['x'] = 5; m.remove('x');",
            "m.length"
        ),
        Value::Int(0)
    );
    assert_eq!(
        ev(
            &mut c,
            "var m = HashMap(); m['x'] = 1; m['x'] = 2;",
            "m['x']"
        ),
        Value::Int(2)
    );
}

#[test]
fn hash_map_from_and_iteration() {
    let mut c = ctx();
    assert_eq!(
        ev(
            &mut c,
            "var m = HashMap.from({'a': 1, 'b': 2});",
            "m.length"
        ),
        Value::Int(2)
    );
    assert_eq!(
        ev(&mut c, "var m = HashMap.from({'a': 1, 'b': 2});", "m['b']"),
        Value::Int(2)
    );
    assert_list(
        ev(
            &mut c,
            "var m = HashMap(); m['a'] = 1; m['b'] = 2;",
            "m.values",
        ),
        vec![1, 2],
    );
    assert_eq!(
        ev(
            &mut c,
            "var m = HashMap(); m['a'] = 1;",
            "m.putIfAbsent('a', () => 99)"
        ),
        Value::Int(1)
    );
    assert_eq!(
        ev(
            &mut c,
            "var m = HashMap(); m.putIfAbsent('a', () => 99);",
            "m['a']"
        ),
        Value::Int(99)
    );
    assert_eq!(
        ev(
            &mut c,
            "var m = HashMap(); m['a'] = 1;",
            "m.update('a', (v) => v + 10)"
        ),
        Value::Int(11)
    );
    assert_eq!(
        ev(
            &mut c,
            "var m = HashMap();",
            "m.update('a', (v) => v + 1, ifAbsent: () => 7)"
        ),
        Value::Int(7)
    );
    assert_eq!(
        ev(
            &mut c,
            "var m = HashMap.from({'a': 1, 'b': 2}); m.updateAll((k, v) => v * 2);",
            "m['b']"
        ),
        Value::Int(4)
    );
    assert_eq!(
        ev(
            &mut c,
            "var m = HashMap.from({'a': 1});",
            "m.map((k, v) => v + 1)['a']"
        ),
        Value::Int(2)
    );
    // keys are a Set
    assert_eq!(
        ev(
            &mut c,
            "var m = HashMap.from({'a': 1, 'b': 2});",
            "m.keys.length"
        ),
        Value::Int(2)
    );
    // entries
    assert_eq!(
        ev(
            &mut c,
            "var m = HashMap.from({'a': 1});",
            "m.entries[0].key"
        ),
        Value::str("a")
    );
    assert_eq!(
        ev(
            &mut c,
            "var m = HashMap.from({'a': 1});",
            "m.entries[0].value"
        ),
        Value::Int(1)
    );
    // addAll
    assert_eq!(
        ev(
            &mut c,
            "var m = HashMap(); m.addAll({'a': 1, 'b': 2});",
            "m.length"
        ),
        Value::Int(2)
    );
    // clear
    assert_eq!(
        ev(
            &mut c,
            "var m = HashMap(); m['a'] = 1; m.clear();",
            "m.length"
        ),
        Value::Int(0)
    );
}

#[test]
fn linked_and_splay_maps() {
    let mut c = ctx();
    assert_eq!(
        ev(&mut c, "var m = LinkedHashMap(); m['a'] = 1;", "m['a']"),
        Value::Int(1)
    );
    assert_eq!(
        ev(
            &mut c,
            "var m = SplayTreeMap(); m[3] = 'x'; m[1] = 'y';",
            "m.length"
        ),
        Value::Int(2)
    );
    assert_eq!(
        ev(
            &mut c,
            "var m = SplayTreeMap(); m[3] = 'x'; m[1] = 'y';",
            "m[1]"
        ),
        Value::str("y")
    );
    assert_eq!(
        ev(
            &mut c,
            "var m = SplayTreeMap.from({1: 'a', 2: 'b'});",
            "m[2]"
        ),
        Value::str("b")
    );
    assert_eq!(
        ev(
            &mut c,
            "var m = HashMap(); var sum = 0; m['a'] = 5; m.forEach((k, v) { sum += v; });",
            "sum"
        ),
        Value::Int(5)
    );
}

// ---------------------------------------------------------------------------
// Sets
// ---------------------------------------------------------------------------

#[test]
fn hash_set_basics() {
    let mut c = ctx();
    assert_eq!(
        ev(
            &mut c,
            "var s = HashSet(); s.add(1); s.add(1); s.add(2);",
            "s.length"
        ),
        Value::Int(2)
    );
    assert_eq!(
        ev(&mut c, "var s = HashSet(); s.add(1);", "s.contains(1)"),
        Value::Bool(true)
    );
    assert_eq!(
        ev(&mut c, "var s = HashSet(); s.add(1);", "s.contains(2)"),
        Value::Bool(false)
    );
    assert_eq!(
        ev(&mut c, "var s = HashSet(); s.add(1);", "s.remove(1)"),
        Value::Bool(true)
    );
    assert_eq!(
        ev(&mut c, "var s = HashSet(); s.add(1);", "s.remove(2)"),
        Value::Bool(false)
    );
    assert_eq!(
        ev(&mut c, "var s = HashSet.from([1, 2, 2, 3]);", "s.length"),
        Value::Int(3)
    );
    assert_eq!(
        ev(
            &mut c,
            "var s = HashSet(); s.add(1); s.clear();",
            "s.length"
        ),
        Value::Int(0)
    );
}

#[test]
fn set_operations() {
    let mut c = ctx();
    assert_eq!(
        ev(
            &mut c,
            "var s = HashSet.from([1, 2, 3]).union(HashSet.from([3, 4]));",
            "s.length"
        ),
        Value::Int(4)
    );
    assert_eq!(
        ev(
            &mut c,
            "var s = HashSet.from([1, 2, 3]).intersection(HashSet.from([2, 3, 4]));",
            "s.length"
        ),
        Value::Int(2)
    );
    assert_eq!(
        ev(
            &mut c,
            "var s = HashSet.from([1, 2, 3]).difference(HashSet.from([2]));",
            "s.length"
        ),
        Value::Int(2)
    );
    assert_eq!(
        ev(&mut c, "var s = HashSet.from([1, 2, 3]);", "s.lookup(2)"),
        Value::Int(2)
    );
    assert_eq!(
        ev(
            &mut c,
            "var s = HashSet.from([1, 2, 3, 4]); s.removeWhere((x) => x % 2 == 0);",
            "s.length"
        ),
        Value::Int(2)
    );
    assert_eq!(
        ev(
            &mut c,
            "var s = HashSet.from([1, 2, 3, 4]); s.retainWhere((x) => x % 2 == 0);",
            "s.length"
        ),
        Value::Int(2)
    );
    assert_eq!(
        ev(
            &mut c,
            "var s = HashSet(); var sum = 0; s.addAll([1, 2, 3]); s.forEach((x) { sum += x; });",
            "sum"
        ),
        Value::Int(6)
    );
    assert_eq!(
        ev(
            &mut c,
            "var s = LinkedHashSet.from([5, 6]);",
            "s.contains(6)"
        ),
        Value::Bool(true)
    );
    assert_eq!(
        ev(&mut c, "var s = SplayTreeSet.from([3, 1, 2]);", "s.first"),
        Value::Int(1)
    );
}

// ---------------------------------------------------------------------------
// Queues
// ---------------------------------------------------------------------------

#[test]
fn list_queue_basics() {
    let mut c = ctx();
    assert_eq!(
        ev(
            &mut c,
            "var q = ListQueue(); q.add(1); q.add(2);",
            "q.length"
        ),
        Value::Int(2)
    );
    assert_eq!(
        ev(
            &mut c,
            "var q = ListQueue(); q.add(1); q.add(2);",
            "q.removeFirst()"
        ),
        Value::Int(1)
    );
    assert_eq!(
        ev(
            &mut c,
            "var q = ListQueue(); q.add(1); q.add(2); q.removeFirst();",
            "q.length"
        ),
        Value::Int(1)
    );
    assert_eq!(
        ev(
            &mut c,
            "var q = ListQueue(); q.add(1); q.add(2);",
            "q.removeLast()"
        ),
        Value::Int(2)
    );
    assert_eq!(
        ev(
            &mut c,
            "var q = ListQueue(); q.addFirst(1); q.addFirst(2);",
            "q.first"
        ),
        Value::Int(2)
    );
    assert_list(
        ev(&mut c, "var q = ListQueue.from([1, 2, 3]);", "q.toList()"),
        vec![1, 2, 3],
    );
    assert_eq!(
        ev(
            &mut c,
            "var q = Queue(); q.add(5); q.addLast(6); q.addFirst(4);",
            "q.length"
        ),
        Value::Int(3)
    );
    assert_eq!(
        ev(&mut c, "var q = Queue.from([1, 2]);", "q.removeFirst()"),
        Value::Int(1)
    );
    assert_eq!(
        ev(&mut c, "var q = ListQueue(); q.add(1); q.add(2);", "q.last"),
        Value::Int(2)
    );
    assert_eq!(
        ev(
            &mut c,
            "var q = ListQueue.from([1, 2, 3, 4]); q.removeWhere((x) => x.isEven);",
            "q.length"
        ),
        Value::Int(2)
    );
}

// ---------------------------------------------------------------------------
// UnmodifiableListView
// ---------------------------------------------------------------------------

#[test]
fn unmodifiable_list_view() {
    let mut c = ctx();
    assert_eq!(
        ev(&mut c, "var l = UnmodifiableListView([1, 2, 3]);", "l[1]"),
        Value::Int(2)
    );
    assert_eq!(
        ev(
            &mut c,
            "var l = UnmodifiableListView([1, 2, 3]);",
            "l.length"
        ),
        Value::Int(3)
    );
    assert_eq!(
        ev(
            &mut c,
            "var l = UnmodifiableListView([1, 2, 3]);",
            "l.indexOf(2)"
        ),
        Value::Int(1)
    );
    assert_eq!(
        ev(
            &mut c,
            "var l = UnmodifiableListView([1, 2, 3]);",
            "l.contains(3)"
        ),
        Value::Bool(true)
    );
    assert_list(
        ev(
            &mut c,
            "var l = UnmodifiableListView([1, 2, 3]);",
            "l.toList()",
        ),
        vec![1, 2, 3],
    );
    assert_eq!(
        ev(
            &mut c,
            "var l = UnmodifiableListView([1, 2, 3]);",
            "l.toString()"
        ),
        Value::str("[1, 2, 3]")
    );
}
