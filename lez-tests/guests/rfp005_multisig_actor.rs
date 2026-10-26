//! RFP-005 spike on the actor model. The multisig actor keeps its configuration in its own
//! state, checks the proposer's, M approvers' and the executor's BIP-340 signatures over one
//! digest against that configuration, records the digest, and pays out of its vault PDA.
use borsh::{BorshDeserialize, BorshSerialize};
use k256::ecdsa::signature::hazmat::PrehashVerifier as _;
use lee_core::{
    account::{AccountId, Actor},
    native_token::Message as NativeMessage,
    program::{PdaSeed, ReceiveInput, Response, SyncMessage, write_once},
};

lee_core::actor_entrypoint!(handle_message);

#[derive(BorshSerialize, BorshDeserialize, Clone)]
pub struct Approval {
    pub public_key: [u8; 32],
    pub signature: [u8; 64],
}

#[derive(BorshSerialize, BorshDeserialize, Default)]
pub struct Config {
    pub members: Vec<[u8; 32]>,
    pub threshold: u32,
    pub executed: Vec<[u8; 32]>,
}

#[derive(BorshSerialize, BorshDeserialize)]
pub enum Message {
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

fn handle_message(input: &ReceiveInput, message: Message) -> Response {
    match message {
        Message::Init { members, threshold } => {
            assert!(input.from.is_none(), "Init is a top-level call");
            let threshold_len = usize::try_from(threshold).expect("threshold fits usize");
            assert!(
                threshold > 0 && threshold_len <= members.len(),
                "threshold within the member set"
            );
            let config = Config {
                members,
                threshold,
                executed: Vec::new(),
            };
            Response::set_state(write_once(
                input.pre_state.as_ref(),
                borsh::to_vec(&config).expect("config encodes"),
            ))
        }
        Message::QueueAndExecute {
            digest,
            proposer,
            approvals,
            executor,
            vault,
            vault_seed,
            recipient,
            amount,
            not_before,
            not_after,
        } => {
            let mut config: Config =
                borsh::from_slice(input.pre_state.as_ref()).expect("initialized configuration");
            assert!(!config.executed.contains(&digest), "digest already executed");
            verify_member(&config, &proposer, &digest);
            verify_member(&config, &executor, &digest);
            let mut seen: Vec<[u8; 32]> = Vec::with_capacity(approvals.len());
            for approval in &approvals {
                assert!(!seen.contains(&approval.public_key), "duplicate approver");
                seen.push(approval.public_key);
                verify_member(&config, approval, &digest);
            }
            let threshold = usize::try_from(config.threshold).expect("threshold fits usize");
            assert!(approvals.len() >= threshold, "below threshold");
            config.executed.push(digest);
            Response::set_state(borsh::to_vec(&config).expect("config encodes"))
                .try_timestamp_window(not_before..not_after)
                .expect("a non-empty window")
                .send(
                    SyncMessage::new(
                        Actor::native_balance(vault),
                        &NativeMessage::Transfer {
                            to: recipient,
                            amount,
                        },
                    )
                    .with_pda_seeds([vault_seed]),
                )
        }
    }
}

fn verify_member(config: &Config, approval: &Approval, digest: &[u8; 32]) {
    assert!(
        config.members.contains(&approval.public_key),
        "signer is not a member"
    );
    let key = k256::schnorr::VerifyingKey::from_bytes(&approval.public_key)
        .expect("valid x-only public key");
    let signature = k256::schnorr::Signature::try_from(&approval.signature[..])
        .expect("valid signature encoding");
    key.verify_prehash(digest, &signature)
        .expect("signature verifies");
}
