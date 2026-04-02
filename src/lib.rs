use std::fmt;

/// All `JoinError`s from spawned tasks in a [`async_scope!`] invocation.
#[derive(Debug)]
pub struct ScopedJoinErrors {
    pub errors: Vec<tokio::task::JoinError>,
}

impl fmt::Display for ScopedJoinErrors {
    // Displays all errors as "[{error}, {error}, ...]".
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("[")?;
        for (i, e) in self.errors.iter().enumerate() {
            if i > 0 {
                f.write_str(",")?;
            }
            write!(f, "{e}")?;
        }
        f.write_str("]")
    }
}

impl std::error::Error for ScopedJoinErrors {}

/// Manually interact with an [`async_scoped::TokioScope`] instance to spawn
/// futures that can borrow non-static data. All borrowed data must live for
/// the lifetime of the scope.
///
/// Returns `Result<R, ScopedJoinErrors>` where `R` is the closure's return value.
/// If any spawned tasks panicked, returns all `JoinError`s.
#[macro_export]
macro_rules! async_scope {
    (|$scope:ident| $body:expr) => {{
        // SAFETY: the `scope_and_collect` future is immediately awaited and never forgotten.
        let (result, collected) =
            unsafe { ::async_scoped::TokioScope::scope_and_collect(|$scope| $body) }.await;
        let errors: Vec<_> = collected.into_iter().filter_map(|r| r.err()).collect();
        if errors.is_empty() {
            Ok(result)
        } else {
            Err($crate::ScopedJoinErrors { errors })
        }
    }};
}

/// Evaluate an async block in a non-static scope, allowing it to borrow
/// data that is not `'static`. All borrowed data must live for the lifetime
/// of the scope.
///
/// Returns `Result<T, tokio::task::JoinError>` where `T` is the async
/// expression's output. If the spawned task panicked, it returns the
/// `JoinError`.
#[macro_export]
macro_rules! scoped_async {
    ($body:expr) => {{
        let (tx, rx) = ::tokio::sync::oneshot::channel();
        // SAFETY: the `scope_and_collect` future is immediately awaited and never forgotten.
        let (_, collected) = unsafe {
            ::async_scoped::TokioScope::scope_and_collect(|scope| {
                scope.spawn(async {
                    let result = $body;
                    let _ = tx.send(result);
                });
            })
        }
        .await;
        let errors: Vec<_> = collected.into_iter().filter_map(|r| r.err()).collect();
        if errors.is_empty() {
            Ok(rx
                .await
                .expect("scoped_async!: oneshot channel closed unexpectedly"))
        } else {
            if errors.len() > 1 {
                panic!(
                    "Expecting exactly one scope.spawn() but found {}: {errors:?}",
                    errors.len()
                )
            }
            Err(errors.into_iter().next().unwrap())
        }
    }};
}

#[cfg(test)]
mod tests {
    use futures::future::join_all;
    use futures::stream::{self, StreamExt};
    use std::pin::Pin;
    use std::time::Duration;

    // Make sure we re-create data so we have non-static scopes for borrows.
    fn test_data() -> Vec<String> {
        vec![
            "alpha".into(),
            "beta".into(),
            "gamma".into(),
            "delta".into(),
        ]
    }

    async fn process(item: &str) -> String {
        tokio::time::sleep(Duration::from_millis(50)).await;
        format!("{}-processed", item)
    }

    fn process_boxed(item: &str) -> Pin<Box<dyn Future<Output = String> + Send + '_>> {
        Box::pin(process(item))
    }

    fn assert_processed(mut actual: Vec<String>) {
        actual.sort();
        assert_eq!(
            actual,
            vec![
                "alpha-processed",
                "beta-processed",
                "delta-processed",
                "gamma-processed",
            ]
        );
    }

    /// Example use of scoped_async! with join_all.
    #[tokio::test]
    async fn test_scoped_async_join_all() {
        let data = test_data();

        let processed: Vec<String> = scoped_async!({
            let futures: Vec<_> = data.iter().map(|item| process(item)).collect();
            join_all(futures).await
        })
        .unwrap();

        assert_processed(processed);
    }

    /// Same as above but uses `stream::iter` with `buffer_unordered`.
    #[tokio::test]
    async fn test_scoped_async_stream() {
        let data = test_data();

        let processed: Vec<String> = scoped_async!({
            stream::iter(data.iter().map(String::as_str))
                .map(process_boxed)
                .buffer_unordered(4)
                .collect::<Vec<_>>()
                .await
        })
        .unwrap();

        assert_processed(processed);
    }

    // README examples

    async fn greet(name: &str) -> String {
        format!("Hello {name}")
    }

    /// README example for scoped_async!
    #[tokio::test]
    async fn test_readme_scoped_async() {
        let data = ["Alex", "Barbra", "Chuck"];

        let mut results: Vec<String> = scoped_async!({
            let futures: Vec<_> = data.iter().map(|name| greet(name)).collect();
            join_all(futures).await
        })
        .unwrap();

        results.sort();
        assert_eq!(results, vec!["Hello Alex", "Hello Barbra", "Hello Chuck"]);
    }

    /// README example for async_scope!
    #[tokio::test]
    async fn test_readme_async_scope() {
        let data = ["Alex", "Barbra", "Chuck"];

        let (rx_max_len, rx_greetings) = async_scope!(|scope| {
            let (tx_ml, rx_ml) = tokio::sync::oneshot::channel();
            scope.spawn(async {
                let max_len = data.iter().map(|x| x.len()).max().unwrap();
                let _ = tx_ml.send(max_len);
            });

            let (tx_g, rx_g) = tokio::sync::oneshot::channel();
            scope.spawn(async {
                let futures: Vec<_> = data.iter().map(|name| greet(name)).collect();
                let results = join_all(futures).await;
                let _ = tx_g.send(results);
            });

            (rx_ml, rx_g)
        })
        .unwrap();

        let max_len = rx_max_len.await.expect("channel closed");
        let mut greetings = rx_greetings.await.expect("channel closed");

        assert_eq!(max_len, 6); // "Barbra".len()
        greetings.sort();
        assert_eq!(greetings, vec!["Hello Alex", "Hello Barbra", "Hello Chuck"]);
    }

    /// Example use of async_scope! for manual scope interaction.
    #[tokio::test]
    async fn test_async_scope_join_all() {
        let data = test_data();

        async_scope!(|scope| {
            scope.spawn(async {
                let futures: Vec<_> = data.iter().map(|item| process(item)).collect();
                let processed = join_all(futures).await;
                assert_processed(processed);
            });
        })
        .unwrap();
    }

    /// Same as above but uses `stream::iter` with `buffer_unordered`.
    #[tokio::test]
    async fn test_async_scope_stream() {
        let data = test_data();

        async_scope!(|scope| {
            scope.spawn(async {
                let processed: Vec<String> = stream::iter(data.iter().map(String::as_str))
                    .map(process_boxed)
                    .buffer_unordered(4)
                    .collect::<Vec<_>>()
                    .await;
                assert_processed(processed);
            });
        })
        .unwrap();
    }
}
