//! The pages of today's settings, generated from their declarations: a
//! scope from the setting id's prefix (`appearance.*` to Interface,
//! `input.*` to Input, `developer.*` to Developer, the rest to General),
//! a section per settings section (the owner's title, or an alter hook's),
//! a field per setting. No hand-written page.

use super::{FORMAT, Field, Group, Item, Manifest, Meta, Scope, Section};

/// The default scopes: id, title, icon.
pub const SCOPES: [(&str, &str, &str); 4] = [
    ("general", "General", "settings"),
    ("interface", "Interface", "palette"),
    ("input", "Input", "keyboard"),
    ("developer", "Developer", "code"),
];

/// A setting as the generator takes it: its id and its section's title.
#[derive(Clone, Debug, PartialEq)]
pub struct Entry<'a> {
    pub id: &'a str,
    pub section: &'a str,
}

/// The scope a setting goes to by default.
pub fn scope_of(id: &str) -> &'static str {
    let prefix = id.split('.').next().unwrap_or("");
    match prefix {
        "appearance" | "window" | "menus" | "dialogs" | "workbench" | "palettes" | "theme" => {
            "interface"
        }
        "input" | "keys" | "pointer" => "input",
        "developer" | "debug" => "developer",
        _ => "general",
    }
}

/// A section id from its title: lower case, words joined by `-`.
pub fn slug(title: &str) -> String {
    let mut out = String::new();
    for w in title
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
    {
        if !out.is_empty() {
            out.push('-');
        }
        out.push_str(&w.to_lowercase());
    }
    if out.is_empty() {
        "settings".into()
    } else {
        out
    }
}

/// The manifest of the settings: the default scopes, a section per
/// (scope, section title) in the settings' order.
pub fn settings<'a>(entries: impl IntoIterator<Item = Entry<'a>>) -> Manifest {
    let mut m = Manifest::new(FORMAT);
    for (order, (id, title, icon)) in SCOPES.iter().enumerate() {
        m.scopes.push(Scope {
            meta: Meta {
                icon: Some((*icon).into()),
                order: order as i32,
                ..Meta::new(id, title)
            },
        });
    }
    for e in entries {
        let scope = scope_of(e.id);
        let id = slug(e.section);
        let section = match m
            .sections
            .iter_mut()
            .position(|s| s.scope == scope && s.meta.id == id)
        {
            Some(i) => &mut m.sections[i],
            None => {
                let order = m.sections.len() as i32;
                m.sections.push(Section {
                    meta: Meta {
                        order,
                        ..Meta::new(&id, e.section)
                    },
                    scope: scope.into(),
                    parent: None,
                    groups: vec![Group {
                        meta: Meta::new("main", ""),
                        items: Vec::new(),
                    }],
                });
                m.sections.last_mut().expect("just pushed")
            }
        };
        section.groups[0]
            .items
            .push(Item::Field(Field::setting(e.id)));
    }
    m
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::admin::Tree;

    #[test]
    fn settings_are_placed_by_prefix_and_section() {
        let m = settings([
            Entry {
                id: "appearance.theme_preference",
                section: "Appearance",
            },
            Entry {
                id: "input.wheel_speed",
                section: "Input",
            },
            Entry {
                id: "appearance.text_size",
                section: "Appearance",
            },
            Entry {
                id: "gallery.size",
                section: "Gallery settings",
            },
            Entry {
                id: "developer.devtools",
                section: "Developer tools",
            },
        ]);
        let mut t = Tree::default();
        t.merge(m.clone());
        let paths: Vec<String> = t.sections.iter().map(|s| s.path()).collect();
        assert_eq!(
            paths,
            [
                "interface/appearance",
                "input/input",
                "general/gallery-settings",
                "developer/developer-tools"
            ]
        );
        assert_eq!(t.sections[0].groups[0].items.len(), 2);
        assert_eq!(t.shown_scopes().len(), 4);
        // Generated manifests round-trip like any other.
        assert_eq!(Manifest::from_json(&m.to_json()).unwrap(), m);
    }

    #[test]
    fn slugs_are_stable() {
        assert_eq!(slug("Gallery settings"), "gallery-settings");
        assert_eq!(slug("  "), "settings");
        assert_eq!(slug("Files & folders"), "files-folders");
    }
}
