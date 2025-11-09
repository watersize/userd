fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("usage: usrdc_compiler <in.usrd> <out.usrdc>\nor: usrdc_compiler pack <template.exe> <in.usrd> <out.exe>");
        std::process::exit(2);
    }

    // Support a pack mode to create a self-contained EXE by embedding the script into a template exe.
    if args[1] == "pack" {
        if args.len() < 5 {
            eprintln!("usage: usrdc_compiler pack <template.exe> <in.usrd> <out.exe>");
            std::process::exit(2);
        }
        let template = &args[2];
        let input = &args[3];
        let outexe = &args[4];
        let src = match std::fs::read_to_string(input) {
            Ok(s) => s,
            Err(e) => { eprintln!("failed to read {}: {}", input, e); std::process::exit(1); }
        };
        let mut tpl = match std::fs::read(template) {
            Ok(b) => b,
            Err(e) => { eprintln!("failed to read template {}: {}", template, e); std::process::exit(1); }
        };
        // append marker and script
        tpl.extend_from_slice(b"\n__USRDSCRIPT__\n");
        tpl.extend_from_slice(src.as_bytes());
        match std::fs::write(outexe, &tpl) {
            Ok(_) => println!("packed {} + {} -> {}", template, input, outexe),
            Err(e) => { eprintln!("failed to write {}: {}", outexe, e); std::process::exit(1); }
        }
        return;
    }

    let input = &args[1];
    let out = &args[2];
    let src = match std::fs::read_to_string(input) {
        Ok(s) => s,
        Err(e) => { eprintln!("failed to read {}: {}", input, e); std::process::exit(1); }
    };
    // Basic validation: parse
    let mut parser = userd::parser::Parser::new(&src);
    let _prog = parser.parse_program();
    // extract simple metadata headers
    let mut meta_lines: Vec<String> = Vec::new();
    for line in src.lines().take(16) {
        if line.trim().is_empty() { continue; }
        let l = line.trim();
        if l.to_lowercase().starts_with("os:") || l.to_lowercase().starts_with("its:") {
            meta_lines.push(l.to_string());
        }
    }
    let meta_marker = b"__USRDMETA__\n";
    let src_marker = b"__USRDSRC__\n";
    let mut out_bytes: Vec<u8> = Vec::new();
    out_bytes.extend_from_slice(meta_marker);
    for m in meta_lines.iter() { out_bytes.extend_from_slice(m.as_bytes()); out_bytes.push(b'\n'); }
    out_bytes.extend_from_slice(src_marker);
    out_bytes.extend_from_slice(src.as_bytes());
    match std::fs::write(out, out_bytes) {
        Ok(_) => println!("compiled {} -> {}", input, out),
        Err(e) => { eprintln!("failed to write {}: {}", out, e); std::process::exit(1); }
    }
}
