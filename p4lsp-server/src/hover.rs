use tower_lsp::lsp_types::{Hover, HoverContents, MarkupContent, MarkupKind, Position, Url};
use tree_sitter::{Node, Point, Tree};
use crate::workspace::WorkspaceIndex;

/// 生成 Hover 信息（向后兼容，无 workspace index）。
pub fn hover(tree: &Tree, source: &str, pos: Position) -> Option<Hover> {
    hover_with_workspace(tree, source, pos, None, None)
}

/// 生成 Hover 信息（支持 workspace index 做跨文件语义解析）。
pub fn hover_with_workspace(
    tree: &Tree,
    source: &str,
    pos: Position,
    index: Option<&WorkspaceIndex>,
    uri: Option<&Url>,
) -> Option<Hover> {
    let root = tree.root_node();
    let point = lsp_pos_to_ts_point(pos);
    let node = root.descendant_for_point_range(point, point)?;

    let contents = hover_for_node(node, source, index, uri)?;
    Some(Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value: contents,
        }),
        range: None,
    })
}

fn lsp_pos_to_ts_point(pos: Position) -> Point {
    Point {
        row: pos.line as usize,
        column: pos.character as usize,
    }
}

/// 根据光标所在节点生成对应的 Hover 文本。
fn hover_for_node(node: Node, source: &str, _index: Option<&WorkspaceIndex>, _uri: Option<&Url>) -> Option<String> {
    let kind = node.kind();

    // 1. 定义节点：header/struct/enum/parser/control/action/table/extern
    if let Some(def) = find_ancestor_definition(node, source) {
        return Some(def);
    }

    // 2. 方法调用：fval 在 call 内部
    if let Some(call) = find_call_context(node) {
        return Some(hover_for_call(call, source));
    }

    // 3. 字段访问：lval（identifier 链）
    if let Some(lval) = find_lval_context(node) {
        return Some(hover_for_lval(lval, source));
    }

    // 4. 如果光标落在 method_identifier 或 identifier 上，尝试向上匹配定义
    if kind == "method_identifier" || kind == "identifier" || kind == "type_identifier" {
        if let Some(def) = find_definition_by_name_node(node, source) {
            return Some(def);
        }
    }

    None
}

// ---------------------------------------------------------------------------
// 定义摘要
// ---------------------------------------------------------------------------

fn find_ancestor_definition(node: Node, source: &str) -> Option<String> {
    let mut current = node;
    loop {
        let kind = current.kind();
        match kind {
            "header_definition" | "header_union_definition" => {
                return Some(hover_header_definition(current, source, kind));
            }
            "struct_definition" => return Some(hover_struct_definition(current, source)),
            "enum_definition" => return Some(hover_enum_definition(current, source)),
            "parser_definition" => return Some(hover_parser_definition(current, source)),
            "control_definition" => return Some(hover_control_definition(current, source)),
            "action" => return Some(hover_action_definition(current, source)),
            "table" => return Some(hover_table_definition(current, source)),
            "extern_definition" => return Some(hover_extern_definition(current, source)),
            "method" => return Some(hover_method_definition(current, source)),
            _ => {}
        }
        // Stop at expression/statement boundaries — don't cross over to parent definition
        if is_expression_boundary(kind) {
            return None;
        }
        if let Some(parent) = current.parent() {
            current = parent;
        } else {
            break;
        }
    }
    None
}

fn is_expression_boundary(kind: &str) -> bool {
    matches!(
        kind,
        "call"
            | "lval"
            | "expr"
            | "conditional"
            | "block_statement"
            | "stmt"
            | "state"
            | "for_statement"
            | "switch_statement"
            | "table_element"
    )
}

fn hover_header_definition(node: Node, source: &str, kind: &str) -> String {
    let keyword = if kind == "header_union_definition" {
        "header_union"
    } else {
        "header"
    };
    let name = child_text_by_kind(node, "type_identifier", source).unwrap_or_default();
    let mut fields = Vec::new();
    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            if child.kind() == "field" {
                if let Some(f) = field_summary(child, source) {
                    fields.push(f);
                }
            }
        }
    }
    let fields_str = if fields.is_empty() {
        String::new()
    } else {
        format!("\n\n**Fields:**\n{}", fields.join("\n"))
    };
    format!("**{keyword}** `{name}` {fields_str}")
}

fn hover_struct_definition(node: Node, source: &str) -> String {
    let name = child_text_by_kind(node, "type_identifier", source).unwrap_or_default();
    let mut fields = Vec::new();
    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            if child.kind() == "field" {
                if let Some(f) = field_summary(child, source) {
                    fields.push(f);
                }
            }
        }
    }
    let fields_str = if fields.is_empty() {
        String::new()
    } else {
        format!("\n\n**Fields:**\n{}", fields.join("\n"))
    };
    format!("**struct** `{name}` {fields_str}")
}

fn hover_enum_definition(node: Node, source: &str) -> String {
    let name = child_text_by_kind(node, "type_identifier", source).unwrap_or_default();
    let mut members = Vec::new();
    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            if child.kind() == "identifier" {
                members.push(format!("- `{}`", node_text(child, source)));
            }
        }
    }
    let members_str = if members.is_empty() {
        String::new()
    } else {
        format!("\n\n**Members:**\n{}", members.join("\n"))
    };
    format!("**enum** `{name}` {members_str}")
}

fn hover_parser_definition(node: Node, source: &str) -> String {
    let name = child_text_by_kind(node, "method_identifier", source)
        .or_else(|| child_text_by_kind(node, "type_identifier", source))
        .unwrap_or_default();
    let params = parameters_summary(node, source);
    let mut states = Vec::new();
    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            if child.kind() == "state" {
                if let Some(s) = state_name(child, source) {
                    states.push(format!("- `{}`", s));
                }
            }
        }
    }
    let states_str = if states.is_empty() {
        String::new()
    } else {
        format!("\n\n**States:**\n{}", states.join("\n"))
    };
    format!("**parser** `{name}({params})` {states_str}")
}

fn hover_control_definition(node: Node, source: &str) -> String {
    let name = child_text_by_kind(node, "method_identifier", source)
        .or_else(|| child_text_by_kind(node, "type_identifier", source))
        .unwrap_or_default();
    let params = parameters_summary(node, source);
    format!("**control** `{name}({params})`")
}

fn hover_action_definition(node: Node, source: &str) -> String {
    let name = child_text_by_kind(node, "method_identifier", source).unwrap_or_default();
    let params = parameters_summary(node, source);
    format!("**action** `{name}({params})`")
}

fn hover_table_definition(node: Node, source: &str) -> String {
    let name = child_text_by_kind(node, "type_identifier", source).unwrap_or_default();
    let mut keys = Vec::new();
    let mut actions = Vec::new();
    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            if child.kind() == "table_element" {
                for j in 0..child.child_count() {
                    if let Some(elem) = child.child(j) {
                        if elem.kind() == "key" {
                            keys.push(format!("- `key` block"));
                        }
                        if elem.kind() == "actions" {
                            actions.push(format!("- `actions` block"));
                        }
                    }
                }
            }
        }
    }
    let mut parts = vec![format!("**table** `{name}`")];
    if !keys.is_empty() {
        parts.push(format!("\n**Keys:**\n{}", keys.join("\n")));
    }
    if !actions.is_empty() {
        parts.push(format!("\n**Actions:**\n{}", actions.join("\n")));
    }
    parts.join("")
}

fn hover_extern_definition(node: Node, source: &str) -> String {
    let name = child_text_by_kind(node, "type_identifier", source).unwrap_or_default();
    let mut methods = Vec::new();
    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            if child.kind() == "method" {
                if let Some(m) = method_summary(child, source) {
                    methods.push(format!("- {m}"));
                }
            }
        }
    }
    let methods_str = if methods.is_empty() {
        String::new()
    } else {
        format!("\n\n**Methods:**\n{}", methods.join("\n"))
    };
    format!("**extern** `{name}` {methods_str}")
}

fn hover_method_definition(node: Node, source: &str) -> String {
    method_summary(node, source).unwrap_or_else(|| "**method**".to_string())
}

// ---------------------------------------------------------------------------
// 辅助：字段 / 参数 / 方法摘要
// ---------------------------------------------------------------------------

fn field_summary(node: Node, source: &str) -> Option<String> {
    let ty = field_type_text(node, source)?;
    let name = child_text_by_kind(node, "identifier", source)?;
    Some(format!("- `{ty} {name}`"))
}

fn field_type_text(node: Node, source: &str) -> Option<String> {
    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            let k = child.kind();
            if k == "type_identifier"
                || k == "bit_type"
                || k == "varbit_type"
                || k == "tuple_type"
                || k == "bool"
                || k == "int"
                || k == "bit"
                || k == "varbit"
                || k == "error"
                || k == "packet_in"
                || k == "packet_out"
            {
                return Some(node_text(child, source).to_string());
            }
        }
    }
    None
}

fn parameters_summary(node: Node, source: &str) -> String {
    let mut params = Vec::new();
    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            if child.kind() == "parameter" {
                if let Some(p) = parameter_summary(child, source) {
                    params.push(p);
                }
            }
        }
    }
    params.join(", ")
}

fn parameter_summary(node: Node, source: &str) -> Option<String> {
    let mut dir = String::new();
    let mut ty = String::new();
    let mut name = String::new();
    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            let k = child.kind();
            if k == "direction" {
                dir = format!("{} ", node_text(child, source));
            }
            if k == "identifier" || k == "type_identifier" {
                if ty.is_empty() && (k == "type_identifier" || is_type_like(child, source)) {
                    ty = node_text(child, source).to_string();
                } else {
                    name = node_text(child, source).to_string();
                }
            }
            if k == "bit_type" || k == "varbit_type" || k == "tuple_type" || k == "bool" || k == "int" || k == "bit" || k == "varbit" || k == "error" || k == "packet_in" || k == "packet_out" {
                ty = node_text(child, source).to_string();
            }
        }
    }
    if name.is_empty() {
        // 无名字，仅类型
        if !ty.is_empty() {
            return Some(format!("{dir}{ty}"));
        }
        return None;
    }
    Some(format!("{dir}{ty} {name}"))
}

fn is_type_like(node: Node, source: &str) -> bool {
    let text = node_text(node, source);
    matches!(
        text,
        "bool" | "int" | "bit" | "varbit" | "error" | "packet_in" | "packet_out" | "string"
    )
}

fn method_summary(node: Node, source: &str) -> Option<String> {
    let name = child_text_by_kind(node, "method_identifier", source)?;
    let mut ret = String::new();
    let params = parameters_summary(node, source);
    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            let k = child.kind();
            if k == "type_identifier" || k == "bit_type" || k == "varbit_type" || k == "tuple_type" || k == "bool" || k == "int" || k == "bit" || k == "varbit" || k == "error" || k == "packet_in" || k == "packet_out" {
                if ret.is_empty() {
                    ret = node_text(child, source).to_string();
                }
            }
        }
    }
    if ret.is_empty() {
        ret = "void".to_string();
    }
    Some(format!("`{ret} {name}({params})`"))
}

fn state_name(node: Node, source: &str) -> Option<String> {
    child_text_by_kind(node, "method_identifier", source)
}

pub fn node_text<'a>(node: Node, source: &'a str) -> &'a str {
    &source[node.start_byte()..node.end_byte()]
}

fn child_text_by_kind(node: Node, kind: &str, source: &str) -> Option<String> {
    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            if child.kind() == kind {
                return Some(node_text(child, source).to_string());
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------
// 定义查找：当光标落在名字节点上时，向上找到对应的定义
// ---------------------------------------------------------------------------

fn find_definition_by_name_node(node: Node, source: &str) -> Option<String> {
    let mut current = node;
    while let Some(parent) = current.parent() {
        let pkind = parent.kind();
        match pkind {
            "header_definition" | "header_union_definition" => {
                return Some(hover_header_definition(parent, source, pkind));
            }
            "struct_definition" => return Some(hover_struct_definition(parent, source)),
            "enum_definition" => return Some(hover_enum_definition(parent, source)),
            "parser_definition" => return Some(hover_parser_definition(parent, source)),
            "control_definition" => return Some(hover_control_definition(parent, source)),
            "action" => return Some(hover_action_definition(parent, source)),
            "table" => return Some(hover_table_definition(parent, source)),
            "extern_definition" => return Some(hover_extern_definition(parent, source)),
            "method" => return Some(hover_method_definition(parent, source)),
            "state" => return Some(format!("**state** `{}`", state_name(parent, source)?)),
            "field" => return Some(field_summary(parent, source)?),
            "parameter" => return Some(parameter_summary(parent, source)?),
            _ => {}
        }
        current = parent;
    }
    None
}

// ---------------------------------------------------------------------------
// 方法调用 (call / fval)
// ---------------------------------------------------------------------------

fn find_call_context(node: Node) -> Option<Node> {
    let mut current = node;
    loop {
        if current.kind() == "call" {
            return Some(current);
        }
        if let Some(parent) = current.parent() {
            current = parent;
        } else {
            break;
        }
    }
    None
}

fn hover_for_call(call: Node, source: &str) -> String {
    // 提取 fval（被调用者名称）
    let fval_opt = call.child(0).filter(|c| c.kind() == "fval");
    let fval_text = fval_opt.map(|f| node_text(f, source).to_string()).unwrap_or_default();

    // 提取 call 的参数列表文本
    let mut args = Vec::new();
    for i in 0..call.child_count() {
        if let Some(c) = call.child(i) {
            if c.kind() == "expr" || c.kind() == "identifier" || c.kind() == "number" {
                args.push(node_text(c, source).to_string());
            }
        }
    }
    let args_str = if args.is_empty() {
        "()".to_string()
    } else {
        format!("({})", args.join(", "))
    };

    // 尝试解析 extern 或 action 的签名
    // 简化版：如果是简单标识符，在当前文件 AST 中查找 extern 定义
    let signature = if let Some(first_name) = fval_text.split('.').next() {
        find_method_signature_in_ast(call, first_name, &fval_text, source)
    } else {
        None
    };

    if let Some(sig) = signature {
        format!("**Call** `{}`  \n{}", fval_text, sig)
    } else {
        format!("**Call** `{}`{}", fval_text, args_str)
    }
}

/// 在 AST 中查找方法签名（简化版：在当前文件的顶层 extern 定义中查找）
fn find_method_signature_in_ast(node: Node, first_name: &str, full_path: &str, source: &str) -> Option<String> {
    // 向上找到 root，遍历顶层 extern 定义
    let root = {
        let mut current = node;
        loop {
            if let Some(p) = current.parent() {
                current = p;
            } else {
                break current;
            }
        }
    };

    // 如果 fval 是链式调用（如 pkt.extract），解析第一个标识符的类型
    let parts: Vec<&str> = full_path.split('.').collect();
    if parts.len() >= 2 {
        // 链式调用：先找第一个标识符的类型，再在类型定义中找方法
        let base_name = parts[0];
        // 在当前文件中查找变量/参数声明，获取类型
        let base_type = find_identifier_type_in_scope(root, base_name, source);
        if let Some(ty) = base_type {
            // 在顶层查找该类型的 extern 定义
            let method_name = parts.last().unwrap_or(&"");
            if let Some(sig) = find_extern_method_signature(root, &ty, method_name, source) {
                return Some(sig);
            }
        }
    } else {
        // 简单调用：直接在当前文件中查找 action 或 extern 方法
        if let Some(sig) = find_action_signature(root, first_name, source) {
            return Some(sig);
        }
        if let Some(sig) = find_extern_method_signature(root, first_name, first_name, source) {
            return Some(sig);
        }
    }
    None
}

/// 在 AST 中查找标识符的类型（从变量声明或参数）
fn find_identifier_type_in_scope(root: Node, name: &str, source: &str) -> Option<String> {
    // 遍历所有 control/parser 等定义，查找参数或局部变量声明
    for i in 0..root.child_count() {
        if let Some(child) = root.child(i) {
            let result = find_type_in_node(child, name, source);
            if result.is_some() {
                return result;
            }
        }
    }
    None
}

fn find_type_in_node(node: Node, name: &str, source: &str) -> Option<String> {
    let kind = node.kind();
    // 在参数中查找
    if kind == "parameter" {
        if let Some((param_name, param_ty)) = extract_parameter_name_type(node, source) {
            if param_name == name {
                return Some(param_ty);
            }
        }
    }
    // 在变量声明中查找
    if kind == "variable_declaration" || kind == "var_decl" {
        if let Some(var_name) = child_text_by_kind(node, "identifier", source) {
            if var_name == name {
                if let Some(ty) = child_text_by_kind(node, "type_identifier", source)
                    .or_else(|| child_text_by_kind(node, "bit_type", source))
                    .or_else(|| child_text_by_kind(node, "varbit_type", source))
                {
                    return Some(ty);
                }
            }
        }
    }
    // 递归查找子节点
    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            let result = find_type_in_node(child, name, source);
            if result.is_some() {
                return result;
            }
        }
    }
    None
}

fn extract_parameter_name_type(node: Node, source: &str) -> Option<(String, String)> {
    let mut name = String::new();
    let mut ty = String::new();
    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            let k = child.kind();
            if k == "identifier" {
                name = node_text(child, source).to_string();
            }
            if k == "type_identifier" {
                ty = node_text(child, source).to_string();
            }
        }
    }
    if name.is_empty() {
        None
    } else {
        Some((name, ty))
    }
}

/// 查找 action 签名
fn find_action_signature(root: Node, name: &str, source: &str) -> Option<String> {
    for i in 0..root.child_count() {
        if let Some(child) = root.child(i) {
            if child.kind() == "action" {
                if let Some(action_name) = child_text_by_kind(child, "method_identifier", source) {
                    if action_name == name {
                        let params = parameters_summary(child, source);
                        return Some(format!("**action** `{}({})`", name, params));
                    }
                }
            }
        }
    }
    None
}

/// 查找 extern 方法签名
fn find_extern_method_signature(root: Node, type_name: &str, method_name: &str, source: &str) -> Option<String> {
    for i in 0..root.child_count() {
        if let Some(child) = root.child(i) {
            if child.kind() == "extern_definition" {
                // 检查 extern 名称是否匹配
                if let Some(ext_name) = child_text_by_kind(child, "type_identifier", source) {
                    if ext_name == type_name {
                        // 在 extern 定义中查找方法
                        for j in 0..child.child_count() {
                            if let Some(method) = child.child(j) {
                                if method.kind() == "method" {
                                    if let Some(m_name) = child_text_by_kind(method, "method_identifier", source) {
                                        if m_name == method_name {
                                            let params = parameters_summary(method, source);
                                            let ret = child_text_by_kind(method, "type_identifier", source)
                                                .or_else(|| child_text_by_kind(method, "bit_type", source))
                                                .or_else(|| child_text_by_kind(method, "varbit_type", source))
                                                .unwrap_or_default();
                                            if ret.is_empty() {
                                                return Some(format!("**method** `{}({})`", method_name, params));
                                            } else {
                                                return Some(format!("**method** `{} {}({})`", ret, method_name, params));
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------
// 字段访问 (lval)
// ---------------------------------------------------------------------------

fn find_lval_context(node: Node) -> Option<Node> {
    let mut current = node;
    loop {
        if current.kind() == "lval" {
            return Some(current);
        }
        if let Some(parent) = current.parent() {
            current = parent;
        } else {
            break;
        }
    }
    None
}

fn hover_for_lval(lval: Node, source: &str) -> String {
    let expr = node_text(lval, source);
    
    // 提取 lval 中的标识符链（如 hdr.ethernet.dstAddr）
    let mut parts = Vec::new();
    for i in 0..lval.child_count() {
        if let Some(child) = lval.child(i) {
            if child.kind() == "identifier" {
                parts.push(node_text(child, source).to_string());
            }
        }
    }
    
    // 尝试解析字段链的类型
    let type_info = if parts.len() >= 2 {
        // 从第一个标识符的类型开始，逐层解析字段类型
        resolve_lval_type(lval, &parts, source)
    } else {
        None
    };
    
    if let Some(ty) = type_info {
        format!("**Field access** `{}`  \n**Type:** `{}`", expr, ty)
    } else {
        format!("**Field access** `{}`", expr)
    }
}

/// 解析 lval 标识符链的类型（简化版）
fn resolve_lval_type(lval: Node, parts: &[String], source: &str) -> Option<String> {
    // 向上找到 root
    let root = {
        let mut current = lval;
        loop {
            if let Some(p) = current.parent() {
                current = p;
            } else {
                break current;
            }
        }
    };
    
    // 查找第一个标识符的类型
    let base_name = &parts[0];
    let base_type = find_identifier_type_in_scope(root, base_name, source)?;
    
    // 逐层解析字段类型
    let mut current_type = base_type;
    for part in &parts[1..] {
        let field_type = find_field_type_in_ast(root, &current_type, part, source);
        if let Some(ty) = field_type {
            current_type = ty;
        } else {
            return Some(current_type); // 返回当前已解析到的类型
        }
    }
    
    Some(current_type)
}

/// 在 AST 中查找某类型的字段类型
fn find_field_type_in_ast(root: Node, type_name: &str, field_name: &str, source: &str) -> Option<String> {
    // 在顶层查找 struct/header 定义
    for i in 0..root.child_count() {
        if let Some(child) = root.child(i) {
            if child.kind() == "struct_definition" || child.kind() == "header_definition" || child.kind() == "header_union_definition" {
                if let Some(def_name) = child_text_by_kind(child, "type_identifier", source) {
                    if def_name == type_name {
                        // 在该定义中查找字段
                        for j in 0..child.child_count() {
                            if let Some(field) = child.child(j) {
                                if field.kind() == "field" {
                                    if let Some(f_name) = child_text_by_kind(field, "identifier", source) {
                                        if f_name == field_name {
                                            return field_type_text(field, source);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Signature Help 支持
// ---------------------------------------------------------------------------

/// 为 Signature Help 查找方法签名字符串
pub fn find_method_signature_for_signature_help(call: &Node, fval_text: &str, source: &str) -> Option<String> {
    let root = {
        let mut current = *call;
        loop {
            if let Some(p) = current.parent() {
                current = p;
            } else {
                break current;
            }
        }
    };
    
    let parts: Vec<&str> = fval_text.split('.').collect();
    if parts.len() >= 2 {
        let base_name = parts[0];
        let base_type = find_identifier_type_in_scope(root, base_name, source)?;
        let method_name = parts.last().unwrap_or(&"");
        find_extern_method_signature(root, &base_type, method_name, source)
    } else {
        let name = parts.first().unwrap_or(&"");
        find_action_signature(root, name, source)
            .or_else(|| find_extern_method_signature(root, name, name, source))
    }
}

/// 计算当前光标所在参数索引
pub fn compute_active_parameter(call: &Node, pos: Position, source: &str) -> u32 {
    let mut count = 0;
    let cursor_byte = pos.character as usize; // 简化：假设单行
    
    for i in 0..call.child_count() {
        if let Some(c) = call.child(i) {
            if c.kind() == "expr" || c.kind() == "identifier" || c.kind() == "number" {
                // 如果参数在光标之前，计数+1
                if c.end_byte() <= cursor_byte {
                    count += 1;
                }
            }
            if c.kind() == "," {
                count += 1;
            }
        }
    }
    
    count
}

// ---------------------------------------------------------------------------
// 单元测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser;
    use tower_lsp::lsp_types::Position;

    fn parse_source(source: &str) -> Tree {
        let mut parser = parser::new_parser().unwrap();
        parser.parse(source, None).expect("parse should succeed")
    }

    fn hover_at(tree: &Tree, source: &str, line: u32, character: u32) -> Option<String> {
        let pos = Position { line, character };
        hover(tree, source, pos).map(|h| match h.contents {
            HoverContents::Markup(m) => m.value,
            _ => String::new(),
        })
    }

    // -----------------------------------------------------------------------
    // 1. Header definition hover
    // -----------------------------------------------------------------------
    #[test]
    fn test_hover_header_definition() {
        let source = "header ethernet_t {\n    bit<48> dstAddr;\n}";
        let tree = parse_source(source);
        let result = hover_at(&tree, source, 0, 10); // cursor on "ethernet_t"
        let result = result.expect("should return hover");
        assert!(result.contains("**header**"));
        assert!(result.contains("`ethernet_t`"));
        assert!(result.contains("**Fields:**"));
        assert!(result.contains("`bit<48> dstAddr`"));
    }

    // -----------------------------------------------------------------------
    // 2. Struct definition hover
    // -----------------------------------------------------------------------
    #[test]
    fn test_hover_struct_definition() {
        let source = "struct headers {\n    ethernet_t eth;\n}";
        let tree = parse_source(source);
        let result = hover_at(&tree, source, 0, 10); // cursor on "headers"
        let result = result.expect("should return hover");
        assert!(result.contains("**struct**"));
        assert!(result.contains("`headers`"));
        assert!(result.contains("**Fields:**"));
        assert!(result.contains("`ethernet_t eth`"));
    }

    // -----------------------------------------------------------------------
    // 3. Enum definition hover
    // -----------------------------------------------------------------------
    #[test]
    fn test_hover_enum_definition() {
        let source = "enum MyEnum {\n    A, B\n}";
        let tree = parse_source(source);
        let result = hover_at(&tree, source, 0, 6); // cursor on "MyEnum"
        let result = result.expect("should return hover");
        assert!(result.contains("**enum**"));
        assert!(result.contains("`MyEnum`"));
        assert!(result.contains("**Members:**"));
        assert!(result.contains("`A`"));
        assert!(result.contains("`B`"));
    }

    // -----------------------------------------------------------------------
    // 4. Parser definition hover
    // -----------------------------------------------------------------------
    #[test]
    fn test_hover_parser_definition() {
        let source = "parser MyParser(packet_in pkt) {\n    state start {}\n}";
        let tree = parse_source(source);
        let result = hover_at(&tree, source, 0, 8); // cursor on "MyParser"
        let result = result.expect("should return hover");
        assert!(result.contains("**parser**"));
        assert!(result.contains("`MyParser"));
        assert!(result.contains("packet_in pkt"));
        assert!(result.contains("**States:**"));
        assert!(result.contains("`start`"));
    }

    // -----------------------------------------------------------------------
    // 5. Control definition hover
    // -----------------------------------------------------------------------
    #[test]
    fn test_hover_control_definition() {
        let source = "control MyControl(inout headers_t h) {\n    apply {}\n}";
        let tree = parse_source(source);
        let result = hover_at(&tree, source, 0, 9); // cursor on "MyControl"
        let result = result.expect("should return hover");
        assert!(result.contains("**control**"));
        assert!(result.contains("`MyControl"));
        assert!(result.contains("inout headers_t h"));
    }

    // -----------------------------------------------------------------------
    // 6. Action definition hover
    // -----------------------------------------------------------------------
    #[test]
    fn test_hover_action_definition() {
        let source = "action my_action(in bit<8> val) {\n}";
        let tree = parse_source(source);
        let result = hover_at(&tree, source, 0, 8); // cursor on "my_action"
        let result = result.expect("should return hover");
        assert!(result.contains("**action**"));
        assert!(result.contains("`my_action"));
        assert!(result.contains("in bit<8> val"));
    }

    // -----------------------------------------------------------------------
    // 7. Method call hover (e.g. .isValid())
    // -----------------------------------------------------------------------
    #[test]
    fn test_hover_method_call() {
        let source = r#"header ethernet_t {
    bit<48> dstAddr;
}
struct headers {
    ethernet_t eth;
}
control MyC(inout headers h) {
    apply {
        if (h.eth.isValid()) {}
    }
}"#;
        let tree = parse_source(source);
        // cursor on "isValid" inside the apply block
        // line 8: "        if (h.eth.isValid()) {}"
        // "isValid" starts at column 18
        let result = hover_at(&tree, source, 8, 18);
        let result = result.expect("should return hover");
        assert!(result.contains("**Call**"));
        assert!(result.contains("isValid"));
    }

    // -----------------------------------------------------------------------
    // 9. Nested struct field hover — struct A contains struct B field
    // -----------------------------------------------------------------------
    #[test]
    fn test_hover_nested_struct_field() {
        let source = r#"struct inner_t {
    bit<8> val;
}
struct outer_t {
    inner_t f;
}"#;
        let tree = parse_source(source);
        // cursor on field name 'f' inside outer_t (line 3, col 12)
        let result = hover_at(&tree, source, 3, 12);
        let result = result.expect("should return hover");
        assert!(result.contains("inner_t"), "should contain type B name: {}", result);
        assert!(result.contains("f"), "should contain field name: {}", result);
    }

    // -----------------------------------------------------------------------
    // 10. Nested header_union hover — union name + inner header field name
    // -----------------------------------------------------------------------
    #[test]
    fn test_hover_nested_header_union() {
        let source = r#"header ethernet_t {
    bit<48> dst;
}
header ipv4_t {
    bit<32> src;
}
header_union my_union {
    ethernet_t eth;
    ipv4_t ip;
}"#;
        let tree = parse_source(source);
        // cursor on union name 'my_union' (line 6, col 14)
        let result = hover_at(&tree, source, 6, 14);
        let result = result.expect("should return hover for union name");
        assert!(result.contains("header_union"), "should contain header_union: {}", result);
        assert!(result.contains("my_union"), "should contain union name: {}", result);

        // cursor on inner header field name 'eth' (line 7, col 17)
        let result = hover_at(&tree, source, 7, 17);
        let result = result.expect("should return hover for inner header field");
        assert!(result.contains("ethernet_t"), "should contain ethernet_t: {}", result);
        assert!(result.contains("eth"), "should contain eth: {}", result);
    }

    // -----------------------------------------------------------------------
    // 11. Multi-layer control nesting — control -> table -> action hover
    // -----------------------------------------------------------------------
    #[test]
    fn test_hover_nested_control_table() {
        let source = r#"control MyCtl(inout bit<8> x) {
    action do_drop() {}
    table my_tbl {
        key = { x : exact; }
        actions = { do_drop; }
    }
    apply { my_tbl.apply(); }
}"#;
        let tree = parse_source(source);
        // cursor on table name 'my_tbl' (line 3, col 10 -> 0-based line 2)
        let result = hover_at(&tree, source, 2, 10);
        let result = result.expect("should return hover for table name");
        assert!(result.contains("table"), "should contain table: {}", result);
        assert!(result.contains("my_tbl"), "should contain table name: {}", result);
        assert!(result.contains("key"), "should contain key block: {}", result);
        assert!(result.contains("actions"), "should contain actions block: {}", result);
    }

    // -----------------------------------------------------------------------
    // 12. Nested parser state hover — multiple states with transitions
    // -----------------------------------------------------------------------
    #[test]
    fn test_hover_nested_parser_states() {
        let source = r#"parser MyP(packet_in pkt) {
    state start {
        transition accept;
    }
    state parse_hdr {
        transition start;
    }
}"#;
        let tree = parse_source(source);
        // cursor on state definition name 'start' (line 1, col 10)
        let result = hover_at(&tree, source, 1, 10);
        let result = result.expect("should return hover for state start");
        assert!(result.contains("state"), "should contain state: {}", result);
        assert!(result.contains("start"), "should contain state name: {}", result);

        // cursor on state definition name 'parse_hdr' (line 4, col 10)
        let result = hover_at(&tree, source, 4, 10);
        let result = result.expect("should return hover for state parse_hdr");
        assert!(result.contains("state"), "should contain state: {}", result);
        assert!(result.contains("parse_hdr"), "should contain state name: {}", result);
    }

    // -----------------------------------------------------------------------
    // 13. Extern method hover — hover on method name inside extern definition
    // -----------------------------------------------------------------------
    #[test]
    fn test_hover_extern_method() {
        let source = r#"extern MyExtern {
    void method1(in bit<8> a);
    bit<32> method2(inout bit<16> b);
}"#;
        let tree = parse_source(source);
        // cursor on method1 name (line 1, col 9)
        let result = hover_at(&tree, source, 1, 9);
        let result = result.expect("should return hover for method1");
        assert!(result.contains("method1"), "should contain method1: {}", result);
        assert!(result.contains("void"), "should contain return type void: {}", result);
        assert!(result.contains("bit<8> a"), "should contain params: {}", result);

        // cursor on method2 name (line 2, col 12)
        let result = hover_at(&tree, source, 2, 12);
        let result = result.expect("should return hover for method2");
        assert!(result.contains("method2"), "should contain method2: {}", result);
        assert!(result.contains("bit<32>"), "should contain return type bit<32>: {}", result);
        assert!(result.contains("bit<16> b"), "should contain params: {}", result);
    }
}
