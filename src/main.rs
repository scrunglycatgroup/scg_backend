/// LLM imports
use anyhow::{Error as E, Result};
use candle_core::{DType, Device, Tensor};
use candle_examples::token_output_stream::TokenOutputStream;
use candle_nn::VarBuilder;
use candle_transformers::generation::LogitsProcessor;
use candle_transformers::models::qwen2::{Config as ConfigBase, ModelForCausalLM as ModelBase};
use hf_hub::api::sync::Api;
use hf_hub::{Repo, RepoType};

/// Kafka imports
use rdkafka::Message;
use rdkafka::client::ClientContext;
use rdkafka::config::ClientConfig;
use rdkafka::consumer::{Consumer, ConsumerContext, StreamConsumer};

use serde::{Deserialize, Serialize};
/// surreal imports
use surrealdb::Surreal;
use surrealdb::engine::remote::ws::{Client, Ws};
use surrealdb::opt::{Resource, auth::Root};
use tokio::runtime::{Handle, Runtime};

/// std imports
use std::sync::{Arc, LazyLock, Mutex, mpsc};
use std::thread;

mod parser {
    /// Parser imports
    use winnow::Result as winnowRes;
    use winnow::prelude::*;
    use winnow::token::take_while;

    pub struct KafkaMessage<'a> {
        pub database_id: &'a str,
        pub message: &'a str,
    }

    fn parse_id<'s>(input: &mut &'s str) -> winnowRes<&'s str> {
        take_while(1.., (('0'..='9'), ('A'..='Z'), ('a'..='z'))).parse_next(input)
    }

    pub fn parse_kafka_message<'a>(input: &mut &'a str) -> Result<KafkaMessage<'a>, ()> {
        let id = parse_id.parse_next(input).map_err(|_| ())?;
        let out = KafkaMessage {
            database_id: id,
            message: input,
        };
        Ok(out)
    }
}

static DB: LazyLock<Surreal<Client>> = LazyLock::new(Surreal::init);

pub struct ThreadPool {
    workers: Vec<Worker>,
    sender: Option<mpsc::Sender<Job>>,
}

pub struct KafkaClientContext;

impl ClientContext for KafkaClientContext {}

impl ConsumerContext for KafkaClientContext {}

type Job = String;

impl ThreadPool {
    pub fn new(size: usize, handle: Arc<Handle>) -> ThreadPool {
        let (tx, rx) = mpsc::channel();
        let rx = Arc::new(Mutex::new(rx));
        let mut workers = Vec::with_capacity(size);
        for id in 0..size {
            workers.push(Worker::new(id, Arc::clone(&rx), Arc::clone(&handle)));
        }

        ThreadPool {
            workers,
            sender: Some(tx),
        }
    }
    pub fn execute(&self, f: Job) {
        self.sender.as_ref().unwrap().send(f).unwrap();
    }
}

impl Drop for ThreadPool {
    fn drop(&mut self) {
        for worker in &mut self.workers {
            println!("Shutting down worker {}", worker.id);
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

impl Worker {
    fn new(id: usize, reciever: Arc<Mutex<mpsc::Receiver<Job>>>, handle: Arc<Handle>) -> Worker {
        let thread = thread::spawn(move || {
            println!(
                "avx: {}, neon: {}, simd128: {}, f16c: {}",
                candle_core::utils::with_avx(),
                candle_core::utils::with_neon(),
                candle_core::utils::with_simd128(),
                candle_core::utils::with_f16c()
            );
            let start = std::time::Instant::now();
            let api = Api::new().unwrap();
            let model_id = "Qwen/Qwen2-1.5B";
            let repo = api.repo(Repo::with_revision(
                model_id.to_string(),
                RepoType::Model,
                "main".to_string(),
            ));
            let tokenizer_filename = repo.get("tokenizer.json").unwrap();
            let filenames = vec![repo.get("model.safetensors").unwrap()];
            let tokenizer =
                tokenizers::tokenizer::Tokenizer::from_file(tokenizer_filename).unwrap();
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
                    serde_json::from_slice(&std::fs::read(config_file).unwrap()).unwrap();
                ModelBase::new(&config, vb.unwrap()).unwrap()
            };
            println!("loaded the model in {:?}", start.elapsed());

            let mut pipeline =
                TextGeneration::new(model, tokenizer, 12345, Some(0.7), None, 1.1, 64, &device);
            loop {
                let message = reciever.lock().unwrap().recv();

                match message {
                    Ok(job) => {
                        // parse the job text into the database id and actual request
                        let contents = parser::parse_kafka_message(&mut &job[..]).unwrap(); // remove unwrap at some point
                        println!("Worker {id} got a job; executing.");
                        let output = match pipeline.run(contents.message, 500) {
                            Ok(s) => s,
                            Err(_) => "Error Parsing data".to_string(),
                        };
                        handle.block_on(async {
                            DB.update(Resource::from(("request", contents.database_id)))
                                .merge(FurfilRequest { output })
                                .await
                                .unwrap();
                        });
                    }
                    Err(_) => {
                        println!("Worker {id} disconnected; shutting down.");
                        break;
                    }
                }
            }
        });
        Worker {
            id,
            thread: Some(thread),
        }
    }
}

pub struct TextGeneration {
    model: ModelBase,
    device: Device,
    tokenizer: TokenOutputStream,
    logits_processor: LogitsProcessor,
    repeat_penalty: f32,
    repeat_last_n: usize,
}

impl TextGeneration {
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
pub async fn thread_dispatcher(topics: &[&'static str], pool: ThreadPool) {
    let context = KafkaClientContext;

    let consumer: StreamConsumer<KafkaClientContext> = ClientConfig::new()
        .set("group.id", "llm_dispatcher")
        .set("bootstrap.servers", "localhost:9092")
        .set("enable.partition.eof", "false")
        .set("session.timeout.ms", "6000")
        .set("enable.auto.commit", "true")
        .create_with_context(context)
        .expect("consumer creation failed");

    consumer
        .subscribe(topics)
        .expect("Could not subscribe to topics");

    loop {
        match consumer.recv().await {
            Err(e) => println!("Error recieving message: {:?}", e),
            Ok(m) => {
                match m.payload_view::<str>() {
                    Some(Ok(s)) => {
                        pool.execute(s.to_string());
                    }
                    Some(Err(e)) => {
                        println!("Error deserializing message: {:?}", e);
                    }
                    None => {}
                };
                consumer
                    .commit_message(&m, rdkafka::consumer::CommitMode::Async)
                    .unwrap();
            }
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct Request {
    input: String,
    output: String,
}

#[derive(Debug, Serialize)]
struct FurfilRequest {
    output: String,
}

async fn setup_db() -> surrealdb::Result<()> {
    DB.connect::<Ws>("localhost:8008").await?;
    DB.signin(Root {
        username: "root",
        password: "root",
    })
    .await?;
    DB.use_ns("test").use_db("test").await?;
    Ok(())
}

fn main() -> Result<(), ()> {
    let rt = Runtime::new().expect("Failed to create tokio runtime");

    let rt_handle = Arc::new(rt.handle().clone());
    // create the threadpool the LLMs will run on
    let pool = ThreadPool::new(1, Arc::clone(&rt_handle));
    let topics: Vec<&str> = vec!["message-topic"]; // TODO: ENVIRONMENT VARIABLE this please

    rt_handle.block_on(async {
        let _ = setup_db().await.map_err(|_| ());
        thread_dispatcher(&topics, pool).await;
    });

    Ok(())
}
