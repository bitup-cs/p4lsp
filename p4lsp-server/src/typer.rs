use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity, Position, Range};
use tree_sitter::{Node, Tree};

/// P4 类型表示
#[derive(Debug, Clone, PartialEq)]
pub enum P4Type {
    Bit(u32),           // bit<w>
    Int(u32),           // int<w>
    Bool,               // bool
    Varbit(u32),        // varbit<w>
    Void,               // void
    String,             // string
    Error,              // error
    MatchKind,          // match_kind
    Header(String),     // header TypeName
    Struct(String),     // struct TypeName
    Enum(String),       // enum TypeName
    Extern(String),     // extern TypeName
    Array(Box<P4Type>, u32), // array_type [N]
    Unknown,            // 无法推断
}

impl P4Type {
    pub fn to_string(&self) -> String {
        match self {
            P4Type::Bit(w) => format!("bit<{}>", w),
            P4Type::Int(w) => format!("int<{}>", w),
            P4Type::Bool => "bool".to_string(),
            P4Type::Varbit(w) => format!("varbit<{}>", w),
            P4Type::Void => "void".to_string(),
            P4Type::String => "string".to_string(),
            P4Type::Error => "error".to_string(),
            P4Type::MatchKind => "match_kind".to_string(),
            P4Type::Header(name) => format!("header {}", name),
            P4Type::Struct(name) => format!("struct {}", name),
            P4Type::Enum(name) => format!("enum {}", name),
            P4Type::Extern(name) => format!("extern {}", name),
            P4Type::Array(ty, n) => format!("{}[{}]", ty.to_string(), n),
            P4Type::Unknown => "unknown".to_string(),
        }
    }
}

/// 从 AST 节点推断表达式类型
pub fn infer_expr_type(node: Node, source: &str, type_env: &TypeEnv) -> P4Type {
    let kind = node.kind();
    match kind {
        "number" | "decimal" | "hex" | "binary" | "octal" => {
            // 默认推断为 bit<32>，实际应根据上下文优化
            P4Type::Bit(32)
        }
        "true" | "false" => P4Type::Bool,
        "string_literal" => P4Type::String,
        "identifier" => {
            let name = node_text(node, source);
            type_env.lookup(&name).cloned().unwrap_or(P4Type::Unknown)
        }
        "lval" => infer_lval_type(node, source, type_env),
        "expr" => {
            // expr 可能是包装节点，也可能是二元表达式
            // 检查是否有 binop 子节点
            let mut has_binop = false;
            for i in 0..node.child_count() {
                if let Some(child) = node.child(i) {
                    if child.kind() == "binop" {
                        has_binop = true;
                        break;
                    }
                }
            }
            if has_binop {
                // 二元表达式: expr binop expr
                let left = node.child(0);
                let right = node.child(2);
                if let (Some(l), Some(r)) = (left, right) {
                    let lty = infer_expr_type(l, source, type_env);
                    let rty = infer_expr_type(r, source, type_env);
                    if let Some(op_node) = node.child(1) {
                        let op_text = node_text(op_node, source);
                        return merge_binary_op_types(&lty, &rty, &op_text);
                    }
                    return merge_binary_op_types(&lty, &rty, "+");
                }
                P4Type::Unknown
            } else {
                // 包装节点，递归到第一个子节点
                if let Some(child) = node.child(0) {
                    infer_expr_type(child, source, type_env)
                } else {
                    P4Type::Unknown
                }
            }
        }
        "call" => {
            // 方法调用，返回类型由方法签名决定
            if let Some(target) = node.child(0) {
                let target_name = node_text(target, source);
                type_env.lookup_method_return(&target_name).cloned().unwrap_or(P4Type::Unknown)
            } else {
                P4Type::Unknown
            }
        }
        "binop" => {
            // 二元操作符节点本身不包含操作数，操作数在父级 expr 中
            // 但如果 binop 是独立传入的（如从 expr 中提取），需要父级上下文
            P4Type::Unknown
        }
        "unop" => {
            if let Some(operand) = node.child(1) {
                infer_expr_type(operand, source, type_env)
            } else {
                P4Type::Unknown
            }
        }
        "cast" => {
            // (type) expr
            if let Some(type_node) = node.child_by_field_name("type") {
                parse_type_node(type_node, source)
            } else {
                P4Type::Unknown
            }
        }
        "array_index" => {
            // expr [ index ]
            if let Some(base) = node.child(0) {
                let base_ty = infer_expr_type(base, source, type_env);
                if let P4Type::Array(elem_ty, _) = base_ty {
                    *elem_ty.clone()
                } else {
                    P4Type::Unknown
                }
            } else {
                P4Type::Unknown
            }
        }
        _ => {
            if P4_KEYWORDS.contains(&kind) {
                match kind {
                    "bit" => P4Type::Bit(32),
                    "int" => P4Type::Int(32),
                    "bool" => P4Type::Bool,
                    "void" => P4Type::Void,
                    "string" => P4Type::String,
                    "error" => P4Type::Error,
                    "match_kind" => P4Type::MatchKind,
                    _ => P4Type::Unknown,
                }
            } else {
                P4Type::Unknown
            }
        }
    }
}

/// 解析 lval 节点类型：单变量或字段访问链
fn infer_lval_type(node: Node, source: &str, type_env: &TypeEnv) -> P4Type {
    // lval: seq($.identifier, repeat(seq(".", $.identifier)))
    let mut children = Vec::new();
    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            if child.kind() == "identifier" {
                children.push(node_text(child, source));
            }
        }
    }

    if children.is_empty() {
        return P4Type::Unknown;
    }

    if children.len() == 1 {
        // 单变量
        let name = &children[0];
        return type_env.lookup(name).cloned().unwrap_or(P4Type::Unknown);
    }

    // 字段访问链：eth.dst_addr.src_port...
    let mut current_ty = type_env.lookup(&children[0]).cloned().unwrap_or(P4Type::Unknown);
    for field_name in &children[1..] {
        let type_name = match current_ty {
            P4Type::Header(ref n) | P4Type::Struct(ref n) | P4Type::Extern(ref n) => n.clone(),
            _ => return P4Type::Unknown,
        };
        current_ty = type_env.lookup_field(&type_name, field_name)
            .unwrap_or(P4Type::Unknown);
    }
    current_ty
}

/// 类型环境：名字 -> 类型
#[derive(Debug, Clone, Default)]
pub struct TypeEnv {
    locals: Vec<(String, P4Type)>,
    params: Vec<(String, P4Type)>,
    type_defs: std::collections::HashMap<String, Vec<(String, P4Type)>>,
    method_sigs: std::collections::HashMap<String, (Vec<P4Type>, P4Type)>,
}

impl TypeEnv {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_local(&mut self, name: &str, ty: P4Type) {
        self.locals.push((name.to_string(), ty));
    }

    pub fn add_param(&mut self, name: &str, ty: P4Type) {
        self.params.push((name.to_string(), ty));
    }

    pub fn add_type_def(&mut self, name: &str, fields: Vec<(String, P4Type)>) {
        self.type_defs.insert(name.to_string(), fields);
    }

    pub fn add_method_sig(&mut self, name: &str, params: Vec<P4Type>, ret: P4Type) {
        self.method_sigs.insert(name.to_string(), (params, ret));
    }

    pub fn lookup(&self, name: &str) -> Option<&P4Type> {
        self.locals.iter().rev().find(|(n, _)| n == name)
            .map(|(_, ty)| ty)
            .or_else(|| self.params.iter().rev().find(|(n, _)| n == name).map(|(_, ty)| ty))
    }

    pub fn lookup_method_return(&self, name: &str) -> Option<&P4Type> {
        self.method_sigs.get(name).map(|(_, ret)| ret)
    }

    pub fn lookup_field(&self, type_name: &str, field: &str) -> Option<P4Type> {
        self.type_defs.get(type_name)
            .and_then(|fields| fields.iter().find(|(n, _)| n == field).map(|(_, ty)| ty.clone()))
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn node_text(node: Node, source: &str) -> String {
    source[node.start_byte()..node.end_byte()].to_string()
}

fn merge_binary_op_types(left: &P4Type, right: &P4Type, _op_kind: &str) -> P4Type {
    match (left, right) {
        (P4Type::Bit(w1), P4Type::Bit(w2)) => P4Type::Bit(*w1.max(w2)),
        (P4Type::Int(w1), P4Type::Int(w2)) => P4Type::Int(*w1.max(w2)),
        (P4Type::Bool, P4Type::Bool) => P4Type::Bool,
        _ => left.clone(),
    }
}

fn parse_type_node(node: Node, source: &str) -> P4Type {
    let kind = node.kind();
    match kind {
        "bit_type" => {
            // bit_type: seq("bit", "<", $.number, ">")
            let text = node_text(node, source);
            if let Some(start) = text.find('<') {
                if let Some(end) = text.find('>') {
                    let w = text[start+1..end].parse::<u32>().unwrap_or(32);
                    return P4Type::Bit(w);
                }
            }
            P4Type::Bit(32)
        }
        "int_type" => {
            let text = node_text(node, source);
            if let Some(start) = text.find('<') {
                if let Some(end) = text.find('>') {
                    let w = text[start+1..end].parse::<u32>().unwrap_or(32);
                    return P4Type::Int(w);
                }
            }
            P4Type::Int(32)
        }
        "varbit_type" => {
            let text = node_text(node, source);
            if let Some(start) = text.find('<') {
                if let Some(end) = text.find('>') {
                    let w = text[start+1..end].parse::<u32>().unwrap_or(32);
                    return P4Type::Varbit(w);
                }
            }
            P4Type::Varbit(32)
        }
        "bool" => P4Type::Bool,
        "void" => P4Type::Void,
        "string" => P4Type::String,
        "error" => P4Type::Error,
        "match_kind" => P4Type::MatchKind,
        "type_identifier" => {
            let text = node_text(node, source);
            P4Type::Header(text)
        }
        _ => P4Type::Unknown,
    }
}

const P4_KEYWORDS: &[&str] = &[
    "bit", "int", "bool", "varbit", "void", "string", "error", "match_kind",
    "packet_in", "packet_out", "header", "struct", "enum", "extern",
    "parser", "control", "action", "table", "function", "package",
];

// ---------------------------------------------------------------------------
// 类型检查诊断
// ---------------------------------------------------------------------------

/// 从 AST 收集类型相关诊断
pub fn type_diagnostics(tree: &Tree, source: &str, env: &TypeEnv) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    check_node_types(tree.root_node(), source, env, &mut diagnostics);
    diagnostics
}

fn check_node_types(node: Node, source: &str, env: &TypeEnv, out: &mut Vec<Diagnostic>) {
    let kind = node.kind();
    match kind {
        "var_decl" => {
            // 查找 var_decl 中的类型节点
            let mut type_node = None;
            for i in 0..node.child_count() {
                if let Some(child) = node.child(i) {
                    match child.kind() {
                        "bit_type" | "int_type" | "varbit_type" | "type_identifier" |
                        "bool" | "string" | "void" | "error" | "match_kind" => {
                            type_node = Some(child);
                        }
                        _ => {}
                    }
                }
            }
            // 查找 var_choice 中的初始化表达式
            let mut init_expr = None;
            for i in 0..node.child_count() {
                if let Some(child) = node.child(i) {
                    if child.kind() == "var_choice" {
                        for j in 0..child.child_count() {
                            if let Some(gc) = child.child(j) {
                                match gc.kind() {
                                    "expr" | "lval" | "number" | "string_literal" | "true" | "false" => {
                                        init_expr = Some(gc);
                                        break;
                                    }
                                    _ => {}
                                }
                            }
                        }
                        break;
                    }
                }
            }
            if let (Some(ty_node), Some(init)) = (type_node, init_expr) {
                let decl_ty = parse_type_node(ty_node, source);
                let expr_ty = infer_expr_type(init, source, env);
                if decl_ty != P4Type::Unknown && expr_ty != P4Type::Unknown && decl_ty != expr_ty {
                    out.push(type_mismatch_diagnostic(
                        init, source, &decl_ty.to_string(), &expr_ty.to_string(),
                    ));
                }
            }
        }
        "assignment" | "compound_assignment" => {
            if let Some(lval) = node.child_by_field_name("left") {
                if let Some(rval) = node.child_by_field_name("right") {
                    let lty = infer_expr_type(lval, source, env);
                    let rty = infer_expr_type(rval, source, env);
                    if lty != P4Type::Unknown && rty != P4Type::Unknown && !is_assignable(&lty, &rty) {
                        out.push(type_mismatch_diagnostic(
                            rval, source, &lty.to_string(), &rty.to_string(),
                        ));
                    }
                }
            }
        }
        "call" => {
            check_call_args(node, source, env, out);
        }
        _ => {}
    }

    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            check_node_types(child, source, env, out);
        }
    }
}

fn check_call_args(node: Node, source: &str, env: &TypeEnv, out: &mut Vec<Diagnostic>) {
    if let Some(target) = node.child(0) {
        let target_name = node_text(target, source);
        if let Some((expected_types, _)) = env.method_sigs.get(&target_name) {
            let mut actual_types = Vec::new();
            for i in 1..node.child_count() {
                if let Some(arg) = node.child(i) {
                    if arg.kind() != "(" && arg.kind() != ")" && arg.kind() != "," {
                        actual_types.push(infer_expr_type(arg, source, env));
                    }
                }
            }
            if actual_types.len() != expected_types.len() {
                out.push(Diagnostic {
                    range: ts_range_to_lsp(node),
                    severity: Some(DiagnosticSeverity::ERROR),
                    code: None,
                    code_description: None,
                    source: Some("p4lsp".to_string()),
                    message: format!(
                        "Expected {} arguments, found {}",
                        expected_types.len(),
                        actual_types.len()
                    ),
                    related_information: None,
                    tags: None,
                    data: None,
                });
                return;
            }
            let mut arg_idx = 0;
            for i in 1..node.child_count() {
                if let Some(arg) = node.child(i) {
                    if arg.kind() != "(" && arg.kind() != ")" && arg.kind() != "," {
                        let expected = &expected_types[arg_idx];
                        let actual = &actual_types[arg_idx];
                        if *expected != P4Type::Unknown && *actual != P4Type::Unknown && expected != actual {
                            out.push(type_mismatch_diagnostic(
                                arg, source, &expected.to_string(), &actual.to_string(),
                            ));
                        }
                        arg_idx += 1;
                    }
                }
            }
        }
    }
}

fn is_assignable(left: &P4Type, right: &P4Type) -> bool {
    match (left, right) {
        (a, b) if a == b => true,
        (P4Type::Bit(w1), P4Type::Bit(w2)) => w1 == w2,
        (P4Type::Int(w1), P4Type::Int(w2)) => w1 == w2,
        (P4Type::Bit(_), P4Type::Int(_)) => true,
        _ => false,
    }
}

fn type_mismatch_diagnostic(node: Node, _source: &str, expected: &str, found: &str) -> Diagnostic {
    Diagnostic {
        range: ts_range_to_lsp(node),
        severity: Some(DiagnosticSeverity::ERROR),
        code: None,
        code_description: None,
        source: Some("p4lsp".to_string()),
        message: format!("Type mismatch: expected '{}', found '{}'", expected, found),
        related_information: None,
        tags: None,
        data: None,
    }
}

fn ts_range_to_lsp(node: Node) -> Range {
    Range {
        start: Position {
            line: node.start_position().row as u32,
            character: node.start_position().column as u32,
        },
        end: Position {
            line: node.end_position().row as u32,
            character: node.end_position().column as u32,
        },
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser;

    fn parse_p4(source: &str) -> Tree {
        let mut p = parser::new_parser().expect("new_parser should succeed");
        p.parse(source, None).expect("parse should succeed")
    }

    fn make_env() -> TypeEnv {
        let mut env = TypeEnv::new();
        env.add_param("eth", P4Type::Header("ethernet_t".to_string()));
        env.add_param("port", P4Type::Bit(9));
        env.add_local("x", P4Type::Bit(8));
        env.add_local("flag", P4Type::Bool);
        env.add_local("a", P4Type::Bit(8));  // for type mismatch test
        env.add_type_def("ethernet_t", vec![
            ("dst_addr".to_string(), P4Type::Bit(48)),
            ("src_addr".to_string(), P4Type::Bit(48)),
            ("ether_type".to_string(), P4Type::Bit(16)),
        ]);
        env.add_method_sig("extract", vec![P4Type::Header("ethernet_t".to_string())], P4Type::Void);
        env
    }

    /// 从 AST 中递归查找 var_choice 节点，然后提取其中的 expr
    fn find_expr_in_var_decl(root: Node) -> Option<Node> {
        fn find_var_choice(node: Node) -> Option<Node> {
            if node.kind() == "var_choice" {
                for j in 0..node.child_count() {
                    if let Some(gc) = node.child(j) {
                        if gc.kind() == "expr" {
                            return Some(gc);
                        }
                    }
                }
                return None;
            }
            for i in 0..node.child_count() {
                if let Some(child) = node.child(i) {
                    if let Some(result) = find_var_choice(child) {
                        return Some(result);
                    }
                }
            }
            None
        }
        find_var_choice(root)
    }

    #[test]
    fn test_infer_number_literal() {
        let source = r#"control C() { apply { bit<8> a = 1; } }"#;
        let tree = parse_p4(source);
        let env = make_env();
        let expr = find_expr_in_var_decl(tree.root_node()).expect("should find expr");
        let ty = infer_expr_type(expr, source, &env);
        assert_eq!(ty, P4Type::Bit(32), "number literal should be bit<32>");
    }

    #[test]
    fn test_infer_bool_literal() {
        let source = r#"control C() { apply { bool b = true; } }"#;
        let tree = parse_p4(source);
        let env = make_env();
        let expr = find_expr_in_var_decl(tree.root_node()).expect("should find expr");
        let ty = infer_expr_type(expr, source, &env);
        assert_eq!(ty, P4Type::Bool, "true should be bool");
    }

    #[test]
    fn test_infer_local_variable() {
        let source = r#"control C() { apply { bit<8> a = x; } }"#;
        let tree = parse_p4(source);
        let env = make_env();
        let expr = find_expr_in_var_decl(tree.root_node()).expect("should find expr");
        let ty = infer_expr_type(expr, source, &env);
        assert_eq!(ty, P4Type::Bit(8), "x should be bit<8>");
    }

    #[test]
    fn test_infer_member_access() {
        let source = r#"control C() { apply { bit<48> a = eth.dst_addr; } }"#;
        let tree = parse_p4(source);
        let env = make_env();
        let expr = find_expr_in_var_decl(tree.root_node()).expect("should find expr");
        let ty = infer_expr_type(expr, source, &env);
        assert_eq!(ty, P4Type::Bit(48), "eth.dst_addr should be bit<48>");
    }

    #[test]
    fn test_infer_binary_op() {
        let source = r#"control C() { apply { bit<32> a = x + port; } }"#;
        let tree = parse_p4(source);
        let env = make_env();
        let expr = find_expr_in_var_decl(tree.root_node()).expect("should find expr");
        let ty = infer_expr_type(expr, source, &env);
        assert_eq!(ty, P4Type::Bit(9), "bit<8> + bit<9> should be bit<9> (max width)");
    }

    #[test]
    fn test_type_mismatch_diagnostic() {
        let source = r#"
control C() {
    apply {
        bool b = a;
    }
}
"#;
        let tree = parse_p4(source);
        let env = make_env();
        let diagnostics = type_diagnostics(&tree, source, &env);
        assert!(
            diagnostics.iter().any(|d| d.message.contains("Type mismatch")),
            "should detect type mismatch: bool = bit<8>, got: {:?}", diagnostics
        );
    }

    #[test]
    fn test_assignable_types() {
        assert!(is_assignable(&P4Type::Bit(32), &P4Type::Bit(32)));
        assert!(!is_assignable(&P4Type::Bit(8), &P4Type::Bit(16)));
        assert!(is_assignable(&P4Type::Bit(32), &P4Type::Int(32)));
    }
}
