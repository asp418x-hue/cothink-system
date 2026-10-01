use crate::error::InterpError;

pub type NodeId = usize;

pub struct AstNode {
    pub kind: &'static str,
    pub field: Option<&'static str>,
    pub is_named: bool,
    pub start: usize,
    pub end: usize,
    pub children: Vec<NodeId>,
}

pub struct CompilationUnit {
    pub source: String,
    pub nodes: Vec<AstNode>,
    pub parents: Vec<Option<NodeId>>,
}

impl CompilationUnit {
    pub fn parse(source: &str) -> Result<CompilationUnit, InterpError> {
        let unit = Self::build(source)?;
        let fixes = ambiguous_lambda_bodies(&unit);
        if fixes.is_empty() {
            return Ok(unit);
        }
        let mut rewritten = unit.source.clone();
        for &(s, e) in fixes.iter().rev() {
            rewritten.insert(e, ')');
            rewritten.insert(s, '(');
        }
        Self::build(&rewritten)
    }

    pub fn build(source: &str) -> Result<CompilationUnit, InterpError> {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_dart::LANGUAGE.into())
            .map_err(|e| InterpError::SourceCode(format!("Failed to load Dart grammar: {e}")))?;
        let tree = parser
            .parse(source, None)
            .ok_or_else(|| InterpError::SourceCode("Failed to parse source".to_string()))?;
        let root = tree.root_node();
        if root.has_error() {
            let node = find_first_error(root).unwrap_or(root);
            let start = node.start_byte();
            let (line, col) = source_line_col(source, start);
            let kind = if node.is_missing() {
                "missing token".to_string()
            } else {
                node.kind().to_string()
            };
            return Err(InterpError::SourceCode(format!(
                "Syntax error in Dart code at line {line}, column {col}: unexpected {kind}"
            )));
        }

        let mut unit = CompilationUnit {
            source: source.to_string(),
            nodes: Vec::new(),
            parents: Vec::new(),
        };
        convert_node(root, None, &mut unit);
        Ok(unit)
    }

    pub fn node(&self, id: NodeId) -> &AstNode {
        &self.nodes[id]
    }

    pub fn kind(&self, id: NodeId) -> &'static str {
        self.nodes[id].kind
    }

    pub fn text(&self, id: NodeId) -> &str {
        let n = &self.nodes[id];
        &self.source[n.start..n.end]
    }

    pub fn children(&self, id: NodeId) -> &[NodeId] {
        &self.nodes[id].children
    }

    pub fn child(&self, id: NodeId, index: usize) -> Option<NodeId> {
        self.nodes[id].children.get(index).copied()
    }

    pub fn parent(&self, id: NodeId) -> Option<NodeId> {
        self.parents[id]
    }

    pub fn child_by_field(&self, id: NodeId, field: &str) -> Option<NodeId> {
        self.nodes[id]
            .children
            .iter()
            .copied()
            .find(|&c| self.nodes[c].field == Some(field))
    }

    pub fn children_of_kind(&self, id: NodeId, kind: &str) -> Vec<NodeId> {
        self.nodes[id]
            .children
            .iter()
            .copied()
            .filter(|&c| self.nodes[c].kind == kind)
            .collect()
    }

    pub fn first_child_of_kind(&self, id: NodeId, kind: &str) -> Option<NodeId> {
        self.nodes[id]
            .children
            .iter()
            .copied()
            .find(|&c| self.nodes[c].kind == kind)
    }

    pub fn last_child_of_kind(&self, id: NodeId, kind: &str) -> Option<NodeId> {
        self.nodes[id]
            .children
            .iter()
            .rev()
            .copied()
            .find(|&c| self.nodes[c].kind == kind)
    }

    pub fn named_children(&self, id: NodeId) -> Vec<NodeId> {
        self.nodes[id]
            .children
            .iter()
            .copied()
            .filter(|&c| self.nodes[c].is_named)
            .collect()
    }

    pub fn line_col(&self, byte: usize) -> (usize, usize) {
        source_line_col(&self.source, byte)
    }
}

fn source_line_col(source: &str, byte: usize) -> (usize, usize) {
    let up_to = &source[..byte.min(source.len())];
    let line = up_to.bytes().filter(|&b| b == b'\n').count() + 1;
    let col = byte - up_to.rfind('\n').map(|i| i + 1).unwrap_or(0);
    (line, col + 1)
}

fn convert_node(
    node: tree_sitter::Node,
    parent: Option<NodeId>,
    unit: &mut CompilationUnit,
) -> NodeId {
    let id = unit.nodes.len();
    unit.nodes.push(AstNode {
        kind: node.kind(),
        field: None,
        is_named: node.is_named(),
        start: node.start_byte(),
        end: node.end_byte(),
        children: Vec::new(),
    });
    unit.parents.push(parent);
    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            let child = cursor.node();
            let cid = convert_node(child, Some(id), unit);
            unit.nodes[cid].field = cursor.field_name();
            unit.nodes[id].children.push(cid);
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
    id
}

/// The Dart grammar parses arrow-lambda bodies that end in a member/call chain
/// ambiguously: `(s) => s.toUpperCase()` becomes `((s) => s).toUpperCase()`,
/// `(s) => f(s)` becomes `((s) => f)(s)` and `(x) => x[0]` becomes
/// `((x) => x)[0]`. Detect those cases and return the (start, end) byte ranges
/// of the intended body expression, which `parse` wraps in parentheses so the
/// re-parse resolves the ambiguity.
fn ambiguous_lambda_bodies(unit: &CompilationUnit) -> Vec<(usize, usize)> {
    let mut fixes: Vec<(usize, usize)> = Vec::new();
    for i in 0..unit.nodes.len() {
        let kind = unit.nodes[i].kind;
        let fn_node: Option<usize> = match kind {
            "call_expression" => unit
                .child_by_field(i, "function")
                .filter(|&c| unit.nodes[c].kind == "function_expression"),
            "member_expression"
            | "null_aware_member_expression"
            | "index_expression"
            | "null_aware_index_expression" => unit
                .child_by_field(i, "object")
                .filter(|&c| unit.nodes[c].kind == "function_expression"),
            _ => None,
        };
        let Some(fn_node) = fn_node else {
            continue;
        };
        let Some(body) = unit
            .first_child_of_kind(fn_node, "function_expression_body")
            .and_then(|b| unit.named_children(b).first().copied())
        else {
            continue;
        };
        let start = unit.nodes[body].start;
        let mut cur = i;
        while let Some(p) = unit.parents[cur] {
            let pk = unit.nodes[p].kind;
            let in_chain = match pk {
                "call_expression" => unit.child_by_field(p, "function").is_some_and(|f| f == cur),
                "member_expression"
                | "null_aware_member_expression"
                | "index_expression"
                | "null_aware_index_expression" => {
                    unit.child_by_field(p, "object").is_some_and(|o| o == cur)
                }
                "null_assertion_expression" => {
                    unit.child_by_field(p, "value").is_some_and(|v| v == cur)
                }
                _ => false,
            };
            if in_chain {
                cur = p;
            } else {
                break;
            }
        }
        let end = unit.nodes[cur].end;
        if !fixes.iter().any(|&(fs, fe)| fs == start && fe == end) {
            fixes.push((start, end));
        }
    }
    fixes
}

fn find_first_error(node: tree_sitter::Node) -> Option<tree_sitter::Node> {
    if node.is_error() || node.is_missing() {
        return Some(node);
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if let Some(e) = find_first_error(child) {
            return Some(e);
        }
    }
    None
}
