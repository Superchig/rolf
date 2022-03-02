use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use nanoserde::DeJson;

#[derive(DeJson)]
pub struct Config {
    // When using an Option value, nanoserde won't require the field to be represented in json
    #[nserde(rename = "preview-converter")]
    preview_converter: Option<String>,
    keybindings: Option<Vec<KeyBinding>>,
}

#[derive(PartialEq, Debug, DeJson)]
pub struct KeyBinding {
    key: String,
    command: String,
}

pub fn parse_config(config_data: &str) -> Config {
    let mut contents = String::new();

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

    DeJson::deserialize_json(&contents).unwrap()
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
        let byte = ch as u8;

        if (b'a'..=b'z').contains(&byte) {
            KeyCode::Char(ch)
        } else {
            unreachable!();
        }
    } else {
        match last_tok {
            "up" => KeyCode::Up,
            _ => unreachable!(),
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

        assert_eq!(config.preview_converter, None);
        assert_eq!(
            config.keybindings,
            Some(vec![
                KeyBinding {
                    key: "up".to_string(),
                    command: "up".to_string(),
                },
                KeyBinding {
                    key: "down".to_string(),
                    command: "down".to_string(),
                }
            ])
        );
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

        assert_eq!(config.preview_converter, None);
        assert_eq!(
            config.keybindings,
            Some(vec![
                KeyBinding {
                    key: "up".to_string(),
                    command: "up".to_string(),
                },
                KeyBinding {
                    key: "down".to_string(),
                    command: "down".to_string(),
                }
            ])
        );
    }
}
