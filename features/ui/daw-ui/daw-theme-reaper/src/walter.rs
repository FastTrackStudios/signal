//! WALTER interpreter — evaluates an `rtconfig.txt` layout program.
//!
//! Implements the language per `reaper-theme/docs/walter-reference.md` (and
//! <https://www.reaper.fm/sdk/walter/walter.php>): values are **coordinate
//! lists** (`Vec<f32>`, omitted trailing values = 0), assigned to element
//! attributes / user variables with `set`, factored with `def` (textual
//! substitution) and `macro … endmacro` (`##` concatenation), scoped by
//! `Layout "name" … EndLayout` blocks, and computed with prefix arithmetic,
//! inline conditional chains, list indexing (`name{n}`), and sparse
//! placement (`expr@slot`).
//!
//! Evaluation is **concrete**: the caller supplies an [`Env`] (panel `w`/`h`,
//! the track-state scalars REAPER exposes, `define_parameter` values), the
//! interpreter runs the program top-down, and the result is the final
//! attribute map. Re-evaluate when the environment changes — WALTER has no
//! hidden state between runs.
//!
//! Semantics per White Tie's *WALTER: A themer's guide* (the authoritative
//! companion to the SDK), verified against the Anti-Theme, Reapertips,
//! Neptune VI and Imperial:
//! - conditions are **strictly binary Polish notation** — `question
//!   true-answer false-answer`, nestable in either answer
//!   (`w>=230 h<40 A B C` = w≥230 ? (h<40 ? A : B) : C); a missing
//!   false-answer at end of statement leaves the value unchanged;
//! - condition tokens: comparisons (`a<b a<=b a==b a!=b …`), truthiness
//!   (`?x` nonzero, `!x` zero), bitwise AND (`a&b`);
//! - arithmetic is element-wise by list position with missing components
//!   read as 0; bare scalars broadcast, `[x]` literals stay positional;
//! - `def` is preprocessor-level token substitution; `set` evaluates.

use std::collections::HashMap;

/// The runtime environment: built-in scalars (`w`, `h`, `trackpanmode`,
/// `recarm`, `track_selected`, `trackcolor_r/g/b`, …) and theme parameter
/// values (`define_parameter` knobs, referenced by name).
#[derive(Clone, Debug, Default)]
pub struct Env {
    pub scalars: HashMap<String, f32>,
}

impl Env {
    pub fn new() -> Self {
        Self::default()
    }
    #[must_use]
    pub fn with(mut self, name: &str, value: f32) -> Self {
        self.scalars.insert(name.to_string(), value);
        self
    }
    pub fn set(&mut self, name: &str, value: f32) {
        self.scalars.insert(name.to_string(), value);
    }

    /// A baseline REAPER environment for a panel of `w`×`h`: a plain,
    /// unselected, unarmed stereo track at 100% DPI scale. Callers override
    /// the track-state scalars that differ.
    pub fn reaper_defaults(w: f32, h: f32) -> Self {
        let mut env = Self::new();
        for (k, v) in [
            ("w", w),
            ("h", h),
            ("trackpanmode", 3.0),
            ("tracknch", 2.0),
            ("recarm", 0.0),
            ("track_selected", 0.0),
            ("folderstate", 0.0),
            ("folderdepth", 0.0),
            ("maxfolderdepth", 0.0),
            ("mcp_maxfolderdepth", 0.0),
            ("trackcolor_valid", 0.0),
            ("trackidx", 1.0),
            ("ntracks", 1.0),
            ("mixer_visible", 1.0),
            ("send_cnt", 0.0),
            ("fx_cnt", 0.0),
            ("fx_parm_cnt", 0.0),
            ("recfx_cnt", 0.0),
            ("mcp_wantextmix", 0.0),
            ("tcp_sends_enabled", 1.0),
            ("tcp_fxlist_enabled", 1.0),
            ("tcp_fxparms", 0.0),
            ("tcp_fxembed", 0.0),
            ("mcp_fxembed", 0.0),
            ("trackfixedlanes", 0.0),
            ("trackpinned", 0.0),
            ("tcp_hidden_overridden", 0.0),
            // Version is an integer ×100 (the guide: "example: 414" = 4.14;
            // themes gate on `reaper_version<700`).
            ("reaper_version", 723.0),
            ("os_type", 2.0),
            // The DPI scale variable theme macros multiply by (1.0 = 100%).
            ("Scale", 1.0),
        ] {
            env.set(k, v);
        }
        env
    }
}

/// Evaluation result: the final value of every assigned attribute/variable
/// (element attributes keep their dots: `mcp.volume`, `mcp.label.color`), the
/// `front` order, and the layout names the program declares.
#[derive(Clone, Debug, Default)]
pub struct Output {
    pub attrs: HashMap<String, Vec<f32>>,
    pub fronts: Vec<String>,
    pub layouts: Vec<String>,
    /// Attribute names in first-assignment order — WALTER's z-order for
    /// custom elements (later `set`s paint over earlier ones).
    pub set_order: Vec<String>,
    /// `custom` declarations that carry a button image (5th arg): element
    /// name → image name (e.g. `tcp.custom.labelBlockBg` → `tcp_labelBlock_bg`).
    pub custom_images: std::collections::HashMap<String, String>,
}

impl Output {
    /// An attribute's value, if assigned.
    pub fn get(&self, name: &str) -> Option<&[f32]> {
        self.attrs.get(name).map(|v| v.as_slice())
    }
    /// An attribute as the 8-value coordinate list (missing trailing = 0).
    pub fn coord(&self, name: &str) -> Option<[f32; 8]> {
        let v = self.attrs.get(name)?;
        let mut out = [0.0; 8];
        for (i, x) in v.iter().take(8).enumerate() {
            out[i] = *x;
        }
        Some(out)
    }
}

/// Evaluate a WALTER program. `layout` selects which `Layout "name"` blocks
/// run (in addition to top-level statements); `None` runs only the top level.
pub fn evaluate(src: &str, layout: Option<&str>, env: &Env) -> Output {
    let lines = preprocess(src);
    let lines = expand_macros(&lines);
    let mut interp = Interp {
        env,
        vars: HashMap::new(),
        defs: HashMap::new(),
        out: Output::default(),
    };
    interp.run(&lines, layout);
    interp.out
}

// ─────────────────────────────────────────────────────────────────────────────
// Preprocessing: comments, continuations, tokenization.
// ─────────────────────────────────────────────────────────────────────────────

/// Strip `;` comments, join `\` continuations, tokenize each line.
/// `[` / `]` become their own tokens.
fn preprocess(src: &str) -> Vec<Vec<String>> {
    let mut logical: Vec<String> = Vec::new();
    let mut pending = String::new();
    for raw in src.lines() {
        let line = match raw.split_once(';') {
            Some((before, _)) => before,
            None => raw,
        };
        let trimmed = line.trim_end();
        if let Some(stripped) = trimmed.strip_suffix('\\') {
            pending.push_str(stripped);
            pending.push(' ');
            continue;
        }
        pending.push_str(trimmed);
        if !pending.trim().is_empty() {
            logical.push(std::mem::take(&mut pending));
        } else {
            pending.clear();
        }
    }
    if !pending.trim().is_empty() {
        logical.push(pending);
    }
    logical.iter().map(|l| tokenize(l)).collect()
}

/// Split a logical line into tokens; brackets separate, quotes group.
fn tokenize(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quote: Option<char> = None;
    let flush = |cur: &mut String, out: &mut Vec<String>| {
        if !cur.is_empty() {
            out.push(std::mem::take(cur));
        }
    };
    for c in line.chars() {
        match quote {
            Some(q) if c == q => quote = None,
            Some(_) => cur.push(c),
            None => match c {
                '\'' | '"' => quote = Some(c),
                '[' | ']' => {
                    flush(&mut cur, &mut out);
                    out.push(c.to_string());
                }
                c if c.is_whitespace() => flush(&mut cur, &mut out),
                _ => cur.push(c),
            },
        }
    }
    flush(&mut cur, &mut out);
    out
}

// ─────────────────────────────────────────────────────────────────────────────
// Macro expansion (`macro name params … endmacro`, invocation, `##`).
// ─────────────────────────────────────────────────────────────────────────────

struct Macro {
    params: Vec<String>,
    body: Vec<Vec<String>>,
}

fn expand_macros(lines: &[Vec<String>]) -> Vec<Vec<String>> {
    let mut macros: HashMap<String, Macro> = HashMap::new();
    let mut out: Vec<Vec<String>> = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let line = &lines[i];
        let head = line[0].to_ascii_lowercase();
        if head == "macro" {
            let name = line.get(1).cloned().unwrap_or_default();
            let params: Vec<String> = line[2..].to_vec();
            let mut body = Vec::new();
            i += 1;
            while i < lines.len() && !lines[i][0].eq_ignore_ascii_case("endmacro") {
                body.push(lines[i].clone());
                i += 1;
            }
            macros.insert(name, Macro { params, body });
        } else if let Some(mac) = macros.get(&line[0]) {
            let args: Vec<String> = line[1..].to_vec();
            expand_invocation(mac, &args, &macros, &mut out, 0);
        } else {
            out.push(line.clone());
        }
        i += 1;
    }
    out
}

fn expand_invocation(
    mac: &Macro,
    args: &[String],
    macros: &HashMap<String, Macro>,
    out: &mut Vec<Vec<String>>,
    depth: usize,
) {
    if depth > 32 {
        return; // recursion guard
    }
    // `##` splices param values into identifiers; params also substitute at
    // word boundaries *inside* tokens — condition tokens embed them
    // (`mcpForm{0}==Form{0}`), indexed reads suffix them (`this{1}`).
    let is_word = |c: char| c.is_ascii_alphanumeric() || c == '_';
    let subst_part = |part: &str| -> String {
        let mut s = part.to_string();
        for (idx, param) in mac.params.iter().enumerate() {
            let arg = args.get(idx).map(String::as_str).unwrap_or("");
            let mut out = String::with_capacity(s.len());
            let mut rest = s.as_str();
            loop {
                match rest.find(param.as_str()) {
                    Some(i) => {
                        let before_ok =
                            i == 0 || !is_word(rest[..i].chars().next_back().unwrap_or(' '));
                        let after = &rest[i + param.len()..];
                        let after_ok = !is_word(after.chars().next().unwrap_or(' '));
                        out.push_str(&rest[..i]);
                        if before_ok && after_ok {
                            out.push_str(arg);
                        } else {
                            out.push_str(param);
                        }
                        rest = after;
                    }
                    None => {
                        out.push_str(rest);
                        break;
                    }
                }
            }
            s = out;
        }
        s
    };
    let subst =
        |tok: &str| -> String { tok.split("##").map(subst_part).collect::<Vec<_>>().concat() };
    for body_line in &mac.body {
        let line: Vec<String> = body_line.iter().map(|t| subst(t)).collect();
        if line.is_empty() {
            continue;
        }
        // Nested macro invocations expand recursively.
        if let Some(inner) = macros.get(&line[0]) {
            expand_invocation(inner, &line[1..], macros, out, depth + 1);
        } else {
            out.push(line);
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Interpreter.
// ─────────────────────────────────────────────────────────────────────────────

/// A runtime value: components + whether it is a *scalar* (broadcasts in
/// arithmetic). `[x]` literals are 1-long **lists** (pad with 0); bare
/// numbers, env scalars and indexed reads are scalars.
#[derive(Clone, Debug, Default)]
struct Val {
    v: Vec<f32>,
    scalar: bool,
}

impl Val {
    fn scalar(x: f32) -> Self {
        Self {
            v: vec![x],
            scalar: true,
        }
    }
    fn list(v: Vec<f32>) -> Self {
        Self { v, scalar: false }
    }
    fn first(&self) -> f32 {
        self.v.first().copied().unwrap_or(0.0)
    }
}

struct Interp<'a> {
    env: &'a Env,
    /// Evaluated user variables + element attributes (shared namespace —
    /// expressions read attributes like variables).
    vars: HashMap<String, Val>,
    /// `def` textual substitutions (token lists).
    defs: HashMap<String, Vec<String>>,
    out: Output,
}

impl Interp<'_> {
    fn run(&mut self, lines: &[Vec<String>], layout: Option<&str>) {
        let mut i = 0;
        while i < lines.len() {
            let line = &lines[i];
            let head = line[0].to_ascii_lowercase();
            match head.as_str() {
                "layout" | "globallayout" => {
                    let name = line.get(1).cloned().unwrap_or_default();
                    if !self.out.layouts.contains(&name) {
                        self.out.layouts.push(name.clone());
                    }
                    // Collect the block (nested layouts included).
                    let mut depth = 1;
                    let mut block: Vec<Vec<String>> = Vec::new();
                    i += 1;
                    while i < lines.len() && depth > 0 {
                        let h = lines[i][0].to_ascii_lowercase();
                        if h == "layout" || h == "globallayout" {
                            depth += 1;
                        } else if h == "endlayout" {
                            depth -= 1;
                            if depth == 0 {
                                break;
                            }
                        }
                        block.push(lines[i].clone());
                        i += 1;
                    }
                    if layout == Some(name.as_str()) {
                        // Run the selected layout's statements (nested layout
                        // blocks inside it are *scale variants*; they only run
                        // when selected themselves — skipped here).
                        self.run(&block, None);
                    }
                }
                "endlayout" => {}
                "set" => self.exec_set(line),
                "def" if line.len() >= 2 => {
                    self.defs.insert(line[1].clone(), line[2..].to_vec());
                }
                "clear" => {
                    if let Some(pat) = line.get(1) {
                        self.clear(pat);
                    }
                }
                "reset" => {
                    // No theme-default table here: reset behaves like clear.
                    if let Some(pat) = line.get(1) {
                        self.clear(pat);
                    }
                }
                "front" => {
                    for name in &line[1..] {
                        self.out.fronts.push(name.clone());
                    }
                }
                // `custom <name> <text_label> <command_id> <accessibility>
                // <button_image>` (7.0+). The declaration itself just creates
                // the element; capture the button-image name (5th arg, quoted)
                // so renderers can draw image-backed customs
                // (`tcp_labelBlock_bg` is the REAPER 7 name-field art).
                "custom" => {
                    if let Some(name) = line.get(1) {
                        // The button image is the *last* argument. The
                        // tokenizer drops empty quoted args (`''`), so arg
                        // positions shift — take the last non-numeric token
                        // (consumers validate it against the image catalog,
                        // so a text label here resolves to nothing).
                        let img = (line.len() >= 3)
                            .then(|| line.last())
                            .flatten()
                            .filter(|s| !s.is_empty() && s.parse::<f64>().is_err());
                        if let Some(img) = img {
                            self.out.custom_images.insert(name.clone(), img.to_string());
                        }
                    }
                }
                // Globals / define_parameter / version gates etc. are handled
                // by RtConfig; the interpreter skips what it doesn't execute.
                _ => {}
            }
            i += 1;
        }
        // Final attribute map = everything assigned.
        for (k, v) in &self.vars {
            self.out.attrs.insert(k.clone(), v.v.clone());
        }
    }

    fn clear(&mut self, pattern: &str) {
        if let Some(prefix) = pattern.strip_suffix('*') {
            self.vars.retain(|k, _| !k.starts_with(prefix));
        } else {
            self.vars.remove(pattern);
        }
    }

    fn exec_set(&mut self, line: &[String]) {
        let Some(dest) = line.get(1) else { return };
        let toks = self.substitute_defs(&line[2..]);
        let current = self.lookup(dest).unwrap_or_default();
        let tracing = std::env::var("WALTER_TRACE").is_ok_and(|f| dest.contains(&f));
        TRACE_OPS.store(tracing, std::sync::atomic::Ordering::Relaxed);
        let value = self.eval_chain(&toks, dest, &current);
        TRACE_OPS.store(false, std::sync::atomic::Ordering::Relaxed);
        // Debug trace: `WALTER_TRACE=<substring>` prints matching assignments.
        if let Ok(filter) = std::env::var("WALTER_TRACE")
            && dest.contains(&filter)
        {
            eprintln!("set {dest} = {:?} <- {}", value.v, toks.join(" "));
            for name in toks
                .iter()
                .filter(|t| t.len() > 2 && !t.contains(['[', ']']) && t.parse::<f32>().is_err())
            {
                let base = name.split(['{', '@']).next().unwrap_or(name);
                if let Some(v) = self.lookup(base) {
                    eprintln!("    {base} = {:?} (scalar={})", v.v, v.scalar);
                }
            }
        }
        if !self.vars.contains_key(dest) {
            self.out.set_order.push(dest.clone());
        }
        self.vars.insert(dest.clone(), value);
    }

    /// Apply `def` substitutions (token-level, recursive one pass).
    fn substitute_defs(&self, toks: &[String]) -> Vec<String> {
        let mut out = Vec::with_capacity(toks.len());
        for t in toks {
            match self.defs.get(t) {
                Some(rep) => out.extend(self.substitute_defs(rep)),
                None => out.push(t.clone()),
            }
        }
        out
    }

    /// Resolve a name: numeric literal, user var / element attribute, or env
    /// scalar. Numbers resolve **first** — macro expansion can produce lines
    /// like `set 1 …` (a parameter substituted into a dest), and a literal
    /// `1` in an expression must never read that accidental variable.
    fn lookup(&self, name: &str) -> Option<Val> {
        if let Ok(n) = name.parse::<f32>() {
            return Some(Val::scalar(n));
        }
        if let Some(v) = self.vars.get(name) {
            return Some(v.clone());
        }
        self.env.scalars.get(name).map(|s| Val::scalar(*s))
    }

    // ── expression evaluation ──

    /// Evaluate a conditional chain: `cond expr cond expr … final-expr`.
    /// `dest`/`current` feed the "keep current value" forms (`.`, dest name,
    /// or a chain with no final else).
    /// Evaluate a full `set` expression (one binary-conditional Polish
    /// expression). An empty token list keeps the current value.
    fn eval_chain(&mut self, toks: &[String], dest: &str, current: &Val) -> Val {
        if toks.is_empty() {
            return current.clone();
        }
        self.eval_expr(toks, 0, dest, current).0
    }

    /// A condition token: `?name` (nonzero), `!name` (zero), or an embedded
    /// comparison `a<b a>b a<=b a>=b a==b a!=b` between scalar operands.
    fn parse_condition(&mut self, tok: &str) -> Option<bool> {
        if let Some(name) = tok.strip_prefix('?') {
            return Some(self.scalar_operand(name) != 0.0);
        }
        if let Some(name) = tok.strip_prefix('!') {
            // `!=` inside a comparison is handled below; a leading `!` with a
            // comparator in the rest is still a comparison token.
            if !name.contains(['<', '>', '=']) {
                return Some(self.scalar_operand(name) == 0.0);
            }
        }
        // `a&b` — bitwise AND as a truth test (`fish{0}&chips{0}`).
        if let Some((l, r)) = tok.split_once('&')
            && !l.is_empty()
            && !r.is_empty()
            && !tok.contains(['<', '>', '='])
        {
            let a = self.scalar_operand(l) as i64;
            let b = self.scalar_operand(r) as i64;
            return Some((a & b) != 0);
        }
        for op in ["<=", ">=", "==", "!=", "<", ">"] {
            if let Some(idx) = tok.find(op) {
                // Guard against matching `<` of `<=` etc: ordered list above.
                let (l, r) = (&tok[..idx], &tok[idx + op.len()..]);
                if l.is_empty() || r.is_empty() {
                    continue;
                }
                let a = self.scalar_operand(l);
                let b = self.scalar_operand(r);
                return Some(match op {
                    "<=" => a <= b,
                    ">=" => a >= b,
                    "==" => a == b,
                    "!=" => a != b,
                    "<" => a < b,
                    ">" => a > b,
                    _ => unreachable!(),
                });
            }
        }
        None
    }

    /// A scalar operand inside a condition: a number, `name`, or `name{i}`.
    fn scalar_operand(&mut self, s: &str) -> f32 {
        self.eval_atom(s).first()
    }

    /// Evaluate one value expression starting at `pos`; returns (value, next).
    fn eval_expr(
        &mut self,
        toks: &[String],
        pos: usize,
        dest: &str,
        current: &Val,
    ) -> (Val, usize) {
        let Some(tok) = toks.get(pos) else {
            return (current.clone(), pos);
        };
        if std::env::var("WALTER_TRACE_EXPR").is_ok() {
            eprintln!("  expr@{pos}: {tok}");
        }
        // Stray `]` (theme typos like `name{3}]==0` — the Anti-Theme ships
        // one): treat the rest of the statement as malformed and keep the
        // current value, as REAPER's lenient parser effectively does.
        if tok == "]" {
            return (current.clone(), toks.len());
        }
        // Conditions are strictly binary Polish notation (the WALTER
        // themer's guide): `question true-answer false-answer`, nestable in
        // either answer (`w>=230 h<40 A B C` = w≥230 ? (h<40 ? A : B) : C).
        // A missing false-answer at end of statement keeps the current value
        // ("WALTER does nothing").
        if let Some(cond) = self.parse_condition(tok) {
            let (t, p1) = self.eval_expr(toks, pos + 1, dest, current);
            let (f, p2) = if p1 < toks.len() {
                self.eval_expr(toks, p1, dest, current)
            } else {
                (current.clone(), p1)
            };
            return (if cond { t } else { f }, p2);
        }
        match tok.as_str() {
            "[" => self.eval_list(toks, pos, dest, current),
            "." => (current.clone(), pos + 1),
            "+" | "-" | "*" | "/" => {
                let op = tok.clone();
                let (a, p1) = self.eval_expr(toks, pos + 1, dest, current);
                let (b, p2) = self.eval_expr(toks, p1, dest, current);
                (apply_op(&op, &a, &b), p2)
            }
            t if t.starts_with("+:") || t.starts_with("*:") => {
                // `+:a:b` → a·A + b·B ; `*:a:b` → (A+[a a…])·(B+[b b…]).
                let parts: Vec<&str> = t[2..].split(':').collect();
                let a_k = parts
                    .first()
                    .and_then(|s| s.parse::<f32>().ok())
                    .unwrap_or(0.0);
                let b_k = parts
                    .get(1)
                    .and_then(|s| s.parse::<f32>().ok())
                    .unwrap_or(0.0);
                let mul = t.starts_with("*:");
                let (a, p1) = self.eval_expr(toks, pos + 1, dest, current);
                let (b, p2) = self.eval_expr(toks, p1, dest, current);
                let map = |val: &Val, k: f32, add: bool| -> Val {
                    Val {
                        v: val
                            .v
                            .iter()
                            .map(|x| if add { x + k } else { x * k })
                            .collect(),
                        scalar: val.scalar,
                    }
                };
                let v = if mul {
                    apply_op("*", &map(&a, a_k, true), &map(&b, b_k, true))
                } else {
                    apply_op("+", &map(&a, a_k, false), &map(&b, b_k, false))
                };
                (v, p2)
            }
            t if t == dest => (current.clone(), pos + 1),
            _ => (self.eval_atom(tok), pos + 1),
        }
    }

    /// Evaluate a bracketed list. Bare names inside use slot-matched
    /// indexing (`[0 0 tmp tmp]` ≡ `[0 0 tmp{2} tmp{3}]`).
    fn eval_list(
        &mut self,
        toks: &[String],
        open: usize,
        dest: &str,
        current: &Val,
    ) -> (Val, usize) {
        let mut vals: Vec<f32> = Vec::new();
        let mut pos = open + 1;
        while pos < toks.len() && toks[pos] != "]" {
            let tok = &toks[pos];
            let slot = vals.len();
            // Bare list-valued name → slot-matched component.
            let v = if tok == "." || tok == dest {
                current.v.get(slot).copied().unwrap_or(0.0)
            } else {
                self.eval_atom_slot(tok, slot).unwrap_or(0.0)
            };
            vals.push(v);
            pos += 1;
        }
        (Val::list(vals), pos + 1)
    }

    /// Evaluate an atom used at a list slot: numbers and explicit indexing
    /// give their value; bare names of list-valued vars use the slot index.
    fn eval_atom_slot(&mut self, tok: &str, slot: usize) -> Option<f32> {
        if let Ok(n) = tok.parse::<f32>() {
            return Some(n);
        }
        if tok.contains('{') || tok.contains('@') {
            return Some(self.eval_atom(tok).first());
        }
        let v = self.lookup_atom(tok)?;
        if v.scalar || v.v.len() == 1 {
            Some(v.first())
        } else {
            Some(v.v.get(slot).copied().unwrap_or(0.0))
        }
    }

    /// Evaluate an atomic token: `name`, `name{idx}`, `expr@slot`, number.
    fn eval_atom(&mut self, tok: &str) -> Val {
        // `expr@slot` — place a scalar into a coordinate slot.
        if let Some((lhs, slot)) = tok.rsplit_once('@') {
            let idx = slot_index(slot);
            let val = self.eval_atom(lhs).first();
            let mut v = vec![0.0; idx + 1];
            v[idx] = val;
            return Val::list(v);
        }
        // `name{i}` — indexed read yields a scalar.
        if let Some((name, rest)) = tok.split_once('{') {
            let idx_str = rest.trim_end_matches('}');
            let idx = slot_index(idx_str);
            let base = self.lookup_atom(name).unwrap_or_default();
            return Val::scalar(base.v.get(idx).copied().unwrap_or(0.0));
        }
        self.lookup_atom(tok).unwrap_or_default()
    }

    fn lookup_atom(&self, name: &str) -> Option<Val> {
        self.lookup(name)
    }
}

/// Slot aliases: `x y w h ls ts rs bs` → 0..7; numeric strings parse.
fn slot_index(s: &str) -> usize {
    match s {
        "x" => 0,
        "y" => 1,
        "w" => 2,
        "h" => 3,
        "ls" => 4,
        "ts" => 5,
        "rs" => 6,
        "bs" => 7,
        _ => s.parse::<usize>().unwrap_or(0),
    }
}

static TRACE_OPS: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Element-wise arithmetic; **scalars broadcast**, list components missing
/// at an index read 0 (so `[x]` literals stay positional).
fn apply_op(op: &str, a: &Val, b: &Val) -> Val {
    if TRACE_OPS.load(std::sync::atomic::Ordering::Relaxed) {
        eprintln!(
            "  op {op} {:?}(s={}) {:?}(s={})",
            a.v, a.scalar, b.v, b.scalar
        );
    }
    let len = a.v.len().max(b.v.len()).max(1);
    let get = |val: &Val, i: usize| -> f32 {
        if val.scalar {
            val.first()
        } else {
            val.v.get(i).copied().unwrap_or(0.0)
        }
    };
    let v = (0..len)
        .map(|i| {
            let (x, y) = (get(a, i), get(b, i));
            match op {
                "+" => x + y,
                "-" => x - y,
                "*" => x * y,
                "/" => {
                    if y == 0.0 {
                        0.0
                    } else {
                        x / y
                    }
                }
                _ => 0.0,
            }
        })
        .collect();
    Val {
        v,
        scalar: a.scalar && b.scalar,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn eval(src: &str) -> Output {
        evaluate(src, None, &Env::new().with("w", 100.0).with("h", 200.0))
    }

    #[test]
    fn set_literal_lists_pad_with_zero() {
        let out = eval("set tcp.mute [10]\nset a [1 2 3 4 5 6 7 8]");
        assert_eq!(
            out.coord("tcp.mute").unwrap(),
            [10., 0., 0., 0., 0., 0., 0., 0.]
        );
        assert_eq!(out.get("a").unwrap(), &[1., 2., 3., 4., 5., 6., 7., 8.]);
    }

    #[test]
    fn def_substitutes_tokens() {
        let out = eval("def square10 10 10\nset tcp.mute [0 0 square10]");
        // `square10` expands to two tokens inside the list.
        assert_eq!(out.coord("tcp.mute").unwrap()[..4], [0., 0., 10., 10.]);
    }

    #[test]
    fn bare_name_in_list_uses_slot_index() {
        let out = eval("set tmp [9 8 7 6]\nset foo [0 0 tmp tmp]");
        assert_eq!(out.coord("foo").unwrap()[..4], [0., 0., 7., 6.]);
    }

    #[test]
    fn conditionals_chain_and_keep_current() {
        // w = 100.
        let out = eval("set v [5]\nset v w<100 [0]");
        assert_eq!(out.get("v").unwrap(), &[5.0]); // condition false → keep
        let out = eval("set v w<200 [1] [2]");
        assert_eq!(out.get("v").unwrap(), &[1.0]);
        let out = eval("set v w<50 [1] w<150 [2] [3]");
        assert_eq!(out.get("v").unwrap(), &[2.0]);
        let out = eval("set v w<50 [1] w<60 [2] [3]");
        assert_eq!(out.get("v").unwrap(), &[3.0]);
    }

    #[test]
    fn truthiness_conditions() {
        let env = Env::new().with("recarm", 1.0).with("muted", 0.0);
        let out = evaluate("set a ?recarm [1] [2]\nset b !muted [3] [4]", None, &env);
        assert_eq!(out.get("a").unwrap(), &[1.0]);
        assert_eq!(out.get("b").unwrap(), &[3.0]);
    }

    #[test]
    fn prefix_arithmetic_and_broadcast() {
        let out = eval("set a + [1 2 3] [10 20 30]\nset s 2\nset b * s [1 2 3 4]");
        assert_eq!(out.get("a").unwrap(), &[11., 22., 33.]);
        // Bare scalars broadcast…
        assert_eq!(out.get("b").unwrap(), &[2., 4., 6., 8.]);
        // …but 1-element list literals stay positional (pad with 0).
        let out = eval("set b * [2] [1 2 3 4]");
        assert_eq!(out.get("b").unwrap(), &[2., 0., 0., 0.]);
        // Nested prefix: - A (* B C)
        let out = eval("set a - [10 10] * [2 2] [3 3]");
        assert_eq!(out.get("a").unwrap(), &[4., 4.]);
    }

    #[test]
    fn indexing_and_at_slot() {
        let out = eval("set tmp [1 2 3 4 5 6 7 8]\nset a [tmp{w}]\nset b 1.5@w\nset c recarm@w");
        assert_eq!(out.get("a").unwrap(), &[3.0]);
        assert_eq!(out.get("b").unwrap(), &[0., 0., 1.5]);
        // `recarm` missing from env → 0 in the w slot.
        assert_eq!(out.get("c").unwrap(), &[0., 0., 0.]);
    }

    #[test]
    fn attribute_references_read_assigned_values() {
        let out = eval("set base [80 60 0 0]\nset tcp.solo [0 0 base{x} base{1}]");
        assert_eq!(out.coord("tcp.solo").unwrap()[..4], [0., 0., 80., 60.]);
    }

    #[test]
    fn layouts_run_only_when_selected() {
        let src = "set a [1]\nLayout \"Mixer\"\nset a [2]\nEndLayout\nset b [3]";
        let none = evaluate(src, None, &Env::new());
        assert_eq!(none.get("a").unwrap(), &[1.0]);
        assert_eq!(none.get("b").unwrap(), &[3.0]);
        let sel = evaluate(src, Some("Mixer"), &Env::new());
        assert_eq!(sel.get("a").unwrap(), &[2.0]);
        assert_eq!(sel.layouts, vec!["Mixer".to_string()]);
    }

    #[test]
    fn clear_supports_wildcards() {
        let out = eval("set tcp.mute [1]\nset tcp.solo [2]\nclear tcp.*\nset x [9]");
        assert!(out.get("tcp.mute").is_none());
        assert!(out.get("tcp.solo").is_none());
        assert_eq!(out.get("x").unwrap(), &[9.0]);
    }

    #[test]
    fn macros_expand_with_concatenation() {
        let src = "\
macro setall dest src
set dest [src src src src]
set master.##dest [src]
endmacro
setall tcp.mute 1
";
        let out = eval(src);
        assert_eq!(out.coord("tcp.mute").unwrap()[..4], [1., 1., 1., 1.]);
        assert_eq!(out.get("master.tcp.mute").unwrap(), &[1.0]);
    }

    #[test]
    fn continuations_join_lines() {
        let out = eval("set a w<50 [1] \\\n w<150 [2] \\\n [3]");
        assert_eq!(out.get("a").unwrap(), &[2.0]);
    }

    #[test]
    fn dot_keeps_current_in_chain() {
        let out = eval("set a [7]\nset a w<200 . [9]");
        assert_eq!(out.get("a").unwrap(), &[7.0]);
    }

    #[test]
    fn true_position_nesting_is_binary() {
        // The themer's guide canonical example:
        // `w>=230 h<40 A B C` = w>=230 ? (h<40 ? A : B) : C.
        let src = "set s w>=230 h<40 [1] [2] [3]";
        let at = |w: f32, h: f32| {
            evaluate(src, None, &Env::new().with("w", w).with("h", h))
                .get("s")
                .unwrap()
                .to_vec()
        };
        assert_eq!(at(300.0, 30.0), vec![1.0]);
        assert_eq!(at(300.0, 100.0), vec![2.0]);
        assert_eq!(at(100.0, 30.0), vec![3.0]);
    }

    #[test]
    fn bitwise_and_condition() {
        let out = evaluate(
            "set fish [21]\nset chips [31]\nset a fish{0}&chips{0} [1] [2]\nset b fish{0}&8 [1] [2]",
            None,
            &Env::new(),
        );
        assert_eq!(out.get("a").unwrap(), &[1.0]); // 21 & 31 != 0
        assert_eq!(out.get("b").unwrap(), &[2.0]); // 21 & 8 == 0
    }

    #[test]
    fn stray_bracket_terminates_chain() {
        // Theme typos like `cond name{3}]==0 [0] …` (the Anti-Theme ships
        // one) must not clobber the current value with garbage: the false
        // condition skips its value, then the stray `]` ends the chain.
        let out = eval("set a [1 2 3 4]\nset a w<50 junk{3} ] ==0 [9]");
        assert_eq!(out.get("a").unwrap(), &[1., 2., 3., 4.]);
    }

    #[test]
    fn front_records_order() {
        let out = eval("front tcp.solo\nfront tcp.mute");
        assert_eq!(
            out.fronts,
            vec!["tcp.solo".to_string(), "tcp.mute".to_string()]
        );
    }
}
