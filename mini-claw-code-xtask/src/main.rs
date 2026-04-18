use std::process::{Command, exit};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();

    match args.first().map(|s| s.as_str()) {
        Some("check") => starter_check("mini-claw-code-starter"),
        Some("solution-check") => check("mini-claw-code"),
        Some("book") => book(),
        Some("book-build") => book_build(),
        Some(cmd) => {
            eprintln!("Unknown command: {cmd}");
            usage();
            exit(1);
        }
        None => {
            usage();
            exit(1);
        }
    }
}

fn usage() {
    eprintln!("Usage: cargo x <command>");
    eprintln!("Commands: check, solution-check, book, book-build");
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

/// Starter-template check: verifies the skeleton compiles and lints cleanly,
/// but does NOT run tests (they're expected to fail on `unimplemented!()` stubs
/// until the learner fills them in).
fn starter_check(package: &str) {
    println!("Checking starter template {package}...\n");

    run("cargo", &["fmt", "--check", "-p", package], "fmt");
    run(
        "cargo",
        &["clippy", "-p", package, "--", "-D", "warnings"],
        "clippy",
    );
    run("cargo", &["test", "-p", package, "--no-run"], "test-build");

    println!("\nStarter template {package} compiles cleanly.");
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
    println!("Building and serving mdbook (English)...");
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

fn book_build() {
    println!("Building the book...\n");

    let book_dir = "mini-claw-code-book";

    let status = Command::new("mdbook")
        .args(["build", book_dir])
        .status()
        .unwrap_or_else(|e| {
            eprintln!("Failed to run mdbook: {e}");
            exit(1);
        });
    if !status.success() {
        eprintln!("Book build failed!");
        exit(1);
    }

    println!("\nBook built to {book_dir}/book/");
}
