import asyncio
from kafka import KafkaProducer
from kafka.admin import KafkaAdminClient, NewTopic
from fastapi import FastAPI
import logging
import sys 
from pydantic import BaseModel
from surrealdb import AsyncSurreal
from surrealdb import RecordID
from dotenv import load_dotenv
import os

from llm_dispatcher import Dispatcher

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
    """)

def load_env(env_var:str) -> str:
    var = os.getenv(env_var)
    if var is None:
        raise RuntimeError(f"Missing environment variable {env_var}")
    return var

def start_logger(log_arg: str):

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

    logging.basicConfig(stream=sys.stderr, level=log_level)
    return

load_dotenv()

if "-h" in sys.argv:
    help()
    sys.exit(0)

log_level = load_env("PY_LOGGER")
start_logger(log_level.upper())

kafka_topics = load_env("PY_KAFKA_TOPICS").split(",")
db_table = load_env("PY_SURREAL_TABLE")

app = FastAPI()

class GenerateBody(BaseModel):
    input: str
    tokens: int

@app.on_event("startup")
async def startup_event():
    global db
    global producer
    # Create the topics for kafka 
    kafka_host = load_env("PY_KAFKA_HOST")
    kafka_port = load_env("PY_KAFKA_PORT")
    admin = KafkaAdminClient(bootstrap_servers=f'{kafka_host}:{kafka_port}')
    topics = []
    for topic in kafka_topics:
        if topic not in (admin.list_topics()): 
            topics.append(NewTopic(topic,num_partitions=1, replication_factor=1))
    if topics != []:
        admin.create_topics(topics)

    # create the producer we will send data with 
    producer = KafkaProducer(bootstrap_servers=f'{kafka_host}:{kafka_port}', acks=1, retries=3)
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
    print("Application Starting")

@app.on_event("shutdown")
async def shutdown_event():
    await db.close()
    print("DB connection closed")

@app.post("/generate")
async def basic_generate(message: GenerateBody):
    request = await db.create(db_table, {"input": message.input.encode("utf-8")})
    database_id = request['id'].id
    kafka_message = (database_id + " " + message.input).encode("utf-8")
    producer.send(topic=kafka_topics[0],value=kafka_message)
    # now we loop over waiting a little bit and checking if 'output' is ever written to, look into subscribe_live
    check_rate = 4
    timeout = 120 / check_rate
    counter = 0
    while True:
        await asyncio.sleep(check_rate)
        check = await db.select(RecordID('request',database_id))
        if 'output' in check:
            return {"message":check['output'].decode('utf-8')}
        if counter == timeout:
            return {"message":"timeout"}
        counter += 1

if __name__ == "__main__":
    print("You should be running me through `fastapi dev run.py`")
    

