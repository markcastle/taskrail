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
            "transition_ticket"
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
    assert_eq!(items.len(), 7);
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
        .set_status("T-103", Status::InProgress)
        .await
        .unwrap();
    let created = connector.create("Write the connector").await.unwrap();
    assert_eq!(created.title, "Write the connector");
    assert_eq!(created.status.as_deref(), Some("Backlog"));

    let again = connector.list().await.unwrap();
    assert_eq!(again.len(), 8);
    let moved = again.iter().find(|i| i.external_id == "T-103").unwrap();
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
    });
    assert!(profile.validate().is_empty());
    let connector = McpConnector::new(profile, SessionAuth::default());
    let items = connector.list().await.unwrap();
    assert_eq!(items.len(), 7, "every ticket across both boards");
    assert!(items.iter().any(|i| i.external_id == "T-101"));
    assert!(items.iter().any(|i| i.external_id == "T-107"));
}
