fn main() {
    // webrtc-sys's own linker flags stop at its crate: the library Android loads is
    // ours, so it must export WebRTC's JNI functions (`Java_livekit_org_webrtc_*`).
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("android") {
        webrtc_sys_build::configure_jni_symbols().expect("WebRTC JNI symbols");
    }
    tauri_build::build()
}
