//! Output helpers: human text by default, single JSON object with `--json`.

use serde::Serialize;

pub fn print_json<T: Serialize>(v: &T) {
    println!(
        "{}",
        serde_json::to_string_pretty(v).unwrap_or_else(|_| "{}".to_string())
    );
}

/// CLI usage/validation failure: exit 1, plain message on stderr.
pub fn usage(msg: &str) -> ! {
    eprintln!("error: {msg}");
    std::process::exit(1);
}

/// Runtime failure: JSON error object on stderr, exit 2.
pub fn fail(err: String, hint: &str) -> ! {
    let v = serde_json::json!({ "error": err, "hint": hint });
    eprintln!("{v}");
    std::process::exit(2);
}

pub fn docker_hint() -> &'static str {
    "is Docker running? start Docker Desktop and retry"
}
