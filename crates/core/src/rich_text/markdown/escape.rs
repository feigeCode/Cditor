pub fn escape_inline_text(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for ch in text.chars() {
        if matches!(
            ch,
            '\\' | '*' | '_' | '~' | '`' | '[' | ']' | '<' | '>' | '(' | ')'
        ) {
            escaped.push('\\');
        }
        escaped.push(ch);
    }
    escaped
}

pub fn escape_link_label(text: &str) -> String {
    escape_inline_text(text)
}

pub fn escape_link_destination(destination: &str) -> String {
    let mut escaped = String::with_capacity(destination.len());
    for ch in destination.chars() {
        match ch {
            '\\' | '<' | '>' => {
                escaped.push('\\');
                escaped.push(ch);
            }
            '\n' | '\r' => escaped.push_str("%0A"),
            _ => escaped.push(ch),
        }
    }
    escaped
}

pub fn escape_table_cell(markdown: &str) -> String {
    markdown.replace('|', "\\|")
}

pub fn escape_block_start(markdown: &str) -> String {
    let heading_marks = markdown.bytes().take_while(|byte| *byte == b'#').count();
    if ((1..=6).contains(&heading_marks) && markdown[heading_marks..].starts_with(' '))
        || markdown.starts_with("> ")
        || markdown.starts_with("- ")
        || markdown.starts_with("+ ")
        || markdown == "---"
    {
        return format!("\\{markdown}");
    }
    let digits = markdown.bytes().take_while(u8::is_ascii_digit).count();
    if digits > 0 && markdown[digits..].starts_with(". ") {
        let mut escaped = markdown.to_owned();
        escaped.insert(digits, '\\');
        return escaped;
    }
    markdown.to_owned()
}

pub fn choose_code_span_delimiter(text: &str) -> String {
    "`".repeat(longest_run(text, '`').saturating_add(1).max(1))
}

pub fn choose_code_fence(text: &str) -> String {
    "`".repeat(longest_run(text, '`').saturating_add(1).max(3))
}

fn longest_run(text: &str, needle: char) -> usize {
    let mut longest = 0;
    let mut current = 0;
    for ch in text.chars() {
        if ch == needle {
            current += 1;
            longest = longest.max(current);
        } else {
            current = 0;
        }
    }
    longest
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn code_delimiters_are_longer_than_embedded_runs() {
        assert_eq!(choose_code_span_delimiter("plain"), "`");
        assert_eq!(choose_code_span_delimiter("a ` b"), "``");
        assert_eq!(choose_code_fence("```inside```"), "````");
    }

    #[test]
    fn block_sensitive_prefixes_are_escaped() {
        assert_eq!(escape_block_start("# literal"), "\\# literal");
        assert_eq!(escape_block_start("12. literal"), "12\\. literal");
        assert_eq!(escape_block_start("ordinary"), "ordinary");
    }
}
