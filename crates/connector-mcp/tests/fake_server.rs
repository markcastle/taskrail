//! End to end against the fake tracker binary: connect over stdio, list,
//! change a status, create, and see the change on the next list.

use taskrail_connector_api::Connector;
use taskrail_connector_mcp::profile::fake_tracker_profile;
use taskrail_connector_mcp::{McpConnector, McpSession, ServerSpec, SessionAuth};
use taskrail_core::domain::{Priority, Status};

fn server() -> &'static str {
    env!("CARGO_BIN_EXE_taskrail-fake-mcp-server")
}

#[tokio::test]
async fn the_fake_tracker_describes_its_tools_with_annotations() {
    let spec = ServerSpec::Stdio {
        command: server().into(),
        args: vec![],
        env: Default::default(),
    };
    let session = McpSession::connect(&spec, &SessionAuth::default())
        .await
        .unwrap();
    let tools = session.tools().await.unwrap();
    let mut names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();
    names.sort_unstable();
    assert_eq!(
        names,
        [
            "create_ticket",
            "get_board",
            "get_ticket",
            "list_boards",
            "list_tickets",
            "transition_ticket",
            "whoami"
        ]
    );
    let by_name = |name: &str| tools.iter().find(|t| t.name == name).unwrap();
    assert!(by_name("list_tickets").read_only);
    assert!(!by_name("transition_ticket").read_only);
    assert!(
        by_name("list_tickets").output_schema.is_some(),
        "structured output is advertised"
    );
    session.close().await;
}

#[tokio::test]
async fn list_then_transition_then_create() {
    let connector = McpConnector::new(fake_tracker_profile(server()), SessionAuth::default());
    let info = connector.info();
    assert!(info.capabilities.write_status && info.capabilities.create);

    let items = connector.list().await.unwrap();
    assert_eq!(
        items.len(),
        7,
        "the demo profile asks for \"me\"; the subtask and the archived one count too"
    );
    let first = &items[0];
    assert_eq!(first.external_id, "T-101");
    assert_eq!(first.title, "Prepare board deck");
    assert_eq!(first.status.as_deref(), Some("Selected"));
    assert_eq!(first.priority, Some(Priority::Highest));
    assert!(first.due.is_some());
    assert_eq!(
        first.url.as_deref(),
        Some("https://tracker.example/tickets/T-101")
    );

    connector
        .set_status("T-104", Status::InProgress)
        .await
        .unwrap();
    let created = connector.create("Write the connector").await.unwrap();
    assert_eq!(created.title, "Write the connector");
    assert_eq!(created.status.as_deref(), Some("Backlog"));

    let again = connector.list().await.unwrap();
    assert_eq!(again.len(), 8, "the new ticket is Mark's too");
    let moved = again.iter().find(|i| i.external_id == "T-104").unwrap();
    assert_eq!(moved.status.as_deref(), Some("In Progress"));
    assert_eq!(
        connector
            .status_map()
            .section_for(moved.status.as_deref().unwrap()),
        Some(Status::InProgress)
    );
}

#[tokio::test]
async fn a_profile_can_fan_out_over_boards() {
    use taskrail_connector_mcp::ForEach;
    let mut profile = fake_tracker_profile(server());
    profile.list.call.tool = "get_board".into();
    profile.list.call.arguments = serde_json::json!({ "board_id": "{board_id}" });
    profile.list.items = "$.tasks[*]".into();
    profile.list.for_each = Some(ForEach {
        tool: "list_boards".into(),
        arguments: serde_json::json!({}),
        items: "$.boards[*]".into(),
        id: "$.board_id".into(),
        placeholder: "{board_id}".into(),
        filter: vec![],
    });
    assert!(profile.validate().is_empty());
    let connector = McpConnector::new(profile, SessionAuth::default());
    let items = connector.list().await.unwrap();
    assert_eq!(items.len(), 9, "every ticket across both boards");
    assert!(items.iter().any(|i| i.external_id == "T-101"));
    assert!(items.iter().any(|i| i.external_id == "T-107"));
}

#[tokio::test]
async fn where_clauses_narrow_a_fan_out_to_one_board_and_one_person() {
    use taskrail_connector_mcp::{Condition, ForEach};
    let mut profile = fake_tracker_profile(server());
    profile.list.call.tool = "get_board".into();
    profile.list.call.arguments = serde_json::json!({ "board_id": "{board_id}" });
    profile.list.items = "$.tasks[*]".into();
    profile.list.filter = vec![Condition {
        path: "$.assignee".into(),
        equals: Some(serde_json::json!("Mark")),
        ..Default::default()
    }];
    profile.list.for_each = Some(ForEach {
        tool: "list_boards".into(),
        arguments: serde_json::json!({}),
        items: "$.boards[*]".into(),
        id: "$.board_id".into(),
        placeholder: "{board_id}".into(),
        filter: vec![Condition {
            path: "$.name".into(),
            contains: Some("work".into()),
            ..Default::default()
        }],
    });
    assert!(profile.validate().is_empty());
    let connector = McpConnector::new(profile, SessionAuth::default());
    let items = connector.list().await.unwrap();
    let ids: Vec<&str> = items.iter().map(|i| i.external_id.as_str()).collect();
    assert_eq!(
        ids,
        ["T-101", "T-102", "T-104", "T-108", "T-109"],
        "the Work board only, without Sam's T-103"
    );
}

#[tokio::test]
async fn list_tickets_honours_the_assignee_argument() {
    let profile = fake_tracker_profile(server());
    let connector = McpConnector::new(profile, SessionAuth::default());
    let items = connector.list().await.unwrap();
    assert_eq!(items.len(), 7, "\"me\" leaves out Sam's two tickets");
    assert!(items
        .iter()
        .all(|i| i.external_id != "T-103" && i.external_id != "T-106"));
}

#[tokio::test]
async fn the_demo_blueprint_connects_the_fake_tracker_with_no_model() {
    use taskrail_connector_mcp::{discover_or_blueprint, DiscoveryRequest, Registry};
    let request = DiscoveryRequest {
        id: "demo".into(),
        name: "Demo".into(),
        server: fake_tracker_profile(server()).server,
        kind: taskrail_core::domain::ItemKind::Task,
        hint: String::new(),
        allow_unannotated_list: false,
        auth: SessionAuth::default(),
        taken_ids: vec![],
    };
    let outcome = discover_or_blueprint(&request, &Registry::bundled(), None)
        .await
        .unwrap();
    assert_eq!(outcome.blueprint.as_deref(), Some("Demo tracker"));
    assert_eq!(
        outcome.profile.name, "Demo",
        "a name the person typed is kept"
    );

    let mut unnamed = request.clone();
    unnamed.name = String::new();
    unnamed.id = String::new();
    unnamed.taken_ids = vec!["demo-tracker".into()];
    let named = discover_or_blueprint(&unnamed, &Registry::bundled(), None)
        .await
        .unwrap();
    assert_eq!(named.profile.name, "Demo tracker", "the blueprint names it");
    assert_eq!(named.profile.id, "demo-tracker-2");
    assert_eq!(outcome.rounds, 0, "no model rounds");
    assert_eq!(
        outcome.sample.len(),
        6,
        "{{me}} became mark, so Sam's tickets stay out, and the archived one too"
    );
    assert!(outcome.profile.identity.is_some());
    assert!(
        outcome.profile.uses_me(),
        "the saved profile keeps {{me}}, so it stays shareable"
    );

    // The saved profile keeps working through the connector, which resolves {me} on connect.
    let connector = McpConnector::new(outcome.profile, SessionAuth::default());
    let items = connector.list().await.unwrap();
    assert_eq!(items.len(), 6);
    assert_eq!(
        items
            .iter()
            .find(|i| i.external_id == "T-108")
            .unwrap()
            .trail,
        ["Work", "Prepare board deck"],
        "the blueprint's trail reaches the rail"
    );
}

#[tokio::test]
async fn no_blueprint_and_no_model_is_a_clear_message() {
    use taskrail_connector_mcp::{
        discover_or_blueprint, DiscoveryError, DiscoveryRequest, Registry,
    };
    let request = DiscoveryRequest {
        id: "demo".into(),
        name: "Demo".into(),
        server: fake_tracker_profile(server()).server,
        kind: taskrail_core::domain::ItemKind::Task,
        hint: String::new(),
        allow_unannotated_list: false,
        auth: SessionAuth::default(),
        taken_ids: vec![],
    };
    let empty = Registry::parse(r#"{"blueprints": []}"#).unwrap();
    let err = discover_or_blueprint(&request, &empty, None)
        .await
        .unwrap_err();
    assert!(matches!(err, DiscoveryError::NoModel(_)), "{err}");
    assert!(err.to_string().contains("no API key"), "{err}");
}

/// The Work and Home boards with a trail (board, group, parent) and the
/// choices a person can make: only mine, which boards, which groups.
fn board_profile_with_choices() -> taskrail_connector_mcp::ConnectorProfile {
    use taskrail_connector_mcp::{Choice, Condition, ForEach, Identity, TrailStep};
    let mut profile = fake_tracker_profile(server());
    profile.list.call.tool = "get_board".into();
    profile.list.call.arguments = serde_json::json!({ "board_id": "{board_id}" });
    profile.list.items = "$.tasks[*]".into();
    profile.list.filter = vec![Condition {
        path: "$.archived_at".into(),
        not: true,
        ..Default::default()
    }];
    profile.list.for_each = Some(ForEach {
        tool: "list_boards".into(),
        arguments: serde_json::json!({}),
        items: "$.boards[*]".into(),
        id: "$.board_id".into(),
        placeholder: "{board_id}".into(),
        filter: vec![],
    });
    profile.list.trail = vec![
        TrailStep {
            name: "board".into(),
            label: "$.board.name".into(),
            key: None,
            items: None,
            id: None,
        },
        TrailStep {
            name: "group".into(),
            label: "$.name".into(),
            key: Some("$.group_id".into()),
            items: Some("$.groups[*]".into()),
            id: Some("$.id".into()),
        },
        TrailStep {
            name: "parent".into(),
            label: "$.summary".into(),
            key: Some("$.parent_id".into()),
            items: Some("$.tasks[*]".into()),
            id: Some("$.ticket_id".into()),
        },
    ];
    profile.identity = Some(Identity {
        tool: "whoami".into(),
        arguments: serde_json::json!({}),
        id: "$.id".into(),
    });
    profile.choices = vec![
        Choice::Switch {
            id: "mine".into(),
            label: "Only tickets assigned to me".into(),
            default: true,
            filter: vec![Condition {
                path: "$.assignee".into(),
                equals: Some(serde_json::json!("{me}")),
                ..Default::default()
            }],
        },
        Choice::Pick {
            id: "boards".into(),
            label: "Boards".into(),
            containers: Some("$.name".into()),
            trail: None,
        },
        Choice::Pick {
            id: "groups".into(),
            label: "Groups".into(),
            containers: None,
            trail: Some("group".into()),
        },
    ];
    assert!(profile.validate().is_empty(), "{:?}", profile.validate());
    profile
}

#[tokio::test]
async fn items_carry_a_trail_of_board_group_and_parent() {
    let connector = McpConnector::new(board_profile_with_choices(), SessionAuth::default());
    let items = connector.list().await.unwrap();
    let by_id = |id: &str| items.iter().find(|i| i.external_id == id).unwrap();
    assert_eq!(by_id("T-101").trail, ["Work", "Now"]);
    assert_eq!(
        by_id("T-108").trail,
        ["Work", "Prepare board deck"],
        "a subtask shows its board and its parent"
    );
    assert_eq!(
        by_id("T-105").trail,
        ["Home"],
        "a board with no groups gives just the board"
    );
    assert!(
        items.iter().all(|i| i.external_id != "T-109"),
        "archived tickets are left out"
    );
    assert!(
        items.iter().all(|i| i.external_id != "T-103"),
        "the switch is on by default, so Sam's are out"
    );
}

#[tokio::test]
async fn a_scope_turns_switches_off_and_picks_boards_and_groups() {
    use taskrail_connector_mcp::Scope;
    let profile = board_profile_with_choices();

    let options = McpConnector::new(profile.clone(), SessionAuth::default())
        .scope_options()
        .await
        .unwrap();
    assert_eq!(options["boards"], ["Work", "Home"]);
    assert_eq!(options["groups"], ["Later", "Now"]);

    let mut scope = Scope::default();
    scope.switches.insert("mine".into(), false);
    let all = McpConnector::with_scope(profile.clone(), SessionAuth::default(), scope.clone())
        .list()
        .await
        .unwrap();
    assert!(
        all.iter().any(|i| i.external_id == "T-103"),
        "switch off: Sam's tickets come in"
    );

    scope.picks.insert("boards".into(), vec!["Work".into()]);
    scope.picks.insert("groups".into(), vec!["Later".into()]);
    let narrow = McpConnector::with_scope(profile, SessionAuth::default(), scope)
        .list()
        .await
        .unwrap();
    let ids: Vec<&str> = narrow.iter().map(|i| i.external_id.as_str()).collect();
    assert_eq!(
        ids,
        ["T-103", "T-104"],
        "one group on one board, including the one not assigned to me"
    );
}

#[tokio::test]
async fn a_blueprint_that_finds_nothing_for_this_person_still_connects() {
    use taskrail_connector_mcp::{discover_or_blueprint, Blueprint, DiscoveryRequest, Registry};
    // The demo blueprint, but with the only-mine switch comparing against someone who has no tickets.
    let mut registry = Registry::bundled();
    let demo: &mut Blueprint = registry
        .blueprints
        .iter_mut()
        .find(|b| b.id == "demo-tracker")
        .unwrap();
    let choices = demo.profile["choices"].as_array_mut().unwrap();
    choices[0]["where"][0]["equals"] = serde_json::json!("nobody");
    let request = DiscoveryRequest {
        id: "demo".into(),
        name: "Demo".into(),
        server: fake_tracker_profile(server()).server,
        kind: taskrail_core::domain::ItemKind::Task,
        hint: String::new(),
        allow_unannotated_list: false,
        auth: SessionAuth::default(),
        taken_ids: vec![],
    };
    let outcome = discover_or_blueprint(&request, &registry, None)
        .await
        .expect("an empty result is a connection, not a failure");
    assert_eq!(outcome.blueprint.as_deref(), Some("Demo tracker"));
    assert!(outcome.sample.is_empty());
    assert!(
        outcome.notes[0].contains("nothing is on the rail yet"),
        "{:?}",
        outcome.notes
    );
}
