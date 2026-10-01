//! dart:core String + StringBuffer coverage.

use d4rt_rs::{Context, Value};

fn expr(src: &str) -> Value {
    Context::new().eval_expression(src).unwrap()
}

fn run(src: &str) -> Value {
    Context::new().eval(src).unwrap()
}

// ---------------------------------------------------------------------------
// Properties
// ---------------------------------------------------------------------------

#[test]
fn properties() {
    assert_eq!(expr("'hello'.length"), Value::Int(5));
    assert_eq!(expr("''.length"), Value::Int(0));
    assert_eq!(expr("'abc'.isEmpty"), Value::Bool(false));
    assert_eq!(expr("''.isEmpty"), Value::Bool(true));
    assert_eq!(expr("'abc'.isNotEmpty"), Value::Bool(true));
    assert_eq!(expr("''.isNotEmpty"), Value::Bool(false));
    assert_eq!(expr("'abc'[0]"), Value::str("a"));
    assert_eq!(expr("'abc'[2]"), Value::str("c"));
    assert_eq!(expr("'abc'.codeUnitAt(1)"), Value::Int(98));
    assert_eq!(expr("'abc'.codeUnits.join(',')"), Value::str("97,98,99"));
    assert_eq!(expr("'é'.runes.first"), Value::Int(233));
    assert_eq!(expr("'😀'.runes.first"), Value::Int(0x1F600));
}

// ---------------------------------------------------------------------------
// Case & trimming
// ---------------------------------------------------------------------------

#[test]
fn case_and_trim() {
    assert_eq!(expr("'HeLLo'.toLowerCase()"), Value::str("hello"));
    assert_eq!(expr("'HeLLo'.toUpperCase()"), Value::str("HELLO"));
    assert_eq!(expr("'  hi  '.trim()"), Value::str("hi"));
    assert_eq!(expr("'  hi  '.trimLeft()"), Value::str("hi  "));
    assert_eq!(expr("'  hi  '.trimRight()"), Value::str("  hi"));
    assert_eq!(expr(r"'\t\n x \n'.trim()"), Value::str("x"));
}

// ---------------------------------------------------------------------------
// Padding & substring
// ---------------------------------------------------------------------------

#[test]
fn padding_and_substring() {
    assert_eq!(expr("'5'.padLeft(3, '0')"), Value::str("005"));
    assert_eq!(expr("'5'.padLeft(3)"), Value::str("  5"));
    assert_eq!(expr("'5'.padRight(3, '0')"), Value::str("500"));
    assert_eq!(expr("'hello'.substring(1)"), Value::str("ello"));
    assert_eq!(expr("'hello'.substring(1, 3)"), Value::str("el"));
    assert_eq!(expr("'hello'.substring(0, 0)"), Value::str(""));
    assert_eq!(expr("'hello'.substring(5)"), Value::str(""));
}

// ---------------------------------------------------------------------------
// Search
// ---------------------------------------------------------------------------

#[test]
fn search() {
    assert_eq!(expr("'hello world'.startsWith('he')"), Value::Bool(true));
    assert_eq!(expr("'hello world'.startsWith('lo')"), Value::Bool(false));
    assert_eq!(expr("'hello world'.endsWith('rld')"), Value::Bool(true));
    assert_eq!(expr("'hello world'.endsWith('llo')"), Value::Bool(false));
    assert_eq!(expr("'hello'.contains('ell')"), Value::Bool(true));
    assert_eq!(expr("'hello'.contains('xyz')"), Value::Bool(false));
    assert_eq!(expr("'banana'.indexOf('na')"), Value::Int(2));
    assert_eq!(expr("'banana'.indexOf('na', 3)"), Value::Int(4));
    assert_eq!(expr("'banana'.indexOf('zz')"), Value::Int(-1));
    assert_eq!(expr("'banana'.lastIndexOf('na')"), Value::Int(4));
    assert_eq!(expr("'banana'.lastIndexOf('a')"), Value::Int(5));
    assert_eq!(expr("'abc'.indexOf('b', 2)"), Value::Int(-1));
    assert_eq!(expr("'aXbXc'.split('X').length"), Value::Int(3));
    assert_eq!(expr("'aXbXc'.split('X')[1]"), Value::str("b"));
    assert_eq!(expr("'abc'.split('').length"), Value::Int(3));
    assert_eq!(
        expr("'a b  c'.split(RegExp(r'\\s+')).join('-')"),
        Value::str("a-b-c")
    );
}

// ---------------------------------------------------------------------------
// Replacements
// ---------------------------------------------------------------------------

#[test]
fn replacements() {
    assert_eq!(expr("'hello'.replaceAll('l', 'L')"), Value::str("heLLo"));
    assert_eq!(
        expr("'hello'.replaceAll(RegExp(r'l+'), 'L')"),
        Value::str("heLo")
    );
    assert_eq!(expr("'hello'.replaceFirst('l', 'L')"), Value::str("heLlo"));
    assert_eq!(
        expr("'abc'.replaceFirst(RegExp(r'[bc]'), 'X')"),
        Value::str("aXc")
    );
    assert_eq!(
        expr("'hello'.replaceRange(1, 3, 'XX')"),
        Value::str("hXXlo")
    );
    assert_eq!(
        expr("'hello'.replaceAllMapped(RegExp(r'l'), (m) => m[0]! + '!')"),
        Value::str("hel!l!o")
    );
    assert_eq!(
        expr("'a1b2'.replaceAllMapped(RegExp(r'\\d'), (m) => '${int.parse(m[0]!) * 10}')"),
        Value::str("a10b20")
    );
    assert_eq!(
        expr("'hello'.replaceFirstMapped(RegExp(r'l'), (m) => 'L${m.start}')"),
        Value::str("heL2lo")
    );
}

#[test]
fn split_map_join() {
    assert_eq!(
        expr("'a,b,c'.splitMapJoin(',', onMatch: (m) => '-', onNonMatch: (s) => s.toUpperCase())"),
        Value::str("A-B-C")
    );
    assert_eq!(
        expr(
            "'hello world'.splitMapJoin(RegExp(r'\\w+'), onMatch: (m) => m[0]!.length.toString())"
        ),
        Value::str("5 5")
    );
}

// ---------------------------------------------------------------------------
// Comparison & misc
// ---------------------------------------------------------------------------

#[test]
fn compare_to() {
    assert_eq!(expr("'a'.compareTo('b')"), Value::Int(-1));
    assert_eq!(expr("'b'.compareTo('a')"), Value::Int(1));
    assert_eq!(expr("'a'.compareTo('a')"), Value::Int(0));
    assert_eq!(expr("'abc' < 'abd'"), Value::Bool(true));
    assert_eq!(expr("'zz' > 'aa'"), Value::Bool(true));
}

#[test]
fn constructors_and_statics() {
    assert_eq!(expr("String.fromCharCode(65)"), Value::str("A"));
    assert_eq!(expr("String.fromCharCodes([72, 105])"), Value::str("Hi"));
    assert_eq!(expr("String.fromCharCodes([0x1F600])"), Value::str("😀"));
    assert_eq!(
        expr("String.fromEnvironment('D4RT_NO_SUCH_ENV_VAR', defaultValue: 'none')"),
        Value::str("none")
    );
}

#[test]
fn iterable_ops_on_string() {
    assert_eq!(expr("'abc'.elementAt(1)"), Value::str("b"));
    assert_eq!(expr("'abc'.join('-')"), Value::str("a-b-c"));
    assert_eq!(
        expr("'abc'.map((c) => c.toUpperCase()).join()"),
        Value::str("ABC")
    );
    assert_eq!(
        expr("'abc'.where((c) => c != 'b').join()"),
        Value::str("ac")
    );
    assert_eq!(expr("'abc'.contains('b')"), Value::Bool(true));
    assert_eq!(expr("'abc'.toList().length"), Value::Int(3));
    assert_eq!(expr("'aab'.allMatches(RegExp('a')).length"), Value::Int(2));
}

// ---------------------------------------------------------------------------
// StringBuffer
// ---------------------------------------------------------------------------

#[test]
fn string_buffer() {
    assert_eq!(
        run(
            "void main() { var sb = StringBuffer(); sb.write('a'); sb.write('b'); return sb.toString(); }"
        ),
        Value::str("ab")
    );
    assert_eq!(
        run(
            "void main() { var sb = StringBuffer('start'); sb.write('end'); return sb.toString(); }"
        ),
        Value::str("startend")
    );
    assert_eq!(
        run(
            "void main() { var sb = StringBuffer(); sb.writeAll([1, 2, 3]); return sb.toString(); }"
        ),
        Value::str("123")
    );
    assert_eq!(
        run(
            "void main() { var sb = StringBuffer(); sb.writeAll([1, 2, 3], '-'); return sb.toString(); }"
        ),
        Value::str("1-2-3")
    );
    assert_eq!(
        run("void main() { var sb = StringBuffer(); sb.writeCharCode(65); return sb.toString(); }"),
        Value::str("A")
    );
    assert_eq!(
        run("void main() { var sb = StringBuffer(); sb.writeln('line'); return sb.toString(); }"),
        Value::str("line\n")
    );
    assert_eq!(
        run("void main() { var sb = StringBuffer('ab'); sb.write('c'); return sb.length; }"),
        Value::Int(3)
    );
    assert_eq!(
        run("void main() { var sb = StringBuffer(); return sb.isEmpty; }"),
        Value::Bool(true)
    );
    assert_eq!(
        run("void main() { var sb = StringBuffer(); sb.clear(); return sb.toString(); }"),
        Value::str("")
    );
    assert_eq!(
        run("void main() { var sb = StringBuffer('ab'); sb.clear(); return sb.toString(); }"),
        Value::str("")
    );
}

// ---------------------------------------------------------------------------
// Patterns / RegExp interaction (Match)
// ---------------------------------------------------------------------------

#[test]
fn match_on_string() {
    assert_eq!(
        expr("'a1b2'.matchAsPrefix(RegExp(r'a1')).start"),
        Value::Int(0)
    );
    assert_eq!(
        expr("'abc123'.matchAsPrefix(RegExp(r'[a-z]+')).end"),
        Value::Int(3)
    );
}
