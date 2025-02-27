from fastapi import FastAPI
from pathlib import Path
import logging
import time
from transformers import AutoTokenizer, AutoModelForCausalLM
import torch
import sys 
from pydantic import BaseModel

# Ok I'm sick of trying to write this nice, I'm just going to write it and move stuff around later :3

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


class Model:
    """A generative AI model that can be used to analyse log files."""

    # The huggingface identifier of the model.
    # Temp should be 0.5 <= x <= 0.7 according to their own documentation.
    MODEL_NAME = "deepseek-ai/DeepSeek-R1-Distill-Qwen-1.5B"

    # The maximum number of tokens we will prompt the model with. The model's token limit applies to
    # the input and output tokens together, so we need to leave some room between the prompt
    # length and the model token limit for the model to append its output.
    @property
    def target_tokens(self) -> int:
        return self._target_tokens

    @target_tokens.setter
    def target_tokens(self, value):
        self._target_tokens = value

    # Text to prepend to each prompt sent to the model.
    PREAMBLE = Path("./README.md").read_text() + "\n\n"

    def __init__(self):
        """Prepares the model such that it's ready to be run."""

        # Load the pretrained model from huggingface.
        logging.info("Preparing model...")
        self._tokenizer = AutoTokenizer.from_pretrained(self.MODEL_NAME)
        self._model = AutoModelForCausalLM.from_pretrained(
            self.MODEL_NAME,
            device_map="cpu",
            torch_dtype=torch.bfloat16,
        )
        self._target_tokens = 4000
        logging.debug(f"Target Token Length: {self.target_tokens}")

    def _encode(self, input: str) -> torch.Tensor:
        return self._tokenizer.encode(
            input,
            add_special_tokens=False,
            return_tensors="pt",
        )

    def _encoded_len(self, input: str) -> int:
        return len(self._encode(input)[0])

    def _split_input(self, input: str) -> list[str]:
        """We split the input into chunks of at most target_tokens tokens. Chunks always begin
        and end at a line break.

        Arguments:
        input -- The string

        Returns an a list of inputs
        """

        logging.debug(
            f"Splitting input text into chunks of max {self.target_tokens} tokens"
        )
        remaining_lines = input.splitlines(keepends=True)

        # Repeatedly create new chunks by appending lines until target_tokens is reached.
        chunks = []
        chunk = ""
        chunk_tokens = 0
        while len(remaining_lines) > 0:
            line_tokens = self._encoded_len(remaining_lines[0])
            if line_tokens > self.target_tokens:
                raise RuntimeError("input line length exceeds the target token limit")

            if chunk_tokens + line_tokens < self.target_tokens:
                chunk += remaining_lines.pop(0)
                chunk_tokens += line_tokens
            else:
                logging.debug(f"Created chunk of {chunk_tokens} tokens")

                chunks.append(chunk)
                chunk = ""
                chunk_tokens = 0

        logging.debug(f"Created chunk of {chunk_tokens} tokens")
        chunks.append(chunk)

        logging.debug(f"Created a total of {len(chunks)} chunks")
        return chunks

    def _prepare_chunk(self, chunk: str, preamble: str) -> torch.Tensor:
        chat = [
            {
                "role": "user",
                "content": preamble + chunk,
            }
        ]

        text = self._tokenizer.apply_chat_template(
            chat,
            tokenize=False,
            add_generation_prompt=True,
        )

        return self._encode(text).to(self._model.device)

    def _prepare_inputs(self, instr: str, use_preamble: bool = True):
        """Prepares the input to the model.

        Arguments:
        instr -- The instruction text that is the main payload of the input to the model.
        use_preamble -- If the function should use the given preamble in `preamble.md` or whatever Model.PREAMBLE reads. Defaults to true

        Returns an input array ready to be used in generation."""

        # Input to this model follows a chat template where the user and the model take turns.
        # We convert the instruction text into a chat in this format containing a single turn
        # by the user.
        logging.debug("Preparing inputs...")
        chunks = self._split_input(instr)
        preamble = Model.PREAMBLE if use_preamble else ""

        return list(map(lambda chunk: self._prepare_chunk(chunk, preamble), chunks))

    def generate(self, instr: str, use_preamble: bool = True) -> list[str]:
        """Generates a response to some instruction.

        Arguments:
        input_text -- The instruction text to provide to the model.
        use_preamble -- If the function should use the given preamble in `preamble.md` or whatever Model.PREAMBLE reads. Defaults to TRUE

        Returns a list of strings that is the text generated by the model in response."""

        logging.info("Running model...")
        logging.debug(f"Running for input containing {self._encoded_len(instr)} tokens")
        start_time = time.time()

        # Run the model.
        inputs = self._prepare_inputs(instr, use_preamble)
        total_output = []
        for (index, input) in enumerate(inputs):
            logging.debug(f"Running model on chunk {index}...")
            chunk_start_time = time.time()

            padding = self._tokenizer.encode(
                "\n\n", add_special_tokens=False, return_tensors="pt"
            )
            outputs = self._model.generate(
                input,
                max_new_tokens=1000,
                temperature=0.7,
                num_beams=2,
                length_penalty=100,
                eos_token_id=151646,
                pad_token_id=padding[0],
            )

            # Decode the output and extract the newly generated part of the chat,
            # stripping any whitespace as well as the <end_of_turn> marker at the end of the message.
            new_output = outputs[0][len(input[0]) :]
            output_text = self._tokenizer.decode(new_output)
            total_output.append(output_text[: -len("<end_of_turn>")].strip())

            chunk_end_time = time.time()
            logging.debug(
                f"Chunk {index} done in {chunk_end_time - chunk_start_time:.2f}s"
            )

        end_time = time.time()
        logging.debug(f"Done in {end_time - start_time:.2f}s")

        return total_output

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
    

