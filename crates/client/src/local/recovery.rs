use super::*;
use remote_codex_adapter::recovery;
use remote_codex_protocol::Request;

pub(super) async fn handle(runtime: &LocalRuntime, event: &Value) -> Option<Value> {
    let request = recovery::request(event)?;
    let result = async {
        if request.thread != runtime.binding.session.id || request.after < 0 {
            return Err(ClientError::Argument(
                "invalid recovery query for this session",
            ));
        }
        let jobs = runtime
            .remote
            .call(Request::Jobs {
                thread: Some(request.thread),
            })
            .await?;
        if let Some(id) = request.job {
            if !jobs
                .as_array()
                .is_some_and(|jobs| jobs.iter().any(|job| job["id"] == id))
            {
                return Err(ClientError::NotFound);
            }
            runtime
                .remote
                .call(Request::JobOutput {
                    id,
                    after: request.after,
                })
                .await
        } else {
            Ok(jobs)
        }
    }
    .await;
    Some(recovery::response(
        request.rpc_id,
        result.map_err(|e| e.to_string()),
    ))
}
