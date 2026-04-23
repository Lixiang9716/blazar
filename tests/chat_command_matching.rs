use blazar::chat::commands::CommandSpec;
use blazar::chat::commands::matcher::ranked_match_names;
use blazar::chat::commands::{CommandRegistry, builtins::register_builtin_commands};
use blazar::chat::picker::{ModalPicker, PickerContext, PickerItem};
use serde_json::json;

fn command_spec(name: &str, description: &str) -> CommandSpec {
    CommandSpec {
        name: name.to_string(),
        description: description.to_string(),
        args_schema: json!({}),
    }
}

mod chat_command_matching {
    use super::*;

    #[test]
    fn exact_match_ranks_first() {
        let specs = vec![
            command_spec("/planner", "Open planner"),
            command_spec("/plan", "Generate a plan"),
            command_spec("/planify", "Plan helper"),
        ];

        let ranked = ranked_match_names("/plan", &specs);
        assert_eq!(ranked.first().copied(), Some("/plan"));
    }

    #[test]
    fn prefix_beats_contains_and_fuzzy() {
        let specs = vec![
            command_spec("/x/mo", "Contains /mo"),
            command_spec("/m-ops", "Fuzzy /mo"),
            command_spec("/model", "Prefix /mo"),
        ];

        let ranked = ranked_match_names("/mo", &specs);
        assert_eq!(ranked.first().copied(), Some("/model"));
        assert_eq!(ranked, vec!["/model", "/x/mo", "/m-ops"]);
    }

    #[test]
    fn stable_order_within_same_tier() {
        let specs = vec![
            command_spec("/model", "Switch model"),
            command_spec("/modal", "Open modal"),
            command_spec("/motion", "Motion"),
        ];

        let ranked = ranked_match_names("/mo", &specs);
        assert_eq!(ranked, vec!["/model", "/modal", "/motion"]);
    }

    #[test]
    fn matching_is_case_insensitive() {
        let specs = vec![
            command_spec("/plan", "Generate a plan"),
            command_spec("/model", "Switch model"),
        ];

        let ranked = ranked_match_names("/PLAN", &specs);
        assert_eq!(ranked.first().copied(), Some("/plan"));
    }

    #[test]
    fn slash_only_returns_all_commands() {
        let specs = vec![
            command_spec("/plan", "Generate a plan"),
            command_spec("/model", "Switch model"),
            command_spec("/theme", "Switch theme"),
        ];

        let ranked = ranked_match_names("/", &specs);
        assert_eq!(ranked, vec!["/plan", "/model", "/theme"]);
    }

    #[test]
    fn non_slash_query_returns_no_commands() {
        let specs = vec![
            command_spec("/plan", "Generate a plan"),
            command_spec("/model", "Switch model"),
        ];

        let ranked = ranked_match_names("pla", &specs);
        assert!(ranked.is_empty());
    }
}

mod chat_command_matching_picker {
    use super::*;

    fn command_palette_for_test() -> ModalPicker {
        let mut registry = CommandRegistry::new();
        register_builtin_commands(&mut registry).expect("built-ins should register");
        ModalPicker::command_palette_from_registry(&registry)
    }

    #[test]
    fn command_picker_requires_slash_prefix() {
        let mut picker = command_palette_for_test();
        picker.filter = "plan".to_string();

        assert!(picker.filtered_items().is_empty());
    }

    #[test]
    fn command_picker_slash_queries_use_layered_ranking() {
        let mut picker = ModalPicker::with_context(
            "Commands",
            vec![
                PickerItem::new("/x/mo", "Contains /mo"),
                PickerItem::new("/m-ops", "Fuzzy /mo"),
                PickerItem::new("/model", "Prefix /mo"),
            ],
            PickerContext::Commands,
        );
        picker.filter = "/mo".to_string();

        let labels: Vec<&str> = picker
            .filtered_items()
            .into_iter()
            .map(|item| item.label.as_str())
            .collect();

        assert_eq!(labels, vec!["/model", "/x/mo", "/m-ops"]);
    }

    #[test]
    fn command_picker_empty_filter_shows_all_commands() {
        let picker = ModalPicker::with_context(
            "Commands",
            vec![
                PickerItem::new("/plan", "Generate plan"),
                PickerItem::new("/help", "Help"),
            ],
            PickerContext::Commands,
        );

        let labels: Vec<&str> = picker
            .filtered_items()
            .into_iter()
            .map(|item| item.label.as_str())
            .collect();

        assert_eq!(labels, vec!["/plan", "/help"]);
    }

    #[test]
    fn command_picker_auto_prefixes_slash_on_first_character() {
        let mut picker = ModalPicker::with_context(
            "Commands",
            vec![
                PickerItem::new("/plan", "Generate plan"),
                PickerItem::new("/help", "Help"),
            ],
            PickerContext::Commands,
        );

        picker.push_filter('p');

        assert_eq!(picker.filter, "/p");
        let labels: Vec<&str> = picker
            .filtered_items()
            .into_iter()
            .map(|item| item.label.as_str())
            .collect();
        assert_eq!(labels, vec!["/plan", "/help"]);
    }

    #[test]
    fn non_command_picker_preserves_contains_matching() {
        let mut picker = ModalPicker::with_context(
            "Themes",
            vec![
                PickerItem::new("Dark", "Dark mode"),
                PickerItem::new("Light", "Light mode"),
            ],
            PickerContext::ThemeSelect,
        );
        picker.filter = "dar".to_string();

        let labels: Vec<&str> = picker
            .filtered_items()
            .into_iter()
            .map(|item| item.label.as_str())
            .collect();

        assert_eq!(labels, vec!["Dark"]);
    }
}
