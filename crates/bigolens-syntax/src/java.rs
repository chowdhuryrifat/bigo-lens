//! Java syntax analysis via Tree-sitter.

use crate::SyntaxError;
use bigolens_core::complexity::{constant, multiply, variable, ExprPtr};
use bigolens_core::ir::{LoopDirection, LoopNestIR, RecursiveFunctionIR, StepKind};
use bigolens_core::models::SourceLocation;
use tree_sitter::{Node, Parser};

/// Information about a detected Java dynamic memory allocation.
#[derive(Debug, Clone, PartialEq)]
pub struct JavaMemoryAlloc {
    pub location: SourceLocation,
    pub space_expr: ExprPtr,
    pub pattern: String,
    pub description: String,
    pub function_name: String,
}

/// Parse Java source code and extract all top-level loop nests as `LoopNestIR`.
pub fn parse_java_loops(source: &str, file_name: &str) -> Result<Vec<LoopNestIR>, SyntaxError> {
    let mut parser = Parser::new();
    let language = tree_sitter_java::LANGUAGE.into();
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
        if child.kind() == "for_statement"
            || child.kind() == "enhanced_for_statement"
            || child.kind() == "while_statement"
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
        "for_statement" => parse_for_statement(node, source, file_name),
        "enhanced_for_statement" => parse_enhanced_for_statement(node, source, file_name),
        "while_statement" => parse_while_statement(node, source, file_name),
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

fn parse_for_statement(
    node: Node,
    source: &[u8],
    file_name: &str,
) -> Result<LoopNestIR, SyntaxError> {
    let start_pos = node.start_position();
    let end_pos = node.end_position();

    let mut var_name = "i".to_string();
    let mut lower = constant(0.0).unwrap();
    let mut upper = variable("n").unwrap();
    let mut step = StepKind::Linear(constant(1.0).unwrap());
    let mut direction = LoopDirection::Increasing;
    let mut is_inclusive = false;

    // tree-sitter-java for_statement fields:
    // init: local_variable_declaration or assignment_expression
    // condition: binary_expression
    // update: update_expression or assignment_expression
    // body: statement / block
    if let Some(init_node) = node.child_by_field_name("init") {
        if let Some((v, val)) = parse_java_initializer(init_node, source) {
            var_name = v;
            lower = val;
        }
    }

    if let Some(cond_node) = node.child_by_field_name("condition") {
        if let Some((bound_expr, incl, dir)) = parse_java_condition(cond_node, &var_name, source) {
            upper = bound_expr;
            is_inclusive = incl;
            direction = dir;
        }
    }

    if let Some(update_node) = node.child_by_field_name("update") {
        step = parse_java_update(update_node, source);
    }

    let mut inner_loops = Vec::new();
    if let Some(body_node) = node.child_by_field_name("body") {
        collect_inner_loops(body_node, source, file_name, &mut inner_loops)?;
    }

    Ok(LoopNestIR {
        induction_var: Some(var_name),
        lower_bound: Some(lower),
        upper_bound: Some(upper),
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

fn parse_enhanced_for_statement(
    node: Node,
    source: &[u8],
    file_name: &str,
) -> Result<LoopNestIR, SyntaxError> {
    let start_pos = node.start_position();
    let end_pos = node.end_position();

    // enhanced_for_statement: for (Type var : iterable) body
    let mut container_name = "n".to_string();
    if let Some(value_node) = node.child_by_field_name("value") {
        if let Ok(text) = value_node.utf8_text(source) {
            container_name = text.trim().to_string();
        }
    }

    let upper = if container_name == "n" {
        variable("n").unwrap()
    } else {
        variable(format!("{}.length", container_name)).unwrap_or_else(|_| variable("n").unwrap())
    };

    let mut inner_loops = Vec::new();
    if let Some(body_node) = node.child_by_field_name("body") {
        collect_inner_loops(body_node, source, file_name, &mut inner_loops)?;
    }

    Ok(LoopNestIR {
        induction_var: Some("_item".to_string()),
        lower_bound: Some(constant(0.0).unwrap()),
        upper_bound: Some(upper),
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
        inner_loops,
        has_early_exit: false,
        bound_modified_in_body: false,
        index_modified_in_body: false,
    })
}

fn parse_while_statement(
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
        } else if body_text.contains("--") || body_text.contains("-= 1") {
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
        if child.kind() == "for_statement"
            || child.kind() == "enhanced_for_statement"
            || child.kind() == "while_statement"
        {
            inner_loops.push(parse_loop(child, source, file_name)?);
        } else {
            collect_inner_loops(child, source, file_name, inner_loops)?;
        }
    }
    Ok(())
}

fn parse_java_initializer(node: Node, source: &[u8]) -> Option<(String, ExprPtr)> {
    let text = node.utf8_text(source).ok()?;
    // e.g. "int i = 0" or "i = 0"
    for part in text.split(',') {
        if let Some((lhs, rhs)) = part.split_once('=') {
            let var_tokens: Vec<&str> = lhs.split_whitespace().collect();
            if let Some(v) = var_tokens.last() {
                let val_str = rhs.trim().trim_end_matches(';');
                let val: f64 = val_str.parse().unwrap_or(0.0);
                return Some((v.to_string(), constant(val).unwrap()));
            }
        }
    }
    None
}

fn parse_java_condition(
    node: Node,
    _var_name: &str,
    source: &[u8],
) -> Option<(ExprPtr, bool, LoopDirection)> {
    let text = node.utf8_text(source).ok()?;
    let clean = text.trim();

    if clean.contains("<=") {
        let parts: Vec<&str> = clean.split("<=").collect();
        if parts.len() == 2 {
            let bound_str = parts[1].trim().trim_end_matches(';');
            let bound = parse_expr_str(bound_str);
            return Some((bound, true, LoopDirection::Increasing));
        }
    } else if clean.contains('<') {
        let parts: Vec<&str> = clean.split('<').collect();
        if parts.len() == 2 {
            let bound_str = parts[1].trim().trim_end_matches(';');
            let bound = parse_expr_str(bound_str);
            return Some((bound, false, LoopDirection::Increasing));
        }
    } else if clean.contains(">=") {
        let parts: Vec<&str> = clean.split(">=").collect();
        if parts.len() == 2 {
            let bound_str = parts[1].trim().trim_end_matches(';');
            let bound = parse_expr_str(bound_str);
            return Some((bound, true, LoopDirection::Decreasing));
        }
    } else if clean.contains('>') {
        let parts: Vec<&str> = clean.split('>').collect();
        if parts.len() == 2 {
            let bound_str = parts[1].trim().trim_end_matches(';');
            let bound = parse_expr_str(bound_str);
            return Some((bound, false, LoopDirection::Decreasing));
        }
    }

    Some((variable("n").unwrap(), false, LoopDirection::Increasing))
}

fn parse_java_update(node: Node, source: &[u8]) -> StepKind {
    let text = node.utf8_text(source).unwrap_or("");
    if text.contains("*=") || text.contains("<<=") {
        if let Some((_, factor_str)) = text.split_once('=') {
            let factor: f64 = factor_str.trim().parse().unwrap_or(2.0);
            return StepKind::GeometricMultiply(factor);
        }
        return StepKind::GeometricMultiply(2.0);
    } else if text.contains("/=") || text.contains(">>=") {
        if let Some((_, factor_str)) = text.split_once('=') {
            let factor: f64 = factor_str.trim().parse().unwrap_or(2.0);
            return StepKind::GeometricDivide(factor);
        }
        return StepKind::GeometricDivide(2.0);
    } else if text.contains("+=") || text.contains("-=") {
        if let Some((_, step_str)) = text.split_once('=') {
            let step_val: f64 = step_str.trim().parse().unwrap_or(1.0);
            return StepKind::Linear(constant(step_val).unwrap());
        }
    }
    StepKind::Linear(constant(1.0).unwrap())
}

fn parse_expr_str(s: &str) -> ExprPtr {
    let trimmed = s.trim().trim_matches(|c: char| c == '(' || c == ')');
    if let Ok(v) = trimmed.parse::<f64>() {
        constant(v).unwrap()
    } else if trimmed.ends_with(".length") || trimmed.ends_with(".size()") {
        let base = trimmed.split('.').next().unwrap_or("n");
        variable(base).unwrap_or_else(|_| variable("n").unwrap())
    } else {
        variable(trimmed).unwrap_or_else(|_| variable("n").unwrap())
    }
}

/// Parse Java recursive methods into `RecursiveFunctionIR`.
pub fn parse_java_recursion(
    source: &str,
    file_name: &str,
) -> Result<Vec<RecursiveFunctionIR>, SyntaxError> {
    let mut parser = Parser::new();
    let language = tree_sitter_java::LANGUAGE.into();
    parser
        .set_language(&language)
        .map_err(|_| SyntaxError::ParseFailure)?;

    let tree = parser
        .parse(source, None)
        .ok_or(SyntaxError::ParseFailure)?;
    let root = tree.root_node();

    let mut recursions = Vec::new();
    collect_methods(root, source.as_bytes(), file_name, &mut recursions)?;
    Ok(recursions)
}

fn collect_methods(
    node: Node,
    source: &[u8],
    file_name: &str,
    recursions: &mut Vec<RecursiveFunctionIR>,
) -> Result<(), SyntaxError> {
    if node.kind() == "method_declaration" {
        if let Some(rec) = analyze_method_recursion(node, source, file_name)? {
            recursions.push(rec);
        }
    }

    for child in node.children(&mut node.walk()) {
        collect_methods(child, source, file_name, recursions)?;
    }
    Ok(())
}

fn analyze_method_recursion(
    method_node: Node,
    source: &[u8],
    file_name: &str,
) -> Result<Option<RecursiveFunctionIR>, SyntaxError> {
    let name_node = match method_node.child_by_field_name("name") {
        Some(n) => n,
        None => return Ok(None),
    };
    let method_name = match name_node.utf8_text(source) {
        Ok(name) => name,
        Err(_) => return Ok(None),
    };

    let mut param_name = "n".to_string();
    if let Some(params_node) = method_node.child_by_field_name("parameters") {
        for child in params_node.children(&mut params_node.walk()) {
            if child.kind() == "formal_parameter" {
                if let Some(id_node) = child.child_by_field_name("name") {
                    if let Ok(name) = id_node.utf8_text(source) {
                        param_name = name.to_string();
                        break;
                    }
                }
            }
        }
    }

    let body_node = match method_node.child_by_field_name("body") {
        Some(b) => b,
        None => return Ok(None),
    };

    let mut call_nodes = Vec::new();
    collect_method_calls(body_node, method_name, source, &mut call_nodes);

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

    // Inspect non-recursive work in the method body (loops)
    let mut has_loop = false;
    let mut loop_checker = |n: Node| {
        if n.kind() == "for_statement"
            || n.kind() == "enhanced_for_statement"
            || n.kind() == "while_statement"
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

    let start_pos = method_node.start_position();
    let end_pos = method_node.end_position();

    Ok(Some(RecursiveFunctionIR {
        name: method_name.to_string(),
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

fn collect_method_calls<'a>(
    node: Node<'a>,
    method_name: &str,
    source: &[u8],
    calls: &mut Vec<Node<'a>>,
) {
    if node.kind() == "method_invocation" {
        if let Some(name_node) = node.child_by_field_name("name") {
            if let Ok(name) = name_node.utf8_text(source) {
                if name == method_name {
                    calls.push(node);
                }
            }
        }
    }

    for child in node.children(&mut node.walk()) {
        collect_method_calls(child, method_name, source, calls);
    }
}

fn walk_node<F: FnMut(Node)>(node: Node, f: &mut F) {
    f(node);
    for child in node.children(&mut node.walk()) {
        walk_node(child, f);
    }
}

/// Parse Java dynamic allocations into `JavaMemoryAlloc`.
pub fn parse_java_memory_allocations(
    source: &str,
    file_name: &str,
) -> Result<Vec<JavaMemoryAlloc>, SyntaxError> {
    let mut parser = Parser::new();
    let language = tree_sitter_java::LANGUAGE.into();
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
    allocs: &mut Vec<JavaMemoryAlloc>,
) -> Result<(), SyntaxError> {
    if node.kind() == "array_creation_expression" {
        let start = node.start_position();
        let end = node.end_position();
        let text = node.utf8_text(source).unwrap_or("");
        // e.g. new int[n] or new long[n]
        let elem_size = if text.contains("long") || text.contains("double") {
            8.0
        } else if text.contains("byte") || text.contains("boolean") {
            1.0
        } else if text.contains("short") || text.contains("char") {
            2.0
        } else {
            4.0 // int, float, reference
        };

        // Extract dimension
        let mut dim_var = "n".to_string();
        for child in node.children(&mut node.walk()) {
            if child.kind() == "dimensions_expr" {
                let inner = child.utf8_text(source).unwrap_or("");
                dim_var = inner
                    .trim_matches(|c: char| c == '[' || c == ']')
                    .trim()
                    .to_string();
                if dim_var.is_empty() {
                    dim_var = "n".to_string();
                }
            }
        }

        let var_expr = if let Ok(val) = dim_var.parse::<f64>() {
            constant(val).unwrap()
        } else {
            variable(&dim_var).unwrap_or_else(|_| variable("n").unwrap())
        };

        let space_expr = multiply(vec![constant(elem_size).unwrap(), var_expr]);

        allocs.push(JavaMemoryAlloc {
            location: SourceLocation {
                file: file_name.to_string(),
                line: (start.row + 1) as u32,
                column: (start.column + 1) as u32,
                end_line: Some((end.row + 1) as u32),
                end_column: Some((end.column + 1) as u32),
            },
            space_expr,
            pattern: "dynamic-array".to_string(),
            description: format!("array allocation with element size {} bytes", elem_size),
            function_name: "main".to_string(),
        });
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
    fn test_java_nested_loops() {
        let code = r#"
class Solution {
    public void solve(int n) {
        for (int i = 0; i < n; i++) {
            for (int j = 0; j < i; j++) {
                System.out.println(i + j);
            }
        }
    }
}
"#;
        let loops = parse_java_loops(code, "Solution.java").unwrap();
        assert_eq!(loops.len(), 1);
        let outcome = loops[0].analyze();
        assert_eq!(outcome.pattern, "triangular-loop");
        assert_eq!(outcome.total_cost.unwrap().to_string(), "n^2");
    }

    #[test]
    fn test_java_enhanced_for() {
        let code = r#"
class Solution {
    public void printAll(int[] arr) {
        for (int x : arr) {
            System.out.println(x);
        }
    }
}
"#;
        let loops = parse_java_loops(code, "Solution.java").unwrap();
        assert_eq!(loops.len(), 1);
        let outcome = loops[0].analyze();
        assert_eq!(outcome.pattern, "linear-loop");
        assert_eq!(outcome.total_cost.unwrap().to_string(), "arr.length");
    }

    #[test]
    fn test_java_recursion_merge_sort() {
        let code = r#"
class Solution {
    public void mergeSort(int n) {
        if (n <= 1) return;
        for (int i = 0; i < n; i++) {
            // merge step
        }
        mergeSort(n / 2);
        mergeSort(n / 2);
    }
}
"#;
        let recs = parse_java_recursion(code, "Solution.java").unwrap();
        assert_eq!(recs.len(), 1);
        assert_eq!(recs[0].branch_factor, 2);
        assert_eq!(recs[0].reduction_divisor, 2.0);
        let (cost, status, conf, _) = recs[0].solve();
        assert_eq!(cost.asymptotic_bound(), "O(n * log_2(n))");
        assert_eq!(status, bigolens_core::models::BoundStatus::Proven);
        assert_eq!(conf, bigolens_core::models::Confidence::High);
    }

    #[test]
    fn test_java_memory_allocation() {
        let code = r#"
class Solution {
    public int[] create(int n) {
        return new int[n];
    }
}
"#;
        let allocs = parse_java_memory_allocations(code, "Solution.java").unwrap();
        assert_eq!(allocs.len(), 1);
        assert_eq!(allocs[0].space_expr.to_string(), "4 * n");
    }
}
