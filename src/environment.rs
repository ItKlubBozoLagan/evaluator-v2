use once_cell::sync::OnceCell;
use std::cmp::min;
use std::{env, fs};

// 2 MiB
const HARD_PIPE_MAX_SIZE_LIMIT: usize = 2 << 20;

#[derive(Debug)]
pub struct SystemEnvironment {
    pub pipe_max_size: usize,
}

#[derive(Debug)]
pub struct Environment {
    pub force_debug_logs: bool,
    pub max_evaluations: u8,
    pub redis_url: String,
    pub redis_queue_key: String,
    pub redis_response_pubsub: String,
    pub run_with_cgroups: bool,
    pub run_with_quotas: bool,
    pub exit_on_empty_queue: bool,

    pub system_environment: SystemEnvironment,
}

static ENVIRONMENT: OnceCell<Environment> = OnceCell::new();

impl Environment {
    fn new() -> Self {
        let system_environment = SystemEnvironment {
            pipe_max_size: read_pipe_max_size()
                .map(|it| min(it, HARD_PIPE_MAX_SIZE_LIMIT))
                .expect("/proc/sys/fs/pipe-max-size not found"),
        };

        Self {
            force_debug_logs: env::var("FORCE_DEBUG_LOGS")
                .unwrap_or("false".to_string())
                .parse::<bool>()
                .expect("FORCE_DEBUG_LOGS must be a boolean"),
            max_evaluations: env::var("EVALUATOR_MAX_EVALUATIONS")
                .unwrap_or("2".to_string())
                .parse::<u8>()
                .expect("EVALUATOR_MAX_EVALUATIONS must be a number"),
            redis_url: env::var("REDIS_URL").unwrap_or("redis://localhost:6379".to_string()),
            redis_queue_key: env::var("REDIS_QUEUE_KEY")
                .unwrap_or("evaluator_msg_queue".to_string()),
            redis_response_pubsub: env::var("REDIS_RESPONSE_PUBSUB")
                .unwrap_or("evaluator_evaluations".to_string()),
            run_with_cgroups: env::var("RUN_WITH_CGROUPS")
                .unwrap_or("true".to_string())
                .parse::<bool>()
                .expect("RUN_WITH_CGROUPS must be a boolean"),
            run_with_quotas: env::var("RUN_WITH_QUOTAS")
                .unwrap_or("true".to_string())
                .parse::<bool>()
                .expect("RUN_WITH_CGROUPS must be a boolean"),
            exit_on_empty_queue: env::var("EXIT_ON_EMPTY_QUEUE")
                .unwrap_or("false".to_string())
                .parse::<bool>()
                .expect("RUN_WITH_CGROUPS must be a boolean"),
            system_environment,
        }
    }

    pub fn init() -> Result<(), Environment> {
        ENVIRONMENT.set(Environment::new())
    }

    pub fn get() -> &'static Environment {
        ENVIRONMENT.get_or_init(Environment::new)
    }
}

fn read_pipe_max_size() -> anyhow::Result<usize> {
    let content = fs::read_to_string("/proc/sys/fs/pipe-max-size")?;
    let pipe_max_size: usize = content.trim().parse()?;

    Ok(pipe_max_size)
}
