use std::path::Path;

fn main() {
    let build_dir = Path::new("svelte/build");
    if !build_dir.join("index.html").exists() {
        println!(
            "cargo:error=frontend build missing at {}: run `npm run build` in web/svelte before building",
            build_dir.display()
        );
        std::process::exit(1);
    }
    track_files(build_dir);
}

fn track_files(dir: &Path) {
    for entry in std::fs::read_dir(dir).expect("failed to read frontend build dir") {
        let path = entry.expect("failed to read build dir entry").path();
        if path.is_dir() {
            track_files(&path);
        } else {
            println!("cargo:rerun-if-changed={}", path.display());
        }
    }
}
