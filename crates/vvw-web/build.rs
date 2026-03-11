fn main() {
    // Always rerun so every build gets a fresh timestamp.
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=src/");
    println!("cargo:rerun-if-changed=index.html");

    let output = std::process::Command::new("date")
        .args(["-u", "+%Y-%m-%d %H:%M UTC"])
        .output()
        .expect("failed to run date");
    let datetime = String::from_utf8_lossy(&output.stdout).trim().to_string();
    println!("cargo:rustc-env=VVW_BUILD_DATETIME={datetime}");
}
