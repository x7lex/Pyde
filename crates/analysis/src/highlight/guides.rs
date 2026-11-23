//! Indent guides: a thin vertical line per indent level, carried through the
//! empty lines inside a block.

/// One level of indentation. The editor always indents with this.
pub const INDENT: &str = "    ";

/// Leading whitespace as one guide span per indent level, which draws a thin
/// vertical line at each level without changing the layout.
pub(super) fn push_guides(html: &mut String, lead: &str) {
    const GUIDE: &str = "<span class=\"indent-guide\">";
    let mut spaces = 0;
    for c in lead.chars() {
        if c == '\t' {
            html.push_str(&" ".repeat(spaces));
            spaces = 0;
            html.push_str(GUIDE);
            html.push('\t');
            html.push_str("</span>");
            continue;
        }
        spaces += 1;
        if spaces == INDENT.len() {
            html.push_str(GUIDE);
            html.push_str(INDENT);
            html.push_str("</span>");
            spaces = 0;
        }
    }
    html.push_str(&" ".repeat(spaces));
}

/// How many indent levels the guides carry through each empty line. Only empty
/// lines inside a block get any: the smaller of the levels of the nearest code
/// lines above and below. Without it the guides break at every gap.
pub(super) fn blank_line_levels(code: &str) -> Vec<usize> {
    let lines: Vec<&str> = code.split('\n').collect();
    let level = |line: &str| {
        let lead = &line[..line.len() - line.trim_start_matches([' ', '\t']).len()];
        let mut levels = 0;
        let mut spaces = 0;
        for c in lead.chars() {
            if c == '\t' {
                levels += 1;
                spaces = 0;
            } else {
                spaces += 1;
                if spaces == INDENT.len() {
                    levels += 1;
                    spaces = 0;
                }
            }
        }
        levels
    };
    let code_level: Vec<Option<usize>> = lines
        .iter()
        .map(|l| (!l.trim().is_empty()).then(|| level(l)))
        .collect();

    let mut above = vec![0; lines.len()];
    let mut last = 0;
    for (i, l) in code_level.iter().enumerate() {
        above[i] = last;
        last = l.unwrap_or(last);
    }
    let mut result = vec![0; lines.len()];
    let mut below = 0;
    for i in (0..lines.len()).rev() {
        if lines[i].is_empty() {
            result[i] = above[i].min(below);
        }
        below = code_level[i].unwrap_or(below);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::super::html::to_html;

    #[test]
    fn indentation_gets_one_guide_per_level() {
        let html = to_html("if x:\n        y\n", &[], &[]);
        let guide = "<span class=\"indent-guide\">    </span>";
        assert!(html.contains(&format!("\n{guide}{guide}<span")), "{html}");
        assert_eq!(html.matches("indent-guide").count(), 2, "{html}");
    }

    #[test]
    fn partial_levels_and_tabs_and_midline_spaces() {
        let html = to_html("if x:\n      y = 1\n\tz\n", &[], &[]);
        let guide = "<span class=\"indent-guide\">    </span>";
        assert!(html.contains(&format!("{guide}  <span")), "{html}");
        assert!(
            html.contains("<span class=\"indent-guide\">\t</span>"),
            "{html}"
        );
        // The spaces around `=` are not indentation.
        assert_eq!(html.matches("indent-guide").count(), 2, "{html}");
    }

    #[test]
    fn blank_lines_inside_a_block_carry_the_guides() {
        let html = to_html("def f():\n    if x:\n        a\n\n        b\n", &[], &[]);
        assert_eq!(html.matches("class=\"indent-bridge\"").count(), 2, "{html}");
        let html = to_html("a\n\nb\n", &[], &[]);
        assert!(!html.contains("indent-bridge"), "{html}");
        let html = to_html("def f():\n    a\n\nb\n", &[], &[]);
        assert!(!html.contains("indent-bridge"), "{html}");
    }

    #[test]
    fn guides_do_not_add_or_remove_text() {
        let code = "def f():\n    if x:\n        return 1\n\n    pass\n";
        let html = to_html(code, &[], &[]);
        let mut text = String::new();
        let mut in_tag = false;
        for c in html.chars() {
            match c {
                '<' => in_tag = true,
                '>' => in_tag = false,
                c if !in_tag => text.push(c),
                _ => {}
            }
        }
        assert_eq!(text, code);
    }
}
