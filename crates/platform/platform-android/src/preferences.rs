//! Android's [`SystemPreferences`]: night mode from the configuration, the rest from the settings providers and `LocaleList` over JNI.

use android_activity::AndroidApp;
use android_activity::ndk::configuration::{Configuration, UiModeNight};
use jni::objects::{JObject, JString, JValue};
use jni::{Env, JavaVM, jni_sig, jni_str};
use platform_core::{ColorScheme, SystemPreferences};

use crate::preference_values::{
    animator_scale_reduces_motion, configuration_locale, split_language_tags,
};

/// Everything the device answers right now.
///
/// The configuration is read from the **asset manager**, not from `AndroidApp::config()`. That cached `ConfigurationRef` is only refreshed when android-activity receives NativeActivity's `onConfigurationChanged`, and that callback does not always arrive: on a Redmi/HyperOS device the system applied `night` to the activity while the cached config stayed on its launch value for the life of the process. The asset manager tracks the change either way.
pub fn read(app: &AndroidApp) -> SystemPreferences {
    let configuration = Configuration::from_asset_manager(&app.asset_manager());
    let color_scheme = match configuration.ui_mode_night() {
        UiModeNight::Yes => Some(ColorScheme::Dark),
        UiModeNight::No => Some(ColorScheme::Light),
        _ => None,
    };
    let java = from_java(app).unwrap_or_else(|error| {
        tracing::debug!(%error, "system preferences unreachable over JNI");
        FromJava::default()
    });
    let locales = if java.locales.is_empty() {
        configuration_locale(
            configuration.language().as_deref(),
            configuration.country().as_deref(),
        )
        .into_iter()
        .collect()
    } else {
        java.locales
    };
    SystemPreferences {
        color_scheme,
        reduced_motion: java.reduced_motion,
        high_contrast: java.high_contrast,
        locales,
    }
}

#[derive(Default)]
struct FromJava {
    reduced_motion: Option<bool>,
    high_contrast: Option<bool>,
    locales: Vec<String>,
}

fn from_java(app: &AndroidApp) -> jni::errors::Result<FromJava> {
    let vm = unsafe { JavaVM::from_raw(app.vm_as_ptr().cast()) };
    let activity = app.activity_as_ptr();
    vm.attach_current_thread(|env| {
        let activity = unsafe { JObject::from_raw(env, activity.cast()) };
        let resolver = env
            .call_method(
                &activity,
                jni_str!("getContentResolver"),
                jni_sig!("()Landroid/content/ContentResolver;"),
                &[],
            )?
            .l()?;
        Ok(FromJava {
            reduced_motion: animator_duration_scale(env, &resolver)
                .ok()
                .map(animator_scale_reduces_motion),
            high_contrast: high_text_contrast(env, &resolver).ok(),
            locales: locale_list(env, &activity).unwrap_or_default(),
        })
    })
}

fn animator_duration_scale(env: &mut Env, resolver: &JObject) -> jni::errors::Result<f32> {
    let name = env.new_string("animator_duration_scale")?;
    env.call_static_method(
        jni_str!("android/provider/Settings$Global"),
        jni_str!("getFloat"),
        jni_sig!("(Landroid/content/ContentResolver;Ljava/lang/String;F)F"),
        &[
            JValue::Object(resolver),
            JValue::Object(&name),
            JValue::Float(1.0),
        ],
    )?
    .f()
}

// `high_text_contrast_enabled` is not in the public SDK, but `Settings.Secure` reads any key by name, and it is the switch the accessibility settings flip.
fn high_text_contrast(env: &mut Env, resolver: &JObject) -> jni::errors::Result<bool> {
    let name = env.new_string("high_text_contrast_enabled")?;
    let value = env
        .call_static_method(
            jni_str!("android/provider/Settings$Secure"),
            jni_str!("getInt"),
            jni_sig!("(Landroid/content/ContentResolver;Ljava/lang/String;I)I"),
            &[
                JValue::Object(resolver),
                JValue::Object(&name),
                JValue::Int(0),
            ],
        )?
        .i()?;
    Ok(value == 1)
}

// `Configuration.getLocales()` is API 24; below that the call fails and the caller falls back to the configuration's single locale.
fn locale_list(env: &mut Env, activity: &JObject) -> jni::errors::Result<Vec<String>> {
    let resources = env
        .call_method(
            activity,
            jni_str!("getResources"),
            jni_sig!("()Landroid/content/res/Resources;"),
            &[],
        )?
        .l()?;
    let configuration = env
        .call_method(
            &resources,
            jni_str!("getConfiguration"),
            jni_sig!("()Landroid/content/res/Configuration;"),
            &[],
        )?
        .l()?;
    let locales = env
        .call_method(
            &configuration,
            jni_str!("getLocales"),
            jni_sig!("()Landroid/os/LocaleList;"),
            &[],
        )?
        .l()?;
    let tags = env
        .call_method(
            &locales,
            jni_str!("toLanguageTags"),
            jni_sig!("()Ljava/lang/String;"),
            &[],
        )?
        .l()?;
    let tags = env.cast_local::<JString>(tags)?;
    Ok(split_language_tags(&tags.try_to_string(env)?))
}
