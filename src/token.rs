#[derive(Debug, PartialEq, Clone)]
pub enum Token {
    Ident(String),
    Int(i64),
    Float(f64),
    Str(String),
    Plus,
    Minus,
    Asterisk,
    Slash,
    LParen,
    RParen,
    LBrace,
    RBrace,
    Comma,
    Assign,     // =
    Semicolon,  // ;
    Dot,
    Eof,
    Illegal(char),
    // Keywords
    Rtd,   // function keyword in your language
    Class, // class keyword
}

impl Token {
    pub fn is_eof(&self) -> bool {
        matches!(self, Token::Eof)
    }
}
