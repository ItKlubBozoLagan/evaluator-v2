use crate::messages::{
    CheckerData, Evaluation, EvaluationLanguage, InteractiveEvaluation, Message, Testcase,
};
use crate::state::AppState;
use crate::tracing::setup_tracing;
use std::sync::Arc;
use ::tracing::info;

mod messages;
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

    let sample_evaluation = Evaluation::Interactive(InteractiveEvaluation {
        id: 1,
        code: r#"
i = 0
while True:
    print("? " + str(i))
    ans = input()
    if ans == "Y":
        print("! " + str(i))
        break
    i += 1
            "#
        .to_string(),
        time_limit: 10000,
        memory_limit: 10240,
        language: EvaluationLanguage::Python,
        checker: CheckerData {
            script: r#"
secret = int(input())

meta = open("interactor_meta.out", "w")

try:
    while True:
        line = input()
        a = line.split(" ")
        if a[0] == "?":
            if int(a[1]) >= secret:
                print("Y")
            else:
                print("N")
        else:
            meta.write("AC" if int(a[1]) == secret else "WA")
            break
except:
    meta.write("WA")

meta.close()
            "#
            .to_string(),
            language: EvaluationLanguage::Python,
        },
        testcases: vec![
            Testcase {
                id: 1,
                input: "10".to_string(),
                output: "".to_string(),
            },
            Testcase {
                id: 2,
                input: "986".to_string(),
                output: "".to_string(),
            },
            Testcase {
                id: 3,
                input: "1000".to_string(),
                output: "".to_string(),
            },
            Testcase {
                id: 4,
                input: "1000000".to_string(),
                output: "".to_string(),
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

    let client = redis::Client::open("redis://localhost:6379")?;

    let (tx, _) = tokio::sync::broadcast::channel::<Message>(16);

    let evaluation_handler = tokio::spawn(evaluate::queue_handler::handle(
        state.clone(),
        tx.subscribe(),
        client.get_connection_manager().await?,
    ));

    info!("Started");

    messages::handler::handle_messages(state.clone(), client.get_connection_manager().await?, tx)
        .await;

    let _ = evaluation_handler.await;

    Ok(())
}
