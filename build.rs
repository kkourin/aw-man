extern crate metadeps;

fn main() {
    metadeps::probe().unwrap();
    #[cfg(target_env = "msvc")]
    {
        static MANIFEST: &str = "windows-manifest.xml";

        let mut manifest = std::env::current_dir().unwrap();
        manifest.push(MANIFEST);

        println!("cargo:rerun-if-changed={}", MANIFEST);
        println!("cargo:rustc-link-arg-bin=aw-man=/MANIFEST:EMBED");
        println!("cargo:rustc-link-arg-bin=aw-man=/MANIFESTINPUT:{}", manifest.to_str().unwrap());
        // Turn linker warnings into errors.
        // Don't treat warnings as errors for this specific warning
        println!("cargo:rustc-link-arg=/NODEFAULTLIB:libcmt.lib");

        embed_resource::compile("resources.rc", embed_resource::NONE);
    }

    cynic_codegen::register_schema("suwayomi")
        .from_sdl_file("schemas/suwayomi.graphql")
        .unwrap()
        .as_default()
        .unwrap();

    if std::env::var("CARGO_CFG_TARGET_OS").unwrap() == "macos" {
        // The path to Homebrew libraries on Apple Silicon
        let library_path = "/opt/homebrew/lib";

        // 1. Link Search Path: Tells rustc where to find .dylib files at *compile* time
        println!("cargo:rustc-link-search=native={}", library_path);

        // 2. RPATH: Tells the linker to embed this path in the binary for *run* time
        println!("cargo:rustc-link-arg=-Wl,-rpath,{}", library_path);
    }
}
