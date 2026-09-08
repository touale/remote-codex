use sqlx::{Row, SqliteConnection};

use crate::{
    ClientError, Result,
    config::{ConfigInput, ConfigKey, ConfigLayer, ConfigValue, SecretRef, VisibleValue},
};

pub(super) async fn load(connection: &mut SqliteConnection, server: &str) -> Result<ConfigLayer> {
    let rows = sqlx::query(
        "SELECT key,spelling,representation,value FROM settings WHERE server=? ORDER BY key",
    )
    .bind(server)
    .fetch_all(connection)
    .await?;
    let mut layer = ConfigLayer::new();
    for row in rows {
        let key: String = row.try_get("key")?;
        let spelling: String = row.try_get("spelling")?;
        let value: String = row.try_get("value")?;
        if ConfigKey::parse(&spelling)?.name() != key {
            return Err(ClientError::Schema);
        }
        match row.try_get::<&str, _>("representation")? {
            "plain" => layer.set(&spelling, ConfigInput::Plain(&value))?,
            "secret" => layer.restore_secret(&spelling, SecretRef::new(&value)?)?,
            _ => return Err(ClientError::Schema),
        }
    }
    Ok(layer)
}

pub(super) async fn save(
    connection: &mut SqliteConnection,
    server: &str,
    layer: &ConfigLayer,
) -> Result<()> {
    sqlx::query("DELETE FROM settings WHERE server=?")
        .bind(server)
        .execute(&mut *connection)
        .await?;
    for (key, spelling, value) in layer.entries() {
        let (representation, raw) = match value {
            ConfigValue::Secret(reference) => {
                let count = sqlx::query("UPDATE credentials SET state='active' WHERE id=?")
                    .bind(reference.id())
                    .execute(&mut *connection)
                    .await?
                    .rows_affected();
                if count != 1 {
                    return Err(ClientError::Credentials);
                }
                ("secret", reference.id().to_owned())
            }
            value => ("plain", plain(value)?),
        };
        sqlx::query(
            "INSERT INTO settings(server,key,spelling,representation,value) VALUES (?,?,?,?,?)",
        )
        .bind(server)
        .bind(key.name())
        .bind(spelling)
        .bind(representation)
        .bind(raw)
        .execute(&mut *connection)
        .await?;
    }
    sqlx::query("UPDATE credentials SET state='retired' WHERE state='active' AND NOT EXISTS (SELECT 1 FROM retained_legacy_credentials r WHERE r.id=credentials.id) AND NOT EXISTS (SELECT 1 FROM settings WHERE representation='secret' AND value=credentials.id)")
        .execute(connection).await?;
    Ok(())
}

pub(super) fn plain(value: &ConfigValue) -> Result<String> {
    match value.visible() {
        Some(VisibleValue::Text(value)) => Ok(value),
        Some(value) => Ok(serde_json::to_string(&value)?.trim_matches('"').to_owned()),
        None => Err(ClientError::Credentials),
    }
}
