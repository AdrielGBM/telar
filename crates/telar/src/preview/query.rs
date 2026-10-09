//! [`Query`]: how a play names what it acts on — by what a reader is told about it, not by where it is drawn.

use std::fmt;

use crate::{AccessNode, Role};

/// What to find in a canvas's accessibility snapshot: a role, a name, or both. Start one with [`by_role`] or [`by_text`].
///
/// A name matches the whole of the name a reader is told, ignoring the space around it; [`containing`](Self::containing) matches a part of it instead. A query that matches several nodes is an error for an action, which has to know which one it means; [`nth`](Self::nth) picks one by its place in reading order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Query {
    role: Option<Role>,
    name: Option<Name>,
    nth: Option<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Name {
    Exact(String),
    Containing(String),
}

/// The nodes with `role`: `by_role(Role::Button).named("Save")`.
pub fn by_role(role: Role) -> Query {
    Query {
        role: Some(role),
        name: None,
        nth: None,
    }
}

/// The nodes a reader names `text`, whatever their role: a control named by the label it draws, or a run of text that belongs to no control.
pub fn by_text(text: impl Into<String>) -> Query {
    Query {
        role: None,
        name: None,
        nth: None,
    }
    .named(text)
}

impl Query {
    /// Only the nodes named exactly `name`.
    pub fn named(self, name: impl Into<String>) -> Self {
        Self {
            name: Some(Name::Exact(name.into())),
            ..self
        }
    }

    /// Only the nodes whose name contains `part`.
    pub fn containing(self, part: impl Into<String>) -> Self {
        Self {
            name: Some(Name::Containing(part.into())),
            ..self
        }
    }

    /// Only the `index`th of the nodes matched, from 0, in reading order.
    pub fn nth(self, index: usize) -> Self {
        Self {
            nth: Some(index),
            ..self
        }
    }

    pub fn matches(&self, node: &AccessNode) -> bool {
        let role = self.role.is_none_or(|role| node.role == role);
        let name = match &self.name {
            None => true,
            Some(Name::Exact(name)) => node.name.trim() == name.trim(),
            Some(Name::Containing(part)) => node.name.contains(part.as_str()),
        };
        role && name
    }

    /// The nodes of `snapshot` this query matches, in reading order.
    pub fn find_all<'a>(&self, snapshot: &'a [AccessNode]) -> Vec<&'a AccessNode> {
        let matched = snapshot.iter().filter(|node| self.matches(node));
        match self.nth {
            Some(index) => matched.skip(index).take(1).collect(),
            None => matched.collect(),
        }
    }
}

impl fmt::Display for Query {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let role = self.role.map_or("node", |role| role.as_str());
        f.write_str(role)?;
        match &self.name {
            None => {}
            Some(Name::Exact(name)) => write!(f, " {name:?}")?,
            Some(Name::Containing(part)) => write!(f, " containing {part:?}")?,
        }
        if let Some(index) = self.nth {
            write!(f, " #{index}")?;
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "query_test.rs"]
mod tests;
