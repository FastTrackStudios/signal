//! Parameter grids: measure a plugin across a cartesian product of its own
//! controls.
//!
//! Capturing factory presets says what a plugin does at the hundred-odd points
//! its designers chose. That validates a conversion, but it is a poor way to
//! *model* one: the presets move every control at once, so nothing in the data
//! separates what attack did from what release did. Fitting a time-constant
//! law needs the opposite — one axis moving at a time, on a regular lattice.
//!
//! This is the piece the legacy `fts-analyzer` had and this crate did not. Its
//! version resolved everything through hand-written JSON listing raw parameter
//! **ids**, which meant a scenario file was specific to one plugin and one
//! build — Pro-C 2's attack is id 5 where Pro-C 3's is id 7, so the same file
//! silently measured the wrong control on the other version. Here an axis
//! names its parameter, and the caller resolves names against whatever the
//! plugin reports.
//!
//! ```
//! use signal_analyzer::param_grid::{self, Axis};
//!
//! let axes = param_grid::parse("Attack=0..1:3;Release=0.1,0.5").unwrap();
//! let points = param_grid::grid(&axes);
//! assert_eq!(points.len(), 6);                       // 3 × 2
//! assert_eq!(points[0][0], ("Attack".to_string(), 0.0));
//! # let _: Vec<Axis> = axes;
//! ```

use serde::{Deserialize, Serialize};

/// One axis of the grid: a named parameter and the values to visit.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Axis {
    /// The parameter's name, as the plugin reports it. Matched
    /// case-insensitively by the caller.
    pub name: String,
    pub values: Vec<f64>,
}

/// Errors from [`parse`].
#[derive(Debug, Clone, PartialEq)]
pub enum GridError {
    /// An axis had no `=`.
    MissingValues(String),
    /// A value or bound did not parse as a number.
    NotANumber(String),
    /// A range asked for fewer than one step.
    EmptyRange(String),
}

impl std::fmt::Display for GridError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingValues(s) => write!(f, "axis '{s}' has no '=' — expected Name=lo..hi:steps or Name=a,b,c"),
            Self::NotANumber(s) => write!(f, "'{s}' is not a number"),
            Self::EmptyRange(s) => write!(f, "axis '{s}' asks for no steps"),
        }
    }
}

impl std::error::Error for GridError {}

/// Parse an axis specification.
///
/// Axes are separated by `;`, because `,` already separates the values of an
/// explicit list. Each axis is one of:
///
/// - `Name=lo..hi:steps` — `steps` points from `lo` to `hi` inclusive. One
///   step yields `lo` alone.
/// - `Name=a,b,c` — exactly those values.
///
/// The parameter name is taken verbatim up to the `=`, spaces included, so
/// `Side Chain Level=-12,0` works without quoting the name itself.
pub fn parse(spec: &str) -> Result<Vec<Axis>, GridError> {
    let mut axes = Vec::new();
    for part in spec.split(';').map(str::trim).filter(|s| !s.is_empty()) {
        let (name, rest) = part
            .split_once('=')
            .ok_or_else(|| GridError::MissingValues(part.to_string()))?;
        let name = name.trim().to_string();
        let rest = rest.trim();

        let values = if let Some((range, steps)) = rest.rsplit_once(':') {
            // `rsplit_once` so a negative bound's '-' is never confused for
            // the separator, and `..` is looked for inside the range half.
            let (lo, hi) = range
                .split_once("..")
                .ok_or_else(|| GridError::MissingValues(part.to_string()))?;
            let lo = num(lo)?;
            let hi = num(hi)?;
            let steps: usize = steps
                .trim()
                .parse()
                .map_err(|_| GridError::NotANumber(steps.to_string()))?;
            if steps == 0 {
                return Err(GridError::EmptyRange(name));
            }
            if steps == 1 {
                vec![lo]
            } else {
                (0..steps)
                    .map(|i| lo + (hi - lo) * (i as f64 / (steps - 1) as f64))
                    .collect()
            }
        } else {
            rest.split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(num)
                .collect::<Result<Vec<_>, _>>()?
        };

        if values.is_empty() {
            return Err(GridError::EmptyRange(name));
        }
        axes.push(Axis { name, values });
    }
    Ok(axes)
}

fn num(s: &str) -> Result<f64, GridError> {
    s.trim().parse().map_err(|_| GridError::NotANumber(s.trim().to_string()))
}

/// Every combination of the axes, as `(name, value)` pairs.
///
/// The **last** axis varies fastest, so a two-axis grid reads like a table
/// with the first axis as its rows — which is how the resulting scenarios
/// list, and how anyone reading the output expects it to be ordered.
pub fn grid(axes: &[Axis]) -> Vec<Vec<(String, f64)>> {
    // No axes is no grid, not one empty point. The mathematical empty product
    // is 1, but a caller that passed nothing wants nothing measured — and a
    // single nameless scenario is a confusing way to discover that.
    if axes.is_empty() {
        return Vec::new();
    }
    let mut points: Vec<Vec<(String, f64)>> = vec![Vec::new()];
    for axis in axes {
        let mut next = Vec::with_capacity(points.len() * axis.values.len());
        for base in &points {
            for &v in &axis.values {
                let mut row = base.clone();
                row.push((axis.name.clone(), v));
                next.push(row);
            }
        }
        points = next;
    }
    points
}

/// How many points [`grid`] will produce, without building them.
///
/// Worth calling first: four axes of ten is ten thousand renders, and the
/// difference between a two-minute run and an overnight one is not obvious
/// from the spec string.
pub fn size(axes: &[Axis]) -> usize {
    if axes.is_empty() {
        return 0;
    }
    axes.iter().map(|a| a.values.len()).product()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_range_with_inclusive_bounds() {
        let axes = parse("Attack=0..1:5").unwrap();
        assert_eq!(axes.len(), 1);
        assert_eq!(axes[0].name, "Attack");
        assert_eq!(axes[0].values, vec![0.0, 0.25, 0.5, 0.75, 1.0]);
    }

    #[test]
    fn parses_an_explicit_list() {
        let axes = parse("Ratio=0.2,0.56,1.0").unwrap();
        assert_eq!(axes[0].values, vec![0.2, 0.56, 1.0]);
    }

    #[test]
    fn a_parameter_name_may_contain_spaces() {
        let axes = parse("Side Chain Level=-12,0").unwrap();
        assert_eq!(axes[0].name, "Side Chain Level");
        assert_eq!(axes[0].values, vec![-12.0, 0.0]);
    }

    #[test]
    fn negative_bounds_are_not_mistaken_for_the_step_separator() {
        let axes = parse("Threshold=-60..-10:3").unwrap();
        assert_eq!(axes[0].values, vec![-60.0, -35.0, -10.0]);
    }

    #[test]
    fn one_step_is_the_lower_bound_alone_rather_than_a_division_by_zero() {
        let axes = parse("Attack=0.2..0.9:1").unwrap();
        assert_eq!(axes[0].values, vec![0.2]);
    }

    #[test]
    fn the_last_axis_varies_fastest() {
        let axes = parse("A=0,1;B=10,20").unwrap();
        let points = grid(&axes);
        assert_eq!(
            points,
            vec![
                vec![("A".into(), 0.0), ("B".into(), 10.0)],
                vec![("A".into(), 0.0), ("B".into(), 20.0)],
                vec![("A".into(), 1.0), ("B".into(), 10.0)],
                vec![("A".into(), 1.0), ("B".into(), 20.0)],
            ]
        );
    }

    #[test]
    fn the_legacy_attack_release_grid_is_one_spec_string() {
        // The reference captures used 196 scenarios: fourteen attacks against
        // fourteen releases.
        let axes = parse("Attack=0..0.93:14;Release=0..0.94:14").unwrap();
        assert_eq!(size(&axes), 196);
        assert_eq!(grid(&axes).len(), 196);
    }

    #[test]
    fn size_agrees_with_the_grid_it_predicts() {
        for spec in ["A=0..1:7", "A=0..1:3;B=0,1", "A=1;B=2;C=3,4,5"] {
            let axes = parse(spec).unwrap();
            assert_eq!(size(&axes), grid(&axes).len(), "{spec}");
        }
        assert_eq!(size(&[]), 0);
    }

    #[test]
    fn malformed_specs_are_refused_with_a_reason() {
        assert!(matches!(parse("Attack"), Err(GridError::MissingValues(_))));
        assert!(matches!(parse("Attack=fast"), Err(GridError::NotANumber(_))));
        assert!(matches!(parse("Attack=0..1:0"), Err(GridError::EmptyRange(_))));
        assert!(matches!(parse("Attack="), Err(GridError::EmptyRange(_))));
        // A stray separator is skipped rather than producing a nameless axis.
        assert_eq!(parse("A=1;;B=2").unwrap().len(), 2);
        assert!(parse("").unwrap().is_empty());
    }
}
