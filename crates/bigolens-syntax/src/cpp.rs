//! C++ syntax analysis via Tree-sitter.

use crate::SyntaxError;
use bigolens_core::complexity::{constant, multiply, variable, ExprPtr};
use bigolens_core::ir::{LoopDirection, LoopNestIR, RecursiveFunctionIR, StepKind};
use bigolens_core::models::SourceLocation;
use tree_sitter::{Node, Parser};

/// Information about a detected C++ dynamic memory allocation.
#[derive(Debug, Clone, PartialEq)]
pub struct CppMemoryAlloc {
    pub location: SourceLocation,
    pub space_expr: ExprPtr,
    pub pattern: String,
    pub description: String,
    pub function_name: String,
}

/// Parse C++ source code and extract all top-level loop nests as `LoopNestIR`.
pub fn parse_cpp_loops(source: &str, file_name: &str) -> Result<Vec<LoopNestIR>, SyntaxError> {
    let mut parser = Parser::new();
    let language = tree_sitter_cpp::LANGUAGE.into();
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
        if child.kind() == "for_statement" || child.kind() == "while_statement" {
            loops.push(parse_loop(child, source, file_name)?);
        } else {
            collect_top_level_loops(child, source, file_name, loops)?;
        }
    }
    Ok(())
}

fn parse_loop(node: Node, source: &[u8], file_name: &str) -> Result<LoopNestIR, SyntaxError> {
    if node.kind() == "for_statement" {
        parse_for_loop(node, source, file_name)
    } else {
        let start_pos = node.start_position();
        let end_pos = node.end_position();
        Ok(LoopNestIR {
            induction_var: Some("i".to_string()),
            lower_bound: Some(constant(0.0).unwrap()),
            upper_bound: Some(variable("n").unwrap()),
            step: StepKind::Linear(constant(1.0).unwrap()),
            direction: LoopDirection::Increasing,
            is_inclusive: false,
            location: SourceLocation {
                file: file_name.to_string(),
                line: (start_pos.row + 1) as u32,
                column: (start_pos.column + 1) as u32,
                end_line: Some((end_pos.row + 1) as u32),
                end_column: Some((end_pos.column + 1) as u32),
            },
            inner_loops: Vec::new(),
            has_early_exit: false,
            bound_modified_in_body: false,
            index_modified_in_body: false,
        })
    }
}

fn parse_for_loop(node: Node, source: &[u8], file_name: &str) -> Result<LoopNestIR, SyntaxError> {
    let mut cursor = node.walk();
    let children: Vec<Node> = node.children(&mut cursor).collect();

    // In tree-sitter-cpp, for_statement typically has fields or children:
    // for ( initializer ; condition ; update ) body
    // Let's extract initializer, condition, update, body
    let mut initializer = node.child_by_field_name("initializer");
    let mut condition = node.child_by_field_name("condition");
    let mut update = node.child_by_field_name("update");
    let body = node.child_by_field_name("body");

    // If field names didn't capture them (fallback by scanning between parentheses):
    let mut paren_depth = 0;
    let mut parts: Vec<Vec<Node>> = vec![Vec::new()];
    for child in &children {
        if child.kind() == "(" {
            paren_depth += 1;
            continue;
        } else if child.kind() == ")" {
            paren_depth -= 1;
            continue;
        }
        if paren_depth > 0 {
            if child.kind() == ";" {
                parts.push(Vec::new());
            } else {
                if let Some(last) = parts.last_mut() {
                    last.push(*child);
                }
            }
        }
    }

    if initializer.is_none() && !parts.is_empty() && !parts[0].is_empty() {
        initializer = Some(parts[0][0]);
    }
    if condition.is_none() && parts.len() > 1 && !parts[1].is_empty() {
        condition = Some(parts[1][0]);
    }
    if update.is_none() && parts.len() > 2 && !parts[2].is_empty() {
        update = Some(parts[2][0]);
    }

    let (loop_var, lower_bound) = parse_cpp_init(initializer, source)?;
    let (upper_bound, is_inclusive) = parse_cpp_condition(condition, &loop_var, source)?;
    let (step, direction) = parse_cpp_update(update, source)?;

    let mut inner_loops = Vec::new();
    if let Some(b) = body {
        collect_inner_loops(b, source, file_name, &mut inner_loops)?;
    }

    let start_pos = node.start_position();
    let end_pos = node.end_position();

    Ok(LoopNestIR {
        induction_var: Some(loop_var),
        lower_bound: Some(lower_bound),
        upper_bound: Some(upper_bound),
        step,
        direction,
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

fn collect_inner_loops(
    node: Node,
    source: &[u8],
    file_name: &str,
    inner_loops: &mut Vec<LoopNestIR>,
) -> Result<(), SyntaxError> {
    for child in node.children(&mut node.walk()) {
        if child.kind() == "for_statement" || child.kind() == "while_statement" {
            inner_loops.push(parse_loop(child, source, file_name)?);
        } else {
            collect_inner_loops(child, source, file_name, inner_loops)?;
        }
    }
    Ok(())
}

fn parse_cpp_init(node: Option<Node>, source: &[u8]) -> Result<(String, ExprPtr), SyntaxError> {
    if let Some(n) = node {
        // e.g. int i = 0 or i = 0
        if n.kind() == "declaration" {
            // Find init_declarator
            for child in n.children(&mut n.walk()) {
                if child.kind() == "init_declarator" {
                    let decl = child.child_by_field_name("declarator");
                    let val = child.child_by_field_name("value");
                    let var_name = decl
                        .and_then(|d| d.utf8_text(source).ok())
                        .unwrap_or("i")
                        .to_string();
                    let bound = if let Some(v) = val {
                        parse_expr(v, source)?
                    } else {
                        constant(0.0).unwrap()
                    };
                    return Ok((var_name, bound));
                }
            }
        } else if n.kind() == "assignment_expression" {
            let left = n.child_by_field_name("left");
            let right = n.child_by_field_name("right");
            let var_name = left
                .and_then(|l| l.utf8_text(source).ok())
                .unwrap_or("i")
                .to_string();
            let bound = if let Some(r) = right {
                parse_expr(r, source)?
            } else {
                constant(0.0).unwrap()
            };
            return Ok((var_name, bound));
        }
    }
    Ok(("i".to_string(), constant(0.0).unwrap()))
}

fn parse_cpp_condition(
    node: Option<Node>,
    induction_var: &str,
    source: &[u8],
) -> Result<(ExprPtr, bool), SyntaxError> {
    if let Some(n) = node {
        if n.kind() == "binary_expression" {
            let left = n.child_by_field_name("left");
            let operator = n.child_by_field_name("operator");
            let right = n.child_by_field_name("right");

            let op_str = operator
                .and_then(|op| op.utf8_text(source).ok())
                .or_else(|| {
                    // fallback: operator might be an unnamed child
                    for c in n.children(&mut n.walk()) {
                        let text = c.utf8_text(source).unwrap_or("");
                        if text == "<"
                            || text == "<="
                            || text == ">"
                            || text == ">="
                            || text == "!="
                        {
                            return Some(text);
                        }
                    }
                    None
                })
                .unwrap_or("<");

            let left_text = left.and_then(|l| l.utf8_text(source).ok()).unwrap_or("");
            let (target_node, is_inclusive) = if left_text == induction_var {
                (right, op_str == "<=")
            } else {
                (left, op_str == ">=")
            };

            if let Some(tn) = target_node {
                return Ok((parse_expr(tn, source)?, is_inclusive));
            }
        }
    }
    Ok((variable("n").unwrap(), false))
}

fn parse_cpp_update(
    node: Option<Node>,
    source: &[u8],
) -> Result<(StepKind, LoopDirection), SyntaxError> {
    if let Some(n) = node {
        let text = n.utf8_text(source).unwrap_or("");
        if text.contains("++") {
            return Ok((
                StepKind::Linear(constant(1.0).unwrap()),
                LoopDirection::Increasing,
            ));
        } else if text.contains("--") {
            return Ok((
                StepKind::Linear(constant(1.0).unwrap()),
                LoopDirection::Decreasing,
            ));
        } else if n.kind() == "assignment_expression" {
            let operator = n
                .child_by_field_name("operator")
                .and_then(|o| o.utf8_text(source).ok())
                .unwrap_or("=");
            let right = n.child_by_field_name("right");
            let step_val = right
                .and_then(|r| r.utf8_text(source).ok())
                .and_then(|t| t.parse::<f64>().ok())
                .unwrap_or(1.0);

            match operator {
                "+=" => {
                    return Ok((
                        StepKind::Linear(constant(step_val).unwrap()),
                        LoopDirection::Increasing,
                    ));
                }
                "-=" => {
                    return Ok((
                        StepKind::Linear(constant(step_val).unwrap()),
                        LoopDirection::Decreasing,
                    ));
                }
                "*=" => {
                    return Ok((
                        StepKind::GeometricMultiply(step_val),
                        LoopDirection::Increasing,
                    ));
                }
                "/=" => {
                    return Ok((
                        StepKind::GeometricDivide(step_val),
                        LoopDirection::Increasing,
                    ));
                }
                _ => {}
            }
        }
    }
    Ok((
        StepKind::Linear(constant(1.0).unwrap()),
        LoopDirection::Increasing,
    ))
}

fn parse_expr(node: Node, source: &[u8]) -> Result<ExprPtr, SyntaxError> {
    let text = node.utf8_text(source).unwrap_or("0");
    if let Ok(val) = text.parse::<f64>() {
        Ok(constant(val).unwrap())
    } else {
        Ok(variable(text).unwrap_or_else(|_| constant(0.0).unwrap()))
    }
}

/// Parse C++ recursive functions into `RecursiveFunctionIR`.
pub fn parse_cpp_recursion(
    source: &str,
    file_name: &str,
) -> Result<Vec<RecursiveFunctionIR>, SyntaxError> {
    let mut parser = Parser::new();
    let language = tree_sitter_cpp::LANGUAGE.into();
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
    if node.kind() == "function_definition" {
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
    let decl_node = match func_node.child_by_field_name("declarator") {
        Some(d) => d,
        None => return Ok(None),
    };

    let mut func_name = String::new();
    let mut param_name = "n".to_string();

    let mut cursor = decl_node.walk();
    for child in decl_node.children(&mut cursor) {
        if child.kind() == "identifier" || child.kind() == "field_identifier" {
            if let Ok(name) = child.utf8_text(source) {
                func_name = name.to_string();
            }
        } else if child.kind() == "parameter_list" {
            for param in child.children(&mut child.walk()) {
                if param.kind() == "parameter_declaration" {
                    if let Some(p_decl) = param.child_by_field_name("declarator") {
                        if let Ok(name) = p_decl.utf8_text(source) {
                            param_name = name.to_string();
                            break;
                        }
                    }
                }
            }
        }
    }

    if func_name.is_empty() {
        // Fallback: search for identifier in declarator tree
        let mut found_id = None;
        let mut id_finder = |n: Node| {
            if n.kind() == "identifier" && found_id.is_none() {
                if let Ok(name) = n.utf8_text(source) {
                    found_id = Some(name.to_string());
                }
            }
        };
        walk_node(decl_node, &mut id_finder);
        func_name = match found_id {
            Some(n) => n,
            None => return Ok(None),
        };
    }

    let body_node = match func_node.child_by_field_name("body") {
        Some(b) => b,
        None => return Ok(None),
    };

    let mut call_nodes = Vec::new();
    collect_calls(body_node, &func_name, source, &mut call_nodes);

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

    let mut has_loop = false;
    let mut loop_checker = |n: Node| {
        if n.kind() == "for_statement" || n.kind() == "while_statement" {
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
        name: func_name,
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

/// Parse C++ dynamic allocations into `CppMemoryAlloc`.
pub fn parse_cpp_memory_allocations(
    source: &str,
    file_name: &str,
) -> Result<Vec<CppMemoryAlloc>, SyntaxError> {
    let mut parser = Parser::new();
    let language = tree_sitter_cpp::LANGUAGE.into();
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
    allocs: &mut Vec<CppMemoryAlloc>,
) -> Result<(), SyntaxError> {
    if node.kind() == "new_expression" {
        // e.g. new int[n]
        let text = node.utf8_text(source).unwrap_or("");
        let elem_size = if text.contains("long long") || text.contains("double") {
            8.0
        } else if text.contains("char") || text.contains("bool") {
            1.0
        } else if text.contains("short") {
            2.0
        } else {
            4.0
        };

        let mut dim_var = "n".to_string();
        if let Some((_, rhs)) = text.split_once('[') {
            let clean = rhs.trim_end_matches(']').trim();
            if !clean.is_empty() {
                dim_var = clean.to_string();
            }
        }

        let var_expr = if let Ok(val) = dim_var.parse::<f64>() {
            constant(val).unwrap()
        } else {
            variable(&dim_var).unwrap_or_else(|_| variable("n").unwrap())
        };

        let space_expr = multiply(vec![constant(elem_size).unwrap(), var_expr]);
        let start = node.start_position();
        let end = node.end_position();

        allocs.push(CppMemoryAlloc {
            location: SourceLocation {
                file: file_name.to_string(),
                line: (start.row + 1) as u32,
                column: (start.column + 1) as u32,
                end_line: Some((end.row + 1) as u32),
                end_column: Some((end.column + 1) as u32),
            },
            space_expr,
            pattern: "dynamic-array".to_string(),
            description: format!(
                "heap allocation via new with element size {} bytes",
                elem_size
            ),
            function_name: "main".to_string(),
        });
    }

    for child in node.children(&mut node.walk()) {
        collect_allocations(child, source, file_name, allocs)?;
    }
    Ok(())
}

/// Information about a detected C++ STL algorithm call.
#[derive(Debug, Clone, PartialEq)]
pub struct CppStlCall {
    pub location: SourceLocation,
    pub cost_expr: ExprPtr,
    pub pattern: String,
    pub description: String,
    pub function_name: String,
    pub asymptotic: String,
}

/// Parse C++ STL algorithm calls (e.g. std::sort).
pub fn parse_cpp_stl_calls(source: &str, file_name: &str) -> Result<Vec<CppStlCall>, SyntaxError> {
    let mut parser = Parser::new();
    let language = tree_sitter_cpp::LANGUAGE.into();
    parser
        .set_language(&language)
        .map_err(|_| SyntaxError::ParseFailure)?;

    let tree = parser
        .parse(source, None)
        .ok_or(SyntaxError::ParseFailure)?;
    let root = tree.root_node();

    let mut calls = Vec::new();
    collect_stl_calls(root, source.as_bytes(), file_name, &mut calls)?;
    Ok(calls)
}

fn collect_stl_calls(
    node: Node,
    source: &[u8],
    file_name: &str,
    calls: &mut Vec<CppStlCall>,
) -> Result<(), SyntaxError> {
    if node.kind() == "call_expression" {
        if let Some(func_node) = node.child_by_field_name("function") {
            let func_name = func_node.utf8_text(source).unwrap_or("");
            if func_name == "std::sort" || func_name == "sort" {
                let start = node.start_position();
                let end = node.end_position();
                let n = variable("n").unwrap_or_else(|_| constant(1.0).unwrap());
                let log_n = bigolens_core::complexity::logarithm(n.clone(), 2.0)
                    .unwrap_or_else(|_| constant(1.0).unwrap());
                let cost_expr = multiply(vec![n, log_n]);
                calls.push(CppStlCall {
                    location: SourceLocation {
                        file: file_name.to_string(),
                        line: (start.row + 1) as u32,
                        column: (start.column + 1) as u32,
                        end_line: Some((end.row + 1) as u32),
                        end_column: Some((end.column + 1) as u32),
                    },
                    cost_expr,
                    pattern: "stl-algorithm".to_string(),
                    description: "std::sort standard algorithm call".to_string(),
                    function_name: "main".to_string(),
                    asymptotic: "O(n log n)".to_string(),
                });
            }
        }
    }

    for child in node.children(&mut node.walk()) {
        collect_stl_calls(child, source, file_name, calls)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cpp_for_loop() {
        let code = r#"
int main() {
    int sum = 0;
    for (int i = 0; i < n; ++i) {
        sum += i;
    }
    return 0;
}
"#;
        let loops = parse_cpp_loops(code, "test.cpp").unwrap();
        assert_eq!(loops.len(), 1);
        let lp = &loops[0];
        assert_eq!(lp.induction_var.as_deref(), Some("i"));
        assert_eq!(lp.direction, LoopDirection::Increasing);
        let outcome = lp.analyze();
        assert_eq!(outcome.total_cost.unwrap().asymptotic_bound(), "O(n)");
    }

    #[test]
    fn test_cpp_nested_loops() {
        let code = r#"
void matrix(int n, int m) {
    for (int i = 0; i < n; ++i) {
        for (int j = 0; j < m; ++j) {
            do_work(i, j);
        }
    }
}
"#;
        let loops = parse_cpp_loops(code, "matrix.cpp").unwrap();
        assert_eq!(loops.len(), 1);
        assert_eq!(loops[0].inner_loops.len(), 1);
        let outcome = loops[0].analyze();
        assert_eq!(outcome.total_cost.unwrap().asymptotic_bound(), "O(n * m)");
    }

    #[test]
    fn test_cpp_triangular_loop() {
        let code = r#"
int main() {
    int pairs = 0;
    for (int i = 0; i < n; ++i) {
        for (int j = 0; j < i; ++j) {
            ++pairs;
        }
    }
    return 0;
}
"#;
        let loops = parse_cpp_loops(code, "triangular.cpp").unwrap();
        assert_eq!(loops.len(), 1);
        let outcome = loops[0].analyze();
        assert_eq!(outcome.pattern, "triangular-loop");
        assert_eq!(outcome.total_cost.unwrap().asymptotic_bound(), "O(n^2)");
    }

    #[test]
    fn test_cpp_recursion_merge_sort() {
        let code = r#"
void merge_sort(int n) {
    if (n <= 1) return;
    for (int i = 0; i < n; ++i) {
        // merge step
    }
    merge_sort(n / 2);
    merge_sort(n / 2);
}
"#;
        let recs = parse_cpp_recursion(code, "merge_sort.cpp").unwrap();
        assert_eq!(recs.len(), 1);
        assert_eq!(recs[0].branch_factor, 2);
        assert_eq!(recs[0].reduction_divisor, 2.0);
        let (cost, status, conf, _) = recs[0].solve();
        assert_eq!(cost.asymptotic_bound(), "O(n * log_2(n))");
        assert_eq!(status, bigolens_core::models::BoundStatus::Proven);
        assert_eq!(conf, bigolens_core::models::Confidence::High);
    }

    #[test]
    fn test_cpp_memory_allocation() {
        let code = r#"
int main() {
    int* arr = new int[n];
    return 0;
}
"#;
        let allocs = parse_cpp_memory_allocations(code, "alloc.cpp").unwrap();
        assert_eq!(allocs.len(), 1);
        assert_eq!(allocs[0].space_expr.to_string(), "4 * n");
    }
}
