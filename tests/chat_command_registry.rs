use blazar::chat::commands::{CommandRegistry, builtins::register_builtin_commands};

#[test]
fn builtin_registry_contains_all_palette_commands() {
    let mut registry = CommandRegistry::new();
    register_builtin_commands(&mut registry).expect("register built-ins");

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
    let mut registry = CommandRegistry::new();
    register_builtin_commands(&mut registry).expect("register built-ins");

    let specs = registry.list();
    assert!(specs.iter().any(|s| s.name == "/plan"));
    assert!(specs.iter().any(|s| s.name == "/discover-agents"));
}

#[test]
fn chat_command_registry_rejects_duplicate_command_names() {
    let mut registry = CommandRegistry::new();
    register_builtin_commands(&mut registry).expect("first registration");

    let err =
        register_builtin_commands(&mut registry).expect_err("duplicate registration must fail");
    assert!(err.to_string().contains("duplicate command"));
}
