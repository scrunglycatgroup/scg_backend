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
        
        response = self.__client.chat.completions.create(
            model=self.__model_name,
            messages=[
                {
                    "role": "system",
                    "content": [
                        {
                            "type": "text",
                            "text": "Generate a docstring from the code block provided by the user, informed by the language and language version where appropriate. Only respond with code. Do not surround your response with triple backticks."
                        }
                    ]
                },
                {
                    "role": "user",
                    "content": [
                        {
                            "type": "text",
                            "text": msg
                        }
                    ]
                }
            ],
            response_format={
                "type": "text"
            },
            temperature=1,
            max_completion_tokens=2048,
            top_p=1,
            frequency_penalty=0,
            presence_penalty=0,
            store=False
        )

        return response.choices[0].message.content

    def close(self):
        return
    
    def __del__(self):
        try:
            self.close()
        except:
            return