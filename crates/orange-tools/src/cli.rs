//! `orange tool ...` headless interface.

use orange_core::{Capabilities, Ctx};
use serde_json::Value;
use std::path::PathBuf;
use std::sync::Arc;

fn usage() -> i32 {
    eprintln!(
        "usage:\n  orange tool list\n  orange tool for <files...>\n  orange tool run <id> <files...> [--params JSON] [--out-dir DIR]\n  orange tool preview <id> <file> [--params JSON] [--max-dim N]\n  orange tool web-preview <file> [--max-dim N]\n  orange tool info <file>\n  orange tool meta <file>"
    );
    2
}

struct Parsed {
    positional: Vec<String>,
    params: Value,
    out_dir: Option<PathBuf>,
    max_dim: u32,
}

fn parse(args: &[String]) -> Result<Parsed, String> {
    let mut p = Parsed { positional: vec![], params: serde_json::json!({}), out_dir: None, max_dim: 900 };
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--params" => p.params = serde_json::from_str(it.next().ok_or("--params needs a value")?).map_err(|e| format!("bad --params JSON: {e}"))?,
            "--out-dir" => p.out_dir = Some(PathBuf::from(it.next().ok_or("--out-dir needs a value")?)),
            "--max-dim" => p.max_dim = it.next().ok_or("--max-dim needs a value")?.parse().map_err(|_| "bad --max-dim")?,
            other => p.positional.push(other.to_string()),
        }
    }
    Ok(p)
}

pub fn run(mut args: Vec<String>) -> i32 {
    if args.is_empty() {
        return usage();
    }
    let sub = args.remove(0);
    let p = match parse(&args) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("{e}");
            return 2;
        }
    };
    let caps = Arc::new(Capabilities::detect(&[]));
    let ctx = Ctx::new(caps.clone()).with_out_dir(p.out_dir.clone()).with_progress(|pr| eprintln!("[{:>3.0}%] {}", pr.fraction * 100.0, pr.message));
    let fail = |e: orange_core::Error| {
        eprintln!("error: {e}");
        1
    };
    match sub.as_str() {
        "list" => {
            println!("{}", serde_json::to_string_pretty(&crate::all_tools()).unwrap());
            0
        }
        "for" => {
            let paths: Vec<PathBuf> = p.positional.iter().map(PathBuf::from).collect();
            let ids: Vec<String> = crate::tools_for(&paths, &caps).into_iter().map(|t| t.id).collect();
            println!("{}", serde_json::to_string(&ids).unwrap());
            0
        }
        "run" => {
            if p.positional.len() < 2 {
                return usage();
            }
            let id = p.positional[0].clone();
            let inputs: Vec<PathBuf> = p.positional[1..].iter().map(PathBuf::from).collect();
            match crate::run_tool(&id, &inputs, &p.params, &ctx) {
                Ok(outs) => {
                    for o in outs {
                        println!("{}", o.display());
                    }
                    0
                }
                Err(e) => fail(e),
            }
        }
        "preview" => {
            if p.positional.len() != 2 {
                return usage();
            }
            match crate::preview(&p.positional[0], &PathBuf::from(&p.positional[1]), &p.params, &ctx, p.max_dim) {
                Ok(o) => {
                    println!("{}", o.display());
                    0
                }
                Err(e) => fail(e),
            }
        }
        "web-preview" => {
            if p.positional.len() != 1 {
                return usage();
            }
            match crate::web_preview(&PathBuf::from(&p.positional[0]), &ctx, p.max_dim) {
                Ok(o) => {
                    println!("{}", o.display());
                    0
                }
                Err(e) => fail(e),
            }
        }
        "info" | "meta" => {
            if p.positional.len() != 1 {
                return usage();
            }
            let path = PathBuf::from(&p.positional[0]);
            let r = if sub == "info" { crate::file_info(&path, &caps) } else { crate::read_metadata(&path, &caps) };
            match r {
                Ok(v) => {
                    println!("{}", serde_json::to_string_pretty(&v).unwrap());
                    0
                }
                Err(e) => fail(e),
            }
        }
        _ => usage(),
    }
}
