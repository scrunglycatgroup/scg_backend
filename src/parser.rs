#[doc = r"This module handles all the parsing of external data to whatever the rust_dispatcher needs"]
use std::env::VarError;
use std::env::{self, var};

use dotenv::dotenv;

use winnow::Result as WinnowResult;
use winnow::ascii::digit1;
use winnow::combinator::{alt, opt, separated};
use winnow::prelude::*;
use winnow::token::{take_until, take_while};

/// Kafka sends us a single string at the moment, the regex for this being:
/// ```
/// /(?<database_id>\w+) (?<whatever_the_llms_total_input_is>.|\n|\r)+/g
/// ```
#[derive(Debug)]
pub struct ParsedKafkaMessage {
    pub database_id: String,
    pub message: String,
}

impl std::str::FromStr for ParsedKafkaMessage {
    type Err = String;

    fn from_str(mut s: &str) -> Result<Self, Self::Err> {
        parse_kafka_message(&mut s).map_err(|e| e.to_string())
    }
}

fn parse_database_id<'s>(input: &mut &'s str) -> WinnowResult<&'s str> {
    take_until(0.., " ").parse_next(input)
}

pub fn parse_kafka_message(input: &mut &str) -> WinnowResult<ParsedKafkaMessage> {
    let id = parse_database_id.parse_next(input)?;
    let out = ParsedKafkaMessage {
        database_id: id.to_string(),
        message: input.to_string(),
    };
    Ok(out)
}

/// Getting from the environment

#[derive(Debug)]
#[allow(dead_code)]
pub enum SetupVariableError {
    VarError(VarError),
    ParseError,
    AskedHelp, // the user asked for help, we shoud now gracefully exit
}

pub use SetupVariableError as SVErr;

type EnvVar = Box<str>;
// technically should move over to Arc<str> or Box<str> (I think I will be Arc'ing the actual struct so Arc is better)
pub struct EnvVars {
    pub kafka: KafkaVars,
    pub surreal: SurrealVars,
    pub lazy_load: bool,
}
pub struct SurrealVars {
    pub surreal_host: EnvVar,      // SURREAL_HOST
    pub surreal_port: EnvVar,      // SURREAL_PORT
    pub surreal_namespace: EnvVar, // SURREAL_NAMESPACE
    pub surreal_database: EnvVar,  // SURREAL_DATABASE
    pub surreal_user: EnvVar,      // SURREAL_USER
    pub surreal_pass: EnvVar,      // SURREAL_PASS
    pub surreal_table: EnvVar,     // SURREAL_TABLE
}

pub struct KafkaVars {
    pub kafka_host: EnvVar,          // KAFKA_HOST
    pub kafka_port: EnvVar,          // KAFKA_PORT
    pub kafka_topics: Box<[EnvVar]>, // KAFKA_TOPICS
    pub kafka_group_id: EnvVar,      // KAFKA_GROUP_ID
    pub kafka_timeout: EnvVar,       // KAFKA_TIMEOUT
}

// Here is where we handle reading environment variables for everything
//
pub fn read_environment_vars() -> Result<EnvVars, SVErr> {
    dotenv().ok();
    let args: Vec<_> = env::args().collect();
    if args.contains(&"-h".to_string()) || args.contains(&"--help".to_string()) {
        println!(
            r"
/=======================================================================================\
|          Environment Variables (can be in a .env)                                     | 
|=======================================================================================| 
| SURREAL_HOST        : The host ip for the surreal db (can take localhost)             | 
| SURREAL_PORT        : The port the surreal db is exposed on                           | 
| SURREAL_NAMESPACE   : The namespace this shard will attach to on the surreal server   | 
| SURREAL_DATABASE    : The database within the namespace this shard will attach to     | 
| SURREAL_USER        : The username for the server                                     | 
| SURREAL_PASS        : The password for the server                                     | 
| SURREAL_TABLE       : The table it will read and write to / from                      | 
|                                                                                       | 
| KAFKA_HOST          : The host ip for the kafka instance (can take localhost)         | 
| KAFKA_PORT          : The port the kafka instance is exposed on                       | 
| KAFKA_TOPICS        : A comma separated list (no spaces) of topics it will listen to  | 
| KAFKA_GROUP_ID      : The name of the group this shard will sign in as                | 
| KAFKA_TIMEOUT       : The length of the timeout to kafka                              | 
|                                                                                       | 
| LAZY_LOAD           : (bool) If the locally run LLMs should be loaded at program start|
|                        or when called, best used for running locally or testing       |
|                                                                                       | 
\=======================================================================================/"
        );
        return Err(SetupVariableError::AskedHelp);
    }
    let lazy_load_var = var("LAZY_LOAD").map_err(SVErr::VarError)?;
    let lazy_load = parse_bool(&mut &lazy_load_var[..]).map_err(|_| SVErr::ParseError)?;
    Ok(EnvVars {
        kafka: parse_kafka_env_vars()?,
        surreal: parse_surreal_env_vars()?,
        lazy_load,
    })
}

/// This checks for all the kafka environment variables, if it can't find any it *NEEDS* then it will error out
/// If any supplied are malformed then it will error out
fn parse_kafka_env_vars() -> Result<KafkaVars, SVErr> {
    let hostname_var = var("KAFKA_HOST").map_err(SVErr::VarError)?;
    let hostname = test_hostname(&mut &hostname_var[..]).map_err(|_| SVErr::ParseError)?;
    let port_var = var("KAFKA_PORT").map_err(SVErr::VarError)?;
    let port = test_port(&mut &port_var[..]).map_err(|_| SVErr::ParseError)?;
    let topics_var = var("KAFKA_TOPICS").map_err(SVErr::VarError)?;
    let topics: Box<[EnvVar]> = (parse_topics(&mut &topics_var[..])
        .map_err(|_| SVErr::ParseError)?)
    .iter()
    .map(|v| ((*v).to_string()).into_boxed_str())
    .collect();
    let group_id_var = var("KAFKA_GROUP_ID").map_err(SVErr::VarError)?;
    let timeout_var = var("KAFKA_TIMEOUT").unwrap_or("60000".to_string());
    let timeout = test_timeout(&mut &timeout_var[..]).map_err(|_| SVErr::ParseError)?;

    Ok(KafkaVars {
        kafka_host: hostname.into(),
        kafka_port: port.into(),
        kafka_topics: topics,
        kafka_group_id: group_id_var.into(),
        kafka_timeout: timeout.into(),
    })
}

fn parse_surreal_env_vars() -> Result<SurrealVars, SVErr> {
    let hostname_var = var("SURREAL_HOST").map_err(SVErr::VarError)?;
    let hostname = test_hostname(&mut &hostname_var[..]).map_err(|_| SVErr::ParseError)?;
    let port_var = var("SURREAL_PORT").map_err(SVErr::VarError)?;
    let port = test_port(&mut &port_var[..]).map_err(|_| SVErr::ParseError)?;
    let namespace_var = var("SURREAL_NAMESPACE").map_err(SVErr::VarError)?;
    let database_var = var("SURREAL_DATABASE").map_err(SVErr::VarError)?;
    let user_var = var("SURREAL_USER").map_err(SVErr::VarError)?;
    let pass_var = var("SURREAL_PASS").map_err(SVErr::VarError)?;
    let table_var = var("SURREAL_TABLE").map_err(SVErr::VarError)?;

    Ok(SurrealVars {
        surreal_host: hostname.into(),
        surreal_port: port.into(),
        surreal_namespace: namespace_var.into(),
        surreal_database: database_var.into(),
        surreal_user: user_var.into(),
        surreal_pass: pass_var.into(),
        surreal_table: table_var.into(),
    })
}

fn test_hostname<'i>(hostname: &mut &'i str) -> Result<&'i str, anyhow::Error> {
    alt((test_ip.take(), "localhost"))
        .parse(hostname)
        .map_err(|e| anyhow::format_err!("{e}"))
}

fn test_ip(input: &mut &str) -> WinnowResult<()> {
    separated(1..=4, test_ip_byte, '.').parse_next(input)
}

fn test_ip_byte<'i>(input: &mut &'i str) -> WinnowResult<&'i str> {
    digit1
        .parse_to::<u32>()
        .verify(|i: &u32| *i <= 255)
        .take()
        .parse_next(input)?;
    Ok(input)
}

fn test_port<'i>(input: &mut &'i str) -> WinnowResult<&'i str> {
    digit1
        .parse_to::<u32>()
        .verify(|i: &u32| *i < (u16::MAX as u32))
        .take()
        .parse_next(input)
}

fn test_timeout<'i>(input: &mut &'i str) -> WinnowResult<&'i str> {
    digit1.parse_to::<u32>().take().parse_next(input)
}

fn parse_topics<'i>(input: &mut &'i str) -> Result<Vec<&'i str>, anyhow::Error> {
    separated(1.., topic_parser, ',')
        .parse(input)
        .map_err(|e| anyhow::format_err!("{e}"))
}

fn topic_parser<'i>(input: &mut &'i str) -> WinnowResult<&'i str> {
    take_while(1.., ('_', '0'..='9', 'a'..='z', 'A'..='Z')).parse_next(input)
}

fn parse_bool(input: &mut &str) -> Result<bool, anyhow::Error> {
    let true_opt: Option<char> = opt(parse_t)
        .parse_next(input)
        .map_err(|e| anyhow::format_err!("{e}"))?;
    if true_opt.is_some() {
        return Ok(true);
    }
    let false_opt = opt(parse_f)
        .parse_next(input)
        .map_err(|e| anyhow::format_err!("{e}"))?;
    if false_opt.is_some() {
        return Ok(false);
    }
    return Err(anyhow::format_err!(
        "None of f, F, False, false, t, T, True, true found",
    ));
}

fn parse_t(input: &mut &str) -> WinnowResult<char> {
    alt(('t', 'T')).parse_next(input)
}

fn parse_f(input: &mut &str) -> WinnowResult<char> {
    alt(('f', 'F')).parse_next(input)
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn parse_kafka_base_case() {
        let input_db_id = "w6xb3izpgvz4n0gow6q7";
        let input_code = "fn parse_something<'s>(input: &mut &str) ->";
        let input = format!("{input_db_id} {input_code}");
        let parsed: Result<ParsedKafkaMessage, String> = input.parse();
        assert!(parsed.is_ok_and(|x| x.database_id == "w6xb3izpgvz4n0gow6q7"));
    }

    #[test]
    fn parse_hostname_base_case() {
        let mut hostname_local = "localhost"; // 3 types of hostnames we should accept 
        let mut hostname_ipv4 = "192.168.0.3";
        let mut hostname_ipv4_reduced = "192.168.3";
        assert!(test_hostname(&mut hostname_local).is_ok_and(|v| v == "localhost"));
        assert!(test_hostname(&mut hostname_ipv4).is_ok_and(|v| v == "192.168.0.3"));
        assert!(test_hostname(&mut hostname_ipv4_reduced).is_ok_and(|v| v == "192.168.3"));

        assert!(test_hostname(&mut "256.773.3.1").is_err());
        assert!(test_hostname(&mut "THIS ISN'T A VALID HOSTNAME").is_err());
    }

    #[test]
    fn parse_port_base_case() {
        let mut port_simple = "8080";
        assert!(test_port(&mut port_simple).is_ok_and(|v| v == "8080"));
        let mut wrong_port = "abc";
        assert!(test_port(&mut wrong_port).is_err());
    }

    #[test]
    fn parse_timeout_base_case() {
        let mut timeout_simple = "60000";
        assert!(test_timeout(&mut timeout_simple).is_ok_and(|v| v == "60000"));
    }

    #[test]
    fn parse_topics_base_case() {
        let mut topics_simple = "a_topic,b_topic,3topic";
        assert!(
            parse_topics(&mut topics_simple)
                .is_ok_and(|v| v[..] == ["a_topic", "b_topic", "3topic"])
        );

        let mut wrong_topics = "a_topic,*";
        assert!(parse_topics(&mut wrong_topics).is_err());

        let mut pre_comma_topic = ",a_topic";
        assert!(parse_topics(&mut pre_comma_topic).is_err());

        let mut post_comma_topic = "a_topic,";
        assert!(parse_topics(&mut post_comma_topic).is_err());
    }

    #[test]
    fn parse_bool_base_case() {
        let true_case: Vec<String> = vec![
            "True".to_string(),
            "t".to_string(),
            "T".to_string(),
            "true".to_string(),
        ];
        for case in true_case {
            assert!(parse_bool(&mut &case[..]).is_ok_and(|v| v));
        }
        let false_case: Vec<String> = vec![
            "False".to_string(),
            "f".to_string(),
            "F".to_string(),
            "false".to_string(),
        ];
        for case in false_case {
            assert!(parse_bool(&mut &case[..]).is_ok_and(|v| !v));
        }
    }
}
