#![windows_subsystem = "windows"]
#![allow(unsafe_op_in_unsafe_fn)]
mod config;
mod ipc;
mod logging;
mod model;
mod platform;
mod sensors;
mod ui;

fn main() {
    if let Err(e) = run() {
        if !std::env::args().any(|a| a == "--collector") {
            logging::write(&e);
        }
        eprintln!("{e}");
        std::process::exit(1);
    }
}
fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let option = |name: &str| {
        args.iter()
            .position(|a| a == name)
            .and_then(|i| args.get(i + 1))
            .cloned()
    };
    if args.iter().any(|a| a == "--default-profile") {
        let path = option("--out")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(config::default_path);
        config::Config::default().save(&path)?;
        return Ok(());
    }
    if args.iter().any(|a| a == "--diagnose") {
        let mut collector = sensors::Collector::new();
        let count = option("--samples")
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(2)
            .clamp(1, 3600);
        let mut snapshots = Vec::new();
        for i in 0..count {
            snapshots.push(collector.poll());
            if i + 1 < count {
                std::thread::sleep(std::time::Duration::from_secs(1));
            }
        }
        let report = serde_json::json!({"version":env!("CARGO_PKG_VERSION"),"hardware":platform::inventory(),"snapshots":snapshots});
        let bytes = serde_json::to_vec_pretty(&report).map_err(|e| e.to_string())?;
        if let Some(path) = option("--out") {
            std::fs::write(path, bytes).map_err(|e| e.to_string())?;
        } else {
            println!("{}", String::from_utf8_lossy(&bytes));
        }
        return Ok(());
    }
    if args.iter().any(|a| a == "--collector") {
        return ipc::serve();
    }
    if args.iter().any(|a| a == "--shutdown") {
        return ipc::shutdown();
    }
    let path = option("--profile")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(config::default_path);
    ui::run(
        path,
        args.iter().any(|a| a == "--preview"),
        option("--exit-after").and_then(|s| s.parse().ok()),
    )
}
