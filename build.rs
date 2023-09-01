#[cfg(not(target_env = "msvc"))]
fn try_vcpkg(_statik: bool) -> Option<Vec<PathBuf>> {
    None
}

#[cfg(target_env = "msvc")]
fn try_vcpkg(statik: bool) -> Option<Vec<std::path::PathBuf>> {
    if !statik {
        std::env::set_var("VCPKGRS_DYNAMIC", "1");
    }

    vcpkg::find_package("ffmpeg")
        .map_err(|e| {
            println!("Could not find ffmpeg with vcpkg: {}", e);
        })
        .map(|library| library.include_paths)
        .ok()
}

fn main() {
    let ffmpeg_libs = ["libavformat", "libavfilter", "libswscale", "libswresample"];
    let include_paths = if let Some(include_paths) = try_vcpkg(false) {
        include_paths
    } else {
        for lib_name in ffmpeg_libs {
            pkg_config::Config::new()
                .statik(false)
                .probe(lib_name)
                .unwrap();
        }
        pkg_config::Config::new()
            .statik(true)
            .probe("libavutil")
            .unwrap()
            .include_paths
    };

    println!("cargo:rerun-if-changed=ffplay/ffplay.c");
    let mut builder = cc::Build::new();
    builder.includes(include_paths);
    builder.file("ffplay/ffplay.c").compile("ffplay");
}
