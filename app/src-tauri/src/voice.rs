//! Android voice: `VoicePlugin.kt` in `gen/android` hands WebRTC its Java side
//! (JVM, app context) before any audio, asks for the microphone and puts the
//! phone in call mode. Desktop: nothing to do, the OS asks for the microphone.

use tauri::plugin::{Builder, TauriPlugin};
use tauri::{AppHandle, Runtime};

#[cfg(target_os = "android")]
pub struct Handle<R: Runtime>(pub tauri::plugin::PluginHandle<R>);

pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("voice")
        .setup(|_app, _api| {
            #[cfg(target_os = "android")]
            {
                use tauri::Manager;
                let handle =
                    _api.register_android_plugin("io.github.erwanheschung.chaton", "VoicePlugin")?;
                _app.manage(Handle(handle));
            }
            Ok(())
        })
        .build()
}

/// Before joining: may I open the microphone? Refused (or none): I only listen.
pub async fn prepare<R: Runtime>(_app: &AppHandle<R>) -> bool {
    #[cfg(target_os = "android")]
    {
        use tauri::Manager;
        #[derive(serde::Deserialize)]
        struct Prepared {
            granted: bool,
        }
        let plugin = _app.state::<Handle<R>>();
        plugin
            .0
            .run_mobile_plugin_async::<Prepared>("prepare", ())
            .await
            .is_ok_and(|p| p.granted)
    }
    #[cfg(not(target_os = "android"))]
    true
}

/// Out of voice: the phone back to normal audio.
pub async fn release<R: Runtime>(_app: &AppHandle<R>) {
    #[cfg(target_os = "android")]
    {
        use tauri::Manager;
        let plugin = _app.state::<Handle<R>>();
        drop(plugin.0.run_mobile_plugin_async::<()>("release", ()).await);
    }
}

/// `VoicePlugin.initWebrtc`, on a Java thread (WebRTC finds its classes through it).
#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_erwanheschung_chaton_VoicePlugin_initWebrtc<'a>(
    env: jni::JNIEnv<'a>,
    _plugin: jni::objects::JObject<'a>,
    context: jni::objects::JObject<'a>,
) {
    if let Ok(vm) = env.get_java_vm() {
        livekit::webrtc::android::initialize_android_context(&vm, &context);
    }
}
