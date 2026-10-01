//! Auto-extracted test cases from
//! ~/d4rt/test/stdlib/collection/linked_list_test.dart.
//! Test names mirror the Dart originals, hence non-snake-case.
#![allow(non_snake_case)]

use d4rt_rs::{Context, Value};

fn ctx() -> Context {
    let mut c = Context::new();
    c.eval("import 'dart:collection'; void main() {}").unwrap();
    c
}

/// Evaluates `body` and returns the value of the `ret` expression.
fn ev(c: &mut Context, body: &str, ret: &str) -> Value {
    c.eval(&format!("void main() {{ {body} return {ret}; }}"))
        .unwrap()
}

fn map_get(v: &Value, key: &str) -> Value {
    match v {
        Value::Map(m) => {
            let m = m.borrow();
            m.entries
                .iter()
                .find(|(k, _)| matches!(k, Value::Str(s) if s.as_ref() == key))
                .map(|(_, v)| v.clone())
                .unwrap_or_else(|| panic!("key {key:?} not in map"))
        }
        other => panic!("expected map, got {other:?}"),
    }
}

#[test]
fn ref_Create_LinkedList_add_entries_check_properties() {
    let mut c = ctx();
    let result = ev(
        &mut c,
        r#"
        var list = LinkedList();
        var entry1 = LinkedListEntry('apple');
        var entry2 = LinkedListEntry(123);

        list.add(entry1);
        list.add(entry2);

        var r = {
          'length': list.length,
          'isEmpty': list.isEmpty,
          'isNotEmpty': list.isNotEmpty,
          'firstValue': list.first.value,
          'lastValue': list.last.value,
          'entry1InList': entry1.list != null,
          'entry2NextIsNull': entry2.next == null,
          'entry1PrevIsNull': entry1.previous == null,
          'entry1NextIsEntry2': entry1.next == entry2,
          'entry2PrevIsEntry1': entry2.previous == entry1,
        };
        return r;
    "#,
        "r",
    );
    assert_eq!(map_get(&result, "length"), Value::Int(2));
    assert_eq!(map_get(&result, "isEmpty"), Value::Bool(false));
    assert_eq!(map_get(&result, "isNotEmpty"), Value::Bool(true));
    assert_eq!(map_get(&result, "firstValue"), Value::str("apple"));
    assert_eq!(map_get(&result, "lastValue"), Value::Int(123));
    assert_eq!(map_get(&result, "entry1InList"), Value::Bool(true));
    assert_eq!(map_get(&result, "entry2NextIsNull"), Value::Bool(true));
    assert_eq!(map_get(&result, "entry1PrevIsNull"), Value::Bool(true));
    assert_eq!(map_get(&result, "entry1NextIsEntry2"), Value::Bool(true));
    assert_eq!(map_get(&result, "entry2PrevIsEntry1"), Value::Bool(true));
}

#[test]
fn ref_LinkedListEntry_unlink() {
    let mut c = ctx();
    let result = ev(
        &mut c,
        r#"
        var list = LinkedList();
        var entry1 = LinkedListEntry('a');
        var entry2 = LinkedListEntry('b');
        list.add(entry1);
        list.add(entry2);

        entry1.unlink();

        var r = {
          'listLength': list.length,
          'firstValue': list.first.value,
          'entry1ListIsNull': entry1.list == null,
          'entry1PrevIsNull': entry1.previous == null,
          'entry1NextIsNull': entry1.next == null,
        };
        return r;
    "#,
        "r",
    );
    assert_eq!(map_get(&result, "listLength"), Value::Int(1));
    assert_eq!(map_get(&result, "firstValue"), Value::str("b"));
    assert_eq!(map_get(&result, "entry1ListIsNull"), Value::Bool(true));
    assert_eq!(map_get(&result, "entry1PrevIsNull"), Value::Bool(true));
    assert_eq!(map_get(&result, "entry1NextIsNull"), Value::Bool(true));
}

#[test]
fn ref_LinkedList_remove_entry() {
    let mut c = ctx();
    let result = ev(
        &mut c,
        r#"
        var list = LinkedList();
        var entry1 = LinkedListEntry(1);
        var entry2 = LinkedListEntry(2);
        var entry3 = LinkedListEntry(3);
        list.add(entry1);
        list.add(entry2);
        list.add(entry3);

        var removed = list.remove(entry2);

        var r = {
          'removedResult': removed,
          'listLength': list.length,
          'firstValue': list.first.value,
          'lastValue': list.last.value,
          'entry2ListIsNull': entry2.list == null,
          'entry1NextIsEntry3': entry1.next == entry3,
          'entry3PrevIsEntry1': entry3.previous == entry1,
        };
        return r;
    "#,
        "r",
    );
    assert_eq!(map_get(&result, "removedResult"), Value::Bool(true));
    assert_eq!(map_get(&result, "listLength"), Value::Int(2));
    assert_eq!(map_get(&result, "firstValue"), Value::Int(1));
    assert_eq!(map_get(&result, "lastValue"), Value::Int(3));
    assert_eq!(map_get(&result, "entry2ListIsNull"), Value::Bool(true));
    assert_eq!(map_get(&result, "entry1NextIsEntry3"), Value::Bool(true));
    assert_eq!(map_get(&result, "entry3PrevIsEntry1"), Value::Bool(true));
}

#[test]
fn ref_LinkedList_removeFirst() {
    let mut c = ctx();
    let result = ev(
        &mut c,
        r#"
        var list = LinkedList();
        var entry1 = LinkedListEntry('x');
        var entry2 = LinkedListEntry('y');
        list.add(entry1);
        list.add(entry2);

        var removedEntry = list.removeFirst();

        var r = {
          'removedValue': removedEntry.value,
          'removedEntryIsEntry1': removedEntry == entry1,
          'listLength': list.length,
          'firstValue': list.first.value,
          'entry1ListIsNull': entry1.list == null,
        };
        return r;
    "#,
        "r",
    );
    assert_eq!(map_get(&result, "removedValue"), Value::str("x"));
    assert_eq!(map_get(&result, "removedEntryIsEntry1"), Value::Bool(true));
    assert_eq!(map_get(&result, "listLength"), Value::Int(1));
    assert_eq!(map_get(&result, "firstValue"), Value::str("y"));
    assert_eq!(map_get(&result, "entry1ListIsNull"), Value::Bool(true));
}

#[test]
fn ref_LinkedList_clear() {
    let mut c = ctx();
    let result = ev(
        &mut c,
        r#"
        var list = LinkedList();
        var entry1 = LinkedListEntry(100);
        list.add(entry1);
        list.clear();

        var r = {
          'listLength': list.length,
          'isEmpty': list.isEmpty,
          'entry1ListIsNull': entry1.list == null,
        };
        return r;
    "#,
        "r",
    );
    assert_eq!(map_get(&result, "listLength"), Value::Int(0));
    assert_eq!(map_get(&result, "isEmpty"), Value::Bool(true));
    assert_eq!(map_get(&result, "entry1ListIsNull"), Value::Bool(true));
}

#[test]
fn ref_Accessing_first_last_on_empty_list_throws_error() {
    let mut c = ctx();
    assert!(
        c.eval(
            "import 'dart:collection'; void main() { var list = LinkedList(); var x = list.first; }"
        )
        .is_err()
    );
    let mut c = ctx();
    assert!(
        c.eval(
            "import 'dart:collection'; void main() { var list = LinkedList(); var x = list.last; }"
        )
        .is_err()
    );
}

#[test]
fn ref_removeFirst_on_empty_list_throws_error() {
    let mut c = ctx();
    assert!(
        c.eval(
            "import 'dart:collection'; void main() { var list = LinkedList(); list.removeFirst(); }"
        )
        .is_err()
    );
}

#[test]
fn ref_Unlink_entry_not_in_a_list_throws_error() {
    let mut c = ctx();
    assert!(c.eval("import 'dart:collection'; void main() { var entry = LinkedListEntry(0); entry.unlink(); }").is_err());
    let mut c = ctx();
    assert!(
        c.eval(
            r#"
        import 'dart:collection';
        void main() {
          var list = LinkedList();
          var entry = LinkedListEntry(0);
          list.add(entry);
          entry.unlink();
          entry.unlink();
        }
    "#
        )
        .is_err()
    );
}

#[test]
fn linked_list_add_entry_already_in_list_throws() {
    let mut c = ctx();
    assert!(
        c.eval(
            r#"
        import 'dart:collection';
        void main() {
          var list = LinkedList();
          var entry = LinkedListEntry(0);
          list.add(entry);
          list.add(entry);
        }
    "#
        )
        .is_err()
    );
}

#[test]
fn linked_list_remove_entry_from_other_list_returns_false() {
    let mut c = ctx();
    let result = ev(
        &mut c,
        r#"
        var l1 = LinkedList();
        var l2 = LinkedList();
        var e = LinkedListEntry(1);
        l1.add(e);
        var removed = l2.remove(e);
        var r = {
          'removed': removed,
          'l1Length': l1.length,
          'eStillInL1': e.list == l1,
        };
        return r;
    "#,
        "r",
    );
    assert_eq!(map_get(&result, "removed"), Value::Bool(false));
    assert_eq!(map_get(&result, "l1Length"), Value::Int(1));
    assert_eq!(map_get(&result, "eStillInL1"), Value::Bool(true));
}

#[test]
fn linked_list_entry_identity_links() {
    let mut c = ctx();
    let result = ev(
        &mut c,
        r#"
        var list = LinkedList();
        var a = LinkedListEntry('a');
        var b = LinkedListEntry('b');
        list.add(a);
        list.add(b);
        var r = {
          'firstIsA': list.first == a,
          'lastIsB': list.last == b,
          'aNextIsB': a.next == b,
          'bPrevIsA': b.previous == a,
        };
        return r;
    "#,
        "r",
    );
    assert_eq!(map_get(&result, "firstIsA"), Value::Bool(true));
    assert_eq!(map_get(&result, "lastIsB"), Value::Bool(true));
    assert_eq!(map_get(&result, "aNextIsB"), Value::Bool(true));
    assert_eq!(map_get(&result, "bPrevIsA"), Value::Bool(true));
}
