fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 { println!("usage: inspect <file>"); return; }
    let path = &args[1];
    let s = std::fs::read_to_string(path).expect("read file");
    println!("--- FILE: {} ---", path);
    for (i, line) in s.lines().enumerate() {
        println!("{:04}: {}", i+1, line);
    }
    println!("--- LEXER TOKENS ---");
    let mut l = userd::lexer::Lexer::new(&s);
    loop {
        let t = l.next_token();
        println!("{:?}", t);
        if t.is_eof() { break; }
    }
    println!("--- PARSED PROGRAM ---");
    let mut p = userd::parser::Parser::new(&s);
    let prog = p.parse_program();
    println!("{:?}", prog);
}
