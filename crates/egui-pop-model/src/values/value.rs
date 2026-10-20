//! A value and what it is: [`Value`], [`Type`] and [`Descriptor`].

use serde::{Deserialize, Serialize};
use serde_json::{Value as Json, json};

use crate::ids::OwnerId;

/// A value the store holds.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Value {
    Float(f64),
    Int(i64),
    Bool(bool),
    /// One of a descriptor's choices.
    Choice(String),
    /// Numbers that belong together (a colour, a matrix).
    Floats(Vec<f64>),
    /// Whole numbers that belong together.
    Ints(Vec<i64>),
    /// Free text.
    Text(String),
}

impl Value {
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Value::Float(v) => Some(*v),
            Value::Int(v) => Some(*v as f64),
            _ => None,
        }
    }

    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Value::Int(v) => Some(*v),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Bool(v) => Some(*v),
            _ => None,
        }
    }

    pub fn as_choice(&self) -> Option<&str> {
        match self {
            Value::Choice(v) => Some(v),
            _ => None,
        }
    }

    pub fn as_text(&self) -> Option<&str> {
        match self {
            Value::Text(v) | Value::Choice(v) => Some(v),
            _ => None,
        }
    }

    /// As plain JSON (a number, a bool, a string, an array).
    pub fn to_json(&self) -> Json {
        match self {
            Value::Float(v) => json!(v),
            Value::Int(v) => json!(v),
            Value::Bool(v) => json!(v),
            Value::Choice(v) | Value::Text(v) => json!(v),
            Value::Floats(v) => json!(v),
            Value::Ints(v) => json!(v),
        }
    }

    /// Plain JSON as the type a path holds.
    pub fn from_json(ty: Type, json: &Json) -> Result<Value, String> {
        let wrong = || format!("{json} is not a {ty:?}");
        Ok(match ty {
            Type::Float => Value::Float(json.as_f64().ok_or_else(wrong)?),
            Type::Int => match json.as_i64() {
                Some(v) => Value::Int(v),
                None => Value::Float(json.as_f64().ok_or_else(wrong)?),
            },
            Type::Bool => Value::Bool(json.as_bool().ok_or_else(wrong)?),
            Type::Choice => Value::Choice(json.as_str().ok_or_else(wrong)?.into()),
            Type::Text => Value::Text(json.as_str().ok_or_else(wrong)?.into()),
            Type::Floats => Value::Floats(
                json.as_array()
                    .ok_or_else(wrong)?
                    .iter()
                    .map(|v| v.as_f64().ok_or_else(wrong))
                    .collect::<Result<_, _>>()?,
            ),
            Type::Ints => Value::Ints(
                json.as_array()
                    .ok_or_else(wrong)?
                    .iter()
                    .map(|v| v.as_i64().ok_or_else(wrong))
                    .collect::<Result<_, _>>()?,
            ),
        })
    }
}

/// The type of a value.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Type {
    Float,
    Int,
    Bool,
    Choice,
    Floats,
    Ints,
    Text,
}

/// What a path names: a directory or a file.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PathKind {
    Dir,
    File,
}

/// A text that names a path on disk: what it names and, for a file, the
/// extensions accepted (none: any). A form shows it with a Browse button.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct PathHint {
    pub kind: PathKind,
    /// Without the dot, lowercase (`json`, `txt`).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub extensions: Vec<String>,
}

impl PathHint {
    /// Whether a file's name has an accepted extension.
    pub fn accepts(&self, path: &std::path::Path) -> bool {
        self.extensions.is_empty()
            || path
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| self.extensions.iter().any(|x| x.eq_ignore_ascii_case(e)))
    }
}

/// Who may write a path, besides the framework acting for a person (see
/// [`Store::set`](super::Store::set)).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Writers {
    /// Its owner alone: one writer per value (the default).
    #[default]
    Owner,
    /// Its owner and these.
    Also(Vec<OwnerId>),
    /// Anyone.
    Any,
}

impl Writers {
    pub fn is_owner(&self) -> bool {
        *self == Writers::Owner
    }

    /// Whether `writer` may write a path `owner` declared.
    pub fn allow(&self, owner: &OwnerId, writer: &OwnerId) -> bool {
        writer == owner
            || *writer == OwnerId::FRAMEWORK
            || match self {
                Writers::Owner => false,
                Writers::Also(list) => list.contains(writer),
                Writers::Any => true,
            }
    }
}

/// What a value is and how it is shown and moved.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[non_exhaustive]
pub struct Descriptor {
    #[serde(rename = "type")]
    pub ty: Type,
    pub label: String,
    pub description: String,
    pub default: Value,
    /// What the value can be, at most.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hard: Option<(f64, f64)>,
    /// What a slider spans; past it up to `hard` only when forced.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub soft: Option<(f64, f64)>,
    pub step: f64,
    pub coarse: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
    /// Shown value = value × scale.
    pub display_scale: f64,
    /// Decimals shown.
    pub precision: u32,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub choices: Vec<String>,
    /// How many values a list holds at most.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub count: Option<u32>,
    /// How many bytes a text holds at most.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capacity: Option<u32>,
    /// A text that names a path on disk.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<PathHint>,
    /// Who may write it besides its owner.
    #[serde(skip_serializing_if = "Writers::is_owner")]
    pub writers: Writers,
}

/// Decimals enough to show one step.
fn precision_of(step: f64) -> u32 {
    if step >= 1.0 || step <= 0.0 {
        0
    } else {
        (-step.log10()).ceil() as u32
    }
}

impl Descriptor {
    fn base(ty: Type, label: &str, default: Value) -> Self {
        Self {
            ty,
            label: label.into(),
            description: String::new(),
            default,
            hard: None,
            soft: None,
            step: 1.0,
            coarse: 10.0,
            unit: None,
            display_scale: 1.0,
            precision: 0,
            choices: Vec::new(),
            count: None,
            capacity: None,
            path: None,
            writers: Writers::Owner,
        }
    }

    /// A float in `[min, max]`, its fine step a hundredth of the range.
    pub fn float(label: &str, min: f64, max: f64, default: f64) -> Self {
        let step = if max > min { (max - min) / 100.0 } else { 0.01 };
        Self {
            hard: Some((min, max)),
            step,
            coarse: step * 10.0,
            precision: precision_of(step),
            ..Self::base(Type::Float, label, Value::Float(default))
        }
    }

    pub fn int(label: &str, min: i64, max: i64, default: i64) -> Self {
        Self {
            hard: Some((min as f64, max as f64)),
            ..Self::base(Type::Int, label, Value::Int(default))
        }
    }

    pub fn bool(label: &str, default: bool) -> Self {
        Self::base(Type::Bool, label, Value::Bool(default))
    }

    pub fn choice(label: &str, choices: &[&str], default: &str) -> Self {
        Self {
            choices: choices.iter().map(|c| c.to_string()).collect(),
            ..Self::base(Type::Choice, label, Value::Choice(default.into()))
        }
    }

    pub fn text(label: &str, default: &str) -> Self {
        Self::base(Type::Text, label, Value::Text(default.into()))
    }

    /// Floats in `[min, max]`, at most `count` of them.
    pub fn floats(label: &str, count: u32, min: f64, max: f64, default: &[f64]) -> Self {
        let step = if max > min { (max - min) / 100.0 } else { 0.01 };
        Self {
            hard: Some((min, max)),
            count: Some(count),
            step,
            coarse: step * 10.0,
            precision: precision_of(step),
            ..Self::base(Type::Floats, label, Value::Floats(default.to_vec()))
        }
    }

    /// Ints in `[min, max]`, at most `count` of them.
    pub fn ints(label: &str, count: u32, min: i64, max: i64, default: &[i64]) -> Self {
        Self {
            hard: Some((min as f64, max as f64)),
            count: Some(count),
            ..Self::base(Type::Ints, label, Value::Ints(default.to_vec()))
        }
    }

    /// A text naming a directory or a file, empty by default; for a file,
    /// the extensions accepted (none: any).
    pub fn path(label: &str, kind: PathKind, extensions: &[&str]) -> Self {
        Self {
            path: Some(PathHint {
                kind,
                extensions: extensions.iter().map(|e| e.to_lowercase()).collect(),
            }),
            ..Self::base(Type::Text, label, Value::Text(String::new()))
        }
    }

    pub fn description(mut self, text: &str) -> Self {
        self.description = text.into();
        self
    }

    /// The range a slider spans, within the hard range.
    pub fn soft(mut self, lo: f64, hi: f64) -> Self {
        self.soft = Some(match self.hard {
            Some((min, max)) => (lo.max(min), hi.min(max)),
            None => (lo, hi),
        });
        self
    }

    /// The fine step (the coarse one becomes ten of them).
    pub fn step(mut self, step: f64) -> Self {
        self.step = step;
        self.coarse = step * 10.0;
        self.precision = precision_of(step);
        self
    }

    pub fn coarse(mut self, coarse: f64) -> Self {
        self.coarse = coarse;
        self
    }

    pub fn unit(mut self, unit: &str) -> Self {
        self.unit = Some(unit.into());
        self
    }

    /// Shown value = value × scale, with so many decimals.
    pub fn display(mut self, scale: f64, precision: u32) -> Self {
        self.display_scale = scale;
        self.precision = precision;
        self
    }

    /// At most so many bytes of text.
    pub fn capacity(mut self, bytes: u32) -> Self {
        self.capacity = Some(bytes);
        self
    }

    /// Lets these write it too, besides its owner (one writer per value
    /// otherwise).
    pub fn writers(mut self, writers: &[OwnerId]) -> Self {
        self.writers = Writers::Also(writers.to_vec());
        self
    }

    /// Lets anyone write it.
    pub fn any_writer(mut self) -> Self {
        self.writers = Writers::Any;
        self
    }

    /// The range a write is held to: the soft one unless forced.
    pub(crate) fn range(&self, force: bool) -> Option<(f64, f64)> {
        if force {
            self.hard
        } else {
            self.soft.or(self.hard)
        }
    }
}
