//! RFP-005 spike: the whole M-of-5 multisig transaction, sent to a live LEZ sequencer.
//!
//! `init DIR` writes fresh keys. `fund DIR RPC HEADER AMOUNT` moves AMOUNT from the funder into
//! the vault PDA. `execute DIR RPC ELF HEADER FIRST_SEGMENT VAULT_BALANCE AMOUNT` proves the
//! wallet-shaped multisig transaction and submits it. `--dry-run` proves without submitting.

use std::{
    borrow::Cow,
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use anyhow::{Context as _, Result, bail};
use common::transaction::LeeTransaction;
use lee::{
    PrivacyPreservingTransaction, PrivateKey, ProvingInput, PublicKey, Signature,
    execute_and_prove,
    privacy_preserving_transaction::{
        circuit::ProgramWithDependencies, message::Message, witness_set::WitnessSet,
    },
    program::Program,
};
use lee_core::{
    AuthorizationSecretKey, Commitment, DummyInput, Identifier, MembershipProof,
    NullifierPublicKey, NullifierSecretKey, NullifierWitness, PrivateWitness, SharedSecretKey,
    WitnessKind,
    account::{Account, AccountId, Nonce, ProgramShardSelector},
    encryption::{Ciphertext, EncryptedAccountData, MlKem768EncapsulationKey, ViewingPublicKey},
    native_token::Instruction as NativeInstruction,
    program::{PdaSeed, ProgramHeader, immutable_mirror_commitment},
};
use rand::{RngCore as _, rngs::OsRng};
use sequencer_service_rpc::{RpcClient as _, SequencerClient, SequencerClientBuilder};
use serde::{Deserialize, Serialize};
use serde_json::json;

const PAD: u32 = 512;
const MAX_PRIVATE: usize = 7;
const SIGNATURES: usize = 7;

#[derive(borsh::BorshSerialize)]
struct Approval {
    public_key: [u8; 32],
    signature: [u8; 64],
}

#[derive(borsh::BorshSerialize)]
struct MultisigVerify {
    message: [u8; 32],
    approvals: Vec<Approval>,
}

#[derive(Serialize, Deserialize)]
struct PrivKeys {
    ask: String,
    d: String,
    z: String,
}

impl PrivKeys {
    fn random() -> Self {
        Self {
            ask: hex::encode(random32()),
            d: hex::encode(random32()),
            z: hex::encode(random32()),
        }
    }
    fn ask(&self) -> AuthorizationSecretKey {
        AuthorizationSecretKey(hex32(&self.ask))
    }
    fn nsk(&self) -> NullifierSecretKey {
        (&self.ask()).into()
    }
    fn npk(&self) -> NullifierPublicKey {
        NullifierPublicKey::from(&self.nsk())
    }
    fn vpk(&self) -> ViewingPublicKey {
        ViewingPublicKey::from_seed(&hex32(&self.d), &hex32(&self.z))
    }
    fn regular_id(&self) -> AccountId {
        AccountId::for_regular_private_account(&self.npk(), &self.vpk(), Identifier::ZERO)
    }
}

#[derive(Serialize, Deserialize)]
struct Keys {
    funder: String,
    approvers: Vec<String>,
    state: PrivKeys,
    group: PrivKeys,
    recipient: PrivKeys,
    vault_seed: String,
}

impl Keys {
    fn funder(&self) -> PrivateKey {
        PrivateKey::try_new(hex32(&self.funder)).expect("stored key is valid")
    }
    fn vault_seed(&self) -> PdaSeed {
        PdaSeed::new(hex32(&self.vault_seed))
    }
    fn vault_id(&self, program_id: &AccountId) -> AccountId {
        AccountId::for_private_pda(
            program_id,
            &self.vault_seed(),
            &self.group.npk(),
            &self.group.vpk(),
            Identifier::ZERO,
        )
    }
}

fn hex32(s: &str) -> [u8; 32] {
    hex::decode(s)
        .expect("valid hex")
        .try_into()
        .expect("32 bytes")
}

fn random32() -> [u8; 32] {
    let mut bytes = [0; 32];
    OsRng.fill_bytes(&mut bytes);
    bytes
}

fn random_signing_key() -> String {
    loop {
        let bytes = random32();
        if PrivateKey::try_new(bytes).is_ok() {
            return hex::encode(bytes);
        }
    }
}

fn account_of(key: &PrivateKey) -> AccountId {
    AccountId::from(&PublicKey::new_from_private_key(key))
}

fn load_keys(dir: &Path) -> Result<Keys> {
    let path = dir.join("keys.json");
    serde_json::from_slice(&std::fs::read(&path).with_context(|| format!("{}", path.display()))?)
        .context("keys.json")
}

fn load_program(elf: &Path) -> Result<Program> {
    Program::new(Cow::Owned(std::fs::read(elf)?)).map_err(|e| anyhow::anyhow!("{e:?}"))
}

fn dummies(count: usize, root: lee_core::CommitmentSetDigest) -> Vec<DummyInput> {
    (0..count)
        .map(|_| {
            let (_, epk) = SharedSecretKey::encapsulate(&MlKem768EncapsulationKey::from_seed(
                &random32(),
                &random32(),
            ));
            let mut ciphertext = vec![0; PAD as usize];
            OsRng.fill_bytes(&mut ciphertext);
            DummyInput {
                nullifier_seed: random32(),
                commitment_seed: random32(),
                note: EncryptedAccountData {
                    ciphertext: Ciphertext::from_inner(ciphertext),
                    epk,
                    view_tag: OsRng.next_u32() as u8,
                },
                commitment_root: root,
            }
        })
        .collect()
}

fn init_regular(keys: &PrivKeys, root: lee_core::CommitmentSetDigest) -> PrivateWitness {
    PrivateWitness {
        vpk: keys.vpk(),
        random_seed: random32(),
        identifier: Identifier::ZERO,
        kind: WitnessKind::Regular {
            ask: Some(keys.ask()),
        },
        nullifier: NullifierWitness::Init {
            npk: keys.npk(),
            commitment_root: root,
        },
    }
}

fn init_pda(
    keys: &PrivKeys,
    binding: (AccountId, PdaSeed),
    root: lee_core::CommitmentSetDigest,
) -> PrivateWitness {
    PrivateWitness {
        vpk: keys.vpk(),
        random_seed: random32(),
        identifier: Identifier::ZERO,
        kind: WitnessKind::Pda { binding },
        nullifier: NullifierWitness::Init {
            npk: keys.npk(),
            commitment_root: root,
        },
    }
}

fn update_pda(
    keys: &PrivKeys,
    binding: (AccountId, PdaSeed),
    account: Account,
    membership_proof: MembershipProof,
) -> PrivateWitness {
    PrivateWitness {
        vpk: keys.vpk(),
        random_seed: random32(),
        identifier: Identifier::ZERO,
        kind: WitnessKind::Pda { binding },
        nullifier: NullifierWitness::Update {
            account,
            view_tag: OsRng.next_u32() as u8,
            nsk: keys.nsk(),
            membership_proof,
        },
    }
}

fn private_account(id: &AccountId, balance: u128) -> Account {
    Account {
        nonce: Nonce::private_account_nonce_init(id),
        ..Account::funded(balance)
    }
}

async fn prove(
    input: ProvingInput,
    deps: ProgramWithDependencies,
) -> Result<(lee_core::PrivacyPreservingCircuitOutput, lee::privacy_preserving_transaction::circuit::Proof, Duration)> {
    tokio::task::spawn_blocking(move || {
        let started = Instant::now();
        let (output, proof) = execute_and_prove(input, &deps).map_err(|e| anyhow::anyhow!("{e:?}"))?;
        Ok((output, proof, started.elapsed()))
    })
    .await?
}

async fn submit(
    client: &SequencerClient,
    tx: PrivacyPreservingTransaction,
) -> Result<(String, u64)> {
    let hash = client
        .send_transaction(LeeTransaction::PrivacyPreserving(tx))
        .await
        .context("sequencer refused the transaction")?;
    let hash_text = format!("{hash}");
    eprintln!("submitted {hash_text}");
    for _ in 0..120 {
        match client.get_transaction(hash).await {
            Ok(Some((_, block_id))) => {
                eprintln!("included in block {block_id}");
                return Ok((hash_text, block_id));
            }
            Ok(None) => {}
            Err(e) => eprintln!("poll failed, retrying: {e}"),
        }
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
    bail!("{hash_text} not included after ten minutes")
}

async fn require_commitment(client: &SequencerClient, what: &str, c: Commitment) -> Result<MembershipProof> {
    let (proofs, _) = client.get_proofs_and_root(vec![c]).await?;
    proofs
        .into_iter()
        .next()
        .flatten()
        .with_context(|| format!("{what}: commitment not in the chain's commitment set"))
}

fn write_result(dir: &Path, name: &str, value: &serde_json::Value) -> Result<()> {
    let out = dir.join("results");
    std::fs::create_dir_all(&out)?;
    std::fs::write(out.join(format!("{name}.json")), serde_json::to_vec_pretty(value)?)?;
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}

fn tx_sizes(tx: &PrivacyPreservingTransaction) -> (usize, usize) {
    (
        tx.witness_set.proof().clone().into_inner().len(),
        borsh::to_vec(tx).expect("serializes").len(),
    )
}

async fn fund(dir: &Path, client: &SequencerClient, header_id: AccountId, amount: u128, dry_run: bool) -> Result<()> {
    let keys = load_keys(dir)?;
    let funder_key = keys.funder();
    let funder_id = account_of(&funder_key);
    let vault_id = keys.vault_id(&header_id);
    let (_, root) = client.get_proofs_and_root(vec![]).await?;
    eprintln!("funder {funder_id}, vault {vault_id}");

    let input = ProvingInput {
        shard_selectors: vec![
            ProgramShardSelector::native_balance(funder_id),
            ProgramShardSelector::native_balance(vault_id),
        ],
        signers: HashSet::from([funder_id]),
        private_witnesses: vec![init_pda(&keys.group, (header_id, keys.vault_seed()), root)],
        instruction_data: Program::serialize_instruction(NativeInstruction::Transfer { amount })
            .map_err(|e| anyhow::anyhow!("{e:?}"))?,
        dummy_inputs: dummies(MAX_PRIVATE - 1, root),
        ciphertext_padding: Some(PAD),
    };
    let (output, proof, elapsed) = prove(input, ProgramWithDependencies::native()).await?;
    // The public nonce is outside the proof, so it is read after proving to survive reorgs.
    let funder = client.get_account(funder_id).await?;
    eprintln!("funder nonce {:?}", funder.nonce);
    let message = Message::from_circuit_output(vec![funder.nonce], output);
    let witness_set = WitnessSet::for_message(&message, proof, &[&funder_key]);
    let tx = PrivacyPreservingTransaction::new(message, witness_set);
    let (proof_bytes, tx_bytes) = tx_sizes(&tx);
    eprintln!("fund proved in {elapsed:?}, proof {proof_bytes} B, tx {tx_bytes} B");
    if dry_run {
        return Ok(());
    }
    let (hash, block) = submit(client, tx).await?;
    let vault_found = require_commitment(client, "vault", Commitment::new(&vault_id, &private_account(&vault_id, amount))).await.is_ok();
    write_result(dir, "fund", &json!({
        "tx": hash, "block": block, "vault_commitment_found": vault_found, "vault": vault_id.to_string(), "amount": amount.to_string(),
        "prove_seconds": elapsed.as_secs_f64(), "proof_bytes": proof_bytes, "tx_bytes": tx_bytes,
    }))
}

#[expect(clippy::too_many_arguments, reason = "one-off driver")]
async fn execute(
    dir: &Path,
    client: &SequencerClient,
    elf: &Path,
    header_id: AccountId,
    first_segment: AccountId,
    vault_balance: u128,
    amount: u128,
    dry_run: bool,
) -> Result<()> {
    let keys = load_keys(dir)?;
    let program = load_program(elf)?;
    let header = ProgramHeader {
        image_id: program.id(),
        program_first_segment: first_segment,
        immutable: true,
    };
    let vault_id = keys.vault_id(&header_id);
    let vault_account = private_account(&vault_id, vault_balance);
    let (proofs, root) = client
        .get_proofs_and_root(vec![
            immutable_mirror_commitment(header_id, &header),
            Commitment::new(&vault_id, &vault_account),
        ])
        .await?;
    let mut proofs = proofs.into_iter();
    let mirror_proof = proofs.next().flatten().context("immutable header mirror not in the commitment set")?;
    let vault_proof = proofs.next().flatten().context("vault commitment not in the commitment set")?;

    let last = client.get_last_block_id().await?;
    let now = client
        .get_block(last)
        .await?
        .context("last block")?
        .header
        .timestamp;
    let (not_before, not_after) = (now - 60_000, now + 3 * 3_600_000);

    let proposal = random32();
    let approvals = keys
        .approvers
        .iter()
        .take(SIGNATURES)
        .map(|k| {
            let key = PrivateKey::try_new(hex32(k)).expect("stored key is valid");
            Approval {
                public_key: *PublicKey::new_from_private_key(&key).value(),
                signature: Signature::new(&key, &proposal).value,
            }
        })
        .collect();
    let instruction = (
        MultisigVerify {
            message: proposal,
            approvals,
        },
        keys.vault_seed(),
        amount,
        not_before,
        not_after,
    );

    let state_id = keys.state.regular_id();
    let recipient_id = keys.recipient.regular_id();
    let input = ProvingInput {
        shard_selectors: vec![
            ProgramShardSelector::new(state_id, header_id),
            ProgramShardSelector::native_balance(vault_id),
            ProgramShardSelector::native_balance(recipient_id),
        ],
        signers: HashSet::new(),
        private_witnesses: vec![
            init_regular(&keys.state, root),
            update_pda(&keys.group, (header_id, keys.vault_seed()), vault_account, vault_proof),
            init_regular(&keys.recipient, root),
        ],
        instruction_data: Program::serialize_instruction(instruction)
            .map_err(|e| anyhow::anyhow!("{e:?}"))?,
        dummy_inputs: dummies(MAX_PRIVATE - 3, root),
        ciphertext_padding: Some(PAD),
    };
    let deps = ProgramWithDependencies::new(program, header_id, HashMap::new())
        .as_undisclosed_program(header, mirror_proof);
    eprintln!("proving the M of 5 transaction, {SIGNATURES} signatures, window {not_before}..{not_after}");
    let (output, proof, elapsed) = prove(input, deps).await?;
    let message = Message::from_circuit_output(vec![], output);
    let witness_set = WitnessSet::for_message(&message, proof, &[]);
    let tx = PrivacyPreservingTransaction::new(message, witness_set);
    let (proof_bytes, tx_bytes) = tx_sizes(&tx);
    let claims = format!("{:?}", tx.message.program_image_claims);
    let notes = tx.message.private_actions.len();
    eprintln!("execute proved in {elapsed:?}, proof {proof_bytes} B, tx {tx_bytes} B, notes {notes}, claims {claims}");
    if dry_run {
        return Ok(());
    }
    let (hash, block) = submit(client, tx).await?;
    let recipient_found = require_commitment(client, "recipient", Commitment::new(&recipient_id, &private_account(&recipient_id, amount))).await.is_ok();
    write_result(dir, "execute", &json!({
        "tx": hash, "block": block, "recipient_commitment_found": recipient_found, "program": header_id.to_string(), "image_id": format!("{:?}", header.image_id),
        "vault": vault_id.to_string(), "recipient": recipient_id.to_string(), "amount": amount.to_string(),
        "signatures": SIGNATURES, "message": hex::encode(proposal),
        "window_ms": [not_before, not_after], "notes": notes, "claims": claims,
        "prove_seconds": elapsed.as_secs_f64(), "proof_bytes": proof_bytes, "tx_bytes": tx_bytes,
    }))
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let dry_run = args.iter().any(|a| a == "--dry-run");
    let args: Vec<&str> = args.iter().filter(|a| *a != "--dry-run").map(String::as_str).collect();
    let client = |url: &str| -> Result<SequencerClient> { Ok(SequencerClientBuilder::default().build(url)?) };
    let rt = tokio::runtime::Runtime::new()?;
    match args.as_slice() {
        ["init", dir] => {
            let dir = PathBuf::from(dir);
            std::fs::create_dir_all(&dir)?;
            let path = dir.join("keys.json");
            if !path.exists() {
                let keys = Keys {
                    funder: random_signing_key(),
                    approvers: (0..SIGNATURES).map(|_| random_signing_key()).collect(),
                    state: PrivKeys::random(),
                    group: PrivKeys::random(),
                    recipient: PrivKeys::random(),
                    vault_seed: hex::encode(random32()),
                };
                std::fs::write(&path, serde_json::to_vec_pretty(&keys)?)?;
            }
            let keys = load_keys(&dir)?;
            println!("funder Public/{}", account_of(&keys.funder()));
            println!("funder key {}", keys.funder);
            Ok(())
        }
        ["image", elf] => {
            let program = load_program(Path::new(elf))?;
            println!("{:?}", program.id());
            Ok(())
        }
        ["fund", dir, rpc, header, amount] => rt.block_on(fund(
            Path::new(dir),
            &client(rpc)?,
            header.parse()?,
            amount.parse()?,
            dry_run,
        )),
        ["execute", dir, rpc, elf, header, first_segment, vault_balance, amount] => rt.block_on(execute(
            Path::new(dir),
            &client(rpc)?,
            Path::new(elf),
            header.parse()?,
            first_segment.parse()?,
            vault_balance.parse()?,
            amount.parse()?,
            dry_run,
        )),
        _ => bail!("usage: init DIR | image ELF | fund DIR RPC HEADER AMOUNT | execute DIR RPC ELF HEADER FIRST_SEGMENT VAULT_BALANCE AMOUNT [--dry-run]"),
    }
}
