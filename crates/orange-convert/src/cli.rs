//! `orange convert <file> --to <fmt> [--out-dir DIR]`, `orange convert --list <file>`, `orange convert --count`

use orange_core::{Capabilities, Ctx};
use std::path::PathBuf;
use std::sync::Arc;

fn usage() {
    eprintln!("usage: orange convert <file> --to <fmt> [--out-dir DIR]\n       orange convert --list <file>\n       orange convert --count");
}

pub fn run(args: Vec<String>) -> i32 {
    let mut file: Option<String> = None;
    let mut to: Option<String> = None;
    let mut out_dir: Option<PathBuf> = None;
    let mut list = false;
    let mut count = false;
    let mut it = args.into_iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--count" => count = true,
            "--list" => list = true,
            "--to" => to = it.next(),
            "--out-dir" => out_dir = it.next().map(PathBuf::from),
            "-h" | "--help" => {
                usage();
                return 0;
            }
            _ if a.starts_with("--") => {
                eprintln!("unknown option: {a}");
                usage();
                return 2;
            }
            _ => file = Some(a),
        }
    }
    if count {
        println!("{}", crate::option_count());
        return 0;
    }
    let Some(file) = file else {
        usage();
        return 2;
    };
    let path = PathBuf::from(&file);
    let caps = Arc::new(Capabilities::detect(&[]));
    if list {
        let opts = crate::options_for(&path, &caps);
        match serde_json::to_string_pretty(&opts) {
            Ok(j) => {
                println!("{j}");
                return 0;
            }
            Err(e) => {
                eprintln!("{e}");
                return 1;
            }
        }
    }
    let Some(to) = to else {
        usage();
        return 2;
    };
    let ctx = Ctx::new(caps).with_out_dir(out_dir).with_progress(|p| {
        eprintln!("[{:>3.0}%] {}", p.fraction * 100.0, p.message);
    });
    match crate::convert(&path, &to, &ctx) {
        Ok(outs) => {
            for o in outs {
                println!("{}", o.display());
            }
            0
        }
        Err(e) => {
            eprintln!("error: {e}");
            1
        }
    }
}
