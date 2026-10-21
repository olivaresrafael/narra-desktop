# Fixtures

| File | Scenes | Lines | Bytes | Seed | SHA-256 |
|---|---|---|---|---|---|
| `screenplay-1000.fountain` | 1000 | 18549 | 384754 | 42 | `f70a97b5c4abaab2ecec2febbc8822184c1426cba581af5ec10e3436824f187c` |

Regenerate (byte-identical output is guaranteed for a given seed):

```sh
cargo run -q -p fixture-gen -- --scenes 1000 --seed 42 --out fixtures/screenplay-1000.fountain
shasum -a 256 fixtures/screenplay-1000.fountain
```

Any change to the generator changes the bytes: bump the version in the table
above and re-record the baseline in the context pack `PERFORMANCE_BASELINES.md`
(D-019). Never compare measurements across different fixture versions.
