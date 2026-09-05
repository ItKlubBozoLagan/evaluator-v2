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

pub struct Environment {
    pub force_debug_logs: bool,
    pub max_evaluations: u8,
    pub redis_url: String,
    pub redis_ca_file: Option<PathBuf>,
    pub redis_require_tls: bool,
    pub redis_queue_key: String,
    pub redis_response_key_prefix: String,
    pub redis_dead_letter_key: String,
    pub redis_connection_timeout: Duration,
    pub redis_command_timeout: Duration,
    pub redis_publish_attempts: u32,
    pub run_with_cgroups: bool,
    pub run_with_quotas: bool,
    pub exit_on_empty_queue: bool,
    pub compile_cache: Option<CompileCache>,
    pub memory_budget_kib: u32,
    pub system_memory_reserve_kib: u32,
    pub max_request_bytes: usize,
    pub max_source_bytes: usize,
    pub max_checker_bytes: usize,
    pub max_testcases: usize,
    pub max_testcase_bytes: usize,
    pub max_output_bytes: usize,
    pub job_timeout: Duration,
    pub health_bind: String,
    pub system_environment: SystemEnvironment,
}

static ENVIRONMENT: OnceCell<Environment> = OnceCell::const_new();

impl Environment {
    fn new() -> anyhow::Result<Self> {
        let system_environment = SystemEnvironment {
            pipe_max_size: read_pipe_max_size()
                .map(|it| min(it, HARD_PIPE_MAX_SIZE_LIMIT))
                .map_err(|err| anyhow::anyhow!("failed to read pipe limit: {err}"))?,
        };

        let max_evaluations = parse_env("EVALUATOR_MAX_EVALUATIONS", 2_u8)?;
        if max_evaluations < 2 {
            anyhow::bail!("EVALUATOR_MAX_EVALUATIONS must be at least 2");
        }

        let memory_budget_mib = parse_env("EVALUATOR_MEMORY_BUDGET_MIB", 4096_u32)?;
        let system_memory_reserve_mib = parse_env("EVALUATOR_SYSTEM_MEMORY_RESERVE_MIB", 1024_u32)?;
        if system_memory_reserve_mib < 1024 {
            anyhow::bail!("EVALUATOR_SYSTEM_MEMORY_RESERVE_MIB must be at least 1024");
        }
        if memory_budget_mib <= system_memory_reserve_mib {
            anyhow::bail!(
                "EVALUATOR_MEMORY_BUDGET_MIB must exceed EVALUATOR_SYSTEM_MEMORY_RESERVE_MIB"
            );
        }

        let redis_url = env::var("REDIS_URL").unwrap_or("redis://localhost:6379".to_string());
        let redis_require_tls = parse_env("REDIS_REQUIRE_TLS", false)?;
        if redis_require_tls && !redis_url.starts_with("rediss://") {
            anyhow::bail!("REDIS_URL must use rediss:// when REDIS_REQUIRE_TLS=true");
        }
        if redis_url.contains("#insecure") {
            anyhow::bail!("insecure Redis TLS verification is not supported");
        }

        let redis_queue_key =
            env::var("REDIS_QUEUE_KEY").unwrap_or("evaluator_msg_queue".to_string());
        let redis_response_key_prefix =
            env::var("REDIS_RESPONSE_KEY_PREFIX").unwrap_or("evaluator_evaluations".to_string());
        if redis_queue_key.is_empty() || redis_response_key_prefix.is_empty() {
            anyhow::bail!("Redis queue keys and prefixes must not be empty");
        }

        let compile_cache = parse_env("COMPILE_CACHE_ENABLED", false)?
            .then(|| env::var("COMPILE_CACHE_DIR").map(|cache_dir| CompileCache { cache_dir }))
            .transpose()
            .map_err(|_| {
                anyhow::anyhow!("COMPILE_CACHE_DIR must be set when caching is enabled")
            })?;

        Ok(Self {
            force_debug_logs: parse_env("FORCE_DEBUG_LOGS", false)?,
            max_evaluations,
            redis_url,
            redis_ca_file: env::var_os("REDIS_CA_FILE").map(PathBuf::from),
            redis_require_tls,
            redis_queue_key: redis_queue_key.clone(),
            redis_response_key_prefix,
            redis_dead_letter_key: env::var("REDIS_DEAD_LETTER_KEY")
                .unwrap_or_else(|_| format!("{redis_queue_key}:dead-letter")),
            redis_connection_timeout: Duration::from_millis(parse_env(
                "REDIS_CONNECTION_TIMEOUT_MS",
                5000_u64,
            )?),
            redis_command_timeout: Duration::from_millis(parse_env(
                "REDIS_COMMAND_TIMEOUT_MS",
                5000_u64,
            )?),
            redis_publish_attempts: parse_env("REDIS_PUBLISH_ATTEMPTS", 5_u32)?.max(1),
            run_with_cgroups: parse_env("RUN_WITH_CGROUPS", true)?,
            run_with_quotas: parse_env("RUN_WITH_QUOTAS", true)?,
            exit_on_empty_queue: parse_env("EXIT_ON_EMPTY_QUEUE", false)?,
            compile_cache,
            memory_budget_kib: memory_budget_mib
                .checked_mul(1024)
                .ok_or_else(|| anyhow::anyhow!("memory budget is too large"))?,
            system_memory_reserve_kib: system_memory_reserve_mib
                .checked_mul(1024)
                .ok_or_else(|| anyhow::anyhow!("system memory reserve is too large"))?,
            max_request_bytes: parse_env("EVALUATOR_MAX_REQUEST_BYTES", 64_usize << 20)?,
            max_source_bytes: parse_env("EVALUATOR_MAX_SOURCE_BYTES", 1_usize << 20)?,
            max_checker_bytes: parse_env("EVALUATOR_MAX_CHECKER_BYTES", 1_usize << 20)?,
            max_testcases: parse_env("EVALUATOR_MAX_TESTCASES", 256_usize)?,
            max_testcase_bytes: parse_env("EVALUATOR_MAX_TESTCASE_BYTES", 64_usize << 20)?,
            max_output_bytes: parse_env("EVALUATOR_MAX_OUTPUT_BYTES", 1_usize << 20)?,
            job_timeout: Duration::from_secs(parse_env("EVALUATOR_JOB_TIMEOUT_SECONDS", 300_u64)?),
            health_bind: env::var("EVALUATOR_HEALTH_BIND").unwrap_or("0.0.0.0:8080".to_string()),
            system_environment,
        })
    }

    pub fn init() -> anyhow::Result<()> {
        ENVIRONMENT
            .set(Environment::new()?)
            .map_err(|_| anyhow::anyhow!("environment already initialized"))
    }

    pub fn get() -> &'static Environment {
        match ENVIRONMENT.get() {
            Some(env) => env,
            None => panic!("Environment not initialized"),
        }
    }
}

fn parse_env<T>(name: &str, default: T) -> anyhow::Result<T>
where
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
{
    match env::var(name) {
        Ok(value) => value
            .parse()
            .map_err(|err| anyhow::anyhow!("{name} has an invalid value: {err}")),
        Err(env::VarError::NotPresent) => Ok(default),
        Err(err) => Err(anyhow::anyhow!("failed to read {name}: {err}")),
    }
}

fn read_pipe_max_size() -> anyhow::Result<usize> {
    let content = fs::read_to_string("/proc/sys/fs/pipe-max-size")?;
    let pipe_max_size: usize = content.trim().parse()?;
    Ok(pipe_max_size)
}
