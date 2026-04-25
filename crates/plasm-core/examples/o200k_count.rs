//! `cargo run -p plasm-core --example o200k_count -- path/to/utf-8.txt`
//! Prints one line: `chars=N o200k=M` using the same encoder as `PromptSurfaceStats`.

use std::env;
use std::fs;

fn main() {
    let path = env::args().nth(1).expect("usage: o200k_count <utf-8 file>");
    let s = fs::read_to_string(&path).expect("read file as UTF-8");
    let chars = s.chars().count();
    let o200k = plasm_core::o200k_token_count(&s);
    println!("chars={chars} o200k={o200k}");
}
