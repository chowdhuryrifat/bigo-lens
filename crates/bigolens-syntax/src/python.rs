//! Python syntax analysis via Tree-sitter.

use crate::SyntaxError;
use bigolens_core::complexity::{constant, multiply, variable, ExprPtr};
use bigolens_core::ir::{LoopDirection, LoopNestIR, RecursiveFunctionIR, StepKind};
use bigolens_core::models::SourceLocation;
use std::collections::HashSet;
use tree_sitter::{Node, Parser};

/// Information about a detected Python dynamic memory allocation.
#[derive(Debug, Clone, PartialEq)]
pub struct PythonMemoryAlloc {
    pub location: SourceLocation,
    pub space_expr: ExprPtr,
    pub pattern: String,
    pub description: String,
    pub function_name: String,
}

/// Parse Python source code and extract all top-level loop nests as `LoopNestIR`.
pub fn parse_python_loops(source: &str, file_name: &str) -> Result<Vec<LoopNestIR>, SyntaxError> {
    let mut parser = Parser::new();
    let language = tree_sitter_python::LANGUAGE.into();
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
        // Fallback for while or other loops
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
    // for <left> in <right>: <body>
    let left = node.child_by_field_name("left");
    let right = node.child_by_field_name("right");
    let body = node.child_by_field_name("body");

    let loop_var = if let Some(l) = left {
        l.utf8_text(source).unwrap_or("i").to_string()
    } else {
        "i".to_string()
    };

    let (start, end, step, direction) = if let Some(r) = right {
        parse_range_call(r, source)?
    } else {
        (
            constant(0.0).unwrap(),
            variable("n").unwrap(),
            StepKind::Linear(constant(1.0).unwrap()),
            LoopDirection::Increasing,
        )
    };

    let mut inner_loops = Vec::new();
    if let Some(b) = body {
        collect_inner_loops(b, source, file_name, &mut inner_loops)?;
    }

    let start_pos = node.start_position();
    let end_pos = node.end_position();

    Ok(LoopNestIR {
        induction_var: Some(loop_var),
        lower_bound: Some(start),
        upper_bound: Some(end),
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
        if child.kind() == "for_statement" || child.kind() == "while_statement" {
            inner_loops.push(parse_loop(child, source, file_name)?);
        } else {
            collect_inner_loops(child, source, file_name, inner_loops)?;
        }
    }
    Ok(())
}

fn parse_range_call(
    node: Node,
    source: &[u8],
) -> Result<(ExprPtr, ExprPtr, StepKind, LoopDirection), SyntaxError> {
    if node.kind() == "call" {
        let function = node.child_by_field_name("function");
        let arguments = node.child_by_field_name("arguments");
        if let Some(f) = function {
            if f.utf8_text(source).unwrap_or("") == "range" {
                if let Some(args) = arguments {
                    let mut arg_exprs = Vec::new();
                    for a in args.children(&mut args.walk()) {
                        if a.kind() != "(" && a.kind() != ")" && a.kind() != "," {
                            arg_exprs.push(a);
                        }
                    }
                    match arg_exprs.len() {
                        1 => {
                            let end = parse_simple_expr(arg_exprs[0], source)?;
                            return Ok((
                                constant(0.0).unwrap(),
                                end,
                                StepKind::Linear(constant(1.0).unwrap()),
                                LoopDirection::Increasing,
                            ));
                        }
                        2 => {
                            let start = parse_simple_expr(arg_exprs[0], source)?;
                            let end = parse_simple_expr(arg_exprs[1], source)?;
                            return Ok((
                                start,
                                end,
                                StepKind::Linear(constant(1.0).unwrap()),
                                LoopDirection::Increasing,
                            ));
                        }
                        3 => {
                            let start = parse_simple_expr(arg_exprs[0], source)?;
                            let end = parse_simple_expr(arg_exprs[1], source)?;
                            let step_val = arg_exprs[2]
                                .utf8_text(source)
                                .unwrap_or("1")
                                .parse::<f64>()
                                .unwrap_or(1.0);
                            let direction = if step_val < 0.0 {
                                LoopDirection::Decreasing
                            } else {
                                LoopDirection::Increasing
                            };
                            return Ok((
                                start,
                                end,
                                StepKind::Linear(constant(step_val.abs()).unwrap()),
                                direction,
                            ));
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    Ok((
        constant(0.0).unwrap(),
        variable("n").unwrap(),
        StepKind::Linear(constant(1.0).unwrap()),
        LoopDirection::Increasing,
    ))
}

fn parse_simple_expr(node: Node, source: &[u8]) -> Result<ExprPtr, SyntaxError> {
    let text = node.utf8_text(source).unwrap_or("0");
    if let Ok(val) = text.parse::<f64>() {
        return Ok(constant(val).unwrap());
    }

    if node.kind() == "binary_operator" {
        // e.g. n - 1 or n + 1
        let left = node.child_by_field_name("left");
        if let Some(l) = left {
            return parse_simple_expr(l, source);
        }
    }

    if node.kind() == "call" {
        // e.g. len(arr)
        let arguments = node.child_by_field_name("arguments");
        if let Some(args) = arguments {
            for child in args.children(&mut args.walk()) {
                if child.kind() == "identifier" {
                    let id_text = child.utf8_text(source).unwrap_or("n");
                    return Ok(variable(id_text).unwrap_or_else(|_| variable("n").unwrap()));
                }
            }
        }
    }

    Ok(variable(text).unwrap_or_else(|_| constant(0.0).unwrap()))
}

/// Parse Python source code and extract all recursive functions as `RecursiveFunctionIR`.
pub fn parse_python_recursion(
    source: &str,
    file_name: &str,
) -> Result<Vec<RecursiveFunctionIR>, SyntaxError> {
    let mut parser = Parser::new();
    let language = tree_sitter_python::LANGUAGE.into();
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
    let name_node = func_node.child_by_field_name("name");
    let func_name = match name_node.and_then(|n| n.utf8_text(source).ok()) {
        Some(name) => name,
        None => return Ok(None),
    };

    // Extract first parameter name
    let mut param_name = "n".to_string();
    if let Some(params_node) = func_node.child_by_field_name("parameters") {
        for child in params_node.children(&mut params_node.walk()) {
            if child.kind() == "identifier" {
                if let Ok(name) = child.utf8_text(source) {
                    param_name = name.to_string();
                    break;
                }
            }
        }
    }

    let body_node = match func_node.child_by_field_name("body") {
        Some(b) => b,
        None => return Ok(None),
    };

    // Find all recursive calls to func_name
    let mut call_nodes = Vec::new();
    collect_calls(body_node, func_name, source, &mut call_nodes);

    if call_nodes.is_empty() {
        return Ok(None);
    }

    let branch_factor = call_nodes.len() as u32;

    // Analyze argument in the recursive calls
    let mut reduction_divisor = 1.0;
    let mut reduction_subtrahend = None;

    for call_node in &call_nodes {
        if let Some(arguments_node) = call_node.child_by_field_name("arguments") {
            for arg in arguments_node.children(&mut arguments_node.walk()) {
                if arg.kind() == "binary_operator" {
                    let op = arg
                        .child_by_field_name("operator")
                        .and_then(|o| o.utf8_text(source).ok())
                        .unwrap_or("");
                    let right = arg.child_by_field_name("right");
                    let right_val = right
                        .and_then(|r| r.utf8_text(source).ok())
                        .and_then(|t| t.parse::<f64>().ok())
                        .unwrap_or(2.0);

                    if op == "//" || op == "/" {
                        reduction_divisor = right_val;
                    } else if op == "-" {
                        reduction_subtrahend = Some(right_val);
                    }
                }
            }
        }
    }

    // Determine work exponent from loops in the function body
    let mut has_loop = false;
    check_for_loops(body_node, &mut has_loop);
    let work_exponent = if has_loop { 1.0 } else { 0.0 };

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

fn collect_calls<'a>(node: Node<'a>, target_name: &str, source: &[u8], calls: &mut Vec<Node<'a>>) {
    if node.kind() == "call" {
        if let Some(func) = node.child_by_field_name("function") {
            if let Ok(name) = func.utf8_text(source) {
                if name == target_name {
                    calls.push(node);
                }
            }
        }
    }

    for child in node.children(&mut node.walk()) {
        collect_calls(child, target_name, source, calls);
    }
}

fn check_for_loops(node: Node, has_loop: &mut bool) {
    if node.kind() == "for_statement" || node.kind() == "while_statement" {
        *has_loop = true;
        return;
    }
    for child in node.children(&mut node.walk()) {
        check_for_loops(child, has_loop);
        if *has_loop {
            return;
        }
    }
}

/// Parse Python source code and extract memory allocations.
pub fn parse_python_memory_allocations(
    source: &str,
    file_name: &str,
) -> Result<Vec<PythonMemoryAlloc>, SyntaxError> {
    let mut parser = Parser::new();
    let language = tree_sitter_python::LANGUAGE.into();
    parser
        .set_language(&language)
        .map_err(|_| SyntaxError::ParseFailure)?;

    let tree = parser
        .parse(source, None)
        .ok_or(SyntaxError::ParseFailure)?;
    let root = tree.root_node();

    let mut allocs = Vec::new();
    let mut handled_nodes = HashSet::new();
    collect_memory_allocations(
        root,
        source.as_bytes(),
        file_name,
        "<global>",
        &mut handled_nodes,
        &mut allocs,
    )?;
    Ok(allocs)
}

fn collect_memory_allocations(
    node: Node,
    source: &[u8],
    file_name: &str,
    current_fn: &str,
    handled_nodes: &mut HashSet<usize>,
    allocs: &mut Vec<PythonMemoryAlloc>,
) -> Result<(), SyntaxError> {
    let fn_name = if node.kind() == "function_definition" {
        node.child_by_field_name("name")
            .and_then(|n| n.utf8_text(source).ok())
            .unwrap_or(current_fn)
    } else {
        current_fn
    };

    // Case 1: 2D Grid: [[0] * m for _ in range(n)]
    if node.kind() == "list_comprehension" {
        if let Some(alloc) = parse_2d_grid(node, source, file_name, fn_name, handled_nodes)? {
            allocs.push(alloc);
        }
    }

    // Case 2: 1D List multiplication: [0] * n
    if node.kind() == "binary_operator" && !handled_nodes.contains(&node.id()) {
        if let Some(alloc) = parse_1d_list_mult(node, source, file_name, fn_name)? {
            allocs.push(alloc);
        }
    }

    for child in node.children(&mut node.walk()) {
        collect_memory_allocations(child, source, file_name, fn_name, handled_nodes, allocs)?;
    }
    Ok(())
}

fn parse_2d_grid(
    comp_node: Node,
    source: &[u8],
    file_name: &str,
    fn_name: &str,
    handled_nodes: &mut HashSet<usize>,
) -> Result<Option<PythonMemoryAlloc>, SyntaxError> {
    // Child 0 is `[`, Child 1 is element expression, Child 2 is `for_in_clause`, Child 3 is `]`
    let mut cursor = comp_node.walk();
    let children: Vec<Node> = comp_node.children(&mut cursor).collect();

    let elem_expr = children.iter().find(|c| c.kind() == "binary_operator");
    let for_clause = children.iter().find(|c| c.kind() == "for_in_clause");

    if let (Some(elem), Some(for_in)) = (elem_expr, for_clause) {
        // Check if elem is [0] * m
        let op = elem
            .child_by_field_name("operator")
            .and_then(|o| o.utf8_text(source).ok());
        let left = elem.child_by_field_name("left");
        let right = elem.child_by_field_name("right");

        if op == Some("*") {
            let (is_list, mult_node) = match (left, right) {
                (Some(l), Some(r)) if l.kind() == "list" => (true, r),
                (Some(l), Some(r)) if r.kind() == "list" => (true, l),
                _ => (false, *elem),
            };

            if is_list {
                let inner_size = parse_simple_expr(mult_node, source)?;
                let inner_text = mult_node.utf8_text(source).unwrap_or("m");

                // Check for_in clause: for _ in range(n)
                let for_right = for_in.child_by_field_name("right");
                let outer_size = if let Some(r) = for_right {
                    let (_, end, _, _) = parse_range_call(r, source)?;
                    end
                } else {
                    variable("n").unwrap()
                };

                handled_nodes.insert(elem.id());

                let start_pos = comp_node.start_position();
                let end_pos = comp_node.end_position();

                let total_size = multiply(vec![outer_size.clone(), inner_size]);

                return Ok(Some(PythonMemoryAlloc {
                    location: SourceLocation {
                        file: file_name.to_string(),
                        line: (start_pos.row + 1) as u32,
                        column: (start_pos.column + 1) as u32,
                        end_line: Some((end_pos.row + 1) as u32),
                        end_column: Some((end_pos.column + 1) as u32),
                    },
                    space_expr: total_size,
                    pattern: "dynamic-allocation".to_string(),
                    description: format!(
                        "2D grid allocation: [[0] * {} for _ in range({})]",
                        inner_text, outer_size
                    ),
                    function_name: fn_name.to_string(),
                }));
            }
        }
    }

    Ok(None)
}

fn parse_1d_list_mult(
    bin_node: Node,
    source: &[u8],
    file_name: &str,
    fn_name: &str,
) -> Result<Option<PythonMemoryAlloc>, SyntaxError> {
    let op = bin_node
        .child_by_field_name("operator")
        .and_then(|o| o.utf8_text(source).ok());
    if op != Some("*") {
        return Ok(None);
    }

    let left = bin_node.child_by_field_name("left");
    let right = bin_node.child_by_field_name("right");

    let (is_list, mult_node) = match (left, right) {
        (Some(l), Some(r)) if l.kind() == "list" && r.kind() != "list" => (true, r),
        (Some(l), Some(r)) if r.kind() == "list" && l.kind() != "list" => (true, l),
        _ => (false, bin_node),
    };

    if !is_list {
        return Ok(None);
    }

    let size_expr = parse_simple_expr(mult_node, source)?;
    let mult_text = mult_node.utf8_text(source).unwrap_or("n");

    let start_pos = bin_node.start_position();
    let end_pos = bin_node.end_position();

    Ok(Some(PythonMemoryAlloc {
        location: SourceLocation {
            file: file_name.to_string(),
            line: (start_pos.row + 1) as u32,
            column: (start_pos.column + 1) as u32,
            end_line: Some((end_pos.row + 1) as u32),
            end_column: Some((end_pos.column + 1) as u32),
        },
        space_expr: size_expr,
        pattern: "dynamic-allocation".to_string(),
        description: format!("dynamic list allocation: [0] * {}", mult_text),
        function_name: fn_name.to_string(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_python_nested_loop_analysis() {
        let py_code = r#"
def matrix_multiply(n, m):
    total = 0
    for i in range(n):
        for j in range(m):
            total += i * j
    return total
"#;
        let loops = parse_python_loops(py_code, "test.py").unwrap();
        assert_eq!(loops.len(), 1);
        let outer = &loops[0];
        assert_eq!(outer.induction_var.as_deref(), Some("i"));
        assert_eq!(outer.inner_loops.len(), 1);
        let inner = &outer.inner_loops[0];
        assert_eq!(inner.induction_var.as_deref(), Some("j"));

        let outcome = outer.analyze();
        assert_eq!(outcome.total_cost.unwrap().asymptotic_bound(), "O(n * m)");
    }

    #[test]
    fn test_python_single_loop() {
        let py_code = r#"
for i in range(n):
    print(i)
"#;
        let loops = parse_python_loops(py_code, "test.py").unwrap();
        assert_eq!(loops.len(), 1);
        let outcome = loops[0].analyze();
        assert_eq!(outcome.total_cost.unwrap().asymptotic_bound(), "O(n)");
    }

    #[test]
    fn test_python_triangular_loop() {
        let py_code = r#"
for i in range(n):
    for j in range(i):
        pass
"#;
        let loops = parse_python_loops(py_code, "test.py").unwrap();
        assert_eq!(loops.len(), 1);
        let outcome = loops[0].analyze();
        assert_eq!(outcome.pattern, "triangular-loop");
        assert_eq!(outcome.total_cost.unwrap().asymptotic_bound(), "O(n^2)");
    }

    #[test]
    fn test_python_range_3_args() {
        let py_code = r#"
for i in range(0, n, 2):
    print(i)
"#;
        let loops = parse_python_loops(py_code, "test.py").unwrap();
        assert_eq!(loops.len(), 1);
        let outcome = loops[0].analyze();
        assert_eq!(outcome.total_cost.unwrap().asymptotic_bound(), "O(n)");
    }

    #[test]
    fn test_python_recursion_merge_sort() {
        let py_code = r#"
def merge_sort(n):
    if n <= 1:
        return 1
    total = 0
    for i in range(n):
        total += i
    return merge_sort(n // 2) + merge_sort(n // 2)
"#;
        let recs = parse_python_recursion(py_code, "rec.py").unwrap();
        assert_eq!(recs.len(), 1);
        let rec = &recs[0];
        assert_eq!(rec.name, "merge_sort");
        assert_eq!(rec.branch_factor, 2);
        assert_eq!(rec.reduction_divisor, 2.0);
        assert_eq!(rec.work_exponent, 1.0);

        let (cost, status, _, note) = rec.solve();
        assert_eq!(status, bigolens_core::models::BoundStatus::Proven);
        assert_eq!(cost.asymptotic_bound(), "O(n * log_2(n))");
        assert!(note.contains("Master Theorem Case 2"));
    }

    #[test]
    fn test_python_recursion_binary_search() {
        let py_code = r#"
def bsearch(n):
    if n <= 1:
        return 1
    return bsearch(n // 2)
"#;
        let recs = parse_python_recursion(py_code, "rec.py").unwrap();
        assert_eq!(recs.len(), 1);
        let rec = &recs[0];
        assert_eq!(rec.name, "bsearch");
        assert_eq!(rec.branch_factor, 1);
        assert_eq!(rec.reduction_divisor, 2.0);
        assert_eq!(rec.work_exponent, 0.0);

        let (cost, status, _, _) = rec.solve();
        assert_eq!(status, bigolens_core::models::BoundStatus::Proven);
        assert_eq!(cost.asymptotic_bound(), "O(log_2(n))");
    }

    #[test]
    fn test_python_recursion_linear_decrement() {
        let py_code = r#"
def countdown(n):
    if n <= 0:
        return 0
    return countdown(n - 1)
"#;
        let recs = parse_python_recursion(py_code, "rec.py").unwrap();
        assert_eq!(recs.len(), 1);
        let rec = &recs[0];
        assert_eq!(rec.name, "countdown");
        assert_eq!(rec.branch_factor, 1);
        assert_eq!(rec.reduction_subtrahend, Some(1.0));

        let (cost, status, _, _) = rec.solve();
        assert_eq!(status, bigolens_core::models::BoundStatus::Proven);
        assert_eq!(cost.asymptotic_bound(), "O(n)");
    }

    #[test]
    fn test_python_memory_allocation() {
        let py_code = r#"
def allocate(n, m):
    arr = [0] * n
    grid = [[0] * m for _ in range(n)]
    return arr, grid
"#;
        let allocs = parse_python_memory_allocations(py_code, "mem.py").unwrap();
        assert_eq!(allocs.len(), 2);
        assert_eq!(allocs[0].space_expr.asymptotic_bound(), "O(n)");
        assert_eq!(allocs[1].space_expr.asymptotic_bound(), "O(n * m)");
    }
}
