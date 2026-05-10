module.exports = grammar({
  name: "p4",

  extras: ($) => [/\s|\\\r?\n/, $.comment, $.preproc],

  precedences: ($) => [
    [$.expr, $.method_identifier],
    [$.bit_type, $._type],
    [$.varbit_type, $._type],
  ],

  conflicts: ($) => [
    [$.lval, $.fval, $.method_identifier],
    [$.lval, $.method_identifier],
    [$.stmt],
  ],

  rules: {
    source_file: ($) => repeat($._definition),

    // top: ($) => $._definition,  // removed, inlined into source_file

    _definition: ($) =>
      choice(
        $.function_declaration,
        $.header_definition,
        $.header_union_definition,
        $.struct_definition,
        $.typedef_definition,
        $.type_definition,
        $.const_definition,
        $.extern_definition,
        $.parser_definition,
        $.control_definition,
        $.package,
        $.annotated_action,
        $.annotated_table,
        $.enum_definition,
        $.error_definition,
        $.match_kind_definition,
        $.value_set_declaration,
      ),

    package: ($) =>
      choice(
        seq(
          repeat($.annotation),
          "package",
          choice(
            $.method_identifier,
            seq($.method_identifier, "<", $.type_identifier, ">"),
          ),
          "(",
          repeat($.method_field),
          ")",
          ";",
        ),
        seq(
          $.method_identifier,
          "(",
          choice(seq($.method_identifier, "(", ")"), $.identifier),
          repeat(seq(",", $.method_identifier, "(", ")")),
          optional(","),
          ")",
          $.identifier,
          ";",
        ),
      ),

    function_declaration: ($) =>
      seq(
        choice($._type, $.type_identifier),
        $.method_identifier,
        "(",
        repeat($.parameter),
        ")",
        "{",
        repeat($.stmt),
        "}",
      ),

    header_definition: ($) =>
      choice(
        seq("header", $.type_identifier, $.identifier_preproc),
        seq(
          repeat($.annotation),
          "header",
          $.type_identifier,
          "{",
          repeat($.field),
          "}",
        ),
      ),

    header_union_definition: ($) =>
      seq(
        repeat($.annotation),
        "header_union",
        $.type_identifier,
        "{",
        repeat($.field),
        "}",
      ),

    struct_definition: ($) =>
      seq(
        repeat($.annotation),
        "struct",
        $.type_identifier,
        "{",
        repeat($.field),
        "}",
      ),

    typedef_definition: ($) =>
      seq(
        repeat($.annotation),
        "typedef",
        choice($._type, $.type_identifier),
        $.type_identifier,
        ";",
      ),

    type_definition: ($) =>
      seq(
        repeat($.annotation),
        "type",
        choice($._type, $.type_identifier),
        $.type_identifier,
        ";",
      ),

    enum_definition: ($) =>
      seq(
        repeat($.annotation),
        "enum",
        optional($._type),
        $.type_identifier,
        "{",
        $.identifier,
        repeat(seq(",", $.identifier)),
        optional(","),
        "}",
      ),

    error_definition: ($) =>
      seq(
        "error",
        "{",
        $.identifier,
        repeat(seq(",", $.identifier)),
        optional(","),
        "}",
      ),

    match_kind_definition: ($) =>
      seq(
        "match_kind",
        "{",
        $.identifier,
        repeat(seq(",", $.identifier)),
        optional(","),
        "}",
      ),

    const_definition: ($) =>
      seq(
        repeat($.annotation),
        "const",
        choice($._type, $.type_identifier),
        $.identifier,
        "=",
        $.expr,
        ";",
      ),

    extern_definition: ($) =>
      seq(
        repeat($.annotation),
        "extern",
        $.type_identifier,
        "{",
        repeat($.method),
        "}",
      ),

    parser_definition: ($) =>
      choice(
        seq(
          repeat($.annotation),
          "parser",
          choice(
            seq($.method_identifier, "<", $.type_identifier, ">"),
            $.method_identifier,
          ),
          "(",
          repeat($.parameter),
          ")",
          ";",
        ),
        seq(
          repeat($.annotation),
          "parser",
          choice(
            seq($.method_identifier, "<", $.type_identifier, ">"),
            $.method_identifier,
          ),
          "(",
          repeat($.parameter),
          ")",
          "{",
          repeat(choice($.value_set_declaration, $.state)),
          "}",
        ),
      ),

    control_definition: ($) =>
      choice(
        seq(
          repeat($.annotation),
          "control",
          choice(
            seq($.method_identifier, "<", $.type_identifier, ">"),
            $.method_identifier,
          ),
          "(",
          repeat($.parameter),
          ")",
          ";",
        ),
        seq(
          repeat($.annotation),
          "control",
          choice(
            seq($.method_identifier, "<", $.type_identifier, ">"),
            $.method_identifier,
          ),
          "(",
          repeat($.parameter),
          ")",
          "{",
          optional($.control_body),
          "}",
        ),
      ),

    control_body: ($) =>
      choice(
        seq(
          repeat($.control_body_element),
          seq("apply", "{", repeat($.stmt), "}"),
          repeat($.control_body_element),
        ),
        repeat1($.control_body_element),
      ),

    control_body_element: ($) =>
      choice(
        seq(repeat($.annotation), $.control_var),
        $.annotated_table,
        $.annotated_action,
      ),

    annotated_action: ($) => seq(repeat($.annotation), $.action),
    annotated_table: ($) => seq(repeat($.annotation), $.table),

    table: ($) =>
      seq("table", $.type_identifier, "{", repeat($.table_element), "}"),

    // TODO: priority 关键字暂不开放
    table_element: ($) =>
      choice(
        seq(
          "key",
          "=",
          "{",
          repeat1(seq($.expr, ":", $.key_type, repeat($.annotation), ";")),
          "}",
        ),
        seq(
          "actions",
          "=",
          "{",
          repeat1(seq(repeat($.annotation), $.action_item, ";")),
          "}",
        ),
        seq(
          optional("const"),
          $.identifier,
          "=",
          "{",
          repeat1(seq($.expr, ":", repeat($.annotation), $.action_item, ";")),
          "}",
        ),
        seq(optional("const"), "default_action", "=", $.action_item, ";"),
        seq("const", $.identifier, "=", $.expr, ";"),
        seq("size", "=", $.expr, ";"),
        seq("meters", "=", $.identifier, ";"),
        seq("counters", "=", $.identifier, ";"),
      ),

    action_item: ($) => choice($.call, $.method_identifier),

    key_type: (_) => choice("range", "exact", "ternary", "lpm", "optional"),

    state: ($) =>
      choice(
        seq($.method_identifier, "()", $.type_identifier, ";"),
        seq("state", $.method_identifier, "{", repeat($.stmt), "}"),
      ),

    stmt: ($) =>
      choice(
        $.break_stmt,
        $.continue_stmt,
        $.compound_assignment,
        $.for_statement,
        $.conditional,
        $.block_statement,
        $.action,
        $.var_decl,
        $.type_decl,
        seq($.transition, optional(";")),
        seq($.call, ";"),
        $.return_stmt,
        seq("exit", ";"),
        $.verify_stmt,
        ";",
      ),

    return_stmt: ($) => choice(seq("return", ";"), seq("return", $.expr, ";")),

    block_statement: ($) => seq("{", repeat($.stmt), "}"),

    break_stmt: ($) => seq("break", ";"),
    continue_stmt: ($) => seq("continue", ";"),

    // TODO: for_statement 语法暂时不考虑
    for_statement: ($) =>
      seq(
        "for",
        "(",
        choice(
          seq(
            optional(choice($.for_init_decl, $.expr)),
            ";",
            optional($.expr),
            ";",
            optional($.expr),
            ")",
            $.stmt,
          ),
          seq(
            choice($._type, $.type_identifier),
            $.identifier,
            "in",
            $.expr,
            ")",
            $.stmt,
          ),
        ),
      ),

    // TODO: for_init_decl 暂时不考虑
    for_init_decl: ($) =>
      seq(
        choice($._type, $.type_identifier),
        $.identifier,
        "=",
        $.expr,
      ),

    compound_assignment: ($) =>
      seq(
        $.lval,
        choice("+=", "-=", "|=", "&=", "^=", "<<=", ">>="),
        $.expr,
        ";",
      ),

    verify_stmt: ($) => seq("verify", "(", $.expr, ",", $.expr, ")", ";"),

    switch_stmt: ($) =>
      seq(
        "switch",
        "(",
        $.expr,
        ")",
        "{",
        repeat($.switch_case),
        "}",
      ),

    switch_case: ($) =>
      choice(
        seq($.expr, ":", $.stmt),
        seq("default", ":", $.stmt),
      ),

    type_decl: ($) =>
      seq(
        optional("("),
        choice($._type, $.type_identifier),
        optional(")"),
        $.identifier,
        ";",
      ),

    var_choice: ($) =>
      seq(
        optional("("),
        optional(choice($._type, $.type_identifier)),
        optional(")"),
        optional("("),
        $.expr,
        optional(")"),
      ),

    var_decl: ($) =>
      seq(
        optional(choice($._type, $.type_identifier)),
        $.lval,
        "=",
        $.var_choice,
        ";",
      ),

    action: ($) =>
      seq(
        "action",
        $.method_identifier,
        "(",
        repeat($.parameter),
        ")",
        "{",
        repeat($.stmt),
        "}",
      ),

    transition: ($) =>
      choice(seq("transition", $.identifier), seq("transition", $.select_expr)),

    select_expr: ($) =>
      seq("select", "(", $.expr, ")", "{", repeat($.select_case), "}"),

    select_case: ($) =>
      choice(
        seq($.expr, ":", $.identifier, ";"),
        seq("default", ":", $.identifier, ";"),
        seq("_", ":", $.identifier, ";"),
      ),

    call: ($) =>
      seq(
        $.fval,
        "(",
        optional(seq($.expr, repeat(seq(",", $.expr)), optional(","))),
        ")",
      ),

    slice: ($) => seq($.lval, "[", $.number, ":", $.number, "]"),
    tuple: ($) => seq("{", $.expr, repeat(seq(",", $.expr)), "}"),

    expr: ($) =>
      choice(
        prec.left(2, seq($.expr, $.binop, $.expr)),
        prec(1, $.call),
        prec(1, $.slice),
        prec(1, $.tuple),
        prec(1, $.range),
        prec(1, $.identifier_preproc),
        prec(1, $.string_literal),
        $.number,
        $.bool,
        $.lval,
        prec(3, seq("(", $._type, ")", $.expr)),
        "this",
      ),

    range: ($) => seq($.number, "..", $.number),

    string_literal: (_) =>
      token(
        seq(
          '"',
          repeat(
            choice(
              /[^"\\]+/,
              seq("\\", /./),
            ),
          ),
          '"',
        ),
      ),

    binop: (_) =>
      choice(
        "==",
        "!=",
        ">=",
        "<=",
        ">",
        "<",
        "=",
        "+",
        "-",
        "*",
        "/",
        "%",
        "|",
        "||",
        "&",
        "&&",
        "&&&",
        "<<",
        ">>",
        "!",
        "++",
        "|+|",
        "|-|",
      ),

    conditional: ($) => seq($._if, optional($._elseif), optional($._else)),
    conditional_binop: (_) => choice("&&", "|", "!"),

    _if: ($) => seq("if", "(", repeat($.expr), ")", "{", repeat($.stmt), "}"),
    _elseif: ($) =>
      seq("else", "if", "(", repeat($.expr), ")", "{", repeat($.stmt), "}"),
    _else: ($) => seq("else", "{", repeat($.stmt), "}"),

    control_var: ($) =>
      choice(
        seq(
          $.type_identifier,
          optional(seq("<", $.type_argument_list, ">")),
          "(",
          repeat(seq($.expr, optional(","))),
          ")",
          $.identifier,
          ";",
        ),
        seq(choice($._type, $.type_identifier), $.identifier, ";"),
      ),

    lval: ($) => seq($.identifier, repeat(seq(".", $.identifier))),

    fval: ($) =>
      choice(
        seq(
          $.identifier,
          repeat(seq(".", $.identifier)),
          seq(".", $.method_identifier),
        ),
        $.method_identifier,
      ),

    method: ($) =>
      seq(
        optional("abstract"),
        choice($._type, $.type_identifier),
        $.method_identifier,
        optional(seq("<", $.type_identifier, ">")),
        "(",
        repeat($.parameter),
        ")",
        ";",
      ),

    parameter: ($) =>
      choice(
        seq($.direction, $.identifier, optional(",")),
        seq(
          $.direction,
          optional("("),
          choice($._type, $.type_identifier),
          optional(")"),
          $.identifier,
          optional(","),
        ),
        seq(
          optional("("),
          choice($._type, $.type_identifier),
          optional(")"),
          $.identifier,
          optional(","),
        ),
      ),

    direction: (_) => choice("in", "out", "inout", "optional"),

    field: ($) =>
      seq(
        repeat($.annotation),
        choice($._type, $.type_identifier),
        $.identifier,
        ";",
        optional($.line_continuation),
      ),

    method_field: ($) =>
      seq(
        choice(
          $.method_identifier,
          seq($.method_identifier, "<", $.type_identifier, ">"),
        ),
        $.identifier,
        ",",
      ),

    _type: ($) =>
      choice(
        "bool",
        "error",
        "int",
        "bit",
        "varbit",
        "packet_in",
        "packet_out",
        "string",
        "void",
        "match_kind",
        $.bit_type,
        $.varbit_type,
        $.tuple_type,
        $.list_type,
        $.array_type,
      ),

    list_type: ($) => seq("list", "<", choice($._type, $.type_identifier), ">"),
    array_type: ($) => seq(choice($._type, $.type_identifier), "[", $.expr, "]"),

    bit_type: ($) => seq("bit", "<", $.number, ">"),
    varbit_type: ($) => seq("varbit", "<", $.number, ">"),
    tuple_type: ($) => seq("tuple", "<", $.type_argument_list, ">"),

    type_argument_list: ($) =>
      seq(
        choice($._type, $.type_identifier),
        repeat(seq(",", choice($._type, $.type_identifier))),
      ),

    type_identifier: ($) => prec(1, $.identifier),

    method_identifier: ($) => $.identifier,

    selection_case: ($) =>
      choice(
        seq($.expr, ":", $.identifier, ";"),
        seq("default", ":", $.identifier, ";"),
        seq("_", ":", $.identifier, ";"),
      ),

    identifier: (_) => /[a-zA-Z_][a-zA-Z0-9_]*/,
    identifier_preproc: (_) => /[A-Z][A-Z0-9_]*/,

    bool: (_) => choice("true", "false"),

    number: ($) => choice($.decimal, $.hex, $.whex, $.wdecimal),

    decimal: (_) => /\d+/,

    hex: (_) => /0x(\d|[a-fA-F])+/,

    whex: (_) => /\d+w0x(\d|[a-fA-F])+/,

    wdecimal: (_) => /\d+w\d+/,

    annotation: ($) => seq("@", $.identifier, optional($.annotation_body)),

    annotation_body: ($) =>
      seq(
        "(",
        optional($.annotation_content),
        repeat(seq(",", $.annotation_content)),
        optional(","),
        ")",
      ),

    annotation_content: ($) => choice($.expr, /["][^"]*["]/, $.annotation),

    comment: (_) =>
      token(
        choice(
          seq("//", /(\\+(.|\r?\n)|[^\\\n])*/),
          seq("/*", /[^*]*\*+([^/*][^*]*\*+)*/, "/"),
        ),
      ),

    line_continuation: (_) => /\s*\\\s*/,
    method_not_constant: (_) => /[a-zA-Z_]*[a-z][a-zA-Z]*/,

    value_set_declaration: ($) =>
      seq(
        repeat($.annotation),
        "value_set",
        "<",
        choice($._type, $.type_identifier),
        ">",
        "(",
        $.expr,
        ")",
        $.identifier,
        ";",
      ),

    preproc: ($) =>
      choice(
        seq(
          "#define",
          choice(
            seq($.identifier_preproc, $.number),
            seq(
              $.identifier_preproc,
              "{",
              $.line_continuation,
              repeat($.field),
              "}",
            ),
            seq(
              $.method_not_constant,
              "(",
              repeat(seq($.identifier, optional(","))),
              ")",
              "(",
              $.expr,
              ")",
            ),
            seq(
              $.identifier_preproc,
              $.line_continuation,
              repeat(
                seq(
                  optional("("),
                  optional(choice($._type, $.type_identifier)),
                  optional(")"),
                  $.lval,
                  ",",
                  $.line_continuation,
                ),
              ),
              seq(
                optional("("),
                optional(choice($._type, $.type_identifier)),
                optional(")"),
                $.lval,
              ),
              /\s/,
            ),
          ),
        ),
        seq("#include", choice("<", '"'), /[^">]*/, choice(">", '"')),
        seq("#if", /.*/),
        seq("#else", /.*/),
        "#endif",
      ),
  },
});
