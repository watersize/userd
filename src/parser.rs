use crate::ast::{Expr, Stmt, BinOp, Program};
use crate::lexer::Lexer;
use crate::token::Token;

pub struct Parser {
    lexer: Lexer,
    cur: Token,
    peek: Token,
}

impl Parser {
    pub fn new(input: &str) -> Self {
        let mut l = Lexer::new(input);
        let cur = l.next_token();
        let peek = l.next_token();
        Self { lexer: l, cur, peek }
    }

    fn bump(&mut self) {
        self.cur = std::mem::replace(&mut self.peek, self.lexer.next_token());
    }

    pub fn parse_program(&mut self) -> Program {
        let mut prog = Vec::new();
        while !self.cur.is_eof() {
            if let Some(stmt) = self.parse_statement() {
                prog.push(stmt);
            } else {
                // skip unknown token
                self.bump();
            }
        }
        prog
    }

    fn parse_statement(&mut self) -> Option<Stmt> {
        match &self.cur {
            Token::Class => self.parse_class_decl(),
            Token::Rtd => self.parse_function_decl(),
            Token::Ident(_) => {
                // could be var-decl if pattern: Ident - Ident = ... ;
                if let Token::Minus = &self.peek {
                    // var decl pattern
                    return self.parse_var_decl();
                }
                // otherwise parse an expression first; this will handle calls and member access.
                let expr = self.parse_expression();
                // if after parsing we have an assignment token, and the parsed expr is a member access,
                // treat it as a member assignment statement: receiver.field = expr;
                if let Some(Expr::MemberAccess { receiver, field }) = &expr {
                    if let Token::Assign = &self.cur {
                        self.bump();
                        if let Some(value) = self.parse_expression() {
                            self.consume_semicolon();
                            return Some(Stmt::MemberAssign { receiver: *receiver.clone(), name: field.clone(), value });
                        }
                    }
                }
                self.consume_semicolon();
                expr.map(Stmt::ExprStmt)
            }
            Token::Semicolon => { self.bump(); None }
            Token::Eof => None,
            _ => {
                let expr = self.parse_expression();
                self.consume_semicolon();
                expr.map(Stmt::ExprStmt)
            }
        }
    }

    fn parse_var_decl(&mut self) -> Option<Stmt> {
        // cur: Ident(type), peek: Minus
        let type_name = if let Token::Ident(s) = &self.cur { s.clone() } else { return None };
        self.bump(); // to Minus
        self.bump(); // to var name
        let name = if let Token::Ident(s) = &self.cur { s.clone() } else { return None };
        self.bump(); // to next
        if let Token::Assign = &self.cur {
            self.bump();
            let expr = self.parse_expression()?;
            self.consume_semicolon();
            Some(Stmt::VarDecl { type_name, name, value: expr })
        } else {
            None
        }
    }

    fn parse_function_decl(&mut self) -> Option<Stmt> {
        // cur == Rtd
        self.bump(); // to name (should be Ident)
        let name = if let Token::Ident(s) = &self.cur { s.clone() } else { return None };
        self.bump(); // to LParen
        // parse params
        let mut params = Vec::new();
        if let Token::LParen = &self.cur {
            self.bump();
            while let Token::Ident(p) = &self.cur {
                params.push(p.clone());
                self.bump();
                if let Token::Comma = &self.cur { self.bump(); } else { break; }
            }
            if let Token::RParen = &self.cur { self.bump(); } else { return None }
        } else { return None }
        // expect block
        if let Token::LBrace = &self.cur { self.bump(); } else { return None }
        let mut body = Vec::new();
        while let Token::RBrace = &self.cur { break; }
        while !matches!(self.cur, Token::RBrace | Token::Eof) {
            if let Some(s) = self.parse_statement() { body.push(s); } else { self.bump(); }
        }
        if let Token::RBrace = &self.cur { self.bump(); }
        Some(Stmt::FunctionDecl { name, params, body })
    }

    fn parse_class_decl(&mut self) -> Option<Stmt> {
        // cur == Class
        self.bump(); // to name
        let name = if let Token::Ident(s) = &self.cur { s.clone() } else { return None };
        self.bump(); // to LBrace
        if let Token::LBrace = &self.cur { self.bump(); } else { return None }
        let mut body = Vec::new();
        while !matches!(self.cur, Token::RBrace | Token::Eof) {
            if let Some(s) = self.parse_statement() { body.push(s); } else { self.bump(); }
        }
        if let Token::RBrace = &self.cur { self.bump(); }
        Some(Stmt::ClassDecl { name, body })
    }

    fn parse_member_assign(&mut self) -> Option<Stmt> {
        // pattern: receiver . name = expr ;
        // cur is Ident(receiver)
        let receiver = if let Token::Ident(s) = &self.cur { Expr::Ident(s.clone()) } else { return None };
        self.bump(); // to Dot
        self.bump(); // to name
        let name = if let Token::Ident(s) = &self.cur { s.clone() } else { return None };
        self.bump(); // to next
        if let Token::Assign = &self.cur {
            self.bump();
            let value = self.parse_expression()?;
            self.consume_semicolon();
            return Some(Stmt::MemberAssign { receiver, name, value });
        }
        None
    }

    fn parse_expression(&mut self) -> Option<Expr> {
        // parse primary then simple binary with + and -
        let mut left = self.parse_primary()?;
        while matches!(self.cur, Token::Plus | Token::Minus | Token::Asterisk | Token::Slash) {
            let op = match &self.cur {
                Token::Plus => BinOp::Add,
                Token::Minus => BinOp::Sub,
                Token::Asterisk => BinOp::Mul,
                Token::Slash => BinOp::Div,
                _ => unreachable!(),
            };
            self.bump();
            let right = self.parse_primary()?;
            left = Expr::BinaryOp { left: Box::new(left), op, right: Box::new(right) };
        }
        Some(left)
    }

    fn parse_primary(&mut self) -> Option<Expr> {
        match &self.cur {
            Token::Int(n) => { let v = *n; self.bump(); Some(Expr::Int(v)) }
            Token::Float(f) => { let v = *f; self.bump(); Some(Expr::Float(v)) }
            Token::Str(s) => { let s2 = s.clone(); self.bump(); Some(Expr::Str(s2)) }
            Token::Ident(name) => {
                let id = name.clone();
                self.bump();
                // member access/call: receiver.method(...)
                if let Token::Dot = &self.cur {
                    self.bump(); // to method name
                    let method = if let Token::Ident(m) = &self.cur { m.clone() } else { return None };
                    self.bump();
                    if let Token::LParen = &self.cur {
                        self.bump();
                        let mut args = Vec::new();
                        while !matches!(self.cur, Token::RParen | Token::Eof) {
                            if let Some(e) = self.parse_expression() { args.push(e); }
                            if let Token::Comma = &self.cur { self.bump(); }
                        }
                        if let Token::RParen = &self.cur { self.bump(); }
                        Some(Expr::MemberCall { receiver: Box::new(Expr::Ident(id)), method, args })
                    } else {
                        Some(Expr::MemberAccess { receiver: Box::new(Expr::Ident(id)), field: method })
                    }
                } else if let Token::LParen = &self.cur {
                    // call
                    self.bump();
                    let mut args = Vec::new();
                    while !matches!(self.cur, Token::RParen | Token::Eof) {
                        if let Some(e) = self.parse_expression() { args.push(e); }
                        if let Token::Comma = &self.cur { self.bump(); }
                    }
                    if let Token::RParen = &self.cur { self.bump(); }
                    Some(Expr::Call { func: Box::new(Expr::Ident(id)), args })
                } else { Some(Expr::Ident(id)) }
            }
            Token::LParen => {
                self.bump();
                let e = self.parse_expression();
                if let Token::RParen = &self.cur { self.bump(); }
                e
            }
            _ => None,
        }
    }

    fn consume_semicolon(&mut self) {
        if let Token::Semicolon = &self.cur { self.bump(); }
    }
}
