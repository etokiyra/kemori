use super::*;
use std::collections::HashSet;

#[test]
fn command_ids_and_shortcuts_are_unique() {
    let registry = CommandRegistry::builtin();
    let mut ids = HashSet::new();
    let mut shortcuts = HashSet::new();
    for command in registry.all() {
        assert!(
            ids.insert(command.id),
            "duplicate command id {}",
            command.id
        );
        if let Some(shortcut) = command.shortcut {
            assert!(
                shortcuts.insert(shortcut),
                "duplicate shortcut {shortcut} on {}",
                command.id
            );
        }
    }
}

#[test]
fn core_commands_are_registered() {
    let registry = CommandRegistry::builtin();
    for id in [
        ids::SAVE,
        ids::SAVE_ALL,
        ids::PALETTE,
        ids::COMPLETE,
        ids::GOTO_DEFINITION,
        ids::FORMAT,
        ids::DIFF,
        ids::SETUP,
    ] {
        assert!(registry.get(id).is_some(), "missing command {id}");
    }
}
