use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity, Position, Range};
use tree_sitter::{Node, Tree};

// ---------------------------------------------------------------------------
// 基础类型检查器
// ---------------------------------------------------------------------------

/// 收集类型诊断：赋值兼容、参数数量、return 类型。
pub fn type_check(tree: &Tree, source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let root = tree.root_node();
    
    // 1. 收集所有定义的类型签名
    let signatures = collect_signatures(root, source);
    
    // 2. 遍历 AST 检查
    walk_type_check(root, source, &signatures, &mut diagnostics);
    
    diagnostics
}

// ---------------------------------------------------------------------------
// 签名收集
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct MethodSig {
    name: String,
    params: Vec<String>, // 参数类型列表（简化，只存类型名）
    ret: Option<String>, // 返回类型
}

#[derive(Debug, Clone)]
struct TypeEnv {
    methods: std::collections::HashMap<String, MethodSig>,
    vars: std::collections::HashMap<String, String>, // name -> type
}

fn collect_signatures(node: Node, source: &str) -> TypeEnv {
    let mut env = TypeEnv {
        methods: std::collections::HashMap::new(),
        vars: std::collections::HashMap::new(),
    };
    collect_signatures_recursive(node, source, &mut env);
    env
}

fn collect_signatures_recursive(node: Node, source: &str, env: &mut TypeEnv) {
    let kind = node.kind();
    
    match kind {
        "action" | "annotated_action" => {
            if let Some(name) = child_text_by_kind(node, "method_identifier", source) {
                let params = extract_param_types(node, source);
                env.methods.insert(name.clone(), MethodSig {
                    name,
                    params,
                    ret: None, // action 无返回值
                });
            }
        }
        "function_declaration" => {
            if let Some(name) = child_text_by_kind(node, "method_identifier", source) {
                let params = extract_param_types(node, source);
                let ret = child_text_by_kind(node, "type_identifier", source)
                    .or_else(|| child_text_by_kind(node, "bit_type", source))
                    .or_else(|| child_text_by_kind(node, "varbit_type", source))
                    .or_else(|| child_text_by_kind(node, "bool", source))
                    .or_else(|| child_text_by_kind(node, "void", source));
                env.methods.insert(name.clone(), MethodSig {
                    name,
                    params,
                    ret,
                });
            }
        }
        "extern_definition" => {
            // 收集 extern 的方法签名
            for i in 0..node.child_count() {
                if let Some(child) = node.child(i) {
                    if child.kind() == "method" {
                        if let Some(m_name) = child_text_by_kind(child, "method_identifier", source) {
                            let params = extract_param_types(child, source);
                            let ret = child_text_by_kind(child, "type_identifier", source)
                                .or_else(|| child_text_by_kind(child, "bit_type", source))
                                .or_else(|| child_text_by_kind(child, "void", source));
                            let full_name = format!("{}.{}", 
                                child_text_by_kind(node, "type_identifier", source).unwrap_or_default(),
                                m_name);
                            env.methods.insert(full_name, MethodSig {
                                name: m_name,
                                params,
                                ret,
                            });
                        }
                    }
                }
            }
        }
        "parameter" => {
            if let Some((name, ty)) = extract_parameter_name_type(node, source) {
                env.vars.insert(name, ty);
            }
        }
        "var_decl" | "variable_declaration" => {
            if let Some((name, ty)) = extract_var_decl_type(node, source) {
                env.vars.insert(name, ty);
            }
        }
        _ => {}
    }
    
    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            collect_signatures_recursive(child, source, env);
        }
    }
}

fn extract_param_types(node: Node, source: &str) -> Vec<String> {
    let mut types = Vec::new();
    for i in 0..node.child_count() {
        if let Some(c) = node.child(i) {
            if c.kind() == "parameter" {
                if let Some((_, ty)) = extract_parameter_name_type(c, source) {
                    types.push(ty);
                }
            }
        }
    }
    types
}

fn extract_parameter_name_type(node: Node, source: &str) -> Option<(String, String)> {
    let mut name = String::new();
    let mut ty = String::new();
    for i in 0..node.child_count() {
        if let Some(c) = node.child(i) {
            let k = c.kind();
            if k == "identifier" {
                name = source[c.start_byte()..c.end_byte()].to_string();
            }
            if k == "type_identifier" || k == "bit_type" || k == "varbit_type" || k == "bool" || k == "int" || k == "bit" || k == "varbit" || k == "error" || k == "packet_in" || k == "packet_out" || k == "string" || k == "void" || k == "match_kind" {
                ty = source[c.start_byte()..c.end_byte()].to_string();
            }
        }
    }
    if name.is_empty() { None } else { Some((name, ty)) }
}

fn extract_var_decl_type(node: Node, source: &str) -> Option<(String, String)> {
    // var_decl: seq(optional(choice($._type, $.type_identifier)), $.lval, "=", $.var_choice, ";")
    let mut name = String::new();
    let mut ty = String::new();
    
    for i in 0..node.child_count() {
        if let Some(c) = node.child(i) {
            let k = c.kind();
            if k == "lval" {
                // 取 lval 的第一个 identifier
                for j in 0..c.child_count() {
                    if let Some(id) = c.child(j) {
                        if id.kind() == "identifier" {
                            name = source[id.start_byte()..id.end_byte()].to_string();
                            break;
                        }
                    }
                }
            }
            if k == "type_identifier" || k == "bit_type" || k == "varbit_type" || k == "bool" || k == "int" || k == "bit" || k == "varbit" || k == "error" || k == "packet_in" || k == "packet_out" || k == "string" || k == "void" || k == "match_kind" {
                ty = source[c.start_byte()..c.end_byte()].to_string();
            }
        }
    }
    if name.is_empty() { None } else { Some((name, ty)) }
}

fn child_text_by_kind(node: Node, kind: &str, source: &str) -> Option<String> {
    for i in 0..node.child_count() {
        if let Some(c) = node.child(i) {
            if c.kind() == kind {
                return Some(source[c.start_byte()..c.end_byte()].to_string());
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------
// 类型检查遍历
// ---------------------------------------------------------------------------

fn walk_type_check(
    node: Node,
    source: &str,
    env: &TypeEnv,
    out: &mut Vec<Diagnostic>,
) {
    let kind = node.kind();
    
    match kind {
        "call" => check_call(node, source, env, out),
        "var_decl" | "variable_declaration" => check_var_decl(node, source, env, out),
        "return_stmt" => check_return(node, source, env, out),
        "assignment" => check_assignment(node, source, env, out),
        _ => {}
    }
    
    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            walk_type_check(child, source, env, out);
        }
    }
}

fn check_call(node: Node, source: &str, env: &TypeEnv, out: &mut Vec<Diagnostic>) {
    // 提取 fval（被调用者）
    let fval_opt = node.child(0).filter(|c| c.kind() == "fval");
    let fval_text = fval_opt.map(|f| source[f.start_byte()..f.end_byte()].to_string()).unwrap_or_default();
    
    // 提取参数数量
    let mut arg_count = 0;
    for i in 0..node.child_count() {
        if let Some(c) = node.child(i) {
            if c.kind() == "expr" || c.kind() == "identifier" || c.kind() == "number" {
                arg_count += 1;
            }
        }
    }
    
    // 查找签名
    if let Some(sig) = env.methods.get(&fval_text) {
        let expected = sig.params.len();
        if arg_count != expected {
            out.push(Diagnostic {
                range: Range {
                    start: Position {
                        line: node.start_position().row as u32,
                        character: node.start_position().column as u32,
                    },
                    end: Position {
                        line: node.end_position().row as u32,
                        character: node.end_position().column as u32,
                    },
                },
                severity: Some(DiagnosticSeverity::ERROR),
                code: None,
                code_description: None,
                source: Some("p4lsp".to_string()),
                message: format!(
                    "expected {} arguments but got {} for '{}'",
                    expected, arg_count, fval_text
                ),
                related_information: None,
                tags: None,
                data: None,
            });
        }
    }
}

fn check_var_decl(node: Node, source: &str, _env: &TypeEnv, out: &mut Vec<Diagnostic>) {
    // 检查 var_decl 是否缺少类型声明
    let mut has_type = false;
    for i in 0..node.child_count() {
        if let Some(c) = node.child(i) {
            let k = c.kind();
            if k == "type_identifier" || k == "bit_type" || k == "varbit_type" || k == "bool" || k == "int" || k == "bit" || k == "varbit" || k == "error" || k == "packet_in" || k == "packet_out" || k == "string" || k == "void" || k == "match_kind" {
                has_type = true;
            }
        }
    }
    if !has_type {
        out.push(Diagnostic {
            range: Range {
                start: Position {
                    line: node.start_position().row as u32,
                    character: node.start_position().column as u32,
                },
                end: Position {
                    line: node.end_position().row as u32,
                    character: node.end_position().column as u32,
                },
            },
            severity: Some(DiagnosticSeverity::WARNING),
            code: None,
            code_description: None,
            source: Some("p4lsp".to_string()),
            message: "variable declaration missing explicit type".to_string(),
            related_information: None,
            tags: None,
            data: None,
        });
    }
}

fn check_return(node: Node, source: &str, env: &TypeEnv, out: &mut Vec<Diagnostic>) {
    // 查找所属函数/控制/解析器
    let mut current = node;
    let mut func_name = None;
    loop {
        let kind = current.kind();
        if kind == "action" || kind == "annotated_action" {
            func_name = child_text_by_kind(current, "method_identifier", source);
            break;
        }
        if kind == "function_declaration" {
            func_name = child_text_by_kind(current, "method_identifier", source);
            break;
        }
        if kind == "control_definition" || kind == "parser_definition" {
            // control/parser 的 apply 块中的 return 不允许返回值
            break;
        }
        if let Some(parent) = current.parent() {
            current = parent;
        } else {
            break;
        }
    }
    
    // 提取 return 表达式
    let mut has_expr = false;
    for i in 0..node.child_count() {
        if let Some(c) = node.child(i) {
            if c.kind() == "expr" || c.kind() == "identifier" || c.kind() == "number" {
                has_expr = true;
            }
        }
    }
    
    if let Some(name) = func_name {
        if let Some(sig) = env.methods.get(&name) {
            let has_ret = sig.ret.is_some() && sig.ret.as_ref().unwrap() != "void";
            if has_expr && !has_ret {
                out.push(Diagnostic {
                    range: Range {
                        start: Position {
                            line: node.start_position().row as u32,
                            character: node.start_position().column as u32,
                        },
                        end: Position {
                            line: node.end_position().row as u32,
                            character: node.end_position().column as u32,
                        },
                    },
                    severity: Some(DiagnosticSeverity::ERROR),
                    code: None,
                    code_description: None,
                    source: Some("p4lsp".to_string()),
                    message: format!("function '{}' has no return type but returns a value", name),
                    related_information: None,
                    tags: None,
                    data: None,
                });
            }
            if !has_expr && has_ret {
                out.push(Diagnostic {
                    range: Range {
                        start: Position {
                            line: node.start_position().row as u32,
                            character: node.start_position().column as u32,
                        },
                        end: Position {
                            line: node.end_position().row as u32,
                            character: node.end_position().column as u32,
                        },
                    },
                    severity: Some(DiagnosticSeverity::ERROR),
                    code: None,
                    code_description: None,
                    source: Some("p4lsp".to_string()),
                    message: format!("function '{}' should return a value", name),
                    related_information: None,
                    tags: None,
                    data: None,
                });
            }
        }
    }
}

fn check_assignment(node: Node, source: &str, _env: &TypeEnv, _out: &mut Vec<Diagnostic>) {
    // TODO: 简化版暂不检查赋值兼容性
    // 需要：左边 lval 类型 vs 右边 expr 类型
    let _ = (node, source);
}

// ---------------------------------------------------------------------------
// 测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser;

    fn parse_source(source: &str) -> Tree {
        let mut parser = parser::new_parser().unwrap();
        parser.parse(source, None).expect("parse should succeed")
    }

    #[test]
    fn test_call_arg_count_mismatch() {
        let source = r#"
action drop() {}
control C() {
    apply {
        drop(1);
    }
}
"#;
        let tree = parse_source(source);
        let diags = type_check(&tree, source);
        assert_eq!(diags.len(), 1, "should report 1 arg count error");
        assert!(diags[0].message.contains("expected 0 arguments but got 1"));
    }

    #[test]
    fn test_call_arg_count_ok() {
        let source = r#"
action set_dscp(bit<6> val) {}
control C() {
    apply {
        set_dscp(1);
    }
}
"#;
        let tree = parse_source(source);
        let diags = type_check(&tree, source);
        assert!(diags.is_empty(), "should not report error for correct arg count");
    }

    #[test]
    fn test_return_without_value_in_void() {
        let source = r#"
action drop() {
    return;
}
"#;
        let tree = parse_source(source);
        let diags = type_check(&tree, source);
        assert!(diags.is_empty(), "void action with empty return should be ok");
    }

    #[test]
    fn test_return_with_value_in_void() {
        let source = r#"
action drop() {
    return 1;
}
"#;
        let tree = parse_source(source);
        let diags = type_check(&tree, source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("has no return type but returns a value"));
    }
}
