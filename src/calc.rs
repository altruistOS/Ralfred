//! Unit-aware calculator. Parses `5 + 3`, `2 GB in MB`, `5 usd to eur`, `sqrt(16)`.
//! Equivalent of AlfredFeatureCalculator.

#[derive(Debug, Clone, Copy, PartialEq)]
struct Unit {
    cat: Cat,
    scale: f64, // to base unit of category
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Cat {
    Dim,
    Byte,
    Length,
    Time,
    Mass,
    Currency,
}

fn unit(sym: &str) -> Option<Unit> {
    let s = sym.trim().to_lowercase();
    let table = [
        ("b", Cat::Byte, 1.0), ("k", Cat::Byte, 1024.0), ("kb", Cat::Byte, 1024.0), ("mb", Cat::Byte, 1024.0*1024.0), ("gb", Cat::Byte, 1024.0*1024.0*1024.0), ("tb", Cat::Byte, 1024.0*1024.0*1024.0*1024.0),
        ("m", Cat::Length, 1.0), ("km", Cat::Length, 1000.0), ("mm", Cat::Length, 0.001), ("cm", Cat::Length, 0.01), ("mi", Cat::Length, 1609.344), ("ft", Cat::Length, 0.3048),
        ("s", Cat::Time, 1.0), ("sec", Cat::Time, 1.0), ("min", Cat::Time, 60.0), ("h", Cat::Time, 3600.0), ("hour", Cat::Time, 3600.0), ("day", Cat::Time, 86400.0), ("week", Cat::Time, 604800.0), ("year", Cat::Time, 31536000.0),
        ("kg", Cat::Mass, 1.0), ("g", Cat::Mass, 0.001), ("mg", Cat::Mass, 1e-6), ("lb", Cat::Mass, 0.45359237), ("oz", Cat::Mass, 0.0283495),
    ];
    for (k, c, sc) in table {
        if s == k {
            return Some(Unit { cat: c, scale: sc });
        }
    }
    None
}

fn unit_str2(s: &str) -> String {
    s.trim().to_string()
}

fn base_sym_of(cat: Cat) -> Option<&'static str> {
    match cat {
        Cat::Byte => Some("B"),
        Cat::Length => Some("m"),
        Cat::Time => Some("s"),
        Cat::Mass => Some("kg"),
        Cat::Dim | Cat::Currency => None,
    }
}

pub fn calculate(expr: &str) -> Option<String> {
    let expr = expr.trim();
    // unit conversion: "N UNIT in|to UNIT"
    if let Some((lhs, op)) = split_first(expr, ["in", "to", "→"]) {
        let (val_str, unit_str) = split_number_unit(lhs.trim())?;
        let from = unit(&unit_str)?;
        let to = unit(op.trim())?;
        if from.cat != to.cat {
            return None;
        }
        let v: f64 = val_str.replace(',', "").parse().ok()?;
        let conv = v * from.scale / to.scale;
        let mut s = format!("{} {} = {} {}", fmt(v), unit_str.trim(), fmt(conv), unit_str2(op));
        // base-unit form for readability, e.g. " = 1000000000 B"
        if let Some(base_sym) = base_sym_of(from.cat) {
            let base = v * from.scale;
            if base != conv && base >= 1.0 {
                s = format!("{} = {} {}", s, fmt(base), base_sym);
            }
        }
        return Some(s);
    }
    // plain arithmetic with functions
    let v = eval_arith(expr)?;
    Some(fmt(v))
}

pub fn is_calc(expr: &str) -> bool {
    let e = expr.trim();
    if e.is_empty() || !e.chars().next().map(|c| c.is_ascii_digit() || "(".contains(c)).unwrap_or(false) {
        return false;
    }
    calc_tokens(e)
}

fn calc_tokens(e: &str) -> bool {
    let mut seen_op = false;
    let bytes = e.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i] as char;
        match c {
            '0'..='9' | '.' | ',' => {}
            _ if "√sqrt".contains(c) => {}
            c if "a-zA-Z_".contains(c) => return false, // no identifiers allowed except sqrt handled above
            _ => {}
        }
        if "+-*/%^()".contains(c) {
            seen_op = true;
        }
        i += 1;
    }
    seen_op || e.contains('(')
}

fn split_first<'a>(s: &'a str, ops: impl IntoIterator<Item = &'a str>) -> Option<(String, &'a str)> {
    let toks: Vec<&str> = ops.into_iter().collect();
    // find the longest match at any position (not in numbers)
    for len in (1..=12).rev() {
        for t in &toks {
            if t.len() != len {
                continue;
            }
            if let Some(pos) = s.rfind(*t) {
                let (before, after) = s.split_at(pos);
                return Some((before.trim().to_string(), &after[t.len()..].trim()));
            }
        }
    }
    None
}

fn split_number_unit(s: &str) -> Option<(String, String)> {
    let idx = s.find(|c: char| c.is_alphabetic()).unwrap_or(s.len());
    let (num, unit) = s.split_at(idx);
    if num.trim().is_empty() {
        return None;
    }
    Some((num.trim().to_string(), unit.trim().to_string()))
}

fn fmt(v: f64) -> String {
    if v.abs() >= 1e15 || (v.abs() < 1e-10 && v != 0.0) {
        format!("{:e}", v)
    } else if v == v.round() && v.abs() < 1e15 {
        format!("{}", v as i64)
    } else {
        let mut s = format!("{:.6}", v);
        s = s.trim_end_matches('0').trim_end_matches('.').to_string();
        s
    }
}

/// Recursive-descent arithmetic evaluator (+ - * / ^ % sqrt())
fn eval_arith(e: &str) -> Option<f64> {
    let clean: String = e.chars().filter(|c| !c.is_whitespace() && *c != ',').collect();
    let mut p = Parser { s: clean.chars().peekable(), expect_operand: true };
    let v = p.expr()?;
    if !p.peek_eof() {
        return None;
    }
    Some(v)
}

struct Parser<'a> {
    s: std::iter::Peekable<std::str::Chars<'a>>,
    expect_operand: bool,
}
impl<'a> Parser<'a> {
    fn peek_eof(&mut self) -> bool {
        self.s.peek().is_none()
    }
    fn expr(&mut self) -> Option<f64> {
        let mut v = self.term()?;
        while let Some(c) = self.s.peek().copied() {
            if c == '+' || c == '-' {
                self.s.next();
                self.expect_operand = true;
                let t = self.term()?;
                if c == '+' { v += t } else { v -= t }
            } else {
                break;
            }
        }
        Some(v)
    }
    fn term(&mut self) -> Option<f64> {
        let mut v = self.power()?;
        while let Some(c) = self.s.peek().copied() {
            if c == '*' || c == '/' || c == '%' {
                self.s.next();
                self.expect_operand = true;
                let t = self.power()?;
                match c { '*' => v *= t, '/' => { if t == 0.0 { return None } v /= t }, _ => v %= t }
            } else if c == '^' {
                self.s.next();
                self.expect_operand = true;
                let t = self.power()?;
                v = v.powf(t);
            } else {
                break;
            }
        }
        Some(v)
    }
    fn power(&mut self) -> Option<f64> {
        if self.s.peek().copied() == Some('-') {
            self.s.next();
            return Some(-self.power()?);
        }
        if self.s.peek().copied() == Some('+') && self.expect_operand {
            self.s.next();
        }
        let v = self.atom()?;
        Some(v)
    }
    fn atom(&mut self) -> Option<f64> {
        if self.s.peek().copied() == Some('(') {
            self.s.next();
            self.expect_operand = true;
            let v = self.expr()?;
            if self.s.next() != Some(')') { return None }
            self.expect_operand = false;
            return Some(v);
        }
        // sqrt(...)
        let mut word = String::new();
        while let Some(c) = self.s.peek().copied() {
            if c.is_alphabetic() { word.push(c); self.s.next(); } else { break }
        }
        if word.to_lowercase() == "sqrt" {
            if self.s.next() != Some('(') { return None }
            let v = self.expr()?;
            if self.s.next() != Some(')') { return None }
            if v < 0.0 { return None }
            self.expect_operand = false;
            return Some(v.sqrt());
        }
        if !word.is_empty() { return None }
        // number
        let mut num = String::new();
        while let Some(c) = self.s.peek().copied() {
            if c.is_ascii_digit() || c == '.' {
                num.push(c);
                self.s.next();
            } else { break }
        }
        if num.is_empty() { return None }
        self.expect_operand = false;
        num.parse::<f64>().ok()
    }
}
