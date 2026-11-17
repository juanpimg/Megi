use std::process::Command;

fn main() {
    // Capture the compiler banner at build time so the reported version always
    // matches the toolchain that built this binary (#1210).
    let rustc = Command::new("rustc")
        .arg("--version")
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| String::from("rustc unknown"));
    println!("cargo:rustc-env=BENCH_RUSTC_VERSION={rustc}");
}
