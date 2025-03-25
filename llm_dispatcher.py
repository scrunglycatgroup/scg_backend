from llm_model import Model
import logging

global NUM_MODELS
NUM_MODELS = 3
global NUM_THREADS
NUM_THREADS = 3

class Dispatcher():

    def __init__(self):
        logging.info("LLM dispatcher started")

        self.threads = []
        self._start_models()
        

    def _start_models(self):
        """Creates self.model_list, these are all the models that should start up and load the llms"""
        self.model_list = [Model() for _ in range(NUM_MODELS)]

    
    async def run_model(self,input):
        # Select some model and then run the .generate on it
        # TODO: Write select code 
        out = self.model_list[0].generate(input,True)
        return "\n".join(out)

        

    
