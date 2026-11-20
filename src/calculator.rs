use std::collections::BTreeMap;
use std::f64::consts::{E, PI};

type CalcResult<T> = Result<T, String>;

#[derive(Default)]
pub(crate) struct Calculator {
    variables: BTreeMap<String, f64>,
    answer: Option<f64>,
    pub(crate) degrees: bool,
}

impl Calculator {
    pub(crate) fn preview(&self, input: &str) -> CalcResult<f64> {
        self.evaluate(input).map(|(_, value)| value)
    }

    pub(crate) fn submit(&mut self, input: &str) -> CalcResult<f64> {
        let (assignment, value) = self.evaluate(input)?;
        if let Some(name) = assignment {
            self.variables.insert(name.to_owned(), value);
        }
        self.answer = Some(value);
        Ok(value)
    }

    pub(crate) fn values(&self) -> impl Iterator<Item = (&str, f64)> {
        self.answer.map(|value| ("ans", value)).into_iter().chain(
            self.variables
                .iter()
                .map(|(name, &value)| (name.as_str(), value)),
        )
    }

    fn evaluate<'a>(&self, input: &'a str) -> CalcResult<(Option<&'a str>, f64)> {
        let (assignment, expression) = match input.split_once('=') {
            Some((name, expression)) => {
                let name = name.trim();
                let mut bytes = name.bytes();
                if !bytes.next().is_some_and(identifier_start) || !bytes.all(identifier_byte) {
                    return Err("Assignment needs a name, e.g. x = 54".into());
                }
                if matches!(name, "pi" | "e" | "ans") {
                    return Err(format!("'{name}' is reserved"));
                }
                (Some(name), expression)
            }
            None => (None, input),
        };
        let mut parser = Parser {
            rest: expression,
            calculator: self,
        };
        let value = parser.expression(0, 0)?;
        if parser.peek().is_some() {
            return Err("Expected an operator or the end of the expression".into());
        }
        Ok((assignment, value))
    }

    fn lookup(&self, name: &str) -> CalcResult<f64> {
        match name {
            "pi" => Some(PI),
            "e" => Some(E),
            "ans" => return self.answer.ok_or_else(|| "No previous result yet".into()),
            _ => self.variables.get(name).copied(),
        }
        .ok_or_else(|| format!("Unknown variable '{name}'"))
    }
}

struct Parser<'a> {
    rest: &'a str,
    calculator: &'a Calculator,
}

impl Parser<'_> {
    fn peek(&mut self) -> Option<u8> {
        self.rest = self.rest.trim_start();
        self.rest.bytes().next()
    }

    fn expression(&mut self, min_precedence: u8, depth: u8) -> CalcResult<f64> {
        if depth == 64 {
            return Err("Expression is nested too deeply".into());
        }
        let mut left = match self.peek() {
            Some(sign @ (b'+' | b'-')) => {
                self.rest = &self.rest[1..];
                let value = self.expression(5, depth + 1)?;
                if sign == b'-' { -value } else { value }
            }
            Some(b'(') => self.group(depth)?,
            Some(b'.' | b'0'..=b'9') => self.number()?,
            Some(byte) if identifier_start(byte) => self.named_value(depth)?,
            _ => return Err("Expected a number, variable, or '('".into()),
        };

        while let Some(op) = self.peek() {
            // Equal binding powers make ^ right-associative.
            let (precedence, right_precedence) = match op {
                b'+' | b'-' => (1, 2),
                b'*' | b'/' | b'%' => (3, 4),
                b'^' => (6, 6),
                _ => break,
            };
            if precedence < min_precedence {
                break;
            }
            self.rest = &self.rest[1..];
            let right = self.expression(right_precedence, depth + 1)?;
            left = finite(match op {
                b'+' => left + right,
                b'-' => left - right,
                b'*' => left * right,
                b'/' | b'%' if right == 0.0 => return Err("Division by zero".into()),
                b'/' => left / right,
                b'%' => left.rem_euclid(right),
                b'^' => left.powf(right),
                _ => unreachable!(),
            })?;
        }
        Ok(left)
    }

    fn group(&mut self, depth: u8) -> CalcResult<f64> {
        self.rest = &self.rest[1..];
        let value = self.expression(0, depth + 1)?;
        self.rest = self.rest.strip_prefix(')').ok_or("Expected ')'")?;
        Ok(value)
    }

    fn named_value(&mut self, depth: u8) -> CalcResult<f64> {
        let length = self
            .rest
            .bytes()
            .take_while(|&b| identifier_byte(b))
            .count();
        let (name, rest) = self.rest.split_at(length);
        self.rest = rest;
        if self.peek() != Some(b'(') {
            return self.calculator.lookup(name);
        }
        let function = match name {
            "sin" => f64::sin,
            "cos" => f64::cos,
            "tan" => f64::tan,
            "asin" => f64::asin,
            "acos" => f64::acos,
            "atan" => f64::atan,
            "sqrt" => f64::sqrt,
            "abs" => f64::abs,
            "ln" => f64::ln,
            "log" | "log10" => f64::log10,
            "exp" => f64::exp,
            "round" => f64::round,
            "floor" => f64::floor,
            "ceil" => f64::ceil,
            _ => return Err(format!("Unknown function '{name}'")),
        };
        let value = self.group(depth)?;
        let degrees = self.calculator.degrees;
        finite(match name {
            "sin" | "cos" | "tan" => {
                let angle = if degrees { value.to_radians() } else { value };
                // pi is inexact, so sin(pi) is ~1e-16: a result within the angle's rounding error
                // is zero. The cap keeps huge angles, whose error is not small, from collapsing.
                let noise = (angle.abs() * f64::EPSILON).min(1e-9);
                let exact = |result: f64| if result.abs() <= noise { 0.0 } else { result };
                match name {
                    "tan" => exact(angle.sin()) / exact(angle.cos()),
                    _ => exact(function(angle)),
                }
            }
            "asin" | "acos" | "atan" if degrees => function(value).to_degrees(),
            _ => function(value),
        })
    }

    fn number(&mut self) -> CalcResult<f64> {
        let start = self.rest;
        self.skip_digits();
        // Results are shown as 1,234.5, so the same grouping is accepted back.
        while let Some(group) = self.rest.strip_prefix(',') {
            self.rest = group;
            self.skip_digits();
            if group.len() - self.rest.len() != 3 {
                return Err("Group digits in threes, e.g. 1,234.5".into());
            }
        }
        if let Some(rest) = self.rest.strip_prefix('.') {
            self.rest = rest;
            self.skip_digits();
        }
        if let Some(rest) = self.rest.strip_prefix(['e', 'E']) {
            self.rest = rest.strip_prefix(['+', '-']).unwrap_or(rest);
            self.skip_digits();
        }
        let literal = &start[..start.len() - self.rest.len()];
        finite(
            literal
                .replace(',', "")
                .parse()
                .map_err(|_| format!("Invalid number '{literal}'"))?,
        )
    }

    fn skip_digits(&mut self) {
        let length = self.rest.bytes().take_while(u8::is_ascii_digit).count();
        self.rest = &self.rest[length..];
    }
}

fn identifier_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

fn identifier_byte(byte: u8) -> bool {
    identifier_start(byte) || byte.is_ascii_digit()
}

fn finite(value: f64) -> CalcResult<f64> {
    if value.is_finite() {
        Ok(value)
    } else {
        Err("Result is not a finite real number".into())
    }
}

pub(crate) fn format_number(value: f64) -> String {
    if value == 0.0 {
        return "0".into();
    }
    let magnitude = value.abs();
    if !(1e-6..1e16).contains(&magnitude) {
        let scientific = format!("{value:.14e}");
        if let Some((mantissa, exponent)) = scientific.split_once('e') {
            return format!("{}e{exponent}", trim_fraction(mantissa));
        }
        return scientific;
    }

    let decimal = if magnitude.fract() == 0.0 || magnitude >= 1e14 {
        magnitude.to_string()
    } else {
        let decimals = (14 - magnitude.log10().floor() as i32) as usize;
        format!("{magnitude:.decimals$}")
    };
    let decimal = trim_fraction(&decimal);
    let (whole, fraction) = decimal.split_once('.').unwrap_or((decimal, ""));
    let mut grouped = String::new();
    if value.is_sign_negative() {
        grouped.push('-');
    }
    for (index, digit) in whole.chars().enumerate() {
        if index != 0 && (whole.len() - index).is_multiple_of(3) {
            grouped.push(',');
        }
        grouped.push(digit);
    }
    if !fraction.is_empty() {
        grouped.push('.');
        grouped.push_str(fraction);
    }
    grouped
}

fn trim_fraction(text: &str) -> &str {
    if text.contains('.') {
        text.trim_end_matches('0').trim_end_matches('.')
    } else {
        text
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evaluates_arithmetic_and_scientific_functions() {
        let calc = Calculator::default();
        for (input, expected) in [
            ("2 + 3 * (4 - 1)^2", 29.0),
            ("2^3^2 - -2^2", 516.0),
            ("2^-3 + 16 / 4 % 3", 1.125),
            ("-17 % 5", 3.0),
            ("sqrt(81) + sin(pi/2) + cos(0) + tan(0)", 11.0),
            ("1.5e-2 + .25", 0.265),
        ] {
            assert!(
                (calc.preview(input).unwrap() - expected).abs() < 1e-12,
                "{input}"
            );
        }
    }

    #[test]
    fn accepts_digit_grouping_only_in_threes() {
        let calc = Calculator::default();
        assert_eq!(calc.preview("1,234.5 + 1,000,000"), Ok(1_001_234.5));
        for input in ["3,5", "1,2345", "1,,234", "1,234,", "1.5,000"] {
            assert!(calc.preview(input).is_err(), "{input}");
        }
    }

    #[test]
    fn degree_mode_converts_only_trig_inputs_and_inverse_trig_results() {
        let calc = Calculator {
            degrees: true,
            ..Default::default()
        };
        for (input, expected) in [
            ("sin(30)", 0.5),
            ("cos(60)", 0.5),
            ("tan(45)", 1.0),
            ("asin(0.5)", 30.0),
            ("acos(0.5)", 60.0),
            ("atan(1)", 45.0),
            ("sqrt(81) + pi", 9.0 + PI),
        ] {
            assert!(
                (calc.preview(input).unwrap() - expected).abs() < 1e-12,
                "{input}"
            );
        }
    }

    #[test]
    fn trig_is_exact_where_binary_pi_is_not() {
        let mut calc = Calculator::default();
        for input in ["sin(pi)", "cos(pi/2)", "tan(pi)", "sin(1000*pi)"] {
            assert_eq!(calc.preview(input), Ok(0.0), "{input}");
        }
        assert_eq!(calc.preview("sin(1e-20)"), Ok(1e-20));
        assert_eq!(calc.preview("sin(1e16)"), Ok(1e16_f64.sin()));
        assert!(calc.preview("tan(pi/2)").is_err());
        calc.degrees = true;
        for input in ["sin(180)", "cos(90)", "cos(270)", "tan(360)"] {
            assert_eq!(calc.preview(input), Ok(0.0), "{input}");
        }
        assert!(calc.preview("tan(90)").is_err());
    }

    #[test]
    fn only_successful_submissions_update_variables_and_ans() {
        let mut calc = Calculator::default();
        assert_eq!(calc.preview("x = 54"), Ok(54.0));
        assert!(calc.preview("x + ans").is_err());
        assert_eq!(calc.submit("x = 54"), Ok(54.0));
        for input in [
            "x = 1 / 0",
            "x = sqrt(-1)",
            "pi = 2",
            "x = 3 4",
            "x = y = 2",
        ] {
            assert!(calc.submit(input).is_err(), "{input}");
        }
        assert_eq!(calc.preview("x + ans"), Ok(108.0));
        assert_eq!(calc.submit("x = x + 1"), Ok(55.0));
        assert_eq!(
            calc.values().collect::<Vec<_>>(),
            [("ans", 55.0), ("x", 55.0)]
        );
        assert!(calc.preview(&"(".repeat(1_000)).is_err());
    }

    #[test]
    fn formats_grouped_decimals_and_extreme_values() {
        for (value, expected) in [
            (132_312.231_23, "132,312.23123"),
            (1_234_567_890_123_456.5, "1,234,567,890,123,456.5"),
            (-100_000.0, "-100,000"),
            (0.1 + 0.2, "0.3"),
            (-0.0, "0"),
            (1e-20, "1e-20"),
            (1e25, "1e25"),
        ] {
            assert_eq!(format_number(value), expected);
        }
    }
}
