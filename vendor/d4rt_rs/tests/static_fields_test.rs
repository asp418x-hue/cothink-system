//! `static const` / `static final` fields and top-level `const` / `final`
//! declarations must be resolvable like plain static fields.
//!
//! tree-sitter-dart parses these with `static_final_declaration_list` (children
//! `static_final_declaration`) instead of `initialized_identifier_list`, so the
//! compiler used to skip them and reading e.g. `URLS.API_URL` produced
//! "Undefined static member". MangaYomi extensions rely on this heavily.

use d4rt_rs::Context;

fn eval_expr(src: &str) -> d4rt_rs::Value {
    let mut c = Context::new();
    c.eval(src)
        .unwrap_or_else(|e| panic!("eval failed: {:?} (debug: {e:?})", e.message()))
}

fn str(v: &d4rt_rs::Value) -> String {
    match v {
        d4rt_rs::Value::Str(s) => s.to_string(),
        other => panic!("expected string, got {other:?}"),
    }
}

#[test]
fn static_const_string_field() {
    let v = eval_expr(
        r#"
class URLS {
  static const String API_URL = 'https://api.allanime.day/api';
}
main() => URLS.API_URL;
"#,
    );
    assert_eq!(str(&v), "https://api.allanime.day/api");
}

#[test]
fn static_final_string_field() {
    let v = eval_expr(
        r#"
class URLS {
  static final String API_URL = 'https://api.allanime.day/api';
}
main() => URLS.API_URL;
"#,
    );
    assert_eq!(str(&v), "https://api.allanime.day/api");
}

#[test]
fn static_plain_field_still_works() {
    let v = eval_expr(
        r#"
class URLS {
  static String API_URL = 'https://api.allanime.day/api';
}
main() => URLS.API_URL;
"#,
    );
    assert_eq!(str(&v), "https://api.allanime.day/api");
}

#[test]
fn top_level_const_identifies() {
    let v = eval_expr(r#"const API_URL = 'x'; main() => API_URL;"#);
    assert_eq!(str(&v), "x");
}

#[test]
fn top_level_final_identifies() {
    let v = eval_expr(r#"final API_URL = 'x'; main() => API_URL;"#);
    assert_eq!(str(&v), "x");
}

#[test]
fn static_final_with_expression_initializer() {
    // `static final` may initialise from a non-const expression; the value
    // must still be read back.
    let v = eval_expr(
        r#"
class Greeter {
  static final String HI = 'hello ' + 'world';
}
main() => Greeter.HI;
"#,
    );
    assert_eq!(str(&v), "hello world");
}

#[test]
fn static_const_multi_declaration_list() {
    let v = eval_expr(
        r#"
class URLS {
  static const String A = 'a';
  static const String B = 'b';
}
main() => URLS.A + URLS.B;
"#,
    );
    assert_eq!(str(&v), "ab");
}
