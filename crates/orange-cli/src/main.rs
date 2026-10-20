//! `orange` headless CLI: handy for scripting and for testing the engine without the GUI.
use orange_core::Capabilities;

fn main() {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!("usage: orange <convert|tool|deps> ...");
        std::process::exit(2);
    }
    let sub = args.remove(0);
    let code = match sub.as_str() {
        "convert" => orange_convert::cli::run(args),
        "tool" => orange_tools::cli::run(args),
        "deps" => {
            let caps = Capabilities::detect(&[]);
            println!("{}", serde_json::to_string_pretty(&caps.to_json()).unwrap());
            0
        }
        "counts" => {
            println!("conversion options: {}", orange_convert::option_count());
            println!("tools: {}", orange_tools::all_tools().len());
            0
        }
        other => {
            eprintln!("unknown subcommand: {other}");
            2
        }
    };
    std::process::exit(code);
}
