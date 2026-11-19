use crate::{color::Oklch, model::Curve};
use serde_json::Value;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImportSpace {
    Hex,
    Rgb,
    Oklch,
}
impl ImportSpace {
    pub fn label(self) -> &'static str {
        match self {
            Self::Hex => "HEX",
            Self::Rgb => "RGB 0–1",
            Self::Oklch => "OKLCH",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExportSpace {
    Hex,
    Rgb,
    Rgb8,
    Oklch,
}
impl ExportSpace {
    pub const ALL: [Self; 4] = [Self::Hex, Self::Rgb, Self::Rgb8, Self::Oklch];
    pub fn label(self) -> &'static str {
        match self {
            Self::Hex => "HEX JSON",
            Self::Rgb => "RGB JSON",
            Self::Rgb8 => "RGB8 CSV",
            Self::Oklch => "OKLCH CSV",
        }
    }
}
pub fn parse(input: &str, space: ImportSpace) -> Result<Vec<Oklch>, String> {
    if input.len() > 2_000_000 {
        return Err("Import is limited to 2 MB".into());
    }
    let mut colors = vec![];
    if space == ImportSpace::Hex {
        for token in input
            .split(|c: char| c.is_whitespace() || "[],;\"'".contains(c))
            .filter(|s| !s.is_empty())
        {
            let s = token.strip_prefix('#').unwrap_or(token);
            let expanded = if s.len() == 3 {
                s.chars().flat_map(|c| [c, c]).collect::<String>()
            } else {
                s.to_string()
            };
            if expanded.len() != 6 || !expanded.is_ascii() {
                return Err(format!("Invalid hex color ‘{token}’. Use #RGB or #RRGGBB."));
            }
            let n = u32::from_str_radix(&expanded, 16)
                .map_err(|_| format!("Invalid hex color ‘{token}’"))?;
            colors.push(Oklch::from_srgb([
                ((n >> 16) & 255) as f64 / 255.,
                ((n >> 8) & 255) as f64 / 255.,
                (n & 255) as f64 / 255.,
            ]));
        }
    } else {
        let triples: Vec<[f64; 3]> = if input.trim().starts_with('[') {
            let v: Value = serde_json::from_str(input).map_err(|e| format!("Invalid JSON: {e}"))?;
            v.as_array()
                .ok_or("Expected a JSON array of triples")?
                .iter()
                .map(|v| {
                    let a = v
                        .as_array()
                        .ok_or("Each color must be an array of three numbers")?;
                    if a.len() != 3 {
                        return Err("Each color needs exactly three numbers".to_string());
                    }
                    Ok([
                        a[0].as_f64().ok_or("Expected a number")?,
                        a[1].as_f64().ok_or("Expected a number")?,
                        a[2].as_f64().ok_or("Expected a number")?,
                    ])
                })
                .collect::<Result<_, String>>()?
        } else {
            input
                .lines()
                .enumerate()
                .filter(|(_, s)| !s.trim().is_empty() && !s.trim().starts_with('#'))
                .map(|(i, s)| {
                    let numbers = s
                        .split(|c: char| c == ',' || c.is_whitespace())
                        .filter(|s| !s.is_empty())
                        .map(str::parse::<f64>)
                        .collect::<Result<Vec<_>, _>>()
                        .map_err(|_| format!("Line {}: expected three numbers", i + 1))?;
                    numbers
                        .try_into()
                        .map_err(|_| format!("Line {}: expected exactly three numbers", i + 1))
                })
                .collect::<Result<_, String>>()?
        };
        for v in triples {
            if !v.iter().all(|v| v.is_finite()) {
                return Err("Colors must be finite".into());
            }
            if space == ImportSpace::Rgb {
                if !v.iter().all(|v| (0. ..=1.).contains(v)) {
                    return Err("RGB input uses normalized channels 0–1".into());
                }
                colors.push(Oklch::from_srgb(v));
            } else {
                colors.push(Oklch::new(v[0], v[1], v[2]));
            }
        }
    }
    if colors.is_empty() {
        return Err("No colors found".into());
    }
    if colors.len() > 16384 {
        return Err("Import at most 16,384 colors".into());
    }
    Ok(colors)
}
pub fn export(curve: &Curve, count: usize, space: ExportSpace, map: bool) -> String {
    export_ordered(curve, count, space, map, false)
}
pub fn export_ordered(
    curve: &Curve,
    count: usize,
    space: ExportSpace,
    map: bool,
    reverse: bool,
) -> String {
    let mut samples = curve.sample(count.clamp(2, 4096));
    if reverse {
        samples.reverse();
    }
    match space {
        ExportSpace::Hex => serde_json::to_string_pretty(
            &samples.iter().map(|s| s.color.hex(map)).collect::<Vec<_>>(),
        )
        .unwrap(),
        ExportSpace::Rgb => serde_json::to_string_pretty(
            &samples
                .iter()
                .map(|s| s.color.srgb(map).map(|v| (v * 1e7).round() / 1e7))
                .collect::<Vec<_>>(),
        )
        .unwrap(),
        ExportSpace::Rgb8 => samples
            .iter()
            .map(|s| {
                let a = s.color.rgba8(map);
                format!("{},{},{}", a[0], a[1], a[2])
            })
            .collect::<Vec<_>>()
            .join("\n"),
        ExportSpace::Oklch => samples
            .iter()
            .map(|s| {
                let c = s.color;
                format!("{:.7},{:.7},{:.7}", c.l, c.c, c.h)
            })
            .collect::<Vec<_>>()
            .join("\n"),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn inversion_reverses_every_encoding_and_roundtrips_without_editing_the_curve() {
        let mut project = crate::model::Project::preset(1);
        let before = project.compile().unwrap().at(0.25);
        project.reverse_output = true;
        let restored: crate::model::Project =
            serde_json::from_str(&serde_json::to_string(&project).unwrap()).unwrap();
        assert!(restored.reverse_output);
        let curve = restored.compile().unwrap();
        assert_eq!(curve.at(0.25), before);
        for space in ExportSpace::ALL {
            let normal = export(&curve, 16, space, true);
            let reversed = export_ordered(&curve, 16, space, true, true);
            match space {
                ExportSpace::Hex | ExportSpace::Rgb => {
                    let normal: Vec<serde_json::Value> = serde_json::from_str(&normal).unwrap();
                    let reversed: Vec<serde_json::Value> = serde_json::from_str(&reversed).unwrap();
                    assert_eq!(normal.into_iter().rev().collect::<Vec<_>>(), reversed);
                }
                _ => assert_eq!(
                    normal.lines().rev().collect::<Vec<_>>().join("\n"),
                    reversed
                ),
            }
        }
        let mut old = serde_json::to_value(&project).unwrap();
        old.as_object_mut().unwrap().remove("reverse_output");
        let old: crate::model::Project = serde_json::from_value(old).unwrap();
        assert!(!old.reverse_output);
    }
    #[test]
    fn import_and_export_roundtrip() {
        let c = parse("[\"#f00\", \"#00ff00\", \"#0000ff\"]", ImportSpace::Hex).unwrap();
        assert_eq!(c.len(), 3);
        assert_eq!(c[0].hex(false), "#ff0000");
        let p = crate::model::Project {
            imported: c,
            mode: crate::model::Mode::Imported,
            ..Default::default()
        };
        let curve = p.compile().unwrap();
        let csv = export(&curve, 3, ExportSpace::Oklch, false);
        let restored = parse(&csv, ImportSpace::Oklch).unwrap();
        assert_eq!(restored[1].hex(false), "#00ff00");
        assert!(parse("[[0,256,0]]", ImportSpace::Rgb).is_err());
        assert!(parse("NaN,0,0", ImportSpace::Oklch).is_err());
    }
    #[test]
    fn export_preserves_count_endpoints_and_original_oklch() {
        let mut p = crate::model::Project::preset(1);
        p.expressions = ["0.4+0.3*t".into(), "0.5".into(), "45".into()];
        let curve = p.compile().unwrap();
        let raw = export(&curve, 17, ExportSpace::Oklch, false);
        assert_eq!(raw, export(&curve, 17, ExportSpace::Oklch, true));
        let points = parse(&raw, ImportSpace::Oklch).unwrap();
        assert_eq!(points.len(), 17);
        assert_eq!(points[0], curve.at(0.));
        assert_eq!(points[16], curve.at(1.));
        let rgb: Vec<[f64; 3]> =
            serde_json::from_str(&export(&curve, 17, ExportSpace::Rgb, true)).unwrap();
        assert_eq!(rgb.len(), 17);
        assert!(rgb.iter().flatten().all(|v| (0. ..=1.).contains(v)));
        assert_ne!(
            export(&curve, 17, ExportSpace::Hex, true),
            export(&curve, 17, ExportSpace::Hex, false)
        );
    }
}
