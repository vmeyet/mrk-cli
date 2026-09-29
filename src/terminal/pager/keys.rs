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

/// The keys that trigger an action, `Ctrl` with `control` as one more, and what the help says the action does.
struct Binding {
    action: Action,
    keys: &'static [KeyCode],
    control: Option<char>,
    meaning: &'static str,
}

const fn bind(action: Action, keys: &'static [KeyCode], control: Option<char>, meaning: &'static str) -> Binding {
    Binding { action, keys, control, meaning }
}

const READING: &[Binding] = &[
    bind(Action::LineDown, &[KeyCode::Char('j'), KeyCode::Down, KeyCode::Enter], None, "down a row"),
    bind(Action::LineUp, &[KeyCode::Char('k'), KeyCode::Up], None, "up a row"),
    bind(Action::PageDown, &[KeyCode::Char(' '), KeyCode::Char('f'), KeyCode::PageDown], Some('f'), "down a page"),
    bind(Action::PageUp, &[KeyCode::Char('b'), KeyCode::PageUp], Some('b'), "up a page"),
    bind(Action::HalfDown, &[KeyCode::Char('d')], Some('d'), "down half a page"),
    bind(Action::HalfUp, &[KeyCode::Char('u')], Some('u'), "up half a page"),
    bind(Action::Top, &[KeyCode::Char('g'), KeyCode::Home], None, "to the top"),
    bind(Action::Bottom, &[KeyCode::Char('G'), KeyCode::End], None, "to the bottom"),
    bind(Action::StartSearch, &[KeyCode::Char('/')], None, "search"),
    bind(Action::NextMatch, &[KeyCode::Char('n')], None, "next match"),
    bind(Action::PreviousMatch, &[KeyCode::Char('N')], None, "previous match"),
    bind(Action::Quit, &[KeyCode::Char('q'), KeyCode::Esc], Some('c'), "quit"),
];

const PROMPTING: &[Binding] = &[
    bind(Action::Confirm, &[KeyCode::Enter], None, "in a search: run it"),
    bind(Action::Cancel, &[KeyCode::Esc], None, "in a search: cancel it"),
    bind(Action::Erase, &[KeyCode::Backspace], None, "in a search: erase a character"),
];

fn find(bindings: &[Binding], is_bound: impl Fn(&Binding) -> bool) -> Option<Action> {
    bindings.iter().find(|binding| is_bound(binding)).map(|binding| binding.action.clone())
}

fn control(code: KeyCode) -> Option<Action> {
    let KeyCode::Char(character) = code else { return None };
    find(READING, |binding| binding.control == Some(character))
}

fn reading(code: KeyCode) -> Option<Action> {
    find(READING, |binding| binding.keys.contains(&code))
}

fn prompting(code: KeyCode) -> Option<Action> {
    match code {
        KeyCode::Char(character) => Some(Action::Type(character)),
        _ => find(PROMPTING, |binding| binding.keys.contains(&code)),
    }
}

/// Spelled the same on every platform, where crossterm says `Return` on macOS.
fn key_name(code: KeyCode) -> String {
    match code {
        KeyCode::Char(' ') => "Space".to_owned(),
        KeyCode::Char(character) => character.to_string(),
        KeyCode::Enter => "Enter".to_owned(),
        KeyCode::Backspace => "Backspace".to_owned(),
        other => other.to_string(),
    }
}

fn keys_label(binding: &Binding) -> String {
    let control = binding.control.map(|character| format!("Ctrl-{character}"));
    binding.keys.iter().map(|code| key_name(*code)).chain(control).collect::<Vec<_>>().join(", ")
}

/// Every binding as its keys and what they do: the reading keys first, then the search prompt's.
pub fn help() -> Vec<(String, &'static str)> {
    READING.iter().chain(PROMPTING).map(|binding| (keys_label(binding), binding.meaning)).collect()
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

    #[test]
    fn the_help_names_every_key_of_an_action() {
        let help = help();

        assert!(help.contains(&("Space, f, Page Down, Ctrl-f".to_owned(), "down a page")), "{help:?}");
        assert!(help.contains(&("q, Esc, Ctrl-c".to_owned(), "quit")), "{help:?}");
        assert_eq!(help.len(), READING.len() + PROMPTING.len());
    }
}
