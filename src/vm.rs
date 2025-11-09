use crate::ast::{Expr, Stmt, BinOp};
use std::collections::HashMap;
use std::rc::Rc;
use std::cell::RefCell;
use std::io::{self, Write};
use std::thread;
use std::time::Duration;
use std::sync::{Mutex, OnceLock, mpsc};
use std::sync::atomic::AtomicU64;

#[derive(Debug, Clone)]
pub enum Value {
    Int(i64),
    Float(f64),
    Str(String),
    Function(FunctionObject),
    Class(ClassObject),
    Object(Rc<RefCell<Object>>),
}

#[derive(Debug, Clone)]
pub struct FunctionObject {
    pub params: Vec<String>,
    pub body: Vec<Stmt>,
}

#[derive(Debug, Clone)]
pub struct ClassObject {
    pub name: String,
    pub methods: HashMap<String, FunctionObject>,
}

#[derive(Debug)]
pub struct Object {
    pub class_name: String,
    pub fields: HashMap<String, Value>,
    pub methods: HashMap<String, FunctionObject>,
}

pub struct VM {
    globals: HashMap<String, Value>,
    frames: Vec<HashMap<String, Value>>, // call stack locals
}

static CH_SENDERS: OnceLock<Mutex<HashMap<u64, mpsc::Sender<String>>>> = OnceLock::new();
static CH_RECEIVERS: OnceLock<Mutex<HashMap<u64, mpsc::Receiver<String>>>> = OnceLock::new();
static CH_NEXT_ID: AtomicU64 = AtomicU64::new(1);
static CH_BCAST: OnceLock<Mutex<HashMap<u64, Vec<(u64, mpsc::Sender<String>)>>>> = OnceLock::new();
static SUB_TO_CHANNEL: OnceLock<Mutex<HashMap<u64, u64>>> = OnceLock::new();

fn ch_senders() -> &'static Mutex<HashMap<u64, mpsc::Sender<String>>> {
    CH_SENDERS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn ch_receivers() -> &'static Mutex<HashMap<u64, mpsc::Receiver<String>>> {
    CH_RECEIVERS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn ch_bcast() -> &'static Mutex<HashMap<u64, Vec<(u64, mpsc::Sender<String>)>>> {
    CH_BCAST.get_or_init(|| Mutex::new(HashMap::new()))
}

fn sub_to_channel() -> &'static Mutex<HashMap<u64, u64>> {
    SUB_TO_CHANNEL.get_or_init(|| Mutex::new(HashMap::new()))
}

impl VM {
    pub fn new() -> Self { Self { globals: HashMap::new(), frames: Vec::new() } }

    fn push_frame(&mut self) { self.frames.push(HashMap::new()); }
    fn pop_frame(&mut self) { self.frames.pop(); }

    fn set_local(&mut self, name: String, val: Value) {
        if let Some(frame) = self.frames.last_mut() { frame.insert(name, val); }
        else { self.globals.insert(name, val); }
    }

    fn get_var(&self, name: &str) -> Option<Value> {
        for frame in self.frames.iter().rev() {
            if let Some(v) = frame.get(name) { return Some(v.clone()); }
        }
        self.globals.get(name).cloned()
    }

    pub fn execute_program(&mut self, prog: Vec<Stmt>) -> Result<Option<Value>, String> {
        let mut last = None;
        for s in prog {
            last = self.execute_stmt(s)?;
        }
        Ok(last)
    }

    /// Тестовый геттер: вернуть глобальное значение по имени
    pub fn get_global(&self, name: &str) -> Option<Value> {
        self.globals.get(name).cloned()
    }

    fn execute_stmt(&mut self, stmt: Stmt) -> Result<Option<Value>, String> {
        match stmt {
            Stmt::VarDecl { type_name: _t, name, value } => {
                let v = self.eval_expr(value)?;
                self.globals.insert(name, v);
                Ok(None)
            }
            Stmt::MemberAssign { receiver, name, value } => {
                let recv = self.eval_expr(receiver)?;
                let val = self.eval_expr(value)?;
                match recv {
                    Value::Object(o) => {
                        o.borrow_mut().fields.insert(name, val);
                        Ok(None)
                    }
                    _ => Err("member assignment on non-object".to_string()),
                }
            }
            Stmt::ExprStmt(e) => {
                let v = self.eval_expr(e)?;
                match &v {
                    Value::Int(n) => println!("{}", n),
                    Value::Float(f) => println!("{}", f),
                    Value::Str(s) => println!("{}", s),
                    Value::Function(_) => println!("<function>"),
                    Value::Class(_) => println!("<class>"),
                    Value::Object(_) => println!("<object>"),
                }
                Ok(Some(v))
            }
            Stmt::FunctionDecl { name, params, body } => {
                let fo = FunctionObject { params, body };
                self.globals.insert(name, Value::Function(fo));
                Ok(None)
            }
            Stmt::ClassDecl { name, body } => {
                let mut methods = HashMap::new();
                for s in body {
                    if let Stmt::FunctionDecl { name: mname, params, body: mb } = s {
                        methods.insert(mname, FunctionObject { params, body: mb });
                    }
                }
                let cls = ClassObject { name: name.clone(), methods };
                self.globals.insert(name, Value::Class(cls));
                Ok(None)
            }
            _ => Ok(None),
        }
    }

    fn eval_expr(&mut self, expr: Expr) -> Result<Value, String> {
        match expr {
            Expr::Int(n) => Ok(Value::Int(n)),
            Expr::Float(f) => Ok(Value::Float(f)),
            Expr::Str(s) => Ok(Value::Str(s)),
            Expr::Ident(name) => {
                if let Some(v) = self.get_var(&name) { Ok(v) }
                else {
                    // debug assistance: print available globals and frames to stderr
                    eprintln!("VM: undefined identifier '{}' — globals: {:?} — frames count: {}", name, self.globals.keys().collect::<Vec<_>>(), self.frames.len());
                    return Err(format!("undefined: {}", name));
                }
            }
            Expr::MemberAccess { receiver, field } => {
                let r = self.eval_expr(*receiver)?;
                if let Value::Object(o) = r {
                    if let Some(v) = o.borrow().fields.get(&field) { Ok(v.clone()) }
                    else { Err(format!("field {} not found", field)) }
                } else { Err("member access on non-object".to_string()) }
            }
            Expr::BinaryOp { left, op, right } => {
                let l = self.eval_expr(*left)?;
                let r = self.eval_expr(*right)?;
                match (l, r, op) {
                    (Value::Int(a), Value::Int(b), BinOp::Add) => Ok(Value::Int(a + b)),
                    (Value::Int(a), Value::Int(b), BinOp::Sub) => Ok(Value::Int(a - b)),
                    (Value::Int(a), Value::Int(b), BinOp::Mul) => Ok(Value::Int(a * b)),
                    (Value::Int(a), Value::Int(b), BinOp::Div) => Ok(Value::Int(a / b)),
                    // float cases
                    (Value::Float(a), Value::Float(b), BinOp::Add) => Ok(Value::Float(a + b)),
                    (Value::Float(a), Value::Float(b), BinOp::Sub) => Ok(Value::Float(a - b)),
                    (Value::Float(a), Value::Float(b), BinOp::Mul) => Ok(Value::Float(a * b)),
                    (Value::Float(a), Value::Float(b), BinOp::Div) => Ok(Value::Float(a / b)),
                    // mixed int/float
                    (Value::Int(a), Value::Float(b), BinOp::Add) => Ok(Value::Float((a as f64) + b)),
                    (Value::Float(a), Value::Int(b), BinOp::Add) => Ok(Value::Float(a + (b as f64))),
                    (Value::Int(a), Value::Float(b), BinOp::Sub) => Ok(Value::Float((a as f64) - b)),
                    (Value::Float(a), Value::Int(b), BinOp::Sub) => Ok(Value::Float(a - (b as f64))),
                    (Value::Int(a), Value::Float(b), BinOp::Mul) => Ok(Value::Float((a as f64) * b)),
                    (Value::Float(a), Value::Int(b), BinOp::Mul) => Ok(Value::Float(a * (b as f64))),
                    (Value::Int(a), Value::Float(b), BinOp::Div) => Ok(Value::Float((a as f64) / b)),
                    (Value::Float(a), Value::Int(b), BinOp::Div) => Ok(Value::Float(a / (b as f64))),
                    _ => Err("type error in binary op".to_string()),
                }
            }
            Expr::Call { func, args } => {
                // calling a function or a class constructor by identifier
                match *func {
                    Expr::Ident(fname) => {
                        // Builtins: get(prompt) -> String, to_int(x) -> Int, apply_op(a,b,op) -> Int
                        if fname == "get" {
                            if args.len() != 1 { return Err("get requires one argument".to_string()); }
                            let p = self.eval_expr(args[0].clone())?;
                            let prompt = match p {
                                Value::Str(s) => s,
                                Value::Int(n) => n.to_string(),
                                _ => return Err("get: prompt must be string or int".to_string()),
                            };
                            print!("{}", prompt);
                            let _ = io::stdout().flush();
                            let mut line = String::new();
                            io::stdin().read_line(&mut line).map_err(|e| e.to_string())?;
                            let s = line.trim().to_string();
                            return Ok(Value::Str(s));
                        }
                        if fname == "to_int" {
                            if args.len() != 1 { return Err("to_int requires one argument".to_string()); }
                            let v = self.eval_expr(args[0].clone())?;
                            match v {
                                Value::Int(n) => return Ok(Value::Int(n)),
                                Value::Str(s) => {
                                    let parsed = s.trim().parse::<i64>().map_err(|_| "to_int: parse error".to_string())?;
                                    return Ok(Value::Int(parsed));
                                }
                                _ => return Err("to_int: unsupported argument type".to_string()),
                            }
                        }
                        if fname == "to_float" {
                            if args.len() != 1 { return Err("to_float requires one argument".to_string()); }
                            let v = self.eval_expr(args[0].clone())?;
                            match v {
                                Value::Float(n) => return Ok(Value::Float(n)),
                                Value::Int(n) => return Ok(Value::Float(n as f64)),
                                Value::Str(s) => {
                                    let parsed = s.trim().parse::<f64>().map_err(|_| "to_float: parse error".to_string())?;
                                    return Ok(Value::Float(parsed));
                                }
                                _ => return Err("to_float: unsupported argument type".to_string()),
                            }
                        }
                        if fname == "apply_op" {
                            if args.len() != 3 { return Err("apply_op requires three arguments".to_string()); }
                            let a = self.eval_expr(args[0].clone())?;
                            let b = self.eval_expr(args[1].clone())?;
                            let opv = self.eval_expr(args[2].clone())?;
                            let ai = if let Value::Int(n) = a { n } else { return Err("apply_op: arg a must be int".to_string()) };
                            let bi = if let Value::Int(n) = b { n } else { return Err("apply_op: arg b must be int".to_string()) };
                            let oc = if let Value::Int(n) = opv { n } else { return Err("apply_op: op must be int".to_string()) };
                            let res = match oc {
                                1 => Value::Int(ai + bi),
                                2 => Value::Int(ai - bi),
                                3 => Value::Int(ai * bi),
                                4 => Value::Int(ai / bi),
                                _ => return Err("apply_op: unknown op code".to_string()),
                            };
                            return Ok(res);
                        }
                        // GUI builtins (stubs): gui_window(title, w, h) -> Object, gui_label(win, text), gui_show(win)
                        if fname == "gui_window" {
                            if args.len() != 3 { return Err("gui_window requires 3 arguments".to_string()); }
                            let t = self.eval_expr(args[0].clone())?;
                            let wv = self.eval_expr(args[1].clone())?;
                            let hv = self.eval_expr(args[2].clone())?;
                            let _title = match t { Value::Str(s) => s, Value::Int(n) => n.to_string(), _ => "window".to_string() };
                            let _w = if let Value::Int(n) = wv { n as u32 } else { 400 };
                            let _h = if let Value::Int(n) = hv { n as u32 } else { 300 };
                            // call platform-specific window creation when available
                            let title = _title;
                            let wid = {
                                #[cfg(target_os = "windows")]
                                {
                                    crate::platform::windows::create_window(&title, _w as i32, _h as i32) as i64
                                }
                                #[cfg(not(target_os = "windows"))]
                                { 0i64 }
                            };
                            return Ok(Value::Int(wid));
                        }
                        if fname == "gui_blit_b64" {
                            // gui_blit_b64(id, b64str, w, h)
                            if args.len() != 4 { return Err("gui_blit_b64 requires 4 arguments".to_string()); }
                            let idv = self.eval_expr(args[0].clone())?;
                            let b64v = self.eval_expr(args[1].clone())?;
                            let wv = self.eval_expr(args[2].clone())?;
                            let hv = self.eval_expr(args[3].clone())?;
                            let id = if let Value::Int(n) = idv { n as u64 } else { return Err("gui_blit_b64: id must be int".to_string()) };
                            let b64s = if let Value::Str(s) = b64v { s } else { return Err("gui_blit_b64: data must be string".to_string()) };
                            let w = if let Value::Int(n) = wv { n as i32 } else { return Err("gui_blit_b64: w must be int".to_string()) };
                            let h = if let Value::Int(n) = hv { n as i32 } else { return Err("gui_blit_b64: h must be int".to_string()) };
                            // decode base64 (simple implementation)
                            fn decode_b64(s: &str) -> Result<Vec<u8>, String> {
                                let mut out = Vec::new();
                                let mut bits: u32 = 0;
                                let mut count: u8 = 0;
                                for ch in s.chars() {
                                    let val = match ch {
                                        'A'..='Z' => (ch as u8 - b'A') as i32,
                                        'a'..='z' => (ch as u8 - b'a' + 26) as i32,
                                        '0'..='9' => (ch as u8 - b'0' + 52) as i32,
                                        '+' => 62,
                                        '/' => 63,
                                        '=' => { break; }
                                        _ => { continue; }
                                    } as u32;
                                    bits = (bits << 6) | val;
                                    count += 6;
                                    while count >= 8 {
                                        count -= 8;
                                        let b = ((bits >> count) & 0xFF) as u8;
                                        out.push(b);
                                    }
                                }
                                Ok(out)
                            }
                            let bytes = decode_b64(&b64s)?;
                            #[cfg(target_os = "windows")]
                            {
                                crate::platform::windows::blit_window(id, bytes, w, h).map_err(|e| e.to_string())?;
                                return Ok(Value::Int(1));
                            }
                            #[cfg(not(target_os = "windows"))]
                            {
                                return Ok(Value::Int(0));
                            }
                        }
                        if fname == "draw_rect" {
                            // draw_rect(id, canvas_w, canvas_h, x,y,w,h, r,g,b,a)
                            if args.len() != 10 { return Err("draw_rect requires 10 arguments".to_string()); }
                            let idv = self.eval_expr(args[0].clone())?;
                            let _canvas_w = if let Value::Int(n) = self.eval_expr(args[1].clone())? { n as i32 } else { return Err("draw_rect: canvas_w must be int".to_string()) };
                            let _canvas_h = if let Value::Int(n) = self.eval_expr(args[2].clone())? { n as i32 } else { return Err("draw_rect: canvas_h must be int".to_string()) };
                            let x = if let Value::Int(n) = self.eval_expr(args[3].clone())? { n as i32 } else { return Err("draw_rect: x must be int".to_string()) };
                            let y = if let Value::Int(n) = self.eval_expr(args[4].clone())? { n as i32 } else { return Err("draw_rect: y must be int".to_string()) };
                            let w = if let Value::Int(n) = self.eval_expr(args[5].clone())? { n as i32 } else { return Err("draw_rect: w must be int".to_string()) };
                            let h = if let Value::Int(n) = self.eval_expr(args[6].clone())? { n as i32 } else { return Err("draw_rect: h must be int".to_string()) };
                            let r = if let Value::Int(n) = self.eval_expr(args[7].clone())? { n as u8 } else { return Err("draw_rect: r must be int".to_string()) };
                            let g = if let Value::Int(n) = self.eval_expr(args[8].clone())? { n as u8 } else { return Err("draw_rect: g must be int".to_string()) };
                            let b = if let Value::Int(n) = self.eval_expr(args[9].clone())? { n as u8 } else { return Err("draw_rect: b must be int".to_string()) };
                            let a = 255u8;
                            let id = if let Value::Int(n) = idv { n as u64 } else { return Err("draw_rect: id must be int".to_string()) };
                            #[cfg(target_os = "windows")]
                            {
                                crate::platform::windows::canvas_draw_rect(id, x, y, w, h, r, g, b, a).map_err(|e| e.to_string())?;
                                return Ok(Value::Int(1));
                            }
                            #[cfg(not(target_os = "windows"))]
                            {
                                // Fallback: construct full buffer (slow) and try to blit if platform supports it; otherwise no-op
                                let canvas_w = _canvas_w as i32;
                                let canvas_h = _canvas_h as i32;
                                let wsz = (canvas_w as usize).saturating_mul(canvas_h as usize).saturating_mul(4);
                                if canvas_w <= 0 || canvas_h <= 0 { return Err("draw_rect: invalid canvas size".to_string()) }
                                let mut buf = vec![0u8; wsz];
                                for yy in 0..canvas_h {
                                    for xx in 0..canvas_w {
                                        let px = xx;
                                        let py = yy;
                                        if px >= x && px < x + w && py >= y && py < y + h {
                                            let idx = ((py as usize) * (canvas_w as usize) + (px as usize)) * 4;
                                            buf[idx+0] = r;
                                            buf[idx+1] = g;
                                            buf[idx+2] = b;
                                            buf[idx+3] = a;
                                        }
                                    }
                                }
                                return Ok(Value::Int(0));
                            }
                        }

                        if fname == "secure_random" {
                            if args.len() != 1 { return Err("secure_random requires 1 argument".to_string()); }
                            let maxv = self.eval_expr(args[0].clone())?;
                            let max = if let Value::Int(n) = maxv { if n <= 0 { return Err("secure_random: max must be >0".to_string()) } else { n as u64 } } else { return Err("secure_random: max must be int".to_string()) };
                            let r = crate::rand::secure_random_u64(max).map_err(|e| e.to_string())?;
                            return Ok(Value::Int(r as i64));
                        }
                        if fname == "canvas_clear" {
                            // canvas_clear(id, r,g,b,a)
                            if args.len() != 5 { return Err("canvas_clear requires 5 arguments".to_string()); }
                            let idv = self.eval_expr(args[0].clone())?;
                            let r = if let Value::Int(n) = self.eval_expr(args[1].clone())? { n as u8 } else { return Err("canvas_clear: r must be int".to_string()) };
                            let g = if let Value::Int(n) = self.eval_expr(args[2].clone())? { n as u8 } else { return Err("canvas_clear: g must be int".to_string()) };
                            let b = if let Value::Int(n) = self.eval_expr(args[3].clone())? { n as u8 } else { return Err("canvas_clear: b must be int".to_string()) };
                            let a = if let Value::Int(n) = self.eval_expr(args[4].clone())? { n as u8 } else { return Err("canvas_clear: a must be int".to_string()) };
                            let id = if let Value::Int(n) = idv { n as u64 } else { return Err("canvas_clear: id must be int".to_string()) };
                            #[cfg(target_os = "windows")] { crate::platform::windows::canvas_clear(id, r,g,b,a).map_err(|e| e.to_string())?; return Ok(Value::Int(1)); }
                            #[cfg(not(target_os = "windows"))] { return Ok(Value::Int(0)); }
                        }

                        if fname == "canvas_present" {
                            if args.len() != 1 { return Err("canvas_present requires 1 argument".to_string()); }
                            let idv = self.eval_expr(args[0].clone())?;
                            let id = if let Value::Int(n) = idv { n as u64 } else { return Err("canvas_present: id must be int".to_string()) };
                            #[cfg(target_os = "windows")] { crate::platform::windows::canvas_present(id).map_err(|e| e.to_string())?; return Ok(Value::Int(1)); }
                            #[cfg(not(target_os = "windows"))] { return Ok(Value::Int(0)); }
                        }

                        if fname == "canvas_draw_text" {
                            // canvas_draw_text(id, x, y, text)
                            if args.len() != 4 { return Err("canvas_draw_text requires 4 arguments".to_string()); }
                            let idv = self.eval_expr(args[0].clone())?;
                            let x = if let Value::Int(n) = self.eval_expr(args[1].clone())? { n as i32 } else { return Err("canvas_draw_text: x must be int".to_string()) };
                            let y = if let Value::Int(n) = self.eval_expr(args[2].clone())? { n as i32 } else { return Err("canvas_draw_text: y must be int".to_string()) };
                            let tv = self.eval_expr(args[3].clone())?;
                            let text = if let Value::Str(s) = tv { s } else { return Err("canvas_draw_text: text must be string".to_string()) };
                            let id = if let Value::Int(n) = idv { n as u64 } else { return Err("canvas_draw_text: id must be int".to_string()) };
                            #[cfg(target_os = "windows")] { crate::platform::windows::canvas_draw_text(id, x, y, &text).map_err(|e| e.to_string())?; return Ok(Value::Int(1)); }
                            #[cfg(not(target_os = "windows"))] { return Ok(Value::Int(0)); }
                        }
                        if fname == "register_widget" {
                            // register_widget(win_id, x, y, w, h, handler_name)
                            if args.len() != 6 { return Err("register_widget requires 6 arguments".to_string()); }
                            let idv = self.eval_expr(args[0].clone())?;
                            let x = if let Value::Int(n) = self.eval_expr(args[1].clone())? { n as i32 } else { return Err("register_widget: x must be int".to_string()) };
                            let y = if let Value::Int(n) = self.eval_expr(args[2].clone())? { n as i32 } else { return Err("register_widget: y must be int".to_string()) };
                            let w = if let Value::Int(n) = self.eval_expr(args[3].clone())? { n as i32 } else { return Err("register_widget: w must be int".to_string()) };
                            let h = if let Value::Int(n) = self.eval_expr(args[4].clone())? { n as i32 } else { return Err("register_widget: h must be int".to_string()) };
                            let hv = self.eval_expr(args[5].clone())?;
                            let handler = if let Value::Str(s) = hv { s } else { return Err("register_widget: handler must be string".to_string()) };
                            let id = if let Value::Int(n) = idv { n as u64 } else { return Err("register_widget: id must be int".to_string()) };
                            #[cfg(target_os = "windows")] { crate::platform::windows::register_widget(id, x, y, w, h, &handler); return Ok(Value::Int(1)); }
                            #[cfg(not(target_os = "windows"))] { return Ok(Value::Int(0)); }
                        }
                        if fname == "gui_button" {
                            // gui_button(win_id, label, handler_name)
                            if args.len() != 3 { return Err("gui_button requires 3 arguments".to_string()); }
                            let idv = self.eval_expr(args[0].clone())?;
                            let _labelv = self.eval_expr(args[1].clone())?;
                            let handlerv = self.eval_expr(args[2].clone())?;
                            let id = if let Value::Int(n) = idv { n as u64 } else { return Err("gui_button: id must be int".to_string()) };
                            let handler_name = if let Value::Str(s) = handlerv { s } else { return Err("gui_button: handler must be string".to_string()) };
                            #[cfg(target_os = "windows")] {
                                // register a widget using simple auto layout and handler name
                                crate::platform::windows::register_widget_auto(id, "button", &handler_name);
                            }
                            return Ok(Value::Int(1));
                        }
                        if fname == "gui_poll" {
                            // poll events and invoke registered handlers
                            #[cfg(target_os = "windows")] {
                                let evs = crate::platform::windows::drain_events();
                                for (win_id, (x,y)) in evs {
                                    if let Some(hname) = crate::platform::windows::get_handler(win_id) {
                                        if let Some(Value::Function(fobj)) = self.get_var(&hname) {
                                            // call handler with x,y
                                            self.push_frame();
                                            if fobj.params.len() >= 1 { self.set_local(fobj.params[0].clone(), Value::Int(x as i64)); }
                                            if fobj.params.len() >= 2 { self.set_local(fobj.params[1].clone(), Value::Int(y as i64)); }
                                            let _ = self.execute_program(fobj.body.clone())?;
                                            self.pop_frame();
                                        }
                                    }
                                }
                                return Ok(Value::Int(1));
                            }
                            #[cfg(not(target_os = "windows"))] { return Ok(Value::Int(0)); }
                        }
                        if fname == "gui_run" {
                            // run loop: keep polling events while windows exist
                            #[cfg(target_os = "windows")] {
                                while crate::platform::windows::has_windows() {
                                    let evs = crate::platform::windows::drain_events();
                                    for (win_id, (x,y)) in evs {
                                        if let Some(hname) = crate::platform::windows::get_handler(win_id) {
                                            if let Some(Value::Function(fobj)) = self.get_var(&hname) {
                                                self.push_frame();
                                                if fobj.params.len() >= 1 { self.set_local(fobj.params[0].clone(), Value::Int(x as i64)); }
                                                if fobj.params.len() >= 2 { self.set_local(fobj.params[1].clone(), Value::Int(y as i64)); }
                                                let _ = self.execute_program(fobj.body.clone())?;
                                                self.pop_frame();
                                            }
                                        }
                                    }
                                    // small sleep
                                    std::thread::sleep(std::time::Duration::from_millis(20));
                                }
                                return Ok(Value::Int(1));
                            }
                            #[cfg(not(target_os = "windows"))] { return Ok(Value::Int(0)); }
                        }
                        if fname == "gui_close" {
                            if args.len() != 1 { return Err("gui_close requires 1 argument".to_string()) }
                            let idv = self.eval_expr(args[0].clone())?;
                            let id = if let Value::Int(n) = idv { n as u64 } else { return Err("gui_close: id must be int".to_string()) };
                            #[cfg(target_os = "windows")] { crate::platform::windows::close_window(id); }
                            return Ok(Value::Int(1));
                        }
                        if fname == "gui_label" {
                            if args.len() != 2 { return Err("gui_label requires 2 arguments".to_string()); }
                            let objv = self.eval_expr(args[0].clone())?;
                            let txtv = self.eval_expr(args[1].clone())?;
                            let text = match txtv { Value::Str(s) => s, Value::Int(n) => n.to_string(), _ => "".to_string() };
                            if let Value::Object(o) = objv {
                                o.borrow_mut().fields.insert("label".to_string(), Value::Str(text));
                                return Ok(Value::Int(1));
                            }
                            return Err("gui_label: first arg must be a Window object".to_string());
                        }
                        if fname == "gui_show" {
                            if args.len() != 1 { return Err("gui_show requires 1 argument".to_string()); }
                            let objv = self.eval_expr(args[0].clone())?;
                            if let Value::Object(_o) = objv {
                                // no-op placeholder; real implementation will present the window
                                return Ok(Value::Int(1));
                            }
                            return Err("gui_show: arg must be a Window object".to_string());
                        }
                        if fname == "gui_message" {
                            if args.len() != 2 { return Err("gui_message requires 2 arguments".to_string()); }
                            let t = self.eval_expr(args[0].clone())?;
                            let m = self.eval_expr(args[1].clone())?;
                            let title = match t { Value::Str(s) => s, Value::Int(n) => n.to_string(), _ => "".to_string() };
                            let text = match m { Value::Str(s) => s, Value::Int(n) => n.to_string(), _ => "".to_string() };
                            #[cfg(target_os = "windows")]
                            {
                                crate::platform::windows::show_message(&title, &text);
                                return Ok(Value::Int(1));
                            }
                            #[cfg(not(target_os = "windows"))]
                            {
                                // fallback to printing on other platforms
                                println!("{}: {}", title, text);
                                return Ok(Value::Int(1));
                            }
                        }
                        if fname == "sleep_ms" {
                            // sleep_ms(ms)
                            if args.len() != 1 { return Err("sleep_ms requires 1 argument".to_string()); }
                            let v = self.eval_expr(args[0].clone())?;
                            let ms = if let Value::Int(n) = v { n } else { return Err("sleep_ms: arg must be int".to_string()) };
                            thread::sleep(Duration::from_millis(ms as u64));
                            return Ok(Value::Int(1));
                        }
                        if fname == "spawn" {
                            // spawn(function_name)
                            if args.len() != 1 { return Err("spawn requires 1 argument".to_string()); }
                            let nv = self.eval_expr(args[0].clone())?;
                            let fname = if let Value::Str(s) = nv { s } else { return Err("spawn: arg must be string".to_string()) };
                            // find function in current globals
                            if let Some(Value::Function(fobj)) = self.get_var(&fname) {
                                let fclone = fobj.clone();
                                // spawn thread and execute function body in fresh VM instance
                                thread::spawn(move || {
                                    let mut vm2 = VM::new();
                                    // run function body (no args / minimal environment)
                                    let _ = vm2.execute_program(fclone.body.clone());
                                });
                                return Ok(Value::Int(1));
                            } else {
                                return Err("spawn: function not found".to_string());
                            }
                        }
                        if fname == "channel_create" {
                            // channel_create() -> id (creates primary channel with one receiver)
                            let id = CH_NEXT_ID.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                            let (tx, rx) = mpsc::channel::<String>();
                            if let Ok(mut s) = ch_senders().lock() { s.insert(id, tx); }
                            if let Ok(mut r) = ch_receivers().lock() { r.insert(id, rx); }
                            return Ok(Value::Int(id as i64));
                        }
                        if fname == "channel_send" {
                            // channel_send(id, text) -> 1 on success
                            if args.len() != 2 { return Err("channel_send requires 2 arguments".to_string()); }
                            let idv = self.eval_expr(args[0].clone())?;
                            let tv = self.eval_expr(args[1].clone())?;
                            let id = if let Value::Int(n) = idv { n as u64 } else { return Err("channel_send: id must be int".to_string()) };
                            let s = if let Value::Str(st) = tv { st } else { return Err("channel_send: text must be string".to_string()) };
                            let mut sent = false;
                            if let Ok(map) = ch_senders().lock() {
                                if let Some(tx) = map.get(&id) {
                                    let _ = tx.send(s.clone());
                                    sent = true;
                                }
                            }
                            // send to broadcast subscribers if any
                            if let Ok(bmap) = ch_bcast().lock() {
                                if let Some(list) = bmap.get(&id) {
                                    for (_subid, tx) in list.iter() {
                                        let _ = tx.send(s.clone());
                                        sent = true;
                                    }
                                }
                            }
                            if sent { return Ok(Value::Int(1)); }
                            return Err("channel_send: channel not found".to_string());
                        }
                        if fname == "channel_try_recv" {
                            // channel_try_recv(id) -> object { ok:1, msg: "..." } or { ok:0 }
                            if args.len() != 1 { return Err("channel_try_recv requires 1 argument".to_string()); }
                            let idv = self.eval_expr(args[0].clone())?;
                            let id = if let Value::Int(n) = idv { n as u64 } else { return Err("channel_try_recv: id must be int".to_string()) };
                            if let Ok(mut map) = ch_receivers().lock() {
                                if let Some(rx) = map.get_mut(&id) {
                                    match rx.try_recv() {
                                        Ok(s) => {
                                            // build Result object { ok:1, msg: s }
                                            let mut fields = HashMap::new();
                                            fields.insert("ok".to_string(), Value::Int(1));
                                            fields.insert("msg".to_string(), Value::Str(s));
                                            let obj = Rc::new(RefCell::new(Object { class_name: "Result".to_string(), fields, methods: HashMap::new() }));
                                            return Ok(Value::Object(obj));
                                        }
                                        Err(mpsc::TryRecvError::Empty) => {
                                            let mut fields = HashMap::new();
                                            fields.insert("ok".to_string(), Value::Int(0));
                                            let obj = Rc::new(RefCell::new(Object { class_name: "Result".to_string(), fields, methods: HashMap::new() }));
                                            return Ok(Value::Object(obj));
                                        }
                                        Err(_) => return Err("channel_try_recv: receive error".to_string()),
                                    }
                                }
                            }
                            return Err("channel_try_recv: channel not found".to_string());
                        }
                        if fname == "channel_recv" {
                            // channel_recv(id) -> blocks until message (returns string)
                            if args.len() != 1 { return Err("channel_recv requires 1 argument".to_string()); }
                            let idv = self.eval_expr(args[0].clone())?;
                            let id = if let Value::Int(n) = idv { n as u64 } else { return Err("channel_recv: id must be int".to_string()) };
                            if let Ok(mut map) = ch_receivers().lock() {
                                if let Some(rx) = map.get_mut(&id) {
                                    match rx.recv() {
                                        Ok(s) => return Ok(Value::Str(s)),
                                        Err(_) => return Err("channel_recv: receive error".to_string()),
                                    }
                                }
                            }
                            return Err("channel_recv: channel not found".to_string());
                        }
                        if fname == "channel_subscribe" {
                            // channel_subscribe(channel_id) -> subscriber_id
                            if args.len() != 1 { return Err("channel_subscribe requires 1 argument".to_string()); }
                            let idv = self.eval_expr(args[0].clone())?;
                            let chid = if let Value::Int(n) = idv { n as u64 } else { return Err("channel_subscribe: id must be int".to_string()) };
                            // create new tx/rx pair for subscriber
                            let sub_id = CH_NEXT_ID.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                            let (tx, rx) = mpsc::channel::<String>();
                            // register receiver under sub_id
                            if let Ok(mut rmap) = ch_receivers().lock() { rmap.insert(sub_id, rx); }
                            // register sender in bcast list
                            if let Ok(mut bmap) = ch_bcast().lock() {
                                bmap.entry(chid).or_insert_with(Vec::new).push((sub_id, tx));
                            }
                            // remember mapping
                            if let Ok(mut m) = sub_to_channel().lock() { m.insert(sub_id, chid); }
                            return Ok(Value::Int(sub_id as i64));
                        }
                        if fname == "channel_close" {
                            // channel_close(id) - closes channel or subscriber and cleans resources
                            if args.len() != 1 { return Err("channel_close requires 1 argument".to_string()); }
                            let idv = self.eval_expr(args[0].clone())?;
                            let id = if let Value::Int(n) = idv { n as u64 } else { return Err("channel_close: id must be int".to_string()) };
                            // first, if it's a primary channel
                            if let Ok(mut smap) = ch_senders().lock() {
                                if smap.remove(&id).is_some() {
                                    // remove primary receiver too
                                    if let Ok(mut rmap) = ch_receivers().lock() { rmap.remove(&id); }
                                    // remove and cleanup broadcast subscribers
                                    if let Ok(mut bmap) = ch_bcast().lock() {
                                        if let Some(list) = bmap.remove(&id) {
                                            for (subid, _tx) in list {
                                                if let Ok(mut rmap) = ch_receivers().lock() { rmap.remove(&subid); }
                                                if let Ok(mut m) = sub_to_channel().lock() { m.remove(&subid); }
                                            }
                                        }
                                    }
                                    return Ok(Value::Int(1));
                                }
                            }
                            // if it's a subscriber or receiver id
                            if let Ok(mut rmap) = ch_receivers().lock() {
                                if rmap.remove(&id).is_some() {
                                    // if subscriber, remove its sender from bcast list
                                    if let Ok(mut m) = sub_to_channel().lock() {
                                        if let Some(chid) = m.remove(&id) {
                                            if let Ok(mut bmap) = ch_bcast().lock() {
                                                if let Some(list) = bmap.get_mut(&chid) {
                                                    list.retain(|(sid, _)| *sid != id);
                                                }
                                            }
                                        }
                                    }
                                    return Ok(Value::Int(1));
                                }
                            }
                            return Err("channel_close: id not found".to_string());
                        }
                        if fname == "set_theme" {
                            // set_theme(name)
                            if args.len() != 1 { return Err("set_theme requires 1 argument".to_string()); }
                            let nv = self.eval_expr(args[0].clone())?;
                            let name = if let Value::Str(s) = nv { s } else { return Err("set_theme: arg must be string".to_string()) };
                            #[cfg(target_os = "windows")] { crate::platform::windows::set_theme(&name); }
                            return Ok(Value::Int(1));
                        }
                        let val = self.get_var(&fname).ok_or_else(|| format!("undefined function/class {}", fname))?;
                        match val {
                            Value::Function(fobj) => {
                                if fobj.params.len() != args.len() { return Err("arg count mismatch".to_string()); }
                                // evaluate args first
                                let mut avals = Vec::new();
                                for a in &args { avals.push(self.eval_expr(a.clone())?); }
                                self.push_frame();
                                for (i, p) in fobj.params.iter().enumerate() {
                                    let aval = avals[i].clone();
                                    self.set_local(p.clone(), aval);
                                }
                                let res = self.execute_program(fobj.body.clone())?;
                                self.pop_frame();
                                Ok(res.unwrap_or(Value::Int(0)))
                            }
                            Value::Class(cobj) => {
                                // construct object: copy class methods
                                let mut obj_methods = HashMap::new();
                                for (k, v) in &cobj.methods { obj_methods.insert(k.clone(), v.clone()); }
                                let obj = Rc::new(RefCell::new(Object { class_name: cobj.name.clone(), fields: HashMap::new(), methods: obj_methods }));
                                // call __init__ if present
                                if let Some(init) = cobj.methods.get("__init__") {
                                    // evaluate args
                                    let mut avals = Vec::new();
                                    for a in &args { avals.push(self.eval_expr(a.clone())?); }
                                    self.push_frame();
                                    // bind params: if param == "self" bind to obj, else take from avals in order
                                    let mut ai = 0usize;
                                    for p in init.params.iter() {
                                        if p == "self" {
                                            self.set_local("self".to_string(), Value::Object(obj.clone()));
                                        } else {
                                            if ai < avals.len() {
                                                self.set_local(p.clone(), avals[ai].clone());
                                            }
                                            ai += 1;
                                        }
                                    }
                                    let _ = self.execute_program(init.body.clone())?;
                                    self.pop_frame();
                                }
                                Ok(Value::Object(obj))
                            }
                            _ => Err("call of non-callable".to_string()),
                        }
                    }
                    _ => Err("call of non-identifier not supported".to_string()),
                }
            }
            Expr::MemberCall { receiver, method, args } => {
                let recv = self.eval_expr(*receiver)?;
                if let Value::Object(o) = recv {
                    // find method in object
                    let m = o.borrow().methods.get(&method).cloned().ok_or_else(|| format!("method {} not found", method))?;
                    // evaluate args first
                    let mut avals = Vec::new();
                    for a in &args { avals.push(self.eval_expr(a.clone())?); }
                    self.push_frame();
                    // bind params: if param == "self" bind to object, else take next arg
                    let mut ai = 0usize;
                    for p in m.params.iter() {
                        if p == "self" {
                            self.set_local("self".to_string(), Value::Object(o.clone()));
                        } else {
                            if ai < avals.len() {
                                self.set_local(p.clone(), avals[ai].clone());
                            }
                            ai += 1;
                        }
                    }
                    let res = self.execute_program(m.body.clone())?;
                    self.pop_frame();
                    Ok(res.unwrap_or(Value::Int(0)))
                } else { Err("member call on non-object".to_string()) }
            }
        }
    }
}
