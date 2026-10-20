fn main() {
    // The updater public key is compiled in (`option_env!` in src/commands/updates.rs).
    println!("cargo:rerun-if-env-changed=FORGE_UPDATER_PUBKEY");
    tauri_build::build()
}
