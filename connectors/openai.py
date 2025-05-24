from .__init__ import BaseConnector
from common import *

from openai import OpenAI, NotGiven
from typing import Optional, Mapping, Union
import httpx
from httpx import Timeout
from fastapi import HTTPException
#import tiktoken

NOT_GIVEN = NotGiven()

OPENAI_API_PRICING_PER_MILLION = {
    "gpt-4.1": [2,8],
    "gpt-4.1-2025-04-14": [2,8],
    "gpt-4.1-mini": [0.4,1.6],
    "gpt-4.1-mini-2025-04-14": [0.4,1.6],
    "gpt-4.1-nano": [0.1,0.4],
    "gpt-4.1-nano-2025-04-14": [0.1,0.4],
    "gpt-4.5-preview": [75,150],
    "gpt-4.5-preview-2025-02-27": [75,150],
    "gpt-4o": [2.5,10],
    "gpt-4o-2024-08-06": [2.5,10],
    "gpt-4o-audio-preview": [2.5,10],
    "gpt-4o-audio-preview-2024-12-17": [2.5,10],
    "gpt-4o-realtime-preview": [5,20],
    "gpt-4o-realtime-preview-2024-12-17": [5,20],
    "gpt-4o-mini": [0.15,0.6],
    "gpt-4o-mini-2024-07-18": [0.15,0.6],
    "gpt-4o-mini-audio-preview": [0.15,0.6],
    "gpt-4o-mini-audio-preview-2024-12-17": [0.15,0.6],
    "gpt-4o-mini-realtime-preview": [0.6,2.4],
    "gpt-4o-mini-realtime-preview-2024-12-17": [0.6,2.4],
    "o1": [15,60],
    "o1-2024-12-17": [15,60],
    "o1-pro": [150,600],
    "o1-pro-2025-03-19": [15,600],
    "o3": [10,40],
    "o3-2025-04-16": [10,40],
    "o4-mini": [1.1,4.4],
    "o4-mini-2025-04-16": [1.1,4.4],
    "o3-mini": [1.1,4.4],
    "o3-mini-2025-01-31": [1.1,4.4],
    "o1-mini": [1.1,4.4],
    "o1-mini-2024-09-12": [1.1,4.4],
    "codex-mini-latest": [1.5,6],
    "gpt-4o-mini-search-preview": [0.15,0.6],
    "gpt-4o-mini-search-preview-2025-03-11": [0.15,0.6],
    "gpt-4o-search-preview": [2.5,10],
    "gpt-4o-search-preview-2025-03-11": [2.5,10],
    "computer-use-preview": [3,13],
    "computer-use-preview-2025-03-11": [3,12],
    "ft:o4-mini-2025-04-16": [4,16],
    "ft:o4-mini-2025-04-16 with data sharing": [2,8],
    "ft:gpt-4.1-2025-04-14": [3,12],
    "ft:gpt-4.1-mini-2025-04-14": [0.8,3.2],
    "ft:gpt-4.1-nano-2025-04-14": [0.2,0.8],
    "ft:gpt-4o-2024-08-06": [3.75,15],
    "ft:gpt-4o-mini-2024-07-18": [0.3,1.2],
    "ft:gpt-3.5-turbo": [3,6],
    "ft:davinci-002": [12,12],
    "ft:babbage-002": [1.6,1.6],
}

#def count_tokens(text:str, model_name:Optional[str] = None, encoding_name:Optional[str] = None):
#    if model_name is None and encoding_name is None:
#        return -1
#    
#    if model_name is not None:
#        encoding = tiktoken.encoding_for_model(model_name)
#    else:
#        encoding = tiktoken.get_encoding(encoding_name)
#    
#    return len(encoding.encode(text))

def token_to_price(num_tokens:int, model_name:str, is_input:bool):
    if model_name.startswith("ft:"):
        model = ":".join(model_name.split(":")[:2])
    else:
        model = model_name
    if model in OPENAI_API_PRICING_PER_MILLION.keys():
        return num_tokens * (OPENAI_API_PRICING_PER_MILLION[model][not is_input] / 1000000)
    else:
        return -1

class OpenAIConnector(BaseConnector):
    __model_name : str
    __client : OpenAI

    def __init__(
            self,
            connector:Connector,
            **kwargs):

        self.__model_name = connector.get("model_name", "undefined")
        
        if self.__model_name == "undefined":
            raise HTTPException(status_code=400, detail="model_name must be defined")

        self.__client = OpenAI(
            api_key=connector["arguments"][0].get("api_key", None),
            organization=connector["arguments"][0].get("organization", None),
            project=connector["arguments"][0].get("project", None),
            base_url=connector["arguments"][0].get("base_url", None),
            websocket_base_url=connector["arguments"][0].get("websocket_base_url", None),
            timeout=connector["arguments"][0].get("timeout", NOT_GIVEN),
            max_retries=connector["arguments"][0].get("max_retries", 2),
            default_headers=connector["arguments"][0].get("default_headers", None),
            default_query=connector["arguments"][0].get("default_query", None),
            http_client=connector["arguments"][0].get("http_client", None),
            _strict_response_validation=connector["arguments"][0].get("_strict_response_validation", False)
        )

    def completion(
            self,
            model_request:ModelRequest,
            **kwargs) -> ModelResponse:
        
        match model_request.purpose:
            
            case "doc_string":
                model = self.__model_name if self.__model_name != "auto" else "ft:gpt-4o-mini-2024-07-18:scg:docstrings:BEnEMYz9"
                system_message = "Generate a docstring from the code block provided by the user, informed by the language and language version where appropriate. Only respond with code. Do not surround your response with triple backticks."
                user_message = f"Language: {model_request.language}\nLanguage Version: {model_request.language_version}\nCode:\n```\n{model_request.content}\n```"

            case "log_analysis":
                model = self.__model_name if self.__model_name != "auto" else "gpt-4o-mini-2024-07-18"
                system_message = "Explain to the user what the logs (or partial logs) provided by the user indicate, including any errors or warnings. Note whether the logs indicate the normal operation of the system, otherwise only explain the errors or warnings."
                user_message = f"Logs:\n```\n{model_request.content}\n```"

            case "compile_err":
                model = self.__model_name if self.__model_name != "auto" else "gpt-4o-mini-2024-07-18"
                system_message = "Explain to the user what the compilation error provided by the user means, informed by the language and language version where appropriate."
                user_message = f"Language: {model_request.language}\nLanguage Version: {model_request.language_version}\nCompilation error:\n```\n{model_request.content}\n```"

            case "unit_test":
                model = self.__model_name if self.__model_name != "auto" else "gpt-4o-mini-2024-07-18"
                system_message = "Generate unit tests for the code block provided by the user, informed by the language and language version where appropriate. Only respond with code. Do not surround your response with triple backticks."
                user_message = f"Language: {model_request.language}\nLanguage Version: {model_request.language_version}\n{(f"Framework: {model_request.extra_data.get("framework", None)}") if model_request.extra_data.get("framework", None) is not None else ""}Code:\n```\n{model_request.content}\n```"

            case "runtime_err":
                model = self.__model_name if self.__model_name != "auto" else "gpt-4o-mini-2024-07-18"
                system_message = "Explain to the user what the runtime error provided by the user means, informed by the language and language version where appropriate."
                user_message = f"Language: {model_request.language}\nLanguage Version: {model_request.language_version}\nCompilation error:\n```\n{model_request.content}\n```"

            case "sql_sanit":
                model = self.__model_name if self.__model_name != "auto" else "gpt-4o-mini-2024-07-18"
                system_message = "Check the SQL query provided by the user for any potential SQL injection vulnerabilities. If the language in which the query is being handled is known, also give specific suggestions for mitigations."
                user_message = f"Language: {model_request.language}\nLanguage Version: {model_request.language_version}\nCode:\n```\n{model_request.content}\n```"

            case _:
                raise HTTPException(status_code=400, detail=f"Purpose '{model_request.purpose}' is not recognised or not supported by the OpenAI connector.")
        
        response = self.__client.chat.completions.create(
            model=model,
            messages=[
                {
                    "role": "system",
                    "content": [
                        {
                            "type": "text",
                            "text": system_message
                        }
                    ]
                },
                {
                    "role": "user",
                    "content": [
                        {
                            "type": "text",
                            "text": user_message
                        }
                    ]
                }
            ],
            response_format={"type": "text"}, temperature=1, max_completion_tokens=2048, top_p=1, frequency_penalty=0, presence_penalty=0, store=False
        )

        return ModelResponse(
            content=response.choices[0].message.content,
            extra_data={
                "tokens": str(response.usage.total_tokens),
                "cost": f"${(token_to_price(response.usage.prompt_tokens, model, True) + token_to_price(response.usage.completion_tokens, model, False)):.10f}".rstrip('0').rstrip('.')
                }
        )

    def close(self):
        return
    
    def __del__(self):
        try:
            self.close()
        except:
            return