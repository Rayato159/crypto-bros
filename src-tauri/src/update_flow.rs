//! No installation can begin until the download is verified and the worker is backed up/stopped.
use std::future::Future;

pub async fn install<D, Q, S, I, DF, QF, SF>(
    download: D,
    quiesce: Q,
    stop: S,
    installer: I,
) -> Result<(), String>
where
    D: FnOnce() -> DF,
    Q: FnOnce() -> QF,
    S: FnOnce() -> SF,
    I: FnOnce(Vec<u8>) -> Result<(), String>,
    DF: Future<Output = Result<Vec<u8>, String>>,
    QF: Future<Output = Result<(), String>>,
    SF: Future<Output = Result<(), String>>,
{
    let bytes = download().await?;
    quiesce().await?;
    stop().await?;
    installer(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    #[tokio::test]
    async fn failure_at_any_stage_never_runs_later_stages() {
        for fail in 0..5 {
            let calls = Arc::new(Mutex::new(Vec::new()));
            let step = |n| {
                calls.lock().unwrap().push(n);
                if fail == n {
                    Err(format!("stage {n}"))
                } else {
                    Ok(())
                }
            };
            let result = install(
                || async {
                    step(0)?;
                    Ok(vec![7])
                },
                || async { step(1) },
                || async { step(2) },
                |bytes| {
                    assert_eq!(bytes, vec![7]);
                    step(3)
                },
            )
            .await;
            assert_eq!(result.is_err(), fail < 4);
            assert_eq!(
                *calls.lock().unwrap(),
                (0..=fail.min(3)).collect::<Vec<_>>()
            );
        }
    }
}
