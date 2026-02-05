use regex::Regex;
use tracing::info;

/// A list of common LaTeX commands that should never be patched.
/// These are core commands that, if "undefined", indicate a deeper problem.
const PROTECTED_COMMANDS: &[&str] = &[
    "begin", "end", "documentclass", "usepackage", "input", "include",
    "newcommand", "renewcommand", "providecommand", "def", "let",
    "section", "subsection", "subsubsection", "paragraph", "chapter",
    "textbf", "textit", "emph", "underline", "texttt", "textrm", "textsf",
    "item", "label", "ref", "cite", "bibliography", "bibliographystyle",
    "caption", "title", "author", "date", "maketitle",
    "hspace", "vspace", "hfill", "vfill", "newline", "linebreak", "pagebreak",
    "footnote", "marginpar", "centering", "raggedleft", "raggedright",
    "frac", "sqrt", "sum", "prod", "int", "lim", "sin", "cos", "tan", "log", "exp",
    "alpha", "beta", "gamma", "delta", "epsilon", "theta", "lambda", "mu", "pi", "sigma", "omega",
    "left", "right", "big", "Big", "bigg", "Bigg",
    "text", "mathrm", "mathbf", "mathit", "mathsf", "mathtt", "mathcal", "mathbb",
    "quad", "qquad", "ldots", "cdots", "dots", "infty", "partial", "nabla",
    "over", "atop", "choose", "brace", "brack",
    "if", "else", "fi", "ifx", "ifnum", "ifdim", "ifcase", "or",
    "relax", "expandafter", "noexpand", "csname", "endcsname",
    "the", "number", "romannumeral", "string", "meaning",
    "par", "indent", "noindent", "smallskip", "medskip", "bigskip",
    "tiny", "scriptsize", "footnotesize", "small", "normalsize", "large", "Large", "LARGE", "huge", "Huge",
];

pub struct SelfHealer;

impl SelfHealer {
    /// Attempts to heal common LaTeX errors based on compilation logs.
    /// Returns `Some(fixed_content)` if a fix was applied, `None` otherwise.
    pub fn attempt_heal(content: &str, logs: &str) -> Option<String> {
        let mut healed = content.to_string();
        let mut applied_fixes: Vec<&str> = Vec::new();

        // =========================================================================
        // FIX 1: Missing \end{document}
        // =========================================================================
        // Many "Emergency stop" or EOF errors are caused by a missing \end{document}.
        // This is a very safe fix.
        if !healed.contains("\\end{document}") && healed.contains("\\begin{document}") {
            info!("🩹 Self-Healing: Detected missing \\end{{document}}. Appending it.");
            healed.push_str("\n\\end{document}\n");
            applied_fixes.push("missing_end_document");
        }

        // =========================================================================
        // FIX 2: Undefined control sequence
        // =========================================================================
        // Strategy: Parse the error log to find the undefined command name.
        // Tectonic logs look like: "[Error] file.tex:4: Undefined control sequence"
        // We need to look at the SOURCE LINE to find the actual command.
        
        let re_undefined_tectonic = Regex::new(r"\[Error\] [^:]+:(\d+): Undefined control sequence").unwrap();
        
        if let Some(caps) = re_undefined_tectonic.captures(logs) {
            if let Ok(line_num) = caps[1].parse::<usize>() {
                // IMPORTANT: Use the ORIGINAL content for line lookup, since the log refers to the original file.
                if let Some(line_str) = content.lines().nth(line_num.saturating_sub(1)) {
                    info!("🩹 Self-Healing: Inspecting line {} for undefined commands: '{}'", line_num, line_str);
                    
                    // Find all LaTeX commands on this line
                    let re_cmd = Regex::new(r"\\([a-zA-Z@]+)").unwrap();
                    let mut cmds_to_patch: Vec<String> = Vec::new();
                    
                    for cap in re_cmd.captures_iter(line_str) {
                        let cmd = &cap[1];
                        // Only patch if NOT a protected command
                        if !PROTECTED_COMMANDS.contains(&cmd) {
                            cmds_to_patch.push(cmd.to_string());
                        }
                    }
                    
                    if !cmds_to_patch.is_empty() {
                        let mut patches = String::new();
                        for cmd_name in &cmds_to_patch {
                            info!("🩹 Self-Healing: Defining dummy for undefined cmd '\\{}'.", cmd_name);
                            // SAFE PATCH: Use simple text replacement, no font commands.
                            // The {} after takes any argument the original command might have expected (up to 1).
                            patches.push_str(&format!(
                                "\n\\providecommand{{\\{}}}[1][]{{[?{}]}}",
                                cmd_name, cmd_name
                            ));
                        }
                        
                        // Insert patches BEFORE \begin{document}
                        if let Some(pos) = healed.find("\\begin{document}") {
                            healed.insert_str(pos, &patches);
                        } else {
                            // Fallback: insert after \documentclass line
                            if let Some(pos) = healed.find('\n') {
                                healed.insert_str(pos, &patches);
                            } else {
                                healed = format!("{}{}", patches, healed);
                            }
                        }
                        applied_fixes.push("undefined_command");
                    }
                }
            }
        }

        // =========================================================================
        // FIX 3: Runaway argument (Unbalanced braces)
        // =========================================================================
        // Log patterns: "Runaway argument?" or "File ended while scanning use of..."
        if logs.contains("Runaway argument") || logs.contains("File ended while scanning") {
            info!("🩹 Self-Healing: Detected runaway argument (unbalanced brace?). Appending closing brace.");
            // Insert before \end{document} if it exists, otherwise at end
            if let Some(pos) = healed.rfind("\\end{document}") {
                healed.insert_str(pos, "\n}\n");
            } else {
                healed.push_str("\n}\n");
            }
            applied_fixes.push("unbalanced_brace");
        }

        // =========================================================================
        // Return result
        // =========================================================================
        if applied_fixes.is_empty() {
            None
        } else {
            info!("🩹 Self-Healing: Applied fixes: {:?}", applied_fixes);
            Some(healed)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_missing_end_document() {
        let content = r#"\documentclass{article}
\begin{document}
Hello World
"#;
        let logs = "[Error] test.tex:3: Emergency stop";
        let result = SelfHealer::attempt_heal(content, logs);
        assert!(result.is_some());
        assert!(result.unwrap().contains("\\end{document}"));
    }

    #[test]
    fn test_undefined_command() {
        let content = r#"\documentclass{article}
\begin{document}
\mybrokencommand
\end{document}
"#;
        let logs = "[Error] test.tex:3: Undefined control sequence";
        let result = SelfHealer::attempt_heal(content, logs);
        assert!(result.is_some());
        let healed = result.unwrap();
        assert!(healed.contains("\\providecommand{\\mybrokencommand}"));
    }

    #[test]
    fn test_protected_command_not_patched() {
        let content = r#"\documentclass{article}
\begin{document}
\textbf{test}
\end{document}
"#;
        // If textbf were somehow undefined, we should NOT patch it
        let logs = "[Error] test.tex:3: Undefined control sequence";
        let result = SelfHealer::attempt_heal(content, logs);
        // Should return None because textbf is protected and document is complete
        assert!(result.is_none() || !result.clone().unwrap().contains("\\providecommand{\\textbf}"));
    }
}
