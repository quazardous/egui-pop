//! What a graph file holds, as read: each part kept with the keys it was
//! written with, in their order, so that an edit can render it back.

use std::fmt;
use std::marker::PhantomData;

use serde::Deserialize;
use serde::de::{self, Deserializer, IgnoredAny, MapAccess, Visitor};
use serde_json::Value as Json;
use serde_saphyr::Spanned;

/// Where something was written: a line and a column, from 1.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct At {
    pub line: u64,
    pub column: u64,
}

impl At {
    fn of<T>(s: &Spanned<T>) -> At {
        At {
            line: s.referenced.line(),
            column: s.referenced.column(),
        }
    }

    /// `None` for a place not known (an entry made by an edit).
    pub fn known(self) -> Option<At> {
        (self.line > 0).then_some(self)
    }
}

/// A mapping read in the order of its keys.
#[derive(Clone, Debug)]
pub(crate) struct Ordered<V>(pub Vec<(String, V)>);

impl<V> Default for Ordered<V> {
    fn default() -> Self {
        Ordered(Vec::new())
    }
}

impl<'de, V: Deserialize<'de>> Deserialize<'de> for Ordered<V> {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct Visit<V>(PhantomData<V>);

        impl<'de, V: Deserialize<'de>> Visitor<'de> for Visit<V> {
            type Value = Ordered<V>;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a mapping")
            }

            fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
                Ok(Ordered::default())
            }

            fn visit_none<E: de::Error>(self) -> Result<Self::Value, E> {
                Ok(Ordered::default())
            }

            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
                let mut out = Vec::new();
                while let Some(key) = map.next_key::<String>()? {
                    out.push((key, map.next_value()?));
                }
                Ok(Ordered(out))
            }
        }

        d.deserialize_any(Visit(PhantomData))
    }
}

/// Reads a mapping's values as JSON, keys in order.
fn json_fields<'de, A: MapAccess<'de>>(mut map: A) -> Result<Vec<(String, Json)>, A::Error> {
    let mut out = Vec::new();
    while let Some(key) = map.next_key::<String>()? {
        out.push((key, map.next_value::<Json>()?));
    }
    Ok(out)
}

/// A port: its type alone (`image: image`), or its fields
/// (`mask: {type: mask, label: Mask, multiple: true}`).
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum PortEntry {
    Type(String),
    Full(Vec<(String, Json)>),
}

impl<'de> Deserialize<'de> for PortEntry {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct Visit;

        impl<'de> Visitor<'de> for Visit {
            type Value = PortEntry;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a port: its type, or a mapping with `type`")
            }

            fn visit_str<E: de::Error>(self, v: &str) -> Result<PortEntry, E> {
                Ok(PortEntry::Type(v.to_string()))
            }

            fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<PortEntry, A::Error> {
                json_fields(map).map(PortEntry::Full)
            }
        }

        d.deserialize_any(Visit)
    }
}

/// A parameter: its value alone (`radius: 2`), or its fields
/// (`radius: {label: Radius, value: 2, min: 0, max: 20}`).
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ParamEntry {
    Value(f64),
    Full(Vec<(String, Json)>),
}

impl<'de> Deserialize<'de> for ParamEntry {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct Visit;

        impl<'de> Visitor<'de> for Visit {
            type Value = ParamEntry;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a parameter: a number, or a mapping with `min` and `max`")
            }

            fn visit_f64<E: de::Error>(self, v: f64) -> Result<ParamEntry, E> {
                Ok(ParamEntry::Value(v))
            }

            fn visit_i64<E: de::Error>(self, v: i64) -> Result<ParamEntry, E> {
                Ok(ParamEntry::Value(v as f64))
            }

            fn visit_u64<E: de::Error>(self, v: u64) -> Result<ParamEntry, E> {
                Ok(ParamEntry::Value(v as f64))
            }

            fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<ParamEntry, A::Error> {
                json_fields(map).map(ParamEntry::Full)
            }
        }

        d.deserialize_any(Visit)
    }
}

/// A group: its label alone (`detail: Detail`), or `{label: Detail}`.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct GroupEntry(pub Option<String>);

impl<'de> Deserialize<'de> for GroupEntry {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct Visit;

        impl<'de> Visitor<'de> for Visit {
            type Value = GroupEntry;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a group: its label, or a mapping with `label`")
            }

            fn visit_unit<E: de::Error>(self) -> Result<GroupEntry, E> {
                Ok(GroupEntry(None))
            }

            fn visit_none<E: de::Error>(self) -> Result<GroupEntry, E> {
                Ok(GroupEntry(None))
            }

            fn visit_str<E: de::Error>(self, v: &str) -> Result<GroupEntry, E> {
                Ok(GroupEntry(Some(v.to_string())))
            }

            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<GroupEntry, A::Error> {
                let mut label = None;
                while let Some(key) = map.next_key::<String>()? {
                    if key == "label" {
                        label = Some(map.next_value::<String>()?);
                    } else {
                        map.next_value::<IgnoredAny>()?;
                    }
                }
                Ok(GroupEntry(label))
            }
        }

        d.deserialize_any(Visit)
    }
}

/// A kind of node: what a node of that kind has unless it says
/// otherwise, and what the canvas's menu adds.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub(crate) struct KindEntry {
    pub title: Option<String>,
    pub inputs: Ordered<PortEntry>,
    pub outputs: Ordered<PortEntry>,
    pub params: Ordered<ParamEntry>,
}

/// A node's field, under its key.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Field {
    Id(String),
    Kind(String),
    Title(String),
    Pos([f32; 2]),
    Group(String),
    Inputs(Vec<(String, PortEntry)>),
    Outputs(Vec<(String, PortEntry)>),
    Params(Vec<(String, ParamEntry)>),
    /// A key egui-pop does not know, kept as read.
    Other(Json),
}

/// A node as written: its fields under their keys, in order.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct NodeEntry {
    /// Where its `id` is.
    pub at: At,
    pub fields: Vec<(String, Field)>,
}

impl NodeEntry {
    pub fn id(&self) -> &str {
        self.fields
            .iter()
            .find_map(|(_, f)| match f {
                Field::Id(id) => Some(id.as_str()),
                _ => None,
            })
            .unwrap_or("")
    }

    pub fn field(&self, key: &str) -> Option<&Field> {
        self.fields.iter().find(|(k, _)| k == key).map(|(_, f)| f)
    }

    /// Sets a field, in place when the key is there, last otherwise.
    pub fn set(&mut self, key: &str, field: Field) {
        match self.fields.iter_mut().find(|(k, _)| k == key) {
            Some((_, f)) => *f = field,
            None => self.fields.push((key.to_string(), field)),
        }
    }
}

impl<'de> Deserialize<'de> for NodeEntry {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct Visit;

        impl<'de> Visitor<'de> for Visit {
            type Value = NodeEntry;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a node: a mapping with an `id`")
            }

            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<NodeEntry, A::Error> {
                let mut at = None;
                let mut fields = Vec::new();
                while let Some(key) = map.next_key::<String>()? {
                    let field = match key.as_str() {
                        "id" => {
                            let id = map.next_value::<Spanned<String>>()?;
                            at = Some(At::of(&id));
                            Field::Id(id.value)
                        }
                        "kind" => Field::Kind(map.next_value()?),
                        "title" => Field::Title(map.next_value()?),
                        "pos" => {
                            let v = map.next_value::<Vec<f64>>()?;
                            let &[x, y] = v.as_slice() else {
                                return Err(de::Error::custom("`pos` takes two numbers, `[x, y]`"));
                            };
                            Field::Pos([x as f32, y as f32])
                        }
                        "group" => Field::Group(map.next_value()?),
                        "inputs" => Field::Inputs(map.next_value::<Ordered<PortEntry>>()?.0),
                        "outputs" => Field::Outputs(map.next_value::<Ordered<PortEntry>>()?.0),
                        "params" => Field::Params(map.next_value::<Ordered<ParamEntry>>()?.0),
                        _ => Field::Other(map.next_value()?),
                    };
                    fields.push((key, field));
                }
                let at = at.ok_or_else(|| de::Error::missing_field("id"))?;
                Ok(NodeEntry { at, fields })
            }
        }

        d.deserialize_map(Visit)
    }
}

/// A wire as written: `load.image -> blur.image`, or a mapping with
/// `from` and `to` (and keys of the application's own).
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum EdgeEntry {
    Text(String),
    Map(Vec<(String, Json)>),
}

impl<'de> Deserialize<'de> for EdgeEntry {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct Visit;

        impl<'de> Visitor<'de> for Visit {
            type Value = EdgeEntry;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a wire: `node.port -> node.port`, or a mapping with `from` and `to`")
            }

            fn visit_str<E: de::Error>(self, v: &str) -> Result<EdgeEntry, E> {
                Ok(EdgeEntry::Text(v.to_string()))
            }

            fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<EdgeEntry, A::Error> {
                json_fields(map).map(EdgeEntry::Map)
            }
        }

        d.deserialize_any(Visit)
    }
}

/// A wire with where it was written.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct EdgeItem {
    pub entry: EdgeEntry,
    pub at: At,
}

/// The file's top level as serde reads it.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub(crate) struct RawFile {
    #[serde(rename = "$extensions")]
    pub extensions: Option<Json>,
    pub groups: Ordered<GroupEntry>,
    pub kinds: Ordered<KindEntry>,
    pub nodes: Option<Vec<NodeEntry>>,
    pub edges: Option<Vec<Spanned<EdgeEntry>>>,
}

/// The parts of a graph file egui-pop reads.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Entries {
    pub extensions: Option<Json>,
    pub groups: Vec<(String, GroupEntry)>,
    pub kinds: Vec<(String, KindEntry)>,
    pub nodes: Vec<NodeEntry>,
    pub edges: Vec<EdgeItem>,
}

impl PartialEq for KindEntry {
    fn eq(&self, other: &Self) -> bool {
        self.title == other.title
            && self.inputs.0 == other.inputs.0
            && self.outputs.0 == other.outputs.0
            && self.params.0 == other.params.0
    }
}

impl From<RawFile> for Entries {
    fn from(raw: RawFile) -> Entries {
        Entries {
            extensions: raw.extensions,
            groups: raw.groups.0,
            kinds: raw.kinds.0,
            nodes: raw.nodes.unwrap_or_default(),
            edges: raw
                .edges
                .unwrap_or_default()
                .into_iter()
                .map(|e| EdgeItem {
                    at: At::of(&e),
                    entry: e.value,
                })
                .collect(),
        }
    }
}
