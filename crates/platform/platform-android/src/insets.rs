//! The part of the window Android keeps for itself — status and navigation bars, a display cutout — read from the decor view's root `WindowInsets` over JNI.

use android_activity::AndroidApp;
use geometry_core::Insets;
use jni::objects::JObject;
use jni::{Env, JavaVM, jni_sig, jni_str};

/// The window's safe area in logical units, `scale_factor` physical pixels to one. `None` while the view has no insets to give, before it is attached to its window.
pub fn read(app: &AndroidApp, scale_factor: f64) -> Option<Insets> {
    let [top, right, bottom, left] = physical(app)
        .inspect_err(|error| tracing::debug!(%error, "window insets unreachable over JNI"))
        .ok()??;
    let logical = |px: i32| (px.max(0) as f64 / scale_factor.max(f64::EPSILON)) as f32;
    Some(Insets::new(
        logical(top),
        logical(right),
        logical(bottom),
        logical(left),
    ))
}

fn physical(app: &AndroidApp) -> jni::errors::Result<Option<[i32; 4]>> {
    let vm = unsafe { JavaVM::from_raw(app.vm_as_ptr().cast()) };
    let activity = app.activity_as_ptr();
    vm.attach_current_thread(|env| {
        let activity = unsafe { JObject::from_raw(env, activity.cast()) };
        let window = env
            .call_method(
                &activity,
                jni_str!("getWindow"),
                jni_sig!("()Landroid/view/Window;"),
                &[],
            )?
            .l()?;
        let decor = env
            .call_method(
                &window,
                jni_str!("getDecorView"),
                jni_sig!("()Landroid/view/View;"),
                &[],
            )?
            .l()?;
        let insets = env
            .call_method(
                &decor,
                jni_str!("getRootWindowInsets"),
                jni_sig!("()Landroid/view/WindowInsets;"),
                &[],
            )?
            .l()?;
        if insets.is_null() {
            return Ok(None);
        }
        let sdk = env
            .get_static_field(
                jni_str!("android/os/Build$VERSION"),
                jni_str!("SDK_INT"),
                jni_sig!("I"),
            )?
            .i()?;
        if sdk >= 30 {
            bars_and_cutout(env, &insets).map(Some)
        } else {
            system_window(env, &insets).map(Some)
        }
    })
}

// `WindowInsets.getInsets(int)` is API 30: the bars and the cutout together, which is everything a layout has to keep clear of.
fn bars_and_cutout(env: &mut Env, insets: &JObject) -> jni::errors::Result<[i32; 4]> {
    let kind = |env: &mut Env, name| -> jni::errors::Result<i32> {
        env.call_static_method(
            jni_str!("android/view/WindowInsets$Type"),
            name,
            jni_sig!("()I"),
            &[],
        )?
        .i()
    };
    let types = kind(env, jni_str!("systemBars"))? | kind(env, jni_str!("displayCutout"))?;
    let edges = env
        .call_method(
            insets,
            jni_str!("getInsets"),
            jni_sig!("(I)Landroid/graphics/Insets;"),
            &[jni::objects::JValue::Int(types)],
        )?
        .l()?;
    let side = |env: &mut Env, name| -> jni::errors::Result<i32> {
        env.get_field(&edges, name, jni_sig!("I"))?.i()
    };
    Ok([
        side(env, jni_str!("top"))?,
        side(env, jni_str!("right"))?,
        side(env, jni_str!("bottom"))?,
        side(env, jni_str!("left"))?,
    ])
}

// Below API 30 the system-window insets are the bars; a cutout is folded into them where the device reports one.
fn system_window(env: &mut Env, insets: &JObject) -> jni::errors::Result<[i32; 4]> {
    let side = |env: &mut Env, name| -> jni::errors::Result<i32> {
        env.call_method(insets, name, jni_sig!("()I"), &[])?.i()
    };
    Ok([
        side(env, jni_str!("getSystemWindowInsetTop"))?,
        side(env, jni_str!("getSystemWindowInsetRight"))?,
        side(env, jni_str!("getSystemWindowInsetBottom"))?,
        side(env, jni_str!("getSystemWindowInsetLeft"))?,
    ])
}
