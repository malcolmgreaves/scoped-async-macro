<div align="center">
  <!-- Crates version -->
  <a href="https://crates.io/crates/scoped-async-macro">
    <img src="https://img.shields.io/crates/v/scoped-async-macro?style=flat-square"
    alt="Crates.io version" />
  </a>
  <!-- CI -->
  <a href="https://github.com/jkelleyrtp/dioxus/actions">
    <img src="https://github.com/malcolmgreaves/scoped-async-macro/actions/workflows/ci.yml/badge.svg"
      alt="CI status" />
  </a>
</div>

# scoped-async-macro

Provides macros for using `async` expressions with non-static scoped data. Use `scoped_async!` for a single `async` block and `async_scope!` to use the scope directly.

Built on the shoulders of [`async_scoped`](https://docs.rs/async-scoped/latest/async_scoped/). These macros enforce safe use of the `TokioScope::scope_and_collect` function.


## Using `scoped_async!`
The `scoped_async!` macro evaluates an async expression that can borrow non-static data.

It creates and manages a scope that has a lifetime for the single `async`-block provided to the macro.

```rust
async fn greet(name: &str) -> String {
    format!("Hello {name}")
}

let data = vec!["Alex", "Barbra", "Chuck"];

let mut results: Vec<String> = scoped_async!({
    let futures: Vec<_> = data.iter().map(|name| greet(name)).collect();
    join_all(futures).await
})
.expect("async execution failure");

results.sort();
assert_eq!(results, vec!["Hello Alex", "Hello Barbra", "Hello Chuck"]);
```


## Using `async_scope!`
The `async_scope!` macro is similar, but it provides the actual `scope` directly to the user-provided closure.

The closure is **sync** -- it sets up spawns, and the scope drives all spawned tasks to completion. Use oneshot channels to send results out of spawned tasks, and return the receivers from the closure.

```rust
async fn greet(name: &str) -> String {
    format!("Hello {name}")
}

let data = vec!["Alex", "Barbra", "Chuck"];

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
.expect("async execution failure");

let max_len = rx_max_len.await.expect("channel closed");
let mut greetings = rx_greetings.await.expect("channel closed");

assert_eq!(max_len, 6); // "Barbra".len()
greetings.sort();
assert_eq!(greetings, vec!["Hello Alex", "Hello Barbra", "Hello Chuck"]);
```


## Development

This project uses `cargo`. If you don't have it, [download `rustup` here to get managed versions of `rustc` and `cargo`](https://rust-lang.org/tools/install/).

Use `cargo test` to run all tests and `cargo bench` to run the benchmarks.

