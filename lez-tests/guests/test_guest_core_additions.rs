// Appended to test_guest_core/src/guests.rs. Verifies M ed25519 approvals over one
// 32-byte message inside the guest, then writes M into the single account's data.

#[derive(borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct MultisigVerify {
    pub message: [u8; 32],
    pub approvals: Vec<Approval>,
}

#[derive(borsh::BorshSerialize, borsh::BorshDeserialize, Clone)]
pub struct Approval {
    pub public_key: [u8; 32],
    pub signature: [u8; 64],
}

pub fn multisig_verify() {
    match read_program_call::<MultisigVerify>() {
        ProgramCall::Plan(input, instruction) => {
            let Ok([account]) = <[_; 1]>::try_from(input.accounts.clone()) else {
                panic!("multisig_verify requires exactly 1 account");
            };
            let mut seen: Vec<[u8; 32]> = Vec::with_capacity(instruction.approvals.len());
            for approval in &instruction.approvals {
                assert!(!seen.contains(&approval.public_key), "duplicate signer");
                seen.push(approval.public_key);
                let key = ed25519_dalek::VerifyingKey::from_bytes(&approval.public_key)
                    .expect("valid public key");
                let signature = ed25519_dalek::Signature::from_bytes(&approval.signature);
                key.verify_strict(&instruction.message, &signature)
                    .expect("approval signature verifies");
            }
            let count = u32::try_from(instruction.approvals.len()).expect("fits u32");
            let effect = ShardEffect::new(&account, &count);
            GuestOutput::Plan(PlanOutput::new(input).with_effects(vec![effect])).write();
        }
        ProgramCall::Apply(input) => {
            let count: u32 = borsh::from_slice(&input.effect_data).expect("own effect");
            let data = count
                .to_le_bytes()
                .to_vec()
                .try_into()
                .expect("4 bytes fit the data limit");
            apply_write(input, data);
        }
    }
}

/// Same as `multisig_verify`, with BIP-340 Schnorr approvals on secp256k1 checked the way
/// LEZ checks its own transaction signatures (`verify_prehash` over a 32-byte digest).
pub fn multisig_verify_schnorr() {
    use k256::ecdsa::signature::hazmat::PrehashVerifier as _;
    match read_program_call::<MultisigVerify>() {
        ProgramCall::Plan(input, instruction) => {
            let Ok([account]) = <[_; 1]>::try_from(input.accounts.clone()) else {
                panic!("multisig_verify_schnorr requires exactly 1 account");
            };
            let mut seen: Vec<[u8; 32]> = Vec::with_capacity(instruction.approvals.len());
            for approval in &instruction.approvals {
                assert!(!seen.contains(&approval.public_key), "duplicate signer");
                seen.push(approval.public_key);
                let key = k256::schnorr::VerifyingKey::from_bytes(&approval.public_key)
                    .expect("valid x-only public key");
                let signature = k256::schnorr::Signature::try_from(&approval.signature[..])
                    .expect("valid signature encoding");
                key.verify_prehash(&instruction.message, &signature)
                    .expect("approval signature verifies");
            }
            let count = u32::try_from(instruction.approvals.len()).expect("fits u32");
            let effect = ShardEffect::new(&account, &count);
            GuestOutput::Plan(PlanOutput::new(input).with_effects(vec![effect])).write();
        }
        ProgramCall::Apply(input) => {
            let count: u32 = borsh::from_slice(&input.effect_data).expect("own effect");
            let data = count
                .to_le_bytes()
                .to_vec()
                .try_into()
                .expect("4 bytes fit the data limit");
            apply_write(input, data);
        }
    }
}

/// RFP-005 spike: `accounts = [state, vault, recipient, recorded..]`. Verifies every approval,
/// records the count and message in `state` and marks each recorded account, holds the action to a timestamp window, and moves `amount` out
/// of the vault PDA derived under `vault_seed`.
pub fn rfp005_multisig_execute() {
    use k256::ecdsa::signature::hazmat::PrehashVerifier as _;
    type Instruction = (MultisigVerify, PdaSeed, u128, u64, u64);
    match read_program_call::<Instruction>() {
        ProgramCall::Plan(input, (instruction, vault_seed, amount, not_before, not_after)) => {
            let [state, vault, recipient, recorded @ ..] = input.accounts.as_slice() else {
                panic!("rfp005_multisig_execute requires [state, vault, recipient, recorded..]");
            };
            let (state, vault, recipient, recorded) =
                (state.clone(), vault.clone(), recipient.clone(), recorded.to_vec());
            let mut seen: Vec<[u8; 32]> = Vec::with_capacity(instruction.approvals.len());
            for approval in &instruction.approvals {
                assert!(!seen.contains(&approval.public_key), "duplicate signer");
                seen.push(approval.public_key);
                let key = k256::schnorr::VerifyingKey::from_bytes(&approval.public_key)
                    .expect("valid x-only public key");
                let signature = k256::schnorr::Signature::try_from(&approval.signature[..])
                    .expect("valid signature encoding");
                key.verify_prehash(&instruction.message, &signature)
                    .expect("approval signature verifies");
            }
            let count = u32::try_from(instruction.approvals.len()).expect("fits u32");
            let mut plan = Plan::new(&input);
            plan.effect(&state, &(count, instruction.message));
            for account in &recorded {
                plan.effect(account, &(0_u32, instruction.message));
            }
            plan.timestamp_window(
                lee_core::program::TimestampValidityWindow::try_from(not_before..not_after)
                    .expect("a non-empty window"),
            );
            plan.call(custody_transfer(
                vault.account_id,
                vault_seed,
                recipient.account_id,
                amount,
            ));
            plan.write();
        }
        ProgramCall::Apply(input) => {
            let (count, message): (u32, [u8; 32]) =
                borsh::from_slice(&input.effect_data).expect("own effect");
            let mut data = count.to_le_bytes().to_vec();
            data.extend_from_slice(&message);
            apply_write(input, data.try_into().expect("fits the data limit"));
        }
    }
}
