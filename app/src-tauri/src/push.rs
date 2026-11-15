//! Android push (UnifiedPush): `PushPlugin.kt` in `gen/android` gets this
//! device's WebPush endpoint, core registers it on my instance. Nothing on
//! desktop, where the running app notifies by itself.

use tauri::Runtime;
use tauri::plugin::{Builder, TauriPlugin};

#[cfg(target_os = "android")]
pub struct Handle<R: Runtime>(pub tauri::plugin::PluginHandle<R>);

pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("push")
        .setup(|_app, _api| {
            #[cfg(target_os = "android")]
            {
                use tauri::Manager;
                let handle =
                    _api.register_android_plugin("io.github.erwanheschung.chaton", "PushPlugin")?;
                _app.manage(Handle(handle));
            }
            Ok(())
        })
        .build()
}
