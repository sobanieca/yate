;; HCL and Terraform highlights, written for yate (tree-sitter-hcl ships none).

(comment) @comment

(block . (identifier) @keyword)
(block (string_lit) @type)
(attribute . (identifier) @property)
(object_elem . (expression (variable_expr . (identifier) @property)))
(function_call . (identifier) @function)
(get_attr (identifier) @property)
(variable_expr (identifier) @variable)

(string_lit) @string
(quoted_template) @string
(template_literal) @string
(heredoc_template) @string
(heredoc_identifier) @punctuation.special
(heredoc_start) @punctuation.special
(template_interpolation_start) @punctuation.special
(template_interpolation_end) @punctuation.special

(numeric_lit) @number
(bool_lit) @boolean
(null_lit) @constant.builtin

["for" "in" "if" "else" "endif" "endfor"] @keyword

["=" "==" "!=" "<" "<=" ">" ">=" "&&" "||" "!" "+" "-" "*" "/" "%" "?" ":" "=>"] @operator
["(" ")" "[" "]" "{" "}"] @punctuation.bracket
["," "."] @punctuation.delimiter
