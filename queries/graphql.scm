;; GraphQL highlights, written for yate (tree-sitter-graphql ships none).

(comment) @comment
(description (string_value) @comment.documentation)
(string_value) @string
(int_value) @number
(float_value) @number
(boolean_value) @boolean
(null_value) @constant.builtin
(enum_value) @constant
(variable) @variable.parameter

(operation_definition (name) @function)
(fragment_name) @function
(directive (name) @attribute)
(directive_definition (name) @attribute)
(named_type (name) @type)
(object_type_definition (name) @type)
(interface_type_definition (name) @type)
(union_type_definition (name) @type)
(enum_type_definition (name) @type)
(input_object_type_definition (name) @type)
(scalar_type_definition (name) @type)
(field (alias (name) @property))
(field (name) @property)
(field_definition (name) @property)
(object_field (name) @property)
(input_value_definition (name) @variable.parameter)
(argument (name) @variable.parameter)

(operation_type) @keyword
["fragment" "on" "type" "interface" "union" "enum" "input" "scalar" "schema" "extend" "directive" "implements" "repeatable"] @keyword

["@" "$"] @punctuation.special
["(" ")" "[" "]" "{" "}"] @punctuation.bracket
["!" "=" "|" "&" "..."] @operator
":" @punctuation.delimiter
