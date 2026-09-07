# Fixing clippy findings in a DSP crate

The recipe handed to every fixer agent working through a `*-dsp` crate. It
exists because these lints have *one* correct answer each in this codebase, and
a fixer inventing its own produces code that compiles, passes the lint, and
changes the audio.

**The single rule: never change what the code computes.** Every crate under
conversion has bit-exact reference vectors in `tests/golden/`. A refactor that
moves one sample fails, and that is the point — so prefer the transformation
that is obviously arithmetic-preserving over the one that is merely shorter.

## Conversions — `as_conversions`, `cast_*`

Never write `as`. Use `dsp_core::num`, which is the one audited place a cast
may happen:

| you have | use |
|---|---|
| `n as f32` where `n: usize` | `num::count_to_f32(n)` |
| `n as f32` where `n: i32` | `num::i32_to_f32(n)` |
| `n as f32` where `n: u32` | `num::u32_to_f32(n)` |
| `x as usize` where `x: f32` | `num::f32_to_index(x)` |
| `x as usize` where `x: f64` | `num::f64_to_index(x)` |
| `x as f32` where `x: f64` | `num::narrow(x)` |
| `x as i32` where `x: f32` | `num::trunc_to_i32(x)` |
| `x.floor() as i32` | `num::floor_to_i32(x)` |
| `x.floor()` in `no_std` | `num::floor_f64(x)` |
| `n as f64` where `n: i32`/`u32` | `f64::from(n)` — infallible, no helper needed |
| `n as f64` where `n: usize` | `num::count_to_f64(n)` |
| `n as usize` where `n: u32` | `num::u32_to_index(n)` |
| `n as i32` where `n: usize` | `num::count_to_i32(n)` |
| `n as f64` where `n: u64` | `num::u64_to_f64(n)` |
| `n as f64` where `n: i64` | `num::i64_to_f64(n)` |
| `x as i64` where `x: f64` | `num::trunc_to_i64(x)` |
| `n as u64` where `n: usize` | `u64::try_from(n).unwrap_or(u64::MAX)` |

**`f64::from` does NOT accept `usize`, `u64` or `i64`.** The standard library
only provides infallible float conversions from integers that fit the mantissa
on every target, and those three do not. Writing `f64::from(n)` on a `usize` is
a compile error, not a lint fix — this was the single largest source of damage
on the first automated run of this recipe. Use the helpers above. Likewise
`count_to_f32` takes a `usize`, not a `u64`: pass `v.len()` directly rather
than wrapping it in `u64::try_from(..)`.

`num::` is `dsp_core::num`; add `use dsp_core::num;` if absent, and
`dsp-core = { workspace = true }` to `[dependencies]`.

Note the semantics you are preserving: Rust's float→int `as` **saturates** and
maps NaN to 0, and `f32_to_index` / `f64_to_index` / `trunc_to_i32` reproduce
that exactly. So swapping them in is arithmetic-neutral.

**The table above is the complete list.** If the conversion you need is not on
it, do not invent a helper name and do not reach for `f64::from` and hope —
say so in your summary and leave the line alone. Every automated run of this
recipe so far has produced at least one call to a `num::` function that does
not exist, which is a compile error rather than a fix.

## Never rewrite the arithmetic inside an assertion

This one has already destroyed working tests, so it is a rule and not a
preference: **do not apply `mul_add`, `ln_1p`, `exp_m1` or any other
arithmetic rewrite inside `assert!`, `assert_eq!`, or `debug_assert!`.** Leave
the assertion exactly as written and report it.

What went wrong: `assert!((p.wp - 0.9 * PI).abs() < 1e-12)` was rewritten to

```rust
assert!((p.wp - 0.9f64.mul_add(-PI, p.wp)).abs() < 1e-12);   // WRONG
```

The inner `mul_add` is correct on its own — `0.9f64.mul_add(-PI, p.wp)` *is*
`p.wp - 0.9 * PI`. But the outer `p.wp -` was left in place, so the assertion
now evaluates `A - (A - B*C)`, which is `B*C`, and asserts that 2.83 is less
than 1e-12. Five of seven assertions were rewritten this way in one run.

This is worse than a compile error. It compiles, it looks like a reasonable
diff, and it silently guts a test — in the one place the golden vectors cannot
see, because they only observe the code under test, not the tests themselves.
An inverted assertion that happens to still pass leaves no trace at all.

The performance argument does not apply in a test, so there is nothing to gain
and a working test to lose.

## Attributes go on items and statements, never expressions

`#[expect(..)]` on an expression is a hard error (E0658, "attributes on
expressions are experimental"). If a lint fires inside an expression, either
fix it properly — usually `try_from`, `saturating_*`, or one of the `num::`
helpers — or hoist the attribute to the enclosing `let` statement or function.

## Patterns bind by reference

Destructuring a slice of tuples binds references, so `let [(a, b), (c, d)] = w`
over a `&[(f64, f64)]` gives `a: &f64`. The fix is one deref on the pattern,
not a deref at every use:

```rust
let &[(q0, k0), (q1, k1)] = w else { continue };   // q0: f64
```

Also: `f64::to_bits()` returns `u64`, not `u32`.

## Indexing — `indexing_slicing`

Do **not** reach for `.get(i).unwrap_or(&0.0)`. A silent zero on an audio path
is a click, and the fallback value is a behaviour change.

**Per-channel state** is the common case, and it has a structural fix. When you
see several `[T; N]` fields indexed by the same clamped channel number:

```rust
// before
struct Stage { hold: [f32; MAX_CH], phase: [f32; MAX_CH], env: [f32; MAX_CH] }
fn process(&mut self, ch: usize, x: f32) -> f32 {
    let ch = ch.min(MAX_CH - 1);
    self.hold[ch] = x;                      // indexing_slicing, x3
    ...
}

// after
use dsp_core::{Channel, PerChannel};
/// Everything one channel remembers between samples.
#[derive(Debug, Clone, Copy, Default)]
struct ChannelState { hold: f32, phase: f32, env: f32 }
struct Stage { channels: PerChannel<ChannelState> }
fn process(&mut self, ch: usize, x: f32) -> f32 {
    let ch = Channel::new(ch.min(MAX_CH - 1));   // keep the ORIGINAL clamp
    self.channels[ch].hold = x;                  // total; no lint
    ...
}
```

Keep the original `.min(...)` inside `Channel::new`. `Channel` clamps to
`dsp_core::channel::MAX_CHANNELS` (8), which is usually wider than the crate's
own width, and dropping the crate's clamp would give channel 5 its own state
where it used to fold onto the last one. That is a behaviour change.

`PerChannel::filled(v)`, `.fill(v)`, `.iter()`, `.iter_mut()` are available;
`reset()` becomes `self.channels.fill(Default::default())`.

**Parallel arrays over bands/taps/sections**: fold them into one struct and
`zip` instead of walking a shared index.

```rust
// before                                  // after
for i in 0..BANDS {                        for (on, filter) in self.on.iter().zip(&mut self.filters) {
    if self.on[i] { self.filters[i]... }       if *on { filter... }
}                                          }
```

**Ring buffers** get a type that owns the invariant, with `get_mut` and a
defined (documented) answer for the empty case — see `Lookahead` in
`comp-dsp/src/chain.rs`.

**Fixed-size arrays** can be destructured, which is irrefutable and needs no
bounds check: `let [a, b, c, d] = self.history;`.

## Integer arithmetic — `arithmetic_side_effects`

Only fires on integers; float DSP math is untouched. Use `saturating_sub`,
`saturating_add`, `checked_rem(n).unwrap_or(0)`. Pick the one that cannot
change the result for in-range inputs: a counter that never goes below zero is
`saturating_sub(1)`.

## Float comparison — `float_cmp`

Usually the code means exactness (a bypassed stage returns its input
untouched). Say so with bit patterns — stronger than the tolerance the lint
wants, and it cannot drift:

```rust
assert_eq!(out.to_bits(), input.to_bits());
```

**Take `.abs()` first when the expected value is zero.** `-0.0` compares equal
to `0.0` under IEEE but has a different bit pattern, so a silence assertion
written as `assert_eq!(x.to_bits(), 0.0_f64.to_bits())` fails the moment the
code produces negative zero — which DSP code does constantly:

```rust
assert_eq!(x.abs().to_bits(), 0.0_f64.to_bits());
```

Only use an epsilon where the value is genuinely computed and approximate.

## `suboptimal_flops` / `imprecise_flops`

Apply them — the tree builds `x86-64-v3`, so `mul_add` is a single instruction.
**But two of clippy's own suggestions in this family do not compile**
(`(-2.0 / 3.0).mul_add(x, -0.25)` is E0689, ambiguous numeric type); write
`(-2.0_f64 / 3.0)`. Nothing in this family ships without a `cargo check`.

## When a reference vector is allowed to change

Almost never — but "never" would be a lie, so here is the actual rule.

The default is that a golden diff means you broke something. Two things make a
change legitimate, and **both** must hold:

1. The drift is at ULP scale. The harness reports an absolute delta and a
   relative figure in ppm for every drifted probe. `1e-16` at `0.000 ppm` on
   `f64` is one or two units in the last place; anything reaching whole ppm is
   a regression in a named fixture, no matter how plausible the diff looks.
2. You can name the cause, and it is an accuracy *improvement* — in practice
   `mul_add` or `ln_1p`, which round once where the original rounded twice.

Then re-record with `UPDATE_GOLDEN=1 cargo nextest run -p <crate>`, in a commit
that does nothing else, quoting the measured drift. Check the diff before you
commit it: only `hash` and `probe` lines may move, and a changed `len` or
`name` means the fixture itself changed rather than the audio.

If the cause is "I am not sure why it moved", that is not one of the two
conditions. Revert and bisect.

## Suppressions

`#[allow]` is denied. If a suppression is genuinely right, it is
`#[expect(clippy::lint, reason = "why")]`, which deletes itself when it stops
being needed. Prefer a real fix; `unused_variables` on a half-implemented
algorithm is a bug report, not a lint to silence.

## Do not restructure a loop you cannot compile

Converting `for i in 0..N { a[i]; b[i]; c[i] }` into a `zip` chain is the right
shape *when the arrays are few and the pattern is obvious*. It is the wrong
thing to attempt on a wide filter bank: nested `zip` produces deeply nested
tuple patterns, getting the arity wrong is a compile error at best and a
silently swapped state variable at worst, and the code where this is most
tempting (a 10-stage complex filter bank, a multi-tap delay) is exactly the
code where a swapped variable is least visible.

Rule of thumb: **at most two sequences in one `zip`.** Three or more, or a
`zip` whose pattern needs more than one level of nesting — leave the loop
alone, report it, and let a human or a stronger model do it against the
reference vectors.

Two files were reverted on the first automated run of this recipe for exactly
this. A third compiled, passed every unit test, and moved the audio by 10%.

## Never introduce a fallible or non-const call into a `const fn`

`TryFrom` is not a const trait and `Result::expect` is not a const method, so
`i8::try_from(i).expect(...)` inside a `const fn` does not compile — and
`expect` is denied by this workspace besides. If a `const fn` needs a narrowing
conversion, do it with const-legal arithmetic and a comment stating the range
the caller guarantees.

## What NOT to do

- Do not group a struct's `pub` fields into a new config struct. That breaks
  every call site in other files, and it has broken this repo twice.
- Do not turn `&self` methods into associated functions (`unused_self`).
- Do not "fix" a lint by widening or narrowing a public signature.
- Do not add or delete tests.
- If the only available fix is an API change, say so in your summary and leave
  the code alone.
