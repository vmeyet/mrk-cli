use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    LineDown,
    LineUp,
    PageDown,
    PageUp,
    HalfDown,
    HalfUp,
    Top,
    Bottom,
    StartSearch,
    NextMatch,
    PreviousMatch,
    Quit,
    Type(char),
    Erase,
    Confirm,
    Cancel,
}

fn control(code: KeyCode) -> Option<Action> {
    match code {
        KeyCode::Char('c') => Some(Action::Quit),
        KeyCode::Char('f') => Some(Action::PageDown),
        KeyCode::Char('b') => Some(Action::PageUp),
        KeyCode::Char('d') => Some(Action::HalfDown),
        KeyCode::Char('u') => Some(Action::HalfUp),
        _ => None,
    }
}

fn reading(code: KeyCode) -> Option<Action> {
    match code {
        KeyCode::Char('j') | KeyCode::Down | KeyCode::Enter => Some(Action::LineDown),
        KeyCode::Char('k') | KeyCode::Up => Some(Action::LineUp),
        KeyCode::Char(' ' | 'f') | KeyCode::PageDown => Some(Action::PageDown),
        KeyCode::Char('b') | KeyCode::PageUp => Some(Action::PageUp),
        KeyCode::Char('d') => Some(Action::HalfDown),
        KeyCode::Char('u') => Some(Action::HalfUp),
        KeyCode::Char('g') | KeyCode::Home => Some(Action::Top),
        KeyCode::Char('G') | KeyCode::End => Some(Action::Bottom),
        KeyCode::Char('/') => Some(Action::StartSearch),
        KeyCode::Char('n') => Some(Action::NextMatch),
        KeyCode::Char('N') => Some(Action::PreviousMatch),
        KeyCode::Char('q') | KeyCode::Esc => Some(Action::Quit),
        _ => None,
    }
}

fn prompting(code: KeyCode) -> Option<Action> {
    match code {
        KeyCode::Enter => Some(Action::Confirm),
        KeyCode::Esc => Some(Action::Cancel),
        KeyCode::Backspace => Some(Action::Erase),
        KeyCode::Char(character) => Some(Action::Type(character)),
        _ => None,
    }
}

/// What a key press does, while reading or while typing a search query.
pub fn action(key: KeyEvent, is_prompting: bool) -> Option<Action> {
    let is_control = key.modifiers.contains(KeyModifiers::CONTROL);
    let is_quit = is_control && key.code == KeyCode::Char('c');
    match () {
        () if is_quit => Some(Action::Quit),
        () if is_control && is_prompting => None,
        () if is_control => control(key.code),
        () if is_prompting => prompting(key.code),
        () => reading(key.code),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn chars(keys: &str) -> Vec<KeyEvent> {
        keys.chars().map(|character| press(KeyCode::Char(character))).collect()
    }

    fn ctrl(character: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(character), KeyModifiers::CONTROL)
    }

    fn reads(keys: &[KeyEvent], expected: &Action) {
        for key in keys {
            assert_eq!(action(*key, false).as_ref(), Some(expected), "{key:?}");
        }
    }

    #[test]
    fn every_reading_key_maps_to_its_action() {
        reads(&[chars("j"), vec![press(KeyCode::Down), press(KeyCode::Enter)]].concat(), &Action::LineDown);
        reads(&[chars("k"), vec![press(KeyCode::Up)]].concat(), &Action::LineUp);
        reads(&[chars(" f"), vec![press(KeyCode::PageDown), ctrl('f')]].concat(), &Action::PageDown);
        reads(&[chars("b"), vec![press(KeyCode::PageUp), ctrl('b')]].concat(), &Action::PageUp);
        reads(&[chars("d"), vec![ctrl('d')]].concat(), &Action::HalfDown);
        reads(&[chars("u"), vec![ctrl('u')]].concat(), &Action::HalfUp);
        reads(&[chars("g"), vec![press(KeyCode::Home)]].concat(), &Action::Top);
        reads(&[chars("G"), vec![press(KeyCode::End)]].concat(), &Action::Bottom);
        reads(&chars("/"), &Action::StartSearch);
        reads(&chars("n"), &Action::NextMatch);
        reads(&chars("N"), &Action::PreviousMatch);
        reads(&[chars("q"), vec![press(KeyCode::Esc), ctrl('c')]].concat(), &Action::Quit);
    }

    #[test]
    fn other_keys_do_nothing() {
        assert_eq!(action(press(KeyCode::Char('x')), false), None);
        assert_eq!(action(ctrl('x'), false), None);
        assert_eq!(action(press(KeyCode::Tab), false), None);
    }

    #[test]
    fn the_prompt_takes_every_character_as_text() {
        for character in "jkq/nG ".chars() {
            assert_eq!(action(press(KeyCode::Char(character)), true), Some(Action::Type(character)));
        }
        assert_eq!(action(press(KeyCode::Enter), true), Some(Action::Confirm));
        assert_eq!(action(press(KeyCode::Esc), true), Some(Action::Cancel));
        assert_eq!(action(press(KeyCode::Backspace), true), Some(Action::Erase));
        assert_eq!(action(ctrl('f'), true), None);
        assert_eq!(action(ctrl('c'), true), Some(Action::Quit));
    }
}
