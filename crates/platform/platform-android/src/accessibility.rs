//! What a screen reader on Android reads: the window's accessibility snapshot, published on the activity's decor view as virtual views, and what a reader asks of them, carried back to the thread the UI lives on.
//!
//! The tree is the one the desktop publishes, mapped by `platform-accesskit`; this is only the Android end of it — finding the view to hang it on, and crossing from Android's UI thread, where a reader's requests arrive, to the activity's main thread, where the UI answers them.

use std::sync::mpsc::{SendError, SyncSender, sync_channel};
use std::time::Duration;

use accesskit_android::InjectingAdapter;
use accesskit_android::jni::objects::JObject;
use accesskit_android::jni::{JNIEnv, JavaVM};
use android_activity::AndroidApp;
use platform_accesskit::accesskit::{ActionHandler, ActionRequest, ActivationHandler, TreeUpdate};
use platform_accesskit::{PublishedTree, WindowFrame};
use platform_core::{EventHandler, Window};
use winit::event_loop::EventLoopProxy;

use crate::platform::UserEvent;
use crate::stepping::{as_step, offer_steps_as_scrolls};

/// What a reader asked for, on its way from Android's UI thread to the one the UI lives on.
pub(crate) enum AccessRequest {
    /// Something started reading the window and waits, on Android's UI thread, for the whole tree.
    InitialTree(SyncSender<TreeUpdate>),
    /// A reader asked a node to do something.
    Action(ActionRequest),
}

/// The adapter on the activity's view, and what the runner last told it.
pub(crate) struct Accessibility {
    vm: JavaVM,
    adapter: InjectingAdapter,
    // Android never says a reader stopped listening, so this only ever turns on, and until it does no snapshot is built.
    asked: bool,
    tree: PublishedTree,
}

/// Room for the local references one call into the adapter makes, which a frame of this size frees when the call returns.
const LOCAL_REFERENCES: i32 = 16;

/// How long a reader asking for the tree waits on Android's UI thread for the loop to build it, before it is given a placeholder the loop fills in when it gets there.
const INITIAL_TREE_WAIT: Duration = Duration::from_millis(500);

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
            tree: PublishedTree::default(),
        })
    }

    /// Answers what a reader asked: the tree, handed to the reader waiting for it, or an action taken through the same door the keyboard and a tap use, and then the tree as it now stands.
    pub(crate) fn answer<W: Window>(
        &mut self,
        request: AccessRequest,
        handler: &mut impl EventHandler<W>,
        window: WindowFrame,
    ) {
        match request {
            AccessRequest::InitialTree(reader) => {
                self.asked = true;
                self.hand_over(handler, window, |adapter, build| {
                    // The reader stopped waiting and was given a placeholder, which the adapter replaces with the tree it is handed instead.
                    if let Err(SendError(update)) = reader.send(build()) {
                        adapter.update_if_active(|| update);
                    }
                });
            }
            AccessRequest::Action(request) => {
                let Some((id, action)) = self.tree.requested_action(&as_step(request)) else {
                    return;
                };
                handler.on_accessibility_action(id, action);
                self.publish(handler, window);
            }
        }
    }

    /// Hands the window's tree over once a reader has asked for it, so an app nobody reads builds no snapshot at all.
    pub(crate) fn publish<W: Window>(
        &mut self,
        handler: &impl EventHandler<W>,
        window: WindowFrame,
    ) {
        if !self.asked {
            return;
        }
        self.hand_over(handler, window, |adapter, build| {
            adapter.update_if_active(build)
        });
    }

    /// Builds the window's tree and gives `deliver` the adapter and the means of building it.
    fn hand_over<W: Window>(
        &mut self,
        handler: &impl EventHandler<W>,
        window: WindowFrame,
        deliver: impl for<'b> FnOnce(&mut InjectingAdapter, Box<dyn FnOnce() -> TreeUpdate + 'b>),
    ) {
        let adapter = &mut self.adapter;
        let tree = &mut self.tree;
        // The main thread never returns to Java, so the references the adapter makes on every update are freed only by a frame popped here.
        let published = self.vm.get_env().and_then(|mut env| {
            env.with_local_frame(LOCAL_REFERENCES, |_| {
                tree.publish(
                    || (handler.accessibility(), handler.root_language()),
                    window,
                    |build| {
                        deliver(
                            adapter,
                            Box::new(move || {
                                let mut update = build();
                                offer_steps_as_scrolls(&mut update);
                                update
                            }),
                        )
                    },
                );
                Ok::<_, accesskit_android::jni::errors::Error>(())
            })
        });
        if let Err(error) = published {
            tracing::warn!(%error, "the tree could not be published");
        }
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
    // Waits rather than answering with a placeholder at once: a reader that reads the window once, as a dump of it does, would otherwise be told the window is empty, and no frame need come to correct it while the app is idle.
    fn request_initial_tree(&mut self) -> Option<TreeUpdate> {
        let (reader, tree) = sync_channel(0);
        self.0
            .send_event(UserEvent::Accessibility(AccessRequest::InitialTree(reader)))
            .ok()?;
        tree.recv_timeout(INITIAL_TREE_WAIT).ok()
    }
}

impl ActionHandler for Forward {
    fn do_action(&mut self, request: ActionRequest) {
        let _ = self
            .0
            .send_event(UserEvent::Accessibility(AccessRequest::Action(request)));
    }
}
