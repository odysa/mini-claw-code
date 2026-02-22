use std::process::{Command, exit};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();

    match args.first().map(|s| s.as_str()) {
        Some("check") => check("mini-claw-code-starter"),
        Some("solution-check") => check("mini-claw-code"),
        Some("book") => book(),
        Some(cmd) => {
            eprintln!("Unknown command: {cmd}");
            eprintln!("Usage: cargo x <command>");
            eprintln!("Commands: check, solution-check, book");
            exit(1);
        }
        None => {
            eprintln!("Usage: cargo x <command>");
            eprintln!("Commands: check, solution-check, book");
            exit(1);
        }
    }
}

fn check(package: &str) {
    println!("Checking {package}...\n");

    run("cargo", &["fmt", "--check", "-p", package], "fmt");
    run(
        "cargo",
        &["clippy", "-p", package, "--", "-D", "warnings"],
        "clippy",
    );
    run("cargo", &["test", "-p", package], "test");

    println!("\nAll checks passed for {package}!");
}

fn run(cmd: &str, args: &[&str], label: &str) {
    println!("--- {label} ---");
    let status = Command::new(cmd).args(args).status().unwrap_or_else(|e| {
        eprintln!("Failed to run {cmd}: {e}");
        exit(1);
    });

    if !status.success() {
        eprintln!("\n{label} failed!");
        exit(1);
    }
    println!();
}

fn book() {
    println!("Building and serving mdbook...");
    let status = Command::new("mdbook")
        .args(["serve", "mini-claw-code-book"])
        .status()
        .unwrap_or_else(|e| {
            eprintln!("Failed to run mdbook: {e}");
            eprintln!("Install mdbook with: cargo install mdbook");
            exit(1);
        });

    if !status.success() {
        exit(1);
    }
}
