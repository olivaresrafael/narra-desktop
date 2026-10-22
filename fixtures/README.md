# Fixtures

| File | Scenes | Lines | Bytes | Seed | SHA-256 |
|---|---|---|---|---|---|
| `screenplay-1000.fountain` | 1000 | 19347 | 385552 | 42 | `11f5d31bc8803000633e89a86f815201bbb7ca3bd378ecac9096c1d175dcbe7e` |

Regenerate (byte-identical output is guaranteed for a given seed):

```sh
cargo run -q -p fixture-gen -- --scenes 1000 --seed 42 --out fixtures/screenplay-1000.fountain
shasum -a 256 fixtures/screenplay-1000.fountain
```

Any change to the generator changes the bytes: bump the version in the table
above and re-record the baseline in the context pack `PERFORMANCE_BASELINES.md`
(D-019). Never compare measurements across different fixture versions.
