//! CLI: parse, inspect, and compile `.wvg` files.
//!
//!   windvg check <file.wvg>
//!   windvg ir    <file.wvg> [-o out.json]
//!   windvg ops   <file.wvg> [-o out.json]
//!   windvg svg   <file.wvg> [-o out.svg]
//!   windvg tvg   <file.wvg> [-o out.tvg] [--scale N]

use std::io::Write;
use std::process::ExitCode;

fn usage() -> String {
    "usage: windvg <check|ir|ops|svg|tvg> <file.wvg> [-o out] [--scale N]".into()
}

struct Args {
    cmd: String,
    file: String,
    out: Option<String>,
    scale: u32,
    drop_text: bool,
}

fn parse_args() -> Result<Args, String> {
    let mut it = std::env::args().skip(1);
    let cmd = it.next().ok_or_else(usage)?;
    if cmd == "--help" || cmd == "-h" {
        return Err(usage());
    }
    let file = it.next().ok_or_else(|| "missing input file".to_string())?;
    let mut out = None;
    let mut scale = 4u32;
    let mut drop_text = false;
    while let Some(a) = it.next() {
        match a.as_str() {
            "--drop-text" => drop_text = true,
            "-o" => out = Some(it.next().ok_or_else(|| "-o needs a value".to_string())?),
            "--scale" => {
                let v = it
                    .next()
                    .ok_or_else(|| "--scale needs a value".to_string())?;
                scale = v
                    .parse()
                    .map_err(|_| "--scale must be an integer 0..15".to_string())?;
                if scale > 15 {
                    return Err("--scale must be 0..15".into());
                }
            }
            other => return Err(format!("unknown option `{other}`")),
        }
    }
    if cmd == "tvg" && out.is_none() {
        let p = std::path::Path::new(&file);
        let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or("out");
        out = Some(format!("{stem}.tvg"));
    }
    Ok(Args {
        cmd,
        file,
        out,
        scale,
        drop_text,
    })
}

fn write_out(path: &Option<String>, data: &str) -> std::io::Result<()> {
    match path {
        Some(p) => std::fs::write(p, data),
        None => {
            std::io::stdout().write_all(data.as_bytes())?;
            if !data.ends_with('\n') {
                println!();
            }
            Ok(())
        }
    }
}

fn fail(file: &str, e: windvg::ir::Diag) -> ! {
    eprintln!("{file}:{}:{}: {}", e.line, e.col, e.msg);
    std::process::exit(1)
}

fn main() -> ExitCode {
    let args = match parse_args() {
        Ok(a) => a,
        Err(msg) => {
            eprintln!("{msg}\n{}", usage());
            return ExitCode::from(2);
        }
    };
    let src = match std::fs::read_to_string(&args.file) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("{}: {e}", args.file);
            return ExitCode::from(1);
        }
    };

    let mut doc = match windvg::parser::parse(&src) {
        Ok(d) => d,
        Err(e) => fail(&args.file, e),
    };
    windvg::parser::assign_ids(&mut doc);

    match args.cmd.as_str() {
        "check" => match windvg::resolve::resolve(&doc) {
            Ok(_) => {
                println!("ok: {} nodes", doc.nodes.len());
                ExitCode::SUCCESS
            }
            Err(e) => fail(&args.file, e),
        },
        "ir" => write_out(&args.out, &windvg::json::document_json(&doc))
            .map(|_| ExitCode::SUCCESS)
            .unwrap_or_else(|e| {
                eprintln!("{e}");
                ExitCode::from(1)
            }),
        "ops" => match windvg::resolve::resolve(&doc) {
            Ok(ops) => write_out(&args.out, &windvg::json::ops_json(&ops))
                .map(|_| ExitCode::SUCCESS)
                .unwrap_or_else(|e| {
                    eprintln!("{e}");
                    ExitCode::from(1)
                }),
            Err(e) => fail(&args.file, e),
        },
        "svg" => match windvg::resolve::resolve(&doc) {
            Ok(ops) => write_out(&args.out, &windvg::svg::render(&ops, doc.width, doc.height))
                .map(|_| ExitCode::SUCCESS)
                .unwrap_or_else(|e| {
                    eprintln!("{e}");
                    ExitCode::from(1)
                }),
            Err(e) => fail(&args.file, e),
        },
        "tvg" => match windvg::resolve::resolve(&doc) {
            Ok(ops) => {
                match windvg::tvg::encode(&ops, doc.width, doc.height, args.scale, args.drop_text) {
                    Ok(bytes) => match &args.out {
                        Some(p) => {
                            if let Err(e) = std::fs::write(p, &bytes) {
                                eprintln!("{e}");
                                return ExitCode::from(1);
                            }
                            println!("wrote {} ({} bytes)", p, bytes.len());
                            ExitCode::SUCCESS
                        }
                        None => {
                            use std::io::Write;
                            std::io::stdout()
                                .write_all(&bytes)
                                .map(|_| ExitCode::SUCCESS)
                                .unwrap_or_else(|e| {
                                    eprintln!("{e}");
                                    ExitCode::from(1)
                                })
                        }
                    },
                    Err(e) => {
                        eprintln!("{}: {e}", args.file);
                        ExitCode::from(1)
                    }
                }
            }
            Err(e) => fail(&args.file, e),
        },
        other => {
            eprintln!("unknown command `{other}`\n{}", usage());
            ExitCode::from(2)
        }
    }
}
