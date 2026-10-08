//! The args a preview reads, and the state behind every control the workshop gives them.

use std::any::Any;
use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::fmt;
use std::rc::Rc;

use reactive_core::{OwnerId, RwSignal, batch, effect, owner_scope, signal, with_owner};

use super::{ArgValue, ControlKind, PreviewArg, PreviewEntry, PropDefault, PropField, PropsSchema};

/// How a change to an arg reaches the preview.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArgBinding {
    /// Read once per build, through [`PreviewCtx::arg`](super::PreviewCtx::arg): a change builds the preview again.
    Remount,
    /// A two-way signal, through [`PreviewCtx::signal`](super::PreviewCtx::signal): a change reaches the tree as it runs, and the tree's own writes reach the control.
    Live,
    /// Passed through as it was written, because its type has no control.
    ReadOnly,
}

/// One arg a preview declares ahead of being built, so a control exists before the first build and a generated table can stay a `const`.
///
/// What it declares wins over what reading the arg says of it, and over the prop of the same name. Its binding stands only until the arg is read, since how the preview reads it is what decides how a change reaches it.
#[non_exhaustive]
#[derive(Clone, Copy, Debug)]
pub struct ArgSpec {
    pub name: &'static str,
    pub binding: ArgBinding,
    /// The default in [`ArgValue`]'s text form.
    pub default: Option<&'static str>,
    /// A fn rather than a value so a generated table can name the control through the probe, which is not `const`.
    pub control: Option<fn() -> ControlKind>,
    /// Empty to leave the doc to the prop of the same name.
    pub doc: &'static str,
}

impl ArgSpec {
    pub const fn new(name: &'static str) -> Self {
        Self {
            name,
            binding: ArgBinding::Remount,
            default: None,
            control: None,
            doc: "",
        }
    }

    pub const fn binding(self, binding: ArgBinding) -> Self {
        Self { binding, ..self }
    }

    pub const fn default(self, text: &'static str) -> Self {
        Self {
            default: Some(text),
            ..self
        }
    }

    pub const fn control(self, control: fn() -> ControlKind) -> Self {
        Self {
            control: Some(control),
            ..self
        }
    }

    pub const fn doc(self, doc: &'static str) -> Self {
        Self { doc, ..self }
    }
}

/// One row of a controls panel: an arg, its control and what it holds now.
#[non_exhaustive]
#[derive(Clone, Debug)]
pub struct ArgState {
    pub name: &'static str,
    pub binding: ArgBinding,
    pub control: ControlKind,
    pub doc: &'static str,
    pub default: Option<ArgValue>,
    /// `None` for a read-only arg, which is shown through [`Self::shown`] instead.
    pub value: Option<ArgValue>,
    /// A read-only arg's `Debug` text, when its type has one.
    pub shown: Option<String>,
    /// Whether the value differs from the default.
    pub edited: bool,
    /// The prop of the same name on the component under preview: where a panel reads the prop's type and its own default.
    pub prop: Option<&'static PropField>,
}

impl PartialEq for ArgState {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
            && self.binding == other.binding
            && self.control == other.control
            && self.doc == other.doc
            && self.default == other.default
            && self.value == other.value
            && self.shown == other.shown
            && self.edited == other.edited
            && same_prop(self.prop, other.prop)
    }
}

fn same_prop(a: Option<&'static PropField>, b: Option<&'static PropField>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => std::ptr::eq(a, b),
        (a, b) => a.is_none() && b.is_none(),
    }
}

/// Why [`Args::set`] refused a value.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq)]
pub enum ArgError {
    ReadOnly {
        name: String,
    },
    /// The arg's type cannot hold the value: a number sent to a toggle, a variant the enum does not have.
    Rejected {
        name: String,
        value: ArgValue,
    },
}

impl fmt::Display for ArgError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ReadOnly { name } => write!(f, "arg `{name}` has no control"),
            Self::Rejected { name, value } => write!(f, "arg `{name}` cannot hold `{value}`"),
        }
    }
}

impl std::error::Error for ArgError {}

type Overrides = BTreeMap<String, ArgValue>;

/// The args of one mounted preview: the overrides its controls set, the signals it shares with them, and the count that tells its canvas to build it again.
///
/// A cheap handle: clones share one state, so the canvas, the build and a controls panel can each hold one. Its signals belong to the owner that was active when it was made, and are freed with it.
#[derive(Clone)]
pub struct Args(Rc<ArgsState>);

struct ArgsState {
    owner: OwnerId,
    /// Every value that differs from its default, by name: what a hot reload carries and a link encodes.
    overrides: RwSignal<Overrides>,
    remounts: RwSignal<u64>,
    /// Bumped when what a panel lists of the args changes, so it lists rows it has not seen.
    rows: RwSignal<u64>,
    /// The component's props, which an arg of the same name takes its doc, control and default from.
    props: Cell<Option<&'static PropsSchema>>,
    slots: RefCell<Vec<Slot>>,
}

/// One arg as its declaration, its prop and its last read describe it. Each source is kept apart, so a read made on every build never overwrites what was declared.
struct Slot {
    name: &'static str,
    declared: Option<Declared>,
    read: Option<Read>,
    live: Option<Rc<dyn LiveArg>>,
}

/// What an [`ArgSpec`] said.
struct Declared {
    binding: ArgBinding,
    control: Option<ControlKind>,
    default: Option<ArgValue>,
    doc: &'static str,
}

/// What reading the arg said: its type's control and the default the preview passed.
struct Read {
    binding: ArgBinding,
    control: ControlKind,
    default: Option<ArgValue>,
    shown: Option<String>,
    accepts: Option<fn(&ArgValue) -> bool>,
}

impl Read {
    fn typed<T: PreviewArg>(binding: ArgBinding, default: &T) -> Self {
        Self {
            binding,
            control: T::CONTROL,
            default: Some(default.to_arg_value()),
            shown: None,
            accepts: Some(T::accepts),
        }
    }
}

/// What a panel shows of an arg, apart from its value.
#[derive(Clone, Debug)]
struct Row {
    binding: ArgBinding,
    control: ControlKind,
    doc: &'static str,
    default: Option<ArgValue>,
    shown: Option<String>,
    prop: Option<&'static PropField>,
}

impl PartialEq for Row {
    fn eq(&self, other: &Self) -> bool {
        self.binding == other.binding
            && self.control == other.control
            && self.doc == other.doc
            && self.default == other.default
            && self.shown == other.shown
            && same_prop(self.prop, other.prop)
    }
}

impl Slot {
    fn new(name: &'static str) -> Self {
        Self {
            name,
            declared: None,
            read: None,
            live: None,
        }
    }

    fn row(&self, props: Option<&'static PropsSchema>) -> Row {
        let declared = self.declared.as_ref();
        let read = self.read.as_ref();
        let prop = props.and_then(|props| props.field(self.name));
        let typed = read.map(|read| read.control);
        let control = declared
            .and_then(|declared| declared.control)
            .or_else(|| prop.map(|prop| prop_control(prop, typed)))
            .or(typed)
            .unwrap_or(ControlKind::ReadOnly);
        let default = declared
            .and_then(|declared| declared.default.clone())
            .or_else(|| read.and_then(|read| read.default.clone()))
            .or_else(|| prop.and_then(|prop| prop_default(prop, &control)));
        Row {
            binding: read
                .map(|read| read.binding)
                .or(declared.map(|declared| declared.binding))
                .unwrap_or(ArgBinding::Remount),
            control,
            doc: declared
                .map(|declared| declared.doc)
                .filter(|doc| !doc.is_empty())
                .or(prop.map(|prop| prop.doc))
                .unwrap_or(""),
            default,
            shown: read.and_then(|read| read.shown.clone()),
            prop,
        }
    }

    fn accepts(&self) -> Option<fn(&ArgValue) -> bool> {
        self.read.as_ref().and_then(|read| read.accepts)
    }
}

/// The prop's control, which refines the type's: it stands where it is the same kind of control as the type's, carrying its range, step or lines, or where it shows the prop read-only. A `some` prop's setter takes what its `Option` holds, so its control is the one inside.
fn prop_control(prop: &PropField, typed: Option<ControlKind>) -> ControlKind {
    let control = match (prop.some, prop.control()) {
        (true, ControlKind::Optional(inner)) => *inner,
        (_, control) => control,
    };
    match typed {
        Some(typed) if !control.is_read_only() && !control.is_same_kind(&typed) => typed,
        _ => control,
    }
}

fn prop_default(prop: &PropField, control: &ControlKind) -> Option<ArgValue> {
    let PropDefault::Expr(text) = prop.default else {
        return None;
    };
    text.parse().ok().filter(|value| control.admits(value))
}

/// A [`ArgBinding::Live`] arg's signal, with its type erased.
trait LiveArg {
    fn read(&self) -> ArgValue;
    fn write(&self, value: &ArgValue) -> bool;
    fn as_any(&self) -> &dyn Any;
}

struct Live<T: 'static>(RwSignal<T>);

impl<T: PreviewArg> LiveArg for Live<T> {
    fn read(&self) -> ArgValue {
        self.0.with(T::to_arg_value)
    }

    fn write(&self, value: &ArgValue) -> bool {
        match T::from_arg_value(value) {
            Some(value) => {
                self.0.set(value);
                true
            }
            None => false,
        }
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl Default for Args {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for Args {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let slots = self.0.slots.borrow();
        f.debug_struct("Args")
            .field("names", &slots.iter().map(|s| s.name).collect::<Vec<_>>())
            .field("overrides", &self.0.overrides.peek())
            .finish_non_exhaustive()
    }
}

impl Args {
    /// Args held in memory only.
    pub fn new() -> Self {
        Self::with_overrides(|| signal(Overrides::new()))
    }

    /// Args whose overrides survive a hot reload, under `key` (a preview's id).
    pub fn persisted(key: &str) -> Self {
        Self::with_overrides(|| {
            crate::hot_signal(&format!("@telar/preview.args/{key}"), Overrides::new())
        })
    }

    /// The args a canvas mounts `entry` with: persisted under its id, linked to its component's props and with its declared args listed.
    pub fn for_entry(entry: &PreviewEntry) -> Self {
        let args = Self::persisted(entry.id);
        if let Some(props) = entry.props {
            args.link_props(props());
        }
        args.declare(entry.args);
        args
    }

    fn with_overrides(overrides: impl FnOnce() -> RwSignal<Overrides>) -> Self {
        let scope = owner_scope();
        let owner = scope.id();
        let state = ArgsState {
            owner,
            overrides: overrides(),
            remounts: signal(0),
            rows: signal(0),
            props: Cell::new(None),
            slots: RefCell::new(Vec::new()),
        };
        drop(scope);
        Self(Rc::new(state))
    }

    /// Links each arg to the prop of the same name, which gives it its doc, its refined control and, until the preview passes one, its default.
    pub fn link_props(&self, props: &'static PropsSchema) {
        if self
            .0
            .props
            .replace(Some(props))
            .is_none_or(|held| !std::ptr::eq(held, props))
        {
            bump(self.0.rows);
        }
    }

    /// Lists `specs` before the preview reads them, with their declared defaults, controls and docs, which reading them does not replace.
    pub fn declare(&self, specs: &[ArgSpec]) {
        for spec in specs {
            let default = spec.default.and_then(|text| {
                let parsed = text.parse().ok();
                debug_assert!(
                    parsed.is_some(),
                    "arg `{}` declares the default `{text}`, which is not an arg value",
                    spec.name
                );
                parsed
            });
            self.record(spec.name, |slot| {
                slot.declared = Some(Declared {
                    binding: spec.binding,
                    control: spec.control.map(|control| control()),
                    default,
                    doc: spec.doc,
                });
            });
        }
    }

    /// The value a control set for `name`, or its default. See [`PreviewCtx::arg`](super::PreviewCtx::arg).
    pub(crate) fn arg<T: PreviewArg>(&self, name: &'static str, default: T) -> T {
        self.record(name, |slot| {
            slot.read = Some(Read::typed(ArgBinding::Remount, &default));
            slot.live = None;
        });
        self.override_or(name, default)
    }

    /// A signal shared with `name`'s control. See [`PreviewCtx::signal`](super::PreviewCtx::signal).
    pub(crate) fn signal<T: PreviewArg>(&self, name: &'static str, default: T) -> RwSignal<T> {
        if let Some(existing) = self.live_signal::<T>(name) {
            return existing;
        }
        self.record(name, |slot| {
            slot.read = Some(Read::typed(ArgBinding::Live, &default));
        });
        let initial = self.override_or(name, default);
        let mirrored = self.row(name).and_then(|row| row.default);
        let overrides = self.0.overrides;
        let live = with_owner(Some(self.0.owner), || {
            let live = signal(initial);
            effect(move || {
                let value = live.with(T::to_arg_value);
                store_override(overrides, name, mirrored.as_ref(), value);
            });
            live
        });
        self.record(name, |slot| slot.live = Some(Rc::new(Live(live))));
        live
    }

    fn live_signal<T: PreviewArg>(&self, name: &str) -> Option<RwSignal<T>> {
        self.with_slot(name, |slot| {
            let live = slot.live.as_ref()?.as_any().downcast_ref::<Live<T>>()?;
            live.0.is_alive().then_some(live.0)
        })
        .flatten()
    }

    /// Lists `name` as an arg with no control, shown by `shown`. What the probe answers for a type without [`PreviewArg`].
    pub(crate) fn read_only(&self, name: &'static str, shown: Option<String>) {
        self.record(name, |slot| {
            slot.read = Some(Read {
                binding: ArgBinding::ReadOnly,
                control: ControlKind::ReadOnly,
                default: None,
                shown,
                accepts: None,
            });
            slot.live = None;
        });
    }

    /// `name`'s override, or else its declared default, or else `default`: whichever the type can hold first.
    fn override_or<T: PreviewArg>(&self, name: &str, default: T) -> T {
        self.0
            .overrides
            .peek_with(|overrides| overrides.get(name).and_then(T::from_arg_value))
            .or_else(|| {
                self.with_slot(name, |slot| {
                    let declared = slot.declared.as_ref()?.default.as_ref()?;
                    T::from_arg_value(declared)
                })
                .flatten()
            })
            .unwrap_or(default)
    }

    fn with_slot<R>(&self, name: &str, read: impl FnOnce(&Slot) -> R) -> Option<R> {
        self.0
            .slots
            .borrow()
            .iter()
            .find(|s| s.name == name)
            .map(read)
    }

    fn row(&self, name: &str) -> Option<Row> {
        let props = self.0.props.get();
        self.with_slot(name, |slot| slot.row(props))
    }

    /// Updates `name`'s slot, adding it if it is new, and tells a panel when what it lists of it changed.
    fn record(&self, name: &'static str, update: impl FnOnce(&mut Slot)) {
        let props = self.0.props.get();
        let changed = {
            let mut slots = self.0.slots.borrow_mut();
            let existing = slots.iter().position(|s| s.name == name);
            let index = existing.unwrap_or_else(|| {
                slots.push(Slot::new(name));
                slots.len() - 1
            });
            let slot = &mut slots[index];
            let before = existing.map(|_| slot.row(props));
            update(slot);
            before != Some(slot.row(props))
        };
        if changed {
            bump(self.0.rows);
        }
    }

    /// Sets `name` from a control. A remounting arg builds the preview again when its value changes; a live one is written through its signal.
    ///
    /// An arg nobody has read yet is held until one does, which is how a link's args reach a preview before its first build.
    pub fn set(&self, name: &str, value: ArgValue) -> Result<(), ArgError> {
        let props = self.0.props.get();
        let slot = self.with_slot(name, |slot| {
            (slot.row(props), slot.accepts(), slot.live.clone())
        });
        let Some((row, accepts, live)) = slot else {
            store_override(self.0.overrides, name, None, value);
            return Ok(());
        };
        if row.binding == ArgBinding::ReadOnly {
            return Err(ArgError::ReadOnly {
                name: name.to_string(),
            });
        }
        let accepted = accepts.is_none_or(|accepts| accepts(&value));
        if !accepted || live.as_ref().is_some_and(|live| !live.write(&value)) {
            return Err(ArgError::Rejected {
                name: name.to_string(),
                value,
            });
        }
        if live.is_none()
            && store_override(self.0.overrides, name, row.default.as_ref(), value)
            && row.binding == ArgBinding::Remount
        {
            bump(self.0.remounts);
        }
        Ok(())
    }

    /// Puts every arg back to its default, building the preview again if one it reads at build had changed.
    pub fn reset(&self) {
        let held = self.0.overrides.peek();
        if held.is_empty() {
            return;
        }
        let props = self.0.props.get();
        let slots = self.0.slots.borrow();
        let rows: Vec<(&Slot, Row)> = slots.iter().map(|slot| (slot, slot.row(props))).collect();
        let remount = rows
            .iter()
            .any(|(slot, row)| row.binding == ArgBinding::Remount && held.contains_key(slot.name));
        let live: Vec<(Rc<dyn LiveArg>, ArgValue)> = rows
            .into_iter()
            .filter_map(|(slot, row)| Some((slot.live.clone()?, row.default?)))
            .collect();
        drop(slots);
        batch(|| {
            for (live, default) in &live {
                live.write(default);
            }
            self.0.overrides.set(Overrides::new());
            if remount {
                bump(self.0.remounts);
            }
        });
    }

    /// What `name` holds now: its live signal's value, its override or its default. Tracked, so a control reading it follows it.
    pub fn get(&self, name: &str) -> Option<ArgValue> {
        let props = self.0.props.get();
        let (live, default) = self
            .with_slot(name, |slot| (slot.live.clone(), slot.row(props).default))
            .unwrap_or_default();
        if let Some(live) = live {
            return Some(live.read());
        }
        self.0
            .overrides
            .with(|overrides| overrides.get(name).cloned())
            .or(default)
    }

    /// Every arg declared or read so far, in that order, with what it holds now. Tracked, so a panel listing them follows both new args and new values.
    pub fn states(&self) -> Vec<ArgState> {
        self.0.rows.get();
        let props = self.0.props.get();
        let rows: Vec<(&'static str, Row)> = self
            .0
            .slots
            .borrow()
            .iter()
            .map(|slot| (slot.name, slot.row(props)))
            .collect();
        rows.into_iter()
            .map(|(name, row)| {
                let value = match row.binding {
                    ArgBinding::ReadOnly => None,
                    _ => self.get(name),
                };
                ArgState {
                    name,
                    binding: row.binding,
                    control: row.control,
                    doc: row.doc,
                    edited: value.is_some() && value != row.default,
                    default: row.default,
                    value,
                    shown: row.shown,
                    prop: row.prop,
                }
            })
            .collect()
    }

    /// Every value that differs from its default, by name, in name order: what a link to this preview carries.
    pub fn overrides(&self) -> Vec<(String, ArgValue)> {
        self.0.overrides.with(|overrides| {
            overrides
                .iter()
                .map(|(name, value)| (name.clone(), value.clone()))
                .collect()
        })
    }

    /// Counts the changes that must build the preview again. Tracked: a canvas reads it where it decides what to mount, and mounts afresh whenever it moves.
    pub fn remounts(&self) -> u64 {
        self.0.remounts.get()
    }
}

fn bump(counter: RwSignal<u64>) {
    counter.update(|n| *n = n.wrapping_add(1));
}

/// Stores `value` as `name`'s override, or drops the override when `value` is the default. Answers whether the stored override changed.
fn store_override(
    overrides: RwSignal<Overrides>,
    name: &str,
    default: Option<&ArgValue>,
    value: ArgValue,
) -> bool {
    let is_default = default == Some(&value);
    let changed = overrides.peek_with(|held| match held.get(name) {
        Some(held) => is_default || *held != value,
        None => !is_default,
    });
    if changed {
        overrides.update(|held| {
            if is_default {
                held.remove(name);
            } else {
                held.insert(name.to_string(), value);
            }
        });
    }
    changed
}

#[cfg(test)]
#[path = "args_test.rs"]
mod tests;
