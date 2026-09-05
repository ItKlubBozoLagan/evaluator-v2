use crate::environment::Environment;
use redis::aio::{ConnectionManager, ConnectionManagerConfig};
use redis::{Client, TlsCertificates};

pub fn build_client() -> anyhow::Result<Client> {
    let environment = Environment::get();

    if environment.redis_url.starts_with("rediss://") {
        let ca_file = environment
            .redis_ca_file
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("REDIS_CA_FILE is required for rediss://"))?;
        let root_cert = std::fs::read(ca_file).map_err(|err| {
            anyhow::anyhow!("failed to read Redis CA file {}: {err}", ca_file.display())
        })?;
        if root_cert.is_empty() {
            anyhow::bail!("Redis CA file {} is empty", ca_file.display());
        }

        return Client::build_with_tls(
            environment.redis_url.as_str(),
            TlsCertificates {
                client_tls: None,
                root_cert: Some(root_cert),
            },
        )
        .map_err(|err| anyhow::anyhow!("failed to configure Redis TLS: {err}"));
    }

    if environment.redis_require_tls {
        anyhow::bail!("REDIS_URL must use rediss:// when REDIS_REQUIRE_TLS=true");
    }

    Client::open(environment.redis_url.as_str())
        .map_err(|err| anyhow::anyhow!("invalid REDIS_URL: {err}"))
}

pub async fn connect(client: &Client) -> anyhow::Result<ConnectionManager> {
    let environment = Environment::get();
    let config = ConnectionManagerConfig::new()
        .set_factor(100)
        .set_exponent_base(2)
        .set_max_delay(5000)
        .set_number_of_retries(5)
        .set_connection_timeout(environment.redis_connection_timeout)
        .set_response_timeout(environment.redis_command_timeout);

    let mut connection = client
        .get_connection_manager_with_config(config)
        .await
        .map_err(|err| anyhow::anyhow!("failed to connect to Redis: {err}"))?;
    let pong: String = redis::cmd("PING")
        .query_async(&mut connection)
        .await
        .map_err(|err| anyhow::anyhow!("Redis startup PING failed: {err}"))?;
    if pong != "PONG" {
        anyhow::bail!("Redis startup PING returned an unexpected response");
    }

    Ok(connection)
}
