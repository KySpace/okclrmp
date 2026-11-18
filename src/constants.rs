//! Typed scalar controls and caret-relative decimal stepping, independent of GPUI.
use crate::expr;
use serde::{Deserialize, Deserializer, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConstantKind {
    Degree,
    Fraction,
    Integer,
    #[default]
    Free,
}
impl ConstantKind {
    pub const ALL: [Self; 4] = [Self::Degree, Self::Fraction, Self::Integer, Self::Free];
    pub fn name(self) -> &'static str {
        match self {
            Self::Degree => "Degree",
            Self::Fraction => "Fraction",
            Self::Integer => "Integer",
            Self::Free => "Free",
        }
    }
    pub fn description(self) -> &'static str {
        match self {
            Self::Degree => "Degree · wraps at 0° / 360° · drag 0.5° per pixel",
            Self::Fraction => "Fraction · clamped to [0, 1] · drag 0.002 per pixel",
            Self::Integer => "Integer · whole numbers · drag 1 per 10 pixels",
            Self::Free => "Free · any finite number · drag 0.01 per pixel",
        }
    }
    pub fn constrain(self, value: f64) -> Option<f64> {
        value.is_finite().then(|| match self {
            Self::Degree => {
                let wrapped = value.rem_euclid(360.);
                // A tiny negative remainder can round back up to 360 in f64.
                if wrapped >= 360. { 0. } else { wrapped }
            }
            Self::Fraction => value.clamp(0., 1.),
            Self::Integer => value.round(),
            Self::Free => value,
        })
    }
    pub fn drag(self, start: f64, pixels: f64) -> Option<f64> {
        let rate = match self {
            Self::Degree => 0.5,
            Self::Fraction => 0.002,
            Self::Integer => 0.1,
            Self::Free => 0.01,
        };
        self.constrain(start + pixels * rate)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Constant {
    pub name: String,
    pub value: f64,
    #[serde(default)]
    pub kind: ConstantKind,
}
impl Constant {
    pub fn from_legacy(source: &str, parameter: &str) -> Result<Vec<Self>, String> {
        let values = expr::constants(source, parameter)?;
        Ok(source
            .split([';', '\n'])
            .filter_map(|entry| {
                let entry = entry.trim();
                if entry.starts_with('#') {
                    return None;
                }
                let name = entry.split_once('=')?.0.trim();
                let value = values[name];
                // Infer UI types without changing the original curve's values.
                let kind = if (0. ..360.).contains(&value)
                    && (name == "hue" || name == "h" || name.starts_with("h_"))
                {
                    ConstantKind::Degree
                } else if (0. ..=1.).contains(&value) {
                    ConstantKind::Fraction
                } else {
                    ConstantKind::Free
                };
                Some(Self {
                    name: name.into(),
                    value,
                    kind,
                })
            })
            .collect())
    }
}

/// Older JSON files stored all assignments in one string. New files store rows.
pub fn deserialize_constants<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<Constant>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Stored {
        Rows(Vec<Constant>),
        Legacy(String),
    }
    match Stored::deserialize(d)? {
        Stored::Rows(rows) => Ok(rows),
        Stored::Legacy(source) => {
            Constant::from_legacy(&source, "").map_err(serde::de::Error::custom)
        }
    }
}

#[derive(Debug, PartialEq)]
pub struct NumberEdit {
    pub text: String,
    pub cursor: usize,
}

/// Step the decimal place immediately left of the caret (skipping a decimal
/// separator). Keep at most two decimal places and the caret's place across carries.
/// Scientific mantissas and exponent digits are both supported.
pub fn step_number(
    text: &str,
    cursor: usize,
    kind: ConstantKind,
    direction: i32,
) -> Option<NumberEdit> {
    if !text.is_ascii() || text.len() > 128 || cursor > text.len() || !matches!(direction, -1 | 1) {
        return None;
    }
    let value = text.parse::<f64>().ok().filter(|v| v.is_finite())?;
    let bytes = text.as_bytes();
    let mut digit = cursor.checked_sub(1)?;
    if bytes[digit] == b'.' {
        digit = digit.checked_sub(1)?;
    }
    if !bytes[digit].is_ascii_digit() {
        return None;
    }
    let exponent_at = text.find(['e', 'E']).unwrap_or(text.len());
    let mantissa = &text[..exponent_at];
    let exponent = if exponent_at < text.len() {
        text[exponent_at + 1..].parse::<i32>().ok()?
    } else {
        0
    };
    let decimals = mantissa
        .split_once('.')
        .map_or(0, |(_, tail)| tail.len())
        .min(2);
    let decimal = mantissa.find('.').unwrap_or(mantissa.len());
    let exponent_digit = digit > exponent_at;
    let power = if exponent_digit {
        (text.len() - digit - 1) as i32
    } else {
        (decimal as i32 - digit as i32 - i32::from(digit < decimal)).max(-2)
    };
    let (next, written_exponent) = if exponent_digit {
        let step = 10_i32.checked_pow(power as u32)?.checked_mul(direction)?;
        let next_exp = exponent.checked_add(step)?;
        if !(-308..=308).contains(&next_exp) {
            return None;
        }
        (
            mantissa.parse::<f64>().ok()? * 10_f64.powi(next_exp),
            next_exp,
        )
    } else {
        let mut step = 10_f64.powi(power.checked_add(exponent)?);
        if kind == ConstantKind::Integer {
            step = step.max(1.);
        }
        if !step.is_finite() || step == 0. {
            return None;
        }
        (value + direction as f64 * step, exponent)
    };
    let next = kind.constrain(next)?;
    let scientific = exponent_at < text.len();
    let mut written = if scientific {
        let scaled = next / 10_f64.powi(written_exponent);
        if !scaled.is_finite() {
            return None;
        }
        let rounded = format!("{scaled:.decimals$}");
        let roundtrip = rounded.parse::<f64>().ok()?;
        // Clamping/wrapping an exponent step can require extra mantissa
        // digits: fraction 1e0 stepped up must remain 1 (0.1e1), not 0e1.
        let mantissa = if (roundtrip - scaled).abs() <= scaled.abs() * f64::EPSILON * 4. {
            rounded
        } else {
            display_value(scaled)
        };
        let candidate = format!("{mantissa}e{written_exponent}");
        // When a bound cannot be represented with two mantissa decimals,
        // use the ordinary value instead of rounding it outside that bound.
        if candidate
            .parse::<f64>()
            .ok()
            .is_some_and(|v| (v - next).abs() <= next.abs() * f64::EPSILON * 4.)
        {
            candidate
        } else {
            display_value(next)
        }
    } else {
        format!("{next:.decimals$}")
    };
    if !exponent_digit && power >= 0 {
        // Keep the selected place present after a borrow or degree wrap, e.g.
        // stepping the hundreds in 359.5 produces 099.5 with the caret after 0.
        let sign = usize::from(written.starts_with('-'));
        let integer_digits = written.find(['.', 'e']).unwrap_or(written.len()) - sign;
        let padding = (power as usize + 1).saturating_sub(integer_digits);
        if padding > 0 {
            written.insert_str(sign, &"0".repeat(padding));
        }
    }
    let new_exponent_at = written.find('e').unwrap_or(written.len());
    let new_decimal = written[..new_exponent_at]
        .find('.')
        .unwrap_or(new_exponent_at);
    let cursor = if exponent_digit {
        written.len().saturating_sub(power as usize)
    } else {
        (new_decimal as i32 - power + i32::from(power < 0)).clamp(1, new_exponent_at as i32)
            as usize
    };
    Some(NumberEdit {
        text: written,
        cursor,
    })
}

/// Constant controls show at most two fractional digits, with no negative zero.
pub fn display_value(value: f64) -> String {
    let text = format!("{value:.2}")
        .trim_end_matches('0')
        .trim_end_matches('.')
        .to_string();
    if text == "-0" { "0".into() } else { text }
}

/// Normalize excess precision while allowing unfinished input such as `-` or
/// `1.2e-` to remain editable. Scientific notation retains its exponent.
pub fn limit_precision(text: &str) -> Option<String> {
    let exponent = text.find(['e', 'E']).unwrap_or(text.len());
    let mantissa = &text[..exponent];
    if mantissa
        .split_once('.')
        .is_none_or(|(_, digits)| digits.len() <= 2)
    {
        return None;
    }
    let value = mantissa.parse::<f64>().ok().filter(|v| v.is_finite())?;
    Some(format!("{}{}", display_value(value), &text[exponent..]))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn types_constrain_and_drag() {
        assert_eq!(ConstantKind::Degree.constrain(360.), Some(0.));
        assert_eq!(ConstantKind::Degree.drag(1., -4.), Some(359.));
        assert_eq!(ConstantKind::Degree.constrain(-1e-20), Some(0.));
        assert_eq!(ConstantKind::Fraction.drag(0.9, 100.), Some(1.));
        assert_eq!(ConstantKind::Fraction.constrain(-1.), Some(0.));
        assert_eq!(ConstantKind::Integer.drag(-2., -6.), Some(-3.));
        assert_eq!(ConstantKind::Free.drag(-2., 10.), Some(-1.9));
        for kind in ConstantKind::ALL {
            assert_eq!(kind.constrain(f64::NAN), None);
        }
    }
    #[test]
    fn digit_places_carries_signs_and_precision() {
        let edit = |s, c, d| step_number(s, c, ConstantKind::Free, d).unwrap();
        assert_eq!(
            edit("12.345", 2, 1),
            NumberEdit {
                text: "13.35".into(),
                cursor: 2
            }
        );
        assert_eq!(edit("12.345", 4, -1).text, "12.25");
        assert_eq!(edit("12.345", 3, 1).text, "13.35");
        assert_eq!(
            edit("9.90", 1, 1),
            NumberEdit {
                text: "10.90".into(),
                cursor: 2
            }
        );
        assert_eq!(
            edit("10.90", 2, -1),
            NumberEdit {
                text: "9.90".into(),
                cursor: 1
            }
        );
        assert_eq!(
            edit("-0.1", 4, 1),
            NumberEdit {
                text: "0.0".into(),
                cursor: 3
            }
        );
        assert_eq!(edit("0.0090", 5, 1).text, "0.02");
        assert_eq!(display_value(0.1 + 0.2), "0.3");
        assert_eq!(display_value(136.596), "136.6");
        assert_eq!(display_value(-0.004), "0");
        assert_eq!(limit_precision("0.1234"), Some("0.12".into()));
        assert_eq!(limit_precision("1.234e-3"), Some("1.23e-3".into()));
        assert_eq!(limit_precision("1.20"), None);
        assert_eq!(limit_precision("-"), None);
        assert_eq!(edit("1.20e-2", 3, 1).text, "1.30e-2");
        assert_eq!(edit("1.20e-2", 7, 1).text, "1.20e-1");
        assert!(step_number("-0.1", 1, ConstantKind::Free, 1).is_none());
        assert!(step_number("1e", 2, ConstantKind::Free, 1).is_none());
        assert!(step_number("NaN", 3, ConstantKind::Free, 1).is_none());
        assert!(step_number("0.1", 0, ConstantKind::Free, 1).is_none());
    }
    #[test]
    fn digit_steps_obey_types() {
        let edit = step_number("1e0", 3, ConstantKind::Fraction, 1).unwrap();
        assert_eq!(edit.text, "0.1e1");
        assert_eq!(edit.text.parse::<f64>().unwrap(), 1.);
        assert_eq!(
            step_number("1e3", 3, ConstantKind::Degree, 1)
                .unwrap()
                .text
                .parse::<f64>()
                .unwrap(),
            280.
        );
        assert_eq!(
            step_number("359.5", 1, ConstantKind::Degree, 1).unwrap(),
            NumberEdit {
                text: "099.5".into(),
                cursor: 1
            }
        );
        assert_eq!(
            step_number("359.5", 3, ConstantKind::Degree, 1)
                .unwrap()
                .text,
            "0.5"
        );
        assert_eq!(
            step_number("0.99", 4, ConstantKind::Fraction, 1)
                .unwrap()
                .text,
            "1.00"
        );
        assert_eq!(
            step_number("1.00", 4, ConstantKind::Fraction, 1)
                .unwrap()
                .text,
            "1.00"
        );
        assert_eq!(
            step_number("2.0", 3, ConstantKind::Integer, -1)
                .unwrap()
                .text,
            "1.0"
        );
    }
}
