//! What a screen reader on Android reads: the window's accessibility snapshot, published on the activity's decor view as virtual views, and what a reader asks of them, carried back to the thread the UI lives on.
//!
//! The tree is the one the desktop publishes, mapped by `platform-accesskit`; this is only the Android end of it — finding the view to hang it on, and crossing from Android's UI thread, where a reader's requests arrive, to the activity's main thread, where the UI answers them.

use accesskit_android::InjectingAdapter;
use accesskit_android::jni::objects::JObject;
use accesskit_android::jni::{JNIEnv, JavaVM};
use android_activity::AndroidApp;
use platform_accesskit::accesskit::{ActionHandler, ActionRequest, ActivationHandler, TreeUpdate};
use platform_core::{AccessNode, EventHandler, Window};
use winit::event_loop::EventLoopProxy;

use crate::platform::UserEvent;
use crate::stepping::{as_step, offer_steps_as_scrolls};

/// What a reader asked for, on its way from Android's UI thread to the one the UI lives on.
pub(crate) enum AccessRequest {
    /// Something started reading the window and needs the whole tree.
    InitialTree,
    /// A reader asked a node to do something.
    Action(ActionRequest),
}

/// The adapter on the activity's view, and what the runner last told it.
pub(crate) struct Accessibility {
    vm: JavaVM,
    adapter: InjectingAdapter,
    // Android never says a reader stopped listening, so this only ever turns on, and until it does no snapshot is built.
    asked: bool,
    nodes: Vec<AccessNode>,
}

/// Room for the local references one call into the adapter makes, which a frame of this size frees when the call returns.
const LOCAL_REFERENCES: i32 = 16;

impl Accessibility {
    /// Hangs the adapter on the activity's decor view, routing what a reader asks through `proxy`. `None` on a device too old to load the adapter's view delegate, or where the activity cannot be reached.
    pub(crate) fn attach(app: &AndroidApp, proxy: EventLoopProxy<UserEvent>) -> Option<Self> {
        if !loads_in_memory_dex() {
            tracing::warn!(
                "the device predates API 26 or names no level, so no screen reader can read this window"
            );
            return None;
        }
        let vm = unsafe { JavaVM::from_raw(app.vm_as_ptr().cast()) }
            .inspect_err(|error| tracing::warn!(%error, "the Java VM is unreachable"))
            .ok()?;
        let adapter = vm
            .get_env()
            .and_then(|mut env| {
                env.with_local_frame(LOCAL_REFERENCES, |env| {
                    let activity = unsafe { JObject::from_raw(app.activity_as_ptr().cast()) };
                    let host = decor_view(env, &activity)?;
                    Ok(InjectingAdapter::new(
                        env,
                        &host,
                        Forward(proxy.clone()),
                        Forward(proxy),
                    ))
                })
            })
            .inspect_err(|error| tracing::warn!(%error, "the activity's decor view is unreachable"))
            .ok()?;
        Some(Self {
            vm,
            adapter,
            asked: false,
            nodes: Vec::new(),
        })
    }

    /// Answers what a reader asked: the tree, or an action taken through the same door the keyboard and a tap use, and then the tree as it now stands.
    pub(crate) fn answer<W: Window>(
        &mut self,
        request: AccessRequest,
        handler: &mut impl EventHandler<W>,
        title: &str,
        scale_factor: f64,
    ) {
        match request {
            AccessRequest::InitialTree => self.asked = true,
            AccessRequest::Action(request) => {
                let Some((id, action)) =
                    platform_accesskit::requested_action(&as_step(request), &self.nodes)
                else {
                    return;
                };
                handler.on_accessibility_action(id, action);
            }
        }
        self.publish(handler, title, scale_factor);
    }

    /// Hands the window's tree over once a reader has asked for it, so an app nobody reads builds no snapshot at all.
    pub(crate) fn publish<W: Window>(
        &mut self,
        handler: &impl EventHandler<W>,
        title: &str,
        scale_factor: f64,
    ) {
        if !self.asked {
            return;
        }
        let nodes = handler.accessibility();
        let lang = handler.root_language();
        let adapter = &mut self.adapter;
        // The main thread never returns to Java, so the references the adapter makes on every update are freed only by a frame popped here.
        let published = self.vm.get_env().and_then(|mut env| {
            env.with_local_frame(LOCAL_REFERENCES, |_| {
                adapter.update_if_active(|| {
                    let mut update =
                        platform_accesskit::tree_update(&nodes, title, lang.as_ref(), scale_factor);
                    offer_steps_as_scrolls(&mut update);
                    update
                });
                Ok::<_, accesskit_android::jni::errors::Error>(())
            })
        });
        if let Err(error) = published {
            tracing::warn!(%error, "the tree could not be published");
        }
        self.nodes = nodes;
    }
}

// The decor view rather than the content view: it starts where the window's surface does, and the bounds the snapshot gives are surface-relative, while the content view sits below whatever system bars the window fits.
fn decor_view<'local>(
    env: &mut JNIEnv<'local>,
    activity: &JObject,
) -> accesskit_android::jni::errors::Result<JObject<'local>> {
    let window = env
        .call_method(activity, "getWindow", "()Landroid/view/Window;", &[])?
        .l()?;
    env.call_method(&window, "getDecorView", "()Landroid/view/View;", &[])?
        .l()
}

// The adapter loads its view delegate through `InMemoryDexClassLoader`, which API 26 introduced, and aborts the process where the class is missing.
fn loads_in_memory_dex() -> bool {
    crate::sys_prop::read_sys_prop("ro.build.version.sdk")
        .and_then(|level| level.parse::<u32>().ok())
        .is_some_and(|level| level >= 26)
}

/// Hands what a reader asks, on Android's UI thread, to the event loop, since the UI that answers is not `Send`.
struct Forward(EventLoopProxy<UserEvent>);

impl ActivationHandler for Forward {
    // `None` leaves the adapter a placeholder until the loop publishes, which it does as soon as this request reaches it.
    fn request_initial_tree(&mut self) -> Option<TreeUpdate> {
        let _ = self
            .0
            .send_event(UserEvent::Accessibility(AccessRequest::InitialTree));
        None
    }
}

impl ActionHandler for Forward {
    fn do_action(&mut self, request: ActionRequest) {
        let _ = self
            .0
            .send_event(UserEvent::Accessibility(AccessRequest::Action(request)));
    }
}
