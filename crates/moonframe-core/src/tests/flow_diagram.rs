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

/// The card of node `id`: inside the middle row of its keyed cell.
fn card<'a>(children: &'a [Node], id: &str) -> &'a Stack {
    let cell = children.iter().find_map(|child| match child {
        Node::Stack(stack) if stack.key.as_deref() == Some(id) => Some(stack),
        _ => None,
    });
    let row = match cell.map(|cell| &cell.children[1]) {
        Some(Node::Stack(row)) => row,
        _ => panic!("no cell row for {id}"),
    };
    match row.children.first() {
        Some(Node::Stack(card)) => card,
        _ => panic!("no card for {id}"),
    }
}

/// Every text under `node`, in order.
fn texts(node: &Node) -> Vec<String> {
    match node {
        Node::Text(text) => vec![text.text.clone()],
        Node::Stack(stack) => stack.children.iter().flat_map(texts).collect(),
        _ => Vec::new(),
    }
}

/// Where each token's centre is, as fractions of the box.
fn tokens(children: &[Node]) -> Vec<(f32, f32)> {
    children
        .iter()
        .filter_map(|child| match child {
            Node::Stack(stack) => {
                let token = stack.children.iter().any(|inner| {
                    matches!(inner, Node::Stack(dot)
                        if dot.look.class.iter().any(|class| class == "flow-diagram-token"))
                });
                let area = stack.frame.area.as_ref()?;
                if !token {
                    return None;
                }
                Some((area.left? + 0.03, area.top? + 0.04))
            },
            _ => None,
        })
        .collect()
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

#[test]
fn flow_diagram_draws_every_link_word_over_the_cards() {
    let drawn = diagram(
        r#"links = { { from = "web", to = "api", label = "HTTPS" }, { from = "api", to = "db", label = "SQL" } }"#,
    );
    // Both words come after the last card, so no card covers them, and the
    // word of the upright link is drawn too.
    let last_cell = drawn
        .iter()
        .rposition(|child| matches!(child, Node::Stack(stack) if stack.key.is_some()))
        .unwrap();
    let after: Vec<String> = drawn[last_cell + 1..].iter().flat_map(texts).collect();
    assert_eq!(after, ["HTTPS", "SQL"]);
}

#[test]
fn flow_diagram_token_speed_is_the_same_in_any_grid() {
    // One token from `web` to `api`, neighbours: at 0.5 cells a second it
    // is half way after one second, however many columns the grid has.
    let flow =
        r#"flows = { { path = { "web", "api" }, tokens = { "warning" } } }, time = 1, speed = 0.5"#;
    for columns in [2.0_f32, 6.0] {
        let drawn = diagram(&format!("{flow}, columns = {columns}"));
        let [(x, _)] = tokens(&drawn)[..] else {
            panic!("expected one token");
        };
        let (web, api) = (0.5 / columns, 1.5 / columns);
        let along = (x - web) / (api - web);
        assert!((along - 0.5).abs() < 0.01, "{columns} columns: {along}");
    }
}

#[test]
fn flow_diagram_a_card_never_takes_more_than_its_cell() {
    let drawn = diagram("");
    let web = card(&drawn, "web");
    // No fixed width: it grows to its size at most, and narrows in a narrow cell.
    assert_eq!(web.frame.width, None);
    assert_eq!(web.frame.max_width, Some(136.0));
    assert!(web.frame.grow);
    let small = diagram(r#"size = "small""#);
    assert_eq!(card(&small, "web").frame.max_width, Some(126.0));
}

#[test]
fn flow_diagram_tokens_skip_a_slanted_step() {
    let slanted =
        diagram(r#"flows = { { path = { "web", "db" }, tokens = { "warning" } } }, time = 1"#);
    assert!(
        tokens(&slanted).is_empty(),
        "no token on a link that is left out"
    );
    // `web` to `db` is slanted and left out; `db` to `api` is upright, in
    // column 1 of 2: every token stays on it.
    let mixed = diagram(
        r#"flows = { { path = { "web", "db", "api" }, tokens = { "warning", "danger", "success" } } }, time = 0.3"#,
    );
    let drawn = tokens(&mixed);
    assert_eq!(drawn.len(), 3);
    for (x, _) in drawn {
        assert!(
            (x - 0.75).abs() < 0.001,
            "a token left the upright link: {x}"
        );
    }
}
