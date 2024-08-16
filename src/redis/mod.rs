use redis::aio::ConnectionManager;

pub async fn get_connection<T: redis::IntoConnectionInfo>(info: T) -> redis::RedisResult<ConnectionManager> {
    let client = redis::Client::open(info)?;

    Ok(ConnectionManager::new(client).await?)
}