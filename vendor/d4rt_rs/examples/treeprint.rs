fn main() {
    let src = std::fs::read_to_string(std::env::args().nth(1).unwrap()).unwrap();
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_dart::LANGUAGE.into())
        .unwrap();
    let tree = parser.parse(&src, None).unwrap();
    let mut stack = vec![(tree.root_node(), 0usize)];
    while let Some((node, depth)) = stack.pop() {
        let ind = "  ".repeat(depth);
        let mut s = format!("{ind}{}", node.kind());
        if let Some(name) = node.field_name_for_child(0) {
            let _ = name;
        }
        // collect fields
        let mut cursor = node.walk();
        let mut fields: Vec<String> = Vec::new();
        if cursor.goto_first_child() {
            loop {
                if let Some(f) = cursor.field_name() {
                    fields.push(format!("@{}:{}", f, cursor.node().kind()));
                }
                if !cursor.goto_next_sibling() {
                    break;
                }
            }
        }
        if !fields.is_empty() {
            s.push_str(&format!("  [{}]", fields.join(", ")));
        }
        println!("{s}");
        let mut kids: Vec<_> = node.children(&mut node.walk()).collect();
        kids.reverse();
        for k in kids {
            stack.push((k, depth + 1));
        }
    }
}
