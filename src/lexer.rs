use crate::token::Token;

pub struct Lexer {
    input: Vec<char>,
    pos: usize,
}

impl Lexer {
    pub fn new(input: &str) -> Self {
        Self { input: input.chars().collect(), pos: 0 }
    }

    fn peek(&self) -> Option<char> {
        self.input.get(self.pos).copied()
    }

    fn next_char(&mut self) -> Option<char> {
        let ch = self.peek();
        if ch.is_some() { self.pos += 1; }
        ch
    }

    fn skip_whitespace(&mut self) {
        while let Some(c) = self.peek() {
            if c.is_whitespace() { self.pos += 1; } else { break; }
        }
    }

    fn read_identifier(&mut self) -> String {
        let start = self.pos;
        while let Some(c) = self.peek() {
            if c.is_alphanumeric() || c == '_' { self.pos += 1; } else { break }
        }
        self.input[start..self.pos].iter().collect()
    }

    fn read_number(&mut self) -> String {
        let start = self.pos;
        let mut seen_dot = false;
        while let Some(c) = self.peek() {
            if c.is_ascii_digit() {
                self.pos += 1;
            } else if c == '.' && !seen_dot {
                seen_dot = true;
                self.pos += 1;
            } else { break; }
        }
        self.input[start..self.pos].iter().collect()
    }

    fn read_string(&mut self) -> String {
        // assume opening '"' already consumed
        let mut s = String::new();
        while let Some(c) = self.next_char() {
            if c == '"' { break; }
            s.push(c);
        }
        s
    }

    pub fn next_token(&mut self) -> Token {
        self.skip_whitespace();
        if let Some(ch) = self.next_char() {
            match ch {
                '+' => Token::Plus,
                '-' => Token::Minus,
                '*' => Token::Asterisk,
                '/' => Token::Slash,
                '(' => Token::LParen,
                ')' => Token::RParen,
                '{' => Token::LBrace,
                '}' => Token::RBrace,
                ',' => Token::Comma,
                ';' => Token::Semicolon,
                '=' => Token::Assign,
                '.' => Token::Dot,
                '"' => {
                    let s = self.read_string();
                    Token::Str(s)
                }
                c if c.is_ascii_digit() => {
                    // roll back one char
                    self.pos -= 1;
                    let num = self.read_number();
                    if num.contains('.') {
                        let val = num.parse::<f64>().unwrap_or(0.0);
                        Token::Float(val)
                    } else {
                        let val = num.parse::<i64>().unwrap_or(0);
                        Token::Int(val)
                    }
                }
                c if c.is_alphabetic() || c == '_' => {
                    self.pos -= 1;
                    let ident = self.read_identifier();
                    match ident.as_str() {
                        "rtd" => Token::Rtd,
                        "class" => Token::Class,
                        _ => Token::Ident(ident),
                    }
                }
                other => Token::Illegal(other),
            }
        } else {
            Token::Eof
        }
    }
}
