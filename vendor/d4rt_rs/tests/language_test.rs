//! Language feature coverage: operators, control flow, functions, records,
//! classes, inheritance, enums, generators, exceptions.

use std::collections::HashMap;

use d4rt_rs::{Context, FromValue, ToValue, Value};

fn expr(src: &str) -> Value {
    Context::new().eval_expression(src).unwrap()
}

fn run(src: &str) -> Value {
    Context::new().eval(src).unwrap()
}

fn is_err(expr_src: &str) {
    assert!(
        Context::new().eval_expression(expr_src).is_err(),
        "expected error: {expr_src}"
    );
}

// ---------------------------------------------------------------------------
// Arithmetic & numeric
// ---------------------------------------------------------------------------

#[test]
fn arithmetic() {
    assert_eq!(expr("1 + 2 * 3"), Value::Int(7));
    assert_eq!(expr("(1 + 2) * 3"), Value::Int(9));
    assert_eq!(expr("10 / 4"), Value::Double(2.5));
    assert_eq!(expr("10 ~/ 4"), Value::Int(2));
    assert_eq!(expr("10 % 3"), Value::Int(1));
    assert_eq!(expr("-5 + 3"), Value::Int(-2));
    assert_eq!(expr("1 - 2 - 3"), Value::Int(-4));
    assert_eq!(expr("2 + 3 * 4 - 5"), Value::Int(9));
    assert_eq!(expr("5 * -2"), Value::Int(-10));
    assert_eq!(expr("0x10 + 0xF"), Value::Int(31));
    assert_eq!(expr("1_000_000"), Value::Int(1000000));
}

#[test]
fn numeric_edge_cases() {
    assert_eq!(expr("7 % -3"), Value::Int(1));
    assert_eq!(expr("-7 % 3"), Value::Int(-1));
    assert_eq!(expr("7 ~/ -2"), Value::Int(-3));
    assert_eq!(expr("0.1 + 0.2"), Value::Double(0.30000000000000004));
    assert_eq!(expr("5.5 ~/ 2"), Value::Int(2));
    assert_eq!(expr("-5.5 ~/ 2"), Value::Int(-2));
    assert_eq!(expr("1 / 0"), Value::Double(f64::INFINITY));
    assert_eq!(expr("0 / 0").as_bool(), None); // NaN double, not bool
    assert_eq!(expr("3 == 3.0"), Value::Bool(true));
}

#[test]
fn bitwise_and_shift() {
    assert_eq!(expr("5 & 3"), Value::Int(1));
    assert_eq!(expr("5 | 3"), Value::Int(7));
    assert_eq!(expr("5 ^ 3"), Value::Int(6));
    assert_eq!(expr("~5"), Value::Int(!5));
    assert_eq!(expr("1 << 4"), Value::Int(16));
    assert_eq!(expr("256 >> 4"), Value::Int(16));
    assert_eq!(expr("2 << 3 + 1"), Value::Int(32));
}

#[test]
fn comparison_and_logic() {
    assert_eq!(expr("1 < 2"), Value::Bool(true));
    assert_eq!(expr("2 <= 2"), Value::Bool(true));
    assert_eq!(expr("3 > 4"), Value::Bool(false));
    assert_eq!(expr("4 >= 4"), Value::Bool(true));
    assert_eq!(expr("'a' < 'b'"), Value::Bool(true));
    assert_eq!(expr("true && false"), Value::Bool(false));
    assert_eq!(expr("true || false"), Value::Bool(true));
    assert_eq!(expr("!true"), Value::Bool(false));
    assert_eq!(expr("false && (1 ~/ 0)"), Value::Bool(false));
    assert_eq!(expr("true || (1 ~/ 0)"), Value::Bool(true));
}

#[test]
fn null_handling() {
    assert_eq!(expr("null"), Value::Null);
    assert_eq!(expr("null ?? 5"), Value::Int(5));
    assert_eq!(expr("0 ?? 5"), Value::Int(0));
    assert_eq!(expr("'' ?? 'x'"), Value::str(""));
    assert_eq!(expr("null ?? null ?? 3"), Value::Int(3));
    is_err("null + 1");
}

#[test]
fn string_operators() {
    assert_eq!(expr("'ab' + 'cd'"), Value::str("abcd"));
    assert_eq!(expr("'a' * 3"), Value::str("aaa"));
    assert_eq!(expr("'a' * 0"), Value::str(""));
    assert_eq!(expr("'abc' == 'abc'"), Value::Bool(true));
    assert_eq!(expr("'abc' != 'abd'"), Value::Bool(true));
}

#[test]
fn string_interpolation() {
    assert_eq!(expr("'v: ${1 + 2}'"), Value::str("v: 3"));
    assert_eq!(expr("'${'nested ${'inner'}'}'"), Value::str("nested inner"));
    is_err(r"'$x'");
    assert_eq!(expr(r"'\n\t\x41\u{1F600}'"), Value::str("\n\tA\u{1f600}"));
    assert_eq!(expr(r"r'raw\n'"), Value::str(r"raw\n"));
}

// ---------------------------------------------------------------------------
// Control flow
// ---------------------------------------------------------------------------

#[test]
fn if_else() {
    assert_eq!(
        run("void main() { var x = 5; if (x > 3) { return 'big'; } else { return 'small'; } }"),
        Value::str("big")
    );
    assert_eq!(
        run(
            "void main() { var x = 1; if (x > 3) { return 'big'; } else if (x > 0) { return 'pos'; } else { return 'other'; } }"
        ),
        Value::str("pos")
    );
    assert_eq!(
        run(
            "void main() { var x = -1; if (x > 3) { return 'big'; } else if (x > 0) { return 'pos'; } else { return 'other'; } }"
        ),
        Value::str("other")
    );
    assert_eq!(
        run("void main() { if (false) { return 1; } return 2; }"),
        Value::Int(2)
    );
    assert_eq!(
        run("void main() { if (true) return 1; return 2; }"),
        Value::Int(1)
    );
}

#[test]
fn loops() {
    assert_eq!(
        run("void main() { var s = 0; for (var i = 1; i <= 10; i++) { s += i; } return s; }"),
        Value::Int(55)
    );
    assert_eq!(
        run("void main() { var s = 0; var i = 0; while (i < 5) { s += i; i++; } return s; }"),
        Value::Int(10)
    );
    assert_eq!(
        run("void main() { var s = 0; var i = 0; do { s += i; i++; } while (i < 5); return s; }"),
        Value::Int(10)
    );
    assert_eq!(
        run(
            "void main() { var s = 0; for (var i = 0; i < 10; i++) { if (i == 5) { break; } if (i % 2 == 0) { continue; } s += i; } return s; }"
        ),
        Value::Int(4)
    );
    assert_eq!(
        run(
            "void main() { var s = 0; for (var i = 0, j = 10; i < 3; i++, j--) { s += j; } return s; }"
        ),
        Value::Int(27)
    );
}

#[test]
fn for_in_loops() {
    assert_eq!(
        run("void main() { var s = 0; for (var v in [1, 2, 3, 4]) { s += v; } return s; }"),
        Value::Int(10)
    );
    assert_eq!(
        run("void main() { var s = ''; for (var c in 'abc') { s += c; } return s; }"),
        Value::str("abc")
    );
    assert_eq!(
        run(
            "void main() { var s = ''; for (final (k, v) in {'a': 1, 'b': 2}.entries) { s += '${k}$v'; } return s; }"
        ),
        Value::str("a1b2")
    );
}

#[test]
fn switch_statement() {
    assert_eq!(
        run(
            "void main() { var x = 2; switch (x) { case 1: return 'one'; case 2: return 'two'; default: return 'other'; } }"
        ),
        Value::str("two")
    );
    assert_eq!(
        run(
            "void main() { var x = 9; switch (x) { case 1: return 'one'; case 2: return 'two'; default: return 'other'; } }"
        ),
        Value::str("other")
    );
    assert_eq!(
        run(
            "void main() { var x = 3; switch (x) { case 1: return 'one'; case 3: return 'three'; default: return 'other'; } }"
        ),
        Value::str("three")
    );
}

// ---------------------------------------------------------------------------
// Collections & records
// ---------------------------------------------------------------------------

#[test]
fn list_operations() {
    assert_eq!(expr("[1, 2, 3].length"), Value::Int(3));
    assert_eq!(expr("[1, 2, 3][0]"), Value::Int(1));
    assert_eq!(expr("[[1, 2], [3]][1][0]"), Value::Int(3));
    assert_eq!(
        run("void main() { var l = [1]; l.add(2); return l.length; }"),
        Value::Int(2)
    );
    assert_eq!(
        run("void main() { var l = [1, 2]; l[0] = 9; return l[0] + l[1]; }"),
        Value::Int(11)
    );
    assert_eq!(expr("[1, 2, ...[3, 4]].length"), Value::Int(4));
    assert_eq!(expr("[...'ab'].join('')"), Value::str("ab"));
    assert_eq!(
        expr("List.generate(4, (i) => i * i).join(',')"),
        Value::str("0,1,4,9")
    );
}

#[test]
fn map_operations() {
    assert_eq!(expr("{'a': 1}['a']"), Value::Int(1));
    assert_eq!(expr("{'a': 1}['z']"), Value::Null);
    assert_eq!(
        run("void main() { var m = {'a': 1}; m['b'] = 2; return m['a'] + m['b']; }"),
        Value::Int(3)
    );
    assert_eq!(expr("{1: 'one', 2: 'two'}[2]"), Value::str("two"));
    assert_eq!(expr("{'a': {'b': 3}}['a']['b']"), Value::Int(3));
    assert_eq!(expr("{'x': 1, 'y': 2, ...{'z': 3}}['z']"), Value::Int(3));
}

#[test]
fn records() {
    assert_eq!(expr("(1, 2).$0"), Value::Int(1));
    assert_eq!(expr("(1, 2).$1"), Value::Int(2));
    assert_eq!(expr("(a: 1, b: 2).b"), Value::Int(2));
    assert_eq!(expr("(x: 1).x"), Value::Int(1));
    assert_eq!(expr("(1, a: 2).$0 + (1, a: 2).a"), Value::Int(3));
    assert_eq!(expr("((1, 2), 3).$0.$1"), Value::Int(2));
}

#[test]
fn sets() {
    assert_eq!(expr("{1, 2, 3}.length"), Value::Int(3));
    assert_eq!(expr("{1, 2, 3}.contains(2)"), Value::Bool(true));
    assert_eq!(expr("{1, 2, 3}.contains(9)"), Value::Bool(false));
    assert_eq!(
        run("void main() { var s = {1}; s.add(2); return s.length; }"),
        Value::Int(2)
    );
}

// ---------------------------------------------------------------------------
// Functions
// ---------------------------------------------------------------------------

#[test]
fn function_basics() {
    assert_eq!(
        run("int add(int a, int b) { return a + b; } void main() { return add(2, 3); }"),
        Value::Int(5)
    );
    assert_eq!(
        run("int sub(int a, int b) => a - b; void main() { return sub(10, 4); }"),
        Value::Int(6)
    );
    assert_eq!(
        run("void main() { var f = (int x) => x * 2; return f(21); }"),
        Value::Int(42)
    );
    assert_eq!(
        run(
            "int sum(int n) { if (n == 0) { return 0; } return n + sum(n - 1); } void main() { return sum(100); }"
        ),
        Value::Int(5050)
    );
    assert_eq!(
        run(
            "int fib(int n) { if (n < 2) { return n; } return fib(n - 1) + fib(n - 2); } void main() { return fib(15); }"
        ),
        Value::Int(610)
    );
}

#[test]
fn function_params() {
    assert_eq!(
        run("int g(int x, [int y = 10]) { return x + y; } void main() { return g(1) + g(1, 2); }"),
        Value::Int(14)
    );
    assert_eq!(
        run(
            "int h({required int a, int b = 5}) { return a + b; } void main() { return h(a: 1) + h(a: 1, b: 2); }"
        ),
        Value::Int(9)
    );
    assert_eq!(
        run(
            "int m(int x, [int? y, int z = 3]) { return x + (y ?? 0) + z; } void main() { return m(1) + m(1, 2); }"
        ),
        Value::Int(10)
    );
    assert_eq!(
        run("void main() { var f = ({int a = 1, int b = 2}) => a + b; return f() + f(a: 10); }"),
        Value::Int(15)
    );
}

#[test]
fn closures() {
    assert_eq!(
        run(
            "void main() { var calls = 0; int counter() { calls++; return calls; } counter(); counter(); return counter(); }"
        ),
        Value::Int(3)
    );
    assert_eq!(
        run(
            "void main() { List<Function> fs = []; for (var i = 0; i < 3; i++) { fs.add(() => i); } return fs[2](); }"
        ),
        Value::Int(2)
    );
    assert_eq!(
        run(
            "void main() { int makeAdder(int n) => (int x) => x + n; var add5 = makeAdder(5); return add5(37); }"
        ),
        Value::Int(42)
    );
}

#[test]
fn higher_order() {
    assert_eq!(
        expr("[1, 2, 3].map((x) => x * 2).toList()[1]"),
        Value::Int(4)
    );
    assert_eq!(
        expr("[1, 2, 3, 4].where((x) => x % 2 == 0).length"),
        Value::Int(2)
    );
    assert_eq!(expr("[1, 2, 3].fold(0, (a, b) => a + b)"), Value::Int(6));
    assert_eq!(
        expr("[3, 1, 2].reduce((a, b) => a > b ? a : b)"),
        Value::Int(3)
    );
    assert_eq!(expr("[1, 2, 3].any((x) => x > 2)"), Value::Bool(true));
    assert_eq!(expr("[1, 2, 3].every((x) => x > 0)"), Value::Bool(true));
    assert_eq!(expr("[1, 2, 3].forEach((x) => print(x))"), Value::Null);
}

#[test]
fn generators() {
    assert_eq!(
        run(
            "Iterable<int> gen() sync* { yield 1; yield 2; yield 3; } void main() { return gen().toList().length; }"
        ),
        Value::Int(3)
    );
    assert_eq!(
        run(
            "Iterable<int> gen() sync* { yield 1; yield* [2, 3]; } void main() { return gen().toList().join(','); }"
        ),
        Value::str("1,2,3")
    );
    assert_eq!(
        run(
            "Iterable<int> countDown(int n) sync* { while (n > 0) { yield n--; } } void main() { return countDown(3).toList()[1]; }"
        ),
        Value::Int(2)
    );
}

// ---------------------------------------------------------------------------
// Classes
// ---------------------------------------------------------------------------

#[test]
fn classes_basic() {
    let src = r#"
        class Point {
          int x;
          int y;
          Point(this.x, this.y);
          int sum() => x + y;
          String toString() => 'Point($x, $y)';
        }
        void main() {
          var p = Point(3, 4);
          return '${p.sum()}:${p}';
        }
    "#;
    assert_eq!(run(src), Value::str("7:Point(3, 4)"));
}

#[test]
fn classes_fields_and_this() {
    let src = r#"
        class Counter {
          int _count = 0;
          int get value => _count;
          set value(int v) { _count = v; }
          void increment() { _count++; }
        }
        void main() {
          var c = Counter();
          c.increment();
          c.increment();
          c.value = 10;
          c.increment();
          return c.value;
        }
    "#;
    assert_eq!(run(src), Value::Int(11));
}

#[test]
fn classes_constructors() {
    let src = r#"
        class Foo {
          int a;
          int b;
          Foo(this.a, this.b);
          Foo.one(this.a) : b = 1;
          Foo.two() : a = 2, b = 3;
        }
        void main() { return Foo.two().a * 10 + Foo.one(7).b + Foo(1, 2).a; }
    "#;
    assert_eq!(run(src), Value::Int(22));
}

#[test]
fn inheritance() {
    let src = r#"
        class Animal {
          String name;
          Animal(this.name);
          String speak() => '...';
        }
        class Dog extends Animal {
          Dog(String name) : super(name);
          String speak() => 'Woof';
          String whoAmI() => name;
        }
        void main() {
          var d = Dog('Rex');
          return '${d.speak()} ${d.whoAmI()}';
        }
    "#;
    assert_eq!(run(src), Value::str("Woof Rex"));
}

#[test]
fn inheritance_super_call() {
    let src = r#"
        class Base {
          int baseValue() => 10;
        }
        class Derived extends Base {
          int baseValue() => super.baseValue() + 5;
        }
        void main() { return Derived().baseValue(); }
    "#;
    assert_eq!(run(src), Value::Int(15));
}

#[test]
fn abstract_and_override() {
    let src = r#"
        class Shape {
          int area() => 0;
        }
        class Square extends Shape {
          int side;
          Square(this.side);
          int area() => side * side;
        }
        void main() { return Square(5).area(); }
    "#;
    assert_eq!(run(src), Value::Int(25));
}

#[test]
fn static_members() {
    let src = r#"
        class Mathx {
          static int pi = 3;
          static int double_(int x) => x * 2;
        }
        void main() { return Mathx.pi + Mathx.double_(2); }
    "#;
    assert_eq!(run(src), Value::Int(7));
}

#[test]
fn enum_basics() {
    let src = r#"
        enum Color { red, green, blue }
        void main() {
          var c = Color.green;
          return '${c.name}:${c.index}:${Color.values.length}:${Color.blue.name}';
        }
    "#;
    assert_eq!(run(src), Value::str("green:1:3:blue"));
}

#[test]
fn enum_with_method() {
    let src = r#"
        enum Suit {
          hearts(1), diamonds(2), clubs(3), spades(4);
          final int rank;
          const Suit(this.rank);
        }
        void main() { return Suit.spades.rank; }
    "#;
    assert_eq!(run(src), Value::Int(4));
}

// ---------------------------------------------------------------------------
// Exceptions
// ---------------------------------------------------------------------------

#[test]
fn try_catch() {
    let src = r#"
        void main() {
          try { throw 'oops'; } catch (e) { return e; }
        }
    "#;
    assert_eq!(run(src), Value::str("oops"));
}

#[test]
fn try_catch_on_types() {
    let src = r#"
        void main() {
          try { throw 42; } catch (e) { return 'int:$e'; }
        }
    "#;
    assert_eq!(run(src), Value::str("int:42"));
}

#[test]
fn try_finally() {
    let src = r#"
        void main() {
          var log = '';
          try { log += 'try;'; } finally { log += 'finally;'; }
          return log;
        }
    "#;
    assert_eq!(run(src), Value::str("try;finally;"));
}

#[test]
fn throw_custom() {
    let src = r#"
        class MyError {
          String msg;
          MyError(this.msg);
          String toString() => 'MyError: $msg';
        }
        void main() {
          try { throw MyError('bad'); } catch (e) { return e.toString(); }
        }
    "#;
    assert_eq!(run(src), Value::str("MyError: bad"));
}

#[test]
fn rethrow() {
    let src = r#"
        void main() {
          try {
            try { throw 'inner'; } catch (e) { rethrow; }
          } catch (e) { return 'caught:$e'; }
        }
    "#;
    assert_eq!(run(src), Value::str("caught:inner"));
}

// ---------------------------------------------------------------------------
// Native function registration (typed via FromValue/ToValue)
// ---------------------------------------------------------------------------

#[test]
fn native_fn_typed_args_and_result() {
    let mut ctx = Context::new();
    ctx.register_function("concat_upper", |_v, args, _named| {
        let a = String::from_value(&args[0])?;
        let b = String::from_value(&args[1])?;
        Ok(format!("{}{}", a.to_uppercase(), b.to_uppercase()).to_value())
    });
    assert_eq!(
        ctx.eval_expression(r#"concat_upper("ab", "cd")"#).unwrap(),
        Value::str("ABCD")
    );
}

#[test]
fn native_fn_with_named_args() {
    let mut ctx = Context::new();
    ctx.register_function("make_point", |_v, args, named| {
        let x = i64::from_value(&args[0])?;
        let y = i64::from_value(named.get("y").unwrap_or(&Value::Int(0)))?;
        Ok(vec![x, y].to_value())
    });
    assert_eq!(
        format!("{:?}", ctx.eval_expression("make_point(1, y: 2)").unwrap()),
        "[1, 2]"
    );
}

#[test]
fn native_fn_calls_dart() {
    let mut ctx = Context::new();
    ctx.eval("int square(int x) => x * x;").unwrap();
    let square = ctx.get("square").unwrap();
    ctx.register_function("apply_twice", move |v, args, _named| {
        let f = args[0].clone();
        let x = i64::from_value(&args[1])?;
        let r1 = v.call_value(&f, vec![Value::Int(x)], HashMap::new())?;
        v.call_value(&f, vec![r1], HashMap::new())
    });
    let _ = square;
    assert_eq!(
        ctx.eval_expression("apply_twice(square, 3)").unwrap(),
        Value::Int(81)
    );
}

#[test]
fn native_fn_errors_propagate() {
    let mut ctx = Context::new();
    ctx.register_function("boom", |_v, _args, _named| {
        Err(d4rt_rs::InterpError::runtime("explosion"))
    });
    assert!(ctx.eval_expression("boom()").is_err());
    assert!(
        ctx.eval("void main() { try { boom(); } catch (e) { return 'ok'; } }")
            .is_ok()
    );
}

// ---------------------------------------------------------------------------
// Misc
// ---------------------------------------------------------------------------

#[test]
fn null_aware_access() {
    assert_eq!(expr("(null as dynamic?)?.x"), Value::Null);
    assert_eq!(
        run("void main() { var m = {'a': 1}; return m['a']?.toString(); }"),
        Value::str("1")
    );
    is_err("null.x");
}

#[test]
fn type_test_is_not() {
    // Regression: `is!` used to behave like `is` because the negated
    // `is_operator` node is nested inside `type_test`, so the negation
    // was never detected.
    assert_eq!(expr("[1] is List"), Value::Bool(true));
    assert_eq!(expr("[1] is! List"), Value::Bool(false));
    assert_eq!(expr("5 is! List"), Value::Bool(true));
    assert_eq!(expr("'x' is String"), Value::Bool(true));
    assert_eq!(expr("'x' is! String"), Value::Bool(false));
    assert_eq!(expr("5 is! String"), Value::Bool(true));
    assert_eq!(
        run(
            "void main() { var items = [1, 2]; if (items == null || items is! List) return 'bad'; return 'ok'; }"
        ),
        Value::str("ok")
    );
}

#[test]
fn collection_if_in_map_and_set() {
    // Regression: map/set literals containing `if` elements used to fail
    // with "UnimplementedError: Expression 'if_element'".
    assert_eq!(
        run(r#"void main() { var map = {'a': 1, if (true) 'b': 2}; return map['b'].toString(); }"#),
        Value::str("2")
    );
    assert_eq!(
        run(
            r#"void main() { var map = {'a': 1, if (false) 'b': 2}; return map.containsKey('b').toString(); }"#
        ),
        Value::str("false")
    );
    assert_eq!(
        run(
            r#"void main() { var map = {'a': 1, if (false) 'b': 2 else 'c': 3}; return map['c'].toString(); }"#
        ),
        Value::str("3")
    );
    assert_eq!(
        run(r#"void main() { var s = {1, if (true) 2}; return s.contains(2).toString(); }"#),
        Value::str("true")
    );
    assert_eq!(
        run(
            r#"void main() { var s = {1, if (false) 2 else 3}; return s.contains(3).toString(); }"#
        ),
        Value::str("true")
    );
}

#[test]
fn quote_stripping_multiline_and_single() {
    // Regression: multiline `"""` literals kept a leading `""` after quote
    // stripping, and a bad stripping rule mangled single-quoted URLs.
    assert_eq!(
        expr(r#"'https://example.com/x'"#),
        Value::str("https://example.com/x")
    );
    assert_eq!(
        expr(r#"'''https://example.com/x'''"#),
        Value::str("https://example.com/x")
    );
    assert_eq!(
        expr("\"\"\"\n    query {\n        x\n    }\n\"\"\""),
        Value::str("\n    query {\n        x\n    }\n")
    );
    assert_eq!(expr("'''\n    single\n'''"), Value::str("\n    single\n"));
}

#[test]
fn comment_in_default_parameter() {
    // Regression: a trailing comment on a default-parameter line was
    // mistaken for the default value expression, producing
    // "Expression 'comment' is not yet implemented".
    assert_eq!(
        run("int f({int size = 20, // cap\n int page = 1}) => size + page; int main() => f();"),
        Value::Int(21)
    );
    assert_eq!(
        run(
            "int f(int a, [int size = 20, // cap\n int page = 1]) => a + size + page; int main() => f(1);"
        ),
        Value::Int(22)
    );
}

#[test]
fn comment_in_literals() {
    // Regression: comments inside collection/record literals were visited as
    // expressions, producing "Expression 'comment' is not yet implemented".
    assert_eq!(
        run("int main() { var m = { // ongoing\n 'a': 1, 'b': 2 }; return m['b']; }"),
        Value::Int(2)
    );
    assert_eq!(
        run("int main() { var l = [1, // one\n 2, // two\n 3]; return l[1]; }"),
        Value::Int(2)
    );
    assert_eq!(
        run("int main() { var s = {1, // one\n 2}; return s.length; }"),
        Value::Int(2)
    );
    assert_eq!(
        run("int main() { var r = (a: 1, // label\n b: 2); return r.b; }"),
        Value::Int(2)
    );
}

#[test]
fn string_buffer_and_casts() {
    assert_eq!(
        run("void main() { var sb = StringBuffer('x'); sb.write('y'); return sb.toString(); }"),
        Value::str("xy")
    );
}

#[test]
fn named_field_formals() {
    // Regression: `{required this.x}` used to fail with
    // "Function '' has no named parameter 'x'" because the parameter was
    // stored as `this.x` while callers pass the bare field name.
    assert_eq!(
        run("class A { final int x; A({required this.x}); } int main() => A(x: 42).x;"),
        Value::Int(42)
    );
    assert_eq!(
        run("class B { final int x; B({this.x = 7}); } int main() => B().x;"),
        Value::Int(7)
    );
}

#[test]
fn bridged_enum_static_access() {
    use d4rt_rs::class::BridgedEnumDef;

    let mut ctx = Context::new();
    let def = BridgedEnumDef::new(
        "MStatus",
        vec![
            ("ongoing", Value::Int(0)),
            ("completed", Value::Int(1)),
            ("unknown", Value::Int(2)),
        ],
    );
    ctx.register_bridged_enum(def);
    assert_eq!(
        ctx.eval_expression("MStatus.ongoing.name").unwrap(),
        Value::str("ongoing")
    );
    assert_eq!(
        ctx.eval_expression("MStatus.ongoing == MStatus.ongoing")
            .unwrap(),
        Value::Bool(true)
    );
    assert_eq!(
        ctx.eval_expression("MStatus.ongoing == MStatus.completed")
            .unwrap(),
        Value::Bool(false)
    );
    assert!(ctx.eval_expression("MStatus.nope").is_err());
}
