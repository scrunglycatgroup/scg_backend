import asyncio
from fastapi import FastAPI

from kafka import KafkaProducer
from kafka.admin import KafkaAdminClient, NewTopic
from kafka.errors import KafkaTimeoutError

from surrealdb import AsyncSurreal
from surrealdb import RecordID

from common import ModelRequest, ModelResponse, Connector
from connectors.openai import OpenAIConnector

import os
from dotenv import load_dotenv
import logging
import sys 
from typing import Optional
from pydantic import BaseModel

def help():
    print("""
    -h : brings up this menu

    We read these environment variables either from the environment or a file `.env`

    PY_LOGGER : string for the log level (Debug, info, warning, error, critical)
    PY_KAFKA_HOST: str ip or localhost address of kafka server
    PY_KAFKA_PORT: str port of the kafka server
    PY_KAFKA_TOPICS: str comma separated list of kafka topics to write to 

    PY_SURREAL_HOST: str ip or localhost address of the db
    PY_SURREAL_PORT: str port of the db
    PY_SURREAL_USER: str Username for the db
    PY_SURREAL_PASS: str password for the db
    PY_SURREAL_NAMESPACE: str namespace
    PY_SURREAL_DB: str db name
    PY_SURREAL_TABLE: str table writing to

    CONNECTOR_API_KEY: str api key for whatever connector you are using, CHECK DOCS
    CONNECTOR_MODEL_NAME: str name of the model we are using in the connector, CHECK DOCS
    """)

def load_env(env_var:str) -> str:
    var = os.getenv(env_var)
    if var is None:
        raise RuntimeError(f"Missing environment variable {env_var}")
    return var

def start_logger(log_arg: str):
    global logging
    match log_arg:
        case "DEBUG":
            log_level = logging.DEBUG
        case "INFO":
            log_level = logging.INFO
        case "WARNING":
            log_level = logging.WARNING
        case "ERROR":
            log_level = logging.ERROR
        case "CRITICAL":
            log_level = logging.CRITICAL
        case _:
            raise RuntimeError(f"Log level '{log_arg}' is not recognised")

    logging.basicConfig(stream=sys.stdout, level=log_level)
    return

load_dotenv()

if "-h" in sys.argv:
    help()
    sys.exit(0)

log_level = load_env("PY_LOGGER")
start_logger(log_level.upper())

kafka_topics = load_env("PY_KAFKA_TOPICS").split(",")
kafka_topics.append("web_heartbeat")
db_table = load_env("PY_SURREAL_TABLE")


app = FastAPI()

class HealthCheck(BaseModel):
        status: str = "Ok"

@app.on_event("startup")
async def startup_event():
    global db
    global producer
    # Create the topics for kafka 
    global kafka_host
    global kafka_port
    # Open ai connector
    global openAiConnector 

    connector = Connector(connector="openai", model_name=load_env("CONNECTOR_MODEL_NAME"), model_tag="undefined",arguments=[{"api_key":load_env("CONNECTOR_API_KEY")}])
    openAiConnector = OpenAIConnector(connector)
    kafka_host = load_env("PY_KAFKA_HOST")
    kafka_port = load_env("PY_KAFKA_PORT")
    admin = KafkaAdminClient(bootstrap_servers=f'{kafka_host}:{kafka_port}',api_version=(4,0))
    topics = []
    for topic in kafka_topics:
        if topic not in (admin.list_topics()): 
            topics.append(NewTopic(topic,num_partitions=1, replication_factor=1))
    if topics != []:
        admin.create_topics(topics)
    
    admin.close()

    # create the producer we will send data with 
    producer = KafkaProducer(bootstrap_servers=f'{kafka_host}:{kafka_port}', acks=1, retries=3,api_version=(4,0))
    if not producer.bootstrap_connected:
        logging.error("not connected to KAFKA server")
    
    # connect to the database
    db_host = load_env("PY_SURREAL_HOST")
    db_port = load_env("PY_SURREAL_PORT")
    db_namespace = load_env("PY_SURREAL_NAMESPACE")
    db_db = load_env("PY_SURREAL_DB")
    db_user = load_env("PY_SURREAL_USER")
    db_pass = load_env("PY_SURREAL_PASS")
    db = AsyncSurreal(f"ws://{db_host}:{db_port}")
    await db.use(db_namespace,db_db)
    await db.signin({"username":db_user,"password":db_pass})
    logging.debug("Application Starting")


@app.on_event("shutdown")
async def shutdown_event():
    await db.close()
    logging.debug("DB connection closed")

@app.post("/generate")
async def basic_generate(message: ModelRequest):
    logging.debug(f"Message: {message}")
    if message.connector['connector'] == 'undefined':
        logging.info("Got generate request")
        request = await db.create(db_table, {"input": message.content})
        database_id = request['id'].id
        # TODO: use more of the request details ESPECIALLY the purpose
        logging.info("Generate request to be sent to local llm")
        kafka_message = (database_id + " " + message.content).encode("utf-8")
        producer.send(topic=kafka_topics[0],value=kafka_message)
        logging.debug("Sent kafka message")
        # now we loop over waiting a little bit and checking if 'output' is ever written to, look into subscribe_live
        check_rate = 4
        timeout = 500 / check_rate
        counter = 0
        logging.debug("Starting polling loop")
        while True:
            await asyncio.sleep(check_rate)
            logging.debug("Poll")
            check = await db.select(RecordID(db_table,database_id))
            logging.debug(f"Poll response: {check}")
            if check != None and 'output' in check:
                logging.debug("Returning positive!")
                return ModelResponse(content=check['output'],status_code=200,flags=[])
            if counter >= timeout:
                logging.debug("Timeout!")
                return ModelResponse(content='timeout',status_code=500,flags=['timeout'])
            counter += 1
    else:
        return openAiConnector.completion(model_request=message)

@app.get("/health")
async def healthcheck():
    return HealthCheck(status="Ok")


if __name__ == "__main__":
    print("You should be running me through `fastapi dev run.py`")
    

