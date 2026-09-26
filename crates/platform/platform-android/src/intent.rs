//! The address an activity was started at, and what back means when the app has nowhere left to go.

use android_activity::AndroidApp;
use jni::objects::{JObject, JString, JValue};
use jni::{Env, JavaVM, jni_sig, jni_str};
use platform_core::{Location, LocationFormat, LocationSource};

const ACTION_VIEW: &str = "android.intent.action.VIEW";

/// The location in the `ACTION_VIEW` intent the activity was started with: an `https://` app link or a URI with the app's own scheme, read as [`LocationFormat::parse`] reads one.
///
/// Android keeps no history of its own for an activity's pages, so the moves the app makes go nowhere; back reaches the app through `EventHandler::on_back` instead. A link opened while the app is already running is not seen: `NativeActivity` does not forward `onNewIntent`, so it opens only if the system starts the activity afresh.
pub struct IntentLocation {
    app: AndroidApp,
}

impl IntentLocation {
    pub fn new(app: AndroidApp) -> Self {
        Self { app }
    }
}

impl LocationSource for IntentLocation {
    fn initial(&mut self) -> Vec<Location> {
        let uri = view_uri(&self.app).unwrap_or_else(|error| {
            tracing::debug!(%error, "the starting intent is unreachable over JNI");
            None
        });
        uri.and_then(|uri| LocationFormat::root().parse(&uri))
            .into_iter()
            .collect()
    }

    fn push(&mut self, _history: &[Location]) {}

    fn replace(&mut self, _history: &[Location]) {}

    fn back(&mut self, _count: usize, _history: &[Location]) {}
}

/// Sends the task to the background, which is what back does for a root activity since Android 12. Finishing it instead would tear down a `NativeActivity` whose process lives on to start it again.
pub fn leave(app: &AndroidApp) {
    let moved = with_activity(app, |env, activity| {
        env.call_method(
            activity,
            jni_str!("moveTaskToBack"),
            jni_sig!("(Z)Z"),
            &[JValue::Bool(true)],
        )?
        .z()
    });
    if let Err(error) = moved {
        tracing::warn!(%error, "could not send the task to the background");
    }
}

/// Opens URIs with an `ACTION_VIEW` intent, as any Android app opens a link: the system picks the browser, the mail app, or the app that claimed the link.
pub struct AndroidUriOpener {
    app: AndroidApp,
}

impl AndroidUriOpener {
    pub fn new(app: AndroidApp) -> Self {
        Self { app }
    }
}

impl services_core::UriOpener for AndroidUriOpener {
    fn open(&self, uri: &str) -> bool {
        start_view(&self.app, uri)
            .inspect_err(|error| tracing::warn!(uri, %error, "no activity opened the URI"))
            .is_ok()
    }
}

fn start_view(app: &AndroidApp, uri: &str) -> jni::errors::Result<()> {
    with_activity(app, |env, activity| {
        let text = env.new_string(uri)?;
        let parsed = env
            .call_static_method(
                jni_str!("android/net/Uri"),
                jni_str!("parse"),
                jni_sig!("(Ljava/lang/String;)Landroid/net/Uri;"),
                &[JValue::Object(&text)],
            )?
            .l()?;
        let action = env.new_string(ACTION_VIEW)?;
        let intent = env.new_object(
            jni_str!("android/content/Intent"),
            jni_sig!("(Ljava/lang/String;Landroid/net/Uri;)V"),
            &[JValue::Object(&action), JValue::Object(&parsed)],
        )?;
        env.call_method(
            activity,
            jni_str!("startActivity"),
            jni_sig!("(Landroid/content/Intent;)V"),
            &[JValue::Object(&intent)],
        )?;
        Ok(())
    })
}

fn view_uri(app: &AndroidApp) -> jni::errors::Result<Option<String>> {
    with_activity(app, |env, activity| {
        let intent = env
            .call_method(
                activity,
                jni_str!("getIntent"),
                jni_sig!("()Landroid/content/Intent;"),
                &[],
            )?
            .l()?;
        if intent.is_null() {
            return Ok(None);
        }
        let action = env
            .call_method(
                &intent,
                jni_str!("getAction"),
                jni_sig!("()Ljava/lang/String;"),
                &[],
            )?
            .l()?;
        let action = string_of(env, action)?;
        if action.as_deref() != Some(ACTION_VIEW) {
            return Ok(None);
        }
        let data = env
            .call_method(
                &intent,
                jni_str!("getDataString"),
                jni_sig!("()Ljava/lang/String;"),
                &[],
            )?
            .l()?;
        string_of(env, data)
    })
}

fn string_of(env: &mut Env, object: JObject) -> jni::errors::Result<Option<String>> {
    if object.is_null() {
        return Ok(None);
    }
    let string = env.cast_local::<JString>(object)?;
    Ok(Some(string.try_to_string(env)?))
}

fn with_activity<T>(
    app: &AndroidApp,
    f: impl FnOnce(&mut Env, &JObject) -> jni::errors::Result<T>,
) -> jni::errors::Result<T> {
    let vm = unsafe { JavaVM::from_raw(app.vm_as_ptr().cast()) };
    let activity = app.activity_as_ptr();
    vm.attach_current_thread(|env| {
        let activity = unsafe { JObject::from_raw(env, activity.cast()) };
        f(env, &activity)
    })
}
