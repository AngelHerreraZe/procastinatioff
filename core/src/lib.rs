// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! ProcastinatiOFF core: day rules, signed events, crypto,
//! encrypted storage and direct sync between devices. No platform dependencies.

uniffi::setup_scaffolding!();

pub mod api;
pub mod auth;
pub mod backup;
pub mod clock;
pub mod crypto;
pub mod desktop;
pub mod error;
pub mod events;
pub mod goals;
pub mod ledger;
pub mod model;
pub mod netpolicy;
pub mod offline_code;
pub mod pairing;
pub mod peer;
pub mod rules;
pub mod storage;
pub mod strictness;
pub mod time;

/// Android: gives iroh the JVM and the application context (it reads the DNS servers and the network
/// interfaces through JNI). Without it the first network use (pairing, sync) aborts the process.
/// Called by `CoreHolder.initAndroid` (Kotlin) before opening any core; later calls do nothing.
///
/// # Safety
/// Only the JVM calls it, with a valid `env` and `context`.
#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
pub unsafe extern "system" fn Java_dev_procastinatioff_app_core_CoreHolder_initAndroid(
    env: *mut jni_sys::JNIEnv,
    _this: jni_sys::jobject,
    context: jni_sys::jobject,
) {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| unsafe {
        let jni = &(**env).v1_1;
        let mut vm = std::ptr::null_mut();
        if (jni.GetJavaVM)(env, &mut vm) != jni_sys::JNI_OK {
            return;
        }
        // A global reference: the context must stay valid until the process exits.
        let context = (jni.NewGlobalRef)(env, context);
        iroh::dns::install_android_jni_context(vm.cast(), context.cast());
    });
}
