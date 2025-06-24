;; Class and function bodies
(class_body "{" @start "}" @end) @indent
(function_body "{" @start "}" @end) @indent

;; Lambda expressions
(lambda_literal "{" @start "}" @end) @indent

;; Control flow statements
(if_expression "{" @start "}" @end) @indent
(when_expression "{" @start "}" @end) @indent
(for_statement "{" @start "}" @end) @indent
(while_statement "{" @start "}" @end) @indent
(do_while_statement "{" @start "}" @end) @indent
(try_expression "{" @start "}" @end) @indent

;; General parentheses and brackets
(_ "(" @start ")" @end) @indent
(_ "[" @start "]" @end) @indent

;; Fallback for any other braced expressions
(_ "{" @start "}" @end) @indent
