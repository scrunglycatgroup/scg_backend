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

mod llm_pool;
mod parser;

static DB: LazyLock<Surreal<Client>> = LazyLock::new(Surreal::init);

pub struct KafkaClientContext;

impl ClientContext for KafkaClientContext {}

impl ConsumerContext for KafkaClientContext {}

pub async fn thread_dispatcher(vars: &KafkaVars, pool: LLMPool) {
    let context = KafkaClientContext;

    let consumer: StreamConsumer<KafkaClientContext> = ClientConfig::new()
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

    let topics: Vec<&str> = vars.kafka_topics.iter().map(|v| v.as_ref()).collect();
    consumer
        .subscribe(&topics)
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
async fn setup_db(env_vars: &SurrealVars) -> surrealdb::Result<()> {
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
    let env_vars = match read_environment_vars() {
        Ok(v) => v,
        Err(SetupVariableError::AskedHelp) => {
            return Err(());
        }
        Err(SetupVariableError::ParseError) => {
            panic!("Error parsing environment variables, please check them") // TODO: more on this 
        }
        Err(SetupVariableError::VarError(e)) => {
            panic!("Error reading {e}")
        }
    };
    let rt = Runtime::new().expect("Failed to create tokio runtime");

    let rt_handle = Arc::new(rt.handle().clone());
    // create the threadpool the LLMs will run on
    let pool = LLMPool::new(1, Arc::clone(&rt_handle), &env_vars);

    rt_handle.block_on(async {
        let _ = setup_db(&env_vars.surreal).await.map_err(|_| ());
        thread_dispatcher(&env_vars.kafka, pool).await;
    });

    Ok(())
}
