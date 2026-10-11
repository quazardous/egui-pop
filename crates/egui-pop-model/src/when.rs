//! `when` clauses: where a command applies or a key binds, over named
//! context keys (in the manner of VS Code's `when` clauses).
//!
//! Grammar: `key` (true when the key is set and not `false` or empty),
//! `key == 'text'`, `key != 'text'`, `!expr`, `a && b`, `a || b`,
//! parentheses. `&&` binds tighter than `||`.
//!
//! Each key of a [`WhenContext`] has a depth: 0 for application-wide
//! keys, more for keys set by scopes nearer the focus (ADR 0007).
//!
//! ```
//! use egui_pop_model::when::{When, WhenContext};
//!
//! let cx = WhenContext::new().flag("listFocus", true).with("view", "editor");
//! let w = When::parse("listFocus && view == 'editor'").unwrap();
//! assert!(w.eval(&cx));
//! assert_eq!(w.specificity(), 2);
//! ```

use std::collections::BTreeMap;

use serde::Serialize;

/// The current context: focus, view, selection... as named values, each
/// at the depth of the scope that set it.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct WhenContext(BTreeMap<String, (String, u32)>);

impl WhenContext {
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets a boolean key at depth 0.
    pub fn flag(mut self, key: &str, on: bool) -> Self {
        self.set(key, if on { "true" } else { "false" });
        self
    }

    /// Sets a key at depth 0.
    pub fn with(mut self, key: &str, value: &str) -> Self {
        self.set(key, value);
        self
    }

    /// Sets a key at depth 0.
    pub fn set(&mut self, key: &str, value: &str) {
        self.set_at(key, value, 0);
    }

    /// Sets a key at a depth; a deeper scope setting the same key wins.
    pub fn set_at(&mut self, key: &str, value: &str, depth: u32) {
        self.0.insert(key.into(), (value.into(), depth));
    }

    /// Sets a boolean key at a depth.
    pub fn flag_at(&mut self, key: &str, on: bool, depth: u32) {
        self.set_at(key, if on { "true" } else { "false" }, depth);
    }

    fn truthy(&self, key: &str) -> bool {
        self.0
            .get(key)
            .is_some_and(|(v, _)| !v.is_empty() && v != "false")
    }

    pub fn get(&self, key: &str) -> &str {
        self.0.get(key).map_or("", |(v, _)| v.as_str())
    }

    /// The depth a key was set at, if set.
    pub fn depth_of(&self, key: &str) -> Option<u32> {
        self.0.get(key).map(|(_, d)| *d)
    }

    /// Every key, its value and depth.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &str, u32)> {
        self.0
            .iter()
            .map(|(k, (v, d))| (k.as_str(), v.as_str(), *d))
    }
}

/// A parsed expression.
#[derive(Clone, Debug, PartialEq)]
pub enum When {
    Always,
    Key(String),
    Eq(String, String),
    Ne(String, String),
    Not(Box<When>),
    And(Box<When>, Box<When>),
    Or(Box<When>, Box<When>),
}

#[derive(Clone, Debug, PartialEq)]
enum Token {
    Ident(String),
    Text(String),
    Not,
    And,
    Or,
    Eq,
    Ne,
    Open,
    Close,
}

fn tokens(src: &str) -> Result<Vec<Token>, String> {
    let mut out = Vec::new();
    let mut chars = src.chars().peekable();
    while let Some(&c) = chars.peek() {
        match c {
            ' ' | '\t' => {
                chars.next();
            }
            '(' => {
                chars.next();
                out.push(Token::Open);
            }
            ')' => {
                chars.next();
                out.push(Token::Close);
            }
            '!' => {
                chars.next();
                if chars.peek() == Some(&'=') {
                    chars.next();
                    out.push(Token::Ne);
                } else {
                    out.push(Token::Not);
                }
            }
            '=' | '&' | '|' => {
                chars.next();
                if chars.next() != Some(c) {
                    return Err(format!("`{c}` must be doubled in `{src}`"));
                }
                out.push(match c {
                    '=' => Token::Eq,
                    '&' => Token::And,
                    _ => Token::Or,
                });
            }
            '\'' | '"' => {
                chars.next();
                let mut text = String::new();
                loop {
                    match chars.next() {
                        Some(q) if q == c => break,
                        Some(ch) => text.push(ch),
                        None => return Err(format!("unclosed text in `{src}`")),
                    }
                }
                out.push(Token::Text(text));
            }
            c if c.is_alphanumeric() || c == '_' || c == '.' => {
                let mut ident = String::new();
                while let Some(&c) = chars.peek() {
                    if c.is_alphanumeric() || c == '_' || c == '.' {
                        ident.push(c);
                        chars.next();
                    } else {
                        break;
                    }
                }
                out.push(Token::Ident(ident));
            }
            other => return Err(format!("unexpected `{other}` in `{src}`")),
        }
    }
    Ok(out)
}

struct Parser {
    tokens: Vec<Token>,
    at: usize,
}

impl Parser {
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.at)
    }

    fn next(&mut self) -> Option<Token> {
        let t = self.tokens.get(self.at).cloned();
        self.at += 1;
        t
    }

    fn or(&mut self) -> Result<When, String> {
        let mut left = self.and()?;
        while self.peek() == Some(&Token::Or) {
            self.next();
            left = When::Or(Box::new(left), Box::new(self.and()?));
        }
        Ok(left)
    }

    fn and(&mut self) -> Result<When, String> {
        let mut left = self.unary()?;
        while self.peek() == Some(&Token::And) {
            self.next();
            left = When::And(Box::new(left), Box::new(self.unary()?));
        }
        Ok(left)
    }

    fn unary(&mut self) -> Result<When, String> {
        match self.next() {
            Some(Token::Not) => Ok(When::Not(Box::new(self.unary()?))),
            Some(Token::Open) => {
                let inner = self.or()?;
                match self.next() {
                    Some(Token::Close) => Ok(inner),
                    _ => Err("missing `)`".into()),
                }
            }
            Some(Token::Ident(key)) => match self.peek() {
                Some(Token::Eq) | Some(Token::Ne) => {
                    let eq = self.next() == Some(Token::Eq);
                    match self.next() {
                        Some(Token::Text(t)) | Some(Token::Ident(t)) => Ok(if eq {
                            When::Eq(key, t)
                        } else {
                            When::Ne(key, t)
                        }),
                        _ => Err(format!("`{key}` compared to nothing")),
                    }
                }
                _ => Ok(When::Key(key)),
            },
            other => Err(format!("unexpected {other:?}")),
        }
    }
}

impl When {
    /// Parses an expression; an empty one always applies.
    pub fn parse(src: &str) -> Result<When, String> {
        let tokens = tokens(src)?;
        if tokens.is_empty() {
            return Ok(When::Always);
        }
        let mut p = Parser { tokens, at: 0 };
        let w = p.or()?;
        if p.at != p.tokens.len() {
            return Err(format!("trailing tokens in `{src}`"));
        }
        Ok(w)
    }

    pub fn eval(&self, cx: &WhenContext) -> bool {
        match self {
            When::Always => true,
            When::Key(k) => cx.truthy(k),
            When::Eq(k, v) => cx.get(k) == v,
            When::Ne(k, v) => cx.get(k) != v,
            When::Not(w) => !w.eval(cx),
            When::And(a, b) => a.eval(cx) && b.eval(cx),
            When::Or(a, b) => a.eval(cx) || b.eval(cx),
        }
    }

    /// How specific the expression is: the number of keys it tests. Among
    /// bindings of one layer and depth, the most specific wins.
    pub fn specificity(&self) -> usize {
        match self {
            When::Always => 0,
            When::Key(_) | When::Eq(..) | When::Ne(..) => 1,
            When::Not(w) => w.specificity(),
            When::And(a, b) | When::Or(a, b) => a.specificity() + b.specificity(),
        }
    }

    /// The keys the expression tests.
    pub fn keys(&self) -> Vec<&str> {
        match self {
            When::Always => vec![],
            When::Key(k) | When::Eq(k, _) | When::Ne(k, _) => vec![k.as_str()],
            When::Not(w) => w.keys(),
            When::And(a, b) | When::Or(a, b) => {
                let mut v = a.keys();
                v.extend(b.keys());
                v
            }
        }
    }

    /// How deep in the focus chain the expression applies: the deepest
    /// depth among the keys it tests that are set (0 when none is).
    pub fn depth(&self, cx: &WhenContext) -> u32 {
        self.keys()
            .into_iter()
            .filter_map(|k| cx.depth_of(k))
            .max()
            .unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expressions_evaluate_against_the_context() {
        let cx = WhenContext::new()
            .flag("listFocus", true)
            .flag("textInputFocus", false)
            .with("view", "editor");
        for (src, expected) in [
            ("", true),
            ("listFocus", true),
            ("textInputFocus", false),
            ("!textInputFocus", true),
            ("listFocus && !textInputFocus", true),
            ("view == 'editor'", true),
            ("view != editor", false),
            ("canvasFocus || view == 'editor'", true),
            ("(canvasFocus || listFocus) && view == \"browser\"", false),
            ("missing", false),
        ] {
            assert_eq!(When::parse(src).unwrap().eval(&cx), expected, "{src}");
        }
    }

    #[test]
    fn mistakes_are_named() {
        for bad in ["a & b", "(a", "a ==", "a b", "'open"] {
            assert!(When::parse(bad).is_err(), "{bad}");
        }
        assert_eq!(When::parse("a && !b || c").unwrap().specificity(), 3);
    }

    #[test]
    fn depth_is_the_deepest_key_tested() {
        let mut cx = WhenContext::new().flag("app", true);
        cx.flag_at("dialogFocus", true, 1);
        cx.flag_at("textInputFocus", true, 2);
        assert_eq!(When::parse("app").unwrap().depth(&cx), 0);
        assert_eq!(When::parse("dialogFocus && app").unwrap().depth(&cx), 1);
        assert_eq!(When::parse("textInputFocus").unwrap().depth(&cx), 2);
        assert_eq!(When::parse("").unwrap().depth(&cx), 0);
        assert_eq!(When::parse("unset").unwrap().depth(&cx), 0);
    }
}
