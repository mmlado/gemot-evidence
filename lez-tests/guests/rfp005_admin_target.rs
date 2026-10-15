use lee_core::{
    account::AccountId,
    program::{GuestOutput, PlanOutput, ProgramCall, ShardEffect, apply_write, read_program_call},
};

/// RFP-005 spike: `accounts = [authority, config]`. The first write records `authority` in
/// `config`, later writes need that same authority, and every write needs it authorized.
fn main() {
    match read_program_call::<Vec<u8>>() {
        ProgramCall::Plan(input, payload) => {
            let Ok([authority, config]) = <[_; 2]>::try_from(input.accounts.clone()) else {
                panic!("rfp005_admin_target requires [authority, config]");
            };
            assert!(
                authority.is_authorized,
                "rfp005_admin_target: authority not authorized"
            );
            let effect = ShardEffect::new(&config, &(authority.account_id, payload));
            GuestOutput::Plan(PlanOutput::new(input).with_effects(vec![effect])).write();
        }
        ProgramCall::Apply(input) => {
            let (authority, payload): (AccountId, Vec<u8>) =
                borsh::from_slice(&input.effect_data).expect("own effect");
            let stored = input.pre_data.as_ref();
            if !stored.is_empty() {
                assert!(
                    stored.get(..32) == Some(authority.value().as_slice()),
                    "rfp005_admin_target: not the stored authority"
                );
            }
            let mut data = authority.value().to_vec();
            data.extend_from_slice(&payload);
            apply_write(input, data.try_into().expect("fits the data limit"));
        }
    }
}
