//! RFP-005 feasibility spike: a whole multisig queue-and-execute as one private transaction.

use lee_core::{
    DummyInput, SharedSecretKey, compute_digest_for_path,
    encryption::{Ciphertext, EncryptedAccountData, MlKem768EncapsulationKey},
    program::{PROGRAM_LOADER_ACCOUNT_ID, ProgramHeader, immutable_mirror_commitment},
};
use program_loader_core::Instruction as LoaderInstruction;

use super::*;
use crate::{PublicKey, Signature};

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

fn recipient_keys() -> TestPrivateKeys {
    TestPrivateKeys {
        ask: AuthorizationSecretKey([55; 32]),
        d: [56; 32],
        z: [57; 32],
    }
}

struct Shape {
    signatures: usize,
    dummies: usize,
    undisclosed: bool,
    recorded: usize,
}

fn deploy_immutable(state: &mut V03State, program: &Program) -> (AccountId, ProgramHeader) {
    let segments = force_insert_segment_chain(state, program.elf(), 0x05);
    let header_key = PrivateKey::try_new([0x21; 32]).unwrap();
    let header_id = AccountId::from(&PublicKey::new_from_private_key(&header_key));
    let mut account_ids = vec![header_id];
    account_ids.extend_from_slice(&segments);
    let message = public_transaction::Message::try_new(
        PROGRAM_LOADER_ACCOUNT_ID,
        account_ids
            .into_iter()
            .map(|id| ProgramShardSelector::new(id, PROGRAM_LOADER_ACCOUNT_ID))
            .collect(),
        vec![Nonce(0)],
        LoaderInstruction::CreateHeader {
            first_segment: segments[0],
            immutable: true,
        },
    )
    .unwrap();
    let witness_set = public_transaction::WitnessSet::for_message(&message, &[&header_key]);
    state
        .transition_from_public_transaction(&PublicTransaction::new(message, witness_set), 1, 0)
        .expect("deploying the immutable header succeeds");
    let header = ProgramHeader {
        image_id: program.id(),
        program_first_segment: segments[0],
        immutable: true,
    };
    (header_id, header)
}

fn whole_multisig_tx(label: &str, shape: &Shape) {
    let program = crate::test_methods::rfp005_multisig_execute();
    let state_keys = test_private_account_keys_1();
    let group_keys = test_private_account_keys_2();
    let recipient_keys = recipient_keys();
    let vault_seed = PdaSeed::new([9; 32]);

    let state_account = Account {
        nonce: Nonce(1),
        ..Account::default()
    };
    let mut state = V03State::new().with_private_account(&state_keys, &state_account);
    let (program_id, header) = if shape.undisclosed {
        let (id, header) = deploy_immutable(&mut state, &program);
        (id, Some(header))
    } else {
        state = state.with_programs([program.clone()]);
        (AccountId::from_builtin_program(program.id()), None)
    };

    let state_id = AccountId::for_regular_private_account(
        &state_keys.npk(),
        &state_keys.vpk(),
        Identifier::ZERO,
    );
    let pda = |seed: PdaSeed| {
        AccountId::for_private_pda(
            &program_id,
            &seed,
            &group_keys.npk(),
            &group_keys.vpk(),
            Identifier::ZERO,
        )
    };
    let vault_id = pda(vault_seed);
    let recipient_id = AccountId::for_regular_private_account(
        &recipient_keys.npk(),
        &recipient_keys.vpk(),
        Identifier::ZERO,
    );
    let vault_account = Account {
        nonce: Nonce(2),
        ..Account::funded(100)
    };
    let recorded: Vec<(PdaSeed, AccountId, Account)> = (0..shape.recorded)
        .map(|i| {
            let seed = PdaSeed::new([u8::try_from(10 + i).unwrap(); 32]);
            let account = Account {
                nonce: Nonce(u128::try_from(10 + i).unwrap()),
                ..Account::default()
            };
            (seed, pda(seed), account)
        })
        .collect();
    let mut commitments = vec![Commitment::new(&vault_id, &vault_account)];
    commitments.extend(recorded.iter().map(|(_, id, a)| Commitment::new(id, a)));
    state.private_state.0.extend(&commitments);

    let state_commitment = Commitment::new(&state_id, &state_account);
    let state_proof = state.get_proof_for_commitment(&state_commitment).unwrap();
    let root = compute_digest_for_path(&state_commitment, &state_proof);
    let vault_proof = state
        .get_proof_for_commitment(&Commitment::new(&vault_id, &vault_account))
        .unwrap();

    let message = [0x42_u8; 32];
    let approvals = (1..=shape.signatures)
        .map(|i| {
            let key = PrivateKey::try_new([u8::try_from(i).unwrap(); 32]).unwrap();
            Approval {
                public_key: *PublicKey::new_from_private_key(&key).value(),
                signature: Signature::new(&key, &message).value,
            }
        })
        .collect();
    let instruction = (
        MultisigVerify { message, approvals },
        vault_seed,
        30_u128,
        1_000_u64,
        2_000_u64,
    );

    let mut shard_selectors = vec![
        ProgramShardSelector::new(state_id, program_id),
        ProgramShardSelector::native_balance(vault_id),
        ProgramShardSelector::native_balance(recipient_id),
    ];
    let mut private_witnesses = vec![
        update_witness(&state_keys, Identifier::ZERO, state_account, state_proof),
        update_pda_witness(
            &group_keys,
            Identifier::ZERO,
            (program_id, vault_seed),
            vault_account,
            vault_proof,
        ),
        init_witness(&recipient_keys, Identifier::ZERO),
    ];
    for (seed, id, account) in &recorded {
        shard_selectors.push(ProgramShardSelector::new(*id, program_id));
        let proof = state
            .get_proof_for_commitment(&Commitment::new(id, account))
            .unwrap();
        private_witnesses.push(update_pda_witness(
            &group_keys,
            Identifier::ZERO,
            (program_id, *seed),
            account.clone(),
            proof,
        ));
    }
    let (_, epk) =
        SharedSecretKey::encapsulate(&MlKem768EncapsulationKey::from_seed(&[1; 32], &[2; 32]));
    let dummy_inputs = (0..shape.dummies)
        .map(|i| {
            let i = u8::try_from(i).unwrap();
            DummyInput {
                nullifier_seed: [100 + i; 32],
                commitment_seed: [150 + i; 32],
                note: EncryptedAccountData {
                    ciphertext: Ciphertext::from_inner(vec![i; 512]),
                    epk: epk.clone(),
                    view_tag: 7,
                },
                commitment_root: root,
            }
        })
        .collect();

    let mut deps = ProgramWithDependencies::new(program, program_id, HashMap::new());
    if let Some(header) = header {
        let membership = state
            .get_proof_for_commitment(&immutable_mirror_commitment(program_id, &header))
            .expect("the immutable header's mirror commitment is in state");
        deps = deps.as_undisclosed_program(header, membership);
    }

    let started = std::time::Instant::now();
    let (output, proof) = execute_and_prove(
        ProvingInput {
            shard_selectors,
            private_witnesses,
            instruction_data: Program::serialize_instruction(instruction).unwrap(),
            dummy_inputs,
            ciphertext_padding: Some(512),
            ..Default::default()
        },
        &deps,
    )
    .unwrap();
    let elapsed = started.elapsed();

    let message = Message::from_circuit_output(vec![], output);
    let witness_set = WitnessSet::for_message(&message, proof, &[]);
    let tx = PrivacyPreservingTransaction::new(message, witness_set);
    let tx_bytes = borsh::to_vec(&tx).unwrap().len();
    let proof_bytes = tx.witness_set.proof().clone().into_inner().len();
    eprintln!(
        "RFP005 {label} signatures={} dummies={} undisclosed={} recorded={} prove_time={elapsed:?}",
        shape.signatures, shape.dummies, shape.undisclosed, shape.recorded
    );
    eprintln!(
        "RFP005   private_actions={} public_actions={} signatures_in_tx={}",
        tx.message.private_actions.len(),
        tx.message.public_actions.len(),
        tx.witness_set.signatures_and_public_keys().len()
    );
    eprintln!(
        "RFP005   timestamp_window={:?} claims={:?}",
        tx.message.timestamp_validity_window, tx.message.program_image_claims
    );
    eprintln!("RFP005   proof_bytes={proof_bytes} tx_bytes={tx_bytes}");

    let late = crate::validated_state_diff::ValidatedStateDiff::from_privacy_preserving_transaction(
        &tx, &state, 2, 2_500,
    )
    .err();
    eprintln!("RFP005   outside its window: {late:?}");
    state
        .transition_from_privacy_preserving_transaction(&tx, 2, 1_500)
        .unwrap();
    eprintln!("RFP005   settled inside its window");
}

const fn shape(signatures: usize) -> Shape {
    Shape {
        signatures,
        dummies: 0,
        undisclosed: false,
        recorded: 0,
    }
}

#[test]
fn rfp005_whole_multisig_tx_m5() {
    whole_multisig_tx("whole M=5", &shape(7));
}

#[test]
fn rfp005_curve_m1() {
    whole_multisig_tx("curve M=1", &shape(3));
}

#[test]
fn rfp005_curve_m3() {
    whole_multisig_tx("curve M=3", &shape(5));
}

#[test]
fn rfp005_curve_m10() {
    whole_multisig_tx("curve M=10", &shape(12));
}

#[test]
fn rfp005_wallet_shape_m5() {
    whole_multisig_tx(
        "wallet-shaped M=5",
        &Shape {
            signatures: 7,
            dummies: 4,
            undisclosed: true,
            recorded: 0,
        },
    );
}

#[test]
fn rfp005_fallback_queue_m5() {
    whole_multisig_tx(
        "chain-fallback queue M=5",
        &Shape {
            signatures: 2,
            dummies: 0,
            undisclosed: true,
            recorded: 6,
        },
    );
}
