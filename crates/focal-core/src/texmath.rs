//! Inline math as Unicode text: `$\alpha^2$` reads as `α²` while the caret is
//! elsewhere. GPUI text runs cannot hold typeset images, so inline math is
//! approximated with Unicode letters, operators and super- and subscripts.

/// `tex` as Unicode text, or `None` when it uses something Unicode cannot
/// show (an unknown command, an environment, a malformed group).
pub fn tex_to_unicode(tex: &str) -> Option<String> {
    let mut parser = Parser {
        chars: tex.chars().collect(),
        at: 0,
    };
    let text = parser.sequence(false)?;
    // TeX ignores spaces in math; keep the writer's single spaces only.
    let collapsed = text.split_whitespace().collect::<Vec<_>>().join(" ");
    Some(collapsed)
}

struct Parser {
    chars: Vec<char>,
    at: usize,
}

impl Parser {
    fn peek(&self) -> Option<char> {
        self.chars.get(self.at).copied()
    }

    fn next(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.at += 1;
        Some(c)
    }

    /// Text up to the end, or up to the closing brace of a group.
    fn sequence(&mut self, in_group: bool) -> Option<String> {
        let mut out = String::new();
        loop {
            match self.peek() {
                None if in_group => return None,
                None => return Some(out),
                Some('}') if in_group => {
                    self.at += 1;
                    return Some(out);
                }
                Some('}') => return None,
                Some(' ') => {
                    self.at += 1;
                    out.push(' ');
                }
                Some('^' | '_') => out.push_str(&self.script()?),
                _ => out.push_str(&self.atom()?),
            }
        }
    }

    /// One argument: a group, a command or a character.
    fn atom(&mut self) -> Option<String> {
        while self.peek() == Some(' ') {
            self.at += 1;
        }
        match self.next()? {
            '{' => self.sequence(true),
            '\\' => self.command(),
            c => Some(c.to_string()),
        }
    }

    fn script(&mut self) -> Option<String> {
        let superscript = self.next()? == '^';
        let argument = self.atom()?;
        let mapped: Option<String> = argument
            .chars()
            .map(|c| {
                if superscript {
                    superscript_of(c)
                } else {
                    subscript_of(c)
                }
            })
            .collect();
        let mark = if superscript { '^' } else { '_' };
        Some(match mapped {
            Some(mapped) if !argument.is_empty() => mapped,
            _ if argument.chars().count() == 1 => format!("{mark}{argument}"),
            _ => format!("{mark}({argument})"),
        })
    }

    fn command(&mut self) -> Option<String> {
        let first = self.next()?;
        if !first.is_ascii_alphabetic() {
            return match first {
                ',' | ':' | ';' | ' ' => Some(" ".into()),
                '!' => Some(String::new()),
                '{' | '}' | '$' | '%' | '&' | '#' | '_' | '|' => Some(first.to_string()),
                _ => None,
            };
        }
        let mut name = String::from(first);
        while let Some(c) = self.peek().filter(char::is_ascii_alphabetic) {
            name.push(c);
            self.at += 1;
        }
        match name.as_str() {
            "frac" | "dfrac" | "tfrac" => {
                let (top, bottom) = (self.atom()?, self.atom()?);
                Some(format!("{}/{}", parenthesize(&top), parenthesize(&bottom)))
            }
            "sqrt" => {
                let root = if self.peek() == Some('[') {
                    self.at += 1;
                    let index: String = std::iter::from_fn(|| self.next())
                        .take_while(|&c| c != ']')
                        .collect();
                    match index.trim() {
                        "3" => '∛',
                        "4" => '∜',
                        _ => return None,
                    }
                } else {
                    '√'
                };
                let argument = self.atom()?;
                Some(if argument.chars().count() == 1 {
                    format!("{root}{argument}")
                } else {
                    format!("{root}({argument})")
                })
            }
            "mathbb" => self.atom()?.chars().map(double_struck).collect(),
            "text" | "textrm" | "textit" | "textbf" | "mathrm" | "mathit" | "mathbf" | "mathsf"
            | "mathtt" | "operatorname" | "mathcal" => self.atom(),
            "left" | "right" | "big" | "Big" | "bigl" | "bigr" | "Bigl" | "Bigr"
            | "displaystyle" => Some(String::new()),
            name => symbol(name).map(String::from),
        }
    }
}

/// Wraps a fraction's part in parentheses unless it is one simple term.
fn parenthesize(part: &str) -> String {
    let simple = part.chars().all(|c| c.is_alphanumeric() || c == '.');
    if simple {
        part.to_owned()
    } else {
        format!("({part})")
    }
}

fn greek(name: &str) -> Option<&'static str> {
    Some(match name {
        "alpha" => "α",
        "beta" => "β",
        "gamma" => "γ",
        "delta" => "δ",
        "epsilon" => "ϵ",
        "varepsilon" => "ε",
        "zeta" => "ζ",
        "eta" => "η",
        "theta" => "θ",
        "vartheta" => "ϑ",
        "iota" => "ι",
        "kappa" => "κ",
        "lambda" => "λ",
        "mu" => "μ",
        "nu" => "ν",
        "xi" => "ξ",
        "pi" => "π",
        "varpi" => "ϖ",
        "rho" => "ρ",
        "varrho" => "ϱ",
        "sigma" => "σ",
        "varsigma" => "ς",
        "tau" => "τ",
        "upsilon" => "υ",
        "phi" => "ϕ",
        "varphi" => "φ",
        "chi" => "χ",
        "psi" => "ψ",
        "omega" => "ω",
        "Gamma" => "Γ",
        "Delta" => "Δ",
        "Theta" => "Θ",
        "Lambda" => "Λ",
        "Xi" => "Ξ",
        "Pi" => "Π",
        "Sigma" => "Σ",
        "Upsilon" => "Υ",
        "Phi" => "Φ",
        "Psi" => "Ψ",
        "Omega" => "Ω",
        _ => return None,
    })
}

fn symbol(name: &str) -> Option<&'static str> {
    Some(match name {
        "cdot" => "·",
        "times" => "×",
        "div" => "÷",
        "pm" => "±",
        "mp" => "∓",
        "ast" => "∗",
        "le" | "leq" => "≤",
        "ge" | "geq" => "≥",
        "ne" | "neq" => "≠",
        "approx" => "≈",
        "equiv" => "≡",
        "sim" => "∼",
        "simeq" => "≃",
        "cong" => "≅",
        "propto" => "∝",
        "ll" => "≪",
        "gg" => "≫",
        "infty" => "∞",
        "partial" => "∂",
        "nabla" => "∇",
        "sum" => "∑",
        "prod" => "∏",
        "int" => "∫",
        "iint" => "∬",
        "oint" => "∮",
        "in" => "∈",
        "notin" => "∉",
        "ni" => "∋",
        "subset" => "⊂",
        "supset" => "⊃",
        "subseteq" => "⊆",
        "supseteq" => "⊇",
        "cup" => "∪",
        "cap" => "∩",
        "setminus" => "∖",
        "emptyset" | "varnothing" => "∅",
        "forall" => "∀",
        "exists" => "∃",
        "neg" | "lnot" => "¬",
        "land" | "wedge" => "∧",
        "lor" | "vee" => "∨",
        "oplus" => "⊕",
        "otimes" => "⊗",
        "to" | "rightarrow" => "→",
        "leftarrow" | "gets" => "←",
        "leftrightarrow" => "↔",
        "Rightarrow" | "implies" => "⇒",
        "Leftarrow" => "⇐",
        "Leftrightarrow" | "iff" => "⇔",
        "mapsto" => "↦",
        "uparrow" => "↑",
        "downarrow" => "↓",
        "ldots" | "dots" => "…",
        "cdots" => "⋯",
        "vdots" => "⋮",
        "ddots" => "⋱",
        "circ" => "∘",
        "bullet" => "•",
        "star" => "⋆",
        "prime" => "′",
        "angle" => "∠",
        "perp" => "⊥",
        "parallel" => "∥",
        "mid" => "∣",
        "hbar" => "ℏ",
        "ell" => "ℓ",
        "Re" => "ℜ",
        "Im" => "ℑ",
        "aleph" => "ℵ",
        "degree" => "°",
        "langle" => "⟨",
        "rangle" => "⟩",
        "lfloor" => "⌊",
        "rfloor" => "⌋",
        "lceil" => "⌈",
        "rceil" => "⌉",
        "lvert" | "rvert" | "vert" => "|",
        "lVert" | "rVert" | "Vert" => "‖",
        "quad" | "qquad" => " ",
        "sin" => "sin",
        "cos" => "cos",
        "tan" => "tan",
        "log" => "log",
        "ln" => "ln",
        "exp" => "exp",
        "lim" => "lim",
        "max" => "max",
        "min" => "min",
        "det" => "det",
        "dim" => "dim",
        "mod" | "bmod" => "mod",
        _ => return greek(name),
    })
}

fn double_struck(c: char) -> Option<char> {
    Some(match c {
        'C' => 'ℂ',
        'H' => 'ℍ',
        'N' => 'ℕ',
        'P' => 'ℙ',
        'Q' => 'ℚ',
        'R' => 'ℝ',
        'Z' => 'ℤ',
        _ => return None,
    })
}

pub(crate) fn superscript_of(c: char) -> Option<char> {
    Some(match c {
        '0' => '⁰',
        '1' => '¹',
        '2' => '²',
        '3' => '³',
        '4' => '⁴',
        '5' => '⁵',
        '6' => '⁶',
        '7' => '⁷',
        '8' => '⁸',
        '9' => '⁹',
        '+' => '⁺',
        '-' => '⁻',
        '=' => '⁼',
        '(' => '⁽',
        ')' => '⁾',
        'a' => 'ᵃ',
        'b' => 'ᵇ',
        'c' => 'ᶜ',
        'd' => 'ᵈ',
        'e' => 'ᵉ',
        'f' => 'ᶠ',
        'g' => 'ᵍ',
        'h' => 'ʰ',
        'i' => 'ⁱ',
        'j' => 'ʲ',
        'k' => 'ᵏ',
        'l' => 'ˡ',
        'm' => 'ᵐ',
        'n' => 'ⁿ',
        'o' => 'ᵒ',
        'p' => 'ᵖ',
        'r' => 'ʳ',
        's' => 'ˢ',
        't' => 'ᵗ',
        'u' => 'ᵘ',
        'v' => 'ᵛ',
        'w' => 'ʷ',
        'x' => 'ˣ',
        'y' => 'ʸ',
        'z' => 'ᶻ',
        'A' => 'ᴬ',
        'B' => 'ᴮ',
        'D' => 'ᴰ',
        'E' => 'ᴱ',
        'G' => 'ᴳ',
        'H' => 'ᴴ',
        'I' => 'ᴵ',
        'J' => 'ᴶ',
        'K' => 'ᴷ',
        'L' => 'ᴸ',
        'M' => 'ᴹ',
        'N' => 'ᴺ',
        'O' => 'ᴼ',
        'P' => 'ᴾ',
        'R' => 'ᴿ',
        'T' => 'ᵀ',
        'U' => 'ᵁ',
        'W' => 'ᵂ',
        'α' => 'ᵅ',
        'β' => 'ᵝ',
        'γ' => 'ᵞ',
        'δ' => 'ᵟ',
        'φ' => 'ᵠ',
        'χ' => 'ᵡ',
        'θ' => 'ᶿ',
        '′' => '′',
        _ => return None,
    })
}

pub(crate) fn subscript_of(c: char) -> Option<char> {
    Some(match c {
        '0' => '₀',
        '1' => '₁',
        '2' => '₂',
        '3' => '₃',
        '4' => '₄',
        '5' => '₅',
        '6' => '₆',
        '7' => '₇',
        '8' => '₈',
        '9' => '₉',
        '+' => '₊',
        '-' => '₋',
        '=' => '₌',
        '(' => '₍',
        ')' => '₎',
        'a' => 'ₐ',
        'e' => 'ₑ',
        'h' => 'ₕ',
        'i' => 'ᵢ',
        'j' => 'ⱼ',
        'k' => 'ₖ',
        'l' => 'ₗ',
        'm' => 'ₘ',
        'n' => 'ₙ',
        'o' => 'ₒ',
        'p' => 'ₚ',
        'r' => 'ᵣ',
        's' => 'ₛ',
        't' => 'ₜ',
        'u' => 'ᵤ',
        'v' => 'ᵥ',
        'x' => 'ₓ',
        'β' => 'ᵦ',
        'γ' => 'ᵧ',
        'ρ' => 'ᵨ',
        'φ' => 'ᵩ',
        'χ' => 'ᵪ',
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn u(tex: &str) -> Option<String> {
        tex_to_unicode(tex)
    }

    #[test]
    fn letters_and_operators() {
        assert_eq!(
            u(r"\alpha + \beta \le \infty").as_deref(),
            Some("α + β ≤ ∞")
        );
        assert_eq!(
            u(r"a \cdot b \neq c \to d").as_deref(),
            Some("a · b ≠ c → d")
        );
        assert_eq!(u(r"\mathbb{R}^n").as_deref(), Some("ℝⁿ"));
        assert_eq!(u(r"\text{if } x").as_deref(), Some("if x"));
    }

    #[test]
    fn scripts_use_unicode_when_they_can() {
        assert_eq!(u("x^2 + y_1").as_deref(), Some("x² + y₁"));
        assert_eq!(u("x^{10}").as_deref(), Some("x¹⁰"));
        assert_eq!(u(r"e^{i\pi} + 1 = 0").as_deref(), Some("e^(iπ) + 1 = 0"));
    }

    #[test]
    fn fractions_and_roots() {
        assert_eq!(u(r"\frac{a}{b}").as_deref(), Some("a/b"));
        assert_eq!(u(r"\frac{a+1}{2}").as_deref(), Some("(a+1)/2"));
        assert_eq!(u(r"\sqrt{2}").as_deref(), Some("√2"));
        assert_eq!(u(r"\sqrt{x+1}").as_deref(), Some("√(x+1)"));
    }

    #[test]
    fn unknown_commands_keep_the_source() {
        assert_eq!(u(r"\begin{pmatrix}"), None);
        assert_eq!(u(r"\frac{a}"), None, "incomplete");
        assert_eq!(u("x = 1").as_deref(), Some("x = 1"));
    }
}
