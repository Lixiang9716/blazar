use blazar::chat::commands::{CommandRegistry, builtins::register_builtin_commands};

#[test]
fn builtin_registry_contains_plan_and_discover_agents() {
    let mut registry = CommandRegistry::new();
    register_builtin_commands(&mut registry).expect("register built-ins");

    let specs = registry.list();
    assert!(specs.iter().any(|s| s.name == "/plan"));
    assert!(specs.iter().any(|s| s.name == "/discover-agents"));
}

#[test]
fn registry_rejects_duplicate_command_names() {
    let mut registry = CommandRegistry::new();
    register_builtin_commands(&mut registry).expect("first registration");

    let err =
        register_builtin_commands(&mut registry).expect_err("duplicate registration must fail");
    assert!(err.to_string().contains("duplicate command"));
}
