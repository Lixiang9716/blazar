use blazar::chat::commands::CommandRegistry;

#[test]
fn builtin_registry_contains_all_palette_commands() {
    let registry = CommandRegistry::with_builtins().expect("bootstrap built-ins");

    let names: Vec<&str> = registry
        .list()
        .iter()
        .map(|spec| spec.name.as_str())
        .collect();
    for required in [
        "/help",
        "/clear",
        "/copy",
        "/init",
        "/skills",
        "/model",
        "/mcp",
        "/theme",
        "/history",
        "/plan",
        "/export",
        "/compact",
        "/config",
        "/tools",
        "/agents",
        "/discover-agents",
        "/context",
        "/diff",
        "/git",
        "/undo",
        "/terminal",
        "/debug",
        "/log",
        "/quit",
    ] {
        assert!(names.contains(&required), "missing {required}");
    }
}

#[test]
fn chat_command_registry_builtin_registry_contains_plan_and_discover_agents() {
    let registry = CommandRegistry::with_builtins().expect("bootstrap built-ins");

    let specs = registry.list();
    assert!(specs.iter().any(|s| s.name == "/plan"));
    assert!(specs.iter().any(|s| s.name == "/discover-agents"));
}

#[test]
fn chat_command_registry_rejects_duplicate_command_names() {
    // This test verifies that the inventory-based bootstrap path enforces
    // uniqueness. We can't re-run with_builtins twice on the same registry
    // because it creates a new registry, so we test that duplicates within
    // the inventory submissions are caught at build time (covered by plugin.rs logic).
    // Here we test the manual registration path still rejects duplicates.
    use blazar::chat::commands::builtins::register_builtin_commands;

    let mut registry = CommandRegistry::new();
    register_builtin_commands(&mut registry).expect("first registration");

    let err =
        register_builtin_commands(&mut registry).expect_err("duplicate registration must fail");
    assert!(err.to_string().contains("duplicate command"));
}

#[test]
fn inventory_bootstrap_produces_sorted_commands() {
    let registry = CommandRegistry::with_builtins().expect("bootstrap built-ins");

    let names: Vec<String> = registry
        .list()
        .iter()
        .map(|spec| spec.name.clone())
        .collect();

    let mut sorted_names = names.clone();
    sorted_names.sort();

    assert_eq!(
        names, sorted_names,
        "commands should be registered in alphabetical order"
    );
}

#[test]
fn inventory_bootstrap_and_manual_registration_produce_same_commands() {
    use blazar::chat::commands::builtins::register_builtin_commands;

    let inventory_registry = CommandRegistry::with_builtins().expect("bootstrap built-ins");
    let mut manual_registry = CommandRegistry::new();
    register_builtin_commands(&mut manual_registry).expect("manual registration");

    let mut inventory_names: Vec<String> = inventory_registry
        .list()
        .iter()
        .map(|spec| spec.name.clone())
        .collect();

    let mut manual_names: Vec<String> = manual_registry
        .list()
        .iter()
        .map(|spec| spec.name.clone())
        .collect();

    // Sort both lists for comparison since inventory produces sorted output
    // but manual registration preserves registration order
    inventory_names.sort();
    manual_names.sort();

    assert_eq!(
        inventory_names, manual_names,
        "inventory and manual registration should produce the same set of commands"
    );
}
