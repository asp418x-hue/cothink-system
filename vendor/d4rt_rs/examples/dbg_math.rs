use d4rt_rs::Context;
fn main() {
    let mut c = Context::new();
    for src in [
        "import 'dart:collection'; void main() { return HashMap(); }",
        "import 'dart:collection'; void main() { return new HashMap(); }",
    ] {
        match c.eval(src) {
            Ok(v) => println!("OK {v:?}"),
            Err(e) => println!("ERR {:?}", e.message()),
        }
    }
}
