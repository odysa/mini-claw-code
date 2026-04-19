use std::process::{Command, exit};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();

    match args.first().map(|s| s.as_str()) {
        Some("check") => starter_check("mini-claw-code-starter"),
        Some("solution-check") => check("mini-claw-code"),
        Some("book") => book(),
        Some("book-build") => book_build(),
        Some("book-filter-check") => book_filter_check(),
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
    eprintln!("Commands: check, solution-check, book, book-build, book-filter-check");
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

/// Scan every `cargo test -p <pkg> <filter>` invocation that appears in the
/// book's markdown and verify the filter matches at least one test. Catches
/// silent-zero-match filters like `test_streaming_parse_sse_` that look
/// passing but exercise nothing.
fn book_filter_check() {
    println!("Checking book test-name filters...\n");

    let book_src = "mini-claw-code-book/src";
    let entries = std::fs::read_dir(book_src).unwrap_or_else(|e| {
        eprintln!("Failed to read {book_src}: {e}");
        exit(1);
    });

    let mut failures: Vec<String> = Vec::new();
    let mut checked = 0usize;

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }
        let text = match std::fs::read_to_string(&path) {
            Ok(t) => t,
            Err(_) => continue,
        };
        let mut skip_next_block = false;
        let mut in_code_fence = false;
        for (lineno, line) in text.lines().enumerate() {
            let trimmed_full = line.trim();
            if trimmed_full.contains("<!-- book-filter-check: skip-block -->") {
                skip_next_block = true;
                continue;
            }
            if trimmed_full.starts_with("```") {
                if in_code_fence {
                    // Leaving a fence — clear the per-block skip flag.
                    in_code_fence = false;
                    skip_next_block = false;
                } else {
                    in_code_fence = true;
                }
                continue;
            }
            if skip_next_block {
                continue;
            }
            let trimmed = line.trim_start_matches(['`', ' ']);
            let Some(rest) = trimmed.strip_prefix("cargo test -p ") else {
                continue;
            };

            // Split on whitespace, drop trailing backticks from inline code.
            let tokens: Vec<&str> = rest
                .split_whitespace()
                .map(|t| t.trim_end_matches('`'))
                .filter(|t| !t.is_empty())
                .collect();
            if tokens.len() < 2 {
                continue;
            }
            let pkg = tokens[0];
            if pkg != "mini-claw-code" && pkg != "mini-claw-code-starter" {
                continue;
            }

            // Filter tokens are anything before `--` or `#` comment.
            let filters: Vec<&str> = tokens[1..]
                .iter()
                .take_while(|t| **t != "--" && !t.starts_with('#'))
                .filter(|t| !t.starts_with("--"))
                .copied()
                .collect();
            if filters.is_empty() {
                continue;
            }

            for filter in filters {
                checked += 1;
                let out = Command::new("cargo")
                    .args(["test", "-p", pkg, filter, "--", "--list"])
                    .output();
                let out = match out {
                    Ok(o) => o,
                    Err(e) => {
                        eprintln!("Failed to run cargo test: {e}");
                        exit(1);
                    }
                };
                let stdout = String::from_utf8_lossy(&out.stdout);
                // Count lines that look like `<test_name>: test` — the
                // `--list` output for each matched test case.
                let matches = stdout
                    .lines()
                    .filter(|l| l.trim_end().ends_with(": test"))
                    .count();
                let rel = path.strip_prefix(book_src).unwrap_or(&path).display();
                if matches == 0 {
                    failures.push(format!(
                        "{rel}:{}: `cargo test -p {pkg} {filter}` matches 0 tests",
                        lineno + 1
                    ));
                } else {
                    println!(
                        "  ok  {rel}:{}  {pkg} {filter}  ({matches} tests)",
                        lineno + 1
                    );
                }
            }
        }
    }

    if failures.is_empty() {
        println!("\nAll {checked} book test-filter invocation(s) match at least one test.");
    } else {
        eprintln!("\n{} zero-match filter(s) found:", failures.len());
        for f in &failures {
            eprintln!("  {f}");
        }
        eprintln!(
            "\nFix: rename the filter in the book to a real substring, \
             or add the corresponding tests."
        );
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
