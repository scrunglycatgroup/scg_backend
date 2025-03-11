from fastapi import FastAPI
import logging
import sys 
from pydantic import BaseModel


from llm_model import Model 

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
model = Model() 

app = FastAPI()

class GenerateBody(BaseModel):
    input: str
    tokens: int

@app.post("/generate")
async def basic_generate(message: GenerateBody):
    out = model.generate(message.input, False)
    return {"message": out}

    
if __name__ == "__main__":
    print("You should be running me through `fastapi dev run.py`")
    

