#[doc = r"This module handles all the parsing of external data to whatever the rust_dispatcher needs"]
use std::env::VarError;
use std::env::{self, var};

use dotenv::dotenv;

use winnow::Result as WinnowResult;
use winnow::ascii::digit1;
use winnow::combinator::{alt, opt, separated, separated_pair};
use winnow::prelude::*;
use winnow::token::{literal, take_until, take_while};

// logging
use log::{info, trace};
/// Kafka sends us a single string at the moment, the regex for this being:
/// ```
/// /(?<database_id>\w+) (?<whatever_the_llms_total_input_is>.|\n|\r)+/g
/// ```
/// basically the first word is some database id that we will write to, the rest is whatever we are sent
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

// get all the characters before the first space
fn parse_database_id<'s>(input: &mut &'s str) -> WinnowResult<&'s str> {
    take_until(0.., " ").parse_next(input)
}

// run [`parse_database_id`] and whatever is left is the actual stuff we are going to send to the llm
pub fn parse_kafka_message(input: &mut &str) -> WinnowResult<ParsedKafkaMessage> {
    let id = parse_database_id.parse_next(input)?;
    let out = ParsedKafkaMessage {
        database_id: id.to_string(),
        message: (**input).to_string(),
    };
    Ok(out)
}

/// Taking variables from the environment (that's either .env or environment variables)

#[derive(Debug)]
pub enum SetupVariableError {
    VarError(VarError),
    ParseError(String),
    AskedHelp, // the user asked for help, we shoud now gracefully exit
}

pub use SetupVariableError as SVErr;

// Box<str> is a slightly faster String, as long as you don't mutate the str, we
// will just read these values so it's easy to use this
// Rc handles copying better and Arc does that across threads safely. we are just
// referencing these afaik so Box will be just fine though if we do want to clone the data this should switch to a Arc
type EnvVar = Box<str>;
#[allow(clippy::struct_field_names)]
#[derive(Debug)]
pub struct EnvVars {
    pub kafka: KafkaVars,     // stuff we need for kafka
    pub surreal: SurrealVars, // stuff we need for the db
    pub lazy_load: bool, // if we want to load the llm threads at the start or wait until our first message
}
#[allow(clippy::struct_field_names)]
#[derive(Debug)]
pub struct SurrealVars {
    pub surreal_host: EnvVar,      // SURREAL_HOST
    pub surreal_namespace: EnvVar, // SURREAL_NAMESPACE
    pub surreal_database: EnvVar,  // SURREAL_DATABASE
    pub surreal_user: EnvVar,      // SURREAL_USER
    pub surreal_pass: EnvVar,      // SURREAL_PASS
    pub surreal_table: EnvVar,     // SURREAL_TABLE
}

#[allow(clippy::struct_field_names)]
#[derive(Debug)]
pub struct KafkaVars {
    pub kafka_host: EnvVar, // KAFKA_HOST
    // this is a collection of strings (or Box<str> to be exact)
    pub kafka_topics: Box<[EnvVar]>, // KAFKA_TOPICS
    pub kafka_group_id: EnvVar,      // KAFKA_GROUP_ID
    pub kafka_timeout: EnvVar,       // KAFKA_TIMEOUT
}

// Read environment variables and send them back
pub fn read_environment_vars() -> Result<EnvVars, SVErr> {
    trace!("loaded info from .env into the environment");
    dotenv().ok();
    let args: Vec<_> = env::args().collect();
    // check if they have started with -h in which case give them some info and safely quit out
    if args.contains(&"-h".to_string()) || args.contains(&"--help".to_string()) {
        info!(
            "-h was included in the call, we should give the user some info and then close / stop"
        );
        println!(
            r"
/=======================================================================================\
|          Environment Variables (can be in a .env)                                     | 
|=======================================================================================| 
| SURREAL_HOST        : The host ip for the surreal db (can take localhost) (comma sep) |  
| SURREAL_NAMESPACE   : The namespace this shard will attach to on the surreal server   | 
| SURREAL_DATABASE    : The database within the namespace this shard will attach to     | 
| SURREAL_USER        : The username for the server                                     | 
| SURREAL_PASS        : The password for the server                                     | 
| SURREAL_TABLE       : The table it will read and write to / from                      | 
|                                                                                       | 
| KAFKA_HOST          : The host ip for the kafka instance (can take localhost)         | 
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
    // is there a variable called LAZY_LOAD and if so is it something we can read as either true of false
    let lazy_load_var = var("LAZY_LOAD").map_err(SVErr::VarError)?;
    let lazy_load = parse_bool(&mut &lazy_load_var[..]).map_err(|_| {
        SVErr::ParseError(
            "LAZY_LOAD couldn't be read as a bool, consider using true or false".to_string(),
        )
    })?;
    Ok(EnvVars {
        kafka: parse_kafka_env_vars()?,
        surreal: parse_surreal_env_vars()?,
        lazy_load,
    })
}

/// This checks for all the kafka environment variables, if it can't find any it *NEEDS* then it will error out
/// If any supplied are malformed then it will error out
fn parse_kafka_env_vars() -> Result<KafkaVars, SVErr> {
    // for each environment variable we check if it exists and then if it looks valid
    let hostname_var = var("KAFKA_HOST").map_err(SVErr::VarError)?;
    let hostname = parse_hostname_set(&mut &hostname_var[..]).map_err(|_| {
        SVErr::ParseError(
            "KAFKA_HOST couldn't be parsed, use either a valid ipv4 address or localhost along with valid ports, it should be a comma separated list"
                .to_string(),
        )
    })?;
    let topics_var = var("KAFKA_TOPICS").map_err(SVErr::VarError)?;
    let topics: Box<[EnvVar]> = (parse_topics(&mut &topics_var[..]) // this is a little messy but just collects up the data we need
        .map_err(|_| SVErr::ParseError("KAFKA_TOPICS could not be parsed, must be a word (constructed out of some alphanumerics and _'s) or comma seperated list of words, make sure you don't have any trailing commas".to_string()))?)
    .iter()
    .map(|v| ((*v).to_string()).into_boxed_str())
    .collect();
    let group_id_var = var("KAFKA_GROUP_ID").map_err(SVErr::VarError)?;
    let timeout_var = var("KAFKA_TIMEOUT").unwrap_or("60000".to_string()); // if we don't get timeout then set to 60000
    let timeout = test_timeout(&mut &timeout_var[..]).map_err(|_| {
        SVErr::ParseError("KAFKA_TIMEOUT could not be parsed, must be a number".to_string())
    })?;

    Ok(KafkaVars {
        kafka_host: hostname.into(),
        kafka_topics: topics,
        kafka_group_id: group_id_var.into(),
        kafka_timeout: timeout.into(),
    })
}

fn parse_surreal_env_vars() -> Result<SurrealVars, SVErr> {
    // same as above
    let hostname_var = var("SURREAL_HOST").map_err(SVErr::VarError)?;
    let hostname = parse_hostname(&mut &hostname_var[..]).map_err(|_| {
        SVErr::ParseError(
            "SURREAL_HOST could not be parsed, please use a valid ipv4 address or localhost, or port, it should be a comma separated list"
                .to_string(),
        )
    })?;
    let namespace_var = var("SURREAL_NAMESPACE").map_err(SVErr::VarError)?;
    let database_var = var("SURREAL_DATABASE").map_err(SVErr::VarError)?;
    let user_var = var("SURREAL_USER").map_err(SVErr::VarError)?;
    let pass_var = var("SURREAL_PASS").map_err(SVErr::VarError)?;
    let table_var = var("SURREAL_TABLE").map_err(SVErr::VarError)?;

    Ok(SurrealVars {
        surreal_host: hostname.into(),
        surreal_namespace: namespace_var.into(),
        surreal_database: database_var.into(),
        surreal_user: user_var.into(),
        surreal_pass: pass_var.into(),
        surreal_table: table_var.into(),
    })
}

fn parse_hostname_set<'i>(hostname: &mut &'i str) -> WinnowResult<&'i str> {
    Parser::take(separated::<_, _, (), _, _, _, _>(
        ..,
        parse_hostname_and_port,
        ',',
    ))
    .parse_next(hostname)
}

fn parse_hostname_and_port<'i>(input: &mut &'i str) -> WinnowResult<&'i str> {
    Parser::take(separated_pair(parse_hostname, ':', parse_port)).parse_next(input)
}

fn parse_hostname<'i>(hostname: &mut &'i str) -> WinnowResult<&'i str> {
    Parser::take(alt((parse_ip, "localhost"))).parse_next(hostname)
}

// check if ipv4 is valid
fn parse_ip<'i>(input: &mut &'i str) -> WinnowResult<&'i str> {
    alt((parse_ip_quad, parse_ip_trip)).take().parse_next(input)
}

fn parse_ip_quad<'i>(input: &mut &'i str) -> WinnowResult<&'i str> {
    Parser::take(separated::<_, _, (), _, _, _, _>(4, parse_ip_octet, '.')).parse_next(input)
}

fn parse_ip_trip<'i>(input: &mut &'i str) -> WinnowResult<&'i str> {
    Parser::take((
        parse_ip_octet,
        '.',
        parse_ip_octet,
        '.',
        parse_ip_double_octet,
    ))
    .parse_next(input)
}

// can't have ipv4 bytes above 255!
fn parse_ip_octet<'i>(input: &mut &'i str) -> WinnowResult<&'i str> {
    digit1
        .parse_to::<u32>()
        .verify(|i: &u32| *i <= 255)
        .take()
        .parse_next(input)?;
    Ok(input)
}

fn parse_ip_double_octet<'i>(input: &mut &'i str) -> WinnowResult<&'i str> {
    digit1
        .parse_to::<u32>()
        .verify(|i: &u32| *i <= 65_535)
        .take()
        .parse_next(input)
}

// is the port in the valid range
fn parse_port<'i>(input: &mut &'i str) -> WinnowResult<&'i str> {
    digit1
        .parse_to::<u32>()
        .verify(|i: &u32| *i < (<u32>::from(u16::MAX)))
        .take()
        .parse_next(input)
}

fn test_timeout<'i>(input: &mut &'i str) -> WinnowResult<&'i str> {
    digit1.parse_to::<u32>().take().parse_next(input)
}

// comma separated list of words
fn parse_topics<'i>(input: &mut &'i str) -> Result<Vec<&'i str>, anyhow::Error> {
    separated(1.., topic_parser, ',')
        .parse(input)
        .map_err(|e| anyhow::format_err!("{e}"))
}

// a word is just some number of alphanumerics with _'s allowed
fn topic_parser<'i>(input: &mut &'i str) -> WinnowResult<&'i str> {
    take_while(1.., ('_', '0'..='9', 'a'..='z', 'A'..='Z')).parse_next(input)
}

// bool just checks if the first letter is t or f and returns true or false respectively
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
    Err(anyhow::format_err!(
        "None of f, F, False, false, t, T, True, true found",
    ))
}

fn parse_t(input: &mut &str) -> WinnowResult<char> {
    alt(('t', 'T')).parse_next(input)
}

fn parse_f(input: &mut &str) -> WinnowResult<char> {
    alt(('f', 'F')).parse_next(input)
}

// test driven development is good, I should do it more
#[cfg(test)]
mod test {
    use std::str::FromStr;

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
    fn parsing_hostnames() {
        let mut local = "localhost";
        let local_parsed = parse_hostname(&mut local);
        assert!(local_parsed.is_ok());
        assert_eq!(local_parsed.unwrap(), "localhost");
        let mut simple_ip = "192.168.0.4";
        let simple_ip_parsed = parse_hostname(&mut simple_ip);
        assert!(simple_ip_parsed.is_ok());
        assert_eq!(simple_ip_parsed.unwrap(), "192.168.0.4");
        let mut local = "localhost:8080";
        let local_parsed = parse_hostname_and_port(&mut local);
        assert!(local_parsed.is_ok());
        assert_eq!(local_parsed.unwrap(), "localhost:8080");
        let mut simple_ip = "192.168.0.4:3030";
        let simple_ip_parsed = parse_hostname_and_port(&mut simple_ip);
        assert!(simple_ip_parsed.is_ok());
        assert_eq!(simple_ip_parsed.unwrap(), "192.168.0.4:3030");

        let mut super_simple_test = "192.168.0.4:3030,localhost:8080,232.101.455:1234";
        let super_simple_parsed = parse_hostname_set(&mut super_simple_test);
        assert!(super_simple_parsed.is_ok());
        assert_eq!(
            super_simple_parsed.unwrap(),
            "192.168.0.4:3030,localhost:8080,232.101.455:1234"
        );
    }

    #[test]
    fn parse_ips() {
        let mut quad = "192.168.0.4";
        let quad_parsed = parse_ip(&mut quad);
        assert!(quad_parsed.is_ok());
        assert_eq!(quad_parsed.unwrap(), "192.168.0.4");
        let mut trip = "192.168.267";
        let trip_parsed = parse_ip(&mut trip);
        assert!(trip_parsed.is_ok());
        assert_eq!(trip_parsed.unwrap(), "192.168.267");
    }

    #[test]
    fn parse_port_base_case() {
        let mut port_simple = "8080";
        assert!(parse_port(&mut port_simple).is_ok_and(|v| v == "8080"));
        let mut wrong_port = "abc";
        assert!(parse_port(&mut wrong_port).is_err());
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
