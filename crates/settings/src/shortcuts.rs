use gpui_kit::{Keystroke, SharedString};

#[derive(Clone)]
pub struct ShortcutReference {
    pub action: SharedString,
    pub context: SharedString,
    pub keystrokes: Vec<Keystroke>,
}

pub(crate) fn shortcut_context_name(context: &str) -> SharedString {
    match context {
        "AppShell" => "Application".into(),
        "CommandPalette" => "Command Palette".into(),
        "DocumentEditor" => "Document Editor".into(),
        "DocumentOutline" => "Document Outline".into(),
        "EmmetInput" => "Emmet Input".into(),
        "TextView" => "Text View".into(),
        _ => humanize_identifier(context),
    }
}

fn humanize_identifier(value: &str) -> SharedString {
    let mut label = String::with_capacity(value.len() + 4);
    let mut previous_is_lowercase = false;

    for character in value.chars() {
        if character == '_' || character == '-' {
            if !label.ends_with(' ') {
                label.push(' ');
            }
            previous_is_lowercase = false;
            continue;
        }

        if character.is_uppercase() && previous_is_lowercase {
            label.push(' ');
        }
        label.push(character);
        previous_is_lowercase = character.is_lowercase();
    }

    label.into()
}
