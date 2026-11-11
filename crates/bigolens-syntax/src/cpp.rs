//! C++ syntax analysis via Tree-sitter.

use crate::SyntaxError;
use bigolens_core::complexity::{constant, variable, ExprPtr};
use bigolens_core::ir::{LoopDirection, LoopNestIR, StepKind};
use bigolens_core::models::SourceLocation;
use tree_sitter::{Node, Parser};

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
}
