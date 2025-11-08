//! userd — каркас для собственного языка программирования
//!
//! Этот crate содержит базовые модули: токены, AST, лексер, парсер, VM, REPL и CLI.
#![allow(dead_code)]

pub mod token;
pub mod ast;
pub mod lexer;
pub mod parser;
pub mod vm;
pub mod repl;
pub mod cli;
pub mod web_server;
pub mod gui;
pub mod platform;
pub mod rand;

#[cfg(test)]
mod tests {
    use crate::lexer::Lexer;
    use crate::parser::Parser;
    use crate::vm::VM;

    #[test]
    fn lexer_eof() {
        let mut l = Lexer::new("");
        assert_eq!(l.next_token().is_eof(), true);
    }

    #[test]
    fn var_decl_and_eval() {
        let src = "int-x = 5;";
        let mut p = Parser::new(src);
        let prog = p.parse_program();
        let mut vm = VM::new();
        let res = vm.execute_program(prog).unwrap();
        assert!(res.is_none());
        // after var decl, x should be present
        // can't access VM internals here easily, but ensure no crash
    }

    #[test]
    fn class_methods_and_self() {
        use crate::vm::Value;
        let src = r#"
        class Point {
          rtd __init__(self,x,y) { self.x = x; self.y = y; }
          rtd move(self,dx,dy) { self.x = self.x + dx; self.y = self.y + dy; }
        }
        Point-p = Point(1,2);
        p.move(3,4);
        "#;
        let mut pr = Parser::new(src);
        let prog = pr.parse_program();
        let mut vm = VM::new();
        vm.execute_program(prog).unwrap();
        let val = vm.get_global("p").expect("p missing");
        if let Value::Object(o) = val {
            let b = o.borrow();
            match b.fields.get("x") {
                Some(Value::Int(n)) => assert_eq!(*n, 4),
                _ => panic!("x missing or wrong type"),
            }
            match b.fields.get("y") {
                Some(Value::Int(n)) => assert_eq!(*n, 6),
                _ => panic!("y missing or wrong type"),
            }
        } else { panic!("p is not object") }
    }

    #[test]
    fn calculator_simple() {
        use crate::vm::Value;
        let src = r#"
        int-a = 1 + 2;
        int-b = (a + 3) * 2;
        b;
        "#;
        let mut p = Parser::new(src);
        let prog = p.parse_program();
        let mut vm = VM::new();
        let res = vm.execute_program(prog).unwrap();
        match res {
            Some(Value::Int(n)) => assert_eq!(n, 12),
            other => panic!("unexpected result from calculator: {:?}", other),
        }
    }
}
