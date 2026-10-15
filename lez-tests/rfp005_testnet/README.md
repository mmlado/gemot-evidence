# The public-testnet run

The steps behind the "On the public testnet" paragraph in FINDINGS section 2, run by hand against https://testnet.lez.logos.co on 2026-10-14 and 15. `scripts/setup-testnet.sh` builds what they need. A testnet reset wipes the chain, after which every step runs again.

Paths below: `T=work/testnet` after the setup script, `W=$T/target/release/wallet`, `D=$T/target/release/rfp005_testnet`, `G=$T/target/riscv-guest/test_methods/test_methods_guests/riscv32im-risc0-zkvm-elf/release/rfp005_multisig_execute.bin`, and `S` an empty directory for this run's keys and results.

1. **A wallet on the testnet.** `LEE_WALLET_HOME_DIR` names its directory. The first command creates the storage after asking for a password on stdin.

   ```
   export LEE_WALLET_HOME_DIR=$S/wallet
   $W change-network testnet
   ```

   In `$S/wallet/wallet_config.json` raise `seq_tx_poll_max_blocks` and `seq_poll_max_retries` to 200: testnet blocks come from 10 seconds to over a minute apart, and the defaults give up after five.

2. **Funds.** The genesis state funds two public accounts whose signing keys LEZ publishes in `lez/testnet_initial_state/src/lib.rs` (`PRIVATE_KEY_PUB_ACC_A` and `_B`, 32 bytes each, hex-encode one). `init` writes `$S/keys.json`, the funder's signing key, seven approver keys, the three private key sets and the vault seed, and prints the funder's account id and key. Send it 100 LGO, 10^11 units.

   ```
   $D init $S
   $W account import public --private-key <genesis key A, hex>
   $W account import public --private-key <funder key, from the init output>
   $W account label --account-id Public/<funder id> --label funder
   $W auth-transfer send --from Public/<genesis account A id> --to funder --amount 100000000000
   ```

   Our run: block 11339, `logs/testnet-01-genesis-to-funder.log`.

3. **Deploy.** Five fresh public accounts, four segments and the header, then the guest behind an immutable header, every transaction paid by the funder. Keep the header's and `seg0`'s account ids.

   ```
   for i in 0 1 2 3; do $W account new public --label seg$i; done
   $W account new public --label header
   $W program-loader deploy --elf $G --header header --segments seg0 seg1 seg2 seg3 --immutable --payer funder
   ```

   Our run: five transactions in blocks 11344 to 11400, about 15 minutes at the testnet's block times, `logs/testnet-02-deploy.log`.

4. **Fund the vault**, a private transaction proved for real, about 4.5 minutes and 10 GB of memory.

   ```
   $D fund $S https://testnet.lez.logos.co <header id> 1000000000
   ```

   Our run: block 11482, `logs/testnet-fund.json`.

5. **Execute**, the wallet-shaped M of 5, about 15.5 minutes and 10 GB.

   ```
   $D execute $S https://testnet.lez.logos.co $G <header id> <seg0 id> 1000000000 300000000
   ```

   Our run: block 14439, `logs/testnet-execute.json`.

`--dry-run` on a driver command proves without submitting. `RISC0_DEV_MODE=1` rehearses a command in seconds with a proof the chain would reject. The driver reads the funder's public nonce after proving and signs then, because the testnet can re-include transactions in later blocks while a proof runs, which is what dropped our first funding attempt (`logs/testnet-03-fund-attempt1-stale-nonce.log`). `keys.json` holds every secret of the run and is not in this repository.
