use super::*;

pub async fn read(program: &Path, binding: &SessionBinding, cursor: Option<&str>) -> Result<Value> {
    if codex_home()? != Path::new(&binding.codex_home) {
        return Err(ClientError::Argument(
            "session belongs to another local CODEX_HOME",
        ));
    }
    let (engine, _) = Engine::local(program, Path::new(&binding.codex_home)).await?;
    let result = if let Some(cursor) = cursor {
        engine
            .call(
                "thread/turns/list",
                json!({"threadId":binding.session.id,"cursor":cursor}),
            )
            .await
    } else {
        engine
            .call(
                "thread/read",
                json!({"threadId":binding.session.id,"includeTurns":true}),
            )
            .await
    };
    engine.shutdown().await;
    Ok(result?)
}
