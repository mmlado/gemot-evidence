# The code, for reading

These are copies of the files the patches in `patches/` add to LEZ v0.3.0, so the code can be read without applying anything. To build and run them, use `scripts/setup.sh`, which applies the patches to fresh checkouts, and `scripts/setup-testnet.sh` for the v0.3.1 driver.

| File here | Where the patch puts it | Patch |
| --- | --- | --- |
| `multisig_bench/` | `tools/multisig_bench/` | 01 |
| `guests/test_guest_core_additions.rs` | appended to `test_guest_core/src/guests.rs` | 01 and 04 |
| `guests/multisig_verify_schnorr_bin.rs` | `test_programs/guest/src/bin/multisig_verify_schnorr.rs` | 01 |
| `rfp005_spike.rs` | `lee/state_machine/src/state/tests/rfp005_spike.rs` | 03 |
| `guests/rfp005_admin_target.rs` | `lee/state_machine/test_methods/guest/src/bin/rfp005_admin_target.rs` | 03 |
| `rfp005_whole_tx.rs` | `lee/state_machine/src/state/tests/rfp005_whole_tx.rs` | 04 |
| `guests/rfp005_multisig_execute_bin.rs` | `lee/state_machine/test_methods/guest/src/bin/rfp005_multisig_execute.rs` | 04 |
| `rfp005_testnet/` | `tools/rfp005_testnet/`, against LEZ v0.3.1 with the same guest, for the public-testnet run in FINDINGS section 2 | 05 |

The bench and `guests.rs` also carry an ed25519 variant we measured first. No claim in FINDINGS.md uses it: every script passes `--scheme schnorr`, which is BIP-340 Schnorr on secp256k1, LEZ's own scheme.
