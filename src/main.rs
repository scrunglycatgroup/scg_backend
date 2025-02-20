use anyhow::{Error as E, Result};
use candle_core::{DType, Device, Tensor};
use candle_examples::token_output_stream::TokenOutputStream;
use candle_transformers::generation::LogitsProcessor;
use candle_transformers::models::mimi::candle_nn::VarBuilder;
use candle_transformers::models::qwen2::{Config as ConfigBase, ModelForCausalLM as ModelBase};
use hf_hub::{api::sync::Api, Repo, RepoType};
use tokenizers::tokenizer::Tokenizer as TTokenizer;

struct TextGeneration {
    model: ModelBase,
    device: Device,
    tokenizer: TokenOutputStream,
    logits_processor: LogitsProcessor,
    repeat_penalty: f32,
    repeat_last_n: usize,
}

impl TextGeneration {
    #[allow(clippy::too_many_arguments)]
    fn new(
        model: ModelBase,
        tokenizer: TTokenizer,
        seed: u64,
        temp: Option<f64>,
        top_p: Option<f64>,
        repeat_penalty: f32,
        repeat_last_n: usize,
        device: &Device,
    ) -> Self {
        let logits_processor = LogitsProcessor::new(seed, temp, top_p);
        let token: TTokenizer = tokenizer;
        Self {
            model,
            tokenizer: TokenOutputStream::new(token),
            logits_processor,
            repeat_penalty,
            repeat_last_n,
            device: device.clone(),
        }
    }

    fn run(&mut self, prompt: &str, sample_len: usize) -> Result<()> {
        use std::io::Write;
        self.tokenizer.clear();
        let mut tokens = self
            .tokenizer
            .tokenizer()
            .encode(prompt, true)
            .map_err(E::msg)?
            .get_ids()
            .to_vec();
        for &t in tokens.iter() {
            if let Some(t) = self.tokenizer.next_token(t)? {
                print!("{t}");
            }
        }
        std::io::stdout().flush()?;
        let mut generated_tokens = 0usize;
        let eos_token = match self.tokenizer.get_token("<|endoftext|>") {
            Some(token) => token,
            None => anyhow::bail!("cannot find the <|endoftext|> token"),
        };
        let start_gen = std::time::Instant::now();
        for index in 0..sample_len {
            let context_size = if index > 0 { 1 } else { tokens.len() };
            let start_pos = tokens.len().saturating_sub(context_size);
            let ctxt = &tokens[start_pos..];
            let input = Tensor::new(ctxt, &self.device)?.unsqueeze(0)?;
            let logits = self.model.forward(&input, start_pos)?;
            let logits = logits.squeeze(0)?.squeeze(0)?.to_dtype(DType::F32)?;
            let logits = if self.repeat_penalty == 1. {
                logits
            } else {
                let start_at = tokens.len().saturating_sub(self.repeat_last_n);
                candle_transformers::utils::apply_repeat_penalty(
                    &logits,
                    self.repeat_penalty,
                    &tokens[start_at..],
                )?
            };
            let next_token = self.logits_processor.sample(&logits)?;
            tokens.push(next_token);
            generated_tokens += 1;
            if next_token == eos_token {
                break;
            }
            if let Some(t) = self.tokenizer.next_token(next_token)? {
                print!("{t}");
                std::io::stdout().flush()?;
            }
        }
        let dt = start_gen.elapsed();
        if let Some(rest) = self.tokenizer.decode_rest().map_err(E::msg)? {
            print!("{rest}");
        }
        std::io::stdout().flush()?;
        println!(
            "\n{generated_tokens} tokens generated ({:.2} tokens/s)",
            generated_tokens as f64 / dt.as_secs_f64(),
        );

        Ok(())
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!(
        "avx: {}, neon: {}, simd128: {}, f16c: {}",
        candle_core::utils::with_avx(),
        candle_core::utils::with_neon(),
        candle_core::utils::with_simd128(),
        candle_core::utils::with_f16c()
    );
    let start = std::time::Instant::now();
    let api = Api::new()?;
    let model_id = "Qwen/Qwen2-1.5B";
    let repo = api.repo(Repo::with_revision(
        model_id.to_string(),
        RepoType::Model,
        "main".to_string(),
    ));
    let tokenizer_filename = repo.get("tokenizer.json")?;
    let filenames = vec![repo.get("model.safetensors")?];
    let tokenizer =
        tokenizers::tokenizer::Tokenizer::from_file(tokenizer_filename).map_err(E::msg)?;
    let config_file = repo.get("config.json")?;
    let device = candle_examples::device(true)?;
    let dtype = if device.is_cuda() {
        DType::BF16
    } else {
        DType::F32
    };
    let vb = unsafe { VarBuilder::from_mmaped_safetensors(&filenames, dtype, &device)? };
    let model = {
        let config: ConfigBase = serde_json::from_slice(&std::fs::read(config_file)?)?;
        ModelBase::new(&config, vb)?
    };
    println!("loaded the model in {:?}", start.elapsed());

    let mut pipeline =
        TextGeneration::new(model, tokenizer, 12345, Some(0.7), None, 1.1, 64, &device);
    pipeline.run(
        "You are a text generation tool, generate a two sentence horror",
        1000,
    )?;
    Ok(())
}
