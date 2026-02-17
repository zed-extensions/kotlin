; Single test function (@Test, @ParameterizedTest, @RepeatedTest)
(
    (package_header (identifier) @kotlin_package_name)
    (class_declaration
        (type_identifier) @kotlin_class_name
        (class_body
            (function_declaration
                (modifiers
                    (annotation
                        [(user_type (type_identifier) @_annotation_name)
                         (constructor_invocation
                             (user_type (type_identifier) @_annotation_name))]))
                (simple_identifier) @run @kotlin_method_name
                (#match? @_annotation_name "^(Test|ParameterizedTest|RepeatedTest)$"))))
    @_
    (#set! tag kotlin-test)
)

; All tests in a class
(
    (package_header (identifier) @kotlin_package_name)
    (class_declaration
        (type_identifier) @run @kotlin_class_name
        (class_body
            (function_declaration
                (modifiers
                    (annotation
                        [(user_type (type_identifier) @_annotation_name)
                         (constructor_invocation
                             (user_type (type_identifier) @_annotation_name))]))
                (#match? @_annotation_name "^(Test|ParameterizedTest|RepeatedTest)$"))))
    @_
    (#set! tag kotlin-test-class)
)

; Single test function inside a @Nested inner class
(
    (package_header (identifier) @kotlin_package_name)
    (class_declaration
        (type_identifier) @kotlin_outer_class_name
        (class_body
            (class_declaration
                (modifiers
                    (annotation
                        (user_type (type_identifier) @_nested_annotation_name)))
                (type_identifier) @kotlin_class_name
                (class_body
                    (function_declaration
                        (modifiers
                            (annotation
                                [(user_type (type_identifier) @_annotation_name)
                                 (constructor_invocation
                                     (user_type (type_identifier) @_annotation_name))]))
                        (simple_identifier) @run @kotlin_method_name
                        (#match? @_annotation_name "^(Test|ParameterizedTest|RepeatedTest)$")))
                (#eq? @_nested_annotation_name "Nested"))
            @_))
    (#set! tag kotlin-test-nested)
)

; All tests in a @Nested inner class
(
    (package_header (identifier) @kotlin_package_name)
    (class_declaration
        (type_identifier) @kotlin_outer_class_name
        (class_body
            (class_declaration
                (modifiers
                    (annotation
                        (user_type (type_identifier) @_nested_annotation_name)))
                (type_identifier) @run @kotlin_class_name
                (class_body
                    (function_declaration
                        (modifiers
                            (annotation
                                [(user_type (type_identifier) @_annotation_name)
                                 (constructor_invocation
                                     (user_type (type_identifier) @_annotation_name))]))
                        (#match? @_annotation_name "^(Test|ParameterizedTest|RepeatedTest)$")))
                (#eq? @_nested_annotation_name "Nested"))
            @_))
    (#set! tag kotlin-test-class-nested)
)

; All tests in an outer class containing @Nested inner classes
(
    (package_header (identifier) @kotlin_package_name)
    (class_declaration
        (type_identifier) @run @kotlin_class_name
        (class_body
            (class_declaration
                (modifiers
                    (annotation
                        (user_type (type_identifier) @_nested_annotation_name)))
                (class_body
                    (function_declaration
                        (modifiers
                            (annotation
                                [(user_type (type_identifier) @_annotation_name)
                                 (constructor_invocation
                                     (user_type (type_identifier) @_annotation_name))]))
                        (#match? @_annotation_name "^(Test|ParameterizedTest|RepeatedTest)$")))
                (#eq? @_nested_annotation_name "Nested"))))
    @_
    (#set! tag kotlin-test-class)
)

; Main function
(
    (function_declaration
        (simple_identifier) @run
        (#eq? @run "main"))
    @_
    (#set! tag kotlin-main)
)
