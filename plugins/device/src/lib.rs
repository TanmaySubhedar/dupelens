//! The Android-only system calls of DupeLens (docs/decisions/0016-android-app.md).
//!
//! Android hides some things from ordinary file calls; they go through Kotlin, in this
//! crate's `android/` folder. So far:
//! - whether the app may read all files on the phone, and opening Android's settings page
//!   where the person turns that on;
//! - fitting the page between the status bar and the navigation bar.
//!
//! On every other system the calls answer "not supported" (and the page never asks).

use serde::Serialize;
use tauri::{
    plugin::{Builder, PluginApi, TauriPlugin},
    AppHandle, Manager, Runtime,
};

/// Whether this app can read the phone's shared storage.
#[derive(Debug, Clone, Serialize, serde::Deserialize)]
pub struct StorageStatus {
    /// `false` everywhere except Android.
    pub supported: bool,
    /// On Android: "All files access" (or, before Android 11, the storage permission).
    pub granted: bool,
}

/// The calls into the Kotlin plugin.
pub struct Device<R: Runtime> {
    #[cfg(target_os = "android")]
    handle: tauri::plugin::PluginHandle<R>,
    #[cfg(not(target_os = "android"))]
    _runtime: std::marker::PhantomData<fn() -> R>,
}

impl<R: Runtime> Device<R> {
    /// Does the app have access to the phone's files right now?
    pub fn storage_status(&self) -> Result<StorageStatus, String> {
        #[cfg(target_os = "android")]
        {
            self.handle
                .run_mobile_plugin("storageStatus", serde_json::json!({}))
                .map_err(|e| e.to_string())
        }
        #[cfg(not(target_os = "android"))]
        {
            Ok(StorageStatus {
                supported: false,
                granted: true,
            })
        }
    }

    /// Opens Android's settings page where the person allows access to all files.
    pub fn open_storage_settings(&self) -> Result<(), String> {
        #[cfg(target_os = "android")]
        {
            self.handle
                .run_mobile_plugin::<serde_json::Value>("openStorageSettings", serde_json::json!({}))
                .map(|_| ())
                .map_err(|e| e.to_string())
        }
        #[cfg(not(target_os = "android"))]
        {
            Err("Only Android has a storage permission page.".into())
        }
    }
}

/// `app.device()` from anywhere that has the app.
pub trait DeviceExt<R: Runtime> {
    fn device(&self) -> &Device<R>;
}

impl<R: Runtime, T: Manager<R>> DeviceExt<R> for T {
    fn device(&self) -> &Device<R> {
        self.state::<Device<R>>().inner()
    }
}

fn open<R: Runtime, C: serde::de::DeserializeOwned>(
    _app: &AppHandle<R>,
    api: PluginApi<R, C>,
) -> Result<Device<R>, Box<dyn std::error::Error>> {
    #[cfg(target_os = "android")]
    {
        let handle = api.register_android_plugin("org.dupelens.device", "DevicePlugin")?;
        Ok(Device { handle })
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = api;
        Ok(Device {
            _runtime: std::marker::PhantomData,
        })
    }
}

pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("dupelens-device")
        .setup(|app, api| {
            app.manage(open(app, api)?);
            Ok(())
        })
        .build()
}
