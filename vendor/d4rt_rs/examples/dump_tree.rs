use d4rt_rs::ast::CompilationUnit;

fn dump(unit: &CompilationUnit, id: d4rt_rs::ast::NodeId, depth: usize) {
    let kind = unit.kind(id);
    let text = unit.text(id).trim();
    let snippet: String = text.chars().take(30).collect();
    println!(
        "{}{} {:?} [{}]",
        "  ".repeat(depth),
        kind,
        unit.child_by_field(id, "field_name"),
        snippet
    );
    for c in unit.children(id) {
        if unit.kind(*c) == "comment" {
            continue;
        }
        dump(unit, *c, depth + 1);
    }
}

fn main() {
    let src = r#"
import 'dart:async';

Stream<int> countAsync() async* {
  yield 1;
  yield 2;
}

Future<List<int>> main() async {
  List<int> results = [];
  await for (var value in countAsync()) {
    results.add(value);
  }
  return results;
}
"#;
    let unit = CompilationUnit::parse(src).unwrap();
    let root = 0usize;
    dump(&unit, root, 0);
}
