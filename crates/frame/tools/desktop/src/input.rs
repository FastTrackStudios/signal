//! Window ↔ surface geometry and winit → surface input mapping. Pure, so
//! the host-specific half of the interaction contract is unit-tested.

use surface_runtime::{Bounds, Key, Modifiers};
use winit::event::MouseScrollDelta;
use winit::keyboard::{Key as WinitKey, ModifiersState, NamedKey};

/// Fit-to-window transform: the surface's content box scaled uniformly to
/// fill the window's physical pixels, centered (letterboxed). Rendering in
/// physical pixels makes it HiDPI-aware for free; winit reports cursor
/// positions in the same physical pixels, so one inverse maps them back.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct View {
    pub scale: f64,
    pub tx: f64,
    pub ty: f64,
}

impl View {
    #[must_use]
    pub fn fit(content: Bounds, width: f64, height: f64) -> Self {
        if content.width <= 0.0 || content.height <= 0.0 || width <= 0.0 || height <= 0.0 {
            return Self {
                scale: 1.0,
                tx: -content.x,
                ty: -content.y,
            };
        }
        let scale = (width / content.width).min(height / content.height);
        Self {
            scale,
            tx: (width - content.width * scale) / 2.0 - content.x * scale,
            ty: (height - content.height * scale) / 2.0 - content.y * scale,
        }
    }

    /// Window (physical px) → surface coordinates.
    #[must_use]
    pub fn to_surface(self, x: f64, y: f64) -> (f64, f64) {
        ((x - self.tx) / self.scale, (y - self.ty) / self.scale)
    }

    /// Surface coordinates → window (physical px).
    #[must_use]
    pub fn to_window(self, x: f64, y: f64) -> (f64, f64) {
        (x * self.scale + self.tx, y * self.scale + self.ty)
    }

    #[must_use]
    pub fn affine(&self) -> kurbo::Affine {
        kurbo::Affine::new([self.scale, 0.0, 0.0, self.scale, self.tx, self.ty])
    }
}

/// Max gap between two presses to count as a double-click.
pub const DOUBLE_CLICK_MS: f64 = 350.0;
/// Max travel (logical px) between two presses of a double-click.
pub const DOUBLE_CLICK_PX: f64 = 4.0;

/// Detects double-clicks from successive presses (logical px, ms).
#[derive(Debug, Clone, Copy, Default)]
pub struct Clicks {
    last: Option<(f64, f64, f64)>,
}

impl Clicks {
    /// Record a press; true when it completes a double-click. A triple
    /// press is a double-click then a fresh first click.
    pub fn press(&mut self, x: f64, y: f64, t: f64) -> bool {
        let double = self.last.is_some_and(|(lx, ly, lt)| {
            t - lt <= DOUBLE_CLICK_MS && (x - lx).hypot(y - ly) <= DOUBLE_CLICK_PX
        });
        self.last = if double { None } else { Some((x, y, t)) };
        double
    }
}

/// Shift = fine, Cmd/Ctrl = alternate (reset-on-click).
#[must_use]
pub fn modifiers(m: ModifiersState) -> Modifiers {
    Modifiers {
        fine: m.shift_key(),
        alt: m.control_key() || m.super_key(),
        option: m.alt_key(),
    }
}

#[must_use]
pub fn key(k: &WinitKey) -> Option<Key> {
    let named = match k {
        WinitKey::Named(named) => named,
        // A single character (a shortcut with Cmd/Ctrl, e.g. select all).
        WinitKey::Character(s) => {
            let mut chars = s.chars();
            return match (chars.next(), chars.next()) {
                (Some(c), None) => Some(Key::Char(c)),
                _ => None,
            };
        }
        _ => return None,
    };
    Some(match named {
        NamedKey::ArrowUp => Key::Up,
        NamedKey::ArrowDown => Key::Down,
        NamedKey::ArrowLeft => Key::Left,
        NamedKey::ArrowRight => Key::Right,
        NamedKey::PageUp => Key::PageUp,
        NamedKey::PageDown => Key::PageDown,
        NamedKey::Home => Key::Home,
        NamedKey::End => Key::End,
        NamedKey::Space => Key::Space,
        NamedKey::Enter => Key::Enter,
        NamedKey::Escape => Key::Escape,
        NamedKey::Tab => Key::Tab,
        NamedKey::Backspace => Key::Backspace,
        NamedKey::Delete => Key::Delete,
        _ => return None,
    })
}

/// Logical px of trackpad scroll per wheel notch.
const PX_PER_NOTCH: f64 = 20.0;

/// Wheel notches, positive = up/away = increase.
#[must_use]
pub fn wheel_notches(delta: MouseScrollDelta, scale_factor: f64) -> f64 {
    match delta {
        MouseScrollDelta::LineDelta(_, y) => f64::from(y),
        MouseScrollDelta::PixelDelta(p) => p.y / (PX_PER_NOTCH * scale_factor.max(0.1)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use winit::dpi::PhysicalPosition;

    const CONTENT: Bounds = Bounds {
        x: 0.0,
        y: 0.0,
        width: 400.0,
        height: 200.0,
    };

    #[test]
    fn fit_letterboxes_and_centers() {
        // Window taller than the content: full width, bars top and bottom.
        let v = View::fit(CONTENT, 800.0, 800.0);
        assert_eq!(v.scale, 2.0);
        assert_eq!((v.tx, v.ty), (0.0, 200.0));
        // Wider: full height, bars left and right.
        let v = View::fit(CONTENT, 1000.0, 200.0);
        assert_eq!(v.scale, 1.0);
        assert_eq!((v.tx, v.ty), (300.0, 0.0));
    }

    #[test]
    fn fit_honors_content_origin() {
        let v = View::fit(
            Bounds {
                x: 50.0,
                y: 10.0,
                ..CONTENT
            },
            400.0,
            200.0,
        );
        assert_eq!(v.to_window(50.0, 10.0), (0.0, 0.0));
        assert_eq!(v.to_surface(400.0, 200.0), (450.0, 210.0));
    }

    #[test]
    fn window_and_surface_round_trip() {
        // A HiDPI window (2x) of 900x700 logical = 1800x1400 physical.
        let v = View::fit(CONTENT, 1800.0, 1400.0);
        for (x, y) in [(0.0, 0.0), (123.5, 67.25), (400.0, 200.0)] {
            let (wx, wy) = v.to_window(x, y);
            let (sx, sy) = v.to_surface(wx, wy);
            assert!((sx - x).abs() < 1e-9 && (sy - y).abs() < 1e-9);
        }
        let p = v.affine() * kurbo::Point::new(10.0, 20.0);
        assert_eq!((p.x, p.y), v.to_window(10.0, 20.0));
    }

    #[test]
    fn degenerate_content_does_not_divide_by_zero() {
        let v = View::fit(Bounds::default(), 800.0, 600.0);
        assert_eq!(v.scale, 1.0);
        assert!(v.to_surface(5.0, 5.0).0.is_finite());
    }

    #[test]
    fn double_click_needs_time_and_distance() {
        let mut c = Clicks::default();
        assert!(!c.press(10.0, 10.0, 0.0));
        assert!(c.press(12.0, 13.0, 300.0), "within 350ms and 4px");
        // Third press starts over.
        assert!(!c.press(12.0, 13.0, 400.0));
        // Too slow.
        assert!(!c.press(12.0, 13.0, 800.0));
        // Too far.
        assert!(!c.press(20.0, 13.0, 900.0));
        // Exactly at the limits still counts.
        assert!(c.press(24.0, 13.0, 1250.0));
    }

    #[test]
    fn modifiers_map_shift_to_fine_and_ctrl_or_cmd_to_alt() {
        assert_eq!(
            modifiers(ModifiersState::SHIFT),
            Modifiers {
                fine: true,
                ..Modifiers::default()
            }
        );
        assert_eq!(
            modifiers(ModifiersState::CONTROL),
            Modifiers {
                alt: true,
                ..Modifiers::default()
            }
        );
        assert_eq!(
            modifiers(ModifiersState::SUPER),
            Modifiers {
                alt: true,
                ..Modifiers::default()
            }
        );
        assert_eq!(modifiers(ModifiersState::empty()), Modifiers::default());
    }

    #[test]
    fn keys_map_to_control_keys() {
        assert_eq!(key(&WinitKey::Named(NamedKey::ArrowUp)), Some(Key::Up));
        assert_eq!(
            key(&WinitKey::Named(NamedKey::PageDown)),
            Some(Key::PageDown)
        );
        assert_eq!(key(&WinitKey::Named(NamedKey::Enter)), Some(Key::Enter));
        assert_eq!(key(&WinitKey::Named(NamedKey::Escape)), Some(Key::Escape));
        assert_eq!(
            key(&WinitKey::Named(NamedKey::Backspace)),
            Some(Key::Backspace)
        );
        assert_eq!(key(&WinitKey::Character("a".into())), Some(Key::Char('a')));
        assert_eq!(key(&WinitKey::Character("ab".into())), None);
    }

    #[test]
    fn wheel_lines_and_pixels_become_notches() {
        assert_eq!(
            wheel_notches(MouseScrollDelta::LineDelta(0.0, 2.0), 1.0),
            2.0
        );
        assert_eq!(
            wheel_notches(
                MouseScrollDelta::PixelDelta(PhysicalPosition::new(0.0, 40.0)),
                2.0
            ),
            1.0
        );
    }
}
