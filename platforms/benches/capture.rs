use criterion::{Criterion, criterion_group, criterion_main};
use platforms::windows::{Capture, Handle};

// is runtime bench a bench? I wanted to try and use it anyway...
fn benchmark(c: &mut Criterion) {
    let mut capture = Capture::new(Handle::new(Some("MapleStoryClass"), None).unwrap());
    c.bench_function("capture", |b| {
        b.iter(|| std::hint::black_box(capture.grab().unwrap()))
    });
}

criterion_group!(benches, benchmark);
criterion_main!(benches);
