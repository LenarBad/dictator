use tauri_plugin_global_shortcut::{Code, Modifiers, Shortcut};

pub fn parse_hotkey(combo: &str) -> Result<Shortcut, String> {
    let raw = combo.trim().to_lowercase().replace('-', "+");
    if raw.is_empty() {
        return Err("hotkey is empty".into());
    }
    let mut modifiers = Modifiers::empty();
    let mut key: Option<Code> = None;
    for part in raw
        .split('+')
        .map(str::trim)
        .filter(|part| !part.is_empty())
    {
        let token = part.trim_matches(|c| c == '<' || c == '>');
        match token {
            "ctrl" | "control" => modifiers |= Modifiers::CONTROL,
            "shift" => modifiers |= Modifiers::SHIFT,
            "alt" | "option" | "opt" => modifiers |= Modifiers::ALT,
            "cmd" | "command" | "super" | "meta" | "win" | "windows" => {
                modifiers |= Modifiers::SUPER;
            }
            "space" => key = Some(Code::Space),
            other => {
                if let Some(code) = key_code(other) {
                    key = Some(code);
                } else {
                    return Err(format!("unsupported hotkey key: {other}"));
                }
            }
        }
    }
    let Some(code) = key else {
        return Err(format!("hotkey has no key: {combo}"));
    };
    Ok(Shortcut::new(Some(modifiers), code))
}

fn key_code(token: &str) -> Option<Code> {
    if let Some(rest) = token.strip_prefix('f') {
        if let Ok(index) = rest.parse::<u8>() {
            return function_key(index);
        }
    }
    if token.len() == 1 {
        let ch = token.chars().next()?;
        return match ch {
            'a'..='z' => letter_code(ch),
            '0'..='9' => digit_code(ch),
            _ => None,
        };
    }
    None
}

fn letter_code(ch: char) -> Option<Code> {
    Some(match ch {
        'a' => Code::KeyA,
        'b' => Code::KeyB,
        'c' => Code::KeyC,
        'd' => Code::KeyD,
        'e' => Code::KeyE,
        'f' => Code::KeyF,
        'g' => Code::KeyG,
        'h' => Code::KeyH,
        'i' => Code::KeyI,
        'j' => Code::KeyJ,
        'k' => Code::KeyK,
        'l' => Code::KeyL,
        'm' => Code::KeyM,
        'n' => Code::KeyN,
        'o' => Code::KeyO,
        'p' => Code::KeyP,
        'q' => Code::KeyQ,
        'r' => Code::KeyR,
        's' => Code::KeyS,
        't' => Code::KeyT,
        'u' => Code::KeyU,
        'v' => Code::KeyV,
        'w' => Code::KeyW,
        'x' => Code::KeyX,
        'y' => Code::KeyY,
        'z' => Code::KeyZ,
        _ => return None,
    })
}

fn digit_code(ch: char) -> Option<Code> {
    Some(match ch {
        '0' => Code::Digit0,
        '1' => Code::Digit1,
        '2' => Code::Digit2,
        '3' => Code::Digit3,
        '4' => Code::Digit4,
        '5' => Code::Digit5,
        '6' => Code::Digit6,
        '7' => Code::Digit7,
        '8' => Code::Digit8,
        '9' => Code::Digit9,
        _ => return None,
    })
}

fn function_key(index: u8) -> Option<Code> {
    Some(match index {
        1 => Code::F1,
        2 => Code::F2,
        3 => Code::F3,
        4 => Code::F4,
        5 => Code::F5,
        6 => Code::F6,
        7 => Code::F7,
        8 => Code::F8,
        9 => Code::F9,
        10 => Code::F10,
        11 => Code::F11,
        12 => Code::F12,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_ctrl_shift_d() {
        let shortcut = parse_hotkey("ctrl+shift+d").expect("parse");
        assert_eq!(
            shortcut,
            Shortcut::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::KeyD)
        );
    }

    #[test]
    fn parse_cmd_shift_letter() {
        let shortcut = parse_hotkey("cmd+shift+s").expect("parse");
        assert_eq!(
            shortcut,
            Shortcut::new(Some(Modifiers::SUPER | Modifiers::SHIFT), Code::KeyS)
        );
    }

    #[test]
    fn parse_function_and_digit_keys() {
        assert_eq!(
            parse_hotkey("ctrl+f12").expect("parse"),
            Shortcut::new(Some(Modifiers::CONTROL), Code::F12)
        );
        assert_eq!(
            parse_hotkey("alt+shift+1").expect("parse"),
            Shortcut::new(Some(Modifiers::ALT | Modifiers::SHIFT), Code::Digit1)
        );
    }
}
