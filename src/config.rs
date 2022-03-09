use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use nanoserde::DeJson;
use std::collections::HashMap;
use std::vec::Vec;

#[derive(DeJson)]
pub struct JsonConfig {
    // When using an Option value, nanoserde won't require the field to be represented in json
    #[nserde(rename = "preview-converter")]
    #[nserde(default = "")]
    preview_converter: String,
    #[nserde(default = "Vec::new()")] // nanoserde requires the use of (), while serde does not
    keybindings: Vec<KeyBinding>,
}

#[derive(PartialEq, Debug, DeJson)]
pub struct KeyBinding {
    key: String,
    command: String,
}

#[derive(Debug)]
pub struct Config {
    pub preview_converter: String,
    pub keybindings: HashMap<KeyEvent, String>,
}

pub fn parse_config(config_data: &str) -> Config {
    let mut contents = String::new();

    // Remove single-line comments
    let mut prev_char = '\0';
    let mut skip_to_newline = false;
    for ch in config_data.chars() {
        if skip_to_newline {
            if ch == '\n' {
                skip_to_newline = false;
            } else {
                continue;
            }
        }

        if ch == '/' && prev_char == '/' {
            contents.pop(); // Remove the last char in contents
            skip_to_newline = true;
            continue;
        }

        contents.push(ch);

        prev_char = ch;
    }

    let mut json_config: JsonConfig = DeJson::deserialize_json(&contents).unwrap();

    json_config.keybindings.extend(default_key_bindings());

    Config {
        preview_converter: json_config.preview_converter,
        keybindings: make_binding_hash_map(&json_config.keybindings),
    }
}

fn default_key_bindings() -> Vec<KeyBinding> {
    let mut key_bindings = Vec::new();

    add_raw_binding(&mut key_bindings, "q", "quit");
    add_raw_binding(&mut key_bindings, "h", "updir");
    add_raw_binding(&mut key_bindings, "l", "open");
    add_raw_binding(&mut key_bindings, "j", "down");
    add_raw_binding(&mut key_bindings, "k", "up");
    add_raw_binding(&mut key_bindings, "e", "edit");
    add_raw_binding(&mut key_bindings, "g", "top");
    add_raw_binding(&mut key_bindings, "G", "bottom");
    add_raw_binding(&mut key_bindings, ":", "read");
    add_raw_binding(&mut key_bindings, "/", "search");
    add_raw_binding(&mut key_bindings, "?", "search-back");
    add_raw_binding(&mut key_bindings, "n", "search-next");
    add_raw_binding(&mut key_bindings, "N", "search-prev");
    add_raw_binding(&mut key_bindings, "space", "search-prev");

    key_bindings
}

fn make_binding_hash_map(raw_bindings: &[KeyBinding]) -> HashMap<KeyEvent, String> {
    let mut result = HashMap::new();

    for raw_binding in raw_bindings {
        let code = to_key(&raw_binding.key);
        result.insert(code, raw_binding.command.clone());
    }

    result
}

fn add_raw_binding(key_bindings: &mut Vec<KeyBinding>, key: &str, command: &str) {
    key_bindings.push(KeyBinding {
        key: key.to_string(),
        command: command.to_string(),
    });
}

fn to_key(key_s: &str) -> KeyEvent {
    let mut modifiers = KeyModifiers::NONE;
    let tokens: Vec<&str> = key_s.split('+').collect();
    for token in &tokens {
        match *token {
            "ctrl" => modifiers |= KeyModifiers::CONTROL,
            "alt" => modifiers |= KeyModifiers::ALT,
            "shift" => modifiers |= KeyModifiers::SHIFT,
            _ => (),
        }
    }

    let last_tok = *tokens.last().unwrap();

    let code = if last_tok.len() == 1 {
        let ch = last_tok.chars().next().unwrap();

        if ch.is_uppercase() {
            modifiers |= KeyModifiers::SHIFT;
        }

        KeyCode::Char(ch)
    } else {
        match last_tok {
            "enter" => KeyCode::Enter,
            "left" => KeyCode::Left,
            "right" => KeyCode::Right,
            "up" => KeyCode::Up,
            "down" => KeyCode::Down,
            "space" => KeyCode::Char(' '),
            _ => unreachable!("Command token not supported: {}", last_tok),
        }
    };

    KeyEvent { code, modifiers }
}

#[cfg(test)]
mod tests {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    use super::*;

    #[test]
    fn test_to_key() {
        assert_eq!(
            to_key("up"),
            KeyEvent {
                code: KeyCode::Up,
                modifiers: KeyModifiers::NONE
            }
        );
    }

    #[test]
    fn test_to_key_space() {
        assert_eq!(
            to_key("space"),
            KeyEvent {
                code: KeyCode::Char(' '),
                modifiers: KeyModifiers::NONE
            }
        );
    }

    #[test]
    fn test_to_key_one_mod() {
        assert_eq!(
            to_key("ctrl+f"),
            KeyEvent {
                code: KeyCode::Char('f'),
                modifiers: KeyModifiers::CONTROL,
            }
        );
    }

    #[test]
    fn test_parse_config_keybindings_no_mod() {
        let json = r#"
        {
          "keybindings": [
            { "key": "up", "command": "up" },
            { "key": "down", "command": "down" },
          ]
        }
        "#;

        let config = parse_config(json);

        assert_eq!(config.preview_converter, "");
        assert_eq!(config.keybindings[&to_key("up")], "up");
        assert_eq!(config.keybindings[&to_key("down")], "down");
        assert_eq!(config.keybindings[&to_key("h")], "updir");
    }

    #[test]
    fn test_parse_config_comment() {
        let json = r#"
        {
          // This is a comment.
          "keybindings": [
            { "key": "up", "command": "up" }, // This is on the first keybinding.
            { "key": "down", "command": "down" }
          ]
        }
        "#;

        let config = parse_config(json);

        assert_eq!(config.preview_converter, "");
        assert_eq!(config.keybindings[&to_key("up")], "up");
        assert_eq!(config.keybindings[&to_key("down")], "down");
        assert_eq!(config.keybindings[&to_key("h")], "updir");
    }

    #[test]
    fn test_make_binding_hash_map() {
        let mut raw_bindings = vec![];
        add_raw_binding(&mut raw_bindings, "h", "updir");

        let hash_map = make_binding_hash_map(&raw_bindings);

        assert_eq!(
            hash_map[&KeyEvent {
                code: KeyCode::Char('h'),
                modifiers: KeyModifiers::NONE
            }],
            "updir",
        );
    }
}
