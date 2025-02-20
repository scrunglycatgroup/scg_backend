use crate::llm::TextGeneration;
use anyhow::{Error as E, Result};
use candle_core::DType;
use candle_nn::VarBuilder;
use candle_transformers::models::qwen2::{Config as ConfigBase, ModelForCausalLM as ModelBase};
use hf_hub::{api::sync::Api, Repo, RepoType};

mod llm;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Currently this is a lot of test code to check the model runs (very very slowly)
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
