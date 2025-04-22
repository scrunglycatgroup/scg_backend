use anyhow::{Error as E, Result};
use candle_core::{DType, Device, Tensor};
use candle_examples::token_output_stream::TokenOutputStream;
use candle_nn::VarBuilder;
use candle_transformers::generation::LogitsProcessor;
use candle_transformers::models::qwen2::{Config as ConfigBase, ModelForCausalLM as ModelBase};
use hf_hub::api::sync::Api;
use hf_hub::{Repo, RepoType};
use serde::{Deserialize, Serialize};
use std::sync::mpsc::RecvError;
use std::{
    sync::{
        Arc, Mutex,
        mpsc::{Receiver, Sender, channel},
    },
    thread,
};
use surrealdb::opt::Resource;
use tokio::runtime::Handle;

use crate::parser::EnvVars;
use crate::{DB, parser};

pub struct LLMPool {
    workers: Vec<Worker>,
    sender: Option<Sender<String>>,
}

impl LLMPool {
    pub fn new(size: usize, handle: Arc<Handle>, env_vars: &EnvVars) -> LLMPool {
        let (tx, rx) = channel();
        let rx = Arc::new(Mutex::new(rx));
        let mut workers = Vec::with_capacity(size);
        for id in 0..size {
            workers.push(Worker::new(
                id,
                Arc::clone(&rx),
                Arc::clone(&handle),
                env_vars.surreal.surreal_table.to_string().clone(),
                env_vars.lazy_load,
            ));
        }

        LLMPool {
            workers,
            sender: Some(tx),
        }
    }

    pub fn execute(&self, job: String) {
        self.sender.as_ref().unwrap().send(job).unwrap();
    }
}

impl Drop for LLMPool {
    fn drop(&mut self) {
        for worker in &mut self.workers {
            println!("Shutting down workers {}", worker.id);
            if let Some(thread) = worker.thread.take() {
                thread.join().unwrap();
            }
        }
    }
}

pub struct Worker {
    id: usize,
    thread: Option<thread::JoinHandle<()>>,
}

type WorkerRecv = Arc<Mutex<Receiver<String>>>;

#[derive(Debug, Serialize, Deserialize)]
struct Request {
    input: String,
    output: String,
}

#[derive(Debug, Serialize)]
struct FurfilRequest {
    output: String,
}

impl Worker {
    pub fn new(
        id: usize,
        recv: WorkerRecv,
        handle: Arc<Handle>,
        db_table: String,
        lazy_load: bool,
    ) -> Worker {
        let thread = thread::spawn(move || {
            if lazy_load {
                let message = recv.lock().unwrap().recv();
                let llm = Worker::spool_llm().unwrap();
                if let Ok(llm_out) =
                    Worker::handle_message(message, llm, Arc::clone(&handle), &db_table)
                {
                    Worker::run(llm_out, &recv, handle, &db_table)
                }
            } else {
                let llm = Worker::spool_llm().unwrap();
                Worker::run(llm, &recv, handle, &db_table)
            }
        });
        Worker {
            id,
            thread: Some(thread),
        }
    }

    fn spool_llm() -> Result<LLM, ()> {
        let start = std::time::Instant::now();
        let api = Api::new().map_err(|_| ())?;
        let model_id = "Qwen/Qwen2-1.5B"; // paramiterize this please
        let repo = api.repo(Repo::with_revision(
            model_id.to_string(),
            RepoType::Model,
            "main".to_string(),
        ));
        let tokenizer_filename = repo.get("tokenizer.json").unwrap();
        let filenames = vec![repo.get("model.safetensors").unwrap()];
        let tokenizer = tokenizers::tokenizer::Tokenizer::from_file(tokenizer_filename).unwrap();
        let config_file = repo.get("config.json").unwrap();
        let device = candle_examples::device(true).unwrap();
        let dtype = if device.is_cuda() {
            DType::BF16
        } else {
            DType::F32
        };
        let vb = unsafe { VarBuilder::from_mmaped_safetensors(&filenames, dtype, &device) };
        let model = {
            let config: ConfigBase =
                serde_json::from_slice(&std::fs::read(config_file).map_err(|_| ())?)
                    .map_err(|_| ())?;
            ModelBase::new(&config, vb.map_err(|_| ())?).map_err(|_| ())?
        };
        println!("loaded the model in {:?}", start.elapsed());

        Ok(LLM::new(
            model,
            tokenizer,
            12345,
            Some(0.7),
            None,
            1.1,
            64,
            &device,
        ))
    }

    fn run(llm: LLM, reciever: &WorkerRecv, handle: Arc<Handle>, surreal_table: &String) {
        let mut running_llm = llm;
        loop {
            let message = reciever.lock().unwrap().recv();
            if let Ok(llm_out) =
                Worker::handle_message(message, running_llm, Arc::clone(&handle), surreal_table)
            {
                running_llm = llm_out;
            } else {
                break;
            }
        }
    }

    fn handle_message(
        msg: Result<String, RecvError>,
        mut llm: LLM,
        handle: Arc<Handle>,
        surreal_table: &String,
    ) -> Result<LLM, ()> {
        match msg {
            Ok(s) => {
                let contents = parser::parse_kafka_message(&mut &s[..]).unwrap();
                let output = match llm.run(&contents.message, 500) {
                    Ok(s) => s,
                    Err(_) => "Error Generating output".to_string(),
                };
                handle.block_on(async {
                    DB.update(Resource::from((surreal_table, contents.database_id)))
                        .merge(FurfilRequest { output })
                        .await
                        .unwrap();
                });
                Ok(llm)
            }
            Err(_) => {
                println!("disconnected; shutting down.");
                Err(())
            }
        }
    }
}

#[allow(clippy::upper_case_acronyms)]
pub struct LLM {
    model: ModelBase,
    device: Device,
    tokenizer: TokenOutputStream,
    logits_processor: LogitsProcessor,
    repeat_penalty: f32,
    repeat_last_n: usize,
}

impl LLM {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        model: ModelBase,
        tokenizer: tokenizers::Tokenizer,
        seed: u64,
        temp: Option<f64>,
        top_p: Option<f64>,
        repeat_penalty: f32,
        repeat_last_n: usize,
        device: &Device,
    ) -> Self {
        let logits_processor = LogitsProcessor::new(seed, temp, top_p);
        let token: tokenizers::tokenizer::Tokenizer = tokenizer;
        Self {
            model,
            tokenizer: TokenOutputStream::new(token),
            logits_processor,
            repeat_penalty,
            repeat_last_n,
            device: device.clone(),
        }
    }

    pub fn run(&mut self, prompt: &str, sample_len: usize) -> Result<String, anyhow::Error> {
        use std::io::Write;
        let mut out: String = String::new();
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
                out.push_str(&t);
                std::io::stdout().flush()?;
            }
        }
        let dt = start_gen.elapsed();
        if let Some(rest) = self.tokenizer.decode_rest().map_err(E::msg)? {
            print!("{rest}");
            out.push_str(&rest);
        }
        std::io::stdout().flush()?;
        println!(
            "\n{generated_tokens} tokens generated ({:.2} tokens/s)",
            generated_tokens as f64 / dt.as_secs_f64(),
        );

        Ok(out)
    }
}
