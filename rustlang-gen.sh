#!/data/data/com.termux/files/usr/bin/env bash
# rust_gen.sh - Utility to generate Rust project structures and code skeletons

set -euo pipefail

usage() {
  cat << EOF
Usage: $(basename "$0") COMMAND [OPTIONS]

Commands:
  init-lalrpop <project_name>   Create a Rust Cargo project configured with LALRPOP
  convert-py <file.py>          Generate a Rust FFI/PyO3 or struct skeleton from Python
  convert-c  <file.c|file.h>    Generate Rust 'extern "C"' FFI bindings skeleton from C header

Options:
  -h, --help                    Show this help menu
EOF
  exit 1
}

# --- Command: Init LALRPOP Project ---
init_lalrpop() {
  local proj_name="$1"
  echo "==> Creating new Cargo project with LALRPOP setup: ${proj_name}"
  
  cargo new "${proj_name}"
  cd "${proj_name}"

  # Update Cargo.toml
  cat << 'EOF' >> Cargo.toml

[build-dependencies]
lalrpop = "0.20"

[dependencies]
lalrpop-util = "0.20"
regex = "1.10"
EOF

  # Create build.rs
  cat << 'EOF' > build.rs
fn main() {
    lalrpop::process_root().unwrap();
}
EOF

  # Create grammar file
  mkdir -p src
  cat << 'EOF' > src/ast.rs
#[derive(Debug, PartialEq)]
pub enum Expr {
    Number(i32),
    Op(Box<Expr>, Opcode, Box<Expr>),
}

#[derive(Debug, PartialEq)]
pub enum Opcode {
    Add,
    Sub,
}
}
EOF

  cat << 'EOF' > src/parser.lalrpop
use std::str::FromStr;
use crate::ast::{Expr, Opcode};

grammar;

pub Expr: Box<Expr> = {
    l:Expr "+" r:Factor => Box::new(Expr::Op(l, Opcode::Add, r)),
    l:Expr "-" r:Factor => Box::new(Expr::Op(l, Opcode::Sub, r)),
    Factor,
};

Factor: Box<Expr> = {
    r"[0-9]+" => Box::new(Expr::Number(i32::from_str(<>).unwrap())),
    "(" <e:Expr> ")" => e,
};
EOF

  # Update main.rs
  cat << 'EOF' > src/main.rs
#[macro_use]
extern crate lalrpop_util;

pub mod ast;
lalrpop_mod!(pub parser);

fn main() {
    let expr = parser::ExprParser::new().parse("22 + 20").unwrap();
    println!("Parsed AST: {:?}", expr);
}
EOF

  echo "==> Done! Run 'cd ${proj_name} && cargo run' to test the LALRPOP parser."
}

# --- Command: Convert Python to PyO3 / Rust Skeleton ---
convert_py() {
  local py_file="$1"
  local rs_file="${py_file%.py}.rs"
  
  if [[ ! -f "$py_file" ]]; then
    echo "Error: File '$py_file' not found."
    exit 1
  fi

  echo "==> Converting '$py_file' to Rust/PyO3 skeleton '$rs_file'..."

  cat << EOF > "$rs_file"
// Auto-generated Rust wrapper skeleton for $py_file
use pyo3::prelude::*;

#[pyclass]
pub struct $(basename "${py_file%.py}" | sed -r 's/(^|_)([a-z])/\U\2/g') {
    // TODO: Map fields from $py_file
}

#[pymethods]
impl $(basename "${py_file%.py}" | sed -r 's/(^|_)([a-z])/\U\2/g') {
    #[new]
    fn new() -> Self {
        Self {}
    }
}

EOF

  # Extract Python function defs into Rust function stubs
  grep -E '^\s*def ' "$py_file" | while read -r line; do
    fn_name=$(echo "$line" | sed -E 's/^\s*def\s+([a-zA-Z0-9_]+).*/\1/')
    cat << EOF >> "$rs_file"
/// Skeleton for Python function: $fn_name
#[pyfunction]
pub fn ${fn_name}() -> PyResult<()> {
    // TODO: Implement converted logic
    Ok(())
}
EOF
  done

  echo "==> Output written to $rs_file"
}

# --- Command: Convert C to Rust FFI Skeleton ---
convert_c() {
  local c_file="$1"
  local rs_file="${c_file%.*}.rs"

  if [[ ! -f "$c_file" ]]; then
    echo "Error: File '$c_file' not found."
    exit 1
  fi

  echo "==> Converting '$c_file' headers/signatures to Rust FFI in '$rs_file'..."

  cat << EOF > "$rs_file"
// Auto-generated Rust FFI bindings skeleton for $c_file
use std::os::raw::{c_char, c_int, c_void};

#[link(name = "cothink_native")]
extern "C" {
EOF

  grep -E '^[a-zA-Z0-9_]+\s+\*?[a-zA-Z0-9_]+\(.*\)' "$c_file" | while read -r line; do
    echo "    // Source: $line" >> "$rs_file"
  done

  cat << EOF >> "$rs_file"
    // TODO: Replace with explicit bindgen declarations if needed
}
EOF

  echo "==> Output written to $rs_file"
}

# --- Main Entry Point ---
if [[ $# -lt 1 ]]; then
  usage
fi

case "$1" in
  init-lalrpop)
    [[ $# -ne 2 ]] && usage
    init_lalrpop "$2"
    ;;
  convert-py)
    [[ $# -ne 2 ]] && usage
    convert_py "$2"
    ;;
  convert-c)
    [[ $# -ne 2 ]] && usage
    convert_c "$2"
    ;;
  -h|--help)
    usage
    ;;
  *)
    echo "Unknown command: $1"
    usage
    ;;
esac
