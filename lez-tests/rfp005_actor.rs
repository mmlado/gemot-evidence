//! RFP-005 spike on the actor model: the multisig execute as one actor, measured on the public
//! path under the fee-market cap, and proved for real on the private path.

use std::time::Instant;

use super::*;
use crate::{Cycles, ExecutionCharge, Signature, ValidatedStateDiff};

const CONFIG_SEED: PdaSeed = PdaSeed::new([8; 32]);
const VAULT_SEED: PdaSeed = PdaSeed::new([9; 32]);
const DIGEST: [u8; 32] = [0x42; 32];
const AMOUNT: u128 = 30;
const PUBLIC_CAP: Cycles = 10_000_000;

#[derive(borsh::BorshSerialize, Clone)]
struct Approval {
    public_key: [u8; 32],
    signature: [u8; 64],
}

#[derive(borsh::BorshSerialize)]
struct Config {
    members: Vec<[u8; 32]>,
    threshold: u32,
    executed: Vec<[u8; 32]>,
}

#[derive(borsh::BorshSerialize)]
enum Message {
    Init {
        members: Vec<[u8; 32]>,
        threshold: u32,
    },
    QueueAndExecute {
        digest: [u8; 32],
        proposer: Approval,
        approvals: Vec<Approval>,
        executor: Approval,
        vault: AccountId,
        vault_seed: PdaSeed,
        recipient: AccountId,
        amount: u128,
        not_before: u64,
        not_after: u64,
    },
}

fn program_id() -> AccountId {
    AccountId::from_builtin_program(crate::test_methods::rfp005_multisig_actor().id())
}

fn member_keys(n: usize) -> Vec<PrivateKey> {
    (1..=n)
        .map(|i| PrivateKey::try_new([u8::try_from(i).unwrap(); 32]).unwrap())
        .collect()
}

fn xonly(key: &PrivateKey) -> [u8; 32] {
    *PublicKey::new_from_private_key(key).value()
}

fn approval(key: &PrivateKey) -> Approval {
    Approval {
        public_key: xonly(key),
        signature: Signature::new(key, &DIGEST).value,
    }
}

fn config_bytes(keys: &[PrivateKey], threshold: usize) -> Vec<u8> {
    borsh::to_vec(&Config {
        members: keys.iter().map(xonly).collect(),
        threshold: u32::try_from(threshold).unwrap(),
        executed: Vec::new(),
    })
    .unwrap()
}

/// The proposer and the executor are the first and the last member, so the guest checks M + 2
/// signatures, as the Gemot queue does.
fn execute_message(keys: &[PrivateKey], m: usize, vault: AccountId, recipient: AccountId) -> Message {
    Message::QueueAndExecute {
        digest: DIGEST,
        proposer: approval(&keys[0]),
        approvals: keys.iter().take(m).map(approval).collect(),
        executor: approval(&keys[keys.len() - 1]),
        vault,
        vault_seed: VAULT_SEED,
        recipient,
        amount: AMOUNT,
        not_before: 1_000,
        not_after: 2_000,
    }
}

fn m_list(default: &[usize]) -> Vec<usize> {
    std::env::var("RFP005_M")
        .ok()
        .map(|list| list.split(',').map(|m| m.trim().parse().unwrap()).collect())
        .unwrap_or_else(|| default.to_vec())
}

struct PublicFixture {
    state: V03State,
    program_id: AccountId,
    config_id: AccountId,
    vault_id: AccountId,
    recipient_id: AccountId,
    keys: Vec<PrivateKey>,
}

/// A public multisig of `n` members, initialized through a top-level `Init` call.
fn public_fixture(n: usize) -> PublicFixture {
    let program = crate::test_methods::rfp005_multisig_actor();
    let program_id = program_id();
    let config_id = AccountId::for_public_pda(&program_id, &CONFIG_SEED);
    let vault_id = AccountId::for_public_pda(&program_id, &VAULT_SEED);
    let recipient_id = AccountId::new([77; 32]);
    let keys = member_keys(n);
    let mut state = V03State::new()
        .with_programs([program])
        .with_empty_public_accounts([config_id])
        .with_public_account_balances([(vault_id, 100), (recipient_id, 0)]);
    let multisig = Actor::new(config_id, program_id);
    let init = public_tx(
        multisig,
        vec![multisig],
        vec![],
        Message::Init {
            members: keys.iter().map(xonly).collect(),
            threshold: u32::try_from(n).unwrap(),
        },
        &[],
    );
    state
        .transition_from_public_transaction(&init, 1, 0)
        .expect("the configuration initializes");
    PublicFixture {
        state,
        program_id,
        config_id,
        vault_id,
        recipient_id,
        keys,
    }
}

impl PublicFixture {
    fn execute(
        &self,
        m: usize,
        budget: Cycles,
    ) -> Result<(ValidatedStateDiff, ExecutionCharge), LeeError> {
        let multisig = Actor::new(self.config_id, self.program_id);
        let tx = public_tx(
            multisig,
            vec![
                multisig,
                Actor::native_balance(self.vault_id),
                Actor::native_balance(self.recipient_id),
            ],
            vec![],
            execute_message(&self.keys, m, self.vault_id, self.recipient_id),
            &[],
        );
        ValidatedStateDiff::from_public_transaction_with_cycle_budget(
            &tx,
            &self.state,
            2,
            1_500,
            budget,
        )
    }
}

fn share(cycles: Cycles) -> f64 {
    cycles as f64 * 100.0 / PUBLIC_CAP as f64
}

#[test]
fn rfp005_actor_public_curve() {
    for m in m_list(&[1, 5, 10, 20, 30]) {
        let mut fixture = public_fixture(m);
        let (diff, charge) = fixture
            .execute(m, crate::program::DEFAULT_PUBLIC_CYCLE_BUDGET)
            .expect("executes under the default budget");
        eprintln!(
            "RFP005-0.4 public M={m} signatures={} cycles={} share_of_10M={:.1}%",
            m + 2,
            charge.cycles,
            share(charge.cycles)
        );
        fixture.state.apply_state_diff(diff).unwrap();
        assert_eq!(
            fixture
                .state
                .get_account_by_id(fixture.recipient_id)
                .data
                .native_balance(),
            Ok(AMOUNT)
        );
        assert_eq!(
            fixture
                .state
                .get_account_by_id(fixture.vault_id)
                .data
                .native_balance(),
            Ok(100 - AMOUNT)
        );
        let recorded = fixture
            .state
            .get_account_by_id(fixture.config_id)
            .data
            .actor_state(fixture.program_id)
            .clone();
        let mut expected: Vec<u8> = config_bytes(&fixture.keys, m);
        // The executed list grows by one digest: length prefix 0 -> 1, then the digest.
        let len = expected.len();
        expected[len - 4] = 1;
        expected.extend_from_slice(&DIGEST);
        assert_eq!(Vec::<u8>::from(recorded), expected);
    }
}

#[test]
fn rfp005_actor_public_refusals() {
    let fixture = public_fixture(5);
    // Below threshold: four approvals for a threshold of five.
    let short = fixture.execute(4, PUBLIC_CAP).err();
    eprintln!("RFP005-0.4 public below threshold: {short:?}");
    assert!(short.is_some());
    // Outside the window.
    let multisig = Actor::new(fixture.config_id, fixture.program_id);
    let tx = public_tx(
        multisig,
        vec![
            multisig,
            Actor::native_balance(fixture.vault_id),
            Actor::native_balance(fixture.recipient_id),
        ],
        vec![],
        execute_message(&fixture.keys, 5, fixture.vault_id, fixture.recipient_id),
        &[],
    );
    let late = ValidatedStateDiff::from_public_transaction_with_cycle_budget(
        &tx,
        &fixture.state,
        2,
        2_500,
        PUBLIC_CAP,
    )
    .err();
    eprintln!("RFP005-0.4 public outside its window: {late:?}");
    assert!(late.is_some());
    // A direct debit of the vault without the program's seed.
    let debit = public_tx(
        Actor::native_balance(fixture.vault_id),
        vec![
            Actor::native_balance(fixture.vault_id),
            Actor::native_balance(fixture.recipient_id),
        ],
        vec![],
        transfer(fixture.recipient_id, AMOUNT),
        &[],
    );
    let stolen = ValidatedStateDiff::from_public_transaction_with_cycle_budget(
        &debit,
        &fixture.state,
        2,
        1_500,
        PUBLIC_CAP,
    )
    .err();
    eprintln!("RFP005-0.4 public vault debit without the seed: {stolen:?}");
    assert!(stolen.is_some());
}

/// The largest M whose execute fits the 10M cap, under the enforced budget.
#[test]
fn rfp005_actor_public_ceiling() {
    let at = |m: usize| public_fixture(m).execute(m, PUBLIC_CAP).map(|(_, charge)| charge.cycles);
    let low = at(1).expect("M=1 fits");
    let high = at(5).expect("M=5 fits");
    let per_signature = (high - low) / 4;
    let base = low - 3 * per_signature;
    let mut m = usize::try_from((PUBLIC_CAP - base) / per_signature).unwrap() - 2;
    eprintln!(
        "RFP005-0.4 public per_signature={per_signature} base={base} predicted_ceiling={m}"
    );
    while at(m).is_err() {
        m -= 1;
    }
    while at(m + 1).is_ok() {
        m += 1;
    }
    let fits = at(m).unwrap();
    let fails = at(m + 1).err().unwrap();
    eprintln!(
        "RFP005-0.4 public ceiling M={m} cycles={fits} share_of_10M={:.1}% M={} fails: {fails:?}",
        share(fits),
        m + 1
    );
}

fn recipient_keys() -> TestPrivateKeys {
    TestPrivateKeys {
        ask: AuthorizationSecretKey([55; 32]),
        d: [56; 32],
        z: [57; 32],
    }
}

/// One multisig execute as a privacy-preserving transaction: the configuration and the vault are
/// private PDAs held by the group's keys, the payout goes to a private or a public recipient.
#[test]
#[ignore = "real proving, run explicitly without RISC0_DEV_MODE"]
fn rfp005_actor_private_execute() {
    let m: usize = std::env::var("RFP005_M")
        .ok()
        .and_then(|m| m.parse().ok())
        .unwrap_or(5);
    let private_recipient = std::env::var("RFP005_RECIPIENT").as_deref() != Ok("public");
    let program = crate::test_methods::rfp005_multisig_actor();
    let program_id = program_id();
    let group_keys = test_private_account_keys_2();
    let config_id =
        AccountId::for_private_pda(&program_id, &CONFIG_SEED, &group_keys.npk(), &group_keys.vpk());
    let vault_id =
        AccountId::for_private_pda(&program_id, &VAULT_SEED, &group_keys.npk(), &group_keys.vpk());
    let keys = member_keys(m);
    let config_account =
        Account::default().with_actor_state(program_id, config_bytes(&keys, m).into());
    let vault_account = Account::funded(100);
    let public_recipient_id = AccountId::new([77; 32]);
    let recipient_id = if private_recipient {
        recipient_keys().account_id()
    } else {
        public_recipient_id
    };

    let mut state = V03State::new().with_programs([program.clone()]);
    if !private_recipient {
        state = state.with_public_account_balances([(public_recipient_id, 0)]);
    }
    let config_commitment = Commitment::new(&config_id, &config_account);
    let vault_commitment = Commitment::new(&vault_id, &vault_account);
    state
        .private_state
        .0
        .extend(&[config_commitment.clone(), vault_commitment.clone()]);
    let config_proof = state.get_proof_for_commitment(&config_commitment).unwrap();
    let vault_proof = state.get_proof_for_commitment(&vault_commitment).unwrap();

    let mut private_witnesses = vec![
        update_pda_witness(
            &group_keys,
            (program_id, CONFIG_SEED),
            config_account.clone(),
            config_proof,
        ),
        update_pda_witness(
            &group_keys,
            (program_id, VAULT_SEED),
            vault_account.clone(),
            vault_proof,
        ),
    ];
    let public_context = if private_recipient {
        private_witnesses.push(init_witness(&recipient_keys()));
        PublicExecutionContext::default()
    } else {
        PublicExecutionContext::new(vec![Actor::native_balance(public_recipient_id)], [])
    };
    let multisig = Actor::new(config_id, program_id);
    let input = ProvingInput {
        public_context,
        private_witnesses,
        ..proving_input(root(
            multisig,
            &execute_message(&keys, m, vault_id, recipient_id),
        ))
    };

    let started = Instant::now();
    let (output, proof) = execute_and_prove(
        input,
        &Simulation::default(),
        &synthetic_program(program),
        |_| SenderPresentation::Canonical,
        |_, _| private_recipient,
        no_seal,
    )
    .expect("the execute proves");
    let elapsed = started.elapsed();
    let proof_bytes = proof.0.len();
    let tx = private_tx((output, proof), vec![], &[]);
    let tx_bytes = borsh::to_vec(&tx).unwrap().len();
    eprintln!(
        "RFP005-0.4 private M={m} signatures={} recipient={} prove_time={elapsed:?} proof_bytes={proof_bytes} tx_bytes={tx_bytes}",
        m + 2,
        if private_recipient { "private" } else { "public" }
    );
    eprintln!(
        "RFP005-0.4   private_actions={} commitments={} nullifiers={}",
        tx.message().execution.private_actions.len(),
        tx.message().execution.commitments().len(),
        tx.message().execution.nullifiers().len()
    );

    let late = ValidatedStateDiff::from_privacy_preserving_transaction(&tx, &state, 2, 2_500).err();
    eprintln!("RFP005-0.4   outside its window: {late:?}");
    assert!(late.is_some());
    state
        .transition_from_privacy_preserving_transaction(&tx, 2, 1_500)
        .expect("settles inside its window");
    if !private_recipient {
        assert_eq!(
            state
                .get_account_by_id(public_recipient_id)
                .data
                .native_balance(),
            Ok(AMOUNT)
        );
    }
    eprintln!("RFP005-0.4   settled inside its window");
}
