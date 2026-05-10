import re

with open('grammar.js', 'r') as f:
    content = f.read()

# 1. extras 去掉 $.preproc
content = content.replace(
    'extras: ($) => [/\\s|\\\\\\r?\\n/, $.comment, $.preproc]',
    'extras: ($) => [/\\s|\\\\\\r?\\n/, $.comment]'
)

# 2. source_file 改为 repeat($.top)
content = content.replace(
    'source_file: ($) => repeat($._definition)',
    'source_file: ($) => repeat($.top)'
)

# 3. 添加 top 规则（在 _definition 前）
content = content.replace(
    '    _definition: ($) =>',
    '    top: ($) => choice($._definition, $.preproc),\n\n    _definition: ($) =>'
)

# 4. _definition 中去掉 $.preproc
content = content.replace(
    '        $.preproc,\n      ),\n\n    function_declaration',
    '      ),\n\n    function_declaration'
)

# 5. transition 分号改为 optional
content = content.replace(
    '    transition: ($) =>\n      seq(\n        "transition",\n        choice(\n          $.method_identifier,\n          $.select_expr,\n        ),\n        ";",\n      ),',
    '    transition: ($) =>\n      seq(\n        "transition",\n        choice(\n          $.method_identifier,\n          $.select_expr,\n        ),\n        optional(";"),\n      ),'
)

# 6. stmt 中 call 优先级改为 2
content = content.replace(
    '        seq($.call, "."),',
    '        prec(2, seq($.call, ";")),'
)

# 7. expr 中 call 优先级改为 2
content = content.replace(
    '        prec(1, $.call),',
    '        prec(2, $.call),'
)

# 8. lval 支持 type_identifier
content = content.replace(
    '    lval: ($) => seq($.identifier, repeat(seq(".", $.identifier))),',
    '    lval: ($) =>\n      seq(\n        choice($.identifier, $.type_identifier),\n        repeat(seq(".", choice($.identifier, $.type_identifier))),\n      ),'
)

# 9. fval 支持 type_identifier，恢复 optional，添加 prec(2)
content = content.replace(
    '    fval: ($) =>\n      choice(\n        seq(\n          $.identifier,\n          repeat(seq(".", $.identifier)),\n          seq(".", $.method_identifier),\n        ),\n        $.method_identifier,\n      ),',
    '    fval: ($) =>\n      prec(2, choice(\n        seq(\n          choice($.identifier, $.type_identifier),\n          repeat(seq(".", choice($.identifier, $.type_identifier))),\n          optional(seq(".", $.method_identifier)),\n        ),\n        $.method_identifier,\n      )),'
)

# 10. enum_definition 底层类型改为 optional
content = content.replace(
    '        "enum",\n        choice($._type, $.type_identifier),\n        $.type_identifier,',
    '        "enum",\n        optional($._type),\n        $.type_identifier,'
)

# 11. var_choice 重构
content = content.replace(
    '    var_choice: ($) =>\n      seq(\n        optional("("),\n        optional(seq(choice($._type, $.type_identifier), ")")),\n        optional("("),\n        $.expr,\n        optional(")"),\n      ),',
    '    var_choice: ($) => $.expr,'
)

# 12. #define 无值形式
content = content.replace(
    '            seq($.identifier_preproc, $.number),',
    '            seq($.identifier_preproc, optional(/[^\\n]*/)),'
)

# 13. method_field 逗号改为 optional
content = content.replace(
    '        $.identifier,\n        ",",',
    '        $.identifier,\n        optional(","),'
)

# 14. type_identifier 去掉 prec(1)
content = content.replace(
    '    type_identifier: ($) => prec(1, $.identifier),',
    '    type_identifier: ($) => $.identifier,'
)

with open('grammar.js', 'w') as f:
    f.write(content)

print("Done")
