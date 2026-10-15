# Gemot evidence

Measurements and platform findings behind Moonstruck's proposal for Logos RFP-005, Multisig and Coordination. Gemot is a private-by-default M-of-N multisig for LEZ with an end-to-end-encrypted coordination room per multisig on the Logos chat module.

Everything here was built and run before filing, on LEZ v0.3.0 (commit db66590ab). Nothing the findings rest on changed in v0.3.1, and the public-testnet run in section 2 of the findings is on v0.3.1, the version the testnet runs.

The proposal this evidence supports is [logos-co/rfp#221](https://github.com/logos-co/rfp/issues/221), filed 2026-10-15.

**Start with [FINDINGS.md](FINDINGS.md).** It states each finding, how we measured it, and where the code, logs and receipts are.

## What is here

| Path | Contents |
| --- | --- |
| `FINDINGS.md` | The findings |
| `patches/` | Four patches against LEZ v0.3.0: the verification bench and guests, RISC Zero's accelerated forks, the private-path and admin-call tests, and the whole-transaction tests. A fifth against v0.3.1 adds the public-testnet driver |
| `lez-tests/` | The same code as plain files, for reading |
| `scripts/` | Set up two LEZ checkouts and rerun every measurement |
| `logs/` | Our runs: cycle counts, real-proof times and memory, test output, the chat capture, the public-testnet run |
| `receipts/` | A receipt from the real proof of the M of 5 verification guest |
| `chat/` | The chat-room runs and a decoder for what a non-member saw |

## Rerun it

```
scripts/setup.sh
scripts/run-vault-tests.sh
scripts/run-stock.sh
scripts/run-accelerated.sh
scripts/run-logic-tests.sh
scripts/run-real-proofs.sh
scripts/run-lez-ppe-bench.sh
scripts/setup-testnet.sh
```

Requirements and expected results are in [FINDINGS.md](FINDINGS.md#reproducing).

## License

MIT. The patches apply to LEZ, which is MIT-licensed by Vac.
