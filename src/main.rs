use llm_pool::LLMPool;
use parser::{KafkaVars, SetupVariableError, SurrealVars, read_environment_vars};
/// Kafka imports
use rdkafka::Message;
use rdkafka::client::ClientContext;
use rdkafka::config::ClientConfig;
use rdkafka::consumer::{Consumer, ConsumerContext, StreamConsumer};

/// surreal imports
use surrealdb::Surreal;
use surrealdb::engine::remote::ws::{Client, Ws};
use surrealdb::opt::auth::Root;
use tokio::runtime::Runtime;

/// std imports
use std::sync::{Arc, LazyLock};

mod llm_pool; // our custom stuff
mod parser;

// This is an application wide (global) lock on the database
static DB: LazyLock<Surreal<Client>> = LazyLock::new(Surreal::init);

/// This is to handle kafka client connection (it's mostly boilerplate)
pub struct KafkaClientContext;

impl ClientContext for KafkaClientContext {}

impl ConsumerContext for KafkaClientContext {}

// This is the main loop for getting jobs and sending them off to the pool
pub async fn thread_dispatcher(vars: &KafkaVars, pool: LLMPool) {
    let context = KafkaClientContext;

    let consumer: StreamConsumer<KafkaClientContext> =
        ClientConfig::new() // here we connect to the kafka stream with all the variables we need to
            .set("group.id", vars.kafka_group_id.to_string())
            .set(
                "bootstrap.servers",
                format!("{}:{}", vars.kafka_host, vars.kafka_port),
            )
            .set("enable.partition.eof", "false")
            .set("session.timeout.ms", vars.kafka_timeout.to_string())
            .set("enable.auto.commit", "true")
            .create_with_context(context)
            .expect("consumer creation failed");

    // connect to all the topics and listen to all of them
    let topics: Vec<&str> = vars.kafka_topics.iter().map(|v| v.as_ref()).collect();
    consumer
        .subscribe(&topics)
        .expect("Could not subscribe to topics");

    loop {
        match consumer.recv().await {
            // we have recieved a message
            Err(e) => println!("Error recieving message: {:?}", e),
            Ok(m) => {
                match m.payload_view::<str>() {
                    Some(Ok(s)) => {
                        // if the message is a string we send it to the LLM
                        pool.execute(s.to_string());
                    }
                    Some(Err(e)) => {
                        // kafka sent us something wrong
                        println!("Error deserializing message: {:?}", e);
                    }
                    None => {}
                };
                // we tell the kafka server we have dealt with all the messages we were given
                consumer
                    .commit_message(&m, rdkafka::consumer::CommitMode::Async)
                    .unwrap();
            }
        }
    }
}

/// Connect to the database with the variables passed to it
async fn setup_db(env_vars: &SurrealVars) -> surrealdb::Result<()> {
    // Ws is a websocket connection (allows for better callback and is recomended mode)
    DB.connect::<Ws>(format!(
        "{}:{}",
        env_vars.surreal_host, env_vars.surreal_port
    ))
    .await?;
    DB.signin(Root {
        username: &format!("{}", env_vars.surreal_user),
        password: &format!("{}", env_vars.surreal_pass),
    })
    .await?;
    DB.use_ns((*env_vars.surreal_namespace).to_string())
        .use_db((*env_vars.surreal_database).to_string())
        .await?;
    Ok(())
}

fn main() -> Result<(), ()> {
    // Entry point to the application
    let env_vars = match read_environment_vars() {
        // Here we read all the environment varaibles if we are missing any or
        // some are broken we exit out
        Ok(v) => v,
        Err(SetupVariableError::AskedHelp) => {
            // If they passed -h we give them the -h text and exit out
            return Ok(());
        }
        Err(SetupVariableError::ParseError) => {
            panic!("Error parsing environment variables, please check them") // TODO: more on this 
        }
        Err(SetupVariableError::VarError(e)) => {
            // They haven't given us an environment variable we need
            panic!("Error reading {e}")
        }
    };
    // tokio is an async package in rust, we use this to call the database functions inside
    // synchronous code
    let rt = Runtime::new().expect("Failed to create tokio runtime");

    // We wrap up this runtime handle so we can pass it around easily to the threads
    let rt_handle = Arc::new(rt.handle().clone());
    // create the threadpool the LLMs will run on
    let pool = LLMPool::new(1, Arc::clone(&rt_handle), &env_vars);

    // now we have the threadpool (and they are spooling up )
    // we can connect to the database and kafka stream and start taking input
    rt_handle.block_on(async {
        // await runs the async function and stops this thread until it's finished
        // map_err takes our error output and changes it to whatever we want
        let _ = setup_db(&env_vars.surreal).await.map_err(|_| ());
        thread_dispatcher(&env_vars.kafka, pool).await;
    });

    // exit out entirely
    Ok(())
}
