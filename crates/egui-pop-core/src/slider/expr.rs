//! What a slider's typed entry accepts: a number or a simple expression,
//! written as the user's locale writes numbers, with an optional unit
//! after it.
//!
//! - Numbers take a point or a comma as the decimal separator; when a
//!   text has both, the last one is the decimal separator and the other
//!   groups thousands (`1,234.5`, `1.234,5`). Spaces, no-break spaces and
//!   apostrophes before three digits group thousands (`1 234,5`,
//!   `1’234.5`).
//! - Operators: `+ - * /`, `^` (power, right-associative), parentheses,
//!   a sign before any term. `x` is the slider's current value.
//! - What follows the expression is a unit and is ignored (`1.5 EV`,
//!   `+3%`); a text that starts with neither a number, a sign, `x` nor a
//!   parenthesis is not a value.
//!
//! ```
//! use egui_pop_core::slider::expr::eval;
//!
//! assert_eq!(eval("1,5 EV", 0.0), Some(1.5));
//! assert_eq!(eval("x * 2", 0.25), Some(0.5));
//! assert_eq!(eval("(1 + 2) ^ 2 - 1", 0.0), Some(8.0));
//! assert_eq!(eval("1 234,5", 0.0), Some(1234.5));
//! assert_eq!(eval("EV", 0.0), None);
//! assert_eq!(eval("1/0", 0.0), None);
//! ```

/// The value of `text`, `x` being `current`; `None` when it is not a
/// value or not finite.
pub fn eval(text: &str, current: f64) -> Option<f64> {
    let normal = normalise(text);
    let mut p = Parser {
        s: normal.as_bytes(),
        at: 0,
        x: current,
    };
    let v = p.expr()?;
    p.space();
    // Whatever is left is a unit: it must not look like more expression.
    match p.peek() {
        None => {}
        Some(c) if c.is_ascii_digit() || b"+-*/^(.".contains(&c) => return None,
        Some(_) => {}
    }
    v.is_finite().then_some(v)
}

/// The text with one decimal separator (`.`) and no group separators.
fn normalise(text: &str) -> String {
    let t = text.trim();
    let (point, comma) = (t.rfind('.'), t.rfind(','));
    // The decimal separator is the last of `.` and `,`; the other groups.
    let (decimal, group) = match (point, comma) {
        (Some(p), Some(c)) if c > p => (',', '.'),
        (Some(_), Some(_)) => ('.', ','),
        (None, Some(_)) => (',', '.'),
        _ => ('.', ','),
    };
    let chars: Vec<char> = t.chars().collect();
    let mut out = String::with_capacity(t.len());
    for (i, &c) in chars.iter().enumerate() {
        // A group separator sits between a digit and exactly three.
        let digit_before = i > 0 && chars[i - 1].is_ascii_digit();
        let three_after = chars.len() > i + 3
            && chars[i + 1..=i + 3].iter().all(char::is_ascii_digit)
            && !chars.get(i + 4).is_some_and(char::is_ascii_digit);
        let grouping = matches!(c, ' ' | '\u{a0}' | '\u{202f}' | '\'' | '’');
        if c == decimal {
            out.push('.');
        } else if (c == group || grouping) && digit_before && three_after {
            // A group separator between digits: dropped.
        } else {
            out.push(c);
        }
    }
    out
}

struct Parser<'a> {
    s: &'a [u8],
    at: usize,
    x: f64,
}

impl Parser<'_> {
    fn space(&mut self) {
        while self.peek().is_some_and(|c| c.is_ascii_whitespace()) {
            self.at += 1;
        }
    }

    fn peek(&self) -> Option<u8> {
        self.s.get(self.at).copied()
    }

    /// The next byte after spaces, consumed when it is one of `ops`.
    fn op(&mut self, ops: &[u8]) -> Option<u8> {
        let save = self.at;
        self.space();
        match self.peek() {
            Some(c) if ops.contains(&c) => {
                self.at += 1;
                Some(c)
            }
            _ => {
                self.at = save;
                None
            }
        }
    }

    fn expr(&mut self) -> Option<f64> {
        let mut v = self.term()?;
        while let Some(op) = self.op(b"+-") {
            let rhs = self.term()?;
            v = if op == b'+' { v + rhs } else { v - rhs };
        }
        Some(v)
    }

    fn term(&mut self) -> Option<f64> {
        let mut v = self.unary()?;
        while let Some(op) = self.op(b"*/") {
            let rhs = self.unary()?;
            v = if op == b'*' { v * rhs } else { v / rhs };
        }
        Some(v)
    }

    fn unary(&mut self) -> Option<f64> {
        match self.op(b"+-") {
            Some(b'-') => Some(-self.unary()?),
            Some(_) => self.unary(),
            None => self.power(),
        }
    }

    fn power(&mut self) -> Option<f64> {
        let base = self.atom()?;
        if self.op(b"^").is_some() {
            let exp = self.unary()?;
            return Some(base.powf(exp));
        }
        Some(base)
    }

    fn atom(&mut self) -> Option<f64> {
        self.space();
        match self.peek()? {
            b'(' => {
                self.at += 1;
                let v = self.expr()?;
                self.op(b")")?;
                Some(v)
            }
            b'x' | b'X' => {
                self.at += 1;
                Some(self.x)
            }
            c if c.is_ascii_digit() || c == b'.' => {
                let start = self.at;
                while self.peek().is_some_and(|c| c.is_ascii_digit() || c == b'.') {
                    self.at += 1;
                }
                std::str::from_utf8(&self.s[start..self.at])
                    .ok()?
                    .parse()
                    .ok()
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::eval;

    #[test]
    fn numbers_in_any_locale() {
        assert_eq!(eval("1,5 EV", 0.0), Some(1.5));
        assert_eq!(eval(" -0.25", 0.0), Some(-0.25));
        assert_eq!(eval("+3%", 0.0), Some(3.0));
        assert_eq!(eval("1,234.5", 0.0), Some(1234.5));
        assert_eq!(eval("1.234,5", 0.0), Some(1234.5));
        assert_eq!(eval("1\u{202f}234,5", 0.0), Some(1234.5));
        assert_eq!(eval("12’000 K", 0.0), Some(12000.0));
        assert_eq!(eval("EV", 0.0), None);
        assert_eq!(eval("", 0.0), None);
    }

    #[test]
    fn expressions() {
        assert_eq!(eval("1 + 2 * 3", 0.0), Some(7.0));
        assert_eq!(eval("(1 + 2) * 3", 0.0), Some(9.0));
        assert_eq!(eval("2 ^ 3 ^ 2", 0.0), Some(512.0));
        assert_eq!(eval("-2 ^ 2", 0.0), Some(-4.0));
        assert_eq!(eval("x / 2 EV", 3.0), Some(1.5));
        assert_eq!(eval("x - 1", 0.5), Some(-0.5));
        assert_eq!(eval("1 / 0", 0.0), None);
        assert_eq!(eval("(1 + 2", 0.0), None);
        assert_eq!(eval("2 3", 0.0), None);
    }
}
