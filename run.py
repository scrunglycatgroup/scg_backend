import asyncio
from kafka import KafkaProducer
from kafka.admin import KafkaAdminClient, NewTopic
from fastapi import FastAPI
import logging
import sys 
from pydantic import BaseModel
from surrealdb import AsyncSurreal
from surrealdb import RecordID

from llm_dispatcher import Dispatcher

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

start_logger("DEBUG".upper())

app = FastAPI()


class GenerateBody(BaseModel):
    input: str
    tokens: int

@app.on_event("startup")
async def startup_event():
    global db
    global producer
    # Create the topics for kafka 
    admin = KafkaAdminClient(bootstrap_servers='172.17.0.3:9092')
    if 'message-topic' not in (admin.list_topics()): 
        topics = [NewTopic('message-topic',num_partitions=1, replication_factor=1)]
        admin.create_topics(topics)

    # create the producer we will send data with 
    producer = KafkaProducer(bootstrap_servers='172.17.0.3:9092', acks=1, retries=3)
    if not producer.bootstrap_connected:
        logging.error("not connected to KAFKA server")
    
    # connect to the database
    db = AsyncSurreal("ws://localhost:8008")
    await db.use("test","test")
    await db.signin({"username":'root',"password":'root'})
    print("Application Starting")

@app.on_event("shutdown")
async def shutdown_event():
    await db.close()
    print("DB connection closed")

@app.post("/generate")
async def basic_generate(message: GenerateBody):
    request = await db.create("request", {"input": message.input.encode("utf-8")})
    database_id = request['id'].id
    kafka_message = (database_id + " " + message.input).encode("utf-8")
    producer.send(topic="message-topic",value=kafka_message)
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
    

