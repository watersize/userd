use std::net::{TcpListener, TcpStream};
use std::io::{Read, Write};
use std::thread;
use std::fs;

fn handle_client(mut stream: TcpStream) {
    let mut buf = Vec::new();
    if let Err(_) = stream.read_to_end(&mut buf) { return; }
    let req = String::from_utf8_lossy(&buf);
    let mut lines = req.lines();
    let first = lines.next().unwrap_or("");
    let mut parts = first.split_whitespace();
    let method = parts.next().unwrap_or("");
    let path = parts.next().unwrap_or("");

    if method == "GET" {
        let file = match path {
            "/" => "static/editor.html",
            "/app.js" => "static/app.js",
            "/style.css" => "static/style.css",
            _ => {
                // try to strip leading /
                let p = &path[1..];
                if p.starts_with("static/") { p } else { "" }
            }
        };
        if file.is_empty() {
            let resp = "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n";
            let _ = stream.write_all(resp.as_bytes());
            return;
        }
        match fs::read_to_string(file) {
            Ok(body) => {
                let content_type = if file.ends_with(".js") { "application/javascript" } else if file.ends_with(".css") { "text/css" } else { "text/html" };
                let header = format!("HTTP/1.1 200 OK\r\nContent-Type: {}; charset=utf-8\r\nContent-Length: {}\r\n\r\n", content_type, body.len());
                let _ = stream.write_all(header.as_bytes());
                let _ = stream.write_all(body.as_bytes());
            }
            Err(_) => {
                let resp = "HTTP/1.1 500 Internal Server Error\r\nContent-Length: 0\r\n\r\n";
                let _ = stream.write_all(resp.as_bytes());
            }
        }
        return;
    }

    if method == "POST" && path == "/run" {
        // find blank line separating headers and body
        let reqs = req.as_ref();
        if let Some(idx) = reqs.find("\r\n\r\n") {
            let body = &reqs[idx+4..];
            // body is raw code
            let code = body.to_string();
            // execute code using parser + vm
            let mut parser = crate::parser::Parser::new(&code);
            let prog = parser.parse_program();
            let mut vm = crate::vm::VM::new();
            match vm.execute_program(prog) {
                Ok(opt) => {
                    let json = match opt {
                        Some(v) => {
                            let mut s = String::from("{\"ok\":true,\"result\":");
                            s.push_str(&serialize_value(&v));
                            s.push('}');
                            s
                        }
                        None => "{\"ok\":true,\"result\":null}".to_string(),
                    };
                    let header = format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n", json.len());
                    let _ = stream.write_all(header.as_bytes());
                    let _ = stream.write_all(json.as_bytes());
                }
                Err(e) => {
                    let esc = e.replace('"', "\\\"");
                    let json = format!("{{\"ok\":false,\"error\":\"{}\"}}", esc);
                    let header = format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n", json.len());
                    let _ = stream.write_all(header.as_bytes());
                    let _ = stream.write_all(json.as_bytes());
                }
            }
        } else {
            let resp = "HTTP/1.1 400 Bad Request\r\nContent-Length: 0\r\n\r\n";
            let _ = stream.write_all(resp.as_bytes());
        }
        return;
    }

    // default 404
    let resp = "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n";
    let _ = stream.write_all(resp.as_bytes());
}

fn serialize_value(v: &crate::vm::Value) -> String {
    match v {
        crate::vm::Value::Int(n) => format!("{{\"type\":\"int\",\"value\":{}}}", n),
        crate::vm::Value::Str(s) => format!("{{\"type\":\"str\",\"value\":\"{}\"}}", s.replace('"', "\\\"")),
        crate::vm::Value::Object(o) => {
            // show fields only
            let b = o.borrow();
            let mut fields = Vec::new();
            for (k, val) in &b.fields {
                fields.push(format!("\"{}\":{}", k, serialize_value(val)));
            }
            format!("{{\"type\":\"object\",\"class\":\"{}\",\"fields\":{{{}}}}}", b.class_name, fields.join(","))
        }
        _ => format!("{{\"type\":\"other\"}}"),
    }
}

pub fn run_server(addr: &str) -> std::io::Result<()> {
    let listener = TcpListener::bind(addr)?;
    println!("Editor server running at http://{}", addr);
    for stream in listener.incoming() {
        match stream {
            Ok(s) => { thread::spawn(|| handle_client(s)); }
            Err(e) => eprintln!("connection failed: {}", e),
        }
    }
    Ok(())
}
