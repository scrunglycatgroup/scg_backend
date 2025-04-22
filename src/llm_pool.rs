// This is the hardest file to understand, it's a little messy, but it works
// and that's all I need to care about
//
// If you are new to rust this looks really intimidating, but just take a bit of
// time. There is a lot of boilerplate and really this just uses an expanded version
// of the final part of the rust-book tutorial.
// feel free to make it look nicer if you want!
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

// This holds all the threads and a message pipe that we can use to give a thread a job
pub struct LLMPool {
    workers: Vec<Worker>, // the threads
    sender: Option<Sender<String>>,
}

impl LLMPool {
    // this is __init__ for rust (basically), there are a few good reasons why
    // it isn't the same as __init__. https://www.youtube.com/watch?v=KWB-gDVuy_I
    // exlpains why this is better, but tldr: creation factories stop you from
    // reading from an object before it is ready
    pub fn new(size: usize, handle: Arc<Handle>, env_vars: &EnvVars) -> LLMPool {
        let (tx, rx) = channel();
        let rx = Arc::new(Mutex::new(rx)); // here we create a message pipe,
        // each worker gets a copy of the receiver and each one will try and `lock`
        // it. The receiver will be unlocked as soon as the the current locking
        // thread gets a message, meaning another thread can lock it and wait.

        let mut workers = Vec::with_capacity(size);
        for id in 0..size {
            workers.push(Worker::new(
                // create a new thread we will hold onto
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

    // when this is called we send something to the receiver, whichever thread
    // has the lock will take the String and then do whatever it needs to with it.
    pub fn execute(&self, job: String) {
        self.sender.as_ref().unwrap().send(job).unwrap();
    }
}

// When we are done with LLMPool we need to shut down all the threads
impl Drop for LLMPool {
    fn drop(&mut self) {
        drop(self.sender.take()); // we drop the sender which sends each thread
        // an Err(_) where in the main loop we close on recieving one
        for worker in &mut self.workers {
            println!("Shutting down workers {}", worker.id);
            if let Some(thread) = worker.thread.take() {
                thread.join().unwrap();
                // once threads have finished running their code join will close them
            }
        }
    }
}

// This is a `thread` really it's just an instance of the LLM and it will
// probably actuall use multiple threads for each one
pub struct Worker {
    id: usize,
    thread: Option<thread::JoinHandle<()>>,
}

type WorkerRecv = Arc<Mutex<Receiver<String>>>;

// What the database entry should have
#[derive(Debug, Serialize, Deserialize)]
struct Request {
    input: String,
    output: String,
}

// for updating a database element with whatever the code produces
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
        // note all this code isn't run before we actually construct the Worker,
        // simply this thread variable creates a new sub-process that will run all of the code inside the `{}` in a seperate thread!
        // Because of thread safety reasons we need to pass certain things as references
        let thread = thread::spawn(move || {
            if lazy_load {
                // if we are lazy loading we wait until recieving the first message before starting everything
                // from then on we just loop like usual
                let message = recv.lock().unwrap().recv();
                let llm = Worker::spool_llm().unwrap();
                if let Ok(llm_out) =
                    Worker::handle_message(message, llm, Arc::clone(&handle), &db_table)
                {
                    Worker::run(llm_out, &recv, handle, &db_table)
                }
            } else {
                // we aren't lazy loading so every thread starts spooling up immediately
                let llm = Worker::spool_llm().unwrap();
                Worker::run(llm, &recv, handle, &db_table)
            }
        });
        Worker {
            id,
            thread: Some(thread),
        }
    }

    // This is **A LOT** of boilerplate, basically just starts a new llm and loads everything we need to
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
            model,     // the model we are using
            tokenizer, // the tokeniser which handles inputs and outputs to the modle
            12345,     // the seed for all the randomness they need
            Some(0.7), // the temp for using that randomness
            None,
            1.1,     // how much we penalise for it repeating itself
            64,      // how many tokens we care about for the repeating
            &device, // info on what we are running it on
        ))
    }

    // this is the main loop of each thread, it waits for a message, and then runs the LLM when it recieves one
    fn run(llm: LLM, reciever: &WorkerRecv, handle: Arc<Handle>, surreal_table: &String) {
        let mut running_llm = llm;
        loop {
            let message = reciever.lock().unwrap().recv();
            if let Ok(llm_out) =
                Worker::handle_message(message, running_llm, Arc::clone(&handle), surreal_table)
            // this function call moves message out of scope so the next thread
            // can start recieving as soon as this is called
            {
                running_llm = llm_out; // this is a bit of a sneaky work around mutating the llm inside the message
            } else {
                // if we reciever gets Err() then we want to gracefully shutdown
                println!("disconnected; shutting down.");
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
                // the message we got is all good!
                //  turn the raw String into the database_id and the input to the transformer
                let contents = parser::parse_kafka_message(&mut &s[..]).unwrap();
                let output = match llm.run(&contents.message, 500) {
                    Ok(s) => s,
                    Err(_) => "Error Generating output".to_string(),
                };
                // now we create a little async function where we write to the
                // database and wait until that's finished before finishing up
                // and getting ready for the next message
                handle.block_on(async {
                    DB.update(Resource::from((surreal_table, contents.database_id)))
                        .merge(FurfilRequest { output })
                        .await
                        .unwrap();
                });
                Ok(llm)
            }
            Err(_) => Err(()), // not sure what would trigger this, maybe kafka recieving a bad message?
        }
    }
}

// this is a LOT of boilerplate, basically ignore. It's where all the LLM code is and
// candle handles most of this
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

    // here we take the input, turn it into tokens, run the transformer on those tokens, with each new token getting added
    // until we see some eos_token and then send whatever is generated
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
