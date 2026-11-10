(program
  (lexical_declaration
    (variable_declarator name: (identifier) @name) @definition.constant))

(program
  (variable_declaration
    (variable_declarator name: (identifier) @name) @definition.constant))

(program
  (export_statement
    declaration: (lexical_declaration
      (variable_declarator name: (identifier) @name) @definition.constant)))

(program
  (export_statement
    declaration: (variable_declaration
      (variable_declarator name: (identifier) @name) @definition.constant)))

