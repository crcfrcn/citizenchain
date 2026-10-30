pub async fn join_blocking_task<T>(
    task: &'static str,
    result: tauri::async_runtime::JoinHandle<Result<T, String>>,
) -> Result<T, String> {
    result
        .await
        .map_err(|e| format!("{task} join failed: {e}"))?
}
