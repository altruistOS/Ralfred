//! Unit-aware calculator. Parses `5 + 3`, `2 GB in MB`, `sqrt(16)` and
//! converts between compatible units for data sizes, lengths, durations,
//! masses and temperatures. Equivalent of AlfredFeatureCalculator.
//!
//! Categories are disjoint, so a conversion across categories (`1 gb to km`)
//! yields no result rather than a wrong number. Currency is deliberately not
//! claimed: a hard-coded rate table would silently go stale.

/// Unit category. Conversion is only possible within one category.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Cat {
    Byte,
    Length,
    Time,
    Mass,
    Temperature,
}

/// One unit of a category. Most categories are linear: `base = value * scale`
/// and `offset` is zero. Temperature is affine (`°F` and `°C` do not share a
/// zero point), so the same pair of constants expresses
/// `base = value * scale + offset`; the inverse is `(base - offset) / scale`.
/// Kelvin is the base unit of `Cat::Temperature`.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Unit {
    cat: Cat,
    scale: f64,
    offset: f64,
}

impl Unit {
    /// Convert a value in this unit into the category's base unit.
    fn to_base(self, v: f64) -> f64 {
        v * self.scale + self.offset
    }
    /// Convert a value in the category's base unit into this unit.
    fn from_base(self, base: f64) -> f64 {
        (base - self.offset) / self.scale
    }
}

fn unit(sym: &str) -> Option<Unit> {
    let s = sym.trim().to_lowercase();
    // affine units carry their own constants
    if let Some(u) = temp_unit(&s) {
        return Some(u);
    }
    let table = [
        ("b", Cat::Byte, 1.0), ("k", Cat::Byte, 1024.0), ("kb", Cat::Byte, 1024.0), ("mb", Cat::Byte, 1024.0*1024.0), ("gb", Cat::Byte, 1024.0*1024.0*1024.0), ("tb", Cat::Byte, 1024.0*1024.0*1024.0*1024.0),
        ("m", Cat::Length, 1.0), ("km", Cat::Length, 1000.0), ("mm", Cat::Length, 0.001), ("cm", Cat::Length, 0.01), ("mi", Cat::Length, 1609.344), ("ft", Cat::Length, 0.3048),
        ("s", Cat::Time, 1.0), ("sec", Cat::Time, 1.0), ("min", Cat::Time, 60.0), ("h", Cat::Time, 3600.0), ("hour", Cat::Time, 3600.0), ("day", Cat::Time, 86400.0), ("week", Cat::Time, 604800.0), ("year", Cat::Time, 31536000.0),
        ("kg", Cat::Mass, 1.0), ("g", Cat::Mass, 0.001), ("mg", Cat::Mass, 1e-6), ("lb", Cat::Mass, 0.45359237), ("oz", Cat::Mass, 0.0283495),
    ];
    for (k, c, sc) in table {
        if s == k {
            return Some(Unit { cat: c, scale: sc, offset: 0.0 });
        }
    }
    None
}

/// Celsius / Fahrenheit / Kelvin as `base(K) = value * scale + offset`.
/// `5/9` and the Fahrenheit offset are written as expressions so the constants
/// agree with each other by construction instead of by transcription.
fn temp_unit(s: &str) -> Option<Unit> {
    let (scale, offset) = match s {
        "c" | "celsius" | "°c" => (1.0, 273.15),
        "f" | "fahrenheit" | "°f" => (5.0 / 9.0, 273.15 - 32.0 * (5.0 / 9.0)),
        "k" | "kelvin" => (1.0, 0.0),
        _ => return None,
    };
    Some(Unit { cat: Cat::Temperature, scale, offset })
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
        Cat::Temperature => Some("K"),
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
        // both units share `cat`, so `from -> base -> to` is well defined even
        // for affine units (temperature), where a plain scale ratio is not.
        let base_v = from.to_base(v);
        let conv = to.from_base(base_v);
        let mut s = format!("{} {} = {} {}", fmt(v), unit_str.trim(), fmt(conv), unit_str2(op));
        // base-unit form for readability, e.g. " = 1000000000 B"
        if let Some(base_sym) = base_sym_of(from.cat) {
            let base = base_v;
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
    // A leading sign is part of the number, so `-5+2` and `+.5*3` are
    // arithmetic rather than a search for the literal text (SPEC §5.4). The
    // sign must be followed by a digit, `.` or `(` or it is not a number.
    let head = e
        .strip_prefix('-')
        .or_else(|| e.strip_prefix('+'))
        .unwrap_or(e);
    let starts_numeric = head
        .chars()
        .next()
        .map(|c| c.is_ascii_digit() || c == '.' || c == '(')
        .unwrap_or(false);
    // `sqrt(...)` is arithmetic the eval_arith parser implements, but it starts
    // with a letter, so the numeric-head test above rejects it. Accept the word
    // explicitly (the token scan already whitelists `sqrt`) — SPEC §5.4.
    let starts_sqrt = head
        .strip_prefix('√')
        .or_else(|| head.strip_prefix("sqrt"))
        .map(|rest| rest.starts_with('('))
        .unwrap_or(false);
    if e.is_empty() || !(starts_numeric || starts_sqrt) {
        return false;
    }
    // A unit conversion (`1 gb to mb`) contains letters, which the arithmetic
    // token scan below rejects, so the router would never reach `calculate`
    // for it (SPEC §5.4). Recognise the conversion shape explicitly.
    if is_conversion(e) {
        return true;
    }
    calc_tokens(e)
}

/// `<number><unit> (to|in|→) <unit>` — the shape `calculate` converts. Both
/// units must be known and in *any* two categories: an incompatible pair is
/// still routed to `calculate` and then yields no result, which is the
/// required refusal rather than a wrong number.
fn is_conversion(e: &str) -> bool {
    let Some((lhs, rhs)) = split_first(e, ["in", "to", "→"]) else {
        return false;
    };
    let Some((value, from)) = split_number_unit(lhs.trim()) else {
        return false;
    };
    value.replace(',', "").parse::<f64>().is_ok() && unit(&from).is_some() && unit(rhs.trim()).is_some()
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
            // The identifier guard must not be written as a literal character
            // class: `"a-zA-Z_"` contains a hyphen, so it used to classify the
            // minus operator as an identifier and reject every subtraction
            // (`1-2` was unreachable via the router). SPEC §5.4.
            c if c.is_ascii_alphabetic() || c == '_' => return false,
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Format a conversion result for comparison: `None` stays `None`, an
    /// `Some` is returned verbatim so assertions can name the exact string.
    fn c(e: &str) -> Option<String> {
        calculate(e)
    }

    #[test]
    fn arithmetic_regression() {
        assert_eq!(c("1+2*3").unwrap(), "7");
        assert_eq!(c("(2+3)*6").unwrap(), "30");
        assert_eq!(c("2^10").unwrap(), "1024");
        assert_eq!(c("sqrt(144) + 10").unwrap(), "22");
        assert_eq!(c("10 % 3").unwrap(), "1");
        assert_eq!(c("-5 + 2").unwrap(), "-3");
        assert_eq!(c("2 + + 3").unwrap(), "5");
        assert!(c("1/0").is_none());
        assert!(c("2 * * 3").is_none());
        assert!(c("1 + abc").is_none());
        assert!(c("(1").is_none());
    }

    #[test]
    fn linear_conversions_regression() {
        let r = c("1 gb to mb").unwrap();
        assert!(r.starts_with("1 gb = 1024 mb"), "got {r}");
        let r = c("3.5km in m").unwrap();
        assert!(r.contains("3500"), "got {r}");
        let r = c("1 hour to s").unwrap();
        assert!(r.contains("3600"), "got {r}");
        // via the alias spelling too
        let r = c("1 lb to g").unwrap();
        assert!(r.contains("453.59237"), "got {r}");
    }

    #[test]
    fn temperature_celsius_to_fahrenheit() {
        assert!(c("100 c to f").unwrap().contains("= 212 f"), "got {:?}", c("100 c to f"));
        assert!(c("0 c to f").unwrap().contains("= 32 f"), "got {:?}", c("0 c to f"));
        assert!(c("-40 c to f").unwrap().contains("= -40 f"), "got {:?}", c("-40 c to f"));
    }

    #[test]
    fn temperature_fahrenheit_to_celsius() {
        assert!(c("32 f to c").unwrap().contains("= 0 c"), "got {:?}", c("32 f to c"));
        assert!(c("212 f to c").unwrap().contains("= 100 c"), "got {:?}", c("212 f to c"));
        assert!(c("-40 f to c").unwrap().contains("= -40 c"), "got {:?}", c("-40 f to c"));
    }

    #[test]
    fn temperature_kelvin_all_directions() {
        assert!(c("0 c to k").unwrap().contains("= 273.15 k"), "got {:?}", c("0 c to k"));
        assert!(c("273.15 k to c").unwrap().contains("= 0 c"), "got {:?}", c("273.15 k to c"));
        assert!(c("0 k to c").unwrap().contains("= -273.15 c"), "got {:?}", c("0 k to c"));
        assert!(c("32 f to k").unwrap().contains("= 273.15 k"), "got {:?}", c("32 f to k"));
        assert!(c("373.15 k to f").unwrap().contains("= 212 f"), "got {:?}", c("373.15 k to f"));
    }

    #[test]
    fn temperature_aliases() {
        assert!(c("100 celsius to fahrenheit").unwrap().contains("212"));
        assert!(c("100 C to K").unwrap().contains("373.15"));
        assert!(unit("F").map(|u| u.cat) == Some(Cat::Temperature));
        assert!(unit("kelvin").is_some());
    }

    /// The affine path must not degrade to a scale ratio.
    #[test]
    fn temperature_is_not_a_scale_ratio() {
        // 100 C is 212 F, not 100 * (1/scale-of-F)
        let r = c("100 c to f").unwrap();
        assert!(!r.contains("= 180 f"), "scale-ratio leak: {r}");
    }

    #[test]
    fn incompatible_dimensions_yield_nothing() {
        assert!(c("1 gb to km").is_none());
        assert!(c("1 km to gb").is_none());
        assert!(c("1 m to s").is_none());
        assert!(c("1 kg to m").is_none());
        assert!(c("1 c to m").is_none());
        assert!(c("1 c to gb").is_none());
        assert!(c("1 s to kg").is_none());
        assert!(c("1 f to kg").is_none());
        assert!(c("1 kb to kg").is_none());
    }

    #[test]
    fn currency_is_not_claimed() {
        // SPEC §5.4 lists data sizes, lengths, masses, temperatures and
        // durations — not currency. The module doc must not advertise it and
        // the calculator must not invent rates.
        assert!(c("5 usd to eur").is_none());
        assert!(c("5 USD to EUR").is_none());
        assert!(unit("usd").is_none());
        assert!(unit("eur").is_none());
    }

    #[test]
    fn malformed_conversion_inputs() {
        assert!(c("1 to m").is_none());
        assert!(c("gb to mb").is_none());
        assert!(c("1 gb to").is_none());
        assert!(c("to m").is_none());
        assert!(c("c to f").is_none());
        assert!(c("").is_none());
    }

    #[test]
    fn detector_regression() {
        assert!(is_calc("1+2"));
        assert!(is_calc("(3)*4"));
        assert!(!is_calc("safari"));
        assert!(!is_calc("g query"));
    }

    /// The detector must route unit conversions to `calculate`; without this
    /// the panel never reaches the conversion path at all (SPEC §5.4).
    #[test]
    fn detector_recognises_unit_conversions() {
        assert!(is_calc("1 gb to mb"));
        assert!(is_calc("2.5km in m"));
        assert!(is_calc("100 c to f"));
        assert!(is_calc("1 hour to s"));
        // a syntactically valid but incompatible conversion is still routed:
        // calculate refuses it, which is the required behaviour
        assert!(is_calc("1 gb to km"));
        assert!(calculate("1 gb to km").is_none());
        // unknown units (and therefore currency) are not conversions
        assert!(!is_calc("5 usd to eur"));
        assert!(!is_calc("1 thing to do"));
        assert!(!is_calc("safari"));
    }

    /// SPEC §5.4 — subtraction, a signed leading number, and `sqrt(...)` must
    /// all be recognised by the detector *and* evaluated by the parser. The
    /// detector used to classify `-` as an identifier (a literal `"a-zA-Z_"`
    /// character class contains a hyphen), so no subtraction ever reached the
    /// calculator, and `sqrt(...)` was rejected because it starts with a letter.
    #[test]
    fn detector_and_parser_cover_subtraction_signed_and_sqrt() {
        for (expr, want) in [
            ("1-2", "-1"),
            ("10 - 4", "6"),
            ("-5+2", "-3"),
            ("+.5*3", "1.5"),
            ("sqrt(9)", "3"),
            ("sqrt(144)", "12"),
        ] {
            assert!(is_calc(expr), "{expr} must be detected as arithmetic");
            assert_eq!(
                calculate(expr).as_deref(),
                Some(want),
                "{expr} must evaluate to {want}"
            );
        }
        // A leading sign only starts a number when something numeric follows.
        assert!(!is_calc("- 5"));
        assert!(!is_calc("-safari"));
        // Identifier words stay searches.
        assert!(!is_calc("foo"));
        assert!(!is_calc("sqrtx(9)"));
    }

    #[test]
    fn base_unit_reported_once() {
        // converting to the base unit must not print the same number twice
        let r = c("1 gb to b").unwrap();
        assert_eq!(r.matches("1073741824").count(), 1, "got {r}");
        let r = c("100 c to k").unwrap();
        assert_eq!(r.matches("373.15").count(), 1, "got {r}");
    }

    #[test]
    fn unit_lookup_is_case_and_space_insensitive() {
        assert_eq!(unit("  GB  ").map(|u| u.cat), Some(Cat::Byte));
        assert_eq!(unit("KM").map(|u| u.cat), Some(Cat::Length));
        assert_eq!(unit("Hour").map(|u| u.cat), Some(Cat::Time));
        assert!(unit("nope").is_none());
    }
}
