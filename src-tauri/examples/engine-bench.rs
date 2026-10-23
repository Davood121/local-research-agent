use llama_cpp_2::context::params::LlamaContextParams;
// llama.cpp's backend registry reads the Windows registry (RegOpenKeyExA) and
// llama-cpp-sys-2 only auto-links advapi32 for MSVC targets; declare the
// imports here so MinGW example binaries carry the advapi32 import library.
#[cfg(all(target_os = "windows", not(target_env = "msvc")))]
#[link(name = "advapi32")]
extern "system" {
    pub fn RegOpenKeyExA(
        key: usize,
        sub_key: *const u8,
        options: u32,
        access: u32,
        result: *mut usize,
    ) -> i32;
    pub fn RegQueryValueExA(
        key: usize,
        name: *const u8,
        reserved: *mut u32,
        reg_type: *mut u32,
        data: *mut u8,
        bytes: *mut u32,
    ) -> i32;
    pub fn RegCloseKey(key: usize) -> i32;
}
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::model::LlamaModel;
use llama_cpp_2::sampling::LlamaSampler;
use llama_cpp_2::token::LlamaToken;
use std::num::{NonZeroU16, NonZeroU32};
use std::time::Instant;

fn main() {
    let model_path = std::env::args().nth(1).unwrap_or_else(|| {
        format!(
            "{}\\dev.localresearch.agent\\models\\qwen2.5-0.5b-instruct-q4_k_m.gguf",
            std::env::var("APPDATA").unwrap_or_default()
        )
    });

    let backend = LlamaBackend::init().expect("backend");
    let params = LlamaModelParams::default().with_n_gpu_layers(0);
    let model = LlamaModel::load_from_file(&backend, &model_path, &params).expect("model load");
    let vocab = model.vocab();

    let prompt = "Explain in detail how a bicycle works, covering the drivetrain, gears, \
        wheels, steering, and brakes, as if teaching a curious beginner mechanic. \
        Include practical examples and a few maintenance tips along the way. \
        Then, separately, describe how the rider's posture affects efficiency.";
    let tokens: Vec<LlamaToken> = vocab.tokenize(prompt.as_bytes(), true, true);
    let prompt_len = tokens.len();

    for threads in [2u32, 4, 6, 8, 12] {
        // fresh context per thread configuration so KV cache stays clean
        let ctx_params = LlamaContextParams::default()
            .with_n_ctx(Some(NonZeroU32::new(2048).unwrap()))
            .with_n_threads(i32::try_from(threads).unwrap_or(2))
            .with_n_threads_batch(i32::try_from(threads).unwrap_or(2));
        let mut ctx = model.new_context(&backend, ctx_params).expect("ctx");

        // deterministic sampling to keep runs comparable
        let mut sampler = LlamaSampler::chain_simple([
            LlamaSampler::top_k(40),
            LlamaSampler::temp(0.7),
            LlamaSampler::top_p(0.95, 1),
            LlamaSampler::dist(0x1A2B3C4D),
        ]);

        let mut prefill_batch = LlamaBatch::new(256, 1);
        let total = tokens.len();
        let mut ingested = 0usize;
        let mut sample_idx = 0i32;
        let prefill_start = Instant::now();
        while ingested < total {
            let end = (ingested + 256).min(total);
            prefill_batch.clear();
            for (offset, &token) in tokens[ingested..end].iter().enumerate() {
                let is_end = ingested + offset == total - 1;
                prefill_batch
                    .add(
                        token,
                        i32::try_from(ingested + offset).unwrap_or(0),
                        &[0],
                        is_end,
                    )
                    .expect("add");
            }
            ctx.decode(&mut prefill_batch).expect("prefill decode");
            sample_idx = i32::try_from(end - ingested - 1).unwrap_or(0);
            ingested = end;
        }
        let prefill_ms = prefill_start.elapsed().as_millis();

        let mut gen_batch = LlamaBatch::new(1, 1);
        const N_GEN: usize = 128usize;
        let mut out_bytes: Vec<u8> = Vec::new();
        let gen_start = Instant::now();
        let mut next_pos = i32::try_from(total).unwrap_or(0);
        for _ in 0..N_GEN {
            let token = sampler.sample(&ctx, sample_idx);
            if vocab.is_eog(token) {
                break;
            }
            out_bytes.extend_from_slice(&vocab.token_to_piece(token, false, NonZeroU16::new(1)));
            sampler.accept(token);
            gen_batch.clear();
            gen_batch.add(token, next_pos, &[0], true).expect("add gen");
            ctx.decode(&mut gen_batch).expect("gen decode");
            sample_idx = 0;
            next_pos += 1;
        }
        let elapsed = gen_start.elapsed().as_secs_f64();
        let text = String::from_utf8_lossy(&out_bytes).to_string();
        let first_words: String = text
            .split_whitespace()
            .take(12)
            .collect::<Vec<_>>()
            .join(" ");
        println!(
            "threads={threads:>2} prompt={prompt_len} tok prefill={prefill_ms} ms gen={N_GEN} tok in {elapsed:.2}s -> {:.1} tok/s | text: {first_words}...",
            N_GEN as f64 / elapsed
        );
    }
}
