use std::cmp::min;
use std::path::PathBuf;
use std::time::Duration;
use std::{env, fs};
use tokio::sync::OnceCell;

// 2 MiB
const HARD_PIPE_MAX_SIZE_LIMIT: usize = 2 << 20;

#[derive(Debug)]
pub struct SystemEnvironment {
    pub pipe_max_size: usize,
}

#[derive(Debug)]
pub struct CompileCache {
    pub cache_dir: String,
}

#[derive(Debug)]
pub struct Environment {
    pub force_debug_logs: bool,
    pub max_evaluations: u8,
    pub redis_url: String,
    pub redis_ca_file: Option<PathBuf>,
    pub redis_require_tls: bool,
    pub redis_queue_key: String,
    pub redis_connection_timeout: Duration,
    pub redis_command_timeout: Duration,
    pub redis_publish_attempts: u32,
    pub run_with_cgroups: bool,
    pub run_with_quotas: bool,
    pub exit_on_empty_queue: bool,
    pub compile_cache: Option<CompileCache>,

    pub system_environment: SystemEnvironment,
}

static ENVIRONMENT: OnceCell<Environment> = OnceCell::const_new();

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
            redis_ca_file: env::var_os("REDIS_CA_FILE").map(PathBuf::from),
            redis_require_tls: env::var("REDIS_REQUIRE_TLS")
                .unwrap_or("false".to_string())
                .parse::<bool>()
                .expect("REDIS_REQUIRE_TLS must be a boolean"),
            redis_queue_key: env::var("REDIS_QUEUE_KEY")
                .unwrap_or("evaluator_msg_queue".to_string()),
            redis_connection_timeout: Duration::from_millis(
                env::var("REDIS_CONNECTION_TIMEOUT_MS")
                    .unwrap_or("5000".to_string())
                    .parse::<u64>()
                    .expect("REDIS_CONNECTION_TIMEOUT_MS must be a number"),
            ),
            redis_command_timeout: Duration::from_millis(
                env::var("REDIS_COMMAND_TIMEOUT_MS")
                    .unwrap_or("5000".to_string())
                    .parse::<u64>()
                    .expect("REDIS_COMMAND_TIMEOUT_MS must be a number"),
            ),
            redis_publish_attempts: env::var("REDIS_PUBLISH_ATTEMPTS")
                .unwrap_or("5".to_string())
                .parse::<u32>()
                .expect("REDIS_PUBLISH_ATTEMPTS must be a number")
                .max(1),
            run_with_cgroups: env::var("RUN_WITH_CGROUPS")
                .unwrap_or("true".to_string())
                .parse::<bool>()
                .expect("RUN_WITH_CGROUPS must be a boolean"),
            run_with_quotas: env::var("RUN_WITH_QUOTAS")
                .unwrap_or("true".to_string())
                .parse::<bool>()
                .expect("RUN_WITH_QUOTAS must be a boolean"),
            exit_on_empty_queue: env::var("EXIT_ON_EMPTY_QUEUE")
                .unwrap_or("false".to_string())
                .parse::<bool>()
                .expect("EXIT_ON_EMPTY_QUEUE must be a boolean"),
            compile_cache: env::var("COMPILE_CACHE_ENABLED")
                .unwrap_or("false".to_string())
                .parse::<bool>()
                .expect("COMPILE_CACHE_ENABLED must be a boolean")
                .then(|| CompileCache {
                    cache_dir: env::var("COMPILE_CACHE_DIR")
                        .expect("COMPILE_CACHE_DIR must be set"),
                }),
            system_environment,
        }
    }

    pub fn init() -> anyhow::Result<()> {
        ENVIRONMENT
            .set(Environment::new())
            .map_err(|_| anyhow::anyhow!("environment already initialized"))
    }

    pub fn get() -> &'static Environment {
        match ENVIRONMENT.get() {
            Some(env) => env,
            None => panic!("Environment not initialized"),
        }
    }
}

fn read_pipe_max_size() -> anyhow::Result<usize> {
    let content = fs::read_to_string("/proc/sys/fs/pipe-max-size")?;
    let pipe_max_size: usize = content.trim().parse()?;

    Ok(pipe_max_size)
}
