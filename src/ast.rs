#[derive(Debug, Clone)]
pub enum Expr {
    Int(i64),
    Float(f64),
    Str(String),
    Ident(String),
    BinaryOp {
        left: Box<Expr>,
        op: BinOp,
        right: Box<Expr>,
    },
    Call {
        func: Box<Expr>,
        args: Vec<Expr>,
    },
    /// receiver.method(args)
    MemberCall {
        receiver: Box<Expr>,
        method: String,
        args: Vec<Expr>,
    },
    /// receiver.field access
    MemberAccess {
        receiver: Box<Expr>,
        field: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BinOp { Add, Sub, Mul, Div }

#[derive(Debug, Clone)]
pub enum Stmt {
    VarDecl { type_name: String, name: String, value: Expr },
    ExprStmt(Expr),
    FunctionDecl { name: String, params: Vec<String>, body: Vec<Stmt> },
    ClassDecl { name: String, body: Vec<Stmt> },
    /// receiver.field = expr;
    MemberAssign { receiver: Expr, name: String, value: Expr },
    Block(Vec<Stmt>),
}

pub type Program = Vec<Stmt>;
