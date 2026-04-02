use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use futures::future::join_all;
use scoped_async_macro::{async_scope, scoped_async};
use std::pin::Pin;

async fn process(item: &str) -> String {
    format!("{}-processed", item)
}

fn process_boxed(item: &str) -> Pin<Box<dyn Future<Output = String> + Send + '_>> {
    Box::pin(process(item))
}

fn make_data(n: usize) -> Vec<String> {
    (0..n).map(|i| format!("item-{i}")).collect()
}

fn bench_scoped_async(c: &mut Criterion) {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();

    let mut group = c.benchmark_group("scoped_async");
    for size in [10, 100, 1000] {
        let data = make_data(size);

        group.bench_with_input(BenchmarkId::new("join_all", size), &data, |b, data| {
            b.to_async(&rt).iter(|| async {
                let _: Vec<String> = scoped_async!({
                    let futures: Vec<_> = data.iter().map(|item| process(item)).collect();
                    join_all(futures).await
                })
                .unwrap();
            });
        });

        group.bench_with_input(
            BenchmarkId::new("stream_buffered", size),
            &data,
            |b, data| {
                use futures::stream::{self, StreamExt};
                b.to_async(&rt).iter(|| async {
                    let _: Vec<String> = scoped_async!({
                        stream::iter(data.iter().map(String::as_str))
                            .map(process_boxed)
                            .buffer_unordered(16)
                            .collect::<Vec<_>>()
                            .await
                    })
                    .unwrap();
                });
            },
        );
    }
    group.finish();
}

fn bench_async_scope(c: &mut Criterion) {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();

    let mut group = c.benchmark_group("async_scope");
    for size in [10, 100, 1000] {
        let data = make_data(size);

        group.bench_with_input(BenchmarkId::new("join_all", size), &data, |b, data| {
            b.to_async(&rt).iter(|| async {
                async_scope!(|scope| {
                    scope.spawn(async {
                        let futures: Vec<_> = data.iter().map(|item| process(item)).collect();
                        join_all(futures).await
                    });
                })
                .unwrap();
            });
        });

        group.bench_with_input(BenchmarkId::new("multi_spawn", size), &data, |b, data| {
            b.to_async(&rt).iter(|| async {
                async_scope!(|scope| {
                    for item in data.iter() {
                        scope.spawn(async { process(item).await });
                    }
                })
                .unwrap();
            });
        });
    }
    group.finish();
}

criterion_group!(benches, bench_scoped_async, bench_async_scope);
criterion_main!(benches);
