//! Best-effort LaTeX math to Unicode conversion for terminal display.

const SYMBOLS: &[(&str, &str)] = &[
    ("alpha", "α"),
    ("beta", "β"),
    ("gamma", "γ"),
    ("delta", "δ"),
    ("epsilon", "ε"),
    ("varepsilon", "ε"),
    ("zeta", "ζ"),
    ("eta", "η"),
    ("theta", "θ"),
    ("vartheta", "ϑ"),
    ("iota", "ι"),
    ("kappa", "κ"),
    ("lambda", "λ"),
    ("mu", "μ"),
    ("nu", "ν"),
    ("xi", "ξ"),
    ("pi", "π"),
    ("rho", "ρ"),
    ("sigma", "σ"),
    ("tau", "τ"),
    ("upsilon", "υ"),
    ("phi", "φ"),
    ("varphi", "φ"),
    ("chi", "χ"),
    ("psi", "ψ"),
    ("omega", "ω"),
    ("Gamma", "Γ"),
    ("Delta", "Δ"),
    ("Theta", "Θ"),
    ("Lambda", "Λ"),
    ("Xi", "Ξ"),
    ("Pi", "Π"),
    ("Sigma", "Σ"),
    ("Upsilon", "Υ"),
    ("Phi", "Φ"),
    ("Psi", "Ψ"),
    ("Omega", "Ω"),
    ("sum", "∑"),
    ("prod", "∏"),
    ("int", "∫"),
    ("iint", "∬"),
    ("oint", "∮"),
    ("infty", "∞"),
    ("partial", "∂"),
    ("nabla", "∇"),
    ("sqrt", "√"),
    ("cdot", "·"),
    ("cdots", "⋯"),
    ("ldots", "…"),
    ("dots", "…"),
    ("times", "×"),
    ("div", "÷"),
    ("pm", "±"),
    ("mp", "∓"),
    ("leq", "≤"),
    ("le", "≤"),
    ("geq", "≥"),
    ("ge", "≥"),
    ("neq", "≠"),
    ("ne", "≠"),
    ("approx", "≈"),
    ("equiv", "≡"),
    ("sim", "∼"),
    ("propto", "∝"),
    ("ll", "≪"),
    ("gg", "≫"),
    ("in", "∈"),
    ("notin", "∉"),
    ("subset", "⊂"),
    ("subseteq", "⊆"),
    ("supset", "⊃"),
    ("supseteq", "⊇"),
    ("cup", "∪"),
    ("cap", "∩"),
    ("emptyset", "∅"),
    ("forall", "∀"),
    ("exists", "∃"),
    ("neg", "¬"),
    ("land", "∧"),
    ("lor", "∨"),
    ("wedge", "∧"),
    ("vee", "∨"),
    ("to", "→"),
    ("rightarrow", "→"),
    ("leftarrow", "←"),
    ("Rightarrow", "⇒"),
    ("Leftarrow", "⇐"),
    ("leftrightarrow", "↔"),
    ("Leftrightarrow", "⇔"),
    ("iff", "⇔"),
    ("implies", "⇒"),
    ("mapsto", "↦"),
    ("uparrow", "↑"),
    ("downarrow", "↓"),
    ("angle", "∠"),
    ("perp", "⊥"),
    ("parallel", "∥"),
    ("circ", "∘"),
    ("bullet", "•"),
    ("star", "⋆"),
    ("deg", "°"),
    ("prime", "′"),
    ("hbar", "ℏ"),
    ("ell", "ℓ"),
    ("Re", "ℜ"),
    ("Im", "ℑ"),
    ("aleph", "ℵ"),
    ("mathbb{R}", "ℝ"),
    ("mathbb{N}", "ℕ"),
    ("mathbb{Z}", "ℤ"),
    ("mathbb{Q}", "ℚ"),
    ("mathbb{C}", "ℂ"),
    ("langle", "⟨"),
    ("rangle", "⟩"),
    ("lfloor", "⌊"),
    ("rfloor", "⌋"),
    ("lceil", "⌈"),
    ("rceil", "⌉"),
    ("quad", "  "),
    ("qquad", "    "),
    (",", " "),
    (";", " "),
    ("!", ""),
    (" ", " "),
    ("{", "{"),
    ("}", "}"),
    ("|", "‖"),
    ("%", "%"),
    ("$", "$"),
    ("&", "&"),
    ("_", "_"),
    ("log", "log"),
    ("ln", "ln"),
    ("exp", "exp"),
    ("sin", "sin"),
    ("cos", "cos"),
    ("tan", "tan"),
    ("lim", "lim"),
    ("max", "max"),
    ("min", "min"),
    ("det", "det"),
    ("left", ""),
    ("right", ""),
    ("big", ""),
    ("Big", ""),
    ("displaystyle", ""),
];

const SUPER: &[(char, char)] = &[
    ('0', '⁰'),
    ('1', '¹'),
    ('2', '²'),
    ('3', '³'),
    ('4', '⁴'),
    ('5', '⁵'),
    ('6', '⁶'),
    ('7', '⁷'),
    ('8', '⁸'),
    ('9', '⁹'),
    ('+', '⁺'),
    ('-', '⁻'),
    ('=', '⁼'),
    ('(', '⁽'),
    (')', '⁾'),
    ('n', 'ⁿ'),
    ('i', 'ⁱ'),
    ('a', 'ᵃ'),
    ('b', 'ᵇ'),
    ('c', 'ᶜ'),
    ('d', 'ᵈ'),
    ('e', 'ᵉ'),
    ('f', 'ᶠ'),
    ('g', 'ᵍ'),
    ('h', 'ʰ'),
    ('j', 'ʲ'),
    ('k', 'ᵏ'),
    ('l', 'ˡ'),
    ('m', 'ᵐ'),
    ('o', 'ᵒ'),
    ('p', 'ᵖ'),
    ('r', 'ʳ'),
    ('s', 'ˢ'),
    ('t', 'ᵗ'),
    ('u', 'ᵘ'),
    ('v', 'ᵛ'),
    ('w', 'ʷ'),
    ('x', 'ˣ'),
    ('y', 'ʸ'),
    ('z', 'ᶻ'),
    ('T', 'ᵀ'),
    ('*', '*'),
    (' ', ' '),
    ('′', '′'),
];

const SUB: &[(char, char)] = &[
    ('0', '₀'),
    ('1', '₁'),
    ('2', '₂'),
    ('3', '₃'),
    ('4', '₄'),
    ('5', '₅'),
    ('6', '₆'),
    ('7', '₇'),
    ('8', '₈'),
    ('9', '₉'),
    ('+', '₊'),
    ('-', '₋'),
    ('=', '₌'),
    ('(', '₍'),
    (')', '₎'),
    ('a', 'ₐ'),
    ('e', 'ₑ'),
    ('h', 'ₕ'),
    ('i', 'ᵢ'),
    ('j', 'ⱼ'),
    ('k', 'ₖ'),
    ('l', 'ₗ'),
    ('m', 'ₘ'),
    ('n', 'ₙ'),
    ('o', 'ₒ'),
    ('p', 'ₚ'),
    ('r', 'ᵣ'),
    ('s', 'ₛ'),
    ('t', 'ₜ'),
    ('u', 'ᵤ'),
    ('v', 'ᵥ'),
    ('x', 'ₓ'),
    (' ', ' '),
];

fn map_all(s: &str, table: &[(char, char)]) -> Option<String> {
    s.chars()
        .map(|c| table.iter().find(|(k, _)| *k == c).map(|(_, v)| *v))
        .collect()
}

pub fn try_superscript(s: &str) -> Option<String> {
    map_all(s, SUPER)
}

pub fn try_subscript(s: &str) -> Option<String> {
    map_all(s, SUB)
}

/// Converts text to superscript characters, or `^(text)` if impossible.
pub fn superscript(s: &str) -> String {
    map_all(s, SUPER).unwrap_or_else(|| {
        if s.chars().count() == 1 {
            format!("^{s}")
        } else {
            format!("^({s})")
        }
    })
}

pub fn subscript(s: &str) -> String {
    map_all(s, SUB).unwrap_or_else(|| {
        if s.chars().count() == 1 {
            format!("_{s}")
        } else {
            format!("_({s})")
        }
    })
}

pub fn to_unicode(src: &str) -> String {
    let chars: Vec<char> = src.chars().collect();
    let mut p = Parser { chars, pos: 0 };
    p.expr(None)
}

struct Parser {
    chars: Vec<char>,
    pos: usize,
}

impl Parser {
    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    /// Converts until `end` (consumed) or end of input.
    fn expr(&mut self, end: Option<char>) -> String {
        let mut out = String::new();
        while let Some(c) = self.peek() {
            self.pos += 1;
            match c {
                c if Some(c) == end => break,
                '\\' => out.push_str(&self.command()),
                '{' => out.push_str(&self.expr(Some('}'))),
                '^' => {
                    let arg = self.argument();
                    out.push_str(&superscript(&arg));
                }
                '_' => {
                    let arg = self.argument();
                    out.push_str(&subscript(&arg));
                }
                '~' => out.push(' '),
                c => out.push(c),
            }
        }
        out
    }

    /// A braced group or a single (possibly command) token.
    fn argument(&mut self) -> String {
        match self.peek() {
            Some('{') => {
                self.pos += 1;
                self.expr(Some('}'))
            }
            Some('\\') => {
                self.pos += 1;
                self.command()
            }
            Some(c) => {
                self.pos += 1;
                c.to_string()
            }
            None => String::new(),
        }
    }

    fn command(&mut self) -> String {
        let start = self.pos;
        while self.peek().is_some_and(|c| c.is_ascii_alphabetic()) {
            self.pos += 1;
        }
        if self.pos == start {
            // Single-symbol command like \, or \{
            let Some(c) = self.peek() else {
                return "\\".into();
            };
            self.pos += 1;
            let name = c.to_string();
            return lookup(&name).unwrap_or(&name).to_string();
        }
        let name: String = self.chars[start..self.pos].iter().collect();
        match name.as_str() {
            "frac" | "dfrac" | "tfrac" => {
                let a = self.argument();
                let b = self.argument();
                format!("{}/{}", wrap_group(&a), wrap_group(&b))
            }
            "sqrt" => {
                let arg = self.argument();
                format!("√{}", wrap_group(&arg))
            }
            "text" | "mathrm" | "mathbf" | "mathit" | "operatorname" | "textbf" | "mathsf"
            | "boldsymbol" => self.argument(),
            "mathbb" => {
                let arg = self.argument();
                lookup(&format!("mathbb{{{arg}}}")).map_or(arg, str::to_string)
            }
            "overline" | "bar" => {
                let arg = self.argument();
                arg.chars().flat_map(|c| [c, '\u{0305}']).collect()
            }
            "hat" => format!("{}\u{0302}", self.argument()),
            "vec" => format!("{}\u{20D7}", self.argument()),
            "dot" => format!("{}\u{0307}", self.argument()),
            "begin" | "end" => {
                self.argument();
                String::new()
            }
            _ => lookup(&name)
                .map(str::to_string)
                .unwrap_or_else(|| format!("\\{name}")),
        }
    }
}

fn lookup(name: &str) -> Option<&'static str> {
    SYMBOLS.iter().find(|(k, _)| *k == name).map(|(_, v)| *v)
}

fn wrap_group(s: &str) -> String {
    if s.chars().count() <= 1 || s.chars().all(|c| c.is_alphanumeric()) {
        s.to_string()
    } else {
        format!("({s})")
    }
}

#[cfg(test)]
mod tests {
    use super::to_unicode;

    #[test]
    fn converts_common_math() {
        assert_eq!(to_unicode("E = mc^2"), "E = mc²");
        assert_eq!(to_unicode(r"\sum_{i=1}^{n} x_i^2"), "∑ᵢ₌₁ⁿ xᵢ²");
        assert_eq!(to_unicode(r"\frac{a+b}{2}"), "(a+b)/2");
        assert_eq!(to_unicode(r"\alpha \leq \beta"), "α ≤ β");
        assert_eq!(to_unicode(r"\sqrt{x}"), "√x");
        assert_eq!(to_unicode(r"x^{\pi}"), "x^π");
        assert_eq!(to_unicode(r"\unknown"), r"\unknown");
    }
}
