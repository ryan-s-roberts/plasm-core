fn main() {
    let target = std::env::var("TARGET").expect("TARGET must be set by Cargo for build scripts");
    println!("cargo:rustc-env=PLASM_HOST_TARGET_TRIPLE={target}");
}
