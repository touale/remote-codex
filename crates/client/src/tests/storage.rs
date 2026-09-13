use crate::{
    ClientError,
    config::{ConfigInput, ConfigKey, ConfigValue},
    connection::SshEndpoint,
    store::LocalStore,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

async fn set(store: &LocalStore, server: &str, key: &str, value: &str) -> TestResult {
    let snapshot = store.config_snapshot(server).await?;
    store
        .set_config(server, key, ConfigInput::Plain(value), snapshot.revision)
        .await?;
    Ok(())
}

#[tokio::test]
async fn config_and_connection_survive_reopening_the_database() -> TestResult {
    let root = tempfile::tempdir()?;
    let store = LocalStore::open(&root.path().join("state")).await?;
    let record = store
        .save_connection(
            &SshEndpoint::parse("root@127.0.0.1", Some(43256))?,
            Some("dev"),
        )
        .await?;
    let server = record.id.clone();
    set(&store, &server, "env.no_proxy", "localhost").await?;
    set(&store, &server, "background", "false").await?;
    set(&store, &server, "env.https_proxy", "http://127.0.0.1:7890").await?;
    store.close().await;
    let reopened = LocalStore::open(&root.path().join("state")).await?;
    assert_eq!(reopened.find_connection("dev").await?.id, record.id);
    let snapshot = reopened.config_snapshot(&server).await?;
    assert_eq!(
        snapshot.effective.get(&ConfigKey::Background),
        Some(&ConfigValue::Boolean(false))
    );
    assert_eq!(
        snapshot.effective.get(&ConfigKey::parse("env.no_proxy")?),
        Some(&ConfigValue::Text("localhost".to_owned()))
    );
    assert_eq!(snapshot.revision.saved, 3);
    reopened.close().await;
    Ok(())
}

#[tokio::test]
async fn independent_process_handles_detect_conflicting_writes() -> TestResult {
    let root = tempfile::tempdir()?;
    let state = root.path().join("state");
    let (first, second) = tokio::join!(LocalStore::open(&state), LocalStore::open(&state));
    let (first, second) = (first?, second?);
    assert_eq!(
        first.installation_id().await?,
        second.installation_id().await?
    );
    let server = first
        .save_connection(&SshEndpoint::parse("example.invalid", None)?, Some("dev"))
        .await?
        .id;
    let expected = first.config_snapshot(&server).await?.revision;
    let (one, two) = tokio::join!(
        first.set_config(&server, "background", ConfigInput::Plain("false"), expected),
        second.set_config(
            &server,
            "disconnect_grace_seconds",
            ConfigInput::Plain("10"),
            expected
        )
    );
    assert_eq!(usize::from(one.is_ok()) + usize::from(two.is_ok()), 1);
    assert!(
        matches!(one, Err(ClientError::RevisionConflict))
            || matches!(two, Err(ClientError::RevisionConflict))
    );
    assert_eq!(first.config_snapshot(&server).await?.revision.saved, 1);
    first.close().await;
    second.close().await;
    Ok(())
}

#[tokio::test]
async fn server_changes_and_unset_do_not_affect_other_servers() -> TestResult {
    let root = tempfile::tempdir()?;
    let store = LocalStore::open(&root.path().join("state")).await?;
    let endpoint = SshEndpoint::parse("Dev", None)?;
    let one = store.save_connection(&endpoint, Some("one")).await?.id;
    let two = store.save_connection(&endpoint, Some("two")).await?.id;
    set(&store, &one, "background", "false").await?;
    set(&store, &one, "env.https_proxy", "http://one.example:7890").await?;
    set(&store, &one, "proxy.mode", "custom").await?;
    let untouched = store.config_snapshot(&two).await?;
    assert_eq!(untouched.revision.saved, 0);
    assert!(
        untouched
            .effective
            .get(&ConfigKey::parse("env.https_proxy")?)
            .is_none()
    );
    let current = store.config_snapshot(&one).await?;
    store
        .unset_config(&one, "background", current.revision)
        .await?;
    assert_eq!(
        store.config_snapshot(&two).await?.revision,
        untouched.revision
    );
    assert_eq!(
        store
            .config_snapshot(&one)
            .await?
            .effective
            .get(&ConfigKey::Background),
        Some(&ConfigValue::Boolean(true))
    );
    let report = store.config_report(&one, false).await?;
    assert_eq!(report.server_id, one);
    let overrides = store.config_report(&one, true).await?;
    assert_eq!(overrides.items.len(), 2);
    assert!(
        overrides
            .items
            .iter()
            .all(|item| item.setting.source == crate::config::Source::Server)
    );
    store.close().await;
    Ok(())
}

#[tokio::test]
async fn invalid_change_rolls_back_only_the_target_server() -> TestResult {
    let root = tempfile::tempdir()?;
    let store = LocalStore::open(&root.path().join("state")).await?;
    let one = store
        .save_connection(&SshEndpoint::parse("one", None)?, Some("one"))
        .await?
        .id;
    let two = store
        .save_connection(&SshEndpoint::parse("two", None)?, Some("two"))
        .await?
        .id;
    set(&store, &one, "env.https_proxy", "http://proxy.example:7890").await?;
    set(&store, &one, "proxy.mode", "custom").await?;
    let before = store.config_snapshot(&one).await?.revision;
    let other = store.config_snapshot(&two).await?.revision;
    assert!(
        store
            .unset_config(&one, "env.https_proxy", before)
            .await
            .is_err()
    );
    assert_eq!(store.config_snapshot(&one).await?.revision, before);
    assert_eq!(store.config_snapshot(&two).await?.revision, other);
    assert!(
        store
            .config_snapshot(&one)
            .await?
            .effective
            .get(&ConfigKey::parse("env.https_proxy")?)
            .is_some()
    );
    store.close().await;
    Ok(())
}

#[tokio::test]
async fn batch_config_is_atomic_and_equal_updates_keep_the_revision() -> TestResult {
    let root = tempfile::tempdir()?;
    let store = LocalStore::open(&root.path().join("state")).await?;
    let server = store
        .save_connection(&SshEndpoint::parse("example.invalid", None)?, Some("dev"))
        .await?
        .id;
    let before = store.config_snapshot(&server).await?;
    let changes = vec![
        ("background".into(), "false".into()),
        ("disconnect_grace_seconds".into(), "10".into()),
    ];
    let mut invalid = changes.clone();
    invalid.push(("proxy.mode".into(), "custom".into()));
    assert!(
        store
            .set_many_config(&server, &invalid, before.revision)
            .await
            .is_err()
    );
    let unchanged = store.config_snapshot(&server).await?;
    assert_eq!(unchanged.revision, before.revision);
    assert_eq!(unchanged.layer, before.layer);
    let revision = store
        .set_many_config(&server, &changes, before.revision)
        .await?;
    assert_eq!(revision.saved, before.revision.saved + 1);
    let saved = store.config_snapshot(&server).await?;
    assert_eq!(
        saved.effective.get(&ConfigKey::Background),
        Some(&ConfigValue::Boolean(false))
    );
    assert_eq!(
        saved.effective.get(&ConfigKey::DisconnectGraceSeconds),
        Some(&ConfigValue::Integer(10))
    );
    assert_eq!(
        store.set_many_config(&server, &changes, revision).await?,
        revision
    );
    assert_eq!(
        store
            .set_config(&server, "background", ConfigInput::Plain("false"), revision)
            .await?,
        revision
    );
    assert_eq!(
        store
            .unset_config(&server, "env.no_proxy", revision)
            .await?,
        revision
    );
    store.close().await;
    Ok(())
}

#[tokio::test]
async fn names_are_stable_and_aliases_have_independent_identities() -> TestResult {
    let root = tempfile::tempdir()?;
    let store = LocalStore::open(&root.path().join("state")).await?;
    let endpoint = SshEndpoint::parse("root@127.0.0.1", Some(43256))?;
    let one = store.save_connection(&endpoint, Some("dev")).await?;
    assert_eq!(store.save_connection(&endpoint, None).await?.id, one.id);
    assert!(matches!(
        store
            .save_connection(&SshEndpoint::parse("other", None)?, Some("dev"))
            .await,
        Err(ClientError::NameConflict)
    ));
    let two = store
        .save_connection(&endpoint, Some("other-profile"))
        .await?;
    assert_ne!(one.id, two.id);
    store.close().await;
    Ok(())
}

#[tokio::test]
async fn cli_and_app_retry_settings_are_persistent_and_server_scoped() -> TestResult {
    use crate::application::{Client, ClientOptions};
    use crate::config::VisibleValue;
    let root = tempfile::tempdir()?;
    let path = root.path().join("state");
    let store = LocalStore::open(&path).await?;
    for name in ["dev", "test"] {
        store
            .save_connection(&SshEndpoint::parse("example.invalid", None)?, Some(name))
            .await?;
    }
    let options = ClientOptions {
        data_dir: Some(path),
        ..Default::default()
    };
    let client = Client::open(options.clone()).await?;
    let key = "reconnect.max_attempts";
    // The same service backs CLI set and the App's batched server settings.
    let report = client.config().set("dev", key, "20", false, None).await?;
    let report = client
        .config()
        .set_many(
            "dev",
            &[
                (key.into(), "3".into()),
                ("proxy.mode".into(), "direct".into()),
            ],
            report.saved_revision,
        )
        .await?;
    let item = report
        .items
        .iter()
        .find(|item| item.setting.key == key)
        .ok_or("missing setting")?;
    assert_eq!(item.application_state, "next_reconnect");
    assert_eq!(item.setting.value, Some(VisibleValue::Integer(3)));
    assert_eq!(
        client.config().get("test", key).await?.items[0]
            .setting
            .value,
        Some(VisibleValue::Integer(10))
    );
    client.close().await;
    let reopened = Client::open(options).await?;
    assert_eq!(
        reopened.config().get("dev", key).await?.items[0]
            .setting
            .value,
        Some(VisibleValue::Integer(3))
    );
    reopened.config().unset("dev", key, None).await?;
    assert_eq!(
        reopened.config().get("dev", key).await?.items[0]
            .setting
            .value,
        Some(VisibleValue::Integer(10))
    );
    reopened.close().await;
    store.close().await;
    Ok(())
}
