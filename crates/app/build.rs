fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }

    println!("cargo:rerun-if-changed=app.rc");
    println!("cargo:rerun-if-changed=assets/icon/castle.ico");

    if let Err(err) = embed_resource::compile("app.rc", embed_resource::NONE).manifest_required() {
        panic!("failed to embed Castle's Windows icon: {err}");
    }
}
