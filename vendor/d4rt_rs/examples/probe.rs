use d4rt_rs::Context;
fn main() {
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
    match c.eval(src) {
        Ok(v) => println!("OK {v:?}"),
        Err(e) => println!("FAIL {e:?}"),
    }
}
