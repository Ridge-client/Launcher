fn main() {
    // the exe's own icon on Windows (assets/ridge.ico, via assets/ridge.rc)
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        println!("cargo:rerun-if-changed=assets/ridge.rc");
        println!("cargo:rerun-if-changed=assets/ridge.ico");
        embed_resource::compile("assets/ridge.rc", embed_resource::NONE)
            .manifest_optional()
            .expect("compiling the Windows icon resource");
    }
}
