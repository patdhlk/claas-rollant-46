// SPDX-License-Identifier: GPL-3.0-only
//! Compile `ui/baler.slint` for the software renderer, embedding glyphs so the
//! binary needs no system fonts at runtime. Only under the `device` feature;
//! the host build neither needs nor builds `slint-build`.

fn main() {
    println!("cargo:rerun-if-changed=ui/baler.slint");
    #[cfg(feature = "device")]
    compile_ui();
}

#[cfg(feature = "device")]
fn compile_ui() {
    slint_build::compile_with_config(
        "ui/baler.slint",
        slint_build::CompilerConfiguration::new()
            .embed_resources(slint_build::EmbedResourcesKind::EmbedForSoftwareRenderer),
    )
    .expect("compile ui/baler.slint");
}
