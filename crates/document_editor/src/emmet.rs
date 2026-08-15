pub(crate) fn parse_emmet_abbreviation(abbreviation: &str, content: &str) -> String {
    let parts = abbreviation.split('>');
    let mut tags = Vec::new();
    let mut prefix = String::new();

    for part in parts {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }

        let mut tag = "div";
        let mut id = "";
        let mut classes = Vec::new();

        let mut current = part;
        if let Some(pos) = current.find(['.', '#']) {
            if pos > 0 {
                tag = &current[..pos];
            }
            current = &current[pos..];
        } else {
            tag = current;
            current = "";
        }

        while !current.is_empty() {
            let is_class = current.starts_with('.');
            let is_id = current.starts_with('#');
            current = &current[1..];
            let next_pos = current.find(['.', '#']).unwrap_or(current.len());
            let name = &current[..next_pos];

            if is_class && !name.is_empty() {
                classes.push(name);
            } else if is_id && !name.is_empty() {
                id = name;
            }
            current = &current[next_pos..];
        }

        prefix.push('<');
        prefix.push_str(tag);

        if !id.is_empty() {
            prefix.push_str(" id=\"");
            prefix.push_str(id);
            prefix.push('"');
        }

        if !classes.is_empty() {
            prefix.push_str(" class=\"");
            for (i, class) in classes.iter().enumerate() {
                if i > 0 {
                    prefix.push(' ');
                }
                prefix.push_str(class);
            }
            prefix.push('"');
        }

        prefix.push('>');
        tags.push(tag);
    }

    let mut result = prefix;
    result.push_str(content);
    for tag in tags.iter().rev() {
        result.push_str("</");
        result.push_str(tag);
        result.push('>');
    }

    result
}
