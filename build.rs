use std::env;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=assets/welly-rs-icon.svg");
    println!("cargo:rerun-if-changed=scripts/make-app-icons.py");

    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"));
    let generator = manifest_dir.join("scripts").join("make-app-icons.py");
    let iconset_dir = out_dir.join("welly-rs.iconset");
    let app_icon_rgba = out_dir.join("welly-rs-app-icon.rgba");
    let python = find_python();

    run_icon_generator(&python, &generator, &iconset_dir, &app_icon_rgba);

    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }

    let icon_path = out_dir.join("welly-rs.ico");

    run_icon_generator(&python, &generator, &iconset_dir, &icon_path);

    winresource::WindowsResource::new()
        .set_icon(icon_path.to_str().expect("icon path is not valid UTF-8"))
        .compile()
        .expect("failed to compile Windows resources");
}

fn run_icon_generator(
    python: &str,
    generator: &PathBuf,
    iconset_dir: &PathBuf,
    output_path: &PathBuf,
) {
    let status = Command::new(python)
        .arg(generator)
        .arg(iconset_dir)
        .arg(output_path)
        .status()
        .unwrap_or_else(|error| panic!("failed to run {python}: {error}"));
    assert!(status.success(), "{python} icon generator failed");
}

fn find_python() -> String {
    if let Ok(python) = env::var("PYTHON") {
        return python;
    }

    for candidate in ["python3", "python"] {
        if Command::new(candidate)
            .arg("--version")
            .status()
            .is_ok_and(|status| status.success())
        {
            return candidate.to_owned();
        }
    }

    "python3".to_owned()
}
