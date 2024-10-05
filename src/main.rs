use crate::messages::{
    BatchEvaluation, CheckerData, Evaluation, EvaluationLanguage, Message, Testcase,
};
use crate::state::AppState;
use crate::tracing::setup_tracing;
use std::sync::Arc;
use ::tracing::info;

mod messages;
mod redis;
mod state;
mod tracing;

mod evaluate;
mod isolate;
mod util;

fn main() -> anyhow::Result<()> {
    setup_tracing();

    let rt = tokio::runtime::Builder::new_current_thread()
        .worker_threads(1)
        .enable_all()
        .build()?;

    rt.block_on(entrypoint())?;

    rt.shutdown_background();

    Ok(())
}

async fn entrypoint() -> anyhow::Result<()> {
    info!("Starting...");

    let sample_evaluation = Evaluation::Batch(BatchEvaluation {
        id: 1,
        code: r#"
a = "A" * 1148576

open("test.txt", "w").write(a)
            "#
        .to_string(),
        time_limit: 0,
        memory_limit: 0,
        language: EvaluationLanguage::Python,
        checker: Some(CheckerData {
            script: r#"
def read_until(separator):
    out = ""
    while True:
        line = input()
        if line == separator:
            return out
        out += " " + line.strip()

while True:
    separator = input()
    if len(separator.strip()) > 0:
        break

read_until(separator)
out = read_until(separator)
subOut = read_until(separator)

print(f"custom:{subOut}" if out.strip() == subOut.strip() else "WA")
            "#
            .to_string(),
            language: EvaluationLanguage::Python,
        }),
        testcases: vec![
            Testcase {
                id: 1,
                input: "-1".to_string(),
                output: "1".to_string(),
            },
            Testcase {
                id: 2,
                input: "10".to_string(),
                output: "89".to_string(),
            },
        ],
    });

    let str = serde_json::to_string(&Message::BeginEvaluation(sample_evaluation))?;
    println!("{}", str);

    let state = Arc::new(AppState {
        redis_queue_key: "evaluator_msg_queue".to_string(),
        // evaluation_wg: WaitGroup::new(),
        // max_evaluations: 1
    });

    let mut connection = redis::get_connection("redis://localhost:6379").await?;

    let (tx, _) = tokio::sync::broadcast::channel::<Message>(16);

    let evaluation_handler = tokio::spawn(evaluate::queue_handler::handle(
        state.clone(),
        tx.subscribe(),
    ));

    info!("Started");

    messages::handler::handle_messages(state.clone(), &mut connection, tx).await;

    let _ = evaluation_handler.await;

    Ok(())
}
