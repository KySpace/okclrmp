//! A small, deliberately closed Julia-like scalar expression language.
//! No host-language evaluation, assignments, IO, or arbitrary code execution.
use std::collections::BTreeMap;

#[derive(Clone, Debug)]
pub enum Expr {
    Number(f64),
    Param,
    Unary(char, Box<Expr>),
    Binary(String, Box<Expr>, Box<Expr>),
    If(Box<Expr>, Box<Expr>, Box<Expr>),
    Call(String, Vec<Expr>),
}

impl Expr {
    pub fn eval(&self, t: f64) -> f64 {
        match self {
            Self::Number(v) => *v,
            Self::Param => t,
            Self::Unary(op, a) => match op {
                '-' => -a.eval(t),
                '!' => truth(!yes(a.eval(t))),
                _ => a.eval(t),
            },
            Self::If(c, a, b) => {
                if yes(c.eval(t)) {
                    a.eval(t)
                } else {
                    b.eval(t)
                }
            }
            Self::Binary(op, a, b) => {
                let a = a.eval(t);
                if op == "&&" && !yes(a) {
                    return 0.;
                }
                if op == "||" && yes(a) {
                    return 1.;
                }
                let b = b.eval(t);
                match op.as_str() {
                    "+" => a + b,
                    "-" => a - b,
                    "*" => a * b,
                    "/" => a / b,
                    "%" => a % b,
                    "^" => a.powf(b),
                    "<" => truth(a < b),
                    "<=" => truth(a <= b),
                    ">" => truth(a > b),
                    ">=" => truth(a >= b),
                    "==" => truth(a == b),
                    "!=" => truth(a != b),
                    "&&" | "||" => truth(yes(b)),
                    _ => f64::NAN,
                }
            }
            Self::Call(name, args) => {
                let a = args[0].eval(t);
                let b = || args[1].eval(t);
                match name.as_str() {
                    "abs" => a.abs(),
                    "sin" => a.sin(),
                    "cos" => a.cos(),
                    "tan" => a.tan(),
                    "sind" => a.to_radians().sin(),
                    "cosd" => a.to_radians().cos(),
                    "sqrt" => a.sqrt(),
                    "exp" => a.exp(),
                    "log" | "ln" => a.ln(),
                    "log10" => a.log10(),
                    "floor" => a.floor(),
                    "ceil" => a.ceil(),
                    "round" => a.round_ties_even(),
                    "sign" => {
                        if a == 0. {
                            a
                        } else {
                            a.signum()
                        }
                    }
                    "deg2rad" => a.to_radians(),
                    "rad2deg" => a.to_degrees(),
                    "min" => a.min(b()),
                    "max" => a.max(b()),
                    "atan2" => a.atan2(b()),
                    "mod" => a.rem_euclid(b()),
                    "clamp" => {
                        let lo = b();
                        let hi = args[2].eval(t);
                        if lo <= hi { a.clamp(lo, hi) } else { f64::NAN }
                    }
                    _ => f64::NAN,
                }
            }
        }
    }
}
fn yes(v: f64) -> bool {
    v != 0. && v.is_finite()
}
fn truth(b: bool) -> f64 {
    if b { 1. } else { 0. }
}
fn arity(name: &str) -> Option<usize> {
    match name {
        "abs" | "sin" | "cos" | "tan" | "sind" | "cosd" | "sqrt" | "exp" | "log" | "ln"
        | "log10" | "floor" | "ceil" | "round" | "sign" | "deg2rad" | "rad2deg" => Some(1),
        "min" | "max" | "atan2" | "mod" => Some(2),
        "clamp" => Some(3),
        _ => None,
    }
}
pub fn identifier(s: &str) -> bool {
    let mut c = s.chars();
    c.next().is_some_and(|v| v == '_' || v.is_alphabetic())
        && c.all(|v| v == '_' || v.is_alphanumeric())
}
pub fn reserved(s: &str) -> bool {
    matches!(s, "pi" | "π" | "e" | "true" | "false") || arity(s).is_some()
}

#[derive(Clone, Debug, PartialEq)]
enum Token {
    Num(f64),
    Id(String),
    Op(String),
    End,
}
fn lex(s: &str) -> Result<Vec<Token>, String> {
    if s.len() > 4096 {
        return Err("Expression is too long (4096 characters maximum)".into());
    }
    let mut tokens = Vec::new();
    let mut i = 0;
    while i < s.len() {
        let c = s[i..].chars().next().unwrap();
        if c.is_whitespace() {
            i += c.len_utf8();
            continue;
        }
        if c.is_ascii_digit() || c == '.' {
            let start = i;
            i += 1;
            while i < s.len() && (s.as_bytes()[i].is_ascii_digit() || s.as_bytes()[i] == b'.') {
                i += 1;
            }
            if i < s.len() && matches!(s.as_bytes()[i], b'e' | b'E') {
                i += 1;
                if i < s.len() && matches!(s.as_bytes()[i], b'+' | b'-') {
                    i += 1;
                }
                while i < s.len() && s.as_bytes()[i].is_ascii_digit() {
                    i += 1;
                }
            }
            let n = s[start..i]
                .parse::<f64>()
                .map_err(|_| format!("Invalid number: {}", &s[start..i]))?;
            if !n.is_finite() {
                return Err("Number must be finite".into());
            }
            tokens.push(Token::Num(n));
            continue;
        }
        if c.is_alphabetic() || c == '_' {
            let start = i;
            i += c.len_utf8();
            while i < s.len() {
                let n = s[i..].chars().next().unwrap();
                if n.is_alphanumeric() || n == '_' {
                    i += n.len_utf8();
                } else {
                    break;
                }
            }
            tokens.push(Token::Id(s[start..i].into()));
            continue;
        }
        let op = if let Some(op) = ["<=", ">=", "==", "!=", "&&", "||"]
            .iter()
            .find(|op| s[i..].starts_with(**op))
        {
            op.to_string()
        } else if "+-*/%^!<>?:(),".contains(c) {
            c.to_string()
        } else {
            return Err(format!("Unexpected character ‘{c}’"));
        };
        i += op.len();
        tokens.push(Token::Op(op));
    }
    tokens.push(Token::End);
    Ok(tokens)
}
struct Parser<'a> {
    tokens: Vec<Token>,
    i: usize,
    param: &'a str,
    constants: &'a BTreeMap<String, f64>,
    depth: usize,
}
impl Parser<'_> {
    fn eat(&mut self, op: &str) -> bool {
        if self.tokens[self.i] == Token::Op(op.into()) {
            self.i += 1;
            true
        } else {
            false
        }
    }
    fn expect(&mut self, op: &str) -> Result<(), String> {
        if self.eat(op) {
            Ok(())
        } else {
            Err(format!("Expected ‘{op}’"))
        }
    }
    fn expr(&mut self, min: u8) -> Result<Expr, String> {
        self.depth += 1;
        if self.depth > 64 {
            return Err("Expression nesting is too deep".into());
        }
        let token = self.tokens[self.i].clone();
        self.i += 1;
        let mut left = match token {
            Token::Num(v) => Expr::Number(v),
            Token::Id(name) => {
                if self.eat("(") {
                    let expected =
                        arity(&name).ok_or_else(|| format!("Unknown function ‘{name}’"))?;
                    let mut args = vec![];
                    if !self.eat(")") {
                        loop {
                            args.push(self.expr(0)?);
                            if self.eat(")") {
                                break;
                            }
                            self.expect(",")?;
                        }
                    }
                    if args.len() != expected {
                        return Err(format!("{name} expects {expected} argument(s)"));
                    }
                    Expr::Call(name, args)
                } else if name == self.param {
                    Expr::Param
                } else if let Some(v) = self.constants.get(&name) {
                    Expr::Number(*v)
                } else {
                    Expr::Number(match name.as_str() {
                        "pi" | "π" => std::f64::consts::PI,
                        "e" => std::f64::consts::E,
                        "true" => 1.,
                        "false" => 0.,
                        _ => {
                            return Err(format!(
                                "Unknown ‘{name}’. Use only ‘{}’ and defined constants.",
                                self.param
                            ));
                        }
                    })
                }
            }
            Token::Op(op) if op == "(" => {
                let e = self.expr(0)?;
                self.expect(")")?;
                e
            }
            Token::Op(op) if matches!(op.as_str(), "+" | "-" | "!") => {
                Expr::Unary(op.chars().next().unwrap(), Box::new(self.expr(13)?))
            }
            _ => return Err("Expected a number, parameter, or function".into()),
        };
        while let Token::Op(op) = &self.tokens[self.i] {
            let op = op.clone();
            if op == "?" && min == 0 {
                self.i += 1;
                let a = self.expr(0)?;
                self.expect(":")?;
                let b = self.expr(0)?;
                left = Expr::If(Box::new(left), Box::new(a), Box::new(b));
                continue;
            }
            let (lb, rb) = match op.as_str() {
                "||" => (1, 2),
                "&&" => (3, 4),
                "==" | "!=" => (5, 6),
                "<" | ">" | "<=" | ">=" => (7, 8),
                "+" | "-" => (9, 10),
                "*" | "/" | "%" => (11, 12),
                "^" => (14, 13),
                _ => break,
            };
            if lb < min {
                break;
            }
            self.i += 1;
            let r = self.expr(rb)?;
            left = Expr::Binary(op, Box::new(left), Box::new(r));
        }
        self.depth -= 1;
        Ok(left)
    }
}
pub fn compile(s: &str, param: &str, constants: &BTreeMap<String, f64>) -> Result<Expr, String> {
    if !identifier(param) || reserved(param) || constants.contains_key(param) {
        return Err("Choose a parameter name distinct from constants and functions".into());
    }
    let mut p = Parser {
        tokens: lex(s)?,
        i: 0,
        param,
        constants,
        depth: 0,
    };
    let e = p.expr(0)?;
    if p.tokens[p.i] != Token::End {
        return Err("Unexpected trailing input. Use * for multiplication.".into());
    }
    Ok(e)
}
pub fn constants(s: &str, param: &str) -> Result<BTreeMap<String, f64>, String> {
    let mut values = BTreeMap::new();
    for entry in s
        .split(['\n', ';'])
        .map(str::trim)
        .filter(|v| !v.is_empty() && !v.starts_with('#'))
    {
        let (name, source) = entry
            .split_once('=')
            .ok_or("Constants use name = value, separated by ; or newlines")?;
        let name = name.trim();
        if !identifier(name) || reserved(name) || name == param || values.contains_key(name) {
            return Err(format!("Invalid or duplicate constant ‘{name}’"));
        }
        // An impossible parameter identifier prohibits varying constants.
        let mut p = Parser {
            tokens: lex(source)?,
            i: 0,
            param: "",
            constants: &values,
            depth: 0,
        };
        let v = p.expr(0)?.eval(0.);
        if p.tokens[p.i] != Token::End || !v.is_finite() {
            return Err(format!("Constant ‘{name}’ must be a finite scalar"));
        }
        values.insert(name.into(), v);
    }
    Ok(values)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn eval(s: &str, t: f64) -> f64 {
        compile(s, "t", &BTreeMap::new()).unwrap().eval(t)
    }
    #[test]
    fn julia_precedence_and_conditions() {
        assert_eq!(eval("-2^2", 0.), -4.);
        assert_eq!(eval("2^3^2", 0.), 512.);
        assert_eq!(eval("t < 0.5 ? 15 : 263", 0.3), 15.);
        assert_eq!(eval("t < 0.5 ? 15 : 263", 0.5), 263.);
        assert_eq!(eval("t > 0 && t < 1 ? abs(t - 0.5) : 9", 0.2), 0.3);
        assert_eq!(eval("1 > 2 ? sqrt(-1) : 3", 0.), 3.);
    }
    #[test]
    fn names_constants_and_errors() {
        let vars = constants("c_max=0.3; l_min = 0.2; l_range=1-l_min", "v").unwrap();
        assert_eq!(
            compile("abs(v-.5)*c_max", "v", &vars).unwrap().eval(0.),
            0.15
        );
        for source in [
            "other+1", "sin(1,2)", "t=1", "2t", "(1", "sin()", "1+", "π; 2", "1 × 2", "1💡",
        ] {
            assert!(compile(source, "t", &vars).is_err(), "{source}");
        }
        assert!(constants("a=t+1", "t").is_err());
        assert!(constants("t=2", "t").is_err());
        assert!(compile("1", "pi", &vars).is_err());
    }
    #[test]
    fn builtins() {
        assert!((eval("sind(90) + cos(pi)", 0.)).abs() < 1e-12);
        assert_eq!(eval("mod(-10,360)", 0.), 350.);
        assert_eq!(eval("sign(0)", 0.), 0.);
        assert_eq!(eval("round(2.5)", 0.), 2.);
        assert_eq!(eval("round(3.5)", 0.), 4.);
    }
}
