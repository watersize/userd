/// Very small CLI: supports `userd repl` and `userd <file.usrd>` to run scripts
pub fn run() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() > 1 {
        match args[1].as_str() {
            "repl" => crate::repl::start_repl(),
            "editor" => {
                // start web editor server and open browser
                let addr = "127.0.0.1:7878";
                // attempt to open default browser
                let url = format!("http://{}", addr);
                // spawn server in foreground (blocking)
                println!("Starting editor at {}", url);
                // try to open browser (best-effort)
                if cfg!(target_os = "windows") {
                    let _ = std::process::Command::new("cmd").args(["/C", "start", &url]).spawn();
                } else if cfg!(target_os = "macos") {
                    let _ = std::process::Command::new("open").arg(&url).spawn();
                } else {
                    let _ = std::process::Command::new("xdg-open").arg(&url).spawn();
                }
                let _ = crate::web_server::run_server(addr);
            }
            "pack" => {
                // pack a .usrd script into a self-extracting exe: userd pack script.usrd out.exe
                if args.len() < 4 {
                    eprintln!("usage: userd pack <script.usrd> <out.exe>");
                    return;
                }
                let script = &args[2];
                let out = &args[3];
                match std::fs::read_to_string(script) {
                    Ok(src) => {
                        // read current exe as template
                        let me = std::env::current_exe().expect("failed to locate current exe");
                        match std::fs::read(&me) {
                            Ok(bin) => {
                                // write to out
                                if let Err(e) = std::fs::write(out, &bin) { eprintln!("failed to write output: {}", e); return; }
                                // append marker and script
                                let mut f = std::fs::OpenOptions::new().append(true).open(out).expect("open out");
                                let marker = b"\n__USRDSCRIPT__\n";
                                use std::io::Write as IoWrite;
                                let _ = f.write_all(marker);
                                let _ = f.write_all(src.as_bytes());
                                println!("packed {} -> {}", script, out);
                            }
                            Err(e) => eprintln!("failed to read current exe: {}", e),
                        }
                    }
                    Err(e) => eprintln!("failed to read script {}: {}", script, e),
                }
            }
            "install" => {
                // install current exe to a user-local bin directory
                let me = match std::env::current_exe() { Ok(p) => p, Err(e) => { eprintln!("failed to locate current exe: {}", e); return; } };
                #[cfg(target_os = "windows")]
                let home = std::env::var("USERPROFILE").unwrap_or(".".to_string());
                #[cfg(not(target_os = "windows"))]
                let home = std::env::var("HOME").unwrap_or(".".to_string());
                #[cfg(target_os = "windows")]
                let dest_dir = format!("{}\\bin", home);
                #[cfg(not(target_os = "windows"))]
                let dest_dir = format!("{}/.local/bin", home);
                if let Err(e) = std::fs::create_dir_all(&dest_dir) { eprintln!("failed to create {}: {}", dest_dir, e); return; }
                #[cfg(target_os = "windows")]
                let dest = format!("{}\\userd.exe", dest_dir);
                #[cfg(not(target_os = "windows"))]
                let dest = format!("{}/userd", dest_dir);
                match std::fs::copy(&me, &dest) {
                    Ok(_) => {
                        #[cfg(not(target_os = "windows") )]
                        { let _ = std::process::Command::new("chmod").args(["+x", &dest]).status(); }
                        println!("installed {} -> {}", me.display(), dest);
                        // optionally auto-add to PATH on Windows
                        if args.len() > 2 && args[2] == "--add-path" {
                            #[cfg(target_os = "windows")]
                            {
                                // Use PowerShell to set user PATH (no admin required)
                                let get_cmd = r#"[Environment]::GetEnvironmentVariable('Path','User')"#;
                                let out = std::process::Command::new("powershell").args(["-NoProfile","-Command", get_cmd]).output();
                                if let Ok(o) = out {
                                    let cur = String::from_utf8_lossy(&o.stdout).trim().to_string();
                                    if !cur.split(';').any(|p| p.eq_ignore_ascii_case(&dest_dir)) {
                                        let new = if cur.is_empty() { dest_dir.clone() } else { format!("{};{}", cur, dest_dir) };
                                        let set_cmd = format!("[Environment]::SetEnvironmentVariable('Path','{}','User')", new.replace("'","''"));
                                        let _ = std::process::Command::new("powershell").args(["-NoProfile","-Command", &set_cmd]).status();
                                        println!("Added {} to user PATH (effective for new processes).", dest_dir);
                                    } else { println!("{} already present in user PATH.", dest_dir); }
                                } else { eprintln!("failed to query user PATH"); }
                            }
                        } else {
                            println!("Make sure {} is in your PATH (add {} to PATH if needed)", dest_dir, dest_dir);
                        }
                    }
                    Err(e) => { eprintln!("failed to copy to {}: {}", dest, e); }
                }
            }
            "uninstall" => {
                // remove installed executable and optionally remove PATH entry
                #[cfg(target_os = "windows")]
                let home = std::env::var("USERPROFILE").unwrap_or(".".to_string());
                #[cfg(not(target_os = "windows"))]
                let home = std::env::var("HOME").unwrap_or(".".to_string());
                #[cfg(target_os = "windows")]
                let dest_dir = format!("{}\\bin", home);
                #[cfg(not(target_os = "windows"))]
                let dest_dir = format!("{}/.local/bin", home);
                #[cfg(target_os = "windows")]
                let dest = format!("{}\\userd.exe", dest_dir);
                #[cfg(not(target_os = "windows"))]
                let dest = format!("{}/userd", dest_dir);
                if std::path::Path::new(&dest).exists() {
                    if let Err(e) = std::fs::remove_file(&dest) { eprintln!("failed to remove {}: {}", dest, e); }
                    else { println!("removed {}", dest); }
                    // remove PATH entry if --remove-path provided
                    if args.len() > 2 && args[2] == "--remove-path" {
                        #[cfg(target_os = "windows")]
                        {
                            let get_cmd = r#"[Environment]::GetEnvironmentVariable('Path','User')"#;
                            if let Ok(o) = std::process::Command::new("powershell").args(["-NoProfile","-Command", get_cmd]).output() {
                                let cur = String::from_utf8_lossy(&o.stdout).trim().to_string();
                                let parts: Vec<&str> = cur.split(';').filter(|p| !p.eq_ignore_ascii_case(&dest_dir) && !p.is_empty()).collect();
                                let new = parts.join(";");
                                let set_cmd = format!("[Environment]::SetEnvironmentVariable('Path','{}','User')", new.replace("'","''"));
                                let _ = std::process::Command::new("powershell").args(["-NoProfile","-Command", &set_cmd]).status();
                                println!("Removed {} from user PATH (effective for new processes).", dest_dir);
                            }
                        }
                    }
                } else {
                    println!("{} not found, nothing to uninstall.", dest);
                }
            }
            "compile" => {
                // compile a .usrd source into a .usrdc artifact: userd compile in.usrd out.usrdc
                if args.len() < 4 {
                    eprintln!("usage: userd compile <in.usrd> <out.usrdc>");
                    return;
                }
                let input = &args[2];
                let out = &args[3];
                match std::fs::read_to_string(input) {
                    Ok(src) => {
                        // basic validation: parse
                        let mut parser = crate::parser::Parser::new(&src);
                        let _prog = parser.parse_program();
                        // try to parse simple metadata headers at top of file
                        let mut meta_lines: Vec<String> = Vec::new();
                        for line in src.lines().take(16) {
                            if line.trim().is_empty() { continue; }
                            let l = line.trim();
                            if l.to_lowercase().starts_with("os:") || l.to_lowercase().starts_with("its:") {
                                meta_lines.push(l.to_string());
                            }
                        }
                        // build artifact: META marker + metadata + SRC marker + source
                        let meta_marker = b"__USRDMETA__\n";
                        let src_marker = b"__USRDSRC__\n";
                        let mut out_bytes: Vec<u8> = Vec::new();
                        out_bytes.extend_from_slice(meta_marker);
                        for m in meta_lines.iter() {
                            out_bytes.extend_from_slice(m.as_bytes());
                            out_bytes.push(b'\n');
                        }
                        out_bytes.extend_from_slice(src_marker);
                        out_bytes.extend_from_slice(src.as_bytes());
                        match std::fs::write(out, out_bytes) {
                            Ok(_) => println!("compiled {} -> {}", input, out),
                            Err(e) => eprintln!("failed to write out file: {}", e),
                        }
                    }
                    Err(e) => eprintln!("failed to read {}: {}", input, e),
                }
            }
            path => {
                if path.ends_with(".usrd") {
                    match std::fs::read_to_string(path) {
                        Ok(src) => {
                            // parse and execute
                            let mut parser = crate::parser::Parser::new(&src);
                            let prog = parser.parse_program();
                            let mut vm = crate::vm::VM::new();
                            if let Err(e) = vm.execute_program(prog) {
                                eprintln!("Execution error: {}", e);
                            }
                        }
                        Err(e) => eprintln!("Failed to read file {}: {}", path, e),
                    }
                } else if path.ends_with(".usrdc") {
                    // compiled artifact produced by `userd compile` -- contains embedded source after marker
                    match std::fs::read(path) {
                        Ok(bytes) => {
                            // marker kept for backward compatibility (not used below)
                            let _marker = b"__USRDSRC__\n";
                                // check for metadata and source markers
                                let meta_marker = b"__USRDMETA__\n";
                                let src_marker = b"__USRDSRC__\n";
                                // optional: read metadata and warn about OS compatibility
                                if let Some(meta_pos) = find_subslice_from_start(&bytes, meta_marker) {
                                    if let Some(src_pos) = find_subslice_from_start(&bytes, src_marker) {
                                        if src_pos > meta_pos {
                                            let meta = &bytes[meta_pos + meta_marker.len()..src_pos];
                                            if let Ok(meta_s) = std::str::from_utf8(meta) {
                                                for line in meta_s.lines() {
                                                    let l = line.trim();
                                                    if l.to_lowercase().starts_with("os:") {
                                                        let oslist = l[3..].trim();
                                                        // if current os not mentioned, warn
                                                        let cur = if cfg!(target_os = "windows") { "windows" }
                                                                  else if cfg!(target_os = "macos") { "macos" }
                                                                  else { "linux" };
                                                        if !oslist.to_lowercase().contains(cur) {
                                                            eprintln!("Warning: artifact targets [{}], current OS {} may be incompatible.", oslist, cur);
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                                // find source marker and run
                                if let Some(pos) = find_subslice_from_start(&bytes, src_marker) {
                                    let script = &bytes[pos + src_marker.len()..];
                                    if let Ok(s) = std::str::from_utf8(script) {
                                        let mut parser = crate::parser::Parser::new(s);
                                        let prog = parser.parse_program();
                                        let mut vm = crate::vm::VM::new();
                                        if let Err(e) = vm.execute_program(prog) { eprintln!("Execution error: {}", e); }
                                    } else { eprintln!("compiled artifact contains invalid utf8"); }
                                } else { eprintln!("compiled artifact missing marker"); }
                        }
                        Err(e) => eprintln!("Failed to read compiled file {}: {}", path, e),
                    }
                } else {
                    println!("unknown command or file: {}\nUse `userd repl`, `userd editor` or pass a .usrd file", path);
                }
            }
        }
    } else {
        // try to detect embedded script in this executable; if present, run it
        if try_run_embedded().is_ok() {
            return;
        }
        println!("userd — экспериментальный язык\nЗапуск REPL: `userd repl`\nЗапуск файла: `userd script.usrd`\nЗапустить редактор: `userd editor`\nУпаковать: `userd pack script.usrd out.exe`");
    }
}

fn find_subslice_from_start(hay: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.len() == 0 || hay.len() < needle.len() { return None }
    for start in 0..=(hay.len() - needle.len()) {
        if &hay[start..start + needle.len()] == needle { return Some(start) }
    }
    None
}

fn try_run_embedded() -> Result<(), ()> {
    // read this executable and look for marker
    let me = match std::env::current_exe() { Ok(p) => p, Err(_) => return Err(()) };
    let data = match std::fs::read(&me) { Ok(d) => d, Err(_) => return Err(()) };
    let marker = b"\n__USRDSCRIPT__\n";
    if let Some(idx) = find_subslice_from_end(&data, marker) {
        let script = &data[idx + marker.len()..];
        if script.is_empty() { return Err(()) }
        // execute script
        if let Ok(s) = std::str::from_utf8(script) {
            let mut parser = crate::parser::Parser::new(s);
            let prog = parser.parse_program();
            let mut vm = crate::vm::VM::new();
            if let Err(e) = vm.execute_program(prog) {
                eprintln!("Execution error: {}", e);
            }
            return Ok(());
        }
    }
    Err(())
}

fn find_subslice_from_end(hay: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.len() == 0 || hay.len() < needle.len() { return None }
    // search backwards
    for start in (0..=(hay.len() - needle.len())).rev() {
        if &hay[start..start + needle.len()] == needle { return Some(start) }
    }
    None
}
