use std::io::{self, Write};
use crate::parser::Parser;
use crate::vm::VM;

/// REPL: собирает ввод до `;`, затем парсит и исполняет программу
pub fn start_repl() {
    println!("userd REPL — введите 'exit' для выхода");
    let mut buffer = String::new();
    let mut vm = VM::new();
    loop {
        print!("> ");
        let _ = io::stdout().flush();
        let mut line = String::new();
        if io::stdin().read_line(&mut line).is_err() { break; }
        let trimmed = line.trim_end();
        if trimmed == "exit" { break; }
        buffer.push_str(trimmed);
        // if there's a semicolon, attempt to parse-execute everything up to last semicolon
        if buffer.contains(';') {
            // naive: parse whole buffer
            let mut parser = Parser::new(&buffer);
            let prog = parser.parse_program();
            match vm.execute_program(prog) {
                Ok(_) => {},
                Err(e) => println!("Error: {}", e),
            }
            buffer.clear();
        }
    }
}
