//! Discovery against the fake tracker, with a scripted stand-in for the model.

use std::sync::Mutex;

use async_trait::async_trait;
use taskrail_connector_mcp::profile::fake_tracker_profile;
use taskrail_connector_mcp::{discover, DiscoveryError, DiscoveryRequest, ServerSpec};
use taskrail_core::domain::ItemKind;
use taskrail_llm::{LlmClient, LlmError, LlmRequest};

/// A model that answers from a script, one answer per call, and remembers
/// what it was asked.
struct Scripted {
    answers: Mutex<Vec<String>>,
    prompts: Mutex<Vec<String>>,
}

impl Scripted {
    fn new(answers: &[&str]) -> Self {
        Self {
            answers: Mutex::new(answers.iter().rev().map(|s| s.to_string()).collect()),
            prompts: Mutex::new(Vec::new()),
        }
    }
}

#[async_trait]
impl LlmClient for Scripted {
    async fn complete(&self, request: &LlmRequest) -> Result<String, LlmError> {
        self.prompts.lock().unwrap().push(request.user.clone());
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
    assert_eq!(outcome.sample.len(), 7);
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
async fn nonsense_three_times_gives_up_with_the_last_problem() {
    let llm = Scripted::new(&["not json", "{\"list\": 5}", "{}"]);
    let err = discover(&request(), &llm).await.unwrap_err();
    assert!(
        matches!(err, DiscoveryError::GaveUp { rounds: 3, .. }),
        "{err}"
    );
}
