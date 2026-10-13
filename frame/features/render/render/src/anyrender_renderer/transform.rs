/// A 2D affine transform in Figma row-major format.
///
/// Figma stores transforms as `[[a, b, tx], [c, d, ty]]` where:
///   - Row 0: `[a, b, tx]` → `x' = a*x + b*y + tx`
///   - Row 1: `[c, d, ty]` → `y' = c*x + d*y + ty`
///
/// Note: SVG's `matrix(a,b,c,d,e,f)` uses column-major convention — to emit
/// an SVG matrix string use [`AffineTransform2D::to_svg_matrix`] or the
/// [`std::fmt::Display`] impl, which reorders fields to `(a, c, b, d, tx, ty)`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AffineTransform2D {
    /// Row 0, col 0: x-scale / cos(θ)
    pub a: f64,
    /// Row 0, col 1: -sin(θ) for pure rotation
    pub b: f64,
    /// Row 0, col 2: x-translation
    pub tx: f64,
    /// Row 1, col 0: sin(θ) for pure rotation
    pub c: f64,
    /// Row 1, col 1: y-scale / cos(θ)
    pub d: f64,
    /// Row 1, col 2: y-translation
    pub ty: f64,
}

impl AffineTransform2D {
    /// The identity transform.
    pub fn identity() -> Self {
        Self {
            a: 1.0,
            b: 0.0,
            tx: 0.0,
            c: 0.0,
            d: 1.0,
            ty: 0.0,
        }
    }

    /// A pure rotation by `degrees` (counter-clockwise).
    pub fn rotation(degrees: f64) -> Self {
        let r = degrees.to_radians();
        let (sin_r, cos_r) = r.sin_cos();
        Self {
            a: cos_r,
            b: -sin_r,
            tx: 0.0,
            c: sin_r,
            d: cos_r,
            ty: 0.0,
        }
    }

    /// A pure translation.
    pub fn translation(tx: f64, ty: f64) -> Self {
        Self {
            a: 1.0,
            b: 0.0,
            tx,
            c: 0.0,
            d: 1.0,
            ty,
        }
    }

    /// A non-uniform scale.
    pub fn scale(sx: f64, sy: f64) -> Self {
        Self {
            a: sx,
            b: 0.0,
            tx: 0.0,
            c: 0.0,
            d: sy,
            ty: 0.0,
        }
    }

    /// Apply this transform to point `(u, v)` and return the transformed point.
    ///
    /// `x' = a*u + b*v + tx`, `y' = c*u + d*v + ty`
    pub fn transform_point(&self, u: f64, v: f64) -> (f64, f64) {
        (self.a * u + self.b * v + self.tx, self.c * u + self.d * v + self.ty)
    }

    /// Determinant of the 2×2 linear part: `a*d - b*c`.
    pub fn determinant(&self) -> f64 {
        self.a * self.d - self.b * self.c
    }

    /// Returns `true` if this is (approximately) the identity transform.
    pub fn is_identity(&self) -> bool {
        (self.a - 1.0).abs() <= f64::EPSILON
            && self.b.abs() <= f64::EPSILON
            && self.tx.abs() <= f64::EPSILON
            && self.c.abs() <= f64::EPSILON
            && (self.d - 1.0).abs() <= f64::EPSILON
            && self.ty.abs() <= f64::EPSILON
    }

    /// Extract the rotation angle in radians from the first column of the
    /// linear part: `atan2(c, a)`.
    ///
    /// For a pure rotation matrix `[[cos θ, -sin θ], [sin θ, cos θ]]` this
    /// recovers θ exactly. In the presence of scale the result is the rotation
    /// of the x-axis.
    pub fn decompose_rotation(&self) -> f64 {
        self.c.atan2(self.a)
    }

    /// Return the inverse transform, or `None` if the matrix is singular
    /// (determinant ≈ 0).
    pub fn inverse(&self) -> Option<Self> {
        let det = self.determinant();
        if det.abs() < 1e-12 {
            return None;
        }
        Some(Self {
            a: self.d / det,
            b: -self.b / det,
            tx: (self.b * self.ty - self.d * self.tx) / det,
            c: -self.c / det,
            d: self.a / det,
            ty: (self.c * self.tx - self.a * self.ty) / det,
        })
    }

    /// Return the SVG `matrix()` coefficients in column-major order:
    /// `(a, c, b, d, tx, ty)`.
    ///
    /// SVG `matrix(a,b,c,d,e,f)` encodes `x' = a*x + c*y + e` and
    /// `y' = b*x + d*y + f`, so the column-major reordering is required.
    pub fn to_svg_matrix_coeffs(&self) -> (f64, f64, f64, f64, f64, f64) {
        (self.a, self.c, self.b, self.d, self.tx, self.ty)
    }
}

/// Compose two transforms: `self` applied *after* `rhs`.
///
/// `(self * rhs)(v) = self(rhs(v))`
impl std::ops::Mul for AffineTransform2D {
    type Output = Self;

    fn mul(self, rhs: Self) -> Self {
        Self {
            a: self.a * rhs.a + self.b * rhs.c,
            b: self.a * rhs.b + self.b * rhs.d,
            tx: self.a * rhs.tx + self.b * rhs.ty + self.tx,
            c: self.c * rhs.a + self.d * rhs.c,
            d: self.c * rhs.b + self.d * rhs.d,
            ty: self.c * rhs.tx + self.d * rhs.ty + self.ty,
        }
    }
}

/// Format as an SVG `matrix(a,b,c,d,e,f)` string using column-major order.
impl std::fmt::Display for AffineTransform2D {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let (a, b, c, d, e, ff) = self.to_svg_matrix_coeffs();
        write!(f, "matrix({a} {b} {c} {d} {e} {ff})")
    }
}

/// Parse from Figma's row-major `[[a, b, tx], [c, d, ty]]` representation.
impl From<[[f64; 3]; 2]> for AffineTransform2D {
    fn from(t: [[f64; 3]; 2]) -> Self {
        Self {
            a: t[0][0],
            b: t[0][1],
            tx: t[0][2],
            c: t[1][0],
            d: t[1][1],
            ty: t[1][2],
        }
    }
}

impl From<AffineTransform2D> for [[f64; 3]; 2] {
    fn from(t: AffineTransform2D) -> Self {
        [[t.a, t.b, t.tx], [t.c, t.d, t.ty]]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx_eq(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-10
    }

    fn transform_approx_eq(a: &AffineTransform2D, b: &AffineTransform2D) -> bool {
        approx_eq(a.a, b.a)
            && approx_eq(a.b, b.b)
            && approx_eq(a.tx, b.tx)
            && approx_eq(a.c, b.c)
            && approx_eq(a.d, b.d)
            && approx_eq(a.ty, b.ty)
    }

    #[test]
    fn test_identity() {
        let id = AffineTransform2D::identity();
        assert!(id.is_identity());
        let (x, y) = id.transform_point(3.0, 7.0);
        assert_eq!(x, 3.0);
        assert_eq!(y, 7.0);
    }

    #[test]
    fn test_translation() {
        let t = AffineTransform2D::translation(5.0, -3.0);
        let (x, y) = t.transform_point(1.0, 2.0);
        assert_eq!(x, 6.0);
        assert_eq!(y, -1.0);
    }

    #[test]
    fn test_scale() {
        let s = AffineTransform2D::scale(2.0, 3.0);
        let (x, y) = s.transform_point(4.0, 5.0);
        assert_eq!(x, 8.0);
        assert_eq!(y, 15.0);
    }

    #[test]
    fn test_rotation_90() {
        // 90° CCW: (1,0) → (0,1), (0,1) → (-1,0)
        let r = AffineTransform2D::rotation(90.0);
        let (x, y) = r.transform_point(1.0, 0.0);
        assert!(approx_eq(x, 0.0), "x={x}");
        assert!(approx_eq(y, 1.0), "y={y}");
    }

    #[test]
    fn test_decompose_rotation() {
        for deg in [0.0f64, 30.0, 45.0, 90.0, 135.0, 180.0, -45.0, -90.0] {
            let r = AffineTransform2D::rotation(deg);
            let recovered = r.decompose_rotation().to_degrees();
            let diff = (recovered - deg).abs();
            // Allow wrap-around for ±180°
            assert!(diff < 1e-9 || (180.0 - diff).abs() < 1e-9, "deg={deg} recovered={recovered}");
        }
    }

    #[test]
    fn test_determinant() {
        let id = AffineTransform2D::identity();
        assert_eq!(id.determinant(), 1.0);

        let s = AffineTransform2D::scale(2.0, 3.0);
        assert_eq!(s.determinant(), 6.0);

        // Reflection: det should be -1
        let reflect = AffineTransform2D { a: -1.0, b: 0.0, tx: 0.0, c: 0.0, d: 1.0, ty: 0.0 };
        assert_eq!(reflect.determinant(), -1.0);
    }

    #[test]
    fn test_composition_identity() {
        let t = AffineTransform2D::translation(3.0, 4.0);
        let _r = AffineTransform2D::rotation(45.0);
        let id = AffineTransform2D::identity();
        assert!(transform_approx_eq(&(t * id), &t));
        assert!(transform_approx_eq(&(id * t), &t));
    }

    #[test]
    fn test_composition_translate_then_scale() {
        // Scale * Translate: translate first, then scale
        let translate = AffineTransform2D::translation(1.0, 2.0);
        let scale = AffineTransform2D::scale(2.0, 2.0);
        let composed = scale * translate;
        // Point (0,0) → translate → (1,2) → scale → (2,4)
        let (x, y) = composed.transform_point(0.0, 0.0);
        assert!(approx_eq(x, 2.0), "x={x}");
        assert!(approx_eq(y, 4.0), "y={y}");
    }

    #[test]
    fn test_inverse_identity() {
        let id = AffineTransform2D::identity();
        let inv = id.inverse().unwrap();
        assert!(transform_approx_eq(&inv, &id));
    }

    #[test]
    fn test_inverse_translation() {
        let t = AffineTransform2D::translation(5.0, -3.0);
        let inv = t.inverse().unwrap();
        let composed = t * inv;
        assert!(transform_approx_eq(&composed, &AffineTransform2D::identity()));
    }

    #[test]
    fn test_inverse_scale() {
        let s = AffineTransform2D::scale(2.0, 4.0);
        let inv = s.inverse().unwrap();
        let composed = s * inv;
        assert!(transform_approx_eq(&composed, &AffineTransform2D::identity()));
    }

    #[test]
    fn test_inverse_rotation() {
        let r = AffineTransform2D::rotation(37.5);
        let inv = r.inverse().unwrap();
        let composed = r * inv;
        assert!(transform_approx_eq(&composed, &AffineTransform2D::identity()));
    }

    #[test]
    fn test_inverse_singular() {
        let singular = AffineTransform2D { a: 1.0, b: 2.0, tx: 0.0, c: 2.0, d: 4.0, ty: 0.0 };
        assert!(singular.inverse().is_none());
    }

    #[test]
    fn test_inverse_near_zero_det() {
        // det = 1e-14 < 1e-12 threshold — treated as singular
        let t = AffineTransform2D { a: 1e-7, b: 0.0, tx: 0.0, c: 0.0, d: 1e-7, ty: 0.0 };
        assert!(t.inverse().is_none());

        // det = 1e-11 > 1e-12 threshold — invertible despite being small
        let t2 = AffineTransform2D { a: 1e-5, b: 0.0, tx: 0.0, c: 0.0, d: 1e-6, ty: 0.0 };
        assert!(t2.inverse().is_some());
    }

    #[test]
    fn test_from_raw_array() {
        let raw: [[f64; 3]; 2] = [[0.5, -0.866, 10.0], [0.866, 0.5, 20.0]];
        let t = AffineTransform2D::from(raw);
        assert_eq!(t.a, 0.5);
        assert_eq!(t.b, -0.866);
        assert_eq!(t.tx, 10.0);
        assert_eq!(t.c, 0.866);
        assert_eq!(t.d, 0.5);
        assert_eq!(t.ty, 20.0);
    }

    #[test]
    fn test_to_svg_matrix_coeffs() {
        // SVG matrix(a,b,c,d,e,f) = column-major: x' = a*x + c*y + e, y' = b*x + d*y + f
        // Our fields: x' = a*x + b*y + tx, y' = c*x + d*y + ty
        // Mapping: svg_a=a, svg_b=c, svg_c=b, svg_d=d, svg_e=tx, svg_f=ty
        let t = AffineTransform2D { a: 1.0, b: 2.0, tx: 5.0, c: 3.0, d: 4.0, ty: 6.0 };
        assert_eq!(t.to_svg_matrix_coeffs(), (1.0, 3.0, 2.0, 4.0, 5.0, 6.0));
    }

    #[test]
    fn test_display() {
        let t = AffineTransform2D { a: 1.0, b: 0.0, tx: 10.0, c: 0.0, d: 1.0, ty: 20.0 };
        let s = t.to_string();
        assert_eq!(s, "matrix(1 0 0 1 10 20)");
    }

    #[test]
    fn test_reflection_inverse() {
        // Reflection across x-axis: det = -1
        let reflect = AffineTransform2D { a: 1.0, b: 0.0, tx: 0.0, c: 0.0, d: -1.0, ty: 0.0 };
        let inv = reflect.inverse().unwrap();
        let composed = reflect * inv;
        assert!(transform_approx_eq(&composed, &AffineTransform2D::identity()));
    }
}
