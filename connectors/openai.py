from .__init__ import BaseConnector
from common import *

from openai import OpenAI, NotGiven
from typing import Optional, Mapping, Union
import httpx
from httpx import Timeout

NOT_GIVEN = NotGiven()

class OpenAIConnector(BaseConnector):
    __model_name : str
    __client : OpenAI

    def __init__(
            self,
            connector:Connector,
            **kwargs):

        self.__model_name = connector.get("model_name", "undefined")
        
        if self.__model_name == "undefined":
            raise ValueError("model_name must be defined")

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
                user_message = f"Language: {model_request.language}\nLanguage Version: {model_request.language_version}\n{(f"Framework: {model_request.extra_data.get("framework", None)}") if model_request["extra_data"].get("framework", None) is not None else ""}Code:\n```\n{model_request["content"]}\n```"

            case "runtime_err":
                model = self.__model_name if self.__model_name != "auto" else "gpt-4o-mini-2024-07-18"
                system_message = "Explain to the user what the runtime error provided by the user means, informed by the language and language version where appropriate."
                user_message = f"Language: {model_request.language}\nLanguage Version: {model_request.language_version}\nCompilation error:\n```\n{model_request.content}\n```"

            case "sql_sanit":
                model = self.__model_name if self.__model_name != "auto" else "gpt-4o-mini-2024-07-18"
                system_message = "Check the SQL query provided by the user for any potential SQL injection vulnerabilities. If the language in which the query si being handled is known, also give specific suggestions for mitigations."
                user_message = f"Language: {model_request.language}\nLanguage Version: {model_request.language_version}\nCode:\n```\n{model_request.content}\n```"

            case _:
                raise ValueError(f"Purpose '{model_request.purpose}' is not recognised or not supported by the OpenAI connector.")
        
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

        return ModelResponse(content=response.choices[0].message.content)

    def close(self):
        return
    
    def __del__(self):
        try:
            self.close()
        except:
            return