//! Discovery against the fake tracker, with a scripted stand-in for the model.

use std::sync::Mutex;

use async_trait::async_trait;
use taskrail_connector_mcp::discovery::MAX_ROUNDS;
use taskrail_connector_mcp::profile::fake_tracker_profile;
use taskrail_connector_mcp::{discover, DiscoveryError, DiscoveryRequest, ServerSpec, SessionAuth};
use taskrail_core::domain::ItemKind;
use taskrail_llm::{LlmClient, LlmError, LlmRequest};

/// A model that answers from a script, one answer per call, and remembers
/// what it was asked.
struct Scripted {
    answers: Mutex<Vec<String>>,
    prompts: Mutex<Vec<String>>,
    histories: Mutex<Vec<usize>>,
}

impl Scripted {
    fn new(answers: &[&str]) -> Self {
        Self {
            answers: Mutex::new(answers.iter().rev().map(|s| s.to_string()).collect()),
            prompts: Mutex::new(Vec::new()),
            histories: Mutex::new(Vec::new()),
        }
    }
}

#[async_trait]
impl LlmClient for Scripted {
    async fn complete(&self, request: &LlmRequest) -> Result<String, LlmError> {
        self.prompts
            .lock()
            .unwrap()
            .push(request.last_user().to_owned());
        self.histories.lock().unwrap().push(request.messages.len());
        self.answers
            .lock()
            .unwrap()
            .pop()
            .ok_or_else(|| LlmError::Malformed("script exhausted".into()))
    }

    fn describe(&self) -> String {
        "scripted".into()
    }
}

fn request() -> DiscoveryRequest {
    DiscoveryRequest {
        id: "fake".into(),
        name: "Fake".into(),
        server: ServerSpec::Stdio {
            command: env!("CARGO_BIN_EXE_taskrail-fake-mcp-server").into(),
            args: vec![],
            env: Default::default(),
        },
        kind: ItemKind::Task,
        hint: String::new(),
        allow_unannotated_list: false,
        auth: SessionAuth::default(),
    }
}

/// The right answer, wrapped the way models like to wrap things.
fn good_answer() -> String {
    let mut profile = serde_json::to_value(fake_tracker_profile("x")).unwrap();
    for key in ["id", "name", "server", "kind", "version"] {
        profile.as_object_mut().unwrap().remove(key);
    }
    format!("Here is the profile:\n```json\n{profile}\n```")
}

#[tokio::test]
async fn a_correct_first_answer_is_validated_against_the_live_server() {
    let llm = Scripted::new(&[&good_answer()]);
    let outcome = discover(&request(), &llm).await.unwrap();
    assert_eq!(outcome.rounds, 1);
    assert_eq!(
        outcome.sample.len(),
        5,
        "the demo profile asks for \"me\", so Sam's tickets stay out"
    );
    assert_eq!(outcome.profile.id, "fake");
    assert!(matches!(outcome.profile.server, ServerSpec::Stdio { .. }));
    assert!(outcome.notes.is_empty());
    let prompts = llm.prompts.lock().unwrap();
    assert!(
        prompts[0].contains("list_tickets"),
        "the tool catalogue is in the prompt"
    );
}

#[tokio::test]
async fn a_wrong_items_path_is_sent_back_with_a_sample_and_fixed_next_round() {
    let bad = good_answer().replace("$.tickets[*]", "$.issues[*]");
    let llm = Scripted::new(&[&bad, &good_answer()]);
    let outcome = discover(&request(), &llm).await.unwrap();
    assert_eq!(outcome.rounds, 2);
    assert_eq!(outcome.notes.len(), 1);
    let prompts = llm.prompts.lock().unwrap();
    assert!(prompts[1].contains("matched nothing"));
    assert!(
        prompts[1].contains("\"ticket_id\""),
        "the retry shows the real response"
    );
}

#[tokio::test]
async fn a_list_tool_that_writes_stops_for_confirmation() {
    let risky = good_answer().replace(
        "\"tool\":\"list_tickets\"",
        "\"tool\":\"transition_ticket\"",
    );
    let llm = Scripted::new(&[&risky]);
    let err = discover(&request(), &llm).await.unwrap_err();
    match &err {
        DiscoveryError::NeedsConfirmation { tool, .. } => assert_eq!(tool, "transition_ticket"),
        other => panic!("unexpected: {other}"),
    }
}

#[tokio::test]
async fn nonsense_every_round_gives_up_with_the_last_problem() {
    let answers: Vec<&str> = (0..MAX_ROUNDS)
        .map(|i| {
            if i % 2 == 0 {
                "not json"
            } else {
                "{\"list\": 5}"
            }
        })
        .collect();
    let llm = Scripted::new(&answers);
    let err = discover(&request(), &llm).await.unwrap_err();
    assert!(
        matches!(err, DiscoveryError::GaveUp { rounds, .. } if rounds == MAX_ROUNDS),
        "{err}"
    );
}

/// A profile that reads tickets board by board, the way a real tracker
/// without a global list would need.
fn board_profile_answer() -> String {
    let mut value = serde_json::to_value(fake_tracker_profile("x")).unwrap();
    for key in ["id", "name", "server", "kind", "version"] {
        value.as_object_mut().unwrap().remove(key);
    }
    value["list"]["tool"] = serde_json::json!("get_board");
    value["list"]["arguments"] = serde_json::json!({ "board_id": "{board_id}" });
    value["list"]["items"] = serde_json::json!("$.tasks[*]");
    value["list"]["for_each"] = serde_json::json!({
        "tool": "list_boards", "arguments": {}, "items": "$.boards[*]", "id": "$.board_id", "placeholder": "{board_id}"
    });
    value.to_string()
}

#[tokio::test]
async fn the_model_can_explore_before_writing_and_fan_out_over_boards() {
    let explore = r#"{"explore": [{"tool": "list_boards", "arguments": {}}, {"tool": "transition_ticket", "arguments": {"ticket_id": "T-101", "state": "Done"}}]}"#;
    let llm = Scripted::new(&[explore, &board_profile_answer()]);
    let outcome = discover(&request(), &llm).await.unwrap();
    assert_eq!(outcome.rounds, 2);
    assert_eq!(outcome.sample.len(), 7, "tickets from both boards");
    assert!(outcome.profile.list.for_each.is_some());
    let prompts = llm.prompts.lock().unwrap();
    assert!(
        prompts[1].contains("b-1111"),
        "exploration results reach the model"
    );
    assert!(
        prompts[1].contains("refused"),
        "a write tool is refused during exploration"
    );
    assert!(outcome
        .notes
        .iter()
        .any(|n| n.contains("looked at list_boards")));
}

#[tokio::test]
async fn the_model_can_narrow_with_where_clauses() {
    let mut value: serde_json::Value = serde_json::from_str(&board_profile_answer()).unwrap();
    value["list"]["where"] = serde_json::json!([{ "path": "$.assignee", "equals": "mark" }]);
    value["list"]["for_each"]["where"] =
        serde_json::json!([{ "path": "$.name", "contains": "Home" }]);
    let llm = Scripted::new(&[&value.to_string()]);
    let outcome = discover(&request(), &llm).await.unwrap();
    assert_eq!(outcome.rounds, 1);
    let ids: Vec<&str> = outcome
        .sample
        .iter()
        .map(|i| i.external_id.as_str())
        .collect();
    assert_eq!(ids, ["T-105", "T-107"], "Home board, Mark's tickets only");
    assert_eq!(
        outcome.profile.list.filter.len(),
        1,
        "the filter survives the round trip"
    );
}

#[tokio::test]
async fn an_invented_board_id_gets_the_real_boards_back_as_a_sample() {
    let bad = board_profile_answer().replace(r#""items":"$.tasks[*]""#, r#""items":"$.cards[*]""#);
    let llm = Scripted::new(&[&bad, &board_profile_answer()]);
    let outcome = discover(&request(), &llm).await.unwrap();
    assert_eq!(outcome.rounds, 2);
    let prompts = llm.prompts.lock().unwrap();
    assert!(prompts[1].contains("matched nothing"), "{}", prompts[1]);
    assert!(
        prompts[1].contains("\"boards\""),
        "the parent call's result is the sample for a fan-out: {}",
        prompts[1]
    );
}
