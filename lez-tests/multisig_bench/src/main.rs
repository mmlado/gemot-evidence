//! Cycle cost of verifying M ed25519 approvals inside a LEZ guest, as a function of M.
//! Mirrors tools/cycle_bench: executor user cycles per phase, optional real proving.
//! Run: cargo run --release -p multisig_bench -- --m 1,2,3,5,7,9,12 [--prove] [--out file.json]

use std::time::Instant;

use anyhow::{Result, bail};
use clap::Parser;
use ed25519_dalek::{Signer, SigningKey};
use lee::program::Program;
use lee_core::{
    account::{AccountId, ProgramShardSelector, ShardData},
    from_frame,
    program::{AccountMeta, ApplyInput, GuestOutput, PlanInput, PlanOutput},
};
use risc0_zkvm::{ExecutorEnv, default_executor, default_prover};
use test_guest_core::guests::{Approval, MultisigVerify};

const EXECUTION_LIMIT: u64 = 10_000_000;

#[derive(Parser)]
struct Args {
    /// Comma-separated list of M values.
    #[arg(long, default_value = "1,2,3,5,7,9,12")]
    m: String,
    /// Also prove each Plan phase with the default prover. Slow on CPU.
    #[arg(long)]
    prove: bool,
    /// Write results as JSON here.
    #[arg(long, default_value = "target/multisig_bench.json")]
    out: String,
    /// Enforce a session limit on the executor, like LEZ's cycle budget.
    #[arg(long)]
    budget: Option<u64>,
    /// Flip one byte of the last signature: the guest must reject it.
    #[arg(long)]
    tamper: bool,
    /// Approval signature scheme.
    #[arg(long, value_enum, default_value_t = Scheme::Ed25519)]
    scheme: Scheme,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, clap::ValueEnum, serde::Serialize)]
enum Scheme {
    Ed25519,
    Schnorr,
}

impl Scheme {
    fn name(self) -> &'static str {
        match self {
            Self::Ed25519 => "ed25519",
            Self::Schnorr => "schnorr",
        }
    }
}

#[derive(serde::Serialize)]
struct Row {
    scheme: &'static str,
    m: usize,
    phase: &'static str,
    user_cycles: u64,
    segments: usize,
    share_of_limit: f64,
    exec_ms: f64,
    prove_ms: Option<f64>,
    prove_total_cycles: Option<u64>,
    prove_segments: Option<usize>,
}

fn instruction(m: usize, tamper: bool, scheme: Scheme) -> MultisigVerify {
    let message = [7u8; 32];
    let mut approvals: Vec<Approval> = (0..m)
        .map(|i| {
            let seed = [u8::try_from(i + 1).expect("small"); 32];
            match scheme {
                Scheme::Ed25519 => {
                    let key = SigningKey::from_bytes(&seed);
                    Approval {
                        public_key: key.verifying_key().to_bytes(),
                        signature: key.sign(&message).to_bytes(),
                    }
                }
                Scheme::Schnorr => {
                    // Signed the way LEZ signs a transaction hash.
                    let key = k256::schnorr::SigningKey::from_bytes(&seed).expect("valid scalar");
                    let aux = [0u8; 32];
                    let signature = key
                        .sign_prehash_with_aux_rand(&message, &aux)
                        .expect("signs");
                    Approval {
                        public_key: key.verifying_key().to_bytes().into(),
                        signature: signature.to_bytes(),
                    }
                }
            }
        })
        .collect();
    if tamper {
        if let Some(last) = approvals.last_mut() {
            last.signature[5] ^= 0x01;
        }
    }
    MultisigVerify { message, approvals }
}

fn run_phase(
    scheme: &'static str,
    m: usize,
    phase: &'static str,
    program: &Program,
    prove: bool,
    budget: Option<u64>,
    write: impl Fn(&mut risc0_zkvm::ExecutorEnvBuilder) -> Result<()>,
) -> Result<(Vec<u8>, Row)> {
    let mut env_builder = ExecutorEnv::builder();
    if let Some(b) = budget {
        env_builder.session_limit(Some(b));
    }
    write(&mut env_builder)?;
    let env = env_builder.build()?;
    let started = Instant::now();
    let info = default_executor().execute(env, program.elf())?;
    let exec_ms = started.elapsed().as_secs_f64() * 1_000.0;
    let user_cycles = info.cycles();
    let mut row = Row {
        scheme,
        m,
        phase,
        user_cycles,
        segments: info.segments.len(),
        share_of_limit: user_cycles as f64 / EXECUTION_LIMIT as f64,
        exec_ms,
        prove_ms: None,
        prove_total_cycles: None,
        prove_segments: None,
    };
    eprintln!(
        "{scheme} M={m} {phase}: user_cycles={user_cycles} ({:.1}% of 10M) segments={} exec={exec_ms:.0}ms",
        row.share_of_limit * 100.0,
        row.segments
    );
    if prove {
        let mut env_builder = ExecutorEnv::builder();
        write(&mut env_builder)?;
        let env = env_builder.build()?;
        let started = Instant::now();
        let prove_info = default_prover()
            .prove(env, program.elf())
            .map_err(|e| anyhow::anyhow!("prove failed: {e}"))?;
        let prove_ms = started.elapsed().as_secs_f64() * 1_000.0;
        row.prove_ms = Some(prove_ms);
        row.prove_total_cycles = Some(prove_info.stats.total_cycles);
        row.prove_segments = Some(prove_info.stats.segments);
        let receipt_path = format!("target/receipt_{scheme}_m{m}_{phase}.json");
        std::fs::write(&receipt_path, serde_json::to_vec(&prove_info.receipt)?)?;
        prove_info
            .receipt
            .verify(program.id())
            .map_err(|e| anyhow::anyhow!("receipt verify failed: {e}"))?;
        eprintln!("M={m} {phase}: receipt verified against the image id and saved to {receipt_path}");
        eprintln!(
            "{scheme} M={m} {phase}: proved in {:.1}s, total_cycles={}, segments={}",
            prove_ms / 1_000.0,
            prove_info.stats.total_cycles,
            prove_info.stats.segments
        );
    }
    Ok((info.journal.bytes, row))
}

fn main() -> Result<()> {
    let args = Args::parse();
    let program = match args.scheme {
        Scheme::Ed25519 => test_programs::multisig_verify(),
        Scheme::Schnorr => test_programs::multisig_verify_schnorr(),
    };
    let scheme = args.scheme.name();
    let self_account_id = AccountId::from_builtin_program(program.id());
    let mut rows = Vec::new();
    for m in args.m.split(',').map(|s| s.trim().parse::<usize>()) {
        let m = m?;
        let account = AccountMeta::new(AccountId::new([9u8; 32]), false, self_account_id);
        let input = PlanInput {
            self_account_id,
            caller_account_id: None,
            accounts: vec![account.clone()],
            instruction_data: borsh::to_vec(&instruction(m, args.tamper, args.scheme))?,
        };
        let (journal, row) = run_phase(scheme, m, "plan", &program, args.prove, args.budget, |env| {
            Ok(Program::write_plan_inputs(&input, env)?)
        })?;
        rows.push(row);
        let payload = from_frame(&journal).ok_or_else(|| anyhow::anyhow!("malformed journal"))?;
        let plan: PlanOutput = match borsh::from_slice::<GuestOutput>(payload)? {
            GuestOutput::Plan(plan) => plan,
            GuestOutput::Apply(_) => bail!("plan phase returned an apply journal"),
        };
        for effect in plan.effects {
            let apply_input = ApplyInput {
                self_account_id,
                selector: effect.selector,
                pre_data: ShardData::default(),
                effect_data: effect.data,
            };
            let (_, row) = run_phase(scheme, m, "apply", &program, false, args.budget, |env| {
                Ok(Program::write_apply_inputs(&apply_input, env)?)
            })?;
            rows.push(row);
        }
        let _ = ProgramShardSelector::from(&account);
    }
    std::fs::write(&args.out, serde_json::to_string_pretty(&rows)?)?;
    eprintln!("wrote {}", args.out);
    Ok(())
}
