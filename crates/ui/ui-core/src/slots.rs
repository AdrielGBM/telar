use crate::layout_item::LayoutItem;

/// The children a component receives from its call site, grouped by slot. A bare child lands in the
/// default slot (`None`); a child written with `slot:"name"` lands in that named slot. Inside the
/// component, the `children` placeholder drains the default slot and `children name:"x"` drains the
/// `"x"` slot — each in call-site order. Draining is one-shot: a slot placeholder consumes its
/// children, so referencing the same slot twice yields an empty list the second time.
#[derive(Default)]
pub struct Slots {
    items: Vec<(Option<&'static str>, Box<dyn LayoutItem>)>,
}

impl Slots {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, name: Option<&'static str>, item: Box<dyn LayoutItem>) {
        self.items.push((name, item));
    }

    /// Drains the default (unnamed) children in call-site order.
    pub fn take_default(&mut self) -> Vec<Box<dyn LayoutItem>> {
        self.take_matching(|n| n.is_none())
    }

    /// Drains the children assigned to the named slot `name`, in call-site order.
    pub fn take(&mut self, name: &str) -> Vec<Box<dyn LayoutItem>> {
        self.take_matching(|n| *n == Some(name))
    }

    fn take_matching(
        &mut self,
        pred: impl Fn(&Option<&'static str>) -> bool,
    ) -> Vec<Box<dyn LayoutItem>> {
        let mut taken = Vec::new();
        let mut rest = Vec::new();
        for (name, item) in std::mem::take(&mut self.items) {
            if pred(&name) {
                taken.push(item);
            } else {
                rest.push((name, item));
            }
        }
        self.items = rest;
        taken
    }
}
