//! RFP-005 feasibility spike: shared private account contention and the public part of a
//! private transaction, against LEZ v0.3.0's own state machine.

use super::*;

fn test_private_account_keys_3() -> TestPrivateKeys {
    TestPrivateKeys {
        ask: AuthorizationSecretKey([55; 32]),
        d: [56; 32],
        z: [57; 32],
    }
}

fn private_id(keys: &TestPrivateKeys, identifier: Identifier) -> AccountId {
    AccountId::for_regular_private_account(&keys.npk(), &keys.vpk(), identifier)
}

fn private_transfer(
    sender_keys: &TestPrivateKeys,
    sender: &Account,
    recipient_keys: &TestPrivateKeys,
    recipient_identifier: Identifier,
    amount: u128,
    state: &V03State,
    ciphertext_padding: Option<u32>,
) -> PrivacyPreservingTransaction {
    let sender_id = private_id(sender_keys, Identifier::ZERO);
    let recipient_id = private_id(recipient_keys, recipient_identifier);
    let sender_commitment = Commitment::new(&sender_id, sender);
    let (output, proof) = execute_and_prove(
        ProvingInput {
            shard_selectors: vec![
                ProgramShardSelector::native_balance(sender_id),
                ProgramShardSelector::native_balance(recipient_id),
            ],
            private_witnesses: vec![
                update_witness(
                    sender_keys,
                    Identifier::ZERO,
                    sender.clone(),
                    state
                        .get_proof_for_commitment(&sender_commitment)
                        .expect("sender's commitment must be in state"),
                ),
                init_witness(recipient_keys, recipient_identifier),
            ],
            instruction_data: Program::serialize_instruction(NativeInstruction::Transfer {
                amount,
            })
            .unwrap(),
            ciphertext_padding,
            ..Default::default()
        },
        &ProgramWithDependencies::native(),
    )
    .unwrap();
    let message = Message::from_circuit_output(vec![], output);
    let witness_set = WitnessSet::for_message(&message, proof, &[]);
    PrivacyPreservingTransaction::new(message, witness_set)
}

#[test]
fn rfp005_two_updates_of_one_private_account_from_one_state_conflict() {
    let k1 = test_private_account_keys_1();
    let sender = Account {
        nonce: Nonce(1),
        ..Account::funded(100)
    };
    let mut state = V03State::new().with_private_account(&k1, &sender);

    let tx_a = private_transfer(
        &k1,
        &sender,
        &test_private_account_keys_2(),
        Identifier::ZERO,
        10,
        &state,
        None,
    );
    let tx_b = private_transfer(
        &k1,
        &sender,
        &test_private_account_keys_3(),
        Identifier::ZERO,
        20,
        &state,
        None,
    );

    state
        .transition_from_privacy_preserving_transaction(&tx_a, 1, 0)
        .unwrap();
    let err = state
        .transition_from_privacy_preserving_transaction(&tx_b, 2, 0)
        .unwrap_err();
    eprintln!("RFP005 second update of one account: {err:?}");
    assert!(matches!(err, LeeError::InvalidInput(ref m) if m == "Nullifier already seen"));
}

#[test]
fn rfp005_updates_of_two_private_accounts_built_on_one_state_both_settle() {
    let k1 = test_private_account_keys_1();
    let k2 = test_private_account_keys_2();
    let k3 = test_private_account_keys_3();
    let s1 = Account {
        nonce: Nonce(1),
        ..Account::funded(100)
    };
    let s2 = Account {
        nonce: Nonce(2),
        ..Account::funded(100)
    };
    let mut state = V03State::new()
        .with_private_account(&k1, &s1)
        .with_private_account(&k2, &s2);

    let tx_a = private_transfer(&k1, &s1, &k3, Identifier::ZERO, 10, &state, None);
    let tx_b = private_transfer(&k2, &s2, &k3, Identifier::new([1; 32]), 20, &state, None);

    state
        .transition_from_privacy_preserving_transaction(&tx_a, 1, 0)
        .unwrap();
    state
        .transition_from_privacy_preserving_transaction(&tx_b, 2, 0)
        .unwrap();
    eprintln!("RFP005 two accounts, one pre-state: both settled");
}

#[test]
fn rfp005_public_part_of_a_private_transaction() {
    let k1 = test_private_account_keys_1();
    let sender = Account {
        nonce: Nonce(1),
        ..Account::funded(100)
    };
    let state = V03State::new().with_private_account(&k1, &sender);

    for padding in [None, Some(512_u32)] {
        let started = std::time::Instant::now();
        let tx = private_transfer(
            &k1,
            &sender,
            &test_private_account_keys_2(),
            Identifier::ZERO,
            10,
            &state,
            padding,
        );
        let elapsed = started.elapsed();
        let m = &tx.message;
        let notes: Vec<(usize, usize, u8)> = m
            .private_actions
            .iter()
            .map(|a| {
                (
                    a.encrypted_post_state.ciphertext.as_bytes().len(),
                    a.encrypted_post_state.epk.0.len(),
                    a.encrypted_post_state.view_tag,
                )
            })
            .collect();
        let message_bytes = borsh::to_vec(m).unwrap().len();
        let proof_bytes = tx.witness_set.proof().clone().into_inner().len();
        let tx_bytes = borsh::to_vec(&tx).unwrap().len();
        eprintln!("RFP005 padding={padding:?} prove_time={elapsed:?}");
        eprintln!("RFP005   public_actions={:?}", m.public_actions);
        eprintln!("RFP005   nonces={:?}", m.nonces);
        eprintln!("RFP005   private_actions={} (ciphertext_len, epk_len, view_tag)={notes:?}", m.private_actions.len());
        eprintln!("RFP005   block_window={:?} timestamp_window={:?}", m.block_validity_window, m.timestamp_validity_window);
        eprintln!("RFP005   program_image_claims={:?}", m.program_image_claims);
        eprintln!("RFP005   signatures={}", tx.witness_set.signatures_and_public_keys().len());
        eprintln!("RFP005   message_bytes={message_bytes} proof_bytes={proof_bytes} tx_bytes={tx_bytes}");
    }
}

#[test]
fn rfp005_a_private_account_initializes_only_once() {
    let k1 = test_private_account_keys_1();
    let k2 = test_private_account_keys_2();
    let k3 = test_private_account_keys_3();
    let s1 = Account {
        nonce: Nonce(1),
        ..Account::funded(100)
    };
    let s2 = Account {
        nonce: Nonce(2),
        ..Account::funded(100)
    };
    let mut state = V03State::new()
        .with_private_account(&k1, &s1)
        .with_private_account(&k2, &s2);

    let tx_a = private_transfer(&k1, &s1, &k3, Identifier::new([7; 32]), 10, &state, None);
    let tx_b = private_transfer(&k2, &s2, &k3, Identifier::new([7; 32]), 20, &state, None);

    state
        .transition_from_privacy_preserving_transaction(&tx_a, 1, 0)
        .unwrap();
    let err = state
        .transition_from_privacy_preserving_transaction(&tx_b, 2, 0)
        .unwrap_err();
    eprintln!("RFP005 second initialization of one account: {err:?}");
    assert!(matches!(err, LeeError::InvalidInput(ref m) if m == "Nullifier already seen"));
}

fn admin_write_tx(
    keys: &TestPrivateKeys,
    seed: PdaSeed,
    pda_witness: PrivateWitness,
    pda_id: AccountId,
    config: AccountId,
    payload: Vec<u8>,
) -> Result<PrivacyPreservingTransaction, LeeError> {
    let _ = keys;
    let delegator = crate::test_methods::selective_pda_delegator();
    let target = crate::test_methods::rfp005_admin_target();
    let delegator_id = AccountId::from_builtin_program(delegator.id());
    let target_id = AccountId::from_builtin_program(target.id());
    let deps = ProgramWithDependencies::new(delegator, delegator_id, [(target_id, target.clone())].into());
    let instruction = (
        seed,
        target.id(),
        Program::serialize_instruction(payload).unwrap(),
        None::<(ProgramId, bool)>,
    );
    let (output, proof) = execute_and_prove(
        ProvingInput {
            shard_selectors: vec![
                ProgramShardSelector::native_balance(pda_id),
                ProgramShardSelector::new(config, target_id),
            ],
            private_witnesses: vec![pda_witness],
            instruction_data: Program::serialize_instruction(instruction).unwrap(),
            ..Default::default()
        },
        &deps,
    )?;
    let message = Message::from_circuit_output(vec![], output);
    let witness_set = WitnessSet::for_message(&message, proof, &[]);
    Ok(PrivacyPreservingTransaction::new(message, witness_set))
}

#[test]
fn rfp005_private_pda_authorizes_a_public_admin_write() {
    let delegator = crate::test_methods::selective_pda_delegator();
    let target = crate::test_methods::rfp005_admin_target();
    let delegator_id = AccountId::from_builtin_program(delegator.id());
    let target_id = AccountId::from_builtin_program(target.id());
    let keys = test_private_account_keys_1();
    let seed = PdaSeed::new([5; 32]);
    let ident = Identifier::ZERO;
    let pda_id = AccountId::for_private_pda(&delegator_id, &seed, &keys.npk(), &keys.vpk(), ident);
    let config = AccountId::new([66; 32]);
    let mut state = V03State::new().with_programs([delegator.clone(), target.clone()]);

    // First write: the authority PDA is created in the same private transaction.
    let tx1 = admin_write_tx(
        &keys,
        seed,
        init_pda_witness(&keys, ident, (delegator_id, seed)),
        pda_id,
        config,
        vec![1, 2, 3],
    )
    .unwrap();
    eprintln!("RFP005 admin tx1 public_actions={:?}", tx1.message.public_actions);
    eprintln!("RFP005 admin tx1 claims={:?}", tx1.message.program_image_claims);
    state
        .transition_from_privacy_preserving_transaction(&tx1, 1, 0)
        .unwrap();
    let written = state.get_account_by_id(config).data.shard(target_id).as_ref().to_vec();
    eprintln!("RFP005 admin config after tx1: {} bytes, authority prefix ok={}", written.len(), written.get(..32) == Some(pda_id.value().as_slice()));
    assert_eq!(written.get(..32), Some(pda_id.value().as_slice()));
    assert_eq!(written.get(32..), Some([1_u8, 2, 3].as_slice()));

    // Second write: the same PDA, now an existing private account, authorizes again.
    let pda_account = Account {
        nonce: Nonce::private_account_nonce_init(&pda_id),
        ..Account::default()
    };
    let proof = state
        .get_proof_for_commitment(&Commitment::new(&pda_id, &pda_account))
        .expect("the PDA's commitment is in state");
    let tx2 = admin_write_tx(
        &keys,
        seed,
        update_pda_witness(&keys, ident, (delegator_id, seed), pda_account, proof),
        pda_id,
        config,
        vec![9],
    )
    .unwrap();
    state
        .transition_from_privacy_preserving_transaction(&tx2, 2, 0)
        .unwrap();
    let written = state.get_account_by_id(config).data.shard(target_id).as_ref().to_vec();
    assert_eq!(written.get(32..), Some([9_u8].as_slice()));
    eprintln!("RFP005 admin tx2 (existing PDA) settled");

    // Another PDA of the same program, so another multisig, cannot write.
    let other_seed = PdaSeed::new([6; 32]);
    let other_id =
        AccountId::for_private_pda(&delegator_id, &other_seed, &keys.npk(), &keys.vpk(), ident);
    let tx3 = admin_write_tx(
        &keys,
        other_seed,
        init_pda_witness(&keys, ident, (delegator_id, other_seed)),
        other_id,
        config,
        vec![7],
    )
    .unwrap();
    let err = state
        .transition_from_privacy_preserving_transaction(&tx3, 3, 0)
        .unwrap_err();
    eprintln!("RFP005 admin tx3 (other multisig) rejected: {err:?}");
    let written = state.get_account_by_id(config).data.shard(target_id).as_ref().to_vec();
    assert_eq!(written.get(32..), Some([9_u8].as_slice()));
}

#[test]
fn rfp005_admin_write_real() {
    let delegator = crate::test_methods::selective_pda_delegator();
    let target = crate::test_methods::rfp005_admin_target();
    let delegator_id = AccountId::from_builtin_program(delegator.id());
    let target_id = AccountId::from_builtin_program(target.id());
    let keys = test_private_account_keys_1();
    let seed = PdaSeed::new([5; 32]);
    let ident = Identifier::ZERO;
    let pda_id = AccountId::for_private_pda(&delegator_id, &seed, &keys.npk(), &keys.vpk(), ident);
    let config = AccountId::new([66; 32]);
    let mut state = V03State::new().with_programs([delegator, target]);
    let started = std::time::Instant::now();
    let tx = admin_write_tx(
        &keys,
        seed,
        init_pda_witness(&keys, ident, (delegator_id, seed)),
        pda_id,
        config,
        vec![1, 2, 3],
    )
    .unwrap();
    let elapsed = started.elapsed();
    let tx_bytes = borsh::to_vec(&tx).unwrap().len();
    let proof_bytes = tx.witness_set.proof().clone().into_inner().len();
    eprintln!("RFP005 admin write prove_time={elapsed:?} proof_bytes={proof_bytes} tx_bytes={tx_bytes}");
    eprintln!("RFP005   public_actions={} private_actions={}", tx.message.public_actions.len(), tx.message.private_actions.len());
    state
        .transition_from_privacy_preserving_transaction(&tx, 1, 0)
        .unwrap();
    let written = state.get_account_by_id(config).data.shard(target_id).as_ref().to_vec();
    assert_eq!(written.get(..32), Some(pda_id.value().as_slice()));
    eprintln!("RFP005   admin write settled");
}
