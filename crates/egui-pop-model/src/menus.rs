//! Menus are declared, never drawn by whoever fills them: a menu is an id
//! (`app.main`, `panel.tab`) to which the shell, a panel or a plugin
//! contributes items. An item names a command (its title and keys come
//! from the command and the keymap, so a menu never shows a key that does
//! not work), a submenu, or a list the application fills when the menu
//! opens (the themes, the recent files). Items sit in groups, separated
//! when shown, and in an order within their group; a `when` clause hides
//! or greys an item, a `checked` clause ticks it.

use serde::Serialize;
use serde_json::Value as Json;

use crate::ids::{CommandId, MenuId, OwnerId};
use crate::registry::OwnedList;
use crate::when::{When, WhenContext};

/// What an item is.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Item {
    /// A command, with the arguments it runs with, and a label of its
    /// own (else the command's title).
    Command {
        id: CommandId,
        args: Json,
        #[serde(skip_serializing_if = "Option::is_none")]
        label: Option<String>,
    },
    /// Another menu, shown under a label.
    Submenu { menu: MenuId, label: String },
    /// Entries the application makes when the menu opens.
    List { name: String },
}

/// One contribution to a menu.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct MenuItem {
    pub menu: MenuId,
    pub item: Item,
    /// Items of a group are shown together; groups are separated and
    /// ordered by name (`1_open`, `2_preferences`).
    pub group: String,
    pub order: i32,
    /// Where it applies; empty: everywhere.
    pub when: String,
    /// Greyed rather than hidden when `when` does not hold.
    pub grey: bool,
    /// When it shows ticked; empty: never.
    pub checked: String,
}

impl MenuItem {
    pub fn command(menu: MenuId, group: &str, order: i32, id: CommandId) -> Self {
        Self {
            menu,
            item: Item::Command {
                id,
                args: Json::Null,
                label: None,
            },
            group: group.into(),
            order,
            when: String::new(),
            grey: true,
            checked: String::new(),
        }
    }

    pub fn submenu(menu: MenuId, group: &str, order: i32, sub: MenuId, label: &str) -> Self {
        Self {
            menu,
            item: Item::Submenu {
                menu: sub,
                label: label.into(),
            },
            group: group.into(),
            order,
            when: String::new(),
            grey: true,
            checked: String::new(),
        }
    }

    pub fn list(menu: MenuId, group: &str, order: i32, name: &str) -> Self {
        Self {
            item: Item::List { name: name.into() },
            ..Self::submenu(menu.clone(), group, order, menu, "")
        }
    }

    pub fn args(mut self, args: Json) -> Self {
        if let Item::Command { args: a, .. } = &mut self.item {
            *a = args;
        }
        self
    }

    /// Its own label (a command run with arguments: `50%`), instead of
    /// the command's title.
    pub fn label(mut self, label: &str) -> Self {
        if let Item::Command { label: l, .. } = &mut self.item {
            *l = Some(label.into());
        }
        self
    }

    pub fn when(mut self, when: &str, grey: bool) -> Self {
        self.when = when.into();
        self.grey = grey;
        self
    }

    pub fn checked(mut self, when: &str) -> Self {
        self.checked = when.into();
        self
    }
}

/// An item as a menu shows it in a context.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "shown", rename_all = "lowercase")]
pub enum Shown {
    Item {
        item: Item,
        enabled: bool,
        checked: bool,
        /// It has a `checked` clause: a toggle, ticked or not.
        checkable: bool,
    },
    Separator,
}

/// The menu contributions, each owned.
#[derive(Debug, Default)]
pub struct Menus {
    items: OwnedList<MenuItem>,
}

/// Sorts items as menus show them: by group, then order.
pub fn sort_items(items: &mut [MenuItem]) {
    items.sort_by(|a, b| (&a.group, a.order).cmp(&(&b.group, b.order)));
}

/// The items of one menu in a context: groups in order, separated, items
/// hidden or greyed by their `when`.
pub fn show_items(items: &[MenuItem], cx: &WhenContext) -> Vec<Shown> {
    let holds = |clause: &str| clause.is_empty() || When::parse(clause).is_ok_and(|w| w.eval(cx));
    let mut items: Vec<&MenuItem> = items.iter().collect();
    items.sort_by(|a, b| (&a.group, a.order).cmp(&(&b.group, b.order)));
    let mut out = Vec::new();
    let mut group: Option<&str> = None;
    for i in items {
        let applies = holds(&i.when);
        if !applies && !i.grey {
            continue;
        }
        if group.is_some_and(|g| g != i.group) {
            out.push(Shown::Separator);
        }
        group = Some(&i.group);
        out.push(Shown::Item {
            item: i.item.clone(),
            enabled: applies,
            checked: !i.checked.is_empty() && holds(&i.checked),
            checkable: !i.checked.is_empty(),
        });
    }
    out
}

impl Menus {
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds an item; its clauses must parse.
    pub fn contribute(&mut self, owner: &OwnerId, item: MenuItem) -> Result<(), String> {
        for clause in [&item.when, &item.checked] {
            When::parse(clause).map_err(|e| format!("menu `{}`: {e}", item.menu))?;
        }
        self.items.push(owner, item);
        Ok(())
    }

    /// The items of a menu as declared, in contribution order.
    pub fn items(&self, menu: &MenuId) -> Vec<MenuItem> {
        self.items
            .iter()
            .filter(|s| s.item.menu == *menu)
            .map(|s| s.item.clone())
            .collect()
    }

    /// The items of a menu in a context.
    pub fn show(&self, menu: &MenuId, cx: &WhenContext) -> Vec<Shown> {
        show_items(&self.items(menu), cx)
    }

    /// Every menu id contributed to.
    pub fn ids(&self) -> Vec<MenuId> {
        let mut v: Vec<MenuId> = self.items.iter().map(|s| s.item.menu.clone()).collect();
        v.sort();
        v.dedup();
        v
    }

    /// Removes an owner's items running a command; returns how many.
    pub fn remove_command(&mut self, owner: &OwnerId, command: &CommandId) -> usize {
        let before = self.items.len();
        self.items.retain(|s| {
            !(s.owner == *owner
                && matches!(&s.item.item, Item::Command { id, .. } if id == command))
        });
        before - self.items.len()
    }

    /// Removes an owner's items; returns how many.
    pub fn remove_owner(&mut self, owner: &OwnerId) -> usize {
        self.items.remove_owner(owner)
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}
