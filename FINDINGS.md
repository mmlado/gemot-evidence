# Gemot: measurements and platform findings on LEZ

Evidence for Moonstruck's RFP-005 proposal. Everything here was built and run before filing, against LEZ v0.3.0, the release testnet 0.3 runs. Each finding gives what we ran, what came out, and where the code, logs and receipts are in this repository.

## Setup

| Item | Value |
| --- | --- |
| LEZ | v0.3.0, commit db66590ab. Nothing the findings rest on changed in v0.3.1 |
| RISC Zero | risc0-zkvm and r0vm 3.0.5, guests compiled with RISC Zero's Rust toolchain 1.91.1 and cargo-risczero 3.0.5 |
| Machine | AMD Ryzen 7 5700X3D, 8 cores and 16 hardware threads, 62 GB RAM, Fedora with Linux 7.2. Its AMD Radeon RX 6600 GPU is unused, since RISC Zero accelerates proving only on NVIDIA and Apple GPUs |
| Proving | Real proving, RISC0_DEV_MODE unset, release builds, 16 threads unless a row says 8 |
| Privacy circuit | LEZ's prebuilt artifacts/lee/privacy_preserving_circuit/privacy_preserving_circuit.bin, unchanged |
| Program crypto | Our guests use RISC Zero's accelerated forks of k256 (v0.13.4-risczero.1), sha2 (v0.10.9-risczero.0) and crypto-bigint (v0.5.5-risczero.0). The forks change our program's build only, not LEZ |
| Chat | Chat module 0.3.0 and delivery module 0.3.0 through logosctl 0.3.2, on the logos.test fleet |

Rows marked dev mode test logic only: the sequencer's checks are the same, but the proof is a stub. Every time and size in this document comes from a real proof.

## 1. Approval verification cost

Approvals are BIP-340 Schnorr signatures on secp256k1, LEZ's own scheme, checked with the same k256 `verify_prehash` call the sequencer uses. The guest verifies M signatures inside a two-phase LEZ program under a session limit of 10,000,000 user cycles, the cap on a charged public transaction. On LEZ v0.3.0 and v0.3.1 every user public transaction pays a fee and declares a `gas_limit`, settlement runs it at that limit, and the fee market caps the limit at `MAX_GAS_EXEC`, 10,000,000 cycles per transaction and per block (`lez/chain_state/src/classify.rs`, `lez/programs/fee/core/src/market.rs`, `lez/chain_state/src/apply.rs`). The 32M `DEFAULT_PUBLIC_CYCLE_BUDGET` in `lee/state_machine/src/program/mod.rs` applies only to paths that carry no transaction budget.

| M | Crates as LEZ ships them | Accelerated forks |
| --- | --- | --- |
| 1 signature | 5.68M cycles, 57 percent of the cap | 0.29M cycles, 3 percent |
| 2 | 11.4M, over the cap | 0.60M, 6 percent |
| 5 | 28.4M, over the cap | 1.5M, 15 percent |
| 7, a queue at M of 5 | 39.8M, over the cap | 2.05M, 21 percent |
| 34 | | 9.9M, passes |
| 35 | | fails |

Until 2026-10-16 this section, and the proposal as filed, took the 32M default for the cap and reported a ceiling of 114 signatures, 118 with Rust 1.97.0, with M of 5 at 4 percent. Those runs are in `logs/spike1c-exec-schnorr-accelerated-ceiling.json` and `logs/spike10-*.log`, and their counts stand. Only the cap was wrong. The ceiling against the real cap was measured on 2026-10-16: `logs/spike11-cap10m-accel.log`, `logs/spike11-cap10m-stock.log`.

**With a newer toolchain.** Everything above was compiled with RISC Zero's Rust 1.91.1. Cycle counts are exact for a given guest toolchain, not across toolchains. An independent run of the reproduction package on another machine, with Rust 1.97.0, cargo-risczero 3.0.6 and r0vm 3.0.6, gave about 3 percent fewer cycles, and our rerun with the same versions matched it:

| Count | Rust 1.91.1, our machine | Rust 1.97.0, our machine | Rust 1.97.0, another machine |
| --- | --- | --- | --- |
| Stock, M of 1 | 5,693,770 | 5,521,275 | 5,521,275 |
| Stock, M of 5 | 28,427,816 | 27,565,601 | 27,565,601 |
| Stock, M of 2 under the cap | 11,377,231, over it | 11,032,306, over it | not rerun |
| Accelerated, M of 1 | 305,643 | 296,049 | 295,986 |
| Accelerated, M of 5 | 1,473,542 | 1,425,650 | 1,425,583 |
| Accelerated ceiling under the cap | 34 at 9,909,145, and 35 fails | 35 at 9,864,760, and 36 fails | not rerun |
| Real proof of the accelerated M of 5 guest | 2,097,152 total cycles | | 1,572,864 total cycles, 231.6 s on 16 threads and 283.4 s on 8, 9.2 GiB |

The stock counts are identical across the two machines. The accelerated counts differ by 60 to 170 cycles in 0.3M to 32M, most likely because each build embeds its own file paths, which shifts memory alignment slightly. The newer toolchain only helps: about 3 percent fewer cycles, and the accelerated ceiling rises from 34 to 35 signatures. Files: `logs/spike10-rust197-ours.log`, `logs/spike10-rust197-independent-stock.log`, `logs/spike10-rust197-independent-accelerated.log`, `logs/spike11-cap10m-accel.log`, `logs/spike11-cap10m-stock.log`.

The accelerated figure reproduces fryorcraken/lez-signature-bench, about 0.3M per verification on LEZ v0.2.0-rc3, on the v0.3.0 two-phase model. A queue checks M plus two signatures, the approvals plus the proposer's and the executor's, so at M of 5 it checks seven: about 2.05M cycles with the forks, 21 percent of the cap, and 39.8M without, four times over it. The private path has no cycle cap, so under the private posture the limit is proving time.

The accelerated M of 5 verification guest alone, 2.1M total cycles in 2 segments, proves in 215.8 s on 16 threads and 288.8 s on 8, with a peak of 9,609,728 KiB either way. A one-byte tamper makes the guest panic on "approval signature verifies".

Files: `lez-tests/multisig_bench/`, `patches/01-bench.patch`, `patches/02-accelerated-forks.patch`, `scripts/run-stock.sh`, `scripts/run-accelerated.sh`, `logs/spike1c-*.json`, `logs/spike1c-prove-memory.log`, `receipts/spike1c-receipt-schnorr-m5-accelerated.json`, `logs/spike11-cap10m-*.json`.

## 2. Real private transactions

Each row is one private transaction proved for real and then settled by LEZ's state machine. The multisig rows run our test guest, which verifies the signatures, writes the multisig's private state account, sets a timestamp validity window, and moves funds out of a private vault PDA bound to the program into a new private recipient. Peak memory was between 9,599,424 and 9,617,612 KiB, about 9.2 GiB, in every run.

| Transaction | Private notes | Prove time | Proof | Whole transaction |
| --- | --- | --- | --- | --- |
| Native two-account transfer, no program of ours, notes at natural length | 2 | 387.0 s (6.5 min) | 225,883 B | 228,635 B |
| The same, notes padded to 512 bytes as the wallet sends them | 2 | 406.3 s (6.8 min) | 226,569 B | 230,007 B |
| Admin call: a private authority PDA writes a public program's state | 1, and 1 public action | 297.2 s (5.0 min) | 224,743 B | 226,354 B |
| Queue and execute at M of 1, three signatures | 3 | 724.4 s (12.1 min) | 228,355 B | 233,579 B |
| The same at M of 3, five signatures | 3 | 877.2 s (14.6 min) | 228,355 B | 233,579 B |
| The same at M of 5, seven signatures | 3 | 933.5 s (15.6 min) | 228,355 B | 233,579 B |
| The same at M of 5 on 8 threads | 3 | 1,280.7 s (21.3 min) | 228,355 B | 233,579 B |
| The same at M of 5 as the wallet sends it: four dummy notes to make seven, the program behind an immutable header and run undisclosed | 7 | 937.0 s (15.6 min) | 235,143 B | 247,155 B |
| The same at M of 10, twelve signatures | 3 | 1,173.0 s (19.5 min) | 228,355 B | 233,579 B |
| A queue at M of 5 that consumes five recorded approvals and the recorded proposal, with two signatures | 9 | 1,810.3 s (30.2 min) | 238,553 B | 253,975 B |

Every multisig transaction settled inside its timestamp window and was rejected outside it with OutOfValidityWindow.

What the curve says:

- The fixed cost of a private transaction with a program dominates. Nine more signatures, from M of 1 to M of 10, add 7.4 minutes, about 0.8 each.
- Dummy notes and running the program undisclosed cost nothing measurable.
- Each private account a transaction consumes and writes costs about three minutes. A queue whose approvals arrive as signatures from the room costs about half of one that consumes them as recorded accounts.
- A member's machine needs about 10 GB of free memory. Peak memory follows RISC Zero's segment size, 2^20 cycles, not the thread count.

**On the public testnet.** On 2026-10-14 and 15 the wallet-shaped M of 5 row was repeated against the public testnet, https://testnet.lez.logos.co, sequencer v0.3.1, after its reset of 2026-10-12. Four steps, each a transaction the sequencer accepted and a block that held:

| Step | Block | Transaction |
| --- | --- | --- |
| 100 LGO from testnet genesis account A, whose key LEZ publishes in `lez/testnet_initial_state`, to a fresh funder account, a public transfer | 11339 | `8d598c4bce7b6349f834720062b3a2868a06a0bf94062a9e456fc16fa59919a5` |
| The multisig guest of this section deployed behind an immutable header at `FyAmXsAJSYL7fB9ALgSYrmPREWZd5eB7FemQhcdwVEoQ`, four segments then the header, five public transactions paid by the funder | 11344 to 11400 | in `logs/testnet-02-deploy.log` |
| 1 LGO from the funder into the private vault PDA `CDjsB56rqU6ydtkBUzmdmGWyMYWJSPUmHZ86q91egMXz`, a private transaction with six dummy notes | 11482 | `f990cd6f7ca2a345d49c5baec4aaffaba5e64167eb0389178ff6602b718cfb05` |
| The M of 5 execution: seven signatures, the program undisclosed behind the header, seven notes, 0.3 LGO from the vault to a new private account | 14439 | `5eef1979c2058a56b9d3f075d70cea79b5dc84d0780089682c0e8b8ca6d9de9c` |

The execution proved in 922.9 s against 937.0 s locally, with the same 235,143 B proof and 247,155 B transaction, at a peak of 9,615,516 KiB. The funding transaction proved in 254.4 s at 235,216 B and 247,412 B. After each, the sequencer's commitment set held the new account's commitment. The funder's public balance fell by the deploy fees, 2,891,104 units, and then by exactly the 1 LGO it sent, and the private transactions paid no fee. The validity window was set from the head block's timestamp, one minute back and three hours forward. A first funding attempt was dropped because the funder's public nonce moved while its proof ran: the testnet re-included two deploy transactions in later blocks, 11386 to 11387 and 11392 to 11400, so the driver reads public nonces after proving and signs then. A testnet reset wipes all of this, so the logs and the two result files are the durable record. A fresh checkout from `scripts/setup-testnet.sh` rebuilds the guest byte for byte, so its image id matches the header on chain. Files: `lez-tests/rfp005_testnet/`, `patches/05-testnet-driver.patch`, `scripts/setup-testnet.sh`, `logs/testnet-*.log`, `logs/testnet-fund.json`, `logs/testnet-execute.json`.

**Against LEZ's own benchmarks.** LEZ publishes 61.5 s on an Apple M2 Pro CPU for its cycle bench's privacy-circuit transfer at commit 2811d0601 (`docs/benchmarks/cycle_bench.md`), and about 127 s per private-to-private step on the same machine in its integration bench on an earlier release (`docs/benchmarks/integration_bench.md`). On our machine LEZ's cycle bench takes 54.3 s for that case at 2811d0601, so this CPU proves at least as fast as the M2 Pro. At v0.3.0 the same bench takes 24.2 s for a native transfer and 67.4 s for a token-program transfer, at a peak of 2.4 GB. Those cases run public accounts through the privacy circuit. A transfer between private accounts also proves each account's place in the commitment tree, emits nullifiers and encrypts each note inside the proof. That work is the difference between the bench's 24.2 s and our two-account private transfer's 387 s and 9.2 GiB, on the same CPU and release. At v0.3.0 the bench's executor table panics on its token Transfer case before the privacy-circuit cases run, a bug LEZ fixed in v0.3.1 (commit 37f971055), so the script skips that table. Files: `scripts/run-lez-ppe-bench.sh`, `logs/spike9-lez-ppe-bench-0901.log`, `logs/spike9-lez-ppe-bench-v030.log`.

Files: `lez-tests/rfp005_whole_tx.rs`, `lez-tests/rfp005_spike.rs`, `lez-tests/guests/`, `patches/03-private-path-tests.patch`, `patches/04-whole-transaction-tests.patch`, `scripts/run-real-proofs.sh`, `logs/spike5-*.log`, `logs/spike6-*.log`, `logs/spike7-*.log`.

## 3. Size and block limits

A transaction may be at most the sequencer's `max_block_size` less 2,048 bytes (`lez/sequencer/core/src/config.rs`). Every config LEZ ships sets 1 MiB, 20 user transactions and one block every 10 seconds. The publishable ceiling is 7/8 of 2 MiB, 1,835,008 bytes. Every private transaction above is between 226 and 254 KB, so four fit a 1 MiB block and a zone on the shipped configuration settles at most four private transactions every 10 seconds, seven at the publishable ceiling.

## 4. Rules of the private path

| Finding | How we know | Mode |
| --- | --- | --- |
| A plan sees each account's id, authorization bit and owner, never its data, and a public account's apply is deferred to settlement. So a private proof cannot read the clock | `lee/state_machine/core/src/program/mod.rs` (`PlanInput`, `AccountMeta`) | source |
| Time bounds are block and timestamp validity windows a program sets, carried in the public message and enforced by the sequencer | LEZ's `validity_window_works_in_privacy_preserving_transactions` and `timestamp_validity_window_works_in_privacy_preserving_transactions`, 26 cases pass. Our multisig transactions above, real proofs | dev mode, then real |
| A private account is only ever initialized or updated, and both emit a nullifier. There is no read-only access | `lee/state_machine/core/src/circuit_io.rs` (`NullifierWitness`) | source |
| Two transactions that update one private account from the same state conflict: the second fails with "Nullifier already seen". Two that touch different accounts both settle, since any historical commitment root is accepted | `rfp005_two_updates_of_one_private_account_from_one_state_conflict`, `rfp005_updates_of_two_private_accounts_built_on_one_state_both_settle` | dev mode |
| A private account initializes only once, because the initialization nullifier hashes the account id alone | `rfp005_a_private_account_initializes_only_once`, `lee/state_machine/core/src/nullifier.rs` | dev mode |
| An all-private transaction carries no signature, public key or fee payer. Private transactions are fee-exempt under v0.3.0's interim policy | the transactions above, `lez/chain_state/src/classify.rs` | real |
| The public part is: public actions, nonces and signatures for public signers only, and per private account a nullifier, root, commitment, 1,088-byte ML-KEM ephemeral key, ciphertext and one-byte view tag, plus the validity windows, the program image claims and the proof | `privacy_preserving_transaction/message.rs`, `witness_set.rs`, spike dump | real |
| A program behind an immutable header runs with an Undisclosed claim, a commitment root only. A mutable header discloses the program | LEZ's `private_claim_matching_a_real_commitment_passes_verification`, our undisclosed runs above | dev mode, then real |
| An immutable header cannot be updated | LEZ's `update_header_rejects_an_immutable_header` and three more loader tests | dev mode |
| The wallet pads every note to 512 bytes, leaving 363 for account data, and every transaction to seven private accounts with dummy notes, and warns when either is exceeded | `lez/wallet/src/account_manager.rs` | source |

What the design takes from this: recording an approval leaves the multisig's state account alone so recordings never collide, the state account holds a hash of the member records so it fits the 363-byte note, execute and cancel share one closing record per digest that can initialize only once, and expiry and the time lock use validity windows rounded to the hour.

## 5. A private multisig acting on a public program

Test guest `rfp005_admin_target` stands for a program with public state and an admin: it checks that the authority account is authorized, records it on the first write and requires the same one afterwards. LEZ's `selective_pda_delegator` stands for the multisig and delegates its private PDA by seed.

| Step | Result |
| --- | --- |
| A private PDA, created in the same transaction, authorizes the first write | settles, the public state holds the PDA's id |
| The same PDA, now an existing account, authorizes a second write | settles |
| A PDA of the same program under another seed, so another multisig, tries | rejected at settlement by the stored-authority check |
| Real proof of the first write | 5.0 minutes, 226,354 bytes |

The public part names the target program and the public account, and shows whatever the target stores, including the authority account's id when the target records it as its admin.

Files: `lez-tests/guests/rfp005_admin_target.rs`, the tests `rfp005_private_pda_authorizes_a_public_admin_write` and `rfp005_admin_write_real` in `lez-tests/rfp005_spike.rs`, `logs/spike7-admin-write.log`.

## 6. The vault and the member group

LEZ's 26 private PDA circuit tests pass on v0.3.0 (`lee/state_machine/src/state/tests/circuit.rs`). A private PDA is bound to its program and seed, a key holder listing it as a signer does not authorize it, a wrong delegated seed fails, and a PDA delegated to one callee is not authorized in a sibling call.

The LEZ wallet's group key holder (`lee/key_protocol/src/key_management/group_key_holder.rs`) is one 32-byte group master secret, shared off-chain sealed under each member's ML-KEM-768 key with AES-256-GCM. Every group account's keys derive from the secret, the program and the seed. There is no rotation: a new member set means a new secret, new keys and new addresses. A removed member keeps the old secret and can read the old accounts, but cannot move a vault bound to the program. The 18 group key holder tests pass.

## 7. The chat module as a room

**Measured on 9 October.** Three sessions on logos.test: a member lists a new group 66 to 77 seconds after being added, a message arrives in 3 seconds, the module has no remove call, and a member left out of a replacement group keeps reading the old one. Script `chat/spike4.sh`, log `logs/spike4-run.log`.

**A non-member watching a room, 12 October.** Three members and a fourth node running only the delivery module. That node computed the room's content topic from the group id, `/logos-chat/1/` plus the 6-byte Blake2b of `delivery_addr|` and the id, plus `/proto`, and recorded what arrived.

| What the non-member saw | What it reveals |
| --- | --- |
| The group id in the clear on all 12 messages, as the envelope's first field | All of one room's traffic links together |
| The MLS epoch in the clear: 0, then 1 after the second member joined, then 2 after the third | How many membership changes the room has had |
| A burst for each member added, its largest message 4,522 bytes at two members and 5,919 at three | Roughly how large the group is |
| Messages of 2, 48 and 640 characters arriving as 348, 529 and 1,257 bytes | The length of unpadded discussion |
| Nothing on an inbox topic computed from a member's published address while that member was invited elsewhere | The inbox is keyed by the installation's signer id, which group members see and outsiders do not |

Message contents, votes and membership changes are encrypted: the chat library runs GroupV2 on de-mls, which sends everything as MLS application messages, and openmls's default wire format encrypts handshake messages too. The group id is 5 random bytes. The source for these is logos-messaging/logos-chat at f26b5765 and vacp2p/de-mls at 4d85a02, the revisions chat module 0.3.0 pins.

**The key registry.** Chat module 0.3.0 hardcodes an HTTP registry at devnet.chat-kc.logos.co. Each installation publishes its key package and account log there, and a client fetches a sender's account log to validate its signature, without caching. Its operator therefore sees which network addresses publish and look up which chat accounts and when. That outlines a room's membership by IP and its activity times, though not the multisig.

**Ahead of the module.** The chat library at the pinned revision already supports removing a member and keeping an installation across restarts. Module 0.3.0 exposes neither.

Files: `chat/spike4.sh`, `chat/spike8.md`, `chat/decode-capture.py`, `logs/spike4-run.log`, `logs/spike8-observer-events.jsonl`.

## 8. Platform facts the design rests on

| Fact | Where in LEZ v0.3.0 | What depends on it |
| --- | --- | --- |
| Execution is two-phase: plan sees account metadata and emits effects and chained calls, apply runs once per account shard | `lee/state_machine/core/src/program/mod.rs` | The program's structure and the SPEL port |
| Every user public transaction is fee-charged and runs at its declared `gas_limit`, capped at `MAX_GAS_EXEC`, 10,000,000 cycles per transaction and per block. The 32M `DEFAULT_PUBLIC_CYCLE_BUDGET` applies only to paths without a transaction budget | `lez/chain_state/src/classify.rs`, `lez/programs/fee/core/src/market.rs`, `lez/chain_state/src/apply.rs`, `lee/state_machine/src/program/mod.rs` | The verification ceiling |
| The private path proves the program with no cycle cap | `lee/state_machine/src/privacy_preserving_transaction/circuit/mod.rs` | Under the private posture the limit is proving time |
| LEZ's signatures are BIP-340 Schnorr on secp256k1, checked with k256 `verify_prehash` | `lee/state_machine/src/signature/mod.rs` | The approval scheme |
| An account holds at most 100 KiB of data | `lee/state_machine/core/src/account/data.rs` | Proposals recorded on chain fit |
| A program header can be immutable, and a mutable one can be replaced by its key holder | `lez/programs/program_loader/core/src/lib.rs` | Immutable deployment |
| The chat module has no remove call, starts a new installation on every init, keeps older groups as read-only history, reads back the newest 500 messages after a restart, and a member sees only messages from after they joined | `rust-lib/chat_module.lidl`, chat module interface commit a83ed21a3 | Reconciliation, re-invite and re-posting |

On LEZ dev at 7e3f7f7, the 0.4 line, the two-phase row gives way to the actor model and the private execution is metered at the 32M default, while the cap, the fee classification and the signature scheme hold, see section 9.

Between v0.3.0 and v0.3.1 nothing changed in the state machine core, the private transaction code, the signatures, the circuit artifact, the fee classification, the block settings or the program loader, and the bumped logos-blockchain pin keeps the 2 MiB block.

## 9. The execute on the 0.4 actor model

Added 2026-10-26. Testnet 0.4 runs LEZ's actor model, merged into the `dev` branch on 2026-10-14 (PR #935) and untagged as of this date. We rewrote the whole execute of section 2 as one actor against `dev` commit 7e3f7f7 (2026-10-24) and measured it on both postures, on the same machine, with RISC Zero's Rust 1.97.0, cargo-risczero 3.0.6 and r0vm 3.0.6, risc0-zkvm 3.0.5 as the dev workspace pins it, and the same accelerated forks as before, k256 v0.13.4-risczero.1, sha2 v0.10.9-risczero.0 and crypto-bigint v0.5.5-risczero.0.

What changes in the model. A program is one handler, `handle_message(input, message) -> Response`, that runs at one actor, an account and program pair, and sees only that actor's own state slot, the sender, and whether the receiving account authorized the message. There are no account lists, claims or program owner. Other actors are reached by messages: synchronous ones run in the same transaction, depth first, asynchronous ones are delivered later. A PDA's authority is granted by its program attaching the seed to a synchronous message, and the native token checks that grant before it debits (`lee/state_machine/core/src/program/mod.rs`, `lee/state_machine/core/src/native_token.rs`).

The guest (`lez-tests/guests/rfp005_multisig_actor.rs`). The multisig actor keeps the member keys, the threshold and the executed digests in its own state. `Init` writes the configuration once, at top level. `QueueAndExecute` decodes the stored configuration, checks that the proposer, each approver and the executor are members and that their BIP-340 signatures over the digest verify with k256 `verify_prehash`, refuses a duplicate approver and a count below the threshold, records the digest, sets the timestamp window, and sends `Transfer` to the vault's native-balance actor with the vault's seed attached. The vault is a PDA of the program, public or private. What section 2's guest did across a plan, its effects and a chained call, this one does in one handler, with the configuration readable in the step that spends.

### Public posture, under the 10M cap

Executed through the state machine with the metering the sequencer bills (`ValidatedStateDiff::from_public_transaction_with_cycle_budget`). On dev the charged gas is the cycle count, clamped to the transaction's declared `gas_limit`, and the fee market still caps the limit at 10,000,000 per transaction and per block (`lez/chain_state/src/apply.rs`, `lez/programs/fee/core/src/market.rs`). The wallet's default `gas_limit` is 2,000,000 (`lez/wallet/src/lib.rs`), so a client declares a higher limit for M above 5.

| M | Signatures checked | Cycles | Share of the cap |
| --- | --- | --- | --- |
| 1 | 3 | 863,472 | 8.6 percent |
| 5 | 7 | 1,990,954 | 19.9 percent |
| 10 | 12 | 3,418,526 | 34.2 percent |
| 20 | 22 | 6,235,343 | 62.4 percent |
| 30 | 32 | 9,097,768 | 91.0 percent |
| 33 | 35 | 9,926,290 | 99.3 percent, the ceiling |
| 34 | 36 | | fails with `OutOfGas { budget: 10000000 }` |

About 283,350 cycles per signature with the membership check, against 282,400 per signature in section 1's verification guest at the same Rust 1.97.0, so the model change moves the cost of the thing the RFP asks about by a third of a percent. Everything else in the execute, decoding the configuration, the digest record and the transfer message, costs about 7,400 cycles. The ceiling of 35 signatures matches section 1's 35 at the same toolchain.

Refusals, each from the state machine: four approvals for a threshold of five, `ProgramExecutionFailed("Guest panicked: below threshold")`; the execute at a timestamp outside its window, `OutOfValidityWindow`; a transfer sent straight to the vault's native-balance actor without the program's seed, `NativeTransferFailed(UnauthorizedSender)`. The last is the custody boundary: the vault has no key, and its balance moves only on a message this program sends with the seed.

### Private posture, real proofs

The configuration and the vault are private PDAs bound to the program under the member group's keys, and the execute is the root call to the configuration's actor. A private PDA at the root runs unauthorized, as LEZ's own tests have it, and the approvals are the authority. The payout goes to a new private account, as in section 2, or to a public account. On dev the host-side simulation meters the private execution at the 32M default (`handle_message_metered` in `lee/state_machine/src/privacy_preserving_transaction/circuit/mod.rs`), so a private execute has a cycle bound now, far above anything here. In dev mode both shapes settle inside their window and are refused outside it with `OutOfValidityWindow` (`logs/spike12-actor-private-dev-*.log`).

Each row is one execute proved for real on 16 threads and then settled by the 0.4 state machine, inside its window, and refused outside it with `OutOfValidityWindow`. Peak memory was between 9,610,020 and 9,617,804 KiB, about 9.2 GiB, in every run, as on v0.3.0.

| Execute | Private notes | Prove time | Proof | Whole transaction |
| --- | --- | --- | --- | --- |
| M of 1, three signatures, payout to a new private account | 3 | 822.9 s (13.7 min) | 227,308 B | 231,489 B |
| M of 5, seven signatures, the same | 3 | 964.1 s (16.1 min) | 227,436 B | 231,745 B |
| M of 10, twelve signatures, the same | 3 | 1,208.0 s (20.1 min) | 227,596 B | 232,065 B |
| M of 5, seven signatures, payout to a public account | 2, and 1 public action | 830.0 s (13.8 min) | 226,331 B | 229,535 B |

The Logos node on this machine was mining during a first pass over the same rows (`logs/spike12-actor-proof-*-run1.log`: 910.1, 1,103.8, 1,311.4 and 860.1 s), and a second pass with mining off still had a few other cores busy (`logs/spike12-actor-proof-m5-private-run2.log`, 1,114.5 s at M of 5). The rows above are the second pass, except M of 5, which is a third run on the quietest machine we had, so they are upper bounds by a few percent.

What the curve says:

- Against section 2 the actor model costs about a minute and a half more per private execute at M of 1, 12.1 to 13.7 minutes, and half a minute more at M of 5 and M of 10, 15.6 to 16.1 and 19.5 to 20.1. The sizes are within a kilobyte. The fixed cost of a private transaction with a program still dominates: nine more signatures, from M of 1 to M of 10, add 6.4 minutes, about 0.7 each.
- A payout to a public account instead of a new private one saves one private note and about two minutes, in line with section 2's three minutes per private account consumed or written.
- A member's machine still needs about 10 GB of free memory, and peak memory still follows RISC Zero's segment size, not M.

The guest's rules are the same on both postures, so the 0.4 facts the design rests on are measured, not read: the configuration is readable in the step that spends, the vault moves only on the program's seed, the window is enforced by the state machine, and a private execute at M of 5 costs about 2M cycles of metered guest work and 16 minutes of proving on a desktop CPU.

Files: `lez-tests/guests/rfp005_multisig_actor.rs`, `lez-tests/rfp005_actor.rs`, `patches/06-actor-model-spike.patch`, `scripts/setup-actor.sh`, `scripts/run-actor-spike.sh`, `logs/spike12-actor-*.log`.

## Reproducing

Everything runs on LEZ v0.3.0 plus the four patches in `patches/`, and section 9 on LEZ dev 7e3f7f7 plus the sixth. You need Linux on x86_64, git, Rust through rustup (LEZ pins its toolchain), the RISC Zero toolchain at the versions we used, and about 10 GB of free memory for the real proofs.

```
scripts/setup.sh              # clones LEZ, makes a stock and an accelerated checkout, applies the patches
scripts/run-vault-tests.sh    # section 6, LEZ's 26 private PDA tests
scripts/run-stock.sh          # section 1 with the crates LEZ ships
scripts/run-accelerated.sh    # section 1 with the forks, and the real proof of the verification guest
scripts/run-logic-tests.sh    # sections 4 and 5 and the whole-transaction shapes, in dev mode
scripts/run-real-proofs.sh    # every real proof in section 2, about two and a half hours on 16 threads
scripts/run-lez-ppe-bench.sh  # LEZ's own privacy-circuit benchmark at 2811d0601 and v0.3.0, for comparison
scripts/setup-testnet.sh      # a LEZ v0.3.1 checkout with the driver, wallet and guest for section 2's public-testnet run
scripts/setup-actor.sh        # a LEZ dev checkout at 7e3f7f7 with the actor-model patch, for section 9
scripts/run-actor-spike.sh    # section 9: the public-path counts in dev mode, then its real proofs, about an hour on 16 threads
```

The RISC Zero toolchain, after `curl -L https://risczero.com/install | bash`:

```
rzup install rust 1.91.1 && rzup default rust 1.91.1
rzup install cargo-risczero 3.0.5 && rzup default cargo-risczero 3.0.5
rzup install r0vm 3.0.5 && rzup default r0vm 3.0.5
```

Check with `rzup show` that these are the versions marked as default, and remove any `work/*/target` built with another version. With these versions, cycle counts match ours exactly. A newer toolchain gives a few percent fewer, as section 1 shows for Rust 1.97.0.

Results go to `results/`. Times depend on the CPU. Keep the two checkouts' build directories separate: the scripts pin `CARGO_TARGET_DIR` to each checkout. The chat observations are run by hand, as `chat/spike8.md` describes, and so are the public-testnet steps, as `lez-tests/rfp005_testnet/README.md` describes. While the testnet keeps its current state, any transaction named in section 2 can be checked with `curl -s -X POST https://testnet.lez.logos.co -H 'content-type: application/json' -d '{"jsonrpc":"2.0","id":1,"method":"getTransaction","params":["<hash>"]}'`, which returns the transaction and its block.
