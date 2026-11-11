//! Rust syntax analysis via Tree-sitter.

use crate::SyntaxError;
use bigolens_core::complexity::{constant, multiply, variable, ExprPtr};
use bigolens_core::ir::{LoopDirection, LoopNestIR, RecursiveFunctionIR, StepKind};
use bigolens_core::models::SourceLocation;
use tree_sitter::{Node, Parser};

/// Information about a detected Rust dynamic memory allocation.
#[derive(Debug, Clone, PartialEq)]
pub struct RustMemoryAlloc {
    pub location: SourceLocation,
    pub space_expr: ExprPtr,
    pub pattern: String,
    pub description: String,
    pub function_name: String,
}

/// Parse Rust source code and extract all top-level loop nests as `LoopNestIR`.
pub fn parse_rust_loops(source: &str, file_name: &str) -> Result<Vec<LoopNestIR>, SyntaxError> {
    let mut parser = Parser::new();
    let language = tree_sitter_rust::LANGUAGE.into();
    parser
        .set_language(&language)
        .map_err(|_| SyntaxError::ParseFailure)?;

    let tree = parser
        .parse(source, None)
        .ok_or(SyntaxError::ParseFailure)?;
    let root = tree.root_node();

    let mut loops = Vec::new();
    collect_top_level_loops(root, source.as_bytes(), file_name, &mut loops)?;
    Ok(loops)
}

fn collect_top_level_loops(
    node: Node,
    source: &[u8],
    file_name: &str,
    loops: &mut Vec<LoopNestIR>,
) -> Result<(), SyntaxError> {
    for child in node.children(&mut node.walk()) {
        if child.kind() == "for_expression"
            || child.kind() == "while_expression"
            || child.kind() == "loop_expression"
        {
            loops.push(parse_loop(child, source, file_name)?);
        } else {
            collect_top_level_loops(child, source, file_name, loops)?;
        }
    }
    Ok(())
}

fn parse_loop(node: Node, source: &[u8], file_name: &str) -> Result<LoopNestIR, SyntaxError> {
    match node.kind() {
        "for_expression" => parse_for_expression(node, source, file_name),
        "while_expression" => parse_while_expression(node, source, file_name),
        _ => {
            let start = node.start_position();
            let end = node.end_position();
            Ok(LoopNestIR {
                induction_var: Some("i".to_string()),
                lower_bound: Some(constant(0.0).unwrap()),
                upper_bound: Some(variable("n").unwrap()),
                step: StepKind::Linear(constant(1.0).unwrap()),
                direction: LoopDirection::Increasing,
                is_inclusive: false,
                location: SourceLocation {
                    file: file_name.to_string(),
                    line: (start.row + 1) as u32,
                    column: (start.column + 1) as u32,
                    end_line: Some((end.row + 1) as u32),
                    end_column: Some((end.column + 1) as u32),
                },
                inner_loops: Vec::new(),
                has_early_exit: false,
                bound_modified_in_body: false,
                index_modified_in_body: false,
            })
        }
    }
}

fn parse_for_expression(
    node: Node,
    source: &[u8],
    file_name: &str,
) -> Result<LoopNestIR, SyntaxError> {
    let start_pos = node.start_position();
    let end_pos = node.end_position();

    // in tree-sitter-rust for_expression:
    // pattern: identifier / pattern
    // value: range_expression or expression
    // body: block
    let mut var_name = "i".to_string();
    if let Some(pattern_node) = node.child_by_field_name("pattern") {
        if let Ok(text) = pattern_node.utf8_text(source) {
            var_name = text.trim().to_string();
        }
    }

    let mut lower = constant(0.0).unwrap();
    let mut upper = variable("n").unwrap();
    let mut is_inclusive = false;

    if let Some(value_node) = node.child_by_field_name("value") {
        let val_text = value_node.utf8_text(source).unwrap_or("");
        if val_text.contains("..=") {
            let parts: Vec<&str> = val_text.split("..=").collect();
            if parts.len() == 2 {
                lower = parse_expr_str(parts[0]);
                upper = parse_expr_str(parts[1]);
                is_inclusive = true;
            }
        } else if val_text.contains("..") {
            let parts: Vec<&str> = val_text.split("..").collect();
            if parts.len() == 2 {
                lower = parse_expr_str(parts[0]);
                upper = parse_expr_str(parts[1]);
                is_inclusive = false;
            }
        } else {
            // iterable
            let clean = val_text.trim_start_matches('&').trim();
            if clean.ends_with(".len()") {
                let base = clean.trim_end_matches(".len()");
                upper = variable(base).unwrap_or_else(|_| variable("n").unwrap());
            } else {
                upper = variable(clean).unwrap_or_else(|_| variable("n").unwrap());
            }
        }
    }

    let mut inner_loops = Vec::new();
    if let Some(body_node) = node.child_by_field_name("body") {
        collect_inner_loops(body_node, source, file_name, &mut inner_loops)?;
    }

    Ok(LoopNestIR {
        induction_var: Some(var_name),
        lower_bound: Some(lower),
        upper_bound: Some(upper),
        step: StepKind::Linear(constant(1.0).unwrap()),
        direction: LoopDirection::Increasing,
        is_inclusive,
        location: SourceLocation {
            file: file_name.to_string(),
            line: (start_pos.row + 1) as u32,
            column: (start_pos.column + 1) as u32,
            end_line: Some((end_pos.row + 1) as u32),
            end_column: Some((end_pos.column + 1) as u32),
        },
        inner_loops,
        has_early_exit: false,
        bound_modified_in_body: false,
        index_modified_in_body: false,
    })
}

fn parse_while_expression(
    node: Node,
    source: &[u8],
    file_name: &str,
) -> Result<LoopNestIR, SyntaxError> {
    let start_pos = node.start_position();
    let end_pos = node.end_position();

    let mut var_name = "n".to_string();
    let mut lower = constant(1.0).unwrap();
    let mut upper = variable("n").unwrap();
    let mut step = StepKind::GeometricDivide(2.0);
    let direction = LoopDirection::Decreasing;

    if let Some(cond_node) = node.child_by_field_name("condition") {
        let cond_text = cond_node.utf8_text(source).unwrap_or("");
        if cond_text.contains('>') {
            let parts: Vec<&str> = cond_text.split('>').collect();
            if let Some(first) = parts.first() {
                var_name = first
                    .trim_matches(|c: char| !c.is_alphanumeric() && c != '_')
                    .to_string();
                upper = variable(&var_name).unwrap_or_else(|_| variable("n").unwrap());
            }
        }
    }

    let mut inner_loops = Vec::new();
    if let Some(body_node) = node.child_by_field_name("body") {
        let body_text = body_node.utf8_text(source).unwrap_or("");
        if body_text.contains("/= 2") || body_text.contains("/=2") || body_text.contains(">>= 1") {
            step = StepKind::GeometricDivide(2.0);
        } else if body_text.contains("-= 1") {
            step = StepKind::Linear(constant(1.0).unwrap());
            lower = constant(0.0).unwrap();
        }
        collect_inner_loops(body_node, source, file_name, &mut inner_loops)?;
    }

    Ok(LoopNestIR {
        induction_var: Some(var_name),
        lower_bound: Some(lower),
        upper_bound: Some(upper),
        step,
        direction,
        is_inclusive: false,
        location: SourceLocation {
            file: file_name.to_string(),
            line: (start_pos.row + 1) as u32,
            column: (start_pos.column + 1) as u32,
            end_line: Some((end_pos.row + 1) as u32),
            end_column: Some((end_pos.column + 1) as u32),
        },
        inner_loops,
        has_early_exit: false,
        bound_modified_in_body: false,
        index_modified_in_body: false,
    })
}

fn collect_inner_loops(
    node: Node,
    source: &[u8],
    file_name: &str,
    inner_loops: &mut Vec<LoopNestIR>,
) -> Result<(), SyntaxError> {
    for child in node.children(&mut node.walk()) {
        if child.kind() == "for_expression"
            || child.kind() == "while_expression"
            || child.kind() == "loop_expression"
        {
            inner_loops.push(parse_loop(child, source, file_name)?);
        } else {
            collect_inner_loops(child, source, file_name, inner_loops)?;
        }
    }
    Ok(())
}

fn parse_expr_str(s: &str) -> ExprPtr {
    let trimmed = s.trim().trim_matches(|c: char| c == '(' || c == ')');
    if let Ok(v) = trimmed.parse::<f64>() {
        constant(v).unwrap()
    } else if trimmed.ends_with(".len()") {
        let base = trimmed.trim_end_matches(".len()");
        variable(base).unwrap_or_else(|_| variable("n").unwrap())
    } else {
        variable(trimmed).unwrap_or_else(|_| variable("n").unwrap())
    }
}

/// Parse Rust recursive functions into `RecursiveFunctionIR`.
pub fn parse_rust_recursion(
    source: &str,
    file_name: &str,
) -> Result<Vec<RecursiveFunctionIR>, SyntaxError> {
    let mut parser = Parser::new();
    let language = tree_sitter_rust::LANGUAGE.into();
    parser
        .set_language(&language)
        .map_err(|_| SyntaxError::ParseFailure)?;

    let tree = parser
        .parse(source, None)
        .ok_or(SyntaxError::ParseFailure)?;
    let root = tree.root_node();

    let mut recursions = Vec::new();
    collect_functions(root, source.as_bytes(), file_name, &mut recursions)?;
    Ok(recursions)
}

fn collect_functions(
    node: Node,
    source: &[u8],
    file_name: &str,
    recursions: &mut Vec<RecursiveFunctionIR>,
) -> Result<(), SyntaxError> {
    if node.kind() == "function_item" {
        if let Some(rec) = analyze_function_recursion(node, source, file_name)? {
            recursions.push(rec);
        }
    }

    for child in node.children(&mut node.walk()) {
        collect_functions(child, source, file_name, recursions)?;
    }
    Ok(())
}

fn analyze_function_recursion(
    func_node: Node,
    source: &[u8],
    file_name: &str,
) -> Result<Option<RecursiveFunctionIR>, SyntaxError> {
    let name_node = match func_node.child_by_field_name("name") {
        Some(n) => n,
        None => return Ok(None),
    };
    let func_name = match name_node.utf8_text(source) {
        Ok(name) => name,
        Err(_) => return Ok(None),
    };

    let mut param_name = "n".to_string();
    if let Some(params_node) = func_node.child_by_field_name("parameters") {
        for child in params_node.children(&mut params_node.walk()) {
            if child.kind() == "parameter" {
                if let Some(pat) = child.child_by_field_name("pattern") {
                    if let Ok(name) = pat.utf8_text(source) {
                        param_name = name.to_string();
                        break;
                    }
                }
            }
        }
    }

    let body_node = match func_node.child_by_field_name("body") {
        Some(b) => b,
        None => return Ok(None),
    };

    let mut call_nodes = Vec::new();
    collect_calls(body_node, func_name, source, &mut call_nodes);

    if call_nodes.is_empty() {
        return Ok(None);
    }

    let branch_factor = call_nodes.len() as u32;

    let mut reduction_divisor = 1.0;
    let mut reduction_step = 1.0;

    for call in &call_nodes {
        if let Some(args_node) = call.child_by_field_name("arguments") {
            let args_text = args_node.utf8_text(source).unwrap_or("");
            if args_text.contains('/') {
                if let Some((_, div_str)) = args_text.split_once('/') {
                    let div_clean = div_str
                        .trim_matches(|c: char| !c.is_ascii_digit())
                        .split_whitespace()
                        .next()
                        .unwrap_or("2");
                    if let Ok(d) = div_clean.parse::<f64>() {
                        reduction_divisor = d;
                    }
                }
            } else if args_text.contains('-') {
                if let Some((_, step_str)) = args_text.split_once('-') {
                    let step_clean = step_str
                        .trim_matches(|c: char| !c.is_ascii_digit())
                        .split_whitespace()
                        .next()
                        .unwrap_or("1");
                    if let Ok(s) = step_clean.parse::<f64>() {
                        reduction_step = s;
                    }
                }
            }
        }
    }

    // Inspect non-recursive work in the function body
    let mut has_loop = false;
    let mut loop_checker = |n: Node| {
        if n.kind() == "for_expression"
            || n.kind() == "while_expression"
            || n.kind() == "loop_expression"
        {
            has_loop = true;
        }
    };
    walk_node(body_node, &mut loop_checker);

    let work_exponent = if has_loop { 1.0 } else { 0.0 };
    let reduction_subtrahend = if reduction_divisor > 1.0 {
        None
    } else {
        Some(reduction_step)
    };

    let start_pos = func_node.start_position();
    let end_pos = func_node.end_position();

    Ok(Some(RecursiveFunctionIR {
        name: func_name.to_string(),
        parameter_name: param_name,
        branch_factor,
        reduction_divisor,
        reduction_subtrahend,
        work_exponent,
        location: SourceLocation {
            file: file_name.to_string(),
            line: (start_pos.row + 1) as u32,
            column: (start_pos.column + 1) as u32,
            end_line: Some((end_pos.row + 1) as u32),
            end_column: Some((end_pos.column + 1) as u32),
        },
    }))
}

fn collect_calls<'a>(node: Node<'a>, func_name: &str, source: &[u8], calls: &mut Vec<Node<'a>>) {
    if node.kind() == "call_expression" {
        if let Some(func_node) = node.child_by_field_name("function") {
            if let Ok(name) = func_node.utf8_text(source) {
                if name == func_name {
                    calls.push(node);
                }
            }
        }
    }

    for child in node.children(&mut node.walk()) {
        collect_calls(child, func_name, source, calls);
    }
}

fn walk_node<F: FnMut(Node)>(node: Node, f: &mut F) {
    f(node);
    for child in node.children(&mut node.walk()) {
        walk_node(child, f);
    }
}

/// Parse Rust dynamic allocations into `RustMemoryAlloc`.
pub fn parse_rust_memory_allocations(
    source: &str,
    file_name: &str,
) -> Result<Vec<RustMemoryAlloc>, SyntaxError> {
    let mut parser = Parser::new();
    let language = tree_sitter_rust::LANGUAGE.into();
    parser
        .set_language(&language)
        .map_err(|_| SyntaxError::ParseFailure)?;

    let tree = parser
        .parse(source, None)
        .ok_or(SyntaxError::ParseFailure)?;
    let root = tree.root_node();

    let mut allocations = Vec::new();
    collect_allocations(root, source.as_bytes(), file_name, &mut allocations)?;
    Ok(allocations)
}

fn collect_allocations(
    node: Node,
    source: &[u8],
    file_name: &str,
    allocs: &mut Vec<RustMemoryAlloc>,
) -> Result<(), SyntaxError> {
    if node.kind() == "macro_invocation" {
        // e.g. vec![0; n]
        let text = node.utf8_text(source).unwrap_or("");
        if text.starts_with("vec!") && text.contains(';') {
            let start = node.start_position();
            let end = node.end_position();
            let mut dim_var = "n".to_string();
            if let Some((_, rhs)) = text.split_once(';') {
                let clean = rhs.trim().trim_end_matches([']', ')', '}']);
                dim_var = clean.trim().to_string();
            }

            let var_expr = if let Ok(val) = dim_var.parse::<f64>() {
                constant(val).unwrap()
            } else {
                variable(&dim_var).unwrap_or_else(|_| variable("n").unwrap())
            };

            // Assuming 8-byte elements by default (e.g. i64/usize)
            let space_expr = multiply(vec![constant(8.0).unwrap(), var_expr]);

            allocs.push(RustMemoryAlloc {
                location: SourceLocation {
                    file: file_name.to_string(),
                    line: (start.row + 1) as u32,
                    column: (start.column + 1) as u32,
                    end_line: Some((end.row + 1) as u32),
                    end_column: Some((end.column + 1) as u32),
                },
                space_expr,
                pattern: "dynamic-vector".to_string(),
                description: "vector allocation via vec! macro (8 bytes per element)".to_string(),
                function_name: "main".to_string(),
            });
        }
    } else if node.kind() == "call_expression" {
        // e.g. Vec::with_capacity(n)
        let text = node.utf8_text(source).unwrap_or("");
        if text.contains("Vec::with_capacity") {
            let start = node.start_position();
            let end = node.end_position();
            let mut dim_var = "n".to_string();
            if let Some(args_node) = node.child_by_field_name("arguments") {
                let arg_text = args_node.utf8_text(source).unwrap_or("");
                let clean = arg_text.trim_matches(|c: char| c == '(' || c == ')').trim();
                dim_var = clean.to_string();
            }

            let var_expr = if let Ok(val) = dim_var.parse::<f64>() {
                constant(val).unwrap()
            } else {
                variable(&dim_var).unwrap_or_else(|_| variable("n").unwrap())
            };

            let space_expr = multiply(vec![constant(8.0).unwrap(), var_expr]);

            allocs.push(RustMemoryAlloc {
                location: SourceLocation {
                    file: file_name.to_string(),
                    line: (start.row + 1) as u32,
                    column: (start.column + 1) as u32,
                    end_line: Some((end.row + 1) as u32),
                    end_column: Some((end.column + 1) as u32),
                },
                space_expr,
                pattern: "dynamic-vector".to_string(),
                description: "vector allocation via Vec::with_capacity (8 bytes per element)"
                    .to_string(),
                function_name: "main".to_string(),
            });
        }
    }

    for child in node.children(&mut node.walk()) {
        collect_allocations(child, source, file_name, allocs)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rust_nested_triangular_loop() {
        let code = r#"
fn solve(n: usize) {
    for i in 0..n {
        for j in 0..i {
            println!("{} {}", i, j);
        }
    }
}
"#;
        let loops = parse_rust_loops(code, "solution.rs").unwrap();
        assert_eq!(loops.len(), 1);
        let outcome = loops[0].analyze();
        assert_eq!(outcome.pattern, "triangular-loop");
        assert_eq!(outcome.total_cost.unwrap().to_string(), "n^2");
    }

    #[test]
    fn test_rust_while_halving_loop() {
        let code = r#"
fn log_work(mut n: usize) {
    while n > 0 {
        n /= 2;
    }
}
"#;
        let loops = parse_rust_loops(code, "solution.rs").unwrap();
        assert_eq!(loops.len(), 1);
        let outcome = loops[0].analyze();
        assert_eq!(outcome.pattern, "logarithmic-loop");
        assert_eq!(outcome.total_cost.unwrap().to_string(), "log_2(n)");
    }

    #[test]
    fn test_rust_recursion_merge_sort() {
        let code = r#"
fn merge_sort(n: usize) {
    if n <= 1 { return; }
    for _ in 0..n {
        // merge step
    }
    merge_sort(n / 2);
    merge_sort(n / 2);
}
"#;
        let recs = parse_rust_recursion(code, "solution.rs").unwrap();
        assert_eq!(recs.len(), 1);
        assert_eq!(recs[0].branch_factor, 2);
        assert_eq!(recs[0].reduction_divisor, 2.0);
        let (cost, status, conf, _) = recs[0].solve();
        assert_eq!(cost.asymptotic_bound(), "O(n * log_2(n))");
        assert_eq!(status, bigolens_core::models::BoundStatus::Proven);
        assert_eq!(conf, bigolens_core::models::Confidence::High);
    }

    #[test]
    fn test_rust_memory_allocation() {
        let code = r#"
fn setup(n: usize) {
    let buf = vec![0; n];
}
"#;
        let allocs = parse_rust_memory_allocations(code, "solution.rs").unwrap();
        assert_eq!(allocs.len(), 1);
        assert_eq!(allocs[0].space_expr.to_string(), "8 * n");
    }
}
