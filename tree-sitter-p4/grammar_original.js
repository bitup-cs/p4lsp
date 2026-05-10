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
  ],

  rules: {
    source_file: ($) => repeat($._definition),

    _definition: ($) =>
      choice(
        $.function_declaration,
        $.header_definition,
        $.header_union_definition,
        $.struct_definition,
        $.typedef_definition,
        $.const_definition,
        $.extern_definition,
        $.parser_definition,
        $.control_definition,
        $.package,
        $.action,
        $.table,
        $.enum_definition,
        $.error_definition,
        $.match_kind_definition,
        $.preproc,
      ),

    function_declaration: ($) =>
      seq(
        repeat($.annotation),
        choice($._type, "void"),
        $.method_identifier,
        "(",
        repeat($.parameter),
        ")",
        optional(
          seq(
            "{",
            repeat($.stmt),
            optional(seq("return", optional($.expr), ";")),
            "}",
          ),
        ),
        optional(";"),
      ),

    annotation: ($) =>
      seq("@", $.identifier, optional(seq("(", repeat($.expr), ")"))),

    package: ($) =>
      seq(
        repeat($.annotation),
        "package",
        choice(
          $.method_identifier,
          seq($.method_identifier, "<", $.type_identifier, ">"),
        ),
        "(",
        repeat($.parameter),
        ")",
        optional(";"),
      ),

    extern_definition: ($) =>
      seq(
        repeat($.annotation),
        "extern",
        choice(
          seq($.type_identifier, "{", repeat($.method), "}"),
          seq(
            $.type_identifier,
            "(",
            repeat(seq($.type_identifier, optional(","))), 
            ")",
            "{",
            repeat($.method),
            "}",
          ),
          seq(
            $.type_identifier,
            "<",
            $.type_identifier,
            ">",
            "(",
            repeat(seq($.type_identifier, optional(","))), 
            ")",
            "{",
            repeat($.method),
            "}",
          ),
        ),
        optional(";"),
      ),

    header_definition: ($) =>
      seq(
        repeat($.annotation),
        "header",
        $.type_identifier,
        "{",
        repeat($.field),
        "}",
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

    enum_definition: ($) =>
      seq(
        repeat($.annotation),
        "enum",
        choice($._type, $.type_identifier),
        $.type_identifier,
        "{",
        $.identifier,
        repeat(seq(",", $.identifier)),
        optional(","),
        "}",
      ),

    error_definition: ($) =>
      seq(
        repeat($.annotation),
        "error",
        "{",
        $.identifier,
        repeat(seq(",", $.identifier)),
        optional(","),
        "}",
      ),

    match_kind_definition: ($) =>
      seq(
        repeat($.annotation),
        "match_kind",
        "{",
        $.identifier,
        repeat(seq(",", $.identifier)),
        optional(","),
        "}",
      ),

    field: ($) =>
      seq(
        repeat($.annotation),
        choice($._type, $.type_identifier),
        $.identifier,
        optional(";"),
      ),

    parser_definition: ($) =>
      seq(
        repeat($.annotation),
        "parser",
        $.type_identifier,
        "(",
        repeat($.parameter),
        ")",
        optional(seq($.type_identifier, "(", repeat($.parameter), ")")),
        "{",
        repeat($.parser_element),
        "}",
      ),

    parser_element: ($) =>
      choice(
        $.function_declaration,
        $.const_definition,
        $.var_decl,
        $.instantiation,
        $.state,
      ),

    state: ($) =>
      seq(
        "state",
        $.method_identifier,
        "{",
        repeat($.stmt),
        optional($.transition),
        "}",
      ),

    transition: ($) =>
      seq(
        "transition",
        choice(
          $.method_identifier,
          $.select_expr,
        ),
        ";",
      ),

    select_expr: ($) =>
      seq(
        "select",
        "(",
        $.expr,
        repeat(seq(",", $.expr)),
        ")",
        "{",
        repeat($.select_case),
        "}",
      ),

    select_case: ($) =>
      choice(
        seq($.expr, ":", $.method_identifier),
        seq("default", ":", $.method_identifier),
      ),

    control_definition: ($) =>
      seq(
        repeat($.annotation),
        "control",
        $.type_identifier,
        "(",
        repeat($.parameter),
        ")",
        optional(seq($.type_identifier, "(", repeat($.parameter), ")")),
        "{",
        repeat($.control_element),
        "}",
      ),

    control_element: ($) =>
      choice(
        $.function_declaration,
        $.const_definition,
        $.var_decl,
        $.instantiation,
        $.table,
        $.action,
      ),

    action: ($) =>
      seq(
        repeat($.annotation),
        "action",
        $.method_identifier,
        "(",
        repeat($.parameter),
        ")",
        "{",
        repeat($.stmt),
        "}",
      ),

    table: ($) =>
      seq(
        repeat($.annotation),
        "table",
        $.type_identifier,
        "{",
        repeat($.table_element),
        "}",
      ),

    table_element: ($) =>
      choice(
        seq("key", "=", "{", repeat($.table_key), "}"),
        seq("actions", "=", "{", repeat($.action_item), "}"),
        seq("default_action", "=", $.expr, ";"),
        seq("size", "=", $.expr, ";"),
        seq("const", "entries", "=", "{", repeat($.entry), "}"),
      ),

    table_key: ($) =>
      seq(
        $.expr,
        ":",
        choice($.method_identifier, $.type_identifier),
        optional(";"),
      ),

    action_item: ($) =>
      choice(
        seq($.method_identifier, optional(",")),
        seq($.call, optional(",")),
      ),

    entry: ($) =>
      seq(
        repeat($.annotation),
        choice(
          seq($.expr, ":", $.action_item, optional(";")),
          seq("default", ":", $.action_item, optional(";")),
        ),
      ),

    instantiation: ($) =>
      seq(
        repeat($.annotation),
        choice($._type, $.type_identifier),
        optional(seq("(", repeat($.expr), ")")),
        $.identifier,
        ";",
      ),

    control_var: ($) =>
      choice(
        seq(
          $.type_identifier,
          optional(seq("<", $.type_argument_list, ">")),
          "(",
          repeat(seq($.expr, optional(","))),
          ")",
        ),
        seq(
          $.type_identifier,
          optional(seq("<", $.type_argument_list, ">")),
          "{",
          repeat(seq($.expr, optional(","))),
          "}",
        ),
      ),

    stmt: ($) =>
      choice(
        $.conditional,
        $.action,
        $.var_decl,
        $.type_decl,
        seq($.transition, ";"),
        seq($.call, ";"),
        seq("return", ";"),
        seq("return", $.expr, ";"),
        seq("exit", ";"),
        $.switch_stmt,
      ),

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
        optional(seq(choice($._type, $.type_identifier), ")")),
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
        prec.left(2, seq(optional($.expr), $.binop, $.expr)),
        prec(1, $.call),
        prec(1, $.slice),
        prec(1, $.tuple),
        prec(1, $.range),
        prec(1, $.identifier_preproc),
        prec(1, $.string_literal),
        $.number,
        $.bool,
        $.lval,
        seq("(", $._type, ")", $.expr),
        "this",
      ),

    range: ($) => seq($.number, "..", $.number),

    string_literal: (_) =>
      token(
        seq(
          "\"",
          repeat(choice(/[^"\\]/, /\\./)),
          "\"",
        ),
      ),

    number: ($) =>
      choice(
        seq($.decimal, optional(choice("w", "s"))),
        seq($.hex, optional(choice("w", "s"))),
        seq($.binary, optional(choice("w", "s"))),
        "0",
      ),

    decimal: (_) => /[0-9]+/,
    hex: (_) => /0[xX][0-9a-fA-F]+/,
    binary: (_) => /0[bB][01]+/,
    bool: (_) => choice("true", "false"),

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
          optional($.direction),
          optional("("),
          choice($._type, $.type_identifier),
          optional(")"),
          $.identifier,
          optional(","),
        ),
      ),

    direction: (_) => choice("in", "out", "inout"),

    method_identifier: ($) => $.identifier,
    type_identifier: ($) => prec(1, $.identifier),
    identifier: (_) => /[a-zA-Z_][a-zA-Z0-9_]*/,

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

    comment: (_) => token(choice(/\/\/[^\\n]*/, /\/\*[\s\S]*?\*\//)),

    _type: ($) =>
      choice(
        "bool",
        $.bit_type,
        $.varbit_type,
        $.tuple_type,
        $.type_identifier,
      ),

    bit_type: ($) => seq("bit", "<", $.number, ">"),
    varbit_type: ($) => seq("varbit", "<", $.number, ">"),
    tuple_type: ($) => seq("tuple", "<", $.type_argument_list, ">"),

    type_argument_list: ($) =>
      seq(
        choice($._type, $.type_identifier),
        repeat(seq(",", choice($._type, $.type_identifier))),
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

    identifier_preproc: (_) => /[a-zA-Z_][a-zA-Z0-9_]*/,
    method_not_constant: (_) => /[a-zA-Z_][a-zA-Z0-9_]*/,
    line_continuation: (_) => /\\\r?\n/,
  },
});
