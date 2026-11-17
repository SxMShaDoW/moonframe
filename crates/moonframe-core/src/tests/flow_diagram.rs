use {super::*, crate::Stack};

const COMPONENT: &str = include_str!("../../../../app/components/flow-diagram.luau");

/// Three nodes in an L: `web` and `api` in one row, `db` under `api`.
const NODES: &str = r#"nodes = {
    { id = "web", label = "Web", col = 0, row = 0 },
    { id = "api", label = "API", col = 1, row = 0 },
    { id = "db", label = "Database", col = 1, row = 1 },
}"#;

/// What the diagram drew with `props` beside [`NODES`]: the children of its box.
fn diagram(props: &str) -> Vec<Node> {
    let source = format!(
        "local flow_diagram = use \"flow-diagram\"\nreturn mod {{ view = function()\n  return \
         flow_diagram {{ {NODES}, {props} }}\nend }}\n"
    );
    let dir = app(&[("flow", &source)]);
    write(dir.path(), "components/flow-diagram.luau", COMPONENT);
    let workspace = Workspace::open(dir.path(), false);
    let slot = workspace.slot("flow").unwrap();
    assert_eq!(slot.error().map(ToString::to_string), None);
    let Some(Node::Stack(root)) = slot.view() else {
        panic!("expected the diagram's box");
    };
    root.children.clone()
}

fn keys(children: &[Node]) -> Vec<&str> {
    children
        .iter()
        .filter_map(|child| match child {
            Node::Stack(stack) => stack.key.as_deref(),
            _ => None,
        })
        .collect()
}

/// The card of node `id`: the middle child of its keyed cell.
fn card<'a>(children: &'a [Node], id: &str) -> &'a Stack {
    let cell = children.iter().find_map(|child| match child {
        Node::Stack(stack) if stack.key.as_deref() == Some(id) => Some(stack),
        _ => None,
    });
    match cell.map(|cell| &cell.children[1]) {
        Some(Node::Stack(card)) => card,
        _ => panic!("no card for {id}"),
    }
}

fn sent<'a>(card: &'a Stack, key: &str) -> Option<&'a str> {
    match &card.action {
        Some(Action::Send { msg, .. }) => msg.get(key).and_then(Data::as_str),
        _ => None,
    }
}

#[test]
fn flow_diagram_draws_level_and_upright_links_and_leaves_out_a_slanted_one() {
    let still = diagram("");
    let linked = diagram(
        r#"links = { { from = "web", to = "api" }, { from = "api", to = "db" }, { from = "web", to = "db" } }"#,
    );
    // Each node is a keyed cell; the two links it can draw come before them.
    assert_eq!(keys(&still), ["web", "api", "db"]);
    assert_eq!(keys(&linked), ["web", "api", "db"]);
    assert_eq!(linked.len() - still.len(), 2);
}

#[test]
fn flow_diagram_tokens_show_only_with_a_time() {
    let flows =
        r#"flows = { { path = { "web", "api", "db" }, tokens = { "warning", "danger" } } }"#;
    let still = diagram(flows);
    let moving = diagram(&format!("{flows}, time = 1.5"));
    assert_eq!(moving.len() - still.len(), 2);
}

#[test]
fn flow_diagram_a_click_on_a_node_sends_its_id() {
    let plain = diagram(r#"selected = "api""#);
    assert_eq!(sent(card(&plain, "web"), "select"), Some("web"));
    assert!(card(&plain, "api").selected);
    assert!(!card(&plain, "web").selected);
    let named = diagram(r#"event = "node""#);
    assert_eq!(sent(card(&named, "db"), "node"), Some("db"));
}
